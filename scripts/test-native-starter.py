#!/usr/bin/env python3
"""Original synthetic rows only. No native content, install, database, or network.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import copy
import hashlib
import importlib.util
import os
import re
import itertools
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from classic_export.capture import capture
from classic_export.core import CORE_REVISION, ExportError, canonical, decode_json, digest
from native_starter.compiler import (Request, SPELL_FIELDS, FLOAT_SPELL_FIELDS, compile_starter,
                                     outfit_quantity, verify_preparation)
from native_starter.dbc import (LAYOUTS, MAX_RECORDS, binary32_bits,
                                load_directory, load_extraction_manifest, parse_table)

ROOT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("export_tests", ROOT / "test-classic-export.py")
export_tests = importlib.util.module_from_spec(spec)
spec.loader.exec_module(export_tests)


def source_bundle():
    source = export_tests.starter_source()
    for name in FLOAT_SPELL_FIELDS:
        col = next(c for c in source.inventory["tables"]["spell_template"]["columns"] if c["name"] == name)
        col["type"] = "float"
    for spell in source.rows["spell_template"]:
        spell.update(CastingTimeIndex=700001, rangeIndex=710001)
    for item in source.rows["item_template"]:
        item["BuyCount"] = 1
    source.rows["creature_template"][0]["Faction"] = 970001
    return capture(source, export_tests.starter_selection())


def dbc_bytes(name, rows):
    layout = LAYOUTS[name]
    records = b""
    for values in rows:
        if name == "CharBaseInfo":
            records += bytes(values)
        else:
            words = [0] * (layout.size//4)
            for key, value in values.items():
                words[key] = value & 0xffffffff
            records += struct.pack("<" + "I"*len(words), *words)
    strings = b"\0"
    return struct.pack("<4s4I", b"WDBC", len(rows), layout.fields, layout.size, len(strings)) + records + strings


def dbcs_for(source):
    rows = {name: [] for name in LAYOUTS}
    rows.update({
        "ChrRaces": [{0: 1, 2: 970001, 4: 940010, 5: 940011}],
        "ChrClasses": [{0: 1, 3: 1}],
        "CharBaseInfo": [(1, 1)],
        "CharStartOutfit": [{0: 960001, 1: 0x00000101, 2: 920001, 3: -1}],
        "Map": [{0: 0}],
        "Faction": [{0: 980001, 1: -1}],
        "FactionTemplate": [{0: 970001, 1: 980001}],
        "SpellCastTimes": [{0: 700001, 1: 1300, 2: -1, 3: 800}],
        "SpellRange": [{0: 710001, 1: binary32_bits("1.5"), 2: binary32_bits("27.25"), 3: 2}],
        "SkillLine": [{0: 60001}, {0: 60002}],
        "SkillRaceClassInfo": [{0: 800001, 1: 60001, 6: 810001}, {0: 800002, 1: 60002}],
        "SkillTiers": [{0: 810001}],
        "SkillLineAbility": [{0: 820001, 1: 60001, 2: 930101}],
    })
    for spell in source["tables"]["spell_template"]:
        row = {}
        for name, field in SPELL_FIELDS.items():
            value = spell[name]
            row[field] = binary32_bits(value["decimal"]) if name in FLOAT_SPELL_FIELDS else value & 0xffffffff
        rows["Spell"].append(row)
    return {name: dbc_bytes(name, values) for name, values in rows.items()}


def parsed(files):
    return {name: parse_table(name, value) for name, value in files.items()}


def extraction_metadata(files):
    return {"format": "benilla-native-dbc-extraction-v2",
            "chain_policy": "benilla-formats-vanilla-chain", "client_build_claim": 5875,
            "client_build_authenticity_verified": False,
            "files": [{**table.manifest(), "winning_archive": "dbc.MPQ"}
                      for _, table in sorted(parsed(files).items())]}


def rehash_source(bundle):
    from classic_export.capture import full_report
    from classic_export.core import Selection, schema_profile
    from classic_export.starter import keys
    bundle["table_sha256"] = {name: digest(rows) for name, rows in bundle["tables"].items()}
    bundle["starter_required_keys"] = keys(bundle["tables"])
    if "native_spawn_capture" in bundle:
        from classic_export.native_spawn import evidence
        bundle["native_spawn_capture"] = evidence(bundle)
    profiles = schema_profile(bundle["schema"], set(bundle["tables"]) | {"db_version"})
    bundle["report"]["unsupported"] = full_report(bundle["tables"], profiles, Selection.parse(bundle["selection"]))


class DbcTests(unittest.TestCase):
    def test_rust_extractor_allowlist_matches_compiler_layouts(self):
        source = (ROOT.parent/"crates"/"benilla-native-dbc"/"src"/"lib.rs").read_text()
        actual = {name: (int(fields), int(size)) for name, fields, size in
                  re.findall(r'\("([A-Za-z]+)", (\d+), (\d+)\)', source)}
        self.assertEqual(actual, {name: (layout.fields, layout.size) for name, layout in LAYOUTS.items()})

    def test_packed_outfit_and_base_info_are_not_dword_tables(self):
        outfit = parse_table("CharStartOutfit", dbc_bytes("CharStartOutfit", [{0: 1000, 1: 0x010801, 2: -1}]))
        self.assertEqual(len(outfit.rows[0].words), 38)
        self.assertEqual(outfit.rows[0].words[1], 0x010801)
        self.assertEqual(parse_table("CharBaseInfo", dbc_bytes("CharBaseInfo", [(1, 8)])).rows[0].key, (1, 8))

    def test_map_zero_and_no_default_key(self):
        self.assertIn(0, parse_table("Map", dbc_bytes("Map", [{0: 0}])).by_id)

    def test_layout_truncation_trailing_and_duplicate_keys_fail(self):
        original = dbc_bytes("Map", [{0: 0}])
        bad_layout = bytearray(original)
        struct.pack_into("<I", bad_layout, 8, 41)
        for data in [b"", b"BAD!"+original[4:], original[:-1], original+b"\0", bad_layout,
                     dbc_bytes("Map", [{0: 0}, {0: 0}])]:
            with self.subTest(data=len(data)), self.assertRaises(ExportError):
                parse_table("Map", bytes(data))
        with self.assertRaises(ExportError):
            parse_table("CharBaseInfo", dbc_bytes("CharBaseInfo", [(1, 8), (1, 8)]))

    def test_counts_and_string_offsets_fail_closed(self):
        data = bytearray(dbc_bytes("Map", [{0: 0}]))
        struct.pack_into("<I", data, 4, MAX_RECORDS+1)
        with self.assertRaisesRegex(ExportError, "record_limit"):
            parse_table("Map", bytes(data))
        with self.assertRaisesRegex(ExportError, "string_offset"):
            parse_table("Map", dbc_bytes("Map", [{0: 0, 1: 40}]))
        bad = bytearray(dbc_bytes("Map", [{0: 0}]))
        bad[-1] = 65
        with self.assertRaisesRegex(ExportError, "string_block"):
            parse_table("Map", bytes(bad))

    def test_invalid_utf8_and_string_limit(self):
        data = dbc_bytes("Map", [{0: 0, 1: 1}])
        invalid = data[:-1]+b"\0\xff\0"
        invalid = invalid[:16]+struct.pack("<I", 3)+invalid[20:]
        with self.assertRaisesRegex(ExportError, "encoding"):
            parse_table("Map", invalid)

    def test_exact_binary32_rounding(self):
        for decimal, bits in [("0", 0), ("-2", 0xc0000000), ("1.5", 0x3fc00000),
                              ("1.000000059604644775390625", 0x3f800000),
                              ("1.000000059604644775390626", 0x3f800001),
                              ("1.000000178813934326171875", 0x3f800002),
                              ("1e-50", 0), ("1e-45", 1)]:
            self.assertEqual(binary32_bits(decimal), bits)
        with self.assertRaisesRegex(ExportError, "overflow"):
            binary32_bits("1e100")

    def test_repeated_string_offsets_share_one_decoded_value(self):
        values = [{0: 900001+i, **{c: 1 for c in LAYOUTS["Spell"].strings}} for i in range(16)]
        data = dbc_bytes("Spell", values)
        strings = b"\0"+b"a"*16_384+b"\0"
        data = data[:16]+struct.pack("<I", len(strings))+data[20:-1]+strings
        table = parse_table("Spell", data)
        self.assertEqual(len({id(value) for row in table.rows for value in row.text.values()}), 1)

    def test_non_regular_input_is_reported_without_blocking(self):
        if not hasattr(os, "mkfifo"):
            self.skipTest("POSIX FIFO regression only")
        with tempfile.TemporaryDirectory() as directory:
            os.mkfifo(Path(directory)/"CharBaseInfo.dbc")
            tables, errors = load_directory(Path(directory))
        self.assertFalse(tables)
        self.assertIn("input.regular_file_required", {e["reason"] for e in errors})

    def test_unique_decoded_text_has_a_cumulative_limit(self):
        data = dbc_bytes("Map", [{0: 0, 1: 1}])
        strings = b"\0original\0"
        data = data[:16]+struct.pack("<I", len(strings))+data[20:-1]+strings
        with mock.patch("native_starter.dbc.MAX_DECODED_TEXT_BYTES", 4):
            with self.assertRaisesRegex(ExportError, "decoded_text_limit"):
                parse_table("Map", data)

    def test_missing_files_all_reported(self):
        with tempfile.TemporaryDirectory() as directory:
            tables, errors = load_directory(Path(directory))
        self.assertFalse(tables)
        self.assertEqual({e["table"] for e in errors}, set(LAYOUTS))

    def test_manifest_requires_complete_exact_allowlist_and_hashes(self):
        files = dbcs_for(source_bundle())
        tables = parsed(files)
        valid = extraction_metadata(files)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ExportError, "manifest_missing"):
                load_extraction_manifest(root, tables)
            for label, mutate in [
                ("duplicate file", lambda v: v["files"].__setitem__(0, v["files"][1])),
                ("missing file", lambda v: v["files"].pop()),
                ("changed bytes", lambda v: v["files"][0].__setitem__("sha256", "0"*64)),
                ("wrong numeric type", lambda v: v["files"][0].__setitem__("fields", True)),
                ("imported path", lambda v: v["files"][0].__setitem__("file", "../CharBaseInfo.dbc")),
                ("archive path", lambda v: v["files"][0].__setitem__("winning_archive", "../dbc.MPQ")),
                ("extra field", lambda v: v.__setitem__("extra", 1)),
                ("legacy extraction", lambda v: v.__setitem__("format", "benilla-native-dbc-extraction-v1")),
                ("unsupported policy", lambda v: v.__setitem__("chain_policy", "other")),
                ("authenticity claim", lambda v: v.__setitem__("client_build_authenticity_verified", True)),
            ]:
                value = copy.deepcopy(valid)
                mutate(value)
                (root/"extraction.json").write_bytes(canonical(value))
                with self.subTest(label=label), self.assertRaises(ExportError):
                    load_extraction_manifest(root, tables)
            (root/"extraction.json").write_bytes(canonical(valid))
            bound = load_extraction_manifest(root, tables)
            self.assertTrue(bound["integrity_verified"])
            self.assertFalse(bound["authenticity_verified"])
            with self.assertRaisesRegex(ExportError, "complete_valid_dbc"):
                load_extraction_manifest(root, {})
            (root/"extraction.json").write_bytes(canonical(valid)+b"\n")
            self.assertNotEqual(bound["sha256"], load_extraction_manifest(root, tables)["sha256"])
            (root/"extraction.json").write_bytes(b'{"format":"one","format":"two"}')
            with self.assertRaises(ExportError):
                load_extraction_manifest(root, tables)


class CompilerTests(unittest.TestCase):
    def setUp(self):
        self.source = source_bundle()
        self.files = dbcs_for(self.source)
        self.request = Request.parse({"race": 1, "class": 1, "gender": 0})

    def compile(self):
        return compile_starter(self.source, parsed(self.files), self.request)

    def test_exact_ids_cast_range_progression_outfit_and_blocked_runtime(self):
        result = self.compile()
        self.assertEqual(result["status"], "blocked")
        self.assertIsNone(result["runtime_export"])
        self.assertNotIn("runnable", result)
        identity = result["player"]["identity"]
        self.assertEqual(identity["display_id"], 940010)
        self.assertEqual(identity["power_type"], 1)
        self.assertEqual(identity["start"]["map"], 0)
        self.assertEqual(result["player"]["level_inputs"][0]["create_health"], 8)
        self.assertEqual(result["player"]["level_inputs"][0]["create_mana"], 0)
        self.assertIsNone(result["player"]["level_inputs"][-1]["xp_for_next_level"])
        grants = result["player"]["outfit"]["grants_in_source_order"]
        self.assertEqual([g["item_id"] for g in grants], [920001, 920101])
        self.assertEqual([g["count"] for g in grants], [1, 2])
        self.assertEqual(result["spells"][0]["cast_time"]["per_level_ms"], -1)
        self.assertEqual(result["spells"][0]["range"]["minimum_binary32"], 0x3fc00000)
        self.assertFalse(any(s["server_client_differences"] for s in result["spells"]))
        self.assertEqual(result["source_sha256"], digest(self.source))
        self.assertIn("native.runtime_profile_not_implemented", {i["reason"] for i in result["unsupported"]})

    def test_every_sql_and_selected_dbc_field_has_exactly_one_disposition(self):
        result = self.compile()
        expected = {(table, tuple(row[k] for k in export_tests.starter.KEYS[table])): set(row)
                    for table, rows in self.source["tables"].items() for row in rows}
        self.assertEqual(len(result["source_field_dispositions"]), len(expected))
        for row in result["source_field_dispositions"]:
            fields = row["consumed_by_preparation"] + row["inert_source_annotations"]
            fields += [f for values in row["blocked_for_gameplay"].values() for f in values]
            self.assertEqual(len(fields), len(set(fields)))
            self.assertEqual(set(fields), expected[(row["table"], tuple(row["key"]))])
        for row in result["dbc_bindings"]:
            cols = row["consumed_by_preparation"]+row["blocked_for_gameplay"]
            self.assertEqual(sorted(cols), list(range(len(row["words"]))))

    def test_no_dbc_data_is_never_filled_with_defaults(self):
        result = compile_starter(self.source, {}, self.request)
        self.assertIsNone(result["player"]["identity"]["display_id"])
        self.assertIsNone(result["player"]["identity"]["power_type"])
        self.assertIsNone(result["player"]["outfit"]["outfit_record_id"])
        self.assertTrue(all(s["cast_time"] is None and s["range"] is None for s in result["spells"]))
        self.assertTrue(result["required_additional_rows"])

    def test_missing_outfit_item_is_exact_dependency_not_fake_grant(self):
        self.files["CharStartOutfit"] = dbc_bytes("CharStartOutfit", [{0: 960001, 1: 0x0101, 2: 999001}])
        result = self.compile()
        self.assertIn({"table": "item_template", "id": 999001, "reason": "player.outfit_item_not_captured"},
                      result["required_additional_rows"])
        self.assertIsNone(result["player"]["outfit"]["grants_in_source_order"][0]["count"])

    def test_gender_and_high_outfit_byte_are_not_ignored(self):
        for packed in [0x010101, 0x01000101]:
            self.files["CharStartOutfit"] = dbc_bytes("CharStartOutfit", [{0: 960001, 1: packed, 2: 920001}])
            result = self.compile()
            self.assertIsNone(result["player"]["outfit"]["outfit_record_id"])
            self.assertIn("player.outfit_missing", {i["reason"] for i in result["unsupported"]})

    def test_duplicate_outfit_variants_fail_without_arbitrary_selection(self):
        self.files["CharStartOutfit"] = dbc_bytes("CharStartOutfit", [{0: 960001, 1: 0x0101}, {0: 960002, 1: 0x0101}])
        result = self.compile()
        self.assertIn("player.outfit_ambiguous", {i["reason"] for i in result["unsupported"]})
        self.assertIsNone(result["player"]["outfit"]["outfit_record_id"])

    def test_multi_effect_aura_trigger_and_no_invented_heal(self):
        spell = self.source["tables"]["spell_template"][0]
        spell.update(Effect2=6, EffectApplyAuraName2=3, EffectAmplitude2=2000)
        rehash_source(self.source)
        self.files = dbcs_for(self.source)
        result = self.compile()
        effects = result["spells"][0]["effects"]
        self.assertEqual([e["Effect"] for e in effects], [2, 6, 0])
        self.assertEqual(effects[1]["EffectAmplitude"], 2000)
        self.assertFalse(any(e["Effect"] == 10 for s in result["spells"] for e in s["effects"]))
        reasons = {i["reason"] for i in result["unsupported"]}
        self.assertIn("spell.multiple_or_missing_effects", reasons)
        self.assertIn("spell.aura_or_trigger_dependency", reasons)

    def test_native_server_client_discrepancy_is_specific(self):
        data = bytearray(self.files["Spell"])
        struct.pack_into("<I", data, 20+61*4, 10)
        self.files["Spell"] = bytes(data)
        result = self.compile()
        self.assertIn("Effect1", result["spells"][0]["server_client_differences"])
        self.assertEqual(result["spells"][0]["effects"][0]["Effect"], 2)

    def test_signed_zero_range_is_equal_to_zero_but_bits_remain_exact(self):
        for low, high in [(0x80000000, 0x3f800000), (0, 0x80000000)]:
            self.files["SpellRange"] = dbc_bytes("SpellRange", [{0: 710001, 1: low, 2: high}])
            result = self.compile()
            self.assertNotIn("spell.range_domain", {i["reason"] for i in result["unsupported"]})
            self.assertEqual(result["spells"][0]["range"]["minimum_binary32"], low)
            self.assertEqual(result["spells"][0]["range"]["maximum_binary32"], high)

    def test_dbc_only_reagent_fields_retained(self):
        data = bytearray(self.files["Spell"])
        struct.pack_into("<I", data, 20+42*4, 999001)
        struct.pack_into("<I", data, 20+50*4, 2)
        self.files["Spell"] = bytes(data)
        result = self.compile()
        self.assertEqual(result["spells"][0]["dbc_only_inputs"]["reagents"][0], 999001)
        self.assertEqual(result["spells"][0]["dbc_only_inputs"]["reagent_counts"][0], 2)

    def test_effect_item_type_modifier_mask_is_not_invented_item_dependency(self):
        spell = self.source["tables"]["spell_template"][0]
        spell.update(Effect1=6, EffectApplyAuraName1=107, EffectItemType1=0x10000000)
        rehash_source(self.source)
        self.files = dbcs_for(self.source)
        result = self.compile()
        self.assertFalse(any(row["table"] == "item_template" and row["id"] == 0x10000000
                             for row in result["required_additional_rows"]))
        self.assertIn("spell.effect_item_type_semantics_required", {i["reason"] for i in result["unsupported"]})

    def test_selected_dbc_binding_budget_is_checked_before_large_report(self):
        with mock.patch("native_starter.compiler.MAX_BINDING_BYTES", 1):
            with self.assertRaisesRegex(ExportError, "dbc_binding_byte_limit"):
                self.compile()

    def test_unrelated_active_spell_is_visible_unavailable_without_its_own_entry_veto(self):
        result = self.compile()
        admission = result["ability_admission"]
        self.assertEqual(admission["available_abilities"], [])
        active = admission["unavailable_active_abilities"]
        self.assertEqual([r["id"] for r in active], [930101])
        self.assertEqual(active[0]["cast_admission"], "reject")
        self.assertEqual(active[0]["presentation"], "visible_unavailable")
        self.assertEqual(active[0]["trigger_dependency_ids"], [930102])
        related = [i for i in result["unsupported"] if i["table"] == "spell_template"
                   and i["key"] in ([930101], "930101") and i["reason"].startswith("spell.")]
        self.assertTrue(related)
        self.assertTrue(all(i["admission_scope"]["kind"] == "unavailable_active_ability" for i in related))
        self.assertTrue(any(i["reason"] == "player.spell_learning_passive_dependencies_not_captured"
                            and i["admission_scope"]["kind"] == "selection_entry" for i in result["unsupported"]))
        self.assertEqual(result["status"], "blocked")

    def test_initial_passive_and_trigger_dependencies_are_entry_critical(self):
        next(r for r in self.source["tables"]["spell_template"] if r["Id"] == 930101)["Attributes"] = 0x40
        rehash_source(self.source)
        self.files = dbcs_for(self.source)
        result = self.compile()
        self.assertFalse(result["ability_admission"]["unavailable_active_abilities"])
        self.assertEqual(result["ability_admission"]["entry_critical_spell_ids"], [930101, 930102])
        self.assertTrue(all(i["admission_scope"]["kind"] == "selection_entry" for i in result["unsupported"]))

    def test_nonpassive_learning_sensitive_roles_are_entry_critical(self):
        for field, value in [("Attributes", 0x80), ("AttributesEx", 0x80000000),
                             ("Effect2", 36), ("Effect3", 44)]:
            self.source = source_bundle()
            row = next(r for r in self.source["tables"]["spell_template"] if r["Id"] == 930101)
            row[field] = value
            rehash_source(self.source)
            self.files = dbcs_for(self.source)
            with self.subTest(field=field):
                result = self.compile()
                admission = result["ability_admission"]
                self.assertFalse(admission["unavailable_active_abilities"])
                self.assertEqual(admission["learning_sensitive_initial_spell_ids"], [930101])
                self.assertEqual(admission["entry_critical_spell_ids"], [930101, 930102])
                self.assertTrue(all(i["admission_scope"]["kind"] == "selection_entry"
                                    for i in result["unsupported"]))

    def test_learning_role_field_mismatches_remain_unresolved(self):
        for field, value in [("AttributesEx", 0x80000000), ("Effect2", 36), ("Effect3", 44)]:
            files = dbcs_for(self.source)
            data = bytearray(files["Spell"])
            index = next(i for i, r in enumerate(self.source["tables"]["spell_template"]) if r["Id"] == 930101)
            struct.pack_into("<I", data, 20+index*692+SPELL_FIELDS[field]*4, value)
            self.files["Spell"] = bytes(data)
            with self.subTest(field=field):
                result = self.compile()
                self.assertFalse(result["ability_admission"]["unavailable_active_abilities"])
                self.assertEqual(result["ability_admission"]["unresolved_initial_spell_roles"], [930101])

    def test_item_effect_dependency_overrides_unavailable_active_classification(self):
        next(r for r in self.source["tables"]["item_template"] if r["entry"] == 920101)["spellid_1"] = 930101
        rehash_source(self.source)
        result = self.compile()
        self.assertFalse(result["ability_admission"]["unavailable_active_abilities"])
        self.assertEqual(result["ability_admission"]["entry_critical_spell_ids"], [930101, 930102])
        self.assertFalse(result["ability_admission"]["unresolved_initial_spell_roles"])

    def test_passive_bit_disagreement_does_not_quarantine_unresolved_role(self):
        data = bytearray(self.files["Spell"])
        index = next(i for i, r in enumerate(self.source["tables"]["spell_template"]) if r["Id"] == 930101)
        struct.pack_into("<I", data, 20+index*692+6*4, 0x40)
        self.files["Spell"] = bytes(data)
        result = self.compile()
        self.assertFalse(result["ability_admission"]["unavailable_active_abilities"])
        self.assertEqual(result["ability_admission"]["unresolved_initial_spell_roles"], [930101])

    def test_candidates_reference_actual_captured_ids_only(self):
        result = self.compile()
        path = result["candidate_paths"][0]
        self.assertEqual(path["quest_id"], 910001)
        self.assertEqual(path["giver_spawn_guids"], [1])
        self.assertEqual(path["target_spawn_guids"], [1])
        self.assertEqual(path["initial_direct_effect_spell_ids"], [930101])
        self.assertEqual(path["eligibility"], "not_evaluated")

    def test_event_ai_empty_capture_is_narrow_and_does_not_enable_ai(self):
        result = self.compile()
        creature = result["creatures"][0]
        self.assertEqual(creature["event_ai_capture"]["status"], "captured_selectors_empty")
        self.assertEqual(creature["event_ai_capture"]["selectors"], [-1, 900001])
        self.assertIsNone(creature["ai_profile"])
        self.assertIn("creature.unexported_groups_assistance_context",
                      {i["reason"] for i in result["unsupported"]})
        self.source.pop("event_ai_capture")
        self.source["tables"].pop("creature_ai_scripts")
        rehash_source(self.source)
        result = self.compile()
        self.assertIsNone(result["creatures"][0]["event_ai_capture"])
        self.assertIn("creature.event_ai_applicability_absence_not_proven",
                      {i["reason"] for i in result["unsupported"]})

    def test_event_ai_entry_and_spawn_rows_both_block(self):
        self.source["tables"]["creature_ai_scripts"] = [
            {"id": 950001, "creature_id": 900001}, {"id": 950002, "creature_id": -1}]
        rehash_source(self.source)
        result = self.compile()
        self.assertEqual(result["creatures"][0]["event_ai_capture"]["matching_rows"],
                         self.source["tables"]["creature_ai_scripts"])
        issue = next(i for i in result["unsupported"]
                     if i["reason"] == "creature.event_ai_matching_rows_require_execution")
        self.assertEqual(issue["event_ids"], [950001, 950002])
        self.assertEqual(issue["admission_scope"]["kind"], "selection_entry")

    def test_recompile_verification_prevents_forged_support_even_with_rehashed_output(self):
        result = self.compile()
        verify_preparation(result, self.source, parsed(self.files))
        result["unsupported"] = []
        del result["preparation_sha256"]
        result["preparation_sha256"] = digest(result)
        with self.assertRaisesRegex(ExportError, "preparation_mismatch"):
            verify_preparation(result, self.source, parsed(self.files))

    def test_changed_dbc_file_changes_identity_and_invalidates_old_preparation(self):
        result = self.compile()
        self.files["ChrRaces"] = dbc_bytes("ChrRaces", [{0: 1, 2: 970001, 4: 940099, 5: 940011}])
        self.assertNotEqual(result["preparation_sha256"], self.compile()["preparation_sha256"])
        with self.assertRaises(ExportError):
            verify_preparation(result, self.source, parsed(self.files))

    def test_class_mismatch_and_unverified_source_hash_fail(self):
        with self.assertRaisesRegex(ExportError, "selection_mismatch"):
            compile_starter(self.source, parsed(self.files), Request(1, 8, 0))
        self.source["tables"]["player_classlevelstats"][0]["basehp"] += 1
        with self.assertRaisesRegex(ExportError, "table_hash_or_order"):
            self.compile()

    def test_deterministic_independent_dictionary_order(self):
        first = self.compile()
        second = compile_starter(self.source, dict(reversed(list(parsed(self.files).items()))), self.request)
        self.assertEqual(canonical(first), canonical(second))

    def test_outfit_quantities_exact_strict_food_drink_boundaries(self):
        base = {"BuyCount": 7, "class": 0, "subclass": 5, "spellcategory_1": 11, "stackable": 4}
        self.assertEqual(outfit_quantity(base), 7)
        self.assertEqual(outfit_quantity({**base, "stackable": 5}), 4)
        self.assertEqual(outfit_quantity({**base, "spellcategory_1": 59, "stackable": 2}), 7)
        self.assertEqual(outfit_quantity({**base, "spellcategory_1": 59, "stackable": 3}), 2)
        self.assertEqual(outfit_quantity({**base, "class": 1, "stackable": 20}), 7)
        with self.assertRaises(ExportError):
            outfit_quantity({**base, "BuyCount": 0})

    def test_invalid_readable_dbc_changes_are_still_hash_bound(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/"Map.dbc"
            path.write_bytes(b"A"*20)
            tables, errors = load_directory(Path(directory))
            first = compile_starter(self.source, tables, self.request, errors)
            path.write_bytes(b"B"*20)
            tables, errors = load_directory(Path(directory))
            second = compile_starter(self.source, tables, self.request, errors)
            self.assertNotEqual(first["preparation_sha256"], second["preparation_sha256"])
            with self.assertRaisesRegex(ExportError, "preparation_mismatch"):
                verify_preparation(first, self.source, tables, errors)

    def test_outfit_quantities_committed_oracle_on_every_platform(self):
        cases = list(itertools.product((0, 1, 15), (4, 5), (0, 11, 59), (1, 2, 7), (-1, 0, 2, 3, 4, 5, 20)))
        fixture = decode_json((ROOT/"native_starter"/"outfit_cases.json").read_bytes(), 65536)
        self.assertEqual(set(fixture), {"format", "core_revision", "oracle_sha256", "cases"})
        self.assertEqual(fixture["format"], "benilla-native-outfit-oracle-v1")
        self.assertEqual(fixture["core_revision"], CORE_REVISION)
        self.assertEqual(fixture["oracle_sha256"], hashlib.sha256(
            (ROOT/"native_starter"/"outfit_oracle.cpp").read_text(encoding="utf-8").encode("utf-8")).hexdigest())
        self.assertEqual([row["input"] for row in fixture["cases"]], [list(case) for case in cases])
        self.assertTrue(all(set(row) == {"input", "quantity"} and type(row["quantity"]) is int
                            for row in fixture["cases"]))
        expected = [row["quantity"] for row in fixture["cases"]]
        actual = [outfit_quantity({"class": cls, "subclass": sub, "spellcategory_1": category,
                                  "BuyCount": buy, "stackable": stack}) for cls, sub, category, buy, stack in cases]
        self.assertEqual(actual, expected)
        self.assertEqual(len(actual), 378)

    def test_cli_private_no_overwrite_verify_and_inputs_unchanged(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            dbc = root / "dbc"
            dbc.mkdir()
            for name, data in self.files.items():
                (dbc / (name+".dbc")).write_bytes(data)
            manifest = dbc/"extraction.json"
            manifest.write_bytes(canonical(extraction_metadata(self.files)))
            source = root / "source.json"
            source.write_bytes(canonical(self.source))
            output = root / "preparation.json"
            command = [sys.executable, str(ROOT/"native-starter.py"), "prepare", "--source", str(source),
                       "--dbc-dir", str(dbc), "--race", "1", "--class", "1", "--gender", "0", "--output", str(output)]
            first = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(first.returncode, 3, first.stderr)
            self.assertTrue(output.exists())
            if os.name == "posix":
                self.assertEqual(output.stat().st_mode & 0o777, 0o600)
            before = output.read_bytes()
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 2)
            self.assertEqual(output.read_bytes(), before)
            verify = subprocess.run([sys.executable, str(ROOT/"native-starter.py"), "verify", "--source", str(source),
                                     "--dbc-dir", str(dbc), str(output)], capture_output=True, text=True)
            self.assertEqual(verify.returncode, 0, verify.stderr)
            dependency_output = root/"dependencies.json"
            dependency_command = list(command)
            dependency_command[2] = "dependencies"
            dependency_command[-1] = str(dependency_output)
            dependencies = subprocess.run(dependency_command, capture_output=True, text=True)
            self.assertEqual(dependencies.returncode, 0, dependencies.stderr)
            request_data = decode_json(dependency_output.read_bytes(), 65536)
            from classic_export.learning import request as validate_request
            self.assertEqual(validate_request(request_data, export_tests.starter_selection()), request_data)
            self.assertEqual(request_data, decode_json(output.read_bytes(), 128*1024*1024)["dependency_request"])
            self.assertEqual(subprocess.run(dependency_command, capture_output=True).returncode, 2)
            if os.name == "posix":
                self.assertEqual(dependency_output.stat().st_mode & 0o777, 0o600)
            manifest.write_bytes(manifest.read_bytes()+b"\n")
            changed = subprocess.run([sys.executable, str(ROOT/"native-starter.py"), "verify", "--source", str(source),
                                     "--dbc-dir", str(dbc), str(output)], capture_output=True, text=True)
            self.assertEqual(changed.returncode, 2)
            self.assertIn("preparation_mismatch", changed.stderr)
            manifest.unlink()
            command[-1] = str(root/"incomplete.json")
            incomplete = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(incomplete.returncode, 2)
            self.assertIn("manifest_missing", incomplete.stderr)
            self.assertFalse((root/"incomplete.json").exists())
            manifest.write_bytes(canonical(extraction_metadata(self.files)))
            for name, data in self.files.items():
                self.assertEqual((dbc/(name+".dbc")).read_bytes(), data)
            self.assertEqual(source.read_bytes(), canonical(self.source))
            command[-1] = str(dbc/"forbidden-output.json")
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 2)
            self.assertFalse((dbc/"forbidden-output.json").exists())


class LearningClosureTests(unittest.TestCase):
    setUp = CompilerTests.setUp
    compile = CompilerTests.compile
    def test_recursive_missing_sql_spells_still_expand_local_skill_effects(self):
        ability = []
        skill_rows = [{0: 60001}, {0: 60002}]
        spell_rows = [{i: word for i, word in enumerate(r.words)}
                      for r in parsed(self.files)["Spell"].rows]
        for index in range(24):
            skill = 60001 if index == 0 else 61000 + index
            next_skill = 61000 + index + 1 if index < 23 else 60001
            ability.append({0: 830000+index, 1: skill, 2: 50001+index, 9: 2})
            spell_rows.append({0: 50001+index, 63: 44, 108: next_skill})
            if index:
                skill_rows.append({0: skill})
        self.files["SkillLine"] = dbc_bytes("SkillLine", skill_rows)
        self.files["SkillLineAbility"] = dbc_bytes("SkillLineAbility", ability)
        self.files["Spell"] = dbc_bytes("Spell", spell_rows)
        result = self.compile()
        closure = result["player"]["skills"]["dependency_closure"]
        self.assertTrue(closure["fixed_point_reached"])
        self.assertFalse(closure["captured_inputs_complete"])
        self.assertEqual(len(closure["ability_candidates"]), 24)
        self.assertTrue(set(range(50001, 50025)) <= set(closure["spell_ids"]))
        self.assertIn(61023, closure["skill_ids"])
        missing = {r["id"] for r in result["required_additional_rows"] if r["table"] == "spell_template"}
        self.assertEqual(missing, set(range(50001, 50025)))
        self.assertEqual(result["dependency_request"]["spell_ids"], closure["spell_ids"])
        self.assertTrue(any(e["role"] == "skill_step" and e["slot"] == 3 for e in closure["edges"]))

    def test_zero_auto_learn_ability_and_spell_to_skill_reverse_edge_are_retained(self):
        self.files["SkillLineAbility"] = dbc_bytes("SkillLineAbility", [
            {0: 830002, 1: 60003, 2: 930101, 9: 0},
            {0: 830001, 1: 60003, 2: 50001, 3: 0xffffffff, 4: 0xffffffff, 9: 0, 8: 50002}])
        result = self.compile()
        closure = result["player"]["skills"]["dependency_closure"]
        self.assertIn(60003, closure["skill_ids"])
        self.assertTrue({50001, 50002} <= set(closure["spell_ids"]))
        self.assertEqual([r["key"][0] for r in closure["ability_candidates"]], [830001, 830002])
        self.assertTrue(any(r["table"] == "SkillLine" and r["id"] == 60003
                            for r in result["required_additional_rows"]))

    def test_raw_effect_roles_only_skill_and_skill_step_introduce_skills(self):
        spell = self.source["tables"]["spell_template"][0]
        spell.update(Effect1=118, EffectMiscValue1=60003, Effect2=6, EffectMiscValue2=60004)
        rehash_source(self.source)
        self.files = dbcs_for(self.source)
        result = self.compile()
        closure = result["player"]["skills"]["dependency_closure"]
        self.assertIn(60003, closure["skill_ids"])
        self.assertNotIn(60004, closure["skill_ids"])
        effects = result["spells"][0]["effects"]
        self.assertEqual(effects[0]["EffectMiscValue"], 60003)
        self.assertEqual(effects[1]["EffectMiscValue"], 60004)

    def test_client_server_role_disagreement_expands_both_and_stays_blocked(self):
        rows = [{i: w for i, w in enumerate(r.words)} for r in parsed(self.files)["Spell"].rows]
        rows[0].update({61: 44, 106: 60003})
        self.files["Spell"] = dbc_bytes("Spell", rows)
        result = self.compile()
        self.assertIn(60003, result["player"]["skills"]["dependency_closure"]["skill_ids"])
        spell = result["spells"][0]
        self.assertIn("Effect1", spell["server_client_differences"])
        self.assertEqual(spell["server_numeric_words"][61], self.source["tables"]["spell_template"][0]["Effect1"])
        self.assertTrue(any(i["reason"] == "spell.server_client_field_mismatch" for i in result["unsupported"]))

    def test_omitted_learning_coverage_is_not_captured_empty(self):
        self.source.pop("learning_capture")
        for name in ("spell_chain", "spell_learn_spell"):
            self.source["tables"].pop(name)
        rehash_source(self.source)
        result = self.compile()
        closure = result["player"]["skills"]["dependency_closure"]
        self.assertIsNone(closure["sql_relationship_coverage"])
        self.assertIsNone(closure["server_relationships"]["spell_learn_spell"])
        self.assertFalse(closure["captured_inputs_complete"])
        self.assertTrue(any(i["reason"] == "player.server_learning_relationships_not_captured"
                            for i in result["unsupported"]))

    def test_missing_skill_table_and_tier_are_not_complete(self):
        for missing in ("SkillLineAbility", "SkillRaceClassInfo", "SkillTiers", "SpellCastTimes", "SpellRange"):
            files = dict(self.files)
            del files[missing]
            result = compile_starter(self.source, parsed(files), self.request)
            self.assertFalse(result["player"]["skills"]["dependency_closure"]["captured_inputs_complete"])
        for missing in ("SkillTiers", "SpellCastTimes", "SpellRange"):
            files = dict(self.files)
            files[missing] = dbc_bytes(missing, [])
            result = compile_starter(self.source, parsed(files), self.request)
            self.assertFalse(result["player"]["skills"]["dependency_closure"]["captured_inputs_complete"])

    def test_dependency_node_edge_and_ability_budgets_fail_closed(self):
        from native_starter import learning
        for name, limit, error in (("MAX_SKILLS", 1, "skill_limit"), ("MAX_SPELLS", 1, "spell_limit"),
                                   ("MAX_ITEMS", 1, "item_limit"), ("MAX_EDGES", 0, "edge_limit"),
                                   ("MAX_ABILITY_ROWS", 0, "ability_row_limit"),
                                   ("MAX_DBC_REFERENCES", 0, "dbc_reference_limit")):
            with self.subTest(name=name), mock.patch.object(learning, name, limit):
                with self.assertRaisesRegex(ExportError, error):
                    self.compile()

    def test_learning_cycle_terminates_and_is_entry_blocker(self):
        rows = [{i: w for i, w in enumerate(r.words)} for r in parsed(self.files)["Spell"].rows]
        rows += [{0: 50001, 61: 36, 109: 50002}, {0: 50002, 62: 36, 110: 50001}]
        self.files["Spell"] = dbc_bytes("Spell", rows)
        self.files["SkillLineAbility"] = dbc_bytes("SkillLineAbility", [{0: 830001, 1: 60001, 2: 50001}])
        result = self.compile()
        issues = [i for i in result["unsupported"] if i["reason"] == "player.learning_spell_cycle"]
        self.assertEqual(len(issues), 1)
        self.assertEqual(issues[0]["admission_scope"], {"kind": "selection_entry"})

    def test_rank_passive_cannot_be_scoped_to_unavailable_active_cast(self):
        spell = next(r for r in self.source["tables"]["spell_template"] if r["Id"] == 930102)
        spell["Attributes"] = 0x40
        self.source["tables"]["spell_chain"] = [
            {"spell_id": 930101, "prev_spell": 930102, "first_spell": 930102, "rank": 2, "req_spell": 0},
            {"spell_id": 930102, "prev_spell": 0, "first_spell": 930102, "rank": 1, "req_spell": 0}]
        rehash_source(self.source)
        self.files = dbcs_for(self.source)
        result = self.compile()
        self.assertIn(930102, result["ability_admission"]["entry_critical_spell_ids"])
        for issue in result["unsupported"]:
            if issue["table"] == "spell_template" and issue["key"] in ([930102], "930102"):
                self.assertEqual(issue["admission_scope"], {"kind": "selection_entry"})

    def test_zero_ability_row_id_and_reverse_forward_reference_are_preserved(self):
        self.files["SkillLineAbility"] = dbc_bytes("SkillLineAbility", [
            {0: 0, 1: 60003, 2: 50001, 8: 930101}])
        result = self.compile()
        closure = result["player"]["skills"]["dependency_closure"]
        self.assertIn(50001, closure["spell_ids"])
        self.assertIn(60003, closure["skill_ids"])
        self.assertTrue(any(e["from_id"] == 0 and e["role"] == "ability_spell" for e in closure["edges"]))
        self.assertIn(50001, result["ability_admission"]["entry_critical_spell_ids"])

    def test_global_table_scan_state_is_not_inferred_from_selected_empty(self):
        self.assertEqual(self.compile()["player"]["skills"]["dependency_closure"]["derived_learn_spell_scan"],
                         "skipped_for_empty_table")
        self.source["learning_capture"]["global_learn_spell_probe"]["rows"] = [
            {"entry": 50001, "SpellID": 50002, "Active": 1}]
        result = self.compile()
        self.assertEqual(result["player"]["skills"]["dependency_closure"]["derived_learn_spell_scan"],
                         "enabled_by_nonempty_table")
        self.assertEqual(result["player"]["skills"]["dependency_closure"]["server_relationships"]["spell_learn_spell"], [])

    def test_rank_cycle_terminates_and_is_entry_blocker(self):
        self.source["tables"]["spell_chain"] = [
            {"spell_id": 930101, "prev_spell": 930102, "first_spell": 930101, "rank": 1, "req_spell": 0},
            {"spell_id": 930102, "prev_spell": 930101, "first_spell": 930101, "rank": 2, "req_spell": 0}]
        rehash_source(self.source)
        result = self.compile()
        issues = [i for i in result["unsupported"] if i["reason"] == "player.learning_rank_cycle"]
        self.assertEqual(len(issues), 1)
        self.assertEqual(issues[0]["admission_scope"], {"kind": "selection_entry"})

    def test_known_active_skill_effect_is_initialization_sensitive(self):
        spell = next(r for r in self.source["tables"]["spell_template"] if r["Id"] == 930101)
        spell.update(Effect3=118, EffectMiscValue3=60001)
        rehash_source(self.source)
        self.files = dbcs_for(self.source)
        result = self.compile()
        self.assertIn(930101, result["ability_admission"]["learning_sensitive_initial_spell_ids"])
        self.assertFalse(any(r["id"] == 930101 for r in result["ability_admission"]["unavailable_active_abilities"]))

    def test_recapture_can_close_local_dependencies_without_cloud_data(self):
        source = export_tests.starter_source()
        source.inventory = copy.deepcopy(self.source["schema"])
        source.rows = {name: [] for name in source.rows}
        for name, rows in self.source["tables"].items():
            source.rows[name] = [{k: export_tests.Decimal(v["decimal"]) if type(v) is dict else v for k, v in row.items()}
                                 for row in rows]
        source.rows["spell_template"].append(export_tests.row("spell_template", Id=50001,
            Effect1=118, EffectMiscValue1=60003, CastingTimeIndex=700001, rangeIndex=710001))
        self.files["SkillLineAbility"] = dbc_bytes("SkillLineAbility", [{0: 830001, 1: 60001, 2: 50001, 9: 2}])
        self.files["SkillLine"] = dbc_bytes("SkillLine", [{0: 60001}, {0: 60002}, {0: 60003}])
        spell_rows = [{i: w for i, w in enumerate(r.words)} for r in parsed(self.files)["Spell"].rows]
        spell_rows.append({0: 50001, 61: 118, 106: 60003, 18: 700001, 36: 710001})
        self.files["Spell"] = dbc_bytes("Spell", spell_rows)
        first = self.compile()
        self.assertFalse(first["player"]["skills"]["dependency_closure"]["captured_inputs_complete"])
        recaptured = capture(source, export_tests.starter_selection(), dependency_request=first["dependency_request"])
        second = compile_starter(recaptured, parsed(self.files), self.request)
        closure = second["player"]["skills"]["dependency_closure"]
        self.assertTrue(closure["captured_inputs_complete"])
        self.assertEqual(closure["skill_ids"], [60001, 60002, 60003])
        self.assertEqual(closure["server_relationships"]["spell_learn_spell"], [])
        self.assertFalse(second["required_additional_rows"])
        self.assertEqual(second["status"], "blocked")
        self.assertIsNone(second["runtime_export"])

    def test_deterministic_bindings_and_no_mutation(self):
        before = canonical(self.source), dict(self.files)
        a, b = self.compile(), self.compile()
        self.assertEqual(a, b)
        self.assertEqual((canonical(self.source), self.files), before)
        self.assertEqual(a["dependency_request"]["source_sha256"], digest(self.source))
        self.assertTrue(all(len(spell["server_numeric_words"]) == 173 for spell in a["spells"]))
        self.assertEqual(a["spells"][0]["server_numeric_words"][67],
                         self.source["tables"]["spell_template"][0]["EffectBaseDice1"])
        self.assertEqual(a["player"]["skills"]["dependency_closure"]["talent_membership"], "raw_table_captured_effective_loader_unproven")


class TalentInputTests(unittest.TestCase):
    def test_missing_empty_and_unrelated_talent_rows_are_distinct(self):
        from native_starter.talent import inputs
        self.assertIsNone(inputs(None, {900001})["matches"])
        empty = inputs(parse_table("Talent", dbc_bytes("Talent", [])), {900001})
        self.assertEqual(empty["coverage"], "complete_raw_table")
        self.assertEqual(empty["matches"], [])
        table = parse_table("Talent", dbc_bytes("Talent", [{0: 10, 4: 900002}]))
        self.assertEqual(inputs(table, {900001})["matches"], [])
        self.assertEqual(inputs(table, {900001})["row_count"], 1)

    def test_every_real_rank_slot_and_duplicate_membership_is_preserved(self):
        from native_starter.talent import inputs
        rows = [{0: 20, 4: 900001, 8: 900005, 9: 900009}, {0: 10, 5: 900001}]
        value = inputs(parse_table("Talent", dbc_bytes("Talent", rows)), {900001, 900005, 900009})
        self.assertEqual([r["talent_id"] for r in value["matches"]], [10, 20])
        self.assertEqual(value["matches"][1]["rank_slots"], [{"slot": 0, "spell": 900001}, {"slot": 4, "spell": 900005}])
        self.assertEqual(len(value["matches"][1]["source"]["words"]), 21)

    def test_compiler_talent_evidence_is_recomputed(self):
        source = source_bundle(); tables = parsed(dbcs_for(source))
        value = compile_starter(source, tables, Request(1, 1, 0))
        self.assertEqual(value["talent_inputs"]["coverage"], "complete_raw_table")
        value["talent_inputs"]["matches"] = [{"forged": True}]
        with self.assertRaises(ExportError):
            verify_preparation(value, source, tables)

class NativeSpawnPreparationTests(unittest.TestCase):
    setUp = CompilerTests.setUp
    compile = CompilerTests.compile

    def test_complete_empty_is_usable_evidence_without_enabling_runtime(self):
        result = self.compile()
        creature = result["creatures"][0]
        spawn = creature["native_spawn_dependencies"][0]
        self.assertEqual((spawn["guid"], spawn["entry"]), (1, 900001))
        self.assertEqual(spawn["tables"]["game_event_creature"], {"status": "complete", "rows": []})
        self.assertEqual(spawn["tables"]["creature_spawn_data"], {"status": "complete", "rows": []})
        self.assertEqual(spawn["addon"], {"status": "captured_absent", "table": None, "row": None})
        self.assertEqual(spawn["spawn_template"]["selection"], "default_zero")
        self.assertEqual(spawn["spawn_template"]["status"], "captured_row_present")
        self.assertEqual(spawn["spawn_template"]["row"]["NpcFlags"], -1)
        self.assertEqual(spawn["coverage_sha256"], digest(result["native_spawn_capture"]))
        self.assertEqual(spawn["unimplemented_hook_tables"], [])
        self.assertIsNone(creature["resolved_combat_stats"])
        self.assertIsNone(result["runtime_export"])
        reasons = {i["reason"] for i in result["unsupported"]}
        self.assertNotIn("creature.spawn_dependency_coverage_not_proven", reasons)
        self.assertNotIn("spawn.overrides_pools_events_not_captured", reasons)
        self.assertIn("creature.unexported_groups_assistance_context", reasons)
        verify_preparation(result, self.source, parsed(self.files))

    def test_legacy_coverage_is_null_and_cannot_be_empty(self):
        from classic_export.native_spawn import TABLES
        self.source.pop("native_spawn_capture")
        for table in TABLES:
            self.source["tables"].pop(table)
        rehash_source(self.source)
        result = self.compile()
        self.assertIsNone(result["native_spawn_capture"])
        self.assertIsNone(result["creatures"][0]["native_spawn_dependencies"])
        self.assertIn("creature.spawn_dependency_coverage_not_proven", {i["reason"] for i in result["unsupported"]})
        self.assertIn("spawn.overrides_pools_events_not_captured", {i["reason"] for i in result["unsupported"]})

    def test_missing_default_is_captured_absence_not_an_invented_default(self):
        self.source["tables"]["creature_spawn_data_template"] = []
        rehash_source(self.source)
        result = self.compile()
        template = result["creatures"][0]["native_spawn_dependencies"][0]["spawn_template"]
        self.assertEqual(template, {"id": 0, "selection": "default_zero",
                                    "status": "captured_row_absent", "row": None})
        self.assertIn("creature.default_spawn_template_absent", {i["reason"] for i in result["unsupported"]})

    def test_guid_addon_precedence_keeps_shadowed_template_and_neutral_hooks(self):
        row = export_tests.row
        self.source["tables"]["creature_addon"] = [row("creature_addon", guid=1, auras=None)]
        self.source["tables"]["creature_template_addon"] = [row("creature_template_addon", entry=900001,
                                                                    auras="original unparsed aura text")]
        self.source["tables"]["game_event_creature"] = [row("game_event_creature", guid=1, event=-1),
                                                         row("game_event_creature", guid=1, event=0)]
        rehash_source(self.source)
        result = self.compile()
        spawn = result["creatures"][0]["native_spawn_dependencies"][0]
        self.assertEqual(spawn["addon"]["table"], "creature_addon")
        self.assertIsNone(spawn["addon"]["row"]["auras"])
        self.assertEqual(len(spawn["tables"]["creature_template_addon"]["rows"]), 1)
        self.assertEqual(spawn["unimplemented_hook_tables"], ["game_event_creature", "creature_addon"])
        issue = next(i for i in result["unsupported"] if i["reason"] == "creature.applicable_spawn_hooks_require_execution")
        self.assertEqual(issue["tables"], ["game_event_creature", "creature_addon"])
        self.source["tables"]["creature_addon"] = []
        rehash_source(self.source)
        spawn = self.compile()["creatures"][0]["native_spawn_dependencies"][0]
        self.assertEqual(spawn["addon"]["table"], "creature_template_addon")
        self.assertEqual(spawn["addon"]["row"]["auras"], "original unparsed aura text")

    def test_explicit_override_row_and_zero_binding_preserve_native_selection(self):
        template = dict(self.source["tables"]["creature_spawn_data_template"][0], Entry=19, CurHealth=11,
                        SpawnFlags=4, RelayId=77, StringId=88)
        self.source["tables"]["creature_spawn_data_template"].append(template)
        self.source["tables"]["creature_spawn_data"] = [export_tests.row("creature_spawn_data", Guid=1, Id=19)]
        rehash_source(self.source)
        spawn = self.compile()["creatures"][0]["native_spawn_dependencies"][0]
        self.assertEqual(spawn["spawn_template"]["row"], template)
        self.assertEqual(spawn["spawn_template"]["selection"], "guid_binding")
        self.source["tables"]["creature_spawn_data"][0]["Id"] = 0
        self.source["tables"]["creature_spawn_data_template"].pop()
        rehash_source(self.source)
        spawn = self.compile()["creatures"][0]["native_spawn_dependencies"][0]
        self.assertEqual(spawn["spawn_template"]["id"], 0)
        self.assertEqual(spawn["spawn_template"]["selection"], "default_zero")

    def test_selected_override_model_row_reaches_native_preparation(self):
        model = dict(self.source["tables"]["creature_model_info"][0], modelid=940099)
        self.source["tables"]["creature_model_info"].append(model)
        self.source["required_ids"]["creature_model_info"].append(940099)
        self.source["tables"]["creature_spawn_data_template"][0]["ModelId"] = 940099
        rehash_source(self.source)
        creature = self.compile()["creatures"][0]
        self.assertEqual([r["modelid"] for r in creature["model_inputs"]], [940001, 940099])
        self.assertEqual(creature["native_spawn_dependencies"][0]["spawn_template"]["row"]["ModelId"], 940099)

    def test_forged_absence_rejects_even_with_rehashed_preparation(self):
        self.source["tables"]["creature_addon"] = [export_tests.row("creature_addon", guid=1)]
        rehash_source(self.source)
        result = self.compile()
        result["creatures"][0]["native_spawn_dependencies"][0]["addon"] = {
            "status": "captured_absent", "table": None, "row": None}
        result.pop("preparation_sha256")
        result["preparation_sha256"] = digest(result)
        with self.assertRaises(ExportError):
            verify_preparation(result, self.source, parsed(self.files))



if __name__ == "__main__":
    unittest.main()
