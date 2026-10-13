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
from classic_export.columns import COLUMNS, STARTER_SCHEMA
from classic_export import starter
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
            if table == "creature_ai_scripts":
                kind = "int unsigned" if name == "id" else "int"
            columns.append({"name": name, "type": kind, "nullable": False, "ordinal": ordinal})
        tables[table] = {"engine": "InnoDB", "table_type": "BASE TABLE", "columns": columns}
    return {"format_version": 1, "tables": tables}

def row(table, **changes):
    result = {name: ("" if name in TEXT_COLUMNS.get(table, set()) else 0) for name in COLUMNS[table]}
    for name in FLOAT_FIELDS & set(result):
        result[name] = Decimal("0")
    result.update(changes)
    return result

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
            {"id": 950001, "creature_id": 900001},
            {"id": 950002, "creature_id": -1},
            {"id": 950003, "creature_id": -2},
            {"id": 950004, "creature_id": 900002},
        ]
        bundle = capture(source, starter_selection())
        self.assertEqual(bundle["event_ai_capture"]["selectors"], [-1, 900001])
        self.assertEqual(bundle["tables"]["creature_ai_scripts"], source.rows["creature_ai_scripts"][:2])
        sql, args = next(q for q in source.queries if "FROM `creature_ai_scripts`" in q[0])
        self.assertEqual(sql, "SELECT `id` AS `id`,`creature_id` AS `creature_id` FROM `creature_ai_scripts` "
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
                    return [{"id": 950001, "creature_id": -2}]
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
        bundle["tables"].pop("creature_ai_scripts")
        bundle["table_sha256"].pop("creature_ai_scripts")
        bundle["schema"]["tables"].pop("creature_ai_scripts")
        bundle["schema_sha256"] = digest(bundle["schema"])
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
            with self.subTest(kind=kind), self.assertRaisesRegex(ExportError, "schema.event_ai_type"):
                capture(source, starter_selection())

    def test_event_table_uses_existing_snapshot_and_row_budgets(self):
        source = starter_source()
        source.inventory["tables"]["creature_ai_scripts"]["engine"] = "MyISAM"
        with self.assertRaisesRegex(ExportError, "quiescent"):
            capture(source, starter_selection())
        source = starter_source()
        source.rows["creature_ai_scripts"] = [{"id": i+1, "creature_id": -1} for i in range(MAX_ROWS+1)]
        with self.assertRaisesRegex(ExportError, "rows.limit:creature_ai_scripts"):
            capture(source, starter_selection())
        self.assertTrue(source.finished)


if __name__ == "__main__":
    unittest.main()
