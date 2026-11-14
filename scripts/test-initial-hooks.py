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
    from classic_export.capture import full_report
    from classic_export.core import Selection, schema_profile
    b["report"]["unsupported"] = full_report(b["tables"], schema_profile(b["schema"], set(b["tables"])),
                                             Selection.parse(b["selection"]))


def global_captured(bundle=None):
    from classic_export import global_hooks as g
    if bundle is None:
        s = source()
        floats = {n for n, c in g.TABLES["spell_template"]["columns"].items() if c["type"] == "float"}
        for column in s.inventory["tables"]["spell_template"]["columns"]:
            if column["name"] in floats:
                column["type"] = "float"
        b = captured(s)
    else:
        b = bundle
    rows = {table: copy.deepcopy(b["tables"].get(table, [])) for table in g.TABLES}
    rows["spell_template"] = [
        {name: row.get(name, {"decimal": "0"} if column["type"] == "float" else 0)
         for name, column in g.TABLES["spell_template"]["columns"].items()}
        for row in b["tables"]["spell_template"]]
    b["global_hook_capture"] = g.evidence(b, rows, g.expected_schema(),
                                         copy.deepcopy(g.CONTRACT["manifest"]["configuration"]))
    return b


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

    def test_global_missing_is_not_complete_empty(self):
        from native_starter import global_hooks_plan as p
        from classic_export import global_hooks as g
        self.assertIsNone(p.supported(captured(), {}))
        b = global_captured()
        validate_bundle(b)
        self.assertEqual(p.supported(b, {})["spell_area"], [])
        for name in ("spell_area", "conditions", "spell_affect", "spell_chain"):
            bad = copy.deepcopy(b)
            del bad["global_hook_capture"]["rows"][name]
            with self.assertRaises(ExportError):
                g.validate(bad)

    def test_global_unrelated_area_preserves_cursor_condition_and_zero_index(self):
        from native_starter import global_hooks_plan as p
        from classic_export import global_hooks as g
        b = global_captured()
        c = b["global_hook_capture"]
        c["rows"]["spell_area"] = [t.row("spell_area", spell=17, area=0, condition_id=1),
                                     t.row("spell_area", spell=18, area=12, gender=2)]
        c["rows"]["conditions"] = [dict(condition_entry=1, type=2, value1=117, value2=1,
                                          value3=0, value4=0, flags=0)]
        b["global_hook_capture"] = g.evidence(b, c["rows"], c["schema"], c["configuration"])
        validate_bundle(b)
        areas = p.supported(b, {})["spell_area"]
        self.assertFalse(areas[0]["area_indexed"])
        self.assertEqual(areas[0]["condition_branch"], "condition")
        self.assertEqual(areas[0]["condition"]["value1"], 117)
        self.assertEqual(areas[1]["status"], "unknown")
        self.assertEqual([r["ordinal"] for r in areas], [0, 1])

    def test_global_inherited_proc_and_opening_callback_are_not_absent(self):
        from native_starter import global_hooks_plan as p
        from classic_export import global_hooks as g
        b = global_captured()
        c = b["global_hook_capture"]
        c["rows"]["spell_chain"] = [dict(spell_id=17, prev_spell=930001, first_spell=930001, rank=2, req_spell=0)]
        c["rows"]["spell_proc_event"] = [t.row("spell_proc_event", entry=17, ppmRate={"decimal": "1"}, CustomChance={"decimal": "0"})]
        c["rows"]["spell_scripts"] = [{"Id": 930001, "ScriptName": "spell_gameobject_call_for_help_on_usage"}]
        b["tables"]["spell_scripts"] = copy.deepcopy(c["rows"]["spell_scripts"])
        repair(b)
        b["global_hook_capture"] = g.evidence(b, c["rows"], c["schema"], c["configuration"])
        validate_bundle(b)
        spell = p.supported(b, {})["spells"][0]
        self.assertEqual(spell["families"]["spell_proc_event"]["status"], "present_unsupported")
        self.assertEqual(spell["families"]["spell_proc_event"]["rows"][0]["entry"], 17)
        self.assertEqual(spell["registry"][0]["registration"]["callbacks"], ["OnSuccessfulStart"])
        self.assertFalse(p.supported(b, {})["effective_learning_proven"])

    def test_global_wrong_manifest_schema_hash_and_duplicates_refuse(self):
        from classic_export import global_hooks as g
        for mutate in (lambda c: c.update(contract_sha256="0"*64),
                       lambda c: c.update(registration_manifest_sha256="0"*64),
                       lambda c: c["binding"].update(table_sha256="0"*64),
                       lambda c: c["schema"]["conditions"]["columns"].update(type=["tinyint", True, False]),
                       lambda c: c["rows"]["spell_template"].append(c["rows"]["spell_template"][0])):
            b = global_captured()
            mutate(b["global_hook_capture"])
            with self.assertRaises(ExportError):
                g.validate(b)

    def test_global_declared_nondefault_and_unknown_registry_stay_unknown(self):
        from native_starter import global_hooks_plan as p
        from classic_export import global_hooks as g
        b = global_captured()
        row = {"Entry": 12423, "ScriptName": "npc_garments_of_quests"}
        self.assertTrue(p.reviewed_creature(b, row))
        callback = p.callback(b, "creature", 12423, row["ScriptName"])
        self.assertEqual(callback["status"], "present_unsupported")
        self.assertIn("Reset", callback["registration"]["ai_callbacks"])
        self.assertEqual(p.callback(b, "spell", 17, "unknown_registered_name")["status"], "unknown")
        c = b["global_hook_capture"]
        c["configuration"]["BUILD_SCRIPTDEV"] = False
        b["global_hook_capture"] = g.evidence(b, c["rows"], c["schema"], c["configuration"])
        validate_bundle(b)
        self.assertFalse(p.reviewed_creature(b, row))
        self.assertEqual(p.callback(b, "creature", 12423, row["ScriptName"])["reason"],
                         "nondefault_build_configuration_not_reviewed")

    def test_global_selected_spell_aura_and_item_effects_remain_unsupported(self):
        from native_starter import global_hooks_plan as p
        from classic_export import global_hooks as g
        b = global_captured()
        c = b["global_hook_capture"]
        c["rows"]["spell_area"] = [t.row("spell_area", spell=930001, area=0, gender=2),
                                     t.row("spell_area", spell=17, aura_spell=-930001, gender=2)]
        b["tables"]["spell_area"] = [copy.deepcopy(c["rows"]["spell_area"][0])]
        repair(b)
        b["global_hook_capture"] = g.evidence(b, c["rows"], c["schema"], c["configuration"])
        validate_bundle(b)
        plan = p.supported(b, {})
        self.assertEqual([r["status"] for r in plan["spell_area"]], ["present_unsupported"]*2)
        self.assertFalse(plan["global_quest_effects_resolved"])
        self.assertEqual(plan["first_entry_location"], "requires_source_location_owner_not_playercreateinfo_zone")

    def test_global_query_retains_source_cursor_and_is_bounded(self):
        from classic_export import global_hooks as g
        sql, args = g.Query("spell_area").sql()
        self.assertNotIn("ORDER BY", sql)
        self.assertNotIn("WHERE", sql)
        self.assertEqual(args, (4097,))
        sql, args = g.Query("spell_template", (17, 18)).sql()
        self.assertIn("`Id` IN (%s,%s)", sql)
        self.assertEqual(args, (17, 18, 4097))
        for query in (g.Query("arbitrary"), g.Query("spell_area", (17,)),
                      g.Query("spell_template", (18, 17)), g.Query("spell_area", limit=4097)):
            with self.assertRaises(ExportError):
                query.sql()

    def test_learning_context_retains_active_hooks_and_refuses_reached_callbacks(self):
        from native_starter.reached_hooks import supported
        b = global_captured()
        b["tables"]["playercreateinfo"][0].update(map=b["selection"]["map_id"],
            position_x={"decimal": "0"}, position_y={"decimal": "0"}, position_z={"decimal": "0"})
        b["selection"].update(min_milliunits=[-1000]*3, max_milliunits=[1000]*3)
        b["global_hook_capture"]["rows"]["spell_scripts"] = [
            {"Id": 81, "ScriptName": "spell_gameobject_call_for_help_on_usage"}]
        words = [0]*173
        words[0] = 81
        spells = [{"id": 81, "server_numeric_words": words}]
        loader = {"default_spell_candidates": [81], "loaded_learning_maps_resolved": True}
        plan = supported(b, {}, spells, loader, {"status": "checked"})
        self.assertEqual(plan["roots"][0]["status"], "hooks_unreachable")
        self.assertEqual(plan["discharged_root_count"], 0)
        self.assertFalse(plan["roots"][0]["effective_learning_proven"])
        self.assertEqual(len(b["global_hook_capture"]["rows"]["spell_scripts"]), 1)
        words[6] = 0x40
        plan = supported(b, {}, spells, loader, {"status": "checked"})
        self.assertIn("reached_callback_unsupported", plan["roots"][0]["refusals"])
        self.assertEqual(plan["roots"][0]["status"], "blocked")

    def test_learning_context_requires_registration_and_captured_location(self):
        from native_starter.reached_hooks import supported
        b = global_captured()
        b["selection"].update(min_milliunits=[-1000]*3, max_milliunits=[1000]*3)
        b["tables"]["playercreateinfo"][0].update(map=b["selection"]["map_id"],
            position_x={"decimal": "0"}, position_y={"decimal": "0"}, position_z={"decimal": "0"})
        words = [0]*173
        words[0], words[6] = 81, 0x40
        spells = [{"id": 81, "server_numeric_words": words}]
        loader = {"default_spell_candidates": [81], "loaded_learning_maps_resolved": True}
        area = t.row("spell_area", spell=6298, area=148, gender=2)
        b["global_hook_capture"]["rows"]["spell_area"] = [area]
        plan = supported(b, {}, spells, loader, {"status": "checked"})
        self.assertEqual(plan["roots"][0]["refusals"], ["first_entry_source_location_required"])
        self.assertIsNone(plan["location_source_epoch"])
        b["tables"]["playercreateinfo"][0]["position_x"] = {"decimal": "2"}
        plan = supported(b, {}, spells, loader, {"status": "checked"})
        self.assertIn("first_entry_location_outside_selection", plan["roots"][0]["refusals"])
        b["global_hook_capture"].pop("registration_manifest_sha256")
        plan = supported(b, {}, spells, loader, {"status": "checked"})
        self.assertEqual(plan["roots"][0]["status"], "unknown")
        self.assertIn("registration_manifest_unknown", plan["roots"][0]["refusals"])

    def test_location_reader_corpus_checks_raw_bytes_and_rebound_mutations(self):
        from native_starter import location_cases, location_inputs
        from native_starter.dbc import parse_table
        from classic_export.core import decode_json
        corpus = decode_json((Path(__file__).parent / "native_starter/source_location_cases.json").read_bytes(), 1024*1024)
        self.assertEqual(corpus, location_cases.cases())
        expected = {
            "group-reached": "wmo_group_requires_owner",
            "missing-slot": "complete_collision_slots",
            "source-binding": "source_binding",
            "source-epoch": "source_epoch_binding",
            "position-outside-box": "position_outside_selection",
            "boolean-position": "input_fields",
            "non-yaw-transform": "wmo_transform_requires_owner",
            "file-hash": "collision_file_binding",
            "raw-area-binding": "raw_area_binding"}
        for case in corpus["cases"]:
            with self.subTest(case=case["name"]):
                values = {name: bytes.fromhex(data) for name, data in case["files"].items()}
                tables = {"Map": parse_table("Map", bytes.fromhex(case["map_dbc"]))}
                if case["name"] in expected:
                    self.assertEqual(case["diagnostic"], "source_location." + expected[case["name"]])
                    with self.assertRaisesRegex(ExportError, "^" + case["diagnostic"] + "$"):
                        location_inputs.prepare(location_inputs.Files(values=values), case["source"], tables)
                else:
                    result = location_inputs.prepare(location_inputs.Files(values=values), case["source"], tables)
                    self.assertEqual(result, case["result"])
                    self.assertEqual((result["area_id"], result["zone_id"]),
                        (12, 12) if case["name"] == "narrowed-flags" else (9, 12))
                    self.assertEqual(result["area_flag"], 1 if case["name"] == "narrowed-flags" else 125)
                    self.assertEqual(result["wmo_exclusion"]["wmo_bounds_containing_position"], 1)
                    self.assertIsNone(result["runtime_export"])

    def test_checked_grid_context_resolves_only_unreached_area_rows_without_sealing(self):
        from native_starter import location_cases, location_inputs
        from native_starter.dbc import parse_table
        from native_starter.reached_hooks import supported
        b = global_captured()
        b["tables"]["playercreateinfo"][0].update(map=0, zone=148,
            position_x={"decimal": "0"}, position_y={"decimal": "0"}, position_z={"decimal": "0"})
        b["selection"].update(map_id=0, min_milliunits=[-1000]*3, max_milliunits=[1000]*3)
        b["global_hook_capture"]["rows"]["spell_scripts"] = []
        b["global_hook_capture"]["rows"]["spell_area"] = [t.row("spell_area", spell=6298, area=148, gender=2)]
        words = [0]*173
        words[0], words[6] = 81, 0x40
        loader = {"default_spell_candidates": [81], "loaded_learning_maps_resolved": True}
        def run():
            source, maps, values = location_cases.bundle(b)
            location = location_inputs.prepare(location_inputs.Files(values=values), source,
                {"Map": parse_table("Map", maps)})
            return supported(b, {}, [{"id":81, "server_numeric_words":words}], loader, {"status":"checked"}, location)
        plan = run()
        self.assertEqual(plan["roots"][0]["status"], "pending_source_seal")
        self.assertEqual(plan["roots"][0]["refusals"], [])
        self.assertEqual(plan["discharged_root_count"], 0)
        self.assertEqual(plan["source_seal_refusal"], "production_profile_constructor_required")
        self.assertFalse(plan["roots"][0]["effective_learning_proven"])
        self.assertEqual(b["global_hook_capture"]["rows"]["spell_area"][0]["area"], 148)
        b["global_hook_capture"]["rows"]["spell_area"][0]["area"] = 9
        self.assertIn("reached_spell_area_unsupported", run()["roots"][0]["refusals"])
        b["global_hook_capture"]["rows"]["spell_area"][0]["area"] = 148
        b["global_hook_capture"]["rows"]["spell_scripts"] = [
            {"Id":81, "ScriptName":"spell_gameobject_call_for_help_on_usage"}]
        self.assertIn("reached_callback_unsupported", run()["roots"][0]["refusals"])
        b["global_hook_capture"].pop("registration_manifest_sha256")
        self.assertEqual(run()["roots"][0]["status"], "unknown")


if __name__ == "__main__": unittest.main()
