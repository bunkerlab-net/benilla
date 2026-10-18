#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Original fixture/integrity tests; no install, Rust build or extractor process."""
import json
from pathlib import Path
import struct
import tempfile
import unittest

import generate_terrain_writer_fixture as oracle


FIXTURES = Path(__file__).resolve().parent / "fixtures"


class TerrainWriterOracle(unittest.TestCase):
    def read_map(self, name):
        return (FIXTURES / (name + ".map")).read_bytes()

    def header(self, name):
        return struct.unpack_from("<10I", self.read_map(name))

    def test_manifest_covers_every_file_and_authored_input(self):
        manifest = json.loads((FIXTURES / "manifest.json").read_text())
        self.assertEqual(manifest["source_revision"], oracle.PIN)
        self.assertEqual(manifest["source_hashes"], oracle.HASHES)
        names = {"manifest.json"}
        for name, raw in oracle.authored_common().items():
            self.assertEqual((FIXTURES / name).read_bytes(), raw)
            self.assertEqual(manifest["common_files"][name],
                             {"bytes": len(raw), "sha256": oracle.sha(raw)})
            names.add(name)
        self.assertEqual([case["name"] for case in manifest["cases"]], list(oracle.CASES))
        for case in manifest["cases"]:
            for suffix in ("adt", "map"):
                name = case["name"] + "." + suffix
                raw = (FIXTURES / name).read_bytes()
                self.assertEqual(case[suffix + "_bytes"], len(raw))
                self.assertEqual(case[suffix + "_sha256"], oracle.sha(raw))
                if suffix == "adt":
                    self.assertEqual(raw, oracle.authored_adt(case["name"]))
                names.add(name)
        self.assertEqual(names, {p.name for p in FIXTURES.iterdir() if p.is_file()})

    def test_dry_layout_and_flat_compression(self):
        for name in oracle.CASES:
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
        self.assertTrue(all(case["rust_supported"] for case in manifest["cases"]))
        self.assertEqual(self.read_map("missing_mcvt"), self.read_map("flat"))
        self.assertEqual(struct.unpack_from("<III", self.read_map("missing_mcvt_signed_zero"),
                                           52), (1, 0x80000000, 0x80000000))

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
