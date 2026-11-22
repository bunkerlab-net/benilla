#!/usr/bin/env python3
"""Independent source-retention/input guards; no compiler or game data required.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import generate_creature_swimming_fixture as oracle


class SourceGuards(unittest.TestCase):
    def test_retained_source_hashes(self):
        oracle.checked_sources()

    def test_each_fragment_mutation_rejects(self):
        manifest = json.loads((oracle.SOURCE / "source_manifest.json").read_bytes())
        for name in manifest["files"]:
            with self.subTest(fragment=name), tempfile.TemporaryDirectory() as directory:
                retained = Path(directory) / "source"
                shutil.copytree(oracle.SOURCE, retained)
                path = retained / name
                path.write_bytes(path.read_bytes() + b"\n")
                with self.assertRaisesRegex(SystemExit, "pinned source hash mismatch"):
                    oracle.checked_sources(retained=retained)

    def test_manifest_mutation_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            retained = Path(directory) / "source"
            shutil.copytree(oracle.SOURCE, retained)
            path = retained / "source_manifest.json"
            path.write_bytes(path.read_bytes() + b"\n")
            with self.assertRaisesRegex(SystemExit, "manifest hash mismatch"):
                oracle.checked_sources(retained=retained)

    def test_missing_source_rejects(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(FileNotFoundError):
                oracle.checked_sources(upstream=Path(directory))

    def test_source_normalizes_only_crlf_and_rejects_changed_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "source.cpp"
            expected = b"source line\nsecond line\n"
            digest = hashlib.sha256(expected).hexdigest()
            path.write_bytes(expected.replace(b"\n", b"\r\n"))
            self.assertEqual(oracle.checked_upstream_file(root, path.name, digest), expected)
            for changed in [expected + b" ", expected.replace(b"\n", b"\r")]:
                path.write_bytes(changed)
                with self.assertRaisesRegex(SystemExit, "upstream source hash mismatch"):
                    oracle.checked_upstream_file(root, path.name, digest)

    def test_input_coverage_and_no_duplicate_cases(self):
        commands = list(oracle.commands())
        self.assertEqual(len(commands), len(set(commands)))
        self.assertEqual(sum(c.startswith("S ") for c in commands), 2432)
        self.assertEqual(sum(c.startswith("I ") for c in commands), 2648)
        cases = set(oracle.scalar_cases())
        for offset in (-1, 0, 1):
            self.assertIn((0x40000000 + offset, 0, 0x40000000), cases)
            self.assertIn((0x4002007e + offset, 0, 0x4002007e), cases)
        self.assertIn((0x00800001, 0x00800000, 0), cases)
        for habitat in range(1, 8):
            self.assertTrue(any(c.startswith(f"I {habitat} ") for c in commands))

    def test_fixture_corruption_rejects_without_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "oracle.test-fixture"
            expected = b"S 1 0 0 0\n"
            for changed in [b"", expected[:-1], expected.replace(b" 1 ", b" 0 "), expected + b" "]:
                target.write_bytes(changed)
                with self.assertRaisesRegex(SystemExit, "fixture mismatch"):
                    oracle.record_fixture(target, expected, True)
                self.assertEqual(target.read_bytes(), changed)
            target.write_bytes(expected.replace(b"\n", b"\r\n"))
            oracle.record_fixture(target, expected, True)

    def test_source_order_and_default_argument_are_guarded_separately(self):
        manifest = json.loads((oracle.SOURCE / "source_manifest.json").read_bytes())
        order = manifest["reviewed_source_order"]["ordered_markers"]
        self.assertLess(order.index("SetNativeDisplayId(display_id);"),
                        next(i for i, marker in enumerate(order) if "IsSwimmable(" in marker))
        self.assertLess(next(i for i, marker in enumerate(order) if "IsSwimmable(" in marker),
                        order.index("SelectLevel();"))
        declaration = (oracle.SOURCE / "LiquidStatusDeclaration.inc").read_text()
        self.assertIn("float collisionHeight = 2.03128f", declaration)
        body = (oracle.SOURCE / "IsSwimmable.inc").read_text()
        self.assertIn("getLiquidStatus(x, y, z, MAP_ALL_LIQUIDS, liquid_ptr)", body)
        self.assertIn("liquid_ptr->level - liquid_ptr->depth_level > radius", body)
        self.assertIn("m_movementInfo.AddMovementFlag(MOVEFLAG_SWIMMING)",
                      (oracle.SOURCE / "InitSwimming.inc").read_text())


if __name__ == "__main__":
    unittest.main()
