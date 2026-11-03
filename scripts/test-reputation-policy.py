#!/usr/bin/env python3
"""Authored reputation policy coverage through capture and preparation interfaces.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import copy
import importlib.util
from pathlib import Path
import unittest
from classic_export import reputation as rep
from classic_export.capture import validate_bundle
from classic_export.core import ExportError, digest
from native_starter.compiler import Request, compile_starter, verify_preparation
from native_starter.world_rules import stock_preset

spec = importlib.util.spec_from_file_location("starter_tests", Path(__file__).with_name("test-native-starter.py"))
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)


def spill(faction, target, rank=7):
    row = {name: {"decimal": "0"} if rep.floating(name) else 0 for name in rep.FIELDS[rep.SPILL]}
    row.update(faction=faction, faction1=target, rate_1={"decimal": "0.5"}, rank_1=rank)
    return row


class Source:
    def __init__(self, rows):
        self.rows = rows
        self.queries = []

    def fetch(self, query):
        if isinstance(query, rep.SchemaQuery):
            if query.keys:
                return [{"table_name": table, "name": columns[0], "ordinal": 1}
                        for table, columns in rep.FIELDS.items()]
            return [{"table_name": table, "name": name, "kind": kind + (" unsigned" if unsigned else ""),
                     "nullable": "NO"} for table, columns in rep.FIELDS.items()
                    for name in columns for kind, unsigned in [rep.domain(table, name)]]
        sql, params = query.sql()
        assert sql.startswith("SELECT ") and params[-1] == 4097
        self.queries.append(query)
        return copy.deepcopy(self.rows.get(query.table, []))


class ReputationPolicyTests(unittest.TestCase):
    def setUp(self):
        self.bundle = helpers.source_bundle()
        self.files = helpers.dbcs_for(self.bundle)
        self.files["Faction"] = helpers.dbc_bytes("Faction", [
            {0: 900, 1: 0, 10: 1000, 18: 901}, {0: 901, 1: 1}])
        self.files["FactionTemplate"] = helpers.dbc_bytes("FactionTemplate", [{0: 970001, 1: 900}])
        self.dbcs = helpers.parsed(self.files)
        self.source = Source({rep.SPILL: [spill(900, 901)]})
        self.bundle = rep.capture(self.source, self.bundle, self.dbcs["Faction"])

    def compile(self, rules=True):
        output = compile_starter(self.bundle, self.dbcs, Request(1, 1, 0),
                                 native_world_rules=stock_preset() if rules else None)
        return output, output["player"]["initial_reputation"]["reward_rates_and_spillover"]

    def rebind(self):
        value = self.bundle["reputation_policy_capture"]
        value["binding"]["lookups_sha256"] = digest(value["lookups"])

    def test_complete_lookup_closure_preserves_kobold_absence_and_spillover(self):
        validate_bundle(self.bundle)
        output, plan = self.compile()
        self.assertEqual(plan["status"], "complete_inputs")
        self.assertEqual(len(plan["policies"]), 2)
        self.assertEqual(plan["kill_sources"], [{"entry": 900001, "row": None,
                                                "admission": "ProvenAbsentOnKillRow"}])
        self.assertEqual(plan["policies"][0]["spillover"]["Present"][0],
                         {"faction": 901, "rate": 0x3f000000, "maximum_rank": "Exalted"})
        self.assertFalse(plan["consumer_certified"])
        self.assertEqual(plan["aura_binding"], "requires_actual_initialized_aura_list")
        self.assertEqual(output["player"]["initial_reputation"]["initial_standings"][0]["base_standing"], 1000)
        verify_preparation(output, self.bundle, self.dbcs, native_world_rules=stock_preset())

    def test_absent_rate_is_not_zero_and_decimal_conversion_is_exact(self):
        lookups = self.bundle["reputation_policy_capture"]["lookups"]
        self.assertEqual(self.compile()[1]["policies"][0]["reward_rates"], "Absent")
        row = {"faction": 900, "quest_rate": {"decimal": "0"},
               "creature_rate": {"decimal": "0.1"}, "spell_rate": {"decimal": "1"}}
        lookups[rep.RATE][0]["row"] = row
        self.rebind()
        validate_bundle(self.bundle)
        self.assertEqual(self.compile()[1]["policies"][0]["reward_rates"],
                         {"Present": {"quest": 0, "creature": 0x3dcccccd, "spell": 0x3f800000}})
        row["quest_rate"] = {"decimal": "-1"}
        self.rebind()
        self.assertEqual(self.compile()[1]["policies"][0]["reward_rates"], "Absent")
        self.assertEqual(self.compile()[1]["loader_dispositions"][0]["status"], "skipped_negative_rate")

    def test_missing_spill_lookup_and_rate_outside_closure_refuse_even_rehashed(self):
        original = copy.deepcopy(self.bundle)
        del self.bundle["reputation_policy_capture"]["lookups"][rep.SPILL][0]
        self.rebind()
        with self.assertRaisesRegex(ExportError, "reputation.lookup_coverage"):
            validate_bundle(self.bundle)
        self.bundle = original
        self.bundle["reputation_policy_capture"]["lookups"][rep.RATE][0]["row"] = {
            "faction": 902, "quest_rate": {"decimal": "1"},
            "creature_rate": {"decimal": "1"}, "spell_rate": {"decimal": "1"}}
        self.rebind()
        with self.assertRaisesRegex(ExportError, "reputation.row_selection"):
            validate_bundle(self.bundle)

    def test_loader_inner_continue_does_not_drop_unlisted_or_invalid_rank_row(self):
        self.files["Faction"] = helpers.dbc_bytes("Faction", [
            {0: 900, 1: 0, 18: 901}, {0: 901, 1: -1}])
        self.dbcs = helpers.parsed(self.files)
        self.bundle = rep.capture(self.source, self.bundle, self.dbcs["Faction"])
        _, plan = self.compile()
        self.assertEqual(plan["status"], "complete_inputs")
        self.assertEqual(plan["loader_dispositions"][2]["status"], "accepted")
        self.assertEqual(plan["loader_dispositions"][2]["warnings"], [{"slot": 0, "reason": "unlisted_faction"}])
        self.bundle["reputation_policy_capture"]["lookups"][rep.SPILL][0]["row"]["rank_1"] = 8
        self.rebind()
        _, plan = self.compile()
        self.assertEqual(plan["loader_dispositions"][2]["status"], "accepted")
        self.assertEqual(plan["status"], "blocked")
        self.assertIn("reputation.spillover_rank_unrepresentable", plan["refusals"])
        self.assertIsNone(plan["policies"])

    def test_missing_target_drops_whole_spill_row_and_wrong_dbc_cannot_certify(self):
        self.bundle["reputation_policy_capture"]["lookups"][rep.SPILL][0]["row"]["faction1"] = 12345
        self.rebind()
        _, plan = self.compile()
        self.assertEqual(plan["policies"][0]["spillover"], "Absent")
        self.assertEqual(plan["loader_dispositions"][2]["status"], "skipped_missing_target")
        self.bundle["reputation_policy_capture"]["faction_sha256"] = "0" * 64
        self.assertEqual(self.compile()[1]["refusals"], ["reputation.complete_faction_binding_required"])

    def test_explicit_configuration_and_actual_aura_binding_are_separate(self):
        self.assertEqual(self.compile(False)[1]["refusals"], ["reputation.explicit_configuration_required"])
        _, plan = self.compile()
        self.assertEqual(plan["configuration"], stock_preset()["reputation"])
        self.assertFalse(plan["consumer_certified"])

    def test_kill_loader_bool_and_unsigned_conversions_and_invalid_faction_skip(self):
        row = {name: 0 for name in rep.FIELDS[rep.KILL]}
        row.update(creature_id=900001, RewOnKillRepFaction1=1, IsTeamAward1=-1, MaxStanding1=-1)
        self.bundle["reputation_policy_capture"]["lookups"][rep.KILL][0]["row"] = row
        self.rebind()
        self.assertEqual(self.compile()[1]["kill_sources"][0]["admission"], "ProvenAbsentOnKillRow")
        row["RewOnKillRepFaction1"] = 0
        self.rebind()
        actual = self.compile()[1]["kill_sources"][0]
        self.assertEqual(actual["admission"], "PresentOnKillRow")
        self.assertFalse(actual["row"]["IsTeamAward1"])
        self.assertEqual(actual["row"]["MaxStanding1"], 0xffffffff)

    def test_no_team_short_circuits_inner_warnings_and_foreign_schema_refuses(self):
        record = self.bundle["reputation_policy_capture"]["lookups"][rep.SPILL][1]
        record["row"] = spill(901, 12345, 8)
        self.rebind()
        disposition = self.compile()[1]["loader_dispositions"][3]
        self.assertEqual(disposition, {"table": rep.SPILL, "id": 901, "status": "skipped_no_team", "warnings": []})
        self.bundle["reputation_policy_capture"]["policy_schema"][rep.RATE]["columns"]["quest_rate"][0] = "double"
        with self.assertRaisesRegex(ExportError, "reputation.schema"):
            validate_bundle(self.bundle)


if __name__ == "__main__":
    unittest.main()
