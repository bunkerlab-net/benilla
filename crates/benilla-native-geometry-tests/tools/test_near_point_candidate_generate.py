#!/usr/bin/env python3
"""Fail-closed source binding and numeric fixture contracts; no Rust/game assets.

Usage: test_near_point_candidate_generate.py MANGOS_ROOT GLIBC_FLT32
Run the generator normally and with --check --sanitize to verify compiled output.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
from fractions import Fraction
import hashlib
import pathlib
import shutil
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import near_point_candidate_generate as oracle

MANGOS = pathlib.Path(sys.argv.pop(1)).resolve() if len(sys.argv) > 1 else None
GLIBC = pathlib.Path(sys.argv.pop(1)).resolve() if len(sys.argv) > 1 else None


def rows(name):
    return [tuple(int(word, 16) for word in line.split())
            for line in (oracle.FIXTURES / name).read_text().splitlines()
            if not line.startswith('#')]


class NearPointOracleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if MANGOS is None or GLIBC is None:
            raise RuntimeError('Both pinned source paths required; never skip missing source')
        cls.source = oracle.guarded_sources(MANGOS, GLIBC)
        cls.trig = rows('trig-cases.test-fixture')
        cls.candidate = rows('candidate-cases.test-fixture')

    def test_exact_source_bodies_macros_and_original_notices(self):
        macros, normalize, candidate, package = oracle.source_fragments(self.source)
        self.assertEqual(package, oracle.PACKAGE.read_text(encoding='latin1'))
        self.assertIn('x = (posX + (distance2d * cosf(absAngle)));', candidate)
        self.assertIn('y = (posY + (distance2d * sinf(absAngle)));', candidate)
        self.assertIn('MAP_HALFSIZE - 0.5', normalize)
        self.assertIn('533.33333f', macros)
        for name in ('src/game/Maps/GridDefines.h', 'src/game/Entities/Object.cpp'):
            self.assertIn(self.source[name][:self.source[name].index('*/') + 2], package)
        for name in oracle.GLIBC_NAMES:
            self.assertIn(self.source[name], package)
        harness, _ = oracle.build_harness(self.source)
        self.assertIn(candidate, harness)
        self.assertIn(normalize, harness)
        self.assertIn('#define sinf source_sin', harness)
        self.assertIn('#define cosf source_cos', harness)
        self.assertIn('FLT_EVAL_METHOD == 0', harness)
        self.assertIn('std::fegetround() != FE_TONEAREST', harness)

    def test_all_seven_full_source_hashes_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            for name in self.source:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile((GLIBC if name in oracle.GLIBC_NAMES else MANGOS) / name, path)
            oracle.guarded_sources(root, root)
            for name in self.source:
                with self.subTest(name=name):
                    path = root / name
                    original = path.read_bytes()
                    path.write_bytes(original + b'\n/* intentional guard probe */\n')
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                        oracle.guarded_sources(root, root)
                    path.write_bytes(original)
            (root / 's_sinf.c').unlink()
            with self.assertRaises(FileNotFoundError):
                oracle.guarded_sources(root, root)

    def test_package_drift_stops_before_compile_and_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            package = pathlib.Path(temporary) / 'package.txt'
            package.write_text('intentional package drift')
            with mock.patch.object(oracle, 'PACKAGE', package), \
                 mock.patch.object(oracle, 'compile_oracle') as compiler, \
                 mock.patch.object(sys, 'argv', ['oracle.py', str(MANGOS), str(GLIBC), '--check']):
                with self.assertRaisesRegex(SystemExit, 'source package differs'):
                    oracle.main()
                compiler.assert_not_called()
            self.assertEqual(package.read_text(), 'intentional package drift')

    def test_duplicate_or_missing_source_and_harness_markers_fail_closed(self):
        for text in ('void x() {}\nvoid x() {}', 'void y() {}'):
            with self.assertRaisesRegex(SystemExit, 'Expected one source method'):
                oracle.exact_method(text, 'void x(')
        with self.assertRaisesRegex(SystemExit, 'Unclosed source method'):
            oracle.exact_method('void x() {', 'void x(')
        with self.assertRaisesRegex(SystemExit, 'Expected one source declaration'):
            oracle.exact_line('#define X 1\n#define X 2', r'^#define X .*$')
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            original = (oracle.HERE / 'near_point_candidate_oracle.cpp').read_text()
            (root / 'near_point_candidate_oracle.cpp').write_text(original + '\n// PINNED_GET_NEAR_POINT\n')
            with mock.patch.object(oracle, 'HERE', root):
                with self.assertRaisesRegex(SystemExit, 'Expected one harness marker'):
                    oracle.build_harness(self.source)

    def test_missing_compiler_is_an_error_never_a_skip(self):
        with mock.patch.object(oracle.subprocess, 'run', side_effect=FileNotFoundError('gcc missing')):
            with self.assertRaises(FileNotFoundError):
                oracle.run(['gcc', '--version'])

    def test_symbol_guard_rejects_host_fallback_missing_bindings_and_fusion(self):
        defined = '00001 T source_sin\n00002 T source_cos\n'
        assembly = ('00003 <WorldObject::GetNearPoint2dAt(float, float, float&, float&, float, float)>:\n'
                    ' call 1 <source_cos>\n mulss\n addss\n call 2 <source_sin>\n mulss\n addss\n\n')
        oracle.verify_symbols(defined, ' U memcpy@GLIBC_2.14\n', assembly)
        for symbol in ('sinf', 'cosf', 'sin', 'cos', 'sincosf', '__sinf', '__cosf'):
            with self.subTest(symbol=symbol), self.assertRaisesRegex(SystemExit, 'Host trigonometric fallback'):
                oracle.verify_symbols(defined, f' U {symbol}@GLIBC_2.2.5\n', assembly)
        with self.assertRaisesRegex(SystemExit, 'Missing explicitly bound'):
            oracle.verify_symbols('', '', assembly)
        with self.assertRaisesRegex(SystemExit, 'not bound to source_sin'):
            oracle.verify_symbols(defined, '', assembly.replace('<source_sin>', '<sinf>'))
        with self.assertRaisesRegex(SystemExit, 'separate binary32'):
            oracle.verify_symbols(defined, '', assembly.replace('mulss\n addss', 'vfmadd132ss'))

    def test_signed_zero_subnormal_and_exact_branch_neighbors(self):
        values = {a: (s, c) for a, s, c in self.trig}
        self.assertEqual(len(values), 258)
        for sign in (0, oracle.SIGN):
            for word in (0, 1, 0x007FFFFF, 0x00800000, 0x39800000 - 1):
                self.assertEqual(values[word | sign], (word | sign, 0x3F800000))
            for center in (0x39800000, 0x3F400000, 0x3FC90FDB, 0x40490FDB, 0x4096CBE4):
                for delta in range(-3, 4):
                    self.assertIn((center + delta) | sign, values)
            for delta in range(-3, 1):
                self.assertIn((oracle.TAU + delta) | sign, values)

    def test_candidate_finite_domain_clamps_and_signed_zero_matrix(self):
        self.assertEqual(len(self.candidate), 3012)
        inputs = {row[:4] for row in self.candidate}
        for x, y, distance, angle, result_x, result_y in self.candidate:
            self.assertLessEqual(x & ~oracle.SIGN, oracle.COORD)
            self.assertLessEqual(y & ~oracle.SIGN, oracle.COORD)
            self.assertTrue(distance <= 0x7F7FFFFF or distance == oracle.SIGN)
            self.assertTrue(angle <= oracle.TAU or angle == oracle.SIGN)
            self.assertLessEqual(result_x & ~oracle.SIGN, oracle.CLAMP)
            self.assertLessEqual(result_y & ~oracle.SIGN, oracle.CLAMP)
        for x in (0, oracle.SIGN):
            for y in (0, oracle.SIGN):
                for distance in (0, oracle.SIGN, 1):
                    for angle in (0, oracle.SIGN, 1):
                        self.assertIn((x, y, distance, angle), inputs)
        outputs = {word for row in self.candidate for word in row[4:]}
        self.assertTrue({oracle.CLAMP, oracle.CLAMP | oracle.SIGN} <= outputs)

    def test_nonfused_binary32_cancellation_witness(self):
        row = (0xBF0A5141, 0xBF576AA6, 0x3F800001, 0x3F800000, 0, 0)
        self.assertIn(row, self.candidate)
        sine, cosine = next((s, c) for a, s, c in self.trig if a == row[3])
        # Diagnostic FMA alternative only. The expected candidate row above came
        # exclusively from the original CMaNGOS body, not this rational arithmetic.
        exact = lambda word: Fraction.from_float(oracle.value(word))
        fused_x = oracle.bits(float(exact(row[2]) * exact(cosine) + exact(row[0])))
        fused_y = oracle.bits(float(exact(row[2]) * exact(sine) + exact(row[1])))
        self.assertEqual((fused_x, fused_y), (0x31A51400, 0xB2A25570))
        self.assertNotEqual((fused_x, fused_y), row[4:])

    def test_outside_clamp_origin_cancels_without_preclamping(self):
        self.assertIn((0x46EA6000, 0xC6EA6000, 0x46EA6000, 0x40490FDB,
                       0, oracle.CLAMP | oracle.SIGN), self.candidate)
        self.assertIn((oracle.COORD, oracle.COORD | oracle.SIGN, 0, 0,
                       oracle.CLAMP, oracle.CLAMP | oracle.SIGN), self.candidate)

    def test_current_bearing_subnormal_is_used_directly_as_radians(self):
        self.assertIn((0, 0, 0x7F7FFFFF, 1, oracle.CLAMP, 0x34FFFFFF), self.candidate)
        # Existing independent GetAngleAt oracle establishes this input origin.
        path = oracle.HERE.parents[1] / 'benilla-sim/tools/fixtures/facing/oracle.tsv'
        self.assertIn('bearing_12_2 bearing 00000001 80000001 3f800000 00000000 00000001 1', path.read_text())

    def test_rejection_fixtures_match_wrapper_domains(self):
        trig, candidate = oracle.rejected_inputs()
        self.assertEqual(rows('trig-rejections.test-fixture'), [(w,) for w in trig])
        self.assertEqual(rows('candidate-rejections.test-fixture'), candidate)
        self.assertEqual((len(trig), len(candidate)), (9, 37))
        for (word,) in rows('trig-rejections.test-fixture'):
            self.assertGreater(word & ~oracle.SIGN, oracle.TAU)
        for x, y, distance, angle in candidate:
            self.assertTrue((x & ~oracle.SIGN) > oracle.COORD or (y & ~oracle.SIGN) > oracle.COORD
                            or distance > 0x7F7FFFFF and distance != oracle.SIGN
                            or (angle > oracle.TAU and angle != oracle.SIGN))

    def test_sample_sequence_and_fixture_extensions(self):
        self.assertEqual(len(oracle.trig_grid()), 737002)
        self.assertEqual(oracle.trig_grid()[:4], [0, oracle.SIGN, 4093, 4093 | oracle.SIGN])
        self.assertEqual(oracle.trig_grid()[-2:], [oracle.TAU, oracle.TAU | oracle.SIGN])
        self.assertEqual(len(oracle.candidate_grid()), 32768)
        self.assertEqual(oracle.DYADIC_STEPS, int(oracle.value(oracle.TAU) * oracle.DYADIC_DENOMINATOR))
        self.assertTrue(all(path.suffix == '.test-fixture' for path in oracle.FIXTURES.iterdir()))

    def test_existing_vmap_202_case_and_sample_digest_are_unchanged(self):
        for name, expected in (
            ('trig-cases.tsv', 'dffff37c6f1685d7c8fa884bde910684add025bc1001eb7fb45eb9ffe1cd86de'),
            ('trig-grid.sha256', 'd880b8cfe49ab912624bb06166ad34c3946d3d0aef758e2518317ece9704e7c1'),
        ):
            data = (oracle.HERE / 'fixtures/vmap_source' / name).read_bytes()
            self.assertEqual(hashlib.sha256(data).hexdigest(), expected)
        cases = (oracle.HERE / 'fixtures/vmap_source/trig-cases.tsv').read_text().splitlines()
        self.assertEqual(sum(not line.startswith('#') for line in cases), 202)


if __name__ == '__main__':
    unittest.main(verbosity=2)
