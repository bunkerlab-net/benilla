#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Source-extraction negative tests; no compiler, downloads or game data."""
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import generate_player_model_fixture as oracle


class SourceGuards(unittest.TestCase):
    def test_changed_hash_fails(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / next(iter(oracle.HASHES))
            path.parent.mkdir(parents=True)
            path.write_bytes(b"changed source\n")
            with self.assertRaisesRegex(SystemExit, "pinned source hash mismatch"):
                oracle.source(Path(root))

    def test_lf_normalization_preserves_matching_source(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "Player.cpp"
            path.write_bytes(b"first\r\nsecond\r\n")
            digest = hashlib.sha256(b"first\nsecond\n").hexdigest()
            with patch.dict(oracle.HASHES, {"Player.cpp": digest}, clear=True):
                self.assertEqual(oracle.source(Path(root))["Player.cpp"], "first\nsecond\n")

    def test_extraction_keeps_complete_bodies_and_comments_unchanged(self):
        body = "void Player::InitDisplayIds()\n{\n // comment\n if (ready) { init(); }\n}\n\n"
        text = "prefix\n" + body + "// Return true is the bought item\n"
        self.assertEqual(oracle.between(text, "void Player::InitDisplayIds()", "// Return true is the bought item"), body)

    def test_missing_start_or_end_fails(self):
        for text in ["missing source", "void Player::InitDisplayIds() { truncated"]:
            with self.assertRaises(ValueError):
                oracle.between(text, "void Player::InitDisplayIds()", "// Return true is the bought item")

    def test_constants_and_formats_are_unchanged(self):
        constant = "static constexpr float const DEFAULT_COLLISION_HEIGHT = 2.03128f;"
        fmt = 'const char CreatureDisplayInfofmt[] = "nixifxxxxxxx";'
        self.assertEqual(oracle.constant(constant, "DEFAULT_COLLISION_HEIGHT"), constant)
        self.assertEqual(oracle.format_declaration(fmt, "CreatureDisplayInfofmt"), fmt)

    def test_missing_constant_or_format_fails(self):
        with self.assertRaisesRegex(SystemExit, "missing source constant"):
            oracle.constant("missing", "DEFAULT_COLLISION_HEIGHT")
        with self.assertRaisesRegex(SystemExit, "missing source format"):
            oracle.format_declaration("missing", "CreatureModelDatafmt")

    def test_missing_or_repeated_marker_fails(self):
        for template in ["no marker", "// PINNED_INIT\n// PINNED_INIT\n"]:
            with patch.object(oracle, "fragments", return_value={"INIT": "original method"}), patch.object(Path, "read_text", return_value=template):
                with self.assertRaisesRegex(SystemExit, "missing or repeated harness insertion marker"):
                    oracle.harness({})

    def test_harness_insertion_is_exact(self):
        body = "original\n unchanged\n"
        with patch.object(oracle, "fragments", return_value={"INIT": body}), patch.object(Path, "read_text", return_value="before\n// PINNED_INIT\nafter\n"):
            self.assertEqual(oracle.harness({}), "before\n" + body + "\nafter\n")

    def test_corrupt_or_truncated_fixture_fails_without_overwriting(self):
        with tempfile.TemporaryDirectory() as root:
            target = Path(root) / "numeric.test-fixture"
            expected = b"0 31 51 0 0 0 0 0 1065353216 0 1073873022\n"
            for corrupted in [b"", b"malformed", expected[:-2], expected.replace(b" 0 ", b" 1 ")]:
                target.write_bytes(corrupted)
                with self.assertRaisesRegex(SystemExit, "fixture differs"):
                    oracle.record_fixture(target, expected, True)
                self.assertEqual(target.read_bytes(), corrupted)
            target.write_bytes(expected)
            oracle.record_fixture(target, expected, True)


if __name__ == "__main__":
    unittest.main()
