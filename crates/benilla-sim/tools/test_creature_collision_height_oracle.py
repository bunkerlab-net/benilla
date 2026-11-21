#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Source-extraction negative tests; do not run or build a compiler."""
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch
import generate_creature_collision_height_fixture as oracle


class SourceGuards(unittest.TestCase):
    def test_changed_hash_fails(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / next(iter(oracle.HASHES))
            path.parent.mkdir(parents=True)
            path.write_bytes(b"changed source\n")
            with self.assertRaisesRegex(SystemExit, "pinned source hash mismatch"):
                oracle.source(Path(root))

    def test_lf_normalization_preserves_matching_capture(self):
        import hashlib
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "Unit.cpp"
            path.write_bytes(b"first\r\nsecond\r\n")
            expected = hashlib.sha256(b"first\nsecond\n").hexdigest()
            with patch.dict(oracle.HASHES, {"Unit.cpp": expected}, clear=True):
                self.assertEqual(oracle.source(Path(root))["Unit.cpp"], "first\nsecond\n")

    def test_method_is_copied_unchanged(self):
        method = "float Unit::GetCollisionHeight() const\n{\n return scale * height;\n}\n"
        self.assertEqual(oracle.extracted_method("prefix\n" + method + "\nfloat Unit::GetCollisionWidth() const {}"), method)

    def test_getter_is_copied_unchanged(self):
        getter = "        float GetObjectScale() const\n        {\n            return stored ? stored : DEFAULT_OBJECT_SCALE;\n        }\n"
        self.assertEqual(oracle.extracted_getter("prefix\n" + getter + "        virtual float GetNativeScale() {}"), getter)

    def test_model_is_copied_unchanged(self):
        model = "struct CreatureModelDataEntry\n{\n    float CollisionHeight;\n};\n"
        self.assertEqual(oracle.extracted_model("prefix\n" + model + "\n#define MAX_CREATURE_SPELL_DATA_SLOT 4"), model)

    def test_missing_method_fails(self):
        with self.assertRaises(ValueError):
            oracle.extracted_method("missing method")

    def test_missing_default_fails(self):
        with self.assertRaisesRegex(SystemExit, "missing source constant"):
            oracle.harness({"ObjectDefines.h": "missing default"})

    def test_missing_getter_or_model_fails(self):
        for extract in [oracle.extracted_getter, oracle.extracted_model]:
            with self.assertRaises(ValueError):
                extract("missing source")

    def test_missing_format_fails(self):
        with self.assertRaisesRegex(SystemExit, "missing source model-data format"):
            oracle.extracted_format("missing format")

    def test_corrupt_or_truncated_fixture_fails_without_overwriting(self):
        with tempfile.TemporaryDirectory() as root:
            target = Path(root) / "numeric.test-fixture"
            expected = b"1065353216 0 0 1073873022\n"
            for corrupted in [b"", b"malformed", expected[:-2], expected.replace(b" 0 ", b" 1 ")]:
                target.write_bytes(corrupted)
                with self.assertRaisesRegex(SystemExit, "fixture differs"):
                    oracle.record_fixture(target, expected, True)
                self.assertEqual(target.read_bytes(), corrupted)
            target.write_bytes(expected)
            oracle.record_fixture(target, expected, True)


if __name__ == "__main__":
    unittest.main()
