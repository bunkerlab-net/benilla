#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fail-closed extraction and fixture checks; no compiler or game data needed."""
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import generate_chase_gate_fixture as oracle


class SourceGuards(unittest.TestCase):
    def test_changed_hash_fails(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / next(iter(oracle.HASHES))
            path.parent.mkdir(parents=True)
            path.write_bytes(b"changed source\n")
            with self.assertRaisesRegex(SystemExit, "pinned source hash mismatch"):
                oracle.source(Path(root))

    def test_lf_normalization_preserves_matching_capture(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "Object.cpp"
            path.write_bytes(b"first\r\nsecond\r\n")
            expected = hashlib.sha256(b"first\nsecond\n").hexdigest()
            with patch.dict(oracle.HASHES, {"Object.cpp": expected}, clear=True):
                self.assertEqual(oracle.source(Path(root))["Object.cpp"], "first\nsecond\n")

    def test_method_and_enum_are_copied_unchanged(self):
        for fragment in ["bool Unit::IsInWater() const\n{\n if (ready) { return true; }\n return false;\n}",
                         "enum DistanceCalculation\n{\n DIST_CALC_NONE,\n};"]:
            signature = fragment.split("\n")[0]
            self.assertEqual(oracle.block("prefix\n" + fragment + "\nnext", signature), fragment)

    def test_defaults_and_inline_player_predicate_are_copied_unchanged(self):
        for line in ["        bool IsWithinLOS(float x, float y, float z, bool ignore = false) const;",
                     "        bool IsInWater() const override { return (m_environmentFlags & ENVIRONMENT_FLAG_IN_WATER); }"]:
            self.assertEqual(oracle.declaration("prefix\n" + line + "\nnext", "bool Is"), line)

    def test_chase_condition_is_unchanged_and_scoped_to_original_method(self):
        line = "    if (owner.IsWithinDist3d(x, y, z, 200.f) && std::abs(owner.GetPositionZ() - z) < 5.f && owner.IsWithinLOS(x, y, z + i_target->GetCollisionHeight()) && !owner.IsInWater() && !i_target->IsInWater())"
        text = "bool ChaseMovementGenerator::DispatchSplineToPosition(Unit& owner)\n{\n" + line + "\n { run(); }\n}"
        self.assertEqual(oracle.gate_condition(line + "\n" + text), line)

    def test_missing_or_malformed_fragments_fail(self):
        for text in ["missing", "bool Unit::IsInWater() const { never ends"]:
            with self.assertRaises(ValueError):
                oracle.block(text, "bool Unit::IsInWater() const")
        for text in ["missing", "bool IsWithinLOS();\nbool IsWithinLOS();"]:
            with self.assertRaises(ValueError):
                oracle.declaration(text, "bool IsWithinLOS")
        with self.assertRaises(ValueError):
            oracle.gate_condition("bool ChaseMovementGenerator::DispatchSplineToPosition() { return false; }")

    def test_missing_or_duplicate_insertion_marker_fails(self):
        with patch.object(oracle, "fragments", return_value={"METHOD": "source"}):
            for text in ["missing", "// PINNED_METHOD\n// PINNED_METHOD\n"]:
                with patch.object(Path, "read_text", return_value=text):
                    with self.assertRaisesRegex(ValueError, "insertion marker"):
                        oracle.harness({})

    def test_check_refuses_different_fixture_without_overwriting(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "fixture.test-fixture"
            expected = b"1 2 3 | 0 1 0 | -\n"
            for corrupt in [b"", b"corrupt", expected[:-1], expected.replace(b"0 1", b"1 1")]:
                path.write_bytes(corrupt)
                with self.assertRaisesRegex(SystemExit, "fixture differs"):
                    oracle.record_fixture(path, expected, True)
                self.assertEqual(path.read_bytes(), corrupt)
            path.write_bytes(expected)
            oracle.record_fixture(path, expected, True)


if __name__ == "__main__":
    unittest.main()
