#!/usr/bin/env python3
"""Authored area/proc capture cases only. No DB, assets, or live connections.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import copy
from decimal import Decimal
import importlib.util
from pathlib import Path
import unittest

from classic_export.capture import Query, capture, validate_bundle
from classic_export.columns import INITIAL_HOOK_SCHEMA
from classic_export.core import ExportError, MAX_ROWS, MAX_SPELLS, digest
from classic_export import initial_hooks, spell_registry, native_spawn

spec = importlib.util.spec_from_file_location("export_tests", Path(__file__).with_name("test-classic-export.py"))
t = importlib.util.module_from_spec(spec); spec.loader.exec_module(t)


def source():
    s = t.starter_source(1)
    for table, columns in INITIAL_HOOK_SCHEMA.items():
        s.inventory["tables"][table] = {"engine": "InnoDB", "table_type": "BASE TABLE",
            "columns": [{"name": name, "type": kind, "nullable": nullable, "ordinal": i}
                        for i, (name, kind, nullable) in enumerate(columns, 1)],
            "primary_key": initial_hooks.PRIMARY_KEYS[table]}
        s.rows[table] = []
    return s


def captured(s=None):
    return capture(s or source(), t.starter_selection(1), capture_initial_hooks=True)


def repair(b):
    b["schema_sha256"] = digest(b["schema"])
    b["table_sha256"] = {k: digest(v) for k, v in b["tables"].items()}
    t.refresh_event_text(b)
    b["spell_script_capture"] = spell_registry.evidence(b)
    b["native_spawn_capture"] = native_spawn.evidence(b)
    b["initial_hook_capture"] = initial_hooks.evidence(b)


class InitialHooksTests(unittest.TestCase):
    def test_opt_in_distinguishes_missing_and_complete_empty(self):
        legacy = capture(source(), t.starter_selection(1))
        self.assertIsNone(initial_hooks.scope(legacy, 930001))
        self.assertNotIn("spell_area", legacy["tables"])
        b = captured()
        validate_bundle(b)
        ids = [r["Id"] for r in b["tables"]["spell_template"]]
        self.assertEqual(b["initial_hook_capture"]["selectors"]["spell_area"]["ids"], ids)
        for id in ids:
            scoped = initial_hooks.scope(b, id)
            self.assertEqual(scoped["status"], "complete")
            self.assertEqual(scoped["spell_area"], [])
            self.assertEqual(scoped["spell_proc_event"], [])

    def test_all_rows_retained_including_zero_and_conditional_rows(self):
        s = source()
        s.rows["spell_area"] = [t.row("spell_area", spell=930001, gender=2),
            t.row("spell_area", spell=930001, area=12, condition_id=0x10001, aura_spell=-17, gender=2),
            t.row("spell_area", spell=930002, gender=2)]
        s.rows["spell_proc_event"] = [t.row("spell_proc_event", entry=930001,
            ppmRate=Decimal("0"), CustomChance=Decimal("0"), SpellFamilyMask0=0xffffffffffffffff),
            t.row("spell_proc_event", entry=930002, procFlags=1)]
        b = captured(s)
        rows = initial_hooks.scope(b, 930001)
        self.assertEqual(len(rows["spell_area"]), 2)
        self.assertEqual(rows["spell_area"][1]["condition_id"], 0x10001)
        self.assertEqual(rows["spell_area"][1]["aura_spell"], -17)
        self.assertEqual(rows["spell_proc_event"][0]["SpellFamilyMask0"], 0xffffffffffffffff)
        self.assertEqual(rows["spell_proc_event"][0]["ppmRate"], {"decimal":"0"})
        self.assertNotIn(930002, [r["entry"] for r in b["tables"]["spell_proc_event"]])

    def test_schema_wrong_key_signedness_width_order_nullability_rejected(self):
        cases = [lambda m: m.update(primary_key=["spell"]),
                 lambda m: m["columns"][0].update(type="int unsigned"),
                 lambda m: m["columns"][0].update(type="mediumint"),
                 lambda m: m["columns"][0].update(nullable=True),
                 lambda m: m["columns"][0].update(ordinal=12)]
        for edit in cases:
            with self.subTest(edit=edit):
                s = source(); edit(s.inventory["tables"]["spell_area"])
                with self.assertRaises(ExportError): captured(s)
        s = source(); s.inventory["tables"]["spell_proc_event"]["columns"][3]["type"] = "bigint"
        with self.assertRaises(ExportError): captured(s)

    def test_table_or_coverage_cannot_be_added_or_removed_independently(self):
        for mutation in [lambda b: b.pop("initial_hook_capture"),
                         lambda b: b["tables"].pop("spell_area"),
                         lambda b: b["tables"].pop("spell_proc_event"),
                         lambda b: b["initial_hook_capture"]["selectors"]["spell_area"].update(ids=[]),
                         lambda b: b["initial_hook_capture"]["snapshot"].update(schema_sha256="0"*64)]:
            b = captured(); mutation(b)
            with self.assertRaises((ExportError, KeyError)): validate_bundle(b)

    def test_exact_parameterized_query_domain_and_cap_plus_one(self):
        for table, field, limit in [("spell_area", "spell", MAX_ROWS), ("spell_proc_event", "entry", MAX_SPELLS)]:
            sql, args = Query(table, "initial_hook_selectors", (17,18), limit=limit).sql()
            self.assertIn(f"`{field}` IN (%s,%s)", sql)
            self.assertEqual(args, (17,18,limit+1))
            for q in [Query(table,"ids",(17,)), Query(table,"initial_hook_selectors",(0,)),
                      Query(table,"initial_hook_selectors",(0x1000000,)),
                      Query(table,"initial_hook_selectors",(17,17)),
                      Query(table,"initial_hook_selectors",("17 OR 1=1",))]:
                with self.assertRaises(ExportError): q.sql()
        with self.assertRaises(ExportError): Query("spell_proc_event","initial_hook_selectors",(17,),limit=513).sql()

    def test_selector_domain_rejects_before_either_hook_read(self):
        s = source()
        s.rows["spell_template"][0]["Id"] = 0x1000000
        selected = t.starter_selection(1).json(); selected["spell_ids"] = [0x1000000]
        # Replace all references to the authored primary fixture spell.
        for rows in s.rows.values():
            for row in rows:
                for key, value in list(row.items()):
                    if value == 930001: row[key] = 0x1000000
        with self.assertRaisesRegex(ExportError, "initial_hooks.selector_domain"):
            capture(s,t.Selection.parse(selected),capture_initial_hooks=True)
        self.assertFalse(any("FROM `spell_area`" in sql or "FROM `spell_proc_event`" in sql for sql,_ in s.queries))

    def test_duplicate_rows_truncation_and_unselected_forgery_reject(self):
        s = source(); s.rows["spell_area"] = [t.row("spell_area",spell=930001)]*2
        with self.assertRaises(ExportError): captured(s)
        s = source(); s.rows["spell_area"] = [t.row("spell_area",spell=930001,area=i) for i in range(MAX_ROWS+1)]
        with self.assertRaises(ExportError): captured(s)
        b = captured(); b["tables"]["spell_area"] = [t.row("spell_area",spell=930002)]
        repair(b)
        with self.assertRaisesRegex(ExportError,"initial_hooks.unselected_row"): validate_bundle(b)

    def test_nontrivial_sql_rank_input_is_preserved_never_resolved_as_no_proc(self):
        s = source()
        s.rows["spell_template"].append(t.row("spell_template",Id=930002))
        s.rows["spell_chain"] = [t.row("spell_chain",spell_id=930001,prev_spell=930002,first_spell=930002,rank=2),
                                 t.row("spell_chain",spell_id=930002,first_spell=930002,rank=1)]
        s.rows["spell_proc_event"] = [t.row("spell_proc_event",entry=930002,procFlags=1)]
        b = captured(s)
        self.assertEqual(initial_hooks.scope(b,930001)["spell_proc_event"], [])
        self.assertEqual(initial_hooks.scope(b,930002)["spell_proc_event"][0]["procFlags"],1)
        self.assertIn("rank_semantics_not_resolved",b["initial_hook_capture"]["scope"])
        self.assertEqual(b["initial_hook_capture"]["snapshot"]["table_sha256"]["spell_chain"],digest(b["tables"]["spell_chain"]))

    def test_snapshot_engine_and_source_mode_requirements(self):
        s=source();s.inventory["tables"]["spell_area"]["engine"]="MyISAM"
        with self.assertRaises(ExportError): captured(s)
        for kwargs in [{"capture_initial_hooks":1},{"capture_initial_hooks":True}]:
            with self.assertRaises(ExportError): capture(source(),t.selection(),**kwargs)

    def test_duration_index_zero_retains_actual_dbc_row_and_signed_words(self):
        spec = importlib.util.spec_from_file_location(
            "preparation_tests", Path(__file__).with_name("test-native-starter.py"))
        preparation = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(preparation)
        bundle = preparation.source_bundle()
        for raw in [{0: 0, 1: 0, 2: 0, 3: 0},
                    {0: 0, 1: 15000, 2: -7, 3: 20000},
                    {0: 0, 1: -1, 2: 0, 3: -1}]:
            with self.subTest(raw=raw):
                files = preparation.dbcs_for(bundle)
                files["SpellDuration"] = preparation.dbc_bytes("SpellDuration", [raw])
                result = preparation.compile_starter(
                    bundle, preparation.parsed(files), preparation.Request(1, 1, 0))
                expected = {"id": 0, "base_ms": raw[1], "per_level_ms": raw[2],
                            "maximum_ms": raw[3]}
                self.assertTrue(result["spells"])
                self.assertTrue(all(row["duration"] == expected for row in result["spells"]))
                rows = [row for row in result["dbc_bindings"] if row["table"] == "SpellDuration"]
                self.assertEqual(len(rows), 1)
                self.assertEqual(rows[0]["key"], [0])
                self.assertEqual(rows[0]["words"], [raw[i] & 0xffffffff for i in range(4)])
                self.assertEqual(rows[0]["consumed_by_preparation"], [0, 1, 2, 3])

    def test_duration_index_zero_does_not_invent_missing_row_or_table(self):
        spec = importlib.util.spec_from_file_location(
            "preparation_tests", Path(__file__).with_name("test-native-starter.py"))
        preparation = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(preparation)
        bundle = preparation.source_bundle()
        for missing_table in (False, True):
            with self.subTest(missing_table=missing_table):
                files = preparation.dbcs_for(bundle)
                if missing_table:
                    del files["SpellDuration"]
                result = preparation.compile_starter(
                    bundle, preparation.parsed(files), preparation.Request(1, 1, 0))
                self.assertTrue(all(row["duration"] is None for row in result["spells"]))
                self.assertFalse(any(row["table"] == "SpellDuration" for row in result["dbc_bindings"]))
                missing = any(row["table"] == "SpellDuration" and row["reason"] == "dbc.file_missing"
                              for row in result["unsupported"])
                self.assertEqual(missing, missing_table)


if __name__ == "__main__": unittest.main()
