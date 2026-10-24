#!/usr/bin/env python3
"""Offline fail-closed and original-source regression tests.

Usage: test_source_atan_generate.py MANGOS_ROOT GLIBC_ROOT
"""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import source_atan_generate as oracle

MANGOS, GLIBC = map(Path, sys.argv[1:3])
del sys.argv[1:3]


class OriginalAtanTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = oracle.guarded_sources(MANGOS, GLIBC)
        cls.temporary = tempfile.TemporaryDirectory(prefix="atan64-tests-")
        cls.binary = oracle.compile_oracle(cls.sources, Path(cls.temporary.name))

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_every_complete_source_hash_is_guarded(self):
        for family, actual in (("cmangos", MANGOS), ("glibc", GLIBC)):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for name in self.sources:
                    prefix, relative = name.split("/", 1)
                    if prefix == family:
                        path = root / relative
                        path.parent.mkdir(parents=True, exist_ok=True)
                        path.write_bytes((actual / relative).read_bytes())
                for name in self.sources:
                    prefix, relative = name.split("/", 1)
                    if prefix != family:
                        continue
                    path = root / relative
                    original = path.read_bytes()
                    path.write_bytes(original + b"\n// changed\n")
                    with self.assertRaisesRegex(SystemExit, "Wrong pinned source"):
                        oracle.guarded_sources(root if family == "cmangos" else MANGOS,
                                               root if family == "glibc" else GLIBC)
                    path.write_bytes(original)

    def test_missing_source_fails(self):
        with tempfile.TemporaryDirectory() as temporary, self.assertRaises(FileNotFoundError):
            oracle.guarded_sources(MANGOS, Path(temporary))

    def test_source_functions_are_unchanged_and_complete(self):
        generated = oracle.harness(self.sources)
        source = self.sources["cmangos/src/game/Maps/ObjectPosSelector.cpp"].decode()
        for signature in ("ObjectPosSelector::ObjectPosSelector(",
                          "void ObjectPosSelector::InitializeAngle()",
                          "void ObjectPosSelector::InitializeAngle(UsedAreaSide side)",
                          "bool ObjectPosSelector::NextAngle(",
                          "bool ObjectPosSelector::NextSideAngle(",
                          "bool ObjectPosSelector::NextUsedAngle("):
            self.assertEqual(generated.count(oracle.exact_method(source, signature)), 1)
        self.assertIn("m_stepAngle[side] += (m_searchedForReqHAngle + 0.01);", generated)
        self.assertIn("m_searchedForReqHAngle = atan(OCCUPY_POS_ANGLE_ATAN_FACTOR * searchedForSize / m_searcherDist);", generated)
        self.assertIn("GNU General Public License", generated)

    def test_extraction_and_marker_drift_fail(self):
        with self.assertRaises(SystemExit):
            oracle.exact_method("void f() {}\nvoid f() {}", "void f()")
        with self.assertRaises(SystemExit):
            oracle.exact_method("void f() {", "void f()")
        with patch.object(Path, "read_text", return_value="no markers"):
            with self.assertRaisesRegex(SystemExit, "Harness marker drift"):
                oracle.harness(self.sources)

    def test_compiler_profile_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            with patch.object(subprocess, "check_output", return_value="13.0.0"):
                with self.assertRaisesRegex(SystemExit, "Requires audited"):
                    oracle.compile_oracle(self.sources, Path(temporary))

    def test_original_table_both_endians_and_digest(self):
        expected = (oracle.FIXTURES / "coefficients.sha256").read_text().split()[0]
        self.assertEqual(oracle.coefficient_digest(self.sources), expected)
        modified = dict(self.sources)
        key = "glibc/sysdeps/ieee754/dbl-64/uatan.tbl"
        modified[key] = modified[key].replace(b"0X3FB04006", b"0X3FB04007", 1)
        with self.assertRaisesRegex(SystemExit, "endian tables disagree"):
            oracle.coefficient_digest(modified)

    def test_complete_fixture_and_digest_reproduction(self):
        for name, expected in oracle.fixture_contents(self.binary).items():
            self.assertEqual((oracle.FIXTURES / name).read_text(), expected, name)

    def test_source_zero_infinity_and_defaults(self):
        observations = list(oracle.observe(self.binary, "atan", [[0], [0x80000000], [1], [0x7f800000]]))
        self.assertEqual(observations[0][1], [0, 0])
        self.assertEqual(observations[1][1], [0x8000000000000000, 0x80000000])
        self.assertEqual(observations[2][1][1], 1)
        self.assertEqual(observations[3][1], [0x3ff921fb54442d18, 0x3fc90fdb])
        observations = list(oracle.observe(self.binary, "selector", [(0, 0), (0x80000000, 0x80000000),
                                                                    (0x3ec72b02, 0x3ec72b02)]))
        self.assertEqual(observations[0][1], observations[1][1])
        self.assertEqual(observations[0][1], observations[2][1])

    def test_exact_pi_and_double_increment_witnesses(self):
        exact = next(oracle.observe(self.binary, "selector", [(0x42142315, 0x3f800000)]))[1]
        self.assertEqual(exact[2:5], [0x40490fdb, 0x40490fdb, 6])
        self.assertEqual(exact[-2:], [0x40490fdb, 0xc0490fdb])
        scaled = next(oracle.observe(self.binary, "selector", [(0x40142315, 0x3d800000)]))[1]
        self.assertEqual(exact, scaled)
        mixed = next(oracle.observe(self.binary, "selector", [(0x3f800000, 0x3f800000)]))[1]
        self.assertEqual(mixed[5], 0x3c23d70a)
        self.assertNotEqual(mixed[5], 0x3c23d700)

    def test_selector_maximum_and_overflow_ratio(self):
        tiny = next(oracle.observe(self.binary, "selector", [(1, 0x7f7fffff)]))[1]
        self.assertEqual(tiny[0:2], [0, 0])
        self.assertEqual(tiny[4], 628)
        overflow = next(oracle.observe(self.binary, "selector", [(0x7f7fffff, 0x3f800000)]))[1]
        self.assertEqual(overflow[0:2], [0x7f800000, 0x3fc90fdb])

    def test_wrapper_rejects_unsupported_inputs(self):
        for row in ("atan bf800000", "atan ff800000", "atan 7fc00000",
                    "selector bf800000 3f800000", "selector 3f800000 7f800000",
                    "selector 3f800000", "unexpected 00000000"):
            result = subprocess.run([str(self.binary)], input=row + "\n", text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0, row)


if __name__ == "__main__":
    unittest.main()
