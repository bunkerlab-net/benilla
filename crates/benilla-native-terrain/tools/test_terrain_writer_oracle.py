#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Original fixture/integrity tests; no install, Rust build or extractor process."""
import functools
import json
from pathlib import Path
import struct
import tempfile
import unittest

import generate_terrain_writer_fixture as oracle


FIXTURES = Path(__file__).resolve().parent / "fixtures"


class TerrainWriterOracle(unittest.TestCase):
    @functools.cache
    def read_map(self, name):
        return (FIXTURES / (name + ".map")).read_bytes()

    @functools.cache
    def header(self, name):
        return struct.unpack_from("<10I", self.read_map(name))

    def test_manifest_covers_every_file_and_authored_input(self):
        manifest = json.loads((FIXTURES / "manifest.json").read_text())
        self.assertEqual(manifest["source_revision"], oracle.PIN)
        self.assertEqual(manifest["source_hashes"], oracle.HASHES)
        names = {"manifest.json"}
        for name, raw in oracle.authored_common().items():
            name = oracle.fixture_name(name)
            self.assertEqual((FIXTURES / name).read_bytes(), raw)
            self.assertEqual(manifest["common_files"][name],
                             {"bytes": len(raw), "sha256": oracle.sha(raw)})
            names.add(name)
        self.assertEqual([case["name"] for case in manifest["cases"]], list(oracle.CASES))
        for case in manifest["cases"]:
            for suffix in ("adt", "map"):
                name = oracle.fixture_name(case["name"] + "." + suffix)
                raw = (FIXTURES / name).read_bytes()
                self.assertEqual(case[suffix + "_bytes"], len(raw))
                self.assertEqual(case[suffix + "_sha256"], oracle.sha(raw))
                if suffix == "adt":
                    self.assertEqual(raw, oracle.authored_adt(case["name"]))
                names.add(name)
        self.assertEqual(names, {p.name for p in FIXTURES.iterdir() if p.is_file()})
        self.assertFalse(any(name.endswith((".adt", ".dbc", ".wdt")) for name in names))

    def test_dry_layout_and_flat_compression(self):
        for name in oracle.DRY_CASES:
            raw = self.read_map(name)
            header = self.header(name)
            self.assertEqual(raw[:8], b"MAPSz1.4")
            self.assertEqual(header[2], 40)
            self.assertEqual(header[4], header[2] + header[3])
            self.assertEqual(header[6:8], (0, 0))
            self.assertEqual(header[8], header[4] + header[5])
            self.assertEqual(header[9], 512)
            self.assertEqual(header[8] + header[9], len(raw))
        raw = self.read_map("flat")
        self.assertEqual(len(raw), 576)
        self.assertEqual(raw[40:48], b"AREA\x01\x00\x34\x12")
        self.assertEqual(struct.unpack_from("<Iff", raw, 52), (1, 7.25, 7.25))
        self.assertEqual(self.header("tiny_range")[5], 16 + (129 * 129 + 128 * 128) * 4)

    def test_interleaving_and_later_cell_seam_ownership(self):
        raw = self.read_map("seams")
        at = self.header("seams")[4] + 16
        v9 = lambda y, x: struct.unpack_from("<f", raw, at + (y * 129 + x) * 4)[0]
        self.assertEqual(v9(0, 8), 10.0)
        self.assertEqual(v9(8, 0), 160.0)
        self.assertEqual(v9(8, 8), 170.0)
        self.assertEqual(v9(128, 128), 2568.0)
        v8_at = at + 129 * 129 * 4
        self.assertEqual(struct.unpack_from("<f", raw, v8_at)[0], 1.125)

    def test_clamp_and_original_extrema_seed(self):
        raw = self.read_map("clamp_flat")
        self.assertEqual(struct.unpack_from("<Iff", raw, 52), (1, -500.0, -500.0))
        raw = self.read_map("clamp_mixed")
        at = self.header("clamp_mixed")[4]
        self.assertEqual(struct.unpack_from("<f", raw, at + 8)[0], -500.0)
        heights = struct.unpack_from("<33025f", raw, at + 16)
        self.assertEqual(min(heights), -500.0)
        self.assertGreater(max(heights), -500.0)
        raw = self.read_map("upper_sentinel")
        self.assertEqual(struct.unpack_from("<Iff", raw, 52), (0, 20000.0, 25255.0))

    def test_full_area_row_order_narrowing_and_fallback(self):
        raw = self.read_map("area_full")
        self.assertEqual(self.header("area_full")[3], 520)
        self.assertEqual(raw[40:48], b"AREA\0\0\0\0")
        flags = struct.unpack_from("<256H", raw, 48)
        cycle = [0xffff, 0x1234, 2, 0xffff, 0, 0xfffe, 0x1234, 0xffff, 0xffff]
        self.assertEqual(flags, tuple(cycle[i % 9] for i in range(256)))
        self.assertEqual(self.read_map("area_fallback")[40:48], b"AREA\x01\0\xff\xff")

    def test_holes_preserve_all_masks_in_flat_and_float_output(self):
        expected = tuple((1 << (i % 16)) if i < 128 else ((i * 257) & 0xffff)
                         for i in range(256))
        for case in ("holes", "nonflat_holes"):
            self.assertEqual(struct.unpack_from("<256H", self.read_map(case),
                                               self.header(case)[8]), expected)
        self.assertEqual(self.header("holes")[5], 16)
        self.assertGreater(self.header("nonflat_holes")[5], 16)

    def test_missing_mcvt_preserves_base_and_signed_zero(self):
        manifest = json.loads((FIXTURES / "manifest.json").read_text())
        for case in manifest["cases"]:
            self.assertEqual(case["rust_supported"], case["name"] not in oracle.ORACLE_ONLY_CASES)
        self.assertEqual(self.read_map("missing_mcvt"), self.read_map("flat"))
        self.assertEqual(struct.unpack_from("<III", self.read_map("missing_mcvt_signed_zero"),
                                           52), (1, 0x80000000, 0x80000000))

    @functools.cache
    def liquid_header(self, name):
        return struct.unpack_from("<4sBBHBBBBf", self.read_map(name), self.header(name)[6])

    def liquid_types(self, name):
        at = self.header(name)[6] + 16
        raw = self.read_map(name)
        return struct.unpack_from("<256H", raw, at), raw[at + 512:at + 768]

    def liquid_height(self, name, y, x):
        header = self.liquid_header(name)
        _, flags, _, _, offset_x, offset_y, width, height, level = header
        self.assertTrue(offset_x <= x < offset_x + width)
        self.assertTrue(offset_y <= y < offset_y + height)
        if flags & 2:
            return level
        at = self.header(name)[6] + 16 + (0 if flags & 1 else 768)
        return struct.unpack_from("<f", self.read_map(name),
                                  at + ((y - offset_y) * width + x - offset_x) * 4)[0]

    def liquid_sample_offset(self, name, y, x):
        _, flags, _, _, offset_x, offset_y, width, height, _ = self.liquid_header(name)
        self.assertFalse(flags & 2)
        self.assertTrue(offset_x <= x < offset_x + width)
        self.assertTrue(offset_y <= y < offset_y + height)
        return (self.header(name)[6] + 16 + (0 if flags & 1 else 768)
                + ((y - offset_y) * width + x - offset_x) * 4)

    def liquid_sample_bits(self, name, y, x):
        return struct.unpack_from("<I", self.read_map(name),
                                  self.liquid_sample_offset(name, y, x))[0]

    def input_liquid_cells(self, name):
        """Inspect complete authored fixture bytes, independently of fixture helpers."""
        raw = (FIXTURES / (name + ".adt.test-fixture")).read_bytes()
        cells = []
        for slot in range(256):
            at = struct.unpack_from("<I", raw, 92 + slot * 16)[0]
            self.assertEqual(raw[at:at + 4], b"KNCM")
            offset, size = struct.unpack_from("<II", raw, at + 8 + 96)
            if not offset or size <= 8:
                continue
            mclq = raw[at + offset:at + offset + size]
            self.assertEqual(mclq[:8], b"QLCM" + struct.pack("<I", 804))
            self.assertEqual(len(mclq), 812)
            flags = struct.unpack_from("<I", raw, at + 8)[0]
            cells.append((slot // 16, slot % 16, flags, mclq))
        return cells

    def test_liquid_section_sizes_and_holes_follow_exact_payload(self):
        for name in oracle.CASES:
            with self.subTest(name=name):
                header = self.header(name)
                self.assertEqual(header[8] + header[9], len(self.read_map(name)))
                self.assertEqual(header[9], 512)
                if not header[6]:
                    self.assertEqual(header[7], 0)
                    self.assertEqual(header[8], header[4] + header[5])
                    continue
                fourcc, flags, _, _, x, y, width, height, _ = self.liquid_header(name)
                self.assertEqual(fourcc, b"MLIQ")
                self.assertEqual(flags & ~3, 0)
                self.assertLessEqual(x + width, 129)
                self.assertLessEqual(y + height, 129)
                self.assertGreater(width, 1)
                self.assertGreater(height, 1)
                expected = 16 + (0 if flags & 1 else 768)
                expected += 0 if flags & 2 else width * height * 4
                self.assertEqual(header[6], header[4] + header[5])
                self.assertEqual(header[7], expected)
                self.assertEqual(header[8], header[6] + expected)

    def test_liquid_absence_empty_small_sizes_and_untyped_are_dry(self):
        for name in ("liquid_absent_flags", "liquid_empty", "liquid_size8", "liquid_untyped"):
            self.assertEqual(self.read_map(name), self.read_map("flat"))
        self.assertEqual({oracle.authored_liquid("liquid_size8", row, col)[2]
                          for row in range(16) for col in range(16)}, set(range(9)))
        self.assertEqual(oracle.authored_liquid("liquid_empty", 0, 0),
                         (28, b"QLCM" + bytes(4), 8))
        _, raw, size = oracle.authored_liquid("liquid_zero_size", 0, 0)
        self.assertEqual(size, 812)
        self.assertEqual(len(raw), 812)
        self.assertEqual(raw[:8], b"QLCM" + bytes(4))
        self.assertEqual(self.read_map("liquid_zero_size"), self.read_map("liquid_water_flat"))

    def test_liquid_flat_uniform_and_full_type_storage(self):
        for name, liquid_flags, entry, level in (
                ("liquid_water_flat", 8, 1, 23.5),
                ("liquid_ocean_flat", 18, 2, -17.25),
                ("liquid_magma_flat", 1, 3, 31.75)):
            self.assertEqual(self.liquid_header(name),
                             (b"MLIQ", 3, liquid_flags, entry, 0, 0, 129, 129, level))
            self.assertEqual(self.header(name)[7], 16)
        self.assertEqual(self.liquid_header("liquid_full_types"),
                         (b"MLIQ", 2, 0, 0, 0, 0, 129, 129, 13.25))
        entries, flags = self.liquid_types("liquid_full_types")
        entry_cycle = [1, 2, 3, 2, 3, 3, 3]
        flag_cycle = [8, 2, 1, 10, 9, 3, 11]
        self.assertEqual(entries, tuple(entry_cycle[i % 7] for i in range(256)))
        self.assertEqual(flags, bytes(flag_cycle[i % 7] for i in range(256)))

    def test_liquid_whole_byte_visibility_and_deep_only(self):
        for name, expected_entry, expected_flags in (
                ("liquid_flag_bytes", 1, 0x18), ("liquid_deep_only", 0, 0x10)):
            self.assertEqual(self.liquid_header(name),
                             (b"MLIQ", 0, 0, 0, 17, 10, 7, 2, -500.0))
            entries, flags = self.liquid_types(name)
            self.assertEqual(entries[18], expected_entry)
            self.assertEqual(flags[18], expected_flags)
            self.assertFalse(any(entries[:18] + entries[19:]))
            self.assertFalse(any(flags[:18] + flags[19:]))
            for x in range(17, 23):
                self.assertEqual(self.liquid_height(name, 10, x), 44.0 + (x - 16) / 4.0)
            self.assertEqual(self.liquid_height(name, 10, 23), -500.0)
            for x in range(17, 24):
                self.assertEqual(self.liquid_height(name, 11, x), -500.0)
        _, raw, _ = oracle.authored_liquid("liquid_flag_bytes", 1, 2)
        tile_flags = raw[664:728]
        self.assertEqual(tile_flags[16:24], bytes([0x0f, 0, 0x1f, 0x8f, 0xff, 0x80, 0x40, 0x0f]))

    def test_liquid_nonsymmetric_crop_masks_hidden_samples(self):
        name = "liquid_nonsymmetric"
        self.assertEqual(self.liquid_header(name),
                         (b"MLIQ", 0, 0, 0, 57, 26, 6, 4, -500.0))
        for y in range(26, 30):
            for x in range(57, 63):
                expected = -500.0 if x == 62 or y == 29 else 100.0 + (y - 24) * 2 + (x - 56) / 4
                self.assertEqual(self.liquid_height(name, y, x), expected)
        entries, flags = self.liquid_types(name)
        self.assertEqual(entries[3 * 16 + 7], 1)
        self.assertEqual(flags[3 * 16 + 7], 8)

    def test_liquid_shared_seams_and_current_tile_outer_edge_heights(self):
        name = "liquid_seams"
        self.assertEqual(self.liquid_header(name),
                         (b"MLIQ", 1, 8, 1, 0, 0, 129, 129, 0.0))
        for y, x, expected in ((0, 8, 10.0), (8, 0, 160.0),
                               (8, 8, 170.0), (128, 128, 2568.0)):
            self.assertEqual(self.liquid_height(name, y, x), expected)
        name = "liquid_outer_edges"
        self.assertEqual(self.liquid_header(name),
                         (b"MLIQ", 0, 0, 0, 121, 122, 8, 7, -500.0))
        for y, x, expected in ((122, 121, 404.25), (122, 128, 406.0),
                               (128, 121, 416.25), (128, 128, 418.0)):
            self.assertEqual(self.liquid_height(name, y, x), expected)

    def test_source_history_changes_only_unwritten_serialized_outer_edges(self):
        first, second = "liquid_stale_edge_a", "liquid_stale_edge_b"
        self.assertEqual(oracle.authored_adt(first), oracle.authored_adt(second))
        self.assertNotEqual(self.read_map(first), self.read_map(second))
        self.assertEqual(self.liquid_header(first), self.liquid_header(second))
        self.assertEqual(self.liquid_types(first), self.liquid_types(second))
        self.assertEqual(self.liquid_header(first),
                         (b"MLIQ", 0, 0, 0, 29, 19, 100, 110, -500.0))
        changed = 0
        for y in range(19, 129):
            for x in range(29, 129):
                a, b = self.liquid_height(first, y, x), self.liquid_height(second, y, x)
                if a != b:
                    changed += 1
                    self.assertTrue(x == 128 or y == 128)
                    self.assertEqual((a, b), (23.5, -17.25))
        self.assertEqual(changed, 199)
        for name, seed in ((first, "liquid_water_flat"), (second, "liquid_ocean_flat")):
            manifest = json.loads((FIXTURES / "manifest.json").read_text())
            case = next(case for case in manifest["cases"] if case["name"] == name)
            self.assertFalse(case["rust_supported"])
            self.assertEqual(case["source_history"], [seed])
            self.assertIn("history-dependent", case["rejection_reason"])

    def test_sparse_flat_omits_unwritten_outer_edge_heights(self):
        name = "liquid_sparse_flat"
        self.assertEqual(self.liquid_header(name),
                         (b"MLIQ", 2, 0, 0, 29, 19, 100, 110, -625.0))
        self.assertEqual(self.header(name)[7], 784)
        self.assertEqual(self.liquid_height(name, 128, 128), -625.0)
        manifest = json.loads((FIXTURES / "manifest.json").read_text())
        self.assertEqual(manifest["history_verification"]["admitted_cases"],
                         list(oracle.DRY_CASES | oracle.LIQUID_CASES))
        self.assertEqual(manifest["history_verification"]["independent_predecessors"],
                         ["liquid_water_flat", "liquid_ocean_flat"])

    def test_exact61_authored_shape_and_missing_boundary_are_independent_of_history(self):
        first = "liquid_history61_initial"
        cells = self.input_liquid_cells(first)
        self.assertEqual([(row, col) for row, col, _, _ in cells],
                         [(0, 5), (0, 15), (15, 5), (15, 6), (15, 7)])
        visible = {(col * 8 + x, row * 8 + y)
                   for row, col, _, mclq in cells for y in range(8) for x in range(8)
                   if mclq[664 + y * 8 + x] != 0x0f}
        self.assertEqual(visible, {(47, 0), (124, 0), (47, 127)})
        writes = {(col * 8 + x, row * 8 + y)
                  for row, col, _, _ in cells for y in range(9) for x in range(9)}
        missing = [(x, y) for y in range(129) for x in range(47, 126)
                   if (x == 128 or y == 128) and (x, y) not in writes]
        self.assertEqual(missing, [(x, 128) for x in range(65, 126)])
        self.assertEqual(len(missing), 61)
        for name in oracle.HISTORY_CASES:
            self.assertEqual(oracle.authored_adt(first), oracle.authored_adt(name))
            self.assertEqual(self.liquid_header(name),
                             (b"MLIQ", 0, 0, 0, 47, 0, 79, 129, -500.0))
            self.assertEqual(self.header(name)[7], 16 + 768 + 79 * 129 * 4)
            for y, x, value in ((0, 47, 106.75), (0, 124, 116.0),
                                (127, 47, 270.75), (127, 48, -500.0),
                                (128, 47, 272.75), (128, 48, 272.0),
                                (128, 55, 273.75), (128, 56, 273.0),
                                (128, 64, 275.0)):
                self.assertEqual(self.liquid_height(name, y, x), value)
            canonical = bytearray(self.read_map(name))
            for x, y in missing:
                at = self.liquid_sample_offset(name, y, x)
                canonical[at:at + 4] = bytes(4)
            self.assertEqual(bytes(canonical), self.read_map(first))

    def test_exact61_fresh_process_and_complete_direct_histories(self):
        expected = {"initial": 0.0, "positive": 23.5, "negative": -17.25,
                    "dry": -17.25, "untyped": 23.5, "hidden": -42.75}
        for suffix, value in expected.items():
            name = "liquid_history61_" + suffix
            bits = struct.unpack("<I", struct.pack("<f", value))[0]
            for x in range(65, 126):
                self.assertEqual(self.liquid_sample_bits(name, 128, x), bits)
        self.assertEqual(self.read_map("liquid_history61_positive"),
                         self.read_map("liquid_history61_untyped"))
        self.assertEqual(self.read_map("liquid_history61_negative"),
                         self.read_map("liquid_history61_dry"))

    def test_hidden_untyped_source_writes_exist_despite_dry_products(self):
        for name in oracle.HISTORY_SEED_CASES:
            self.assertEqual(self.read_map(name), self.read_map("flat"))
            for _, _, flags, mclq in self.input_liquid_cells(name):
                self.assertEqual(flags, 0)
                self.assertEqual(mclq[664:728], bytes([0x0f]) * 64)
        self.assertEqual(len(self.input_liquid_cells("liquid_history_hidden_seed")), 256)
        cells = self.input_liquid_cells("liquid_history_partial_seed")
        self.assertEqual([(row, col) for row, col, _, _ in cells], [(15, 9), (15, 11)])
        for _, col, _, mclq in cells:
            bits = 0x80000000 if col == 9 else struct.unpack("<I", struct.pack("<f", -71.125))[0]
            self.assertEqual([struct.unpack_from("<I", mclq, 20 + sample * 8)[0]
                              for sample in range(81)], [bits] * 81)

    def test_exact61_predecessor_seams_and_partial_last_writers(self):
        for x in range(65, 126):
            expected = (240 + x // 8) * 10.0 + 16.0 + (x % 8) / 4.0
            self.assertEqual(self.liquid_height("liquid_history61_seams", 128, x), expected)
            if 72 <= x <= 80:
                bits = 0x80000000
            else:
                value = -71.125 if 88 <= x <= 96 else -17.25
                bits = struct.unpack("<I", struct.pack("<f", value))[0]
            self.assertEqual(self.liquid_sample_bits("liquid_history61_partial", 128, x), bits)
        self.assertEqual(self.liquid_height("liquid_history61_seams", 128, 72), 2506.0)

    def test_exact61_manifest_records_explicit_proven_history_requirement(self):
        manifest = json.loads((FIXTURES / "manifest.json").read_text())
        closure = manifest["history_closure"]
        self.assertEqual(closure["cases"], list(oracle.HISTORY_CASES))
        self.assertEqual(closure["history_independent_seeds"], list(oracle.HISTORY_SEED_CASES))
        self.assertEqual(closure["authored_shape"],
                         {"offset_x": 47, "offset_y": 0, "width": 79, "height": 129,
                          "missing_samples": 61, "first_missing": [65, 128],
                          "last_missing": [125, 128]})
        self.assertEqual(oracle.SOURCE_HISTORIES["liquid_history61_initial"], [])
        for case in manifest["cases"]:
            if case["name"] in oracle.HISTORY_CASES:
                self.assertTrue(case["requires_proven_history"])
                self.assertTrue(case["rust_supported"])
                self.assertEqual(case["source_history"], oracle.SOURCE_HISTORIES[case["name"]])

    def test_source_hash_rejects_drift_before_compilation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = root / oracle.SYSTEM
            first.parent.mkdir(parents=True)
            first.write_bytes(b"altered source")
            with self.assertRaisesRegex(ValueError, "pinned source hash mismatch"):
                oracle.extract(root)

    def test_comparison_rejects_corruption_and_inventory_drift(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            expected, actual = root / "expected", root / "actual"
            expected.mkdir()
            actual.mkdir()
            (expected / "vector").write_bytes(b"MAPS")
            (actual / "vector").write_bytes(b"MAPS")
            oracle.compare(expected, actual)
            (actual / "vector").write_bytes(b"MAPX")
            with self.assertRaisesRegex(ValueError, "fixture bytes differ"):
                oracle.compare(expected, actual)
            (actual / "extra").write_bytes(b"")
            with self.assertRaisesRegex(ValueError, "inventory differs"):
                oracle.compare(expected, actual)


if __name__ == "__main__":
    unittest.main()
