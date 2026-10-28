#!/usr/bin/env python3
"""No database/network or game content: original fake-source exporter tests.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import copy
from decimal import Decimal
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import types
import warnings
import getpass

from classic_export.capture import Query, capture, validate_bundle
from classic_export.columns import (COLUMNS, STARTER_SCHEMA, LEARNING_SCHEMA, SPAWN_SCHEMA,
                                    REGISTRY_SCHEMA, EVENT_TEXT_SCHEMA, EVENT_TEXT_PRIMARY_KEYS,
                                    LOOT_SCHEMA, LOOT_PRIMARY_KEYS)
from classic_export import starter, quest_relationships
from classic_export.core import (ExportError, MAX_ROWS, Selection, TEXT_COLUMNS,
                                 canonical, decode_json, digest, normalize_cell,
                                 normalize_rows, schema_profile)
from classic_export.driver import LocalSqlSource

FLOAT_FIELDS = {"position_x", "position_y", "position_z", "orientation", "spawndist",
                "bounding_radius", "combat_reach", "SpeedWalk", "SpeedRun"}

def schema():
    tables = {}
    for table, names in COLUMNS.items():
        columns = []
        for ordinal, name in enumerate(names, 1):
            kind = "text" if name in TEXT_COLUMNS.get(table, set()) else "bigint"
            if name in FLOAT_FIELDS:
                kind = "decimal(40,20)"
            columns.append({"name": name, "type": kind, "nullable": False, "ordinal": ordinal})
        if table in (LEARNING_SCHEMA | SPAWN_SCHEMA | REGISTRY_SCHEMA | EVENT_TEXT_SCHEMA | LOOT_SCHEMA):
            columns = [{"name": name, "type": kind, "nullable": nullable, "ordinal": ordinal}
                       for ordinal, (name, kind, nullable) in enumerate((LEARNING_SCHEMA | SPAWN_SCHEMA | REGISTRY_SCHEMA | EVENT_TEXT_SCHEMA | LOOT_SCHEMA)[table], 1)]
        tables[table] = {"engine": "InnoDB", "table_type": "BASE TABLE", "columns": columns}
        if table in REGISTRY_SCHEMA:
            tables[table]["primary_key"] = ["Id"]
        if table in EVENT_TEXT_PRIMARY_KEYS:
            tables[table]["primary_key"] = EVENT_TEXT_PRIMARY_KEYS[table].copy()
        if table in LOOT_PRIMARY_KEYS:
            tables[table]["primary_key"] = LOOT_PRIMARY_KEYS[table].copy()
    return {"format_version": 1, "tables": tables}

def row(table, **changes):
    result = {name: ("" if name in TEXT_COLUMNS.get(table, set()) else 0) for name in COLUMNS[table]}
    for name in FLOAT_FIELDS & set(result):
        result[name] = Decimal("0")
    result.update(changes)
    return result

def refresh_event_text(bundle):
    from classic_export.event_text import evidence
    if "event_text_capture" in bundle:
        bundle["event_text_capture"] = evidence(bundle)


def selection():
    return Selection.parse({"format_version": 1, "map_id": 0,
                            "min_milliunits": [-10000]*3, "max_milliunits": [10000]*3,
                            "quest_ids": [910001], "spell_ids": [930001]})

class FakeSource:
    """All names/values here are original synthetic rows, not native content samples."""
    def __init__(self):
        self.inventory = schema()
        self.queries = []
        self.finished = False
        self.started = False
        self.versions = 0
        self.mutate_version = False
        self.rows = {table: [] for table in COLUMNS}
        self.rows["creature"] = [row("creature", guid=1, id=900001, map=0, spawnMask=1,
                                      spawntimesecsmin=5, spawntimesecsmax=5)]
        self.rows["creature_template"] = [row("creature_template", Entry=900001, Name="Original Practice Construct",
                                              MinLevel=1, MaxLevel=1, UnitClass=1, DisplayId1=940001,
                                              HealthMultiplier=1, MeleeBaseAttackTime=2000)]
        self.rows["creature_model_info"] = [row("creature_model_info", modelid=940001, combat_reach=Decimal("1.5"))]
        self.rows["creature_template_classlevelstats"] = [row("creature_template_classlevelstats", Class=1, Level=1, BaseHealthExp0=30)]
        self.rows["creature_questrelation"] = [row("creature_questrelation", id=900001, quest=910001)]
        self.rows["creature_involvedrelation"] = [row("creature_involvedrelation", id=900001, quest=910001)]
        self.rows["quest_template"] = [row("quest_template", entry=910001, Title="Original Capture Exercise",
                                           MinLevel=1, QuestLevel=1, MaxLevel=255, ReqCreatureOrGOId1=900001,
                                           ReqCreatureOrGOCount1=2, RewItemId1=920001, RewItemCount1=1,
                                           RewMoneyMaxLevel=60)]
        self.rows["item_template"] = [row("item_template", entry=920001, name="Original Export Token", stackable=20)]
        self.rows["spell_template"] = [row("spell_template", Id=930001, Effect1=2, EffectBasePoints1=4, EffectBaseDice1=1)]
    def inspect(self):
        return copy.deepcopy(self.inventory)
    def start_read(self, engines, quiescent):
        if "MyISAM" in engines and not quiescent:
            raise ExportError("snapshot.quiescent_required_for_myisam")
        self.started = True
        return "operator_quiescent_snapshot" if quiescent else "transactional_snapshot"
    def version_notes(self):
        self.versions += 1
        return [{"version": "original test schema " + (str(self.versions) if self.mutate_version else "1"), "creature_ai_version": "original"}]
    def finish_read(self):
        self.finished = True
    def fetch(self, query):
        sql, args = query.sql()
        self.queries.append((sql, args))
        values = self.rows[query.table]
        if query.mode == "region":
            s = query.selection
            values = [r for r in values if r["map"] == s.map_id and all(
                Decimal(low)/1000 <= r[name] <= Decimal(high)/1000 for name, low, high in
                zip(["position_x", "position_y", "position_z"], s.minimum, s.maximum))]
        elif query.mode == "starter":
            s = query.selection.starter
            if query.table == "playercreateinfo_skills":
                values = [r for r in values if
                          (r["raceMask"] == 0 or r["raceMask"] & 2**(s.race-1)) and
                          (r["classMask"] == 0 or r["classMask"] & 2**(s.class_id-1))]
            elif query.table == "player_xp_for_level":
                values = [r for r in values if 1 <= r["lvl"] < s.level_cap]
            else:
                values = [r for r in values if r["class"] == s.class_id
                          and ("race" not in r or r["race"] == s.race)
                          and ("level" not in r or 1 <= r["level"] <= s.level_cap)]
        elif query.mode == "learning_presence":
            return copy.deepcopy(values[:1])
        elif query.mode == quest_relationships.MODE:
            values = [r for r in values if quest_relationships.matches(r, list(query.ids))]
        elif query.mode == "quest_ids":
            values = [r for r in values if r["quest"] in query.ids]
        elif query.mode == "event_ai_selectors":
            values = [r for r in values if r["creature_id"] in query.ids]
        else:
            from classic_export.core import KEYS
            values = [r for r in values if r[KEYS[query.table][0]] in query.ids]
        return copy.deepcopy(values[:query.limit+1])

class ExportTests(unittest.TestCase):
    def test_original_capture_is_valid_and_explicitly_not_runnable(self):
        source = FakeSource()
        result = capture(source, selection())
        validate_bundle(result)
        self.assertTrue(source.finished)
        self.assertFalse(result["report"]["runnable"])
        self.assertFalse(result["report"]["capture_complete"])
        self.assertEqual(result["tables"]["creature"][0]["position_x"], {"decimal": "0"})
        reasons = {r["reason"] for r in result["report"]["unsupported"]}
        self.assertIn("quest.dynamic_xp", reasons)
        self.assertIn("spell.native_effect_target_scaling_rules_required", reasons)
        self.assertIn("creature.native_stats_combat_faction_required", reasons)
        self.assertTrue(all(q.startswith("SELECT ") and ";" not in q for q, _ in source.queries))

    def test_zero_id_spawn_is_preserved_with_unresolved_substitution_blocker(self):
        source = FakeSource()
        source.rows["creature"].append(dict(source.rows["creature"][0], guid=2, id=0))
        bundle = capture(source, selection())
        validate_bundle(bundle)
        self.assertEqual([(r["guid"], r["id"]) for r in bundle["tables"]["creature"]],
                         [(1, 900001), (2, 0)])
        finding = {"table": "creature", "key": "2", "status": "blocked",
                   "reason": "spawn.entry_substitution_not_captured"}
        self.assertEqual([r for r in bundle["report"]["unsupported"]
                          if r["reason"] == finding["reason"]], [finding])
        self.assertFalse(bundle["report"]["capture_complete"])
        self.assertFalse(bundle["report"]["runnable"])
        self.assertTrue(all("creature_spawn_entry" not in sql for sql, _ in source.queries))
        self.assert_required_finding_cannot_be_forged(bundle, finding)

    def test_next_quest_in_chain_alone_is_preserved_and_blocked(self):
        source = FakeSource()
        source.rows["quest_template"][0]["NextQuestInChain"] = 910002
        bundle = capture(source, selection())
        validate_bundle(bundle)
        self.assertEqual(bundle["tables"]["quest_template"][0]["NextQuestInChain"], 910002)
        self.assertEqual([r["entry"] for r in bundle["tables"]["quest_template"]], [910001])
        finding = {"table": "quest_template", "key": "910001", "status": "blocked",
                   "reason": "quest.chain"}
        self.assertIn(finding, bundle["report"]["unsupported"])
        self.assert_required_finding_cannot_be_forged(bundle, finding)

    def test_ordinary_capture_does_not_claim_global_quest_chain_closure(self):
        bundle = capture(FakeSource(), selection())
        validate_bundle(bundle)
        reasons = {r["reason"] for r in bundle["report"]["unsupported"]}
        self.assertNotIn("spawn.entry_substitution_not_captured", reasons)
        self.assertNotIn("quest.chain", reasons)
        self.assertIn("quest.native_eligibility_credit_reward_required", reasons)
        # Zero own-link fields say nothing about uncaptured inbound quest links.
        self.assertFalse(bundle["report"]["capture_complete"])
        self.assertFalse(bundle["report"]["runnable"])

    def assert_required_finding_cannot_be_forged(self, bundle, finding):
        for alteration in ("omit", "reason", "key"):
            with self.subTest(alteration=alteration, finding=finding["reason"]):
                forged = copy.deepcopy(bundle)
                issues = forged["report"]["unsupported"]
                index = issues.index(finding)
                if alteration == "omit":
                    issues.pop(index)
                else:
                    issues[index][alteration] = "forged"
                with self.assertRaisesRegex(ExportError, "bundle.missing_or_inconsistent_blocker"):
                    validate_bundle(forged)

    def test_prepared_query_uses_numeric_parameters_only(self):
        query = Query("creature_template", "ids", (900001,))
        sql, params = query.sql()
        self.assertNotIn("900001", sql)
        self.assertEqual(params, (900001, MAX_ROWS+1))
        self.assertIn("`Entry` AS `Entry`", sql)
        for query in [Query("creature;DROP TABLE test", "ids", (1,)),
                      Query("creature", "ids", ("1 OR 1=1",)), Query("creature", "freeform", (1,)),
                      Query("creature", "ids", (0,)), Query("creature", "ids", (1,1)),
                      Query("creature", "region", selection=Selection(0, (3,0,0), (1,0,0)))]:
            with self.assertRaises(ValueError):
                query.sql()

    def test_selection_rejects_unknown_fields_duplicate_ids_and_bad_bounds(self):
        for name, value in [("where", "1=1"), ("quest_ids", [1,1]), ("map_id", True),
                            ("spell_ids", [-1]), ("min_milliunits", [20]*3)]:
            original = selection().json()
            original[name] = value
            if name == "min_milliunits":
                original["max_milliunits"] = [10]*3
            with self.assertRaises(ExportError):
                Selection.parse(original)

    def test_schema_missing_columns_views_and_wrong_families_reject(self):
        mutations = [lambda s: s["tables"]["creature"]["columns"].pop(),
                     lambda s: s["tables"]["creature"].update(table_type="VIEW"),
                     lambda s: s["tables"]["creature"]["columns"][0].update(type="text")]
        for mutate in mutations:
            source = FakeSource()
            mutate(source.inventory)
            with self.assertRaises(ExportError):
                capture(source, selection())
            self.assertFalse(source.started)

    def test_unknown_extra_column_is_reported_and_never_queried(self):
        source = FakeSource()
        source.inventory["tables"]["creature"]["columns"].append({"name":"ExtraUnknown", "type":"int", "nullable":False,"ordinal":500})
        result = capture(source, selection())
        self.assertTrue(any(r["reason"] == "schema.unreviewed_columns" for r in result["report"]["unsupported"]))
        self.assertTrue(all("ExtraUnknown" not in q for q, _ in source.queries))

    def test_missing_template_reference_rejects_and_rolls_back(self):
        source = FakeSource()
        source.rows["creature_template"] = []
        with self.assertRaisesRegex(ExportError, "reference.missing"):
            capture(source, selection())
        self.assertTrue(source.finished)

    def test_duplicate_ids_reject(self):
        source = FakeSource()
        source.rows["creature"].append(copy.deepcopy(source.rows["creature"][0]))
        with self.assertRaisesRegex(ExportError, "duplicate_key"):
            capture(source, selection())

    def test_max_plus_one_rejects_without_truncation(self):
        source = FakeSource()
        first = source.rows["creature"][0]
        source.rows["creature"] = [dict(first, guid=i+1) for i in range(MAX_ROWS+1)]
        with self.assertRaisesRegex(ExportError, "rows.limit"):
            capture(source, selection())

    def test_myisam_requires_explicit_operator_snapshot(self):
        source = FakeSource()
        source.inventory["tables"]["creature"]["engine"] = "MyISAM"
        with self.assertRaisesRegex(ExportError, "quiescent_required"):
            capture(source, selection())
        result = capture(source, selection(), quiescent=True)
        self.assertEqual(result["consistency"], "operator_quiescent_snapshot")

    def test_version_drift_rejects(self):
        source = FakeSource()
        source.mutate_version = True
        with self.assertRaisesRegex(ExportError, "version_changed"):
            capture(source, selection())
        self.assertTrue(source.finished)

    def test_trigger_cycle_is_bounded_and_preserved_as_blocked(self):
        source = FakeSource()
        source.rows["spell_template"][0]["EffectTriggerSpell1"] = 930002
        source.rows["spell_template"].append(row("spell_template", Id=930002, Effect1=2, EffectTriggerSpell1=930001))
        result = capture(source, selection())
        self.assertEqual(len(result["tables"]["spell_template"]), 2)
        self.assertTrue(any(r["reason"] == "spell.aura_or_trigger_dependency" for r in result["report"]["unsupported"]))

    def test_signed_gameobject_quest_is_reported_not_changed_to_creature(self):
        source = FakeSource()
        source.rows["quest_template"][0]["ReqCreatureOrGOId1"] = -990001
        result = capture(source, selection())
        self.assertEqual(result["tables"]["quest_template"][0]["ReqCreatureOrGOId1"], -990001)
        self.assertTrue(any(r["reason"] == "quest.nonkill_objective" for r in result["report"]["unsupported"]))

    def test_unknown_script_and_multiple_effects_are_blocked(self):
        source = FakeSource()
        source.rows["creature_template"][0]["ScriptName"] = "original_unimplemented_script"
        source.rows["spell_template"][0]["Effect2"] = 6
        result = capture(source, selection())
        reasons = {r["reason"] for r in result["report"]["unsupported"]}
        self.assertIn("creature.ai_or_spell_dependency", reasons)
        self.assertIn("spell.multiple_or_missing_effects", reasons)

    def test_native_decimal_precision_and_null_policy(self):
        column = {"type":"decimal(40,20)", "nullable":False}
        self.assertEqual(normalize_cell(Decimal("12345678901234567890.12345678901234567890"), column),
                         {"decimal":"12345678901234567890.1234567890123456789"})
        for value in [None, "NaN", "Infinity", "1e-1000000000", "1e1000000000", float("nan")]:
            with self.assertRaises(ExportError):
                normalize_cell(value, column)
        self.assertIsNone(normalize_cell(None, {"type":"text", "nullable":True}))

    def test_hashes_row_order_and_runnable_claim_cannot_be_tampered(self):
        base = capture(FakeSource(), selection())
        for mutate in [lambda b: b["tables"]["creature"][0].update(id=123),
                       lambda b: b["report"].update(runnable=True),
                       lambda b: b["report"].update(unsupported=[]),
                       lambda b: b.update(schema_sha256="0"*64),
                       lambda b: b["tables"].pop("item_template")]:
            changed = copy.deepcopy(base)
            mutate(changed)
            with self.assertRaises(ExportError):
                validate_bundle(changed)

    def test_recomputed_hash_cannot_hide_missing_spawn_reference(self):
        bundle = capture(FakeSource(), selection())
        bundle["tables"]["creature"][0]["id"] = 123
        bundle["table_sha256"]["creature"] = digest(bundle["tables"]["creature"])
        with self.assertRaisesRegex(ExportError, "spawn_reference"):
            validate_bundle(bundle)

    def test_json_rejects_duplicates_nonfinite_and_size(self):
        for value in [b'{"a":1,"a":2}', b'NaN', b'[[[', b'\xff']:
            with self.assertRaises(ExportError):
                decode_json(value, 100)
        with self.assertRaisesRegex(ExportError, "byte_limit"):
            decode_json(b'{}', 1)

    def test_private_atomic_writer_refuses_existing_destination(self):
        path = Path(__file__).with_name("classic-export.py")
        spec = importlib.util.spec_from_file_location("export_cli", path)
        cli = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cli)
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)/"export.json"
            cli.write_new(output, {"original":1})
            original = output.read_bytes()
            with self.assertRaises(FileExistsError):
                cli.write_new(output, {"original":2})
            self.assertEqual(output.read_bytes(), original)
            self.assertEqual(list(Path(temp).iterdir()), [output])
            if __import__("os").name == "posix":
                self.assertEqual(output.stat().st_mode & 0o777, 0o600)

    def test_driver_rejects_remote_and_bad_configuration_before_import(self):
        # Constructor must reject these without importing dependencies or prompting.
        for database, user, port in [("db;DROP TABLE x", "reader", 3306), ("world", "reader --host=evil", 3306), ("world", "reader", 0)]:
            with self.assertRaises(ExportError):
                LocalSqlSource(database, user, port)

    def test_native_level_bounds_precede_classlevel_expansion(self):
        for minimum, maximum in [(1, 10**12), (5, 2), (0, 1)]:
            source = FakeSource()
            source.rows["creature_template"][0].update(MinLevel=minimum, MaxLevel=maximum)
            with self.assertRaisesRegex(ExportError, "creature.(min|max)_level"):
                capture(source, selection())
            self.assertTrue(source.finished)

    def test_sql_bit_decimal_and_string_widths_are_checked(self):
        for value, kind in [(255, "bit(1)"), ("123.456", "decimal(2,1)"),
                            ("-0.1", "decimal(2,1) unsigned"), ("three", "varchar(2)")]:
            with self.assertRaises(ExportError):
                normalize_cell(value, {"type":kind,"nullable":False})
        self.assertEqual(normalize_cell(1, {"type":"bit(1)","nullable":False}), 1)
        self.assertEqual(normalize_cell("1.20", {"type":"decimal(2,1)","nullable":False}), {"decimal":"1.2"})

    def test_missing_stats_require_blocker_and_required_ids_cannot_disappear(self):
        result = capture(FakeSource(), selection())
        modified = copy.deepcopy(result)
        modified["required_ids"] = {}
        with self.assertRaisesRegex(ExportError, "required_reference_tables"):
            validate_bundle(modified)
        modified = copy.deepcopy(result)
        modified["tables"]["creature_template_classlevelstats"] = []
        modified["table_sha256"]["creature_template_classlevelstats"] = digest([])
        with self.assertRaisesRegex(ExportError, "missing_or_inconsistent_blocker"):
            validate_bundle(modified)

    def test_db_version_is_inspected_and_myisam_cannot_claim_transactional(self):
        source = FakeSource()
        source.inventory["tables"]["db_version"]["table_type"] = "VIEW"
        with self.assertRaisesRegex(ExportError, "views_rejected"):
            capture(source, selection())
        result = capture(FakeSource(), selection())
        result["schema"]["tables"]["db_version"]["engine"] = "MyISAM"
        result["schema_sha256"] = digest(result["schema"])
        with self.assertRaisesRegex(ExportError, "myisam_consistency"):
            validate_bundle(result)

    def test_invalid_transaction_start_result_is_cleaned_up(self):
        source = FakeSource()
        source.start_read = lambda *_: "unrecognized"
        with self.assertRaisesRegex(ExportError, "snapshot.mode"):
            capture(source, selection())
        self.assertTrue(source.finished)

    def test_driver_streaming_byte_limit_precedes_full_buffer(self):
        class Cursor:
            def __init__(self): self.fetched = 0
            def __enter__(self): return self
            def __exit__(self, *args): pass
            def execute(self, *args): pass
            def fetchone(self):
                self.fetched += 1
                return {"Name": "x"*65500} if self.fetched <= 300 else None
        cursor = Cursor()
        source = LocalSqlSource.__new__(LocalSqlSource)
        source.connection = types.SimpleNamespace(cursor=lambda: cursor)
        with self.assertRaisesRegex(ExportError, "table_byte_limit"):
            source._select("SELECT Name FROM creature_template LIMIT %s", (301,), 301)
        self.assertLess(cursor.fetched, 300)

    def test_secure_password_prompt_never_falls_back_to_echo(self):
        fake = types.SimpleNamespace()
        def insecure_prompt(*_):
            warnings.warn("no safe terminal", getpass.GetPassWarning)
            self.fail("password fallback should not continue")
        with mock.patch.dict("sys.modules", {"pymysql":fake}), mock.patch("getpass.getpass", insecure_prompt):
            with self.assertRaisesRegex(ExportError, "secure_terminal_required"):
                LocalSqlSource("world", "reader")

    def test_local_connection_options_disable_file_loading_and_custom_config(self):
        observed = {}
        def connect(**kwargs):
            observed.update(kwargs)
            return types.SimpleNamespace()
        fake = types.SimpleNamespace(connect=connect, cursors=types.SimpleNamespace(SSDictCursor=object))
        with mock.patch.dict("sys.modules", {"pymysql":fake}), mock.patch("getpass.getpass", return_value="original-fake-test-value"):
            LocalSqlSource("world", "reader")
        self.assertEqual(observed["host"], "127.0.0.1")
        self.assertFalse(observed["local_infile"])
        self.assertEqual(observed["client_flag"], 0)
        self.assertIsNone(observed["read_default_file"])
        self.assertIsNone(observed["init_command"])
        self.assertEqual(observed["read_timeout"], 10)

    def test_transaction_driver_uses_only_guard_and_rollback_commands(self):
        commands = []
        class Cursor:
            def __enter__(self): return self
            def __exit__(self, *args): pass
            def execute(self, sql): commands.append(sql)
        source = LocalSqlSource.__new__(LocalSqlSource)
        source.connection = types.SimpleNamespace(cursor=Cursor, rollback=lambda: commands.append("ROLLBACK"))
        self.assertEqual(source.start_read({"InnoDB"}, False), "transactional_snapshot")
        source.finish_read()
        self.assertEqual(commands, ["ROLLBACK", "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ",
                                    "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY", "ROLLBACK"])

    def test_version_notes_match_inspected_nullability_and_width(self):
        source = FakeSource()
        source.inventory["tables"]["db_version"]["columns"][0]["type"] = "varchar(2)"
        with self.assertRaisesRegex(ExportError, "sql_string_length"):
            capture(source, selection())
        source = FakeSource()
        source.version_notes = lambda: [{"version":None, "creature_ai_version":"original"}]
        with self.assertRaisesRegex(ExportError, "unexpected_null"):
            capture(source, selection())



def starter_selection(cap=3):
    value = selection().json()
    value.update(format_version=2, starter={"race": 1, "class": 1, "level_cap": cap})
    return Selection.parse(value)


def starter_source(cap=3):
    source = FakeSource()
    source.rows["creature_spawn_data_template"] = [row("creature_spawn_data_template", Entry=0,
        NpcFlags=-1, UnitFlags=-1, EquipmentId=-1, Name="Original synthetic defaults")]
    for table, fields in STARTER_SCHEMA.items():
        source.inventory["tables"][table]["columns"] = [
            {"name": name, "type": kind, "nullable": nullable, "ordinal": ordinal}
            for ordinal, (name, kind, nullable) in enumerate(fields, 1)]
    source.rows["playercreateinfo"] = [row("playercreateinfo", race=1, **{"class": 1})]
    source.rows["playercreateinfo_item"] = [row("playercreateinfo_item", race=1, **{"class": 1}, itemid=920101, amount=2)]
    source.rows["playercreateinfo_spell"] = [row("playercreateinfo_spell", race=1, **{"class": 1}, Spell=930101, Note=None)]
    source.rows["playercreateinfo_action"] = [
        row("playercreateinfo_action", race=1, **{"class": 1}, button=0, action=930101, type=0),
        row("playercreateinfo_action", race=1, **{"class": 1}, button=11, action=920102, type=128)]
    source.rows["playercreateinfo_skills"] = [
        row("playercreateinfo_skills", raceMask=0, classMask=0, skill=60001, step=0, note=None),
        row("playercreateinfo_skills", raceMask=1, classMask=1, skill=60002, step=16, note="Original test note"),
        row("playercreateinfo_skills", raceMask=2, classMask=0, skill=60003, step=1, note="Unselected"),
        row("playercreateinfo_skills", raceMask=0, classMask=2, skill=60004, step=1, note="Unselected")]
    source.rows["player_classlevelstats"] = [
        row("player_classlevelstats", **{"class": 1}, level=level, basehp=7+level, basemana=0)
        for level in range(1, cap+1)]
    source.rows["player_levelstats"] = [
        row("player_levelstats", race=1, **{"class": 1}, level=level,
            str=3+level, agi=5+level, sta=7+level, inte=9+level, spi=11+level)
        for level in range(1, cap+1)]
    source.rows["player_xp_for_level"] = [row("player_xp_for_level", lvl=level, xp_for_next_level=19*level)
                                            for level in range(1, cap)]
    source.rows["item_template"] += [row("item_template", entry=i, name="Original Starter Test Item", stackable=3)
                                     for i in (920101, 920102)]
    source.rows["spell_template"] += [row("spell_template", Id=930101, Effect1=2, EffectTriggerSpell1=930102),
                                      row("spell_template", Id=930102, Effect1=2)]
    return source


class StarterExportTests(unittest.TestCase):
    def test_selected_starter_preserves_all_native_fields_and_closes_references(self):
        source = starter_source()
        result = capture(source, starter_selection())
        validate_bundle(result)
        self.assertEqual(result["format"], "benilla-classic-source-slice-v2")
        self.assertEqual(result["starter_required_keys"], starter.keys(result["tables"]))
        self.assertEqual(result["tables"]["playercreateinfo_spell"][0]["Note"], None)
        self.assertEqual(result["tables"]["playercreateinfo_action"][0]["button"], 0)
        self.assertEqual(result["tables"]["player_classlevelstats"][0]["basemana"], 0)
        self.assertEqual(result["tables"]["player_levelstats"][1]["spi"], 13)
        self.assertEqual(result["required_ids"]["item_template"], [920001, 920101, 920102])
        self.assertEqual(result["required_ids"]["spell_template"], [930001, 930101, 930102])
        self.assertEqual([r["skill"] for r in result["tables"]["playercreateinfo_skills"]], [60001, 60002])
        reasons = {r["reason"] for r in result["report"]["unsupported"]}
        for reason in ["player.skill_line_race_class_tiers_ability_dbc_not_captured",
                       "player.char_start_outfit_dbc_items_not_captured",
                       "player.faction_reputation_dbc_not_captured",
                       "player.equipment_aura_stat_modifiers_required"]:
            self.assertIn(reason, reasons)
        self.assertFalse(result["report"]["runnable"])
        self.assertFalse(result["report"]["capture_complete"])
        self.assertTrue(source.finished)

    def test_starter_queries_are_bounded_parameterized_and_narrow(self):
        expected = {
            "playercreateinfo": (1, 1, 2), "playercreateinfo_item": (1, 1, 2049),
            "playercreateinfo_spell": (1, 1, 513), "playercreateinfo_action": (1, 1, 121),
            "playercreateinfo_skills": (1, 1, 513), "player_classlevelstats": (1, 1, 3, 61),
            "player_levelstats": (1, 1, 1, 3, 61), "player_xp_for_level": (1, 3, 60)}
        for table, limit in starter.STARTER_LIMITS.items():
            query = Query(table, "starter", selection=starter_selection(), limit=limit)
            sql, params = query.sql()
            self.assertEqual(params, expected[table])
            self.assertEqual(sql.count("%s"), len(params))
            self.assertTrue(sql.startswith("SELECT "))
            self.assertNotIn(";", sql)
            self.assertIn("ORDER BY", sql)
            self.assertTrue(sql.endswith("LIMIT %s"))
            for bad in [Query(table, "ids", (1,), limit=limit),
                        Query(table, "starter", (1,), starter_selection(), limit),
                        Query(table, "starter", selection=selection(), limit=limit),
                        Query(table, "starter", selection=starter_selection(), limit=limit+1)]:
                with self.assertRaises(ExportError):
                    bad.sql()
        sql, _ = Query("playercreateinfo_skills", "starter", selection=starter_selection(), limit=512).sql()
        self.assertIn("`raceMask`=0 OR (`raceMask` & %s)<>0", sql)
        self.assertIn("`classMask`=0 OR (`classMask` & %s)<>0", sql)

    def test_skill_predicate_uses_id_minus_one_bits_and_filters_other_classes(self):
        selected = starter_selection().json()
        selected["starter"].update(race=8, **{"class": 11})
        query = Query("playercreateinfo_skills", "starter", selection=Selection.parse(selected), limit=512)
        _, params = query.sql()
        self.assertEqual(params, (128, 1024, 513))
        source = starter_source()
        source.rows["playercreateinfo"].append(row("playercreateinfo", race=1, **{"class": 2}))
        source.rows["player_classlevelstats"].append(row("player_classlevelstats", **{"class": 2}, level=1, basehp=999))
        result = capture(source, starter_selection())
        self.assertEqual(len(result["tables"]["playercreateinfo"]), 1)
        self.assertEqual(len(result["tables"]["player_classlevelstats"]), 3)

    def test_all_starter_native_field_names_match_the_pinned_schema(self):
        expected = {
            "playercreateinfo": "race class map zone position_x position_y position_z orientation",
            "playercreateinfo_item": "race class itemid amount",
            "playercreateinfo_spell": "race class Spell Note",
            "playercreateinfo_action": "race class button action type",
            "playercreateinfo_skills": "raceMask classMask skill step note",
            "player_classlevelstats": "class level basehp basemana",
            "player_levelstats": "race class level str agi sta inte spi",
            "player_xp_for_level": "lvl xp_for_next_level"}
        self.assertEqual(set(expected), set(STARTER_SCHEMA))
        for table, names in expected.items():
            self.assertEqual(COLUMNS[table], names.split())

    def test_starter_selection_refuses_invalid_or_injected_inputs(self):
        for field, value in [("race", 0), ("race", 9), ("race", True), ("class", 6),
                             ("class", 10), ("class", "1 OR 1=1"), ("level_cap", 0),
                             ("level_cap", 61), ("where", "1=1")]:
            selected = starter_selection().json()
            selected["starter"][field] = value
            with self.assertRaises(ExportError):
                Selection.parse(selected)
        value = selection().json()
        value["starter"] = starter_selection().starter.json()
        with self.assertRaises(ExportError):
            Selection.parse(value)

    def test_exact_starter_schema_rejects_drift_before_any_content_read(self):
        for table in STARTER_SCHEMA:
            for change in ["missing", "extra", "type", "nullable", "ordinal", "unsigned"]:
                with self.subTest(table=table, change=change):
                    source = starter_source()
                    fields = source.inventory["tables"][table]["columns"]
                    if change == "missing": fields.pop()
                    elif change == "extra": fields.append({"name":"unknown", "type":"int", "nullable":False, "ordinal":99})
                    elif change == "type": fields[0]["type"] = "bigint unsigned"
                    elif change == "nullable": fields[0]["nullable"] = True
                    elif change == "ordinal": fields[0]["ordinal"] = 99
                    elif change == "unsigned": fields[0]["type"] = fields[0]["type"].replace(" unsigned", "")
                    with self.assertRaises(ExportError):
                        capture(source, starter_selection())
                    self.assertFalse(source.started)
                    self.assertEqual(source.queries, [])

    def test_integer_display_width_is_not_a_schema_domain_change(self):
        source = starter_source()
        source.inventory["tables"]["player_classlevelstats"]["columns"][2]["type"] = "smallint(5) unsigned"
        validate_bundle(capture(source, starter_selection()))

    def test_missing_each_starter_table_is_not_an_empty_table(self):
        for table in STARTER_SCHEMA:
            source = starter_source()
            del source.inventory["tables"][table]
            with self.assertRaisesRegex(ExportError, "schema.missing_table"):
                capture(source, starter_selection())
            self.assertFalse(source.started)

    def test_every_selected_progression_row_is_required_without_gap_filling(self):
        for table in ["playercreateinfo", "player_classlevelstats", "player_levelstats", "player_xp_for_level"]:
            source = starter_source()
            source.rows[table].pop(0)
            with self.assertRaisesRegex(ExportError, "starter.missing"):
                capture(source, starter_selection())
            self.assertTrue(source.finished)

    def test_creation_item_spell_action_and_trigger_references_are_required(self):
        for table, field, key in [("item_template", "entry", 920101), ("item_template", "entry", 920102),
                                  ("spell_template", "Id", 930101), ("spell_template", "Id", 930102)]:
            source = starter_source()
            source.rows[table] = [r for r in source.rows[table] if r[field] != key]
            with self.assertRaisesRegex(ExportError, "reference.missing"):
                capture(source, starter_selection())
            self.assertTrue(source.finished)

    def test_native_empty_optional_sets_still_have_explicit_dbc_blockers(self):
        source = starter_source()
        for table in ["playercreateinfo_item", "playercreateinfo_spell", "playercreateinfo_action", "playercreateinfo_skills"]:
            source.rows[table] = []
        result = capture(source, starter_selection())
        self.assertTrue(any(r["reason"] == "player.skill_line_race_class_tiers_ability_dbc_not_captured"
                            for r in result["report"]["unsupported"]))
        self.assertEqual(result["starter_required_keys"]["playercreateinfo_item"], [])

    def test_cap_one_and_sixty_have_exact_stat_and_xp_ranges(self):
        for cap in (1, 60):
            result = capture(starter_source(cap), starter_selection(cap))
            self.assertEqual(len(result["tables"]["player_classlevelstats"]), cap)
            self.assertEqual(len(result["tables"]["player_levelstats"]), cap)
            self.assertEqual(len(result["tables"]["player_xp_for_level"]), cap-1)

    def test_invalid_native_values_are_rejected_without_defaults(self):
        for table, field, value in [("playercreateinfo_item", "amount", 0),
                                    ("player_classlevelstats", "basehp", 0),
                                    ("player_classlevelstats", "basemana", -1),
                                    ("player_levelstats", "str", 0),
                                    ("player_levelstats", "sta", 256),
                                    ("player_xp_for_level", "xp_for_next_level", 0),
                                    ("playercreateinfo_action", "button", 120),
                                    ("playercreateinfo_action", "action", 0x1000000),
                                    ("playercreateinfo_action", "type", 256),
                                    ("playercreateinfo_skills", "step", 17)]:
            source = starter_source()
            source.rows[table][0][field] = value
            with self.assertRaises(ExportError):
                capture(source, starter_selection())

    def test_native_item_duplicates_fail_closed_even_without_upstream_unique_key(self):
        source = starter_source()
        source.rows["playercreateinfo_item"].append(dict(source.rows["playercreateinfo_item"][0], amount=3))
        with self.assertRaisesRegex(ExportError, "duplicate_key:playercreateinfo_item"):
            capture(source, starter_selection())

    def test_unknown_action_type_is_preserved_and_blocked_not_read_as_a_spell(self):
        source = starter_source()
        source.rows["playercreateinfo_action"][0].update(type=64, action=5)
        result = capture(source, starter_selection())
        self.assertEqual(result["tables"]["playercreateinfo_action"][0]["type"], 64)
        self.assertNotIn(5, result["required_ids"]["spell_template"])
        self.assertTrue(any(r["reason"] == "player.action_type_not_supported" for r in result["report"]["unsupported"]))

    def test_action_spell_learning_is_not_inferred_from_spell_existence(self):
        source = starter_source()
        source.rows["playercreateinfo_spell"] = []
        result = capture(source, starter_selection())
        self.assertIn(930101, result["required_ids"]["spell_template"])
        self.assertTrue(any(r["reason"] == "player.action_spell_learning_not_resolved" for r in result["report"]["unsupported"]))

    def test_starter_outside_selected_region_is_blocked(self):
        source = starter_source()
        source.rows["playercreateinfo"][0]["map"] = 1
        result = capture(source, starter_selection())
        self.assertTrue(any(r["reason"] == "player.start_outside_selection" for r in result["report"]["unsupported"]))

    def test_rehashed_starter_keys_levels_scope_and_references_cannot_bypass_validation(self):
        base = capture(starter_source(), starter_selection())
        for mutate, code in [
            (lambda b: b["tables"]["player_levelstats"].pop(), "starter.missing_or_unexpected_levels"),
            (lambda b: b["tables"]["playercreateinfo" ][0].update(race=2), "starter.unselected_race"),
            (lambda b: b["tables"]["playercreateinfo_skills"][0].update(raceMask=2), "starter.unselected_skill_mask"),
            (lambda b: b["tables"]["playercreateinfo_item"][0].update(itemid=920999), "bundle.starter_item_reference"),
            (lambda b: b["tables"]["playercreateinfo_spell"][0].update(Spell=930999), "bundle.starter_spell_reference"),
            (lambda b: b["tables"]["playercreateinfo_action"][0].update(action=930999), "bundle.starter_spell_reference")]:
            changed = copy.deepcopy(base)
            mutate(changed)
            for table, rows in changed["tables"].items():
                from classic_export.core import KEYS
                rows.sort(key=lambda r: tuple(r[k] for k in KEYS[table]))
                changed["table_sha256"][table] = digest(rows)
            changed["starter_required_keys"] = starter.keys(changed["tables"])
            with self.assertRaisesRegex(ExportError, code):
                validate_bundle(changed)
        changed = copy.deepcopy(base)
        changed["starter_required_keys"]["playercreateinfo"][0][0] = 2
        with self.assertRaisesRegex(ExportError, "starter_keys"):
            validate_bundle(changed)

    def test_starter_query_limit_plus_one_rejects_without_partial_bundle(self):
        source = starter_source()
        source.rows["playercreateinfo_skills"] = [
            row("playercreateinfo_skills", skill=i+1, step=0, note=None) for i in range(513)]
        with self.assertRaisesRegex(ExportError, "rows.limit:playercreateinfo_skills"):
            capture(source, starter_selection())
        self.assertTrue(source.finished)

    def test_starter_item_required_spell_is_captured_and_validated(self):
        source = starter_source()
        source.rows["item_template"][1]["requiredspell"] = 930105
        source.rows["spell_template"].append(row("spell_template", Id=930105, Effect1=2))
        result = capture(source, starter_selection())
        self.assertIn(930105, result["required_ids"]["spell_template"])
        result["tables"]["item_template"][1]["requiredspell"] = 930106
        result["table_sha256"]["item_template"] = digest(result["tables"]["item_template"])
        with self.assertRaisesRegex(ExportError, "item_required_spell_reference"):
            validate_bundle(result)

    def test_composite_manifest_requires_integers_not_bool_or_decimal_equivalents(self):
        for value in (True, 1.0):
            result = capture(starter_source(), starter_selection())
            result["starter_required_keys"]["playercreateinfo"][0][0] = value
            with self.assertRaisesRegex(ExportError, "starter_keys"):
                validate_bundle(result)

    def test_v1_roundtrip_remains_supported_but_cannot_claim_starter_capture(self):
        source = FakeSource()
        result = capture(source, selection())
        self.assertEqual(result["format"], "benilla-classic-source-slice-v1")
        self.assertNotIn("starter_required_keys", result)
        self.assertFalse(any("FROM `player" in query for query, _ in source.queries))
        result["format"] = "benilla-classic-source-slice-v2"
        result["starter_required_keys"] = {}
        with self.assertRaisesRegex(ExportError, "selection_version"):
            validate_bundle(result)

    def test_offline_plan_records_starter_tables_and_v2(self):
        path = Path(__file__).with_name("classic-export.py")
        spec = importlib.util.spec_from_file_location("export_cli_starter", path)
        cli = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cli)
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            (directory/"schema.json").write_bytes(canonical(starter_source().inventory))
            (directory/"selection.json").write_bytes(canonical(starter_selection().json()))
            with mock.patch("sys.argv", [str(path), "plan", "--schema", str(directory/"schema.json"),
                                         "--selection", str(directory/"selection.json"), "--output", str(directory/"plan.json")]), \
                 mock.patch("builtins.print"):
                self.assertEqual(cli.main(), 0)
            plan = decode_json((directory/"plan.json").read_bytes(), 65536)
            self.assertEqual(plan["format"], "benilla-classic-capture-plan-v2")
            self.assertTrue(set(STARTER_SCHEMA) <= set(plan["capture_tables"]))
            self.assertFalse(plan["runnable"])


class EventAiCaptureTests(unittest.TestCase):
    def test_complete_query_covers_both_entry_and_guid_without_event_filters(self):
        source = starter_source()
        source.rows["creature_ai_scripts"] = [
            row("creature_ai_scripts", id=950001, creature_id=900001),
            row("creature_ai_scripts", id=950002, creature_id=-1),
            row("creature_ai_scripts", id=950003, creature_id=-2),
            row("creature_ai_scripts", id=950004, creature_id=900002),
        ]
        bundle = capture(source, starter_selection())
        self.assertEqual(bundle["event_ai_capture"]["selectors"], [-1, 900001])
        self.assertEqual(bundle["tables"]["creature_ai_scripts"], source.rows["creature_ai_scripts"][:2])
        sql, args = next(q for q in source.queries if "FROM `creature_ai_scripts`" in q[0])
        columns = ",".join(f"`{name}` AS `{name}`" for name in COLUMNS["creature_ai_scripts"])
        self.assertEqual(sql, f"SELECT {columns} FROM `creature_ai_scripts` "
                             "WHERE `creature_id` IN (%s,%s) ORDER BY `id` LIMIT %s")
        self.assertEqual(args, (-1, 900001, MAX_ROWS+1))
        self.assertEqual(len([i for i in bundle["report"]["unsupported"]
                              if i["reason"] == "creature.event_ai_matching_row_requires_execution"]), 2)
        self.assertTrue(source.finished)

    def test_empty_query_result_records_explicit_coverage(self):
        source = starter_source()
        bundle = capture(source, starter_selection())
        self.assertEqual(bundle["tables"]["creature_ai_scripts"], [])
        self.assertEqual(bundle["event_ai_capture"], {
            "format": "benilla-classic-event-ai-selection-v1", "selector_column": "creature_id",
            "selectors": [-1, 900001], "row_limit": MAX_ROWS,
            "filter": "all_matching_rows_no_event_filters"})

    def test_forged_missing_or_unselected_coverage_rejected(self):
        original = capture(starter_source(), starter_selection())
        for action in (lambda b: b.pop("event_ai_capture"),
                       lambda b: b["event_ai_capture"].__setitem__("selectors", [900001]),
                       lambda b: b["event_ai_capture"].__setitem__("filter", "only_active_events")):
            bundle = copy.deepcopy(original)
            action(bundle)
            with self.assertRaisesRegex(ExportError, "event_ai"):
                validate_bundle(bundle)
        class WrongRows(FakeSource):
            def fetch(self, query):
                if query.mode == "event_ai_selectors":
                    return [row("creature_ai_scripts", id=950001, creature_id=-2)]
                return super().fetch(query)
        source = starter_source()
        source.__class__ = WrongRows
        with self.assertRaisesRegex(ExportError, "event_ai.unselected_row"):
            capture(source, starter_selection())
        self.assertTrue(source.finished)

    def test_old_v2_without_event_evidence_still_validates(self):
        from classic_export.capture import full_report
        bundle = capture(starter_source(), starter_selection())
        bundle.pop("event_ai_capture")
        bundle.pop("event_text_capture")
        for table in ("broadcast_text", "broadcast_text_locale"):
            bundle["tables"].pop(table)
            bundle["table_sha256"].pop(table)
        bundle["tables"].pop("creature_ai_scripts")
        bundle["table_sha256"].pop("creature_ai_scripts")
        bundle["schema"]["tables"].pop("creature_ai_scripts")
        bundle["schema_sha256"] = digest(bundle["schema"])
        refresh_event_text(bundle)
        if "spell_script_capture" in bundle:
            from classic_export.spell_registry import evidence as registry_evidence
            bundle["spell_script_capture"] = registry_evidence(bundle)
        from classic_export.native_spawn import evidence
        bundle["native_spawn_capture"] = evidence(bundle)
        profiles = schema_profile(bundle["schema"], set(bundle["tables"]) | {"db_version"})
        bundle["report"]["unsupported"] = full_report(bundle["tables"], profiles, starter_selection())
        validate_bundle(bundle)

    def test_selector_signed_domain_and_schema_are_enforced(self):
        for value in (0, -0x80000000, 0x80000000, True):
            with self.subTest(value=value), self.assertRaises(ExportError):
                Query("creature_ai_scripts", "event_ai_selectors", (value,)).sql()
        for kind in ("int unsigned", "bigint", "float"):
            source = starter_source()
            source.inventory["tables"]["creature_ai_scripts"]["columns"][1]["type"] = kind
            with self.subTest(kind=kind), self.assertRaisesRegex(ExportError, "schema.event_text_type"):
                capture(source, starter_selection())

    def test_event_table_uses_existing_snapshot_and_row_budgets(self):
        source = starter_source()
        source.inventory["tables"]["creature_ai_scripts"]["engine"] = "MyISAM"
        with self.assertRaisesRegex(ExportError, "quiescent"):
            capture(source, starter_selection())
        source = starter_source()
        source.rows["creature_ai_scripts"] = [row("creature_ai_scripts", id=i+1, creature_id=-1) for i in range(MAX_ROWS+1)]
        with self.assertRaisesRegex(ExportError, "rows.limit:creature_ai_scripts"):
            capture(source, starter_selection())
        self.assertTrue(source.finished)



class LearningExportTests(unittest.TestCase):
    def request(self, source=None):
        from classic_export.learning import REQUEST_FORMAT
        from classic_export.core import CORE_REVISION
        return {"format": REQUEST_FORMAT, "core_revision": CORE_REVISION,
                "selection": starter_selection().json(), "source_sha256": "a"*64,
                "dbc_sha256": {"Spell": "b"*64}, "spell_ids": [50001], "item_ids": []}

    def source(self):
        source = starter_source()
        source.rows["spell_template"] += [row("spell_template", Id=i, Effect1=2) for i in range(50001, 50031)]
        return source

    def test_sql_learning_rank_fixed_point_and_exact_selector_evidence(self):
        source = self.source()
        source.rows["spell_learn_spell"] = [row("spell_learn_spell", entry=i, SpellID=i+1, Active=i % 2)
                                                   for i in range(50001, 50025)]
        source.rows["spell_chain"] = [row("spell_chain", spell_id=50025, prev_spell=50026,
                                              first_spell=50027, rank=2, req_spell=50028)]
        bundle = capture(source, starter_selection(), dependency_request=self.request())
        validate_bundle(bundle)
        self.assertTrue(set(range(50001, 50029)) <= set(bundle["required_ids"]["spell_template"]))
        self.assertNotIn(50029, bundle["required_ids"]["spell_template"])
        evidence = bundle["learning_capture"]
        self.assertEqual(evidence["spell_ids"], bundle["required_ids"]["spell_template"])
        self.assertEqual(evidence["filter"], "all_matching_rows_no_loader_filters")
        self.assertEqual(evidence["dependency_request"], self.request())
        self.assertEqual(len(bundle["tables"]["spell_learn_spell"]), 24)
        self.assertTrue(source.finished)

    def test_learning_and_rank_cycles_terminate_without_duplicates(self):
        source = self.source()
        source.rows["spell_learn_spell"] = [row("spell_learn_spell", entry=50001, SpellID=50002, Active=1),
                                                row("spell_learn_spell", entry=50002, SpellID=50001, Active=0)]
        source.rows["spell_chain"] = [row("spell_chain", spell_id=50001, prev_spell=50002, first_spell=50001, rank=1),
                                          row("spell_chain", spell_id=50002, prev_spell=50001, first_spell=50001, rank=2)]
        bundle = capture(source, starter_selection(), dependency_request=self.request())
        self.assertEqual(len(bundle["tables"]["spell_learn_spell"]), 2)
        self.assertEqual(len(bundle["tables"]["spell_chain"]), 2)
        self.assertEqual(len([q for q in source.queries if "FROM `spell_learn_spell`" in q[0] and "WHERE `entry`" in q[0]]), 2)

    def test_absence_evidence_cannot_be_removed_or_relabelled(self):
        bundle = capture(starter_source(), starter_selection())
        self.assertEqual(bundle["tables"]["spell_learn_spell"], [])
        for mutate in (lambda b: b.pop("learning_capture"),
                       lambda b: b["learning_capture"]["spell_ids"].pop(),
                       lambda b: b["tables"].pop("spell_chain")):
            copy_bundle = copy.deepcopy(bundle)
            mutate(copy_bundle)
            with self.assertRaises(ExportError):
                validate_bundle(copy_bundle)

    def test_missing_table_wrong_signedness_extra_columns_and_nullability_fail(self):
        for mode in ("missing", "unsigned", "nullable", "extra"):
            source = starter_source()
            columns = source.inventory["tables"]["spell_chain"]["columns"]
            if mode == "missing":
                source.inventory["tables"].pop("spell_chain")
            elif mode == "unsigned":
                columns[0]["type"] = "mediumint unsigned"
            elif mode == "nullable":
                columns[0]["nullable"] = True
            else:
                columns.append({"name": "unexpected", "type": "int", "nullable": False, "ordinal": 6})
            with self.subTest(mode=mode), self.assertRaises(ExportError):
                capture(source, starter_selection())
            self.assertFalse(source.started)

    def test_missing_learned_spell_fails_and_ends_read(self):
        source = self.source()
        source.rows["spell_learn_spell"] = [row("spell_learn_spell", entry=50001, SpellID=55000, Active=1)]
        with self.assertRaisesRegex(ExportError, "missing_or_unexpected:spell_template"):
            capture(source, starter_selection(), dependency_request=self.request())
        self.assertTrue(source.finished)

    def test_bad_request_rejected_before_sql_and_id_queries_are_parameterized(self):
        for change in ({"spell_ids": [True]}, {"spell_ids": [2, 1]}, {"spell_ids": [1, 1]},
                       {"spell_ids": [1]*513}, {"item_ids": [-1]}, {"format": "bad"},
                       {"source_sha256": "x"*64}, {"dbc_sha256": {"../../evil": "a"*64}},
                       {"selection": selection().json()}):
            source = self.source()
            request = self.request() | change
            with self.subTest(change=change), self.assertRaises(ExportError):
                capture(source, starter_selection(), dependency_request=request)
            self.assertFalse(source.started)
        sql, args = Query("spell_learn_spell", "ids", (50001,), limit=32).sql()
        self.assertNotIn("50001", sql)
        self.assertEqual(args, (50001, 33))
        self.assertIn("ORDER BY `entry`,`SpellID`", sql)

    def test_global_learning_probe_is_bounded_and_distinct_from_selected_empty(self):
        source = starter_source()
        source.rows["spell_learn_spell"] = [row("spell_learn_spell", entry=50001+i, SpellID=51001+i, Active=1)
                                                 for i in range(5)]
        bundle = capture(source, starter_selection())
        self.assertEqual(bundle["tables"]["spell_learn_spell"], [])
        self.assertEqual(bundle["learning_capture"]["global_learn_spell_probe"]["rows"],
                         source.rows["spell_learn_spell"][:1])
        probes = [q for q in source.queries if "FROM `spell_learn_spell` WHERE 1=1" in q[0]]
        self.assertEqual(len(probes), 1)
        self.assertEqual(probes[0][1], (1,))
        invalid = copy.deepcopy(bundle)
        invalid["learning_capture"]["global_learn_spell_probe"]["rows"] *= 2
        with self.assertRaises(ExportError):
            validate_bundle(invalid)
        with self.assertRaises(ExportError):
            Query("item_template", "learning_presence", limit=1).sql()
        with self.assertRaises(ExportError):
            Query("spell_learn_spell", "learning_presence", limit=2).sql()

    def test_probe_cannot_claim_empty_with_captured_relationships(self):
        source = self.source()
        source.rows["spell_learn_spell"] = [row("spell_learn_spell", entry=50001, SpellID=50002, Active=1)]
        bundle = capture(source, starter_selection(), dependency_request=self.request())
        bundle["learning_capture"]["global_learn_spell_probe"]["rows"] = []
        with self.assertRaisesRegex(ExportError, "empty_probe_with_selected_rows"):
            validate_bundle(bundle)

    def test_invalid_cli_request_fails_before_authentication(self):
        path = Path(__file__).with_name("classic-export.py")
        spec = importlib.util.spec_from_file_location("export_cli_learning", path)
        cli = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cli)
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            (directory/"selection.json").write_bytes(canonical(starter_selection().json()))
            (directory/"request.json").write_bytes(canonical(self.request() | {"spell_ids": [True]}))
            with mock.patch("sys.argv", [str(path), "capture", "--database", "original", "--user", "original",
                "--selection", str(directory/"selection.json"), "--dependency-request", str(directory/"request.json"),
                "--output", str(directory/"output.json")]), mock.patch("builtins.print"), \
                mock.patch("classic_export.driver.LocalSqlSource") as connection:
                self.assertEqual(cli.main(), 2)
                connection.assert_not_called()
            self.assertFalse((directory/"output.json").exists())

    def test_invalid_active_negative_chain_and_budget_fail(self):
        for changes in ({"Active": 2}, {"SpellID": 0}):
            source = self.source()
            source.rows["spell_learn_spell"] = [row("spell_learn_spell", entry=50001, SpellID=50002, Active=1) | changes]
            with self.assertRaises(ExportError):
                capture(source, starter_selection(), dependency_request=self.request())
        source = self.source()
        source.rows["spell_chain"] = [row("spell_chain", spell_id=50001, prev_spell=-1)]
        with self.assertRaisesRegex(ExportError, "chain_reference"):
            capture(source, starter_selection(), dependency_request=self.request())
        source = self.source()
        with mock.patch("classic_export.capture.MAX_SPELLS", 3), self.assertRaises(ExportError):
            capture(source, starter_selection(), dependency_request=self.request())



class NativeSpawnCaptureTests(unittest.TestCase):
    def test_capture_selects_all_dependencies_in_one_readonly_snapshot(self):
        source = starter_source()
        fetch = source.fetch
        starts = []
        start_read = source.start_read
        def start(*args):
            starts.append(args)
            return start_read(*args)
        def checked_fetch(query):
            self.assertTrue(source.started)
            self.assertFalse(source.finished)
            return fetch(query)
        source.start_read, source.fetch = start, checked_fetch
        source.rows["creature_spawn_data"] = [row("creature_spawn_data", Guid=1, Id=17)]
        source.rows["creature_spawn_data_template"].append(row("creature_spawn_data_template", Entry=17,
            NpcFlags=-1, UnitFlags=-1, EquipmentId=-1, Name="Original alternate spawn", ModelId=940099))
        source.rows["creature_model_info"].append(row("creature_model_info", modelid=940099))
        source.rows["game_event_creature"] = [row("game_event_creature", guid=1, event=-7),
                                               row("game_event_creature", guid=1, event=0)]
        source.rows["pool_creature"] = [row("pool_creature", guid=1, pool_entry=18, chance=0)]
        source.rows["pool_creature_template"] = [row("pool_creature_template", id=900001, pool_entry=19)]
        result = capture(source, starter_selection())
        coverage = result["native_spawn_capture"]
        self.assertEqual(coverage["selectors"]["creature_spawn_data_template"]["ids"], [0, 17])
        self.assertEqual(coverage["selectors"]["creature_template_addon"]["ids"], [900001])
        self.assertEqual([r["event"] for r in result["tables"]["game_event_creature"]], [-7, 0])
        self.assertIn(940099, result["required_ids"]["creature_model_info"])
        self.assertEqual(len(starts), 1)
        self.assertEqual(source.versions, 2)
        self.assertTrue(source.finished)
        self.assertEqual(coverage["snapshot"]["table_sha256"]["creature_spawn_data"],
                         result["table_sha256"]["creature_spawn_data"])
        sql, params = next((sql, params) for sql, params in source.queries
                           if "FROM `creature_spawn_data_template`" in sql)
        self.assertEqual(params, (0, 17, MAX_ROWS+1))
        self.assertIn("`Entry` IN (%s,%s)", sql)

    def test_unhandled_hooks_zero_values_and_addon_text_are_never_filtered(self):
        source = starter_source()
        source.rows["game_event_creature"] = [row("game_event_creature", guid=1, event=0)]
        source.rows["pool_creature"] = [row("pool_creature", guid=1, pool_entry=0, chance=0)]
        source.rows["creature_addon"] = [row("creature_addon", guid=1, auras="unparsed source text")]
        result = capture(source, starter_selection())
        for table in ("game_event_creature", "pool_creature", "creature_addon"):
            self.assertEqual(len(result["tables"][table]), 1)
        self.assertEqual(result["tables"]["creature_addon"][0]["auras"], "unparsed source text")

    def test_capture_requires_all_exact_schemas_before_start(self):
        from classic_export.native_spawn import TABLES
        for table in TABLES:
            source = starter_source()
            source.inventory["tables"].pop(table)
            with self.subTest(table=table), self.assertRaisesRegex(ExportError, "schema.missing_table"):
                capture(source, starter_selection())
            self.assertFalse(source.started)
        mutations = [("game_event_creature", "event", {"type": "smallint unsigned"}),
                     ("creature_spawn_data_template", "UnitFlags", {"type": "int"}),
                     ("creature_spawn_data_template", "NpcFlags", {"type": "int unsigned"}),
                     ("creature_template_addon", "emote", {"type": "int unsigned"}),
                     ("creature_addon", "auras", {"nullable": False}),
                     ("pool_creature", "chance", {"type": "double unsigned"}),
                     ("creature_spawn_data", "Guid", {"ordinal": 3})]
        for table, name, changes in mutations:
            source = starter_source()
            next(c for c in source.inventory["tables"][table]["columns"] if c["name"] == name).update(changes)
            with self.subTest(table=table, name=name), self.assertRaisesRegex(ExportError, "schema.starter_type"):
                capture(source, starter_selection())
            self.assertFalse(source.started)
        source = starter_source()
        source.inventory["tables"]["creature_addon"]["columns"].append(
            {"name": "UnknownHook", "type": "int", "nullable": False, "ordinal": 8})
        with self.assertRaisesRegex(ExportError, "schema.starter_columns"):
            capture(source, starter_selection())

    def test_selector_domains_modes_and_injection_reject(self):
        for table, ids in [("creature_spawn_data", (0,)), ("creature_spawn_data", (-1,)),
                           ("creature_spawn_data_template", (-1,)), ("creature_addon", (True,)),
                           ("creature_addon", (0x100000000,)), ("pool_creature", ("1 OR 1=1",)),
                           ("creature_template_addon", (1, 1))]:
            with self.subTest(table=table, ids=ids), self.assertRaises(ExportError):
                Query(table, "native_spawn_selectors", ids).sql()
        self.assertEqual(Query("creature_spawn_data_template", "native_spawn_selectors", (0,)).sql()[1],
                         (0, MAX_ROWS+1))
        with self.assertRaisesRegex(ExportError, "native_spawn_mode"):
            Query("creature_addon", "ids", (1,)).sql()
        with self.assertRaisesRegex(ExportError, "native_spawn_selectors"):
            Query("creature", "native_spawn_selectors", (1,)).sql()

    def test_missing_coverage_partial_tables_and_tampered_snapshot_reject(self):
        for mutate in [lambda b: b.pop("native_spawn_capture"),
                       lambda b: b["native_spawn_capture"]["selectors"]["creature_addon"].update(ids=[]),
                       lambda b: b["native_spawn_capture"]["selectors"]["creature_spawn_data_template"].update(ids=[]),
                       lambda b: b["native_spawn_capture"].update(filter="nonzero_rows_only"),
                       lambda b: b["native_spawn_capture"]["snapshot"].update(consistency="other_snapshot")]:
            bundle = capture(starter_source(), starter_selection())
            mutate(bundle)
            with self.assertRaisesRegex(ExportError, "native_spawn"):
                validate_bundle(bundle)
        bundle = capture(starter_source(), starter_selection())
        bundle["tables"].pop("pool_creature")
        bundle["table_sha256"].pop("pool_creature")
        with self.assertRaisesRegex(ExportError, "native_spawn.tables_required"):
            validate_bundle(bundle)

    def test_legacy_missing_is_valid_but_empty_cannot_be_invented(self):
        from classic_export import native_spawn
        from classic_export.capture import full_report
        bundle = capture(starter_source(), starter_selection())
        bundle.pop("native_spawn_capture")
        for table in native_spawn.TABLES:
            bundle["tables"].pop(table)
            bundle["table_sha256"].pop(table)
            bundle["schema"]["tables"].pop(table)
        bundle["schema_sha256"] = digest(bundle["schema"])
        refresh_event_text(bundle)
        if "spell_script_capture" in bundle:
            from classic_export.spell_registry import evidence as registry_evidence
            bundle["spell_script_capture"] = registry_evidence(bundle)
        profiles = schema_profile(bundle["schema"], set(bundle["tables"]) | {"db_version"})
        bundle["report"]["unsupported"] = full_report(bundle["tables"], profiles, starter_selection())
        validate_bundle(bundle)
        self.assertIsNone(native_spawn.scope(bundle, 900001))
        self.assertTrue(any(i["reason"] == "spawn.overrides_pools_events_not_captured"
                            for i in bundle["report"]["unsupported"]))

    def test_reference_and_duplicate_entry_fail_without_default_substitution(self):
        source = starter_source()
        source.rows["creature_spawn_data"] = [row("creature_spawn_data", Guid=1, Id=17)]
        with self.assertRaisesRegex(ExportError, "native_spawn.template_reference_missing"):
            capture(source, starter_selection())
        self.assertTrue(source.finished)
        source = starter_source()
        source.rows["creature_spawn_data_template"][0]["ModelId"] = 941234
        with self.assertRaisesRegex(ExportError, "reference.missing"):
            capture(source, starter_selection())
        source = starter_source()
        source.rows["creature_spawn_data_template"].append(
            dict(source.rows["creature_spawn_data_template"][0], UnitFlags=1))
        with self.assertRaisesRegex(ExportError, "duplicate_key"):
            capture(source, starter_selection())

    def test_signed_event_and_override_domains_are_checked(self):
        for name, value in [("NpcFlags", -2), ("NpcFlags", 0x80000000),
                            ("UnitFlags", -2), ("UnitFlags", 0x100000000), ("EquipmentId", -2)]:
            source = starter_source()
            source.rows["creature_spawn_data_template"][0][name] = value
            with self.subTest(name=name, value=value), self.assertRaises(ExportError):
                capture(source, starter_selection())
        for value in (-32769, 32768, True):
            source = starter_source()
            source.rows["game_event_creature"] = [row("game_event_creature", guid=1, event=value)]
            with self.subTest(event=value), self.assertRaises(ExportError):
                capture(source, starter_selection())

    def test_unselected_rows_max_plus_one_and_snapshot_changes_reject(self):
        source = starter_source()
        fetch = source.fetch
        def extra(query):
            if query.table == "creature_addon":
                return [row("creature_addon", guid=2)]
            return fetch(query)
        source.fetch = extra
        with self.assertRaisesRegex(ExportError, "native_spawn.unselected_row"):
            capture(source, starter_selection())
        source = starter_source()
        source.rows["game_event_creature"] = [row("game_event_creature", guid=1, event=i)
                                               for i in range(MAX_ROWS+1)]
        with self.assertRaisesRegex(ExportError, "rows.limit:game_event_creature"):
            capture(source, starter_selection())
        self.assertTrue(source.finished)
        source = starter_source()
        source.mutate_version = True
        with self.assertRaisesRegex(ExportError, "snapshot.version_changed"):
            capture(source, starter_selection())

    def test_selector_text_and_aggregate_budgets_reject_before_claiming_absence(self):
        from classic_export import native_spawn
        tables = {"creature": [{"guid": 1}], "creature_template": [{"Entry": 1}],
                  "creature_spawn_data": [{"Guid": 1, "Id": 1}]}
        with mock.patch.object(native_spawn, "MAX_ROWS", 1), self.assertRaisesRegex(ExportError, "selector_limit"):
            native_spawn.selectors(tables)
        source = starter_source()
        source.rows["creature_addon"] = [row("creature_addon", guid=1, auras="x"*16385)]
        with self.assertRaisesRegex(ExportError, "row.text_limit"):
            capture(source, starter_selection())
        source = starter_source()
        with mock.patch("classic_export.capture.MAX_BUNDLE_BYTES", 1), self.assertRaisesRegex(ExportError, "aggregate_byte_limit"):
            capture(source, starter_selection())
        self.assertTrue(source.finished)

    def test_empty_region_does_not_query_empty_in_and_still_captures_default(self):
        source = starter_source()
        s = starter_selection().json()
        s.update(min_milliunits=[50000]*3, max_milliunits=[51000]*3, quest_ids=[])
        bundle = capture(source, Selection.parse(s))
        self.assertEqual(bundle["native_spawn_capture"]["selectors"]["creature_addon"]["ids"], [])
        self.assertEqual(bundle["tables"]["creature_addon"], [])
        self.assertEqual(bundle["native_spawn_capture"]["selectors"]["creature_spawn_data_template"]["ids"], [0])
        self.assertFalse(any(" IN ()" in sql for sql, _ in source.queries))


class SpellRegistryCaptureTests(unittest.TestCase):
    def test_exact_selector_retains_empty_unknown_and_unmatched_rows_are_not_scanned(self):
        from classic_export import spell_registry
        source = starter_source()
        source.rows["spell_scripts"] = [
            row("spell_scripts", Id=930001, ScriptName=""),
            row("spell_scripts", Id=930101, ScriptName="original_unimplemented_handler"),
            row("spell_scripts", Id=-1, ScriptName="unselected_wrapped_id"),
            row("spell_scripts", Id=22, ScriptName="unselected")]
        bundle = capture(source, starter_selection())
        self.assertEqual(bundle["tables"]["spell_scripts"], source.rows["spell_scripts"][:2])
        proof = bundle["spell_script_capture"]
        self.assertEqual(proof, spell_registry.evidence(bundle))
        self.assertEqual(proof["selector"], {"column": "Id", "ids": [930001, 930101, 930102]})
        self.assertEqual(proof["snapshot"]["table_sha256"],
                         {table: bundle["table_sha256"][table] for table in ("spell_template", "spell_scripts")})
        self.assertEqual(spell_registry.scope(bundle, 930001)["rows"], source.rows["spell_scripts"][:1])
        self.assertEqual(spell_registry.scope(bundle, 930102)["rows"], [])
        queries = [(sql, args) for sql, args in source.queries if "FROM `spell_scripts`" in sql]
        self.assertEqual(len(queries), 1)
        sql, args = queries[0]
        self.assertEqual(args, (930001, 930101, 930102, 513))
        self.assertIn("WHERE `Id` IN (%s,%s,%s)", sql)
        self.assertNotIn("ScriptName` !=", sql)
        self.assertEqual(sum(i["reason"] == "spell.registry_row_requires_named_handler_semantics"
                             for i in bundle["report"]["unsupported"]), 2)

    def test_uint32_wrapped_negative_ids_are_rejected_before_registry_query(self):
        for invalid in (0, -1, -0x80000000, 0x80000000, 0xffffffff, True, "1"):
            with self.subTest(invalid=invalid), self.assertRaisesRegex(ExportError, "selector_domain"):
                Query("spell_scripts", "spell_registry_selectors", (invalid,), limit=512).sql()
        self.assertEqual(Query("spell_scripts", "spell_registry_selectors", (0x7fffffff,), limit=512)
                         .sql()[1], (0x7fffffff, 513))
        source = starter_source()
        source.rows["spell_template"][0]["EffectTriggerSpell1"] = 0xffffffff
        source.rows["spell_template"].append(row("spell_template", Id=0xffffffff))
        source.rows["spell_scripts"] = [row("spell_scripts", Id=-1, ScriptName="must_not_appear_absent")]
        with self.assertRaisesRegex(ExportError, "selector_domain"):
            capture(source, starter_selection())
        self.assertFalse(any("FROM `spell_scripts`" in sql for sql, _ in source.queries))
        self.assertTrue(source.finished)
        for query in [Query("spell_scripts", "ids", (1,)),
                      Query("creature", "spell_registry_selectors", (1,), limit=512),
                      Query("spell_scripts", "spell_registry_selectors", (1, 1), limit=512),
                      Query("spell_scripts", "spell_registry_selectors", (1,), limit=513)]:
            with self.assertRaises(ExportError):
                query.sql()

    def test_schema_primary_key_width_signedness_order_and_nullability_are_exact(self):
        mutations = [lambda t: t.pop("primary_key"), lambda t: t.update(primary_key=[]),
                     lambda t: t.update(primary_key=["Id", "ScriptName"]),
                     lambda t: t["columns"][0].update(type="int unsigned"),
                     lambda t: t["columns"][0].update(type="bigint"),
                     lambda t: t["columns"][0].update(nullable=True),
                     lambda t: t["columns"][1].update(type="varchar(64)"),
                     lambda t: t["columns"][1].update(type="char(63)"),
                     lambda t: t["columns"][1].update(nullable=True),
                     lambda t: t["columns"][1].update(ordinal=3),
                     lambda t: t["columns"][1].update(name="command")]
        for mutate in mutations:
            source = starter_source()
            mutate(source.inventory["tables"]["spell_scripts"])
            with self.subTest(mutate=mutate), self.assertRaises(ExportError):
                capture(source, starter_selection())
            self.assertFalse(source.started)
        source = starter_source()
        source.inventory["tables"]["spell_scripts"]["columns"][0]["type"] = "int(11)"
        validate_bundle(capture(source, starter_selection()))
        source.inventory["tables"].pop("spell_scripts")
        with self.assertRaisesRegex(ExportError, "schema.missing_table"):
            capture(source, starter_selection())

    def test_driver_inspects_actual_primary_constraint_with_fixed_readonly_query(self):
        source = LocalSqlSource.__new__(LocalSqlSource)
        meta = schema()["tables"]["spell_scripts"]
        source._select = mock.Mock(side_effect=[
            [{"name": "spell_scripts", "engine": "MyISAM", "table_type": "BASE TABLE"}],
            [dict(c, table_name="spell_scripts", nullable="NO") for c in meta["columns"]],
            [{"name": "Id", "ordinal": 1}]])
        observed = source.inspect()["tables"]["spell_scripts"]
        self.assertEqual(observed["primary_key"], ["Id"])
        sql, params, limit = source._select.call_args.args
        self.assertIn("information_schema.KEY_COLUMN_USAGE", sql)
        self.assertEqual(params, ("spell_scripts", "PRIMARY"))
        self.assertEqual(limit, 3)
        self.assertTrue(sql.startswith("SELECT "))
        self.assertEqual(observed["engine"], "MyISAM")

    def test_snapshot_engine_policy_applies_to_registry(self):
        source = starter_source()
        source.inventory["tables"]["spell_scripts"]["engine"] = "MyISAM"
        with self.assertRaisesRegex(ExportError, "quiescent_required"):
            capture(source, starter_selection())
        result = capture(source, starter_selection(), quiescent=True)
        self.assertEqual(result["spell_script_capture"]["snapshot"]["consistency"],
                         "operator_quiescent_snapshot")

    def test_coverage_tampering_and_unselected_driver_rows_fail_closed(self):
        for mutate in [lambda b: b.pop("spell_script_capture"),
                       lambda b: b["spell_script_capture"]["selector"]["ids"].pop(),
                       lambda b: b["spell_script_capture"].update(filter="nonempty_names"),
                       lambda b: b["spell_script_capture"]["snapshot"].update(selection_sha256="0"*64),
                       lambda b: b["spell_script_capture"]["snapshot"].update(schema_sha256="0"*64),
                       lambda b: b["spell_script_capture"]["snapshot"].update(db_version_sha256="0"*64)]:
            bundle = capture(starter_source(), starter_selection())
            mutate(bundle)
            with self.assertRaisesRegex(ExportError, "spell_registry"):
                validate_bundle(bundle)
        class WrongRows(FakeSource):
            def fetch(self, query):
                if query.table == "spell_scripts":
                    return [row("spell_scripts", Id=12, ScriptName="outside_selector")]
                return super().fetch(query)
        source = starter_source()
        source.__class__ = WrongRows
        with self.assertRaisesRegex(ExportError, "unselected_row"):
            capture(source, starter_selection())

    def test_bounds_duplicates_and_absence_rewrite_are_rejected(self):
        for rows in [[row("spell_scripts", Id=930001, ScriptName="x"*65)],
                     [row("spell_scripts", Id=930001)]*2]:
            source = starter_source()
            source.rows["spell_scripts"] = rows
            with self.assertRaises(ExportError):
                capture(source, starter_selection())
        source = starter_source()
        source.rows["spell_scripts"] = [row("spell_scripts", Id=930001)]
        bundle = capture(source, starter_selection())
        bundle["tables"]["spell_scripts"] = []
        bundle["table_sha256"]["spell_scripts"] = digest([])
        with self.assertRaisesRegex(ExportError, "coverage_mismatch"):
            validate_bundle(bundle)
        class Overflow(FakeSource):
            def fetch(self, query):
                if query.table == "spell_scripts":
                    return [row("spell_scripts", Id=i+1) for i in range(513)]
                return super().fetch(query)
        source = starter_source()
        source.__class__ = Overflow
        with self.assertRaisesRegex(ExportError, "rows.limit:spell_scripts"):
            capture(source, starter_selection())

    def test_legacy_unknown_and_zero_selector_remain_distinct(self):
        from classic_export import spell_registry, native_spawn
        bundle = capture(starter_source(), starter_selection())
        bundle.pop("spell_script_capture")
        bundle["tables"].pop("spell_scripts")
        bundle["table_sha256"].pop("spell_scripts")
        bundle["schema"]["tables"].pop("spell_scripts")
        bundle["schema_sha256"] = digest(bundle["schema"])
        refresh_event_text(bundle)
        bundle["native_spawn_capture"] = native_spawn.evidence(bundle)
        validate_bundle(bundle)
        self.assertIsNone(spell_registry.scope(bundle, 930001))
        source = starter_source()
        source.rows["playercreateinfo_spell"] = []
        source.rows["playercreateinfo_action"] = []
        source.rows["spell_template"] = []
        selection_json = starter_selection().json()
        selection_json["spell_ids"] = []
        result = capture(source, Selection.parse(selection_json))
        self.assertEqual(result["spell_script_capture"]["selector"]["ids"], [])
        self.assertEqual(result["tables"]["spell_scripts"], [])
        self.assertFalse(any("FROM `spell_scripts`" in sql for sql, _ in source.queries))


class RecursiveModelCaptureTests(unittest.TestCase):
    def test_other_gender_fixed_point_cycles_and_alternatives_are_preserved(self):
        source = starter_source()
        source.rows["creature_model_info"][0].update(modelid_other_gender=940002, modelid_alternative=949999)
        source.rows["creature_model_info"] += [row("creature_model_info", modelid=940002, modelid_other_gender=940003),
                                                row("creature_model_info", modelid=940003, modelid_other_gender=940001)]
        result = capture(source, starter_selection())
        self.assertEqual(result["required_ids"]["creature_model_info"], [940001, 940002, 940003])
        self.assertEqual(result["tables"]["creature_model_info"][0]["modelid_alternative"], 949999)
        self.assertFalse(any(949999 in args for sql, args in source.queries if "FROM `creature_model_info`" in sql))
        source.rows["creature_model_info"].pop()
        with self.assertRaisesRegex(ExportError, "missing_or_unexpected:creature_model_info"):
            capture(source, starter_selection())

    def test_only_actually_selected_spawn_override_model_is_a_root(self):
        source = starter_source()
        source.rows["creature_spawn_data_template"][0]["ModelId"] = 949999
        source.rows["creature_spawn_data"] = [row("creature_spawn_data", Guid=1, Id=12)]
        source.rows["creature_spawn_data_template"].append(row("creature_spawn_data_template", Entry=12, ModelId=940002))
        source.rows["creature_model_info"].append(row("creature_model_info", modelid=940002, modelid_other_gender=940003))
        source.rows["creature_model_info"].append(row("creature_model_info", modelid=940003))
        result = capture(source, starter_selection())
        self.assertEqual(result["required_ids"]["creature_model_info"], [940001, 940002, 940003])
        self.assertEqual(result["tables"]["creature_spawn_data_template"][0]["ModelId"], 949999)


class EventTextCaptureTests(unittest.TestCase):
    """Authored IDs/text exercise the real loader shape without copying world rows."""
    def source(self):
        source = starter_source()
        source.rows["creature_ai_scripts"] = [row("creature_ai_scripts", id=950001,
            creature_id=900001, event_type=4, event_inverse_phase_mask=-1,
            event_chance=0, event_flags=0xffffffff, event_param1=-0x80000000,
            event_param2=0x7fffffff, event_param3=-7, event_param4=11,
            event_param5=13, event_param6=17, action1_type=1,
            action1_param1=960001, action1_param2=960002,
            comment="Original synthetic text dependency case")]
        source.rows["broadcast_text"] = [row("broadcast_text", Id=960001,
            Text="Original α message", Text1=None, LanguageID=7, Flags=1, VerifiedBuild=123),
            row("broadcast_text", Id=960002, Text="Original second message", Text1="")]
        source.rows["broadcast_text_locale"] = [row("broadcast_text_locale", Id=960001,
            Locale="frFR", Text_lang="Original e\u0301 text", Text1_lang=None, VerifiedBuild=123),
            row("broadcast_text_locale", Id=960001, Locale="deDE", Text_lang="", Text1_lang="")]
        return source

    def captured(self, source=None):
        return capture(source or self.source(), starter_selection())

    def rehash(self, bundle, *, dependencies=True):
        from classic_export import event_text, spell_registry, native_spawn
        bundle["schema_sha256"] = digest(bundle["schema"])
        bundle["table_sha256"] = {t: digest(rows) for t, rows in bundle["tables"].items()}
        bundle["spell_script_capture"] = spell_registry.evidence(bundle)
        bundle["native_spawn_capture"] = native_spawn.evidence(bundle)
        if dependencies:
            bundle["event_text_capture"] = event_text.evidence(bundle)

    def test_full_schema_preserves_parameters_text_nulls_and_metadata(self):
        from classic_export import event_text
        source = self.source(); bundle = self.captured(source)
        validate_bundle(bundle)
        self.assertEqual(bundle["tables"]["creature_ai_scripts"], source.rows["creature_ai_scripts"])
        self.assertEqual(bundle["tables"]["broadcast_text"], source.rows["broadcast_text"])
        self.assertEqual(bundle["tables"]["broadcast_text_locale"], sorted(
            source.rows["broadcast_text_locale"], key=lambda r: (r["Id"], r["Locale"])))
        evidence = bundle["event_text_capture"]
        self.assertEqual(evidence["status"], "complete")
        self.assertEqual(evidence["references"], [{"event_id":950001, "creature_id":900001,
            "action_index":1, "parameter_index":i, "broadcast_id":960000+i} for i in (1,2)])
        self.assertEqual(event_text.scope(bundle)["status"], "complete")
        self.assertFalse(bundle["revision_claims_verified_against_installation"])
        self.assertFalse(bundle["report"]["runnable"])
        self.assertFalse(bundle["report"]["capture_complete"])
        self.assertIn("creature.event_ai_matching_row_requires_execution",
                      {finding["reason"] for finding in bundle["report"]["unsupported"]})

    def test_every_text_slot_is_derived_without_chance_phase_debug_filters(self):
        source = self.source()
        event = source.rows["creature_ai_scripts"][0]
        event.update(action1_param3=960001, action2_type=1, action2_param1=960002,
                     action2_param2=0, action2_param3=960001, action3_type=1, action3_param3=960002)
        source.rows["creature_ai_scripts"].append(row("creature_ai_scripts", id=950002,
            creature_id=-1, event_type=255, action3_type=1, action3_param1=960001))
        bundle = self.captured(source)
        refs = bundle["event_text_capture"]["references"]
        self.assertEqual([(r["event_id"],r["action_index"],r["parameter_index"]) for r in refs],
                         [(950001,1,1),(950001,1,2),(950001,1,3),(950001,2,1),
                          (950001,2,3),(950001,3,3),(950002,3,1)])
        self.assertEqual(bundle["event_text_capture"]["tables"]["broadcast_text"]["requested_ids"], [960001,960002])
        for sql, args in source.queries:
            if "FROM `broadcast_text" in sql:
                self.assertEqual(args, (960001, 960002, MAX_ROWS+1))
                self.assertNotIn("960001", sql)
                self.assertNotIn("event_chance", sql)

    def test_negative_text_and_other_actions_stay_unresolved_and_unmodified(self):
        source = self.source()
        source.rows["creature_ai_scripts"][0].update(action1_param3=-19, action2_type=2,
                                                    action2_param1=970001)
        bundle = self.captured(source); evidence = bundle["event_text_capture"]
        self.assertEqual(evidence["status"], "unknown")
        self.assertEqual(evidence["unresolved_negative_text_parameters"][0]["broadcast_id"], -19)
        self.assertEqual(evidence["other_actions_not_resolved"][0]["action_type"], 2)
        self.assertEqual(evidence["tables"]["broadcast_text"]["requested_ids"], [960001,960002])
        self.assertEqual(bundle["tables"]["creature_ai_scripts"], source.rows["creature_ai_scripts"])
        self.assertIn("event_text.other_action_dependencies_not_captured",
                      {r["reason"] for r in bundle["report"]["unsupported"]})

    def test_missing_base_is_unresolved_even_with_locale_and_empty_locale_is_valid(self):
        source = self.source(); source.rows["broadcast_text"] = source.rows["broadcast_text"][1:]
        bundle = self.captured(source)
        self.assertEqual(bundle["event_text_capture"]["status"], "missing")
        self.assertEqual(bundle["event_text_capture"]["tables"]["broadcast_text"]["missing_ids"], [960001])
        self.assertTrue(bundle["tables"]["broadcast_text_locale"])
        self.assertIn("event_text.base_row_missing", {r["reason"] for r in bundle["report"]["unsupported"]})
        source = self.source(); source.rows["broadcast_text_locale"] = []
        bundle = self.captured(source)
        self.assertEqual(bundle["event_text_capture"]["status"], "complete")
        self.assertEqual(bundle["event_text_capture"]["tables"]["broadcast_text_locale"]["returned_keys"], [])
        self.assertTrue(bundle["event_text_capture"]["tables"]["broadcast_text_locale"]["query_performed"])

    def test_missing_tables_are_unknown_including_zero_reference_case(self):
        from classic_export.event_text import scope
        for table in ("broadcast_text", "broadcast_text_locale"):
            for zero in (False, True):
                source = self.source(); source.inventory["tables"].pop(table)
                if zero: source.rows["creature_ai_scripts"] = []
                bundle = self.captured(source)
                self.assertNotIn(table, bundle["tables"])
                evidence = bundle["event_text_capture"]["tables"][table]
                self.assertEqual(evidence["status"], "unknown")
                self.assertIsNone(evidence["returned_keys"])
                self.assertIsNone(evidence["query_sha256"])
                self.assertEqual(scope(bundle)["status"], "unknown")
        self.assertEqual(scope(capture(FakeSource(), selection()))["status"], "unknown")

    def test_zero_references_are_complete_without_invalid_empty_in_query(self):
        source = self.source(); source.rows["creature_ai_scripts"] = []
        bundle = self.captured(source)
        self.assertEqual(bundle["event_text_capture"]["status"], "complete")
        for table in ("broadcast_text", "broadcast_text_locale"):
            evidence = bundle["event_text_capture"]["tables"][table]
            self.assertEqual(evidence["returned_keys"], [])
            self.assertEqual(evidence["requested_ids"], [])
            self.assertFalse(evidence["query_performed"])
            self.assertFalse(any(f"FROM `{table}`" in sql for sql, _ in source.queries))

    def test_exact_reviewed_schema_variants_reject_hybrid_width_null_order_and_keys(self):
        for unsigned in (False, True):
            source = self.source()
            source.inventory["tables"]["creature_ai_scripts"]["columns"][0]["type"] = "int(11)" + (" unsigned" if unsigned else "")
            bundle = self.captured(source)
            self.assertIn("unsigned" if unsigned else "int", bundle["schema"]["tables"]["creature_ai_scripts"]["columns"][0]["type"])
        for table in ("creature_ai_scripts", "broadcast_text", "broadcast_text_locale"):
            mutations = [lambda m: m.update(primary_key=[]), lambda m: m.update(table_type="VIEW"),
                lambda m: m.update(engine="MEMORY"), lambda m: m["columns"][0].update(type="bigint"),
                lambda m: m["columns"][0].update(nullable=True),
                lambda m: m["columns"][0].update(ordinal=500), lambda m: m["columns"].pop()]
            for edit in mutations:
                source = self.source(); edit(source.inventory["tables"][table])
                with self.subTest(table=table, edit=edit), self.assertRaises(ExportError): self.captured(source)
                self.assertFalse(source.started)
        for index in (1,3,6,13):
            source = self.source()
            source.inventory["tables"]["creature_ai_scripts"]["columns"][index]["type"] = "int unsigned"
            with self.assertRaisesRegex(ExportError, "schema.event_text_type"): self.captured(source)
        source = self.source(); source.inventory["tables"]["creature_ai_scripts"]["columns"][4]["type"] = "int"
        with self.assertRaisesRegex(ExportError, "schema.event_text_type"): self.captured(source)

    def test_explicit_locale_aliases_are_preserved_and_arbitrary_case_is_rejected(self):
        source = self.source(); meta = source.inventory["tables"]["broadcast_text_locale"]
        meta["columns"][0]["name"] = "ID"; meta["columns"][1]["name"] = "locale"
        meta["primary_key"] = ["ID","locale"]
        bundle = self.captured(source)
        sql, args = next(q for q in source.queries if "FROM `broadcast_text_locale`" in q[0])
        self.assertIn("`ID` AS `Id`,`locale` AS `Locale`", sql)
        self.assertIn("WHERE `ID` IN (%s,%s) ORDER BY `ID`,`locale`", sql)
        self.assertEqual(bundle["event_text_capture"]["tables"]["broadcast_text_locale"]["source_columns"]["Locale"], "locale")
        for name in ("id", "iD"):
            source = self.source(); source.inventory["tables"]["broadcast_text_locale"]["columns"][0]["name"] = name
            with self.assertRaisesRegex(ExportError, "schema.event_text_spelling"): self.captured(source)
        source = self.source(); source.inventory["tables"]["broadcast_text_locale"]["columns"].append(
            {"name":"locale", "type":"varchar(10)", "nullable":False, "ordinal":6})
        with self.assertRaisesRegex(ExportError, "schema.duplicate_column"): self.captured(source)

    def test_legacy_uncaptured_schema_cannot_be_mistaken_for_full_event_rows(self):
        bundle = capture(FakeSource(), selection())
        meta = bundle["schema"]["tables"]["creature_ai_scripts"]
        meta["columns"] = meta["columns"][:2]
        meta.pop("primary_key")
        bundle["schema_sha256"] = digest(bundle["schema"])
        validate_bundle(bundle)
        source = self.source()
        source.inventory["tables"]["creature_ai_scripts"] = copy.deepcopy(meta)
        with self.assertRaises(ExportError): self.captured(source)

    def test_malformed_inventory_has_controlled_schema_errors_before_read(self):
        for inventory in ({}, {"format_version":1,"tables":None}, None):
            source = self.source(); source.inventory = inventory
            with self.assertRaisesRegex(ExportError,"schema."): self.captured(source)
            self.assertFalse(source.started)

    def test_fixed_query_modes_signed_ids_and_parameterization(self):
        for table in ("broadcast_text", "broadcast_text_locale"):
            sql, args = Query(table, "event_text_selectors", (17,19), limit=23).sql()
            self.assertEqual(args, (17,19,24)); self.assertIn("WHERE `Id` IN (%s,%s)", sql)
            for query in [Query(table,"ids",(17,)), Query(table,"event_text_selectors",()),
                Query(table,"event_text_selectors",(0,)), Query(table,"event_text_selectors",(-1,)),
                Query(table,"event_text_selectors",(0x80000000,)), Query(table,"event_text_selectors",(True,)),
                Query(table,"event_text_selectors",("17 OR 1=1",)), Query(table,"event_text_selectors",(19,17)),
                Query(table,"event_text_selectors",(17,17)), Query(table,"event_text_selectors",(17,), source_names=("bad",))]:
                with self.subTest(query=query), self.assertRaises(ExportError): query.sql()

    def test_duplicate_keys_nullability_ranges_text_bytes_and_lengths_reject(self):
        for table in ("creature_ai_scripts", "broadcast_text", "broadcast_text_locale"):
            for conflict in (False, True):
                source = self.source(); duplicate = copy.deepcopy(source.rows[table][0])
                if conflict: duplicate[COLUMNS[table][-1]] = "different" if table == "creature_ai_scripts" else 29
                source.rows[table].append(duplicate)
                with self.subTest(table=table), self.assertRaisesRegex(ExportError, "duplicate_key"): self.captured(source)
                self.assertTrue(source.finished)
        for table, field, value in [("creature_ai_scripts","event_type",256),
            ("creature_ai_scripts","event_chance",-1), ("creature_ai_scripts","event_param1",0x80000000),
            ("creature_ai_scripts","action1_param1",0x80000000), ("creature_ai_scripts","comment","x"*256),
            ("broadcast_text","Id",0x80000000), ("broadcast_text","Flags",None),
            ("broadcast_text","Text","α"*8193), ("broadcast_text","Text","a\0b"),
            ("broadcast_text_locale","Locale",None), ("broadcast_text_locale","Locale",""),
            ("broadcast_text_locale","Locale","x"*11)]:
            source = self.source(); source.rows[table][0][field] = value
            # Keys outside the selected set are normally omitted by the driver.
            if field == "Id":
                with self.assertRaises(ExportError): normalize_rows(table, source.rows[table], schema_profile(source.inventory,{table})[table],MAX_ROWS)
            else:
                with self.subTest(table=table,field=field), self.assertRaises(ExportError): self.captured(source)

    def test_truncation_and_selector_and_aggregate_byte_budgets_reject_and_rollback(self):
        for table in ("broadcast_text", "broadcast_text_locale"):
            source = self.source(); original = source.fetch
            def overflow(query, table=table):
                return [row(table, Id=960001, **({"Locale":str(i)} if table.endswith("locale") else {}))
                        for i in range(MAX_ROWS+1)] if query.table == table else original(query)
            source.fetch = overflow
            with self.assertRaisesRegex(ExportError, f"rows.limit:{table}"): self.captured(source)
            self.assertTrue(source.finished)
        source = self.source(); source.rows["creature_ai_scripts"] = []
        for index in range(MAX_ROWS//9+1):
            fields = {f"action{a}_type":1 for a in range(1,4)}
            fields.update({f"action{a}_param{p}":1+9*index+3*(a-1)+(p-1) for a in range(1,4) for p in range(1,4)})
            source.rows["creature_ai_scripts"].append(row("creature_ai_scripts", id=index+1, creature_id=-1, **fields))
        with self.assertRaisesRegex(ExportError, "event_text.selector_limit"): self.captured(source)
        self.assertFalse(any("FROM `broadcast_text`" in sql for sql,_ in source.queries))
        source = self.source()
        with mock.patch("classic_export.capture.MAX_BUNDLE_BYTES", len(canonical(source.inventory))+500), self.assertRaisesRegex(ExportError,"aggregate_byte_limit"):
            self.captured(source)
        self.assertTrue(source.finished)

    def test_snapshot_engine_version_and_cleanup_cover_both_new_tables(self):
        for table in ("broadcast_text", "broadcast_text_locale"):
            source = self.source(); source.inventory["tables"][table]["engine"] = "MyISAM"
            with self.assertRaisesRegex(ExportError,"quiescent"): self.captured(source)
            source = self.source(); source.inventory["tables"][table]["engine"] = "MyISAM"
            bundle = capture(source, starter_selection(), quiescent=True)
            self.assertEqual(bundle["event_text_capture"]["snapshot"]["consistency"], "operator_quiescent_snapshot")
            bundle["consistency"] = "transactional_snapshot"; self.rehash(bundle)
            with self.assertRaisesRegex(ExportError,"myisam_consistency"): validate_bundle(bundle)
        source = self.source(); original = source.fetch
        def guarded(query):
            self.assertTrue(source.started); self.assertFalse(source.finished); self.assertEqual(source.versions,1)
            return original(query)
        source.fetch = guarded; self.captured(source); self.assertEqual(source.versions,2)
        source = self.source(); source.mutate_version = True
        with self.assertRaisesRegex(ExportError,"snapshot.version_changed"): self.captured(source)
        self.assertTrue(source.finished)

    def test_missing_evidence_tables_and_identity_only_rows_reject(self):
        edits = [lambda b:b.pop("event_text_capture"),
                 lambda b:b["tables"].pop("broadcast_text"),
                 lambda b:b["tables"].pop("broadcast_text_locale"),
                 lambda b:b["tables"]["creature_ai_scripts"][0].pop("action3_param3")]
        for edit in edits:
            bundle = self.captured(); edit(bundle)
            bundle["table_sha256"] = {t:digest(rows) for t,rows in bundle["tables"].items()}
            with self.assertRaises(ExportError): validate_bundle(bundle)

    def test_outer_rehash_cannot_hide_changed_or_dropped_source_rows(self):
        edits = [lambda b:b["tables"]["broadcast_text"][0].update(Text="modified"),
                 lambda b:b["tables"]["broadcast_text"].pop(),
                 lambda b:b["tables"]["broadcast_text_locale"].pop(),
                 lambda b:b["tables"]["creature_ai_scripts"][0].update(action1_param2=0)]
        for edit in edits:
            bundle = self.captured(); edit(bundle); self.rehash(bundle, dependencies=False)
            with self.assertRaisesRegex(ExportError,"event_text.(coverage_mismatch|unselected_row)"): validate_bundle(bundle)

    def test_all_public_hashes_recomputed_still_reject_semantic_orphans_and_forged_closure(self):
        for table in ("broadcast_text", "broadcast_text_locale"):
            bundle = self.captured(); bundle["tables"][table][-1]["Id"] = 960099
            self.rehash(bundle)
            with self.assertRaisesRegex(ExportError,"event_text.unselected_row"): validate_bundle(bundle)
        bundle = self.captured(); bundle["tables"]["creature_ai_scripts"][0]["action1_param2"] = 0
        self.rehash(bundle)
        with self.assertRaisesRegex(ExportError,"event_text.unselected_row"): validate_bundle(bundle)
        bundle = self.captured(); bundle["tables"]["broadcast_text"].pop(); self.rehash(bundle)
        bundle["event_text_capture"]["status"] = "complete"
        bundle["event_text_capture"]["tables"]["broadcast_text"].update(status="complete", missing_ids=[])
        with self.assertRaisesRegex(ExportError,"event_text.coverage_mismatch"): validate_bundle(bundle)
        for edit in [lambda e:e["references"].pop(),
                     lambda e:e["references"][0].update(parameter_index=3),
                     lambda e:e["tables"]["broadcast_text"].update(requested_ids=[960001]),
                     lambda e:e["tables"]["broadcast_text_locale"].update(returned_keys=[]),
                     lambda e:e["tables"]["broadcast_text_locale"].update(query_performed=False),
                     lambda e:e["tables"]["broadcast_text_locale"].update(row_limit=1)]:
            bundle = self.captured(); self.rehash(bundle); edit(bundle["event_text_capture"])
            with self.assertRaisesRegex(ExportError,"event_text.coverage_mismatch"): validate_bundle(bundle)

    def test_driver_probes_exact_keys_for_event_and_both_broadcast_tables(self):
        instance = LocalSqlSource.__new__(LocalSqlSource); inventory = self.source().inventory
        names = ("creature_ai_scripts", "broadcast_text", "broadcast_text_locale")
        calls = []
        def select(sql, params, limit):
            calls.append((sql,params,limit))
            if "information_schema.TABLES" in sql:
                return [{"name":t,"engine":"InnoDB","table_type":"BASE TABLE"} for t in names]
            if "information_schema.COLUMNS" in sql:
                return [{"table_name":t,**{**c,"nullable":"YES" if c["nullable"] else "NO"}}
                        for t in names for c in inventory["tables"][t]["columns"]]
            self.assertIn("information_schema.KEY_COLUMN_USAGE",sql)
            return [{"name":n,"ordinal":i} for i,n in enumerate(EVENT_TEXT_PRIMARY_KEYS[params[0]],1)]
        instance._select = select; inspected = instance.inspect()
        self.assertEqual({t:m["primary_key"] for t,m in inspected["tables"].items()}, EVENT_TEXT_PRIMARY_KEYS)
        for sql,params,_ in calls:
            self.assertTrue(sql.startswith("SELECT ")); self.assertNotIn(";",sql)
            if "KEY_COLUMN_USAGE" in sql: self.assertEqual(params[1],"PRIMARY")


class BodyLootCaptureTests(unittest.TestCase):
    """Original synthetic rows prove capture boundaries, never loot execution."""
    BODY = "creature_loot_template"
    REFERENCE = "reference_loot_template"

    def source(self):
        source = starter_source()
        creature = source.rows["creature_template"][0]
        creature.update(LootId=970101, KillCredit1=900002, PickpocketLootId=972001,
                        SkinningLootId=973001, MinLootGold=7, MaxLootGold=13)
        source.rows["creature_template"] += [
            dict(creature, Entry=900002, KillCredit1=0, PickpocketLootId=0, SkinningLootId=0),
            dict(creature, Entry=900003, LootId=970102, KillCredit1=0,
                 PickpocketLootId=0, SkinningLootId=0)]
        source.rows["quest_template"][0].update(ReqCreatureOrGOId2=900003, ReqCreatureOrGOCount2=1)
        source.rows[self.BODY] = [
            row(self.BODY, entry=970101, item=0, ChanceOrQuestChance=Decimal("0"),
                groupid=255, mincountOrRef=-971101, maxcount=0,
                condition_id=0xffffff, comments="Original α reference sentinel"),
            row(self.BODY, entry=970101, item=920201, ChanceOrQuestChance=Decimal("-15.5"),
                groupid=1, mincountOrRef=1, maxcount=3, condition_id=12, comments=None),
            row(self.BODY, entry=970102, item=930999, ChanceOrQuestChance=Decimal("100"),
                mincountOrRef=-971101, maxcount=2, comments="Original non-item placeholder")]
        source.rows[self.REFERENCE] = [
            row(self.REFERENCE, entry=971101, item=0, ChanceOrQuestChance=Decimal("100"),
                mincountOrRef=-971102, maxcount=1, comments=None),
            row(self.REFERENCE, entry=971101, item=920202, ChanceOrQuestChance=Decimal("0"),
                groupid=2, mincountOrRef=2, maxcount=4),
            row(self.REFERENCE, entry=971102, item=920203, ChanceOrQuestChance=Decimal("75.25"),
                mincountOrRef=1, maxcount=1)]
        source.rows["item_template"] += [
            row("item_template", entry=920201, name="Original Loot Test First", stackable=3,
                requiredspell=930201, spellid_1=-1, spellid_2=930202),
            row("item_template", entry=920202, name="Original Loot Test Second", stackable=4),
            row("item_template", entry=920203, name="Original Loot Test Third", stackable=5)]
        source.rows["spell_template"] += [row("spell_template", Id=930201, Effect1=2),
            row("spell_template", Id=930202, Effect1=2, EffectTriggerSpell1=930203),
            row("spell_template", Id=930203, Effect1=2)]
        return source

    def captured(self, source=None, **kwargs):
        return capture(source or self.source(), starter_selection(), capture_body_loot=True, **kwargs)

    def rehash(self, bundle, *, dependencies=True):
        from classic_export import event_ai, event_text, loot, native_spawn, spell_registry
        bundle["schema_sha256"] = digest(bundle["schema"])
        bundle["table_sha256"] = {table: digest(rows) for table, rows in bundle["tables"].items()}
        bundle["event_ai_capture"] = event_ai.evidence(bundle["tables"])
        bundle["event_text_capture"] = event_text.evidence(bundle)
        bundle["native_spawn_capture"] = native_spawn.evidence(bundle)
        bundle["spell_script_capture"] = spell_registry.evidence(bundle)
        if dependencies:
            bundle["loot_capture"] = loot.evidence(bundle)

    def test_shared_roots_recursive_references_and_leaf_spell_closure(self):
        from classic_export import loot
        source = self.source(); bundle = self.captured(source)
        validate_bundle(bundle)
        evidence = bundle["loot_capture"]
        self.assertEqual(evidence["format"], loot.FORMAT)
        self.assertEqual(evidence["scope"], "selected_body_loot_dependencies_only")
        self.assertEqual(evidence["requested_mode"], "body_loot_selectors")
        self.assertEqual(evidence["root_ids"], [970101, 970102])
        self.assertEqual(evidence["reference_ids"], [971101, 971102])
        self.assertEqual(evidence["leaf_item_ids"], [920201, 920202, 920203])
        self.assertEqual([(r["Entry"], r["LootId"]) for r in evidence["creatures"]],
                         [(900001,970101), (900002,970101), (900003,970102)])
        self.assertEqual([(r["table"],r["entry"],r["target_id"]) for r in evidence["reference_edges"]],
                         [(self.BODY,970101,971101), (self.BODY,970102,971101),
                          (self.REFERENCE,971101,971102)])
        self.assertTrue({920201,920202,920203} <= set(bundle["required_ids"]["item_template"]))
        self.assertTrue({930201,930202,930203} <= set(bundle["required_ids"]["spell_template"]))
        self.assertNotIn(930999, bundle["required_ids"]["item_template"])
        self.assertNotIn(0, bundle["required_ids"]["item_template"])
        for table in (self.BODY, self.REFERENCE):
            expected = normalize_rows(table, source.rows[table],
                schema_profile(source.inventory, {table})[table], MAX_ROWS)
            self.assertEqual(bundle["tables"][table], expected)
        self.assertTrue(source.finished)

    def test_zero_negative_group_condition_money_and_nonbody_sentinels_are_retained(self):
        bundle = self.captured(); evidence = bundle["loot_capture"]
        sentinel = bundle["tables"][self.BODY][0]
        self.assertEqual(sentinel["item"], 0)
        self.assertEqual(sentinel["mincountOrRef"], -971101)
        self.assertEqual(sentinel["ChanceOrQuestChance"], {"decimal":"0"})
        self.assertEqual((sentinel["groupid"],sentinel["maxcount"],sentinel["condition_id"]),
                         (255,0,0xffffff))
        self.assertEqual(evidence["creatures"][0]["MinLootGold"], 7)
        self.assertEqual(evidence["creatures"][0]["MaxLootGold"], 13)
        self.assertEqual(evidence["non_body_dependencies"], [
            {"creature_entry":900001,"field":"PickpocketLootId","loot_id":972001,
             "status":"unknown","reason":"outside_body_scope"},
            {"creature_entry":900001,"field":"SkinningLootId","loot_id":973001,
             "status":"unknown","reason":"outside_body_scope"}])
        self.assertTrue(evidence["selected_rows_complete"])
        self.assertEqual(evidence["loader_order"], "unproven")
        self.assertEqual(evidence["query_order"], "entry_item_key_order")
        self.assertFalse(bundle["revision_claims_verified_against_installation"])
        self.assertFalse(bundle["report"]["runnable"])
        self.assertFalse(bundle["report"]["capture_complete"])
        reasons = {finding["reason"] for finding in evidence["unsupported"]}
        self.assertTrue({"loot.source_loader_order_unproven", "loot.condition_not_resolved",
            "loot.quest_chance_not_interpreted", "loot.group_domain_unsupported",
            "loot.count_shape_unsupported", "loot.nested_reference_not_executable"} <= reasons)

    def test_six_row_candidate_captures_thirty_two_leaf_items_and_full_spell_metadata(self):
        source = starter_source()
        source.rows["creature_template"][0]["LootId"] = 974101
        leaves = list(range(921001,921033))
        source.rows[self.BODY] = [row(self.BODY,entry=974101,item=item,
            ChanceOrQuestChance=Decimal("17.25"),mincountOrRef=1,maxcount=2,
            comments="Original direct candidate leaf") for item in leaves[:4]] + [
            row(self.BODY,entry=974101,item=0,ChanceOrQuestChance=Decimal("31.5"),
                mincountOrRef=-975101,maxcount=1,comments="Original five-leaf reference"),
            row(self.BODY,entry=974101,item=1,ChanceOrQuestChance=Decimal("63.5"),
                mincountOrRef=-975102,maxcount=2,comments="Original twenty-three-leaf reference")]
        source.rows[self.REFERENCE] = [row(self.REFERENCE,entry=entry,item=item,
            ChanceOrQuestChance=0,groupid=1,mincountOrRef=1,maxcount=1,
            comments="Original referenced candidate leaf")
            for entry,items in ((975101,leaves[4:9]),(975102,leaves[9:])) for item in items]
        metadata = {f"{field}_{slot}": value for slot in range(1,6)
            for field,value in (("spellid",0),("spelltrigger",0),("spellcharges",0),
                ("spellppmRate",0),("spellcooldown",-1),("spellcategory",0),
                ("spellcategorycooldown",-1))}
        authored = [row("item_template",entry=item,name=f"Original Candidate Item {index}",
            stackable=1+index%7,**metadata) for index,item in enumerate(leaves)]
        authored[0].update(spellid_1=931201,spelltrigger_1=0,spellcharges_1=-5,
                           spellcooldown_1=1700,spellcategory_1=17,spellcategorycooldown_1=2300)
        authored[-1].update(spellid_3=931202,spelltrigger_3=1,spellcharges_3=7,
                            spellppmRate_3=3,spellcategory_3=19)
        source.rows["item_template"] += authored
        source.rows["spell_template"] += [row("spell_template",Id=spell,Effect1=2)
                                           for spell in (931201,931202)]
        bundle = self.captured(source)
        serialized = canonical(bundle)
        restored = decode_json(serialized,len(serialized))
        validate_bundle(restored)
        evidence = restored["loot_capture"]
        self.assertEqual(len(restored["tables"][self.BODY]),6)
        self.assertEqual([sum(r["entry"] == entry for r in restored["tables"][self.REFERENCE])
                          for entry in (975101,975102)],[5,23])
        self.assertEqual(evidence["leaf_item_ids"],leaves)
        self.assertEqual(len(evidence["reference_edges"]),2)
        actual = [r for r in restored["tables"]["item_template"] if r["entry"] in leaves]
        self.assertEqual(actual,authored)
        self.assertEqual(set(restored["required_ids"]["item_template"]),
                         {920001,920101,920102,*leaves})
        self.assertEqual(set(restored["required_ids"]["spell_template"]),
                         {930001,930101,930102,931201,931202})
        self.assertEqual(actual[0]["spellcharges_1"],-5)
        self.assertEqual(actual[-1]["spellcharges_3"],7)
        self.assertTrue(all(r["spellcooldown_5"] == -1 and r["spellcategorycooldown_5"] == -1
                            for r in actual))
        self.assertFalse(restored["report"]["runnable"])
        self.assertFalse(restored["report"]["capture_complete"])

    def test_positive_and_zero_mincount_zero_items_are_preserved_without_leaf_queries(self):
        source = self.source()
        source.rows[self.BODY][0].update(mincountOrRef=1, maxcount=1)
        source.rows[self.REFERENCE][0].update(mincountOrRef=0)
        bundle = self.captured(source)
        self.assertEqual(bundle["loot_capture"]["leaf_item_ids"], [920201,920202])
        self.assertEqual(bundle["loot_capture"]["reference_ids"], [971101])
        self.assertEqual(bundle["tables"][self.BODY][0]["item"], 0)
        reasons = {r["reason"] for r in bundle["loot_capture"]["unsupported"]}
        self.assertIn("loot.zero_leaf_item_unsupported", reasons)
        self.assertIn("loot.zero_mincount_unsupported", reasons)
        for sql, args in source.queries:
            if "FROM `item_template`" in sql:
                self.assertNotIn(0, args[:-1])

    def test_long_reference_chain_and_cycle_have_no_sixteen_round_cutoff(self):
        source = self.source()
        ids = list(range(980001, 980026))
        source.rows[self.BODY] = [row(self.BODY, entry=970101, item=0,
            mincountOrRef=-ids[0], maxcount=1, ChanceOrQuestChance=100),
            row(self.BODY, entry=970102, item=0, mincountOrRef=-ids[0], maxcount=1)]
        source.rows[self.REFERENCE] = [row(self.REFERENCE, entry=entry, item=0,
            mincountOrRef=-ids[(index+1) % len(ids)], maxcount=1, ChanceOrQuestChance=100)
            for index, entry in enumerate(ids)]
        source.rows[self.REFERENCE].append(row(self.REFERENCE, entry=ids[-1], item=920203,
            mincountOrRef=1, maxcount=1, ChanceOrQuestChance=100))
        bundle = self.captured(source); evidence = bundle["loot_capture"]
        self.assertEqual(evidence["reference_ids"], ids)
        self.assertEqual(evidence["cyclic_reference_components"], [ids])
        self.assertEqual(evidence["leaf_item_ids"], [920203])
        self.assertEqual(len(evidence["tables"][self.REFERENCE]["queries"]), len(ids))
        self.assertEqual(sum(f"FROM `{self.REFERENCE}`" in sql for sql,_ in source.queries), len(ids))
        self.assertTrue(evidence["selected_rows_complete"])
        self.assertIn("loot.reference_cycle_not_executable", {r["reason"] for r in evidence["unsupported"]})

    def test_self_reference_is_visited_once(self):
        source = self.source()
        source.rows[self.REFERENCE][0]["mincountOrRef"] = -971101
        bundle = self.captured(source)
        self.assertEqual(bundle["loot_capture"]["cyclic_reference_components"], [[971101]])
        self.assertEqual(len(bundle["loot_capture"]["tables"][self.REFERENCE]["queries"]), 1)

    def test_missing_tables_and_missing_selected_ids_remain_distinct(self):
        for table in (self.BODY, self.REFERENCE):
            source = self.source(); source.inventory["tables"].pop(table)
            bundle = self.captured(source); record = bundle["loot_capture"]["tables"][table]
            self.assertNotIn(table, bundle["tables"])
            self.assertEqual(record["status"], "unknown")
            self.assertIsNone(record["missing_ids"])
            self.assertFalse(bundle["loot_capture"]["selected_rows_complete"])
            self.assertTrue(all(not query["query_performed"] and query["rows_sha256"] is None
                                for query in record["queries"]))
            self.assertFalse(any(f"FROM `{table}`" in sql for sql,_ in source.queries))
            source = self.source(); source.rows[table] = []
            bundle = self.captured(source); record = bundle["loot_capture"]["tables"][table]
            self.assertEqual(record["status"], "missing")
            self.assertEqual(record["missing_ids"], [970101,970102] if table == self.BODY else [971101])
            self.assertTrue(record["queries"][0]["query_performed"])
            self.assertEqual(record["queries"][0]["returned_keys"], [])
            self.assertFalse(bundle["loot_capture"]["selected_rows_complete"])

    def test_no_roots_never_queries_empty_in_and_absent_schema_stays_unknown(self):
        for missing in (None, self.BODY, self.REFERENCE):
            source = self.source()
            for creature in source.rows["creature_template"]: creature["LootId"] = 0
            if missing: source.inventory["tables"].pop(missing)
            bundle = self.captured(source); evidence = bundle["loot_capture"]
            self.assertEqual(evidence["root_ids"], [])
            self.assertEqual(evidence["reference_ids"], [])
            self.assertEqual(evidence["leaf_item_ids"], [])
            self.assertEqual(evidence["selected_rows_complete"], missing is None)
            for table in (self.BODY, self.REFERENCE):
                record = evidence["tables"][table]
                self.assertEqual(record["status"], "unknown" if table == missing else "not_needed")
                self.assertEqual(record["requested_ids"], [])
                self.assertFalse(record["queries"][0]["query_performed"])
                self.assertIsNone(record["queries"][0]["query_sha256"])
                self.assertFalse(any(f"FROM `{table}`" in sql for sql,_ in source.queries))

    def test_missing_leaf_item_and_its_spell_dependencies_fail_and_cleanup(self):
        for table, key, missing in [("item_template","entry",920203),
                                    ("spell_template","Id",930201),
                                    ("spell_template","Id",930203)]:
            source = self.source()
            source.rows[table] = [r for r in source.rows[table] if r[key] != missing]
            with self.subTest(table=table,missing=missing), self.assertRaisesRegex(ExportError, "reference.missing"):
                self.captured(source)
            self.assertTrue(source.finished)

    def test_explicit_opt_in_requires_boolean_and_starter_and_legacy_remains_valid(self):
        for selected, flag in [(selection(),True), (starter_selection(),1),
                               (starter_selection(),"true"), (starter_selection(),None)]:
            source = self.source()
            with self.subTest(flag=flag), self.assertRaisesRegex(ExportError,"loot."):
                capture(source, selected, capture_body_loot=flag)
            self.assertFalse(source.started)
            self.assertEqual(source.queries, [])
        for selected in (selection(), starter_selection()):
            source = self.source(); bundle = capture(source, selected)
            self.assertNotIn("loot_capture", bundle)
            self.assertFalse({self.BODY,self.REFERENCE} & set(bundle["tables"]))
            validate_bundle(bundle)

    def test_cli_body_loot_plan_rejects_malformed_schema_without_traceback(self):
        import subprocess
        import sys
        path = Path(__file__).with_name("classic-export.py")
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            selected = directory/"selection.json"
            inventory = directory/"schema.json"
            output = directory/"plan.json"
            selected.write_bytes(canonical(starter_selection().json()))
            for value in ([],None,{"format_version":1,"tables":None},
                          {"format_version":1,"tables":[]}):
                inventory.write_bytes(canonical(value))
                completed = subprocess.run([sys.executable,str(path),"plan","--body-loot",
                    "--schema",str(inventory),"--selection",str(selected),"--output",str(output)],
                    capture_output=True,text=True,check=False)
                with self.subTest(schema=value):
                    self.assertEqual(completed.returncode,2)
                    self.assertEqual(completed.stderr.strip(),"schema.tables")
                    self.assertNotIn("Traceback",completed.stderr)
                    self.assertEqual(completed.stdout,"")
                    self.assertFalse(output.exists())

    def test_exact_schema_width_signedness_nullable_order_and_primary_key(self):
        expected = [("entry","mediumint unsigned",False), ("item","mediumint unsigned",False),
            ("ChanceOrQuestChance","float",False), ("groupid","tinyint unsigned",False),
            ("mincountOrRef","mediumint",False), ("maxcount","tinyint unsigned",False),
            ("condition_id","mediumint unsigned",False), ("comments","varchar(300)",True)]
        for table in (self.BODY, self.REFERENCE):
            self.assertEqual(LOOT_SCHEMA[table], expected)
            self.assertEqual(LOOT_PRIMARY_KEYS[table], ["entry","item"])
            edits = [lambda m:m.pop("primary_key"), lambda m:m.update(primary_key=["entry"]),
                lambda m:m.update(primary_key=["item","entry"]), lambda m:m.update(table_type="VIEW"),
                lambda m:m.update(engine="MEMORY"), lambda m:m["columns"].pop(),
                lambda m:m["columns"][0].update(type="mediumint"),
                lambda m:m["columns"][1].update(type="int unsigned"),
                lambda m:m["columns"][2].update(type="double"),
                lambda m:m["columns"][3].update(type="tinyint"),
                lambda m:m["columns"][4].update(type="mediumint unsigned"),
                lambda m:m["columns"][5].update(type="smallint unsigned"),
                lambda m:m["columns"][6].update(type="mediumint"),
                lambda m:m["columns"][7].update(type="varchar(301)"),
                lambda m:m["columns"][7].update(nullable=False),
                lambda m:m["columns"][2].update(nullable=True),
                lambda m:m["columns"][0].update(ordinal=50),
                lambda m:m["columns"].append({"name":"unexpected","type":"int","nullable":False,"ordinal":9})]
            for edit in edits:
                source = self.source(); edit(source.inventory["tables"][table])
                with self.subTest(table=table,edit=edit), self.assertRaises(ExportError): self.captured(source)
                self.assertFalse(source.started)
            source = self.source()
            source.inventory["tables"][table]["columns"][0]["type"] = "mediumint(8) unsigned"
            source.inventory["tables"][table]["columns"][4]["type"] = "mediumint(9)"
            validate_bundle(self.captured(source))

    def test_signed_reference_boundaries_and_unsigned_cell_domains(self):
        for table in (self.BODY, self.REFERENCE):
            profile = schema_profile(schema(), {table})[table]
            for value in (-0x800000, 0x7fffff):
                values = normalize_rows(table, [row(table, entry=0xffffff, item=0,
                    mincountOrRef=value, maxcount=255, groupid=255, condition_id=0xffffff,
                    ChanceOrQuestChance=Decimal("-100"), comments=None)], profile, MAX_ROWS)
                self.assertEqual(values[0]["mincountOrRef"], value)
            for field,value in [("entry",0), ("entry",0x1000000), ("item",-1),
                ("item",0x1000000), ("mincountOrRef",-0x800001), ("mincountOrRef",0x800000),
                ("groupid",256), ("maxcount",-1), ("maxcount",256), ("condition_id",-1),
                ("condition_id",0x1000000), ("comments","x"*301), ("comments","bad\0text"),
                ("ChanceOrQuestChance",float("inf")), ("ChanceOrQuestChance",None)]:
                value_row = row(table, entry=970101, item=0, mincountOrRef=1, maxcount=1)
                value_row[field] = value
                with self.subTest(table=table,field=field,value=value), self.assertRaises(ExportError):
                    normalize_rows(table, [value_row], profile, MAX_ROWS)
        source = self.source(); source.rows[self.BODY][0]["mincountOrRef"] = -0x800000
        source.rows[self.BODY][2]["mincountOrRef"] = -0x800000
        source.rows[self.REFERENCE] = [row(self.REFERENCE, entry=0x800000, item=920203,
            mincountOrRef=1, maxcount=1, ChanceOrQuestChance=100)]
        self.assertEqual(self.captured(source)["loot_capture"]["reference_ids"], [0x800000])

    def test_fixed_parameterized_queries_reject_wrong_modes_and_selector_domains(self):
        from classic_export import loot
        for table in (self.BODY, self.REFERENCE):
            sql,args = Query(table,loot.MODE,(970101,970102),limit=23).sql()
            self.assertEqual(args,(970101,970102,24))
            self.assertIn("WHERE `entry` IN (%s,%s) ORDER BY `entry`,`item` LIMIT %s",sql)
            self.assertNotIn("970101",sql)
            invalid = [Query(table,"ids",(1,)), Query(table,"event_text_selectors",(1,)),
                Query("item_template",loot.MODE,(1,)), Query(table,loot.MODE,()),
                Query(table,loot.MODE,(0,)), Query(table,loot.MODE,(-1,)),
                Query(table,loot.MODE,(0x1000000,)), Query(table,loot.MODE,(True,)),
                Query(table,loot.MODE,("1 OR 1=1",)), Query(table,loot.MODE,(2,1)),
                Query(table,loot.MODE,(1,1)), Query(table,loot.MODE,(1,),selection=starter_selection()),
                Query(table,loot.MODE,(1,),limit=0), Query(table,loot.MODE,(1,),limit=MAX_ROWS+1),
                Query(table,loot.MODE,(1,),source_names=("bad",)),
                Query(table,loot.MODE,tuple(range(1,MAX_ROWS+2)))]
            for query in invalid:
                with self.subTest(query=query), self.assertRaises(ExportError): query.sql()
            Query(table,loot.MODE,(0xffffff,)).sql()

    def test_input_order_is_normalized_and_query_evidence_matches_actual_batches(self):
        first_source = self.source(); first = self.captured(first_source)
        second_source = self.source()
        for rows in second_source.rows.values(): rows.reverse()
        second = self.captured(second_source)
        self.assertEqual(canonical(first), canonical(second))
        self.assertEqual(first_source.queries, second_source.queries)
        for table in (self.BODY,self.REFERENCE):
            actual = [query for query in first_source.queries if f"FROM `{table}`" in query[0]]
            queries = first["loot_capture"]["tables"][table]["queries"]
            self.assertEqual([record["query_sha256"] for record in queries], [digest(query) for query in actual])
            self.assertEqual([record["requested_ids"] for record in queries], [list(args[:-1]) for _,args in actual])
            self.assertTrue(all(record["query_performed"] for record in queries))
            self.assertTrue(all(args[-1] == MAX_ROWS+1 for _,args in actual))

    def test_duplicate_keys_and_unselected_source_rows_reject_and_cleanup(self):
        for table in (self.BODY,self.REFERENCE):
            for conflicting in (False,True):
                source = self.source(); duplicate = copy.deepcopy(source.rows[table][0])
                if conflicting: duplicate["comments"] = "Original conflicting value"
                source.rows[table].append(duplicate)
                with self.subTest(table=table), self.assertRaisesRegex(ExportError,"duplicate_key"): self.captured(source)
                self.assertTrue(source.finished)
            source = self.source(); original = source.fetch
            def extra(query,table=table):
                rows = original(query)
                return rows + [row(table,entry=979999,item=0,mincountOrRef=0)] if query.table == table else rows
            source.fetch = extra
            with self.assertRaisesRegex(ExportError,"loot.unselected_row"): self.captured(source)
            self.assertTrue(source.finished)

    def test_each_query_max_plus_one_rejects_without_partial_bundle(self):
        for table,entry in ((self.BODY,970101),(self.REFERENCE,971101)):
            source = self.source(); original = source.fetch
            def overflow(query,table=table,entry=entry):
                if query.table == table:
                    return [row(table,entry=entry,item=i,mincountOrRef=0) for i in range(MAX_ROWS+1)]
                return original(query)
            source.fetch = overflow
            with self.subTest(table=table), self.assertRaisesRegex(ExportError,f"rows.limit:{table}"):
                self.captured(source)
            self.assertTrue(source.finished)

    def test_second_reference_batch_cumulative_rows_and_bytes_are_bounded(self):
        source = self.source()
        source.rows[self.REFERENCE] = [row(self.REFERENCE,entry=971101,item=i,
            mincountOrRef=-971102,maxcount=1) for i in range(MAX_ROWS)]
        source.rows[self.REFERENCE].append(row(self.REFERENCE,entry=971102,item=0,mincountOrRef=0))
        with self.assertRaisesRegex(ExportError,"loot.accumulated_row_limit"): self.captured(source)
        self.assertTrue(source.finished)
        self.assertEqual(sum(f"FROM `{self.REFERENCE}`" in sql for sql,_ in source.queries),2)
        source = self.source()
        profile = schema_profile(source.inventory,{self.REFERENCE})[self.REFERENCE]
        normalized = normalize_rows(self.REFERENCE,source.rows[self.REFERENCE],profile,MAX_ROWS)
        first_bytes = sum(len(canonical(r)) for r in normalized if r["entry"] == 971101)
        with mock.patch("classic_export.loot.MAX_TABLE_BYTES",first_bytes), self.assertRaisesRegex(
                ExportError,"loot.accumulated_table_byte_limit"):
            self.captured(source)
        self.assertTrue(source.finished)
        self.assertEqual(sum(f"FROM `{self.REFERENCE}`" in sql for sql,_ in source.queries),2)

    def test_reference_selector_and_item_closure_budgets_reject_before_next_query(self):
        source = self.source()
        source.rows[self.REFERENCE] = [row(self.REFERENCE,entry=971101,item=i,
            mincountOrRef=-(980000+i),maxcount=1) for i in range(MAX_ROWS)]
        with self.assertRaisesRegex(ExportError,"loot.reference_id_limit"): self.captured(source)
        self.assertTrue(source.finished)
        self.assertEqual(sum(f"FROM `{self.REFERENCE}`" in sql for sql,_ in source.queries),1)
        from classic_export.core import MAX_ITEMS
        source = self.source()
        source.rows[self.BODY] = [row(self.BODY,entry=970101,item=1000000+i,
            mincountOrRef=1,maxcount=1) for i in range(MAX_ITEMS+1)]
        with self.assertRaisesRegex(ExportError,"loot.item_limit"): self.captured(source)
        self.assertTrue(source.finished)
        self.assertFalse(any("FROM `item_template`" in sql for sql,_ in source.queries))

    def test_item_budget_applies_to_union_of_loot_quest_starter_and_explicit_requests(self):
        request = LearningExportTests().request()
        request.update(spell_ids=[],item_ids=[920201])
        with mock.patch("classic_export.capture.MAX_ITEMS",6):
            bundle = self.captured(dependency_request=request)
        self.assertEqual(bundle["required_ids"]["item_template"],
                         [920001,920101,920102,920201,920202,920203])
        self.assertEqual(bundle["learning_capture"]["dependency_request"],request)
        # Each contributor fits individually, but their distinct-ID union does not.
        for limit,explicit in ((5,[]),(6,[920301])):
            source = self.source(); request["item_ids"] = explicit
            with mock.patch("classic_export.capture.MAX_ITEMS",limit), self.assertRaisesRegex(
                    ExportError,"closure.limit:item_template"):
                self.captured(source,dependency_request=request)
            self.assertTrue(source.finished)
            self.assertFalse(any("FROM `item_template`" in sql for sql,_ in source.queries))

    def test_existing_row_byte_cap_applies_to_both_loot_tables(self):
        for table in (self.BODY,self.REFERENCE):
            source = self.source(); original = source.fetch
            profile = schema_profile(source.inventory,{table})[table]
            rows = normalize_rows(table,source.rows[table],profile,MAX_ROWS)
            cap = len(canonical(rows[0]))-1
            # Earlier wide creature/item records retain their ordinary allowance.
            # Only lower the shared bound once the selected loot response arrives.
            with mock.patch("classic_export.core.MAX_ROW_BYTES",65536):
                def bounded(query,table=table):
                    values = original(query)
                    if query.table == table:
                        import classic_export.core as core
                        core.MAX_ROW_BYTES = cap
                    return values
                source.fetch = bounded
                with self.subTest(table=table), self.assertRaisesRegex(ExportError,f"row.byte_limit:{table}"):
                    self.captured(source)
            self.assertTrue(source.finished)

    def test_aggregate_byte_budget_includes_loot_rows(self):
        source = self.source(); original = source.fetch
        # Measure the first loot response in the ordinary shared accounting path.
        fetched = len(canonical(source.inventory))
        first_loot_total = None
        def measured(query):
            nonlocal fetched, first_loot_total
            rows = original(query)
            profile = schema_profile(source.inventory,{query.table})[query.table]
            fetched += len(canonical(normalize_rows(query.table,rows,profile,query.limit)))
            if query.table == self.BODY: first_loot_total = fetched
            return rows
        source.fetch = measured; self.captured(source)
        self.assertIsNotNone(first_loot_total)
        source = self.source()
        with mock.patch("classic_export.capture.MAX_BUNDLE_BYTES",first_loot_total-1), self.assertRaisesRegex(
                ExportError,"aggregate_byte_limit"):
            self.captured(source)
        self.assertTrue(source.finished)
        self.assertIn(f"FROM `{self.BODY}`", source.queries[-1][0])
        self.assertFalse(any(f"FROM `{self.REFERENCE}`" in sql for sql,_ in source.queries))

    def test_snapshot_engines_version_drift_and_fetch_errors_always_cleanup(self):
        for table in (self.BODY,self.REFERENCE):
            source = self.source(); source.inventory["tables"][table]["engine"] = "MyISAM"
            with self.assertRaisesRegex(ExportError,"quiescent"): self.captured(source)
            self.assertTrue(source.finished)
            source = self.source(); source.inventory["tables"][table]["engine"] = "MyISAM"
            bundle = self.captured(source,quiescent=True)
            self.assertEqual(bundle["loot_capture"]["snapshot"]["consistency"],"operator_quiescent_snapshot")
            source = self.source(); original = source.fetch
            def failed(query,table=table):
                if query.table == table: raise ExportError("original.test_read_failure")
                return original(query)
            source.fetch = failed
            with self.assertRaisesRegex(ExportError,"original.test_read_failure"): self.captured(source)
            self.assertTrue(source.finished)
        source = self.source(); original = source.fetch
        def guarded(query):
            self.assertTrue(source.started); self.assertFalse(source.finished); self.assertEqual(source.versions,1)
            return original(query)
        source.fetch = guarded; self.captured(source); self.assertEqual(source.versions,2)
        source = self.source(); source.mutate_version = True
        with self.assertRaisesRegex(ExportError,"snapshot.version_changed"): self.captured(source)
        self.assertTrue(source.finished)

    def test_required_evidence_tables_and_starter_scope_cannot_be_stripped(self):
        for edit in [lambda b:b.pop("loot_capture"), lambda b:b["tables"].pop(self.BODY),
                     lambda b:b["tables"].pop(self.REFERENCE),
                     lambda b:b["loot_capture"]["snapshot"].update(consistency="operator_quiescent_snapshot")]:
            bundle = self.captured(); edit(bundle)
            bundle["table_sha256"] = {t:digest(rows) for t,rows in bundle["tables"].items()}
            with self.assertRaises(ExportError): validate_bundle(bundle)
        bundle = capture(FakeSource(),selection())
        bundle["loot_capture"] = self.captured()["loot_capture"]
        with self.assertRaises(ExportError): validate_bundle(bundle)

    def test_outer_rehash_does_not_hide_source_mapping_rows_or_query_tampering(self):
        edits = [lambda b:b["tables"][self.BODY][0].update(comments="Modified raw provenance"),
            lambda b:b["tables"][self.REFERENCE].pop(),
            lambda b:b["tables"]["creature_template"][0].update(LootId=970102),
            lambda b:b["tables"]["creature_template"][0].update(MinLootGold=8),
            lambda b:b["loot_capture"].update(loader_order="proven"),
            lambda b:b["loot_capture"].update(selected_rows_complete=False),
            lambda b:b["loot_capture"]["root_ids"].pop(),
            lambda b:b["loot_capture"]["reference_ids"].pop(),
            lambda b:b["loot_capture"]["reference_edges"].pop(),
            lambda b:b["loot_capture"]["leaf_item_ids"].pop(),
            lambda b:b["loot_capture"]["non_body_dependencies"].pop(),
            lambda b:b["loot_capture"]["tables"][self.REFERENCE]["queries"].pop(),
            lambda b:b["loot_capture"]["tables"][self.BODY]["queries"][0].update(query_performed=False),
            lambda b:b["loot_capture"]["tables"][self.BODY]["queries"][0].update(row_limit=1),
            lambda b:b["loot_capture"]["tables"][self.BODY]["queries"][0].update(returned_keys=[])]
        for edit in edits:
            bundle = self.captured(); edit(bundle); self.rehash(bundle,dependencies=False)
            with self.subTest(edit=edit), self.assertRaisesRegex(ExportError,"loot."): validate_bundle(bundle)

    def test_recomputed_evidence_cannot_hide_orphan_rows_or_missing_leaf_dependency(self):
        for table in (self.BODY,self.REFERENCE):
            bundle = self.captured(); bundle["tables"][table][-1]["entry"] = 979999
            self.rehash(bundle,dependencies=False)
            # Recompute all affected row hashes without calling the checked graph
            # builder: it is offline validation itself that must reject the orphan.
            evidence = bundle["loot_capture"]
            evidence["snapshot"]["table_sha256"][table] = digest(bundle["tables"][table])
            query = evidence["tables"][table]["queries"][-1]
            selected_rows = bundle["tables"][table] if table == self.BODY else bundle["tables"][table][-1:]
            query["rows_sha256"] = digest(selected_rows)
            query["returned_keys"] = [[r["entry"],r["item"]] for r in selected_rows]
            with self.subTest(table=table), self.assertRaisesRegex(ExportError,"loot.unselected_row"):
                validate_bundle(bundle)
        bundle = self.captured()
        bundle["tables"][self.REFERENCE][0]["mincountOrRef"] = 0
        self.rehash(bundle,dependencies=False)
        bundle["loot_capture"]["snapshot"]["table_sha256"][self.REFERENCE] = digest(bundle["tables"][self.REFERENCE])
        bundle["loot_capture"]["tables"][self.REFERENCE]["queries"][0]["rows_sha256"] = digest(
            bundle["tables"][self.REFERENCE][:2])
        with self.assertRaisesRegex(ExportError,"loot.unselected_row"): validate_bundle(bundle)
        bundle = self.captured()
        bundle["tables"]["item_template"] = [r for r in bundle["tables"]["item_template"] if r["entry"] != 920203]
        bundle["required_ids"]["item_template"].remove(920203)
        self.rehash(bundle)
        with self.assertRaisesRegex(ExportError,"loot.leaf_item_missing"): validate_bundle(bundle)
        bundle = self.captured(); bundle["tables"][self.REFERENCE].pop(); self.rehash(bundle)
        bundle["loot_capture"]["selected_rows_complete"] = True
        bundle["loot_capture"]["tables"][self.REFERENCE].update(status="complete",missing_ids=[])
        with self.assertRaisesRegex(ExportError,"loot.coverage_mismatch"): validate_bundle(bundle)

    def test_driver_inspects_both_loot_primary_keys_with_readonly_queries(self):
        instance = LocalSqlSource.__new__(LocalSqlSource); inventory = self.source().inventory
        names = (self.BODY,self.REFERENCE); calls = []
        def select(sql,params,limit):
            calls.append((sql,params,limit))
            if "information_schema.TABLES" in sql:
                return [{"name":t,"engine":"InnoDB","table_type":"BASE TABLE"} for t in names]
            if "information_schema.COLUMNS" in sql:
                return [{"table_name":t,**{**c,"nullable":"YES" if c["nullable"] else "NO"}}
                        for t in names for c in inventory["tables"][t]["columns"]]
            self.assertIn("information_schema.KEY_COLUMN_USAGE",sql)
            self.assertEqual(params[1],"PRIMARY")
            return [{"name":n,"ordinal":i} for i,n in enumerate(LOOT_PRIMARY_KEYS[params[0]],1)]
        instance._select = select; inspected = instance.inspect()
        self.assertEqual({t:m["primary_key"] for t,m in inspected["tables"].items()},LOOT_PRIMARY_KEYS)
        self.assertTrue(all(sql.startswith("SELECT ") and ";" not in sql for sql,_,_ in calls))


class QuestRelationshipTests(unittest.TestCase):
    def capture(self, source=None):
        return capture(source or FakeSource(), selection(), capture_quest_relationships=True)

    def test_complete_empty_and_legacy_unknown_are_distinct(self):
        old = capture(FakeSource(), selection())
        self.assertNotIn("quest_relationship_capture", old)
        validate_bundle(old)
        new = self.capture()
        self.assertEqual(new["quest_relationship_capture"]["inbound"], [])
        self.assertEqual(new["quest_relationship_capture"]["status"], "complete")
        self.assertFalse(new["report"]["runnable"])
        self.assertFalse(new["report"]["capture_complete"])

    def test_each_external_inbound_edge_is_preserved_without_dependency_import(self):
        for field, value in [("NextQuestId", 910001), ("NextQuestId", -910001),
                             ("NextQuestInChain", 910001), ("BreadcrumbForQuestId", 910001)]:
            with self.subTest(field=field, value=value):
                source = FakeSource()
                source.rows["quest_template"].append(row("quest_template", entry=910099,
                                                        ReqItemId1=999999, **{field: value}))
                bundle = self.capture(source)
                self.assertEqual([r["entry"] for r in bundle["tables"]["quest_template"]], [910001])
                self.assertEqual(bundle["quest_relationship_capture"]["inbound"][0][field], value)
                self.assertIn("quest.inbound_relationship", [r["reason"] for r in bundle["report"]["unsupported"]])
                validate_bundle(bundle)

    def test_multiple_edges_preserved_once_and_diagnosed_separately(self):
        source = FakeSource()
        source.rows["quest_template"].append(row("quest_template", entry=910099,
            NextQuestId=-910001, NextQuestInChain=910001, BreadcrumbForQuestId=910001))
        bundle = self.capture(source)
        self.assertEqual(len(bundle["quest_relationship_capture"]["inbound"]), 1)
        self.assertEqual(len(quest_relationships.blockers(bundle)), 3)

    def test_raw_own_relationships_not_normalized_or_admitted(self):
        for field, value in [("PrevQuestId", -999999), ("NextQuestId", 999999),
                             ("NextQuestInChain", 999999), ("BreadcrumbForQuestId", 999999),
                             ("ExclusiveGroup", 5), ("ExclusiveGroup", -5)]:
            with self.subTest(field=field, value=value):
                source = FakeSource(); source.rows["quest_template"][0][field] = value
                bundle = self.capture(source)
                self.assertEqual(bundle["quest_relationship_capture"]["own"][0][field], value)
                self.assertIn("quest.chain", [r["reason"] for r in bundle["report"]["unsupported"]])

    def test_self_link_cannot_be_omitted_from_inbound_evidence(self):
        source = FakeSource(); source.rows["quest_template"][0]["NextQuestId"] = -910001
        bundle = self.capture(source)
        bundle["quest_relationship_capture"]["inbound"] = []
        with self.assertRaisesRegex(ExportError, "missing_root_edge"):
            validate_bundle(bundle)

    def test_breadcrumb_cycle_is_raw_blocked_not_traversed(self):
        source = FakeSource(); source.rows["quest_template"][0]["BreadcrumbForQuestId"] = 910099
        source.rows["quest_template"].extend([
            row("quest_template", entry=910099, BreadcrumbForQuestId=910001),
            row("quest_template", entry=910098, BreadcrumbForQuestId=910099)])
        bundle = self.capture(source)
        self.assertEqual([r["entry"] for r in bundle["quest_relationship_capture"]["inbound"]], [910099])
        self.assertTrue(quest_relationships.blockers(bundle))

    def test_fixed_sql_is_signed_bounded_and_allowlisted(self):
        sql, args = Query("quest_template", quest_relationships.MODE, (4, 7)).sql()
        self.assertIn("`NextQuestId` IN (%s,%s,%s,%s)", sql)
        self.assertEqual(args, (-7, -4, 4, 7, 4, 7, 4, 7, MAX_ROWS+1))
        self.assertNotIn("ABS", sql.upper())
        for table, ids in [("creature", (1,)), ("quest_template", (True,)),
                           ("quest_template", (0x80000000,)), ("quest_template", (2, 1)),
                           ("quest_template", (1, 1))]:
            with self.subTest(table=table, ids=ids), self.assertRaises(ExportError):
                Query(table, quest_relationships.MODE, ids).sql()

    def test_numeric_domains_fail_closed(self):
        for field, value in [("PrevQuestId", -0x80000000), ("NextQuestId", True),
                             ("NextQuestInChain", -1), ("BreadcrumbForQuestId", 0x80000000)]:
            source = FakeSource(); source.rows["quest_template"][0][field] = value
            with self.subTest(field=field), self.assertRaises(ExportError):
                self.capture(source)

    def test_request_is_boolean(self):
        with self.assertRaisesRegex(ExportError, "request_boolean"):
            capture(FakeSource(), selection(), capture_quest_relationships=1)

    def test_tampered_bindings_or_rows_fail(self):
        original = self.capture()
        for field, value in [("roots", []), ("status", "partial"), ("row_limit", 1),
                             ("selectors", {}), ("own_sha256", "bad"), ("snapshot", {})]:
            bundle = copy.deepcopy(original); bundle["quest_relationship_capture"][field] = value
            with self.subTest(field=field), self.assertRaises(ExportError):
                validate_bundle(bundle)
        bundle = copy.deepcopy(original)
        bundle["quest_relationship_capture"]["inbound"] = [
            {field: (910099 if field == "entry" else 0) for field in quest_relationships.FIELDS}]
        with self.assertRaisesRegex(ExportError, "unselected_row"):
            validate_bundle(bundle)

    def test_duplicate_and_overflow_returned_rows_fail(self):
        class InvalidSource(FakeSource):
            def fetch(self, query):
                if query.mode == quest_relationships.MODE:
                    return [row("quest_template", entry=910099, NextQuestId=910001)] * self.count
                return super().fetch(query)
        for count, error in [(2, "duplicate_key"), (MAX_ROWS+1, "rows.limit")]:
            source = InvalidSource(); source.count = count
            with self.subTest(count=count), self.assertRaisesRegex(ExportError, error):
                self.capture(source)

    def test_missing_schema_and_failed_scan_are_not_complete_empty(self):
        source = FakeSource()
        source.inventory["tables"].pop("quest_template")
        with self.assertRaises(ExportError):
            self.capture(source)
        class FailedSource(FakeSource):
            def fetch(self, query):
                if query.mode == quest_relationships.MODE:
                    raise RuntimeError("interrupted raw scan")
                return super().fetch(query)
        with self.assertRaisesRegex(RuntimeError, "interrupted raw scan"):
            self.capture(FailedSource())

    def test_nullable_decimal_or_unsigned_signed_field_is_unknown(self):
        for field, changes in [("NextQuestId", {"type": "bigint unsigned"}),
                               ("NextQuestInChain", {"nullable": True}),
                               ("BreadcrumbForQuestId", {"type": "decimal(20,0)"})]:
            source = FakeSource()
            columns = source.inventory["tables"]["quest_template"]["columns"]
            next(c for c in columns if c["name"] == field).update(changes)
            with self.subTest(field=field), self.assertRaisesRegex(ExportError, "schema_domain"):
                self.capture(source)

    def test_required_inbound_blocker_cannot_be_removed(self):
        source = FakeSource()
        source.rows["quest_template"].append(row("quest_template", entry=910099, NextQuestId=910001))
        bundle = self.capture(source)
        bundle["report"]["unsupported"] = [r for r in bundle["report"]["unsupported"]
                                            if r["reason"] != "quest.inbound_relationship"]
        with self.assertRaisesRegex(ExportError, "missing_or_inconsistent_blocker"):
            validate_bundle(bundle)


if __name__ == "__main__":
    unittest.main()
