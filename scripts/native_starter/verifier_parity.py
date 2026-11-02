"""Authored production-verifier corpus, shared with the independent Rust compiler.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
import copy
import json
from pathlib import Path
import tempfile
import unittest

from classic_export import core, columns, quest_columns, spell_numeric
from classic_export.capture import validate_bundle
from . import compiler, creature_plan, fireball_plan, world_rules
from .dbc import LAYOUTS, binary32_bits, load_extraction_manifest, parse_table

ROOT = Path(__file__).resolve().parent


def corpus():
    return core.decode_json((ROOT / "production_verifier_cases.json").read_bytes(), 4 * 1024 * 1024)


def mutations():
    return core.decode_json((ROOT / "production_verifier_mutations.json").read_bytes(), 4 * 1024 * 1024)["cases"]


def pointer(value, path):
    if not path:
        return value
    for part in path.removeprefix("/").split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        value = value[int(part)] if type(value) is list else value[part]
    return value


def mutate(fixture, case):
    value = fixture[case["target"]]
    for operation in case["operations"]:
        path = operation["path"]
        parent_path, _, key = path.rpartition("/")
        parent = pointer(value, parent_path)
        key = int(key) if type(parent) is list else key
        if operation["op"] == "remove":
            if type(parent) is list:
                parent.pop(key)
            else:
                del parent[key]
        elif operation["op"] == "append":
            pointer(value, path).append(copy.deepcopy(operation["value"]))
        else:
            parent[key] = copy.deepcopy(operation["value"])
    for binding in case["bindings"]:
        parent_path, _, key = binding["path"].rpartition("/")
        pointer(value, parent_path)[key] = core.digest(pointer(value, binding["of"]))


def verify(fixture):
    source = fixture["source"]
    validate_bundle(source)
    request = compiler.Request.parse(fixture["request"])
    selected = core.Selection.parse(source["selection"]).starter
    core.check((selected.race, selected.class_id) == (request.race, request.class_id), "compiler.selection_mismatch")
    rules = world_rules.validate(fixture["rules"])
    core.check(rules["progression"]["max_player_level"] == selected.level_cap, "compiler.rules_selection_mismatch")
    tables = {name: parse_table(name, bytes.fromhex(data)) for name, data in fixture["dbc_hex"].items()}
    with tempfile.TemporaryDirectory(prefix="benilla-verifier-parity-") as directory:
        root = Path(directory)
        (root / "extraction.json").write_bytes(fixture["extraction_text"].encode("utf-8"))
        binding = load_extraction_manifest(root, tables)
    core.check(binding["metadata"]["format"] == "benilla-native-dbc-extraction-v4", "extraction.complete_valid_dbc_set_required")
    compiler.verify_preparation(fixture["preparation"], source, tables,
                                extraction_manifest=binding, native_world_rules=rules)
    # Production request is independent of the report's request.
    core.check(fixture["preparation"]["request"] == request.json(), "compiler.preparation_mismatch")


class SourceVerifierParityTests(unittest.TestCase):
    def test_reviewed_rust_schema_metadata_matches_python(self):
        contract_path = ROOT.parents[1] / "crates/benilla-world-runtime/src/native/production_compiler/schema_contract.json"
        contract = json.loads(contract_path.read_text())
        expected = {
            "columns": columns.COLUMNS, "keys": {k: list(v) for k, v in core.KEYS.items()},
            "text": {k: sorted(v) for k, v in core.TEXT_COLUMNS.items()},
            "strict": {k: [list(row) for row in v] for k, v in (columns.STARTER_SCHEMA | columns.LEARNING_SCHEMA
                | columns.SPAWN_SCHEMA | columns.REGISTRY_SCHEMA | columns.INITIAL_HOOK_SCHEMA
                | columns.LOOT_SCHEMA | columns.SPAWN_ENTRY_SCHEMA | columns.SPAWN_CONTEXT_SCHEMA).items()},
            "event_text": {k: [list(row) for row in v] for k, v in columns.EVENT_TEXT_SCHEMA.items()},
            "event_aliases": {k: {n: list(v) for n, v in m.items()} for k, m in columns.EVENT_TEXT_ALIASES.items()},
            "event_primary_keys": columns.EVENT_TEXT_PRIMARY_KEYS,
            "starter": sorted(columns.STARTER_SCHEMA), "learning": sorted(columns.LEARNING_SCHEMA),
            "spawn": sorted(columns.SPAWN_SCHEMA), "registry": sorted(columns.REGISTRY_SCHEMA),
            "hooks": sorted(columns.INITIAL_HOOK_SCHEMA), "loot": sorted(columns.LOOT_SCHEMA),
            "spawn_entry": sorted(columns.SPAWN_ENTRY_SCHEMA), "presentation": sorted(quest_columns.KEYS),
        }
        expected["spawn_context"] = sorted(columns.SPAWN_CONTEXT_SCHEMA)
        self.assertEqual(contract, expected)
        from classic_export import quest_mechanics_columns
        mechanics = {
            "schema": {k: [list(row) for row in v] for k, v in quest_mechanics_columns.SCHEMA.items()},
            "keys": {k: list(v) for k, v in quest_mechanics_columns.KEYS.items()},
            "columns": quest_mechanics_columns.COLUMNS,
            "text": {k: sorted(v) for k, v in quest_mechanics_columns.TEXT.items()}}
        self.assertEqual(json.loads((contract_path.parent / "extensions/quest_mechanics_contract.json").read_text()),
                         mechanics)

    def test_shared_source_mutations_have_exact_diagnostic_ids(self):
        fixtures = {f["name"]: f for f in corpus()["fixtures"]}
        for case in mutations():
            if case["target"] != "source":
                continue
            with self.subTest(case=case["name"]):
                fixture = copy.deepcopy(fixtures[case["fixture"]])
                mutate(fixture, case)
                with self.assertRaises(core.ExportError) as error:
                    verify(fixture)
                self.assertEqual(str(error.exception), case["diagnostic"])

    def test_exact_decimal_comparisons_preserve_sub_float_boundaries(self):
        from decimal import Decimal
        cases = core.decode_json((ROOT / "decimal_parity.json").read_bytes(), 4096)
        for case in cases["cases"]:
            with self.subTest(case=case):
                left, right = Decimal(case["left"]), Decimal(case["right"])
                self.assertEqual((left > right) - (left < right), case["order"])

    def test_strict_json_duplicate_nonfinite_invalid_unicode_and_depth(self):
        cases = core.decode_json((ROOT / "production_verifier_malformed.json").read_bytes(), 1024 * 1024)
        for case in cases["json"]:
            with self.subTest(raw=case["raw"][:32]), self.assertRaises(core.ExportError) as error:
                core.decode_json(case["raw"].encode(), 1024)
            self.assertEqual(str(error.exception), case["diagnostic"])


class PreparationVerifierParityTests(unittest.TestCase):
    def test_reviewed_rust_compiler_metadata_matches_python(self):
        root = ROOT.parents[1] / "crates/benilla-world-runtime/src/native/production_compiler"
        expected = {"numeric_columns": spell_numeric.COLUMNS, "numeric_words": spell_numeric.WORD_FIELDS,
            "numeric_floats": spell_numeric.FLOAT_FIELDS, "numeric_signed": spell_numeric.SIGNED_FIELDS,
            "numeric_text": spell_numeric.TEXT_FIELDS, "spell_fields": compiler.SPELL_FIELDS,
            "float_spell_fields": sorted(compiler.FLOAT_SPELL_FIELDS),
            "consumed": {k: sorted(v) for k, v in compiler.CONSUMED.items()},
            "blocked": compiler.BLOCKED_TABLE_REASON,
            "dbc_layouts": {k: {"fields": v.fields, "size": v.size, "strings": list(v.strings)} for k, v in LAYOUTS.items()}}
        self.assertEqual(json.loads((root / "compiler_contract.json").read_text()), expected)
        expected = {"creature_fields": sorted(creature_plan.FIELDS), "creature_floats": sorted(creature_plan.FLOATS),
            "fireball_exact": {str(k): v for k, v in fireball_plan.EXACT_WORDS.items()},
            "fireball_reasons": sorted(fireball_plan.REASONS), "quest_text": list(quest_columns.QUEST_TEXT),
            "numeric_word_order": list(spell_numeric.WORD_FIELDS), "spell_field_order": list(compiler.SPELL_FIELDS)}
        self.assertEqual(json.loads((root / "preparation/plan_contract.json").read_text()), expected)

    def test_complete_shared_preparations_reproduce(self):
        for fixture in corpus()["fixtures"]:
            with self.subTest(fixture=fixture["name"]):
                verify(fixture)

    def test_extension_fixtures_retain_supported_shapes_and_unknown_host_proof(self):
        fixtures = {f["name"]: f for f in corpus()["fixtures"]}
        mechanics = fixtures["mechanics-spawn"]
        from classic_export import quest_mechanics
        self.assertEqual(quest_mechanics.supported(mechanics["source"])["status"], "resolved")
        plan = mechanics["preparation"]["creatures"][0]["spawn_context_plan"]
        self.assertEqual(plan["status"], "supported_shape")
        self.assertEqual(plan["spawns"][0]["radius_binary32"], 0x40a00000)
        self.assertEqual(plan["spawns"][0]["respawn_seconds"], [7, 9])
        self.assertFalse(plan["production_admission"])
        training = fixtures["training-spawn-membership"]["preparation"]
        closure = training["player"]["skills"]["dependency_closure"]
        self.assertEqual(closure["ability_dispositions"], [
            {"row_id": 31, "disposition": "profession_repair"},
            {"row_id": 32, "disposition": "unmatched_masks"},
            {"row_id": 33, "disposition": "unmatched_masks"},
            {"row_id": 34, "disposition": "minimum_value_excluded"}])
        self.assertIn(90001, closure["spell_ids"])
        self.assertIn(9, closure["skill_ids"])
        self.assertEqual(training["creatures"][0]["spawn_context_plan"]["refusals"], [
            {"guid": 1, "reason": "spawn.unsupported_creature_linking"},
            {"guid": 1, "reason": "spawn.unsupported_creature_linking_template"}])

    def test_shared_non_source_mutations_have_exact_diagnostic_ids(self):
        fixtures = {f["name"]: f for f in corpus()["fixtures"]}
        for case in mutations():
            if case["target"] == "source":
                continue
            with self.subTest(case=case["name"]):
                fixture = copy.deepcopy(fixtures[case["fixture"]])
                mutate(fixture, case)
                with self.assertRaises(core.ExportError) as error:
                    verify(fixture)
                self.assertEqual(str(error.exception), case["diagnostic"])

    def test_shared_malformed_wdbc_and_exact_binary32_cases(self):
        cases = core.decode_json((ROOT / "production_verifier_malformed.json").read_bytes(), 1024 * 1024)
        for case in cases["dbc"]:
            with self.subTest(case=case["name"]), self.assertRaises(core.ExportError) as error:
                parse_table(case["table"], bytes.fromhex(case["hex"]))
            self.assertEqual(str(error.exception), case["diagnostic"])
        for case in cases["binary32"]:
            with self.subTest(decimal=case["decimal"]):
                self.assertEqual(binary32_bits(case["decimal"]), case["bits"])
