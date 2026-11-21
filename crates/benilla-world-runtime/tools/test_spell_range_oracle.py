#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate exact-source extraction, fail-closed guards, and discriminating corpus probes."""
import collections
import math
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import unittest

import generate_spell_range_fixture as generator

SOURCE = pathlib.Path(sys.argv.pop(1)).resolve() if len(sys.argv) > 1 else None
HERE = pathlib.Path(__file__).resolve().parent


def number(value):
    return struct.unpack('!f', bytes.fromhex(value))[0]


def rounded(value):
    return struct.unpack('!f', struct.pack('!f', value))[0]


def bits(value):
    return struct.pack('!f', value).hex()


def rows(name):
    return [line.split('\t') for line in (HERE / 'fixtures' / name).read_text().splitlines()
            if line and not line.startswith('#')]


class SpellRangeOracle(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if SOURCE is None:
            raise RuntimeError('Pass the pinned CMaNGOS source root')
        cls.sources = generator.guarded(SOURCE)
        cls.harness = generator.source_harness(cls.sources)
        cls.rows = rows('spell-range.tsv')
        cls.excluded = rows('spell-range-excluded.tsv')
        cls.cases = {(row[0], row[1]): row for row in cls.rows}

    def test_all_complete_original_bodies_and_dependencies_are_verbatim(self):
        for name, (source, signature, suffix) in generator.BLOCKS.items():
            with self.subTest(block=name):
                self.assertEqual(self.harness.count(generator.block(self.sources[source], signature, suffix)), 1)
        self.assertNotIn('// PINNED_', self.harness)
        self.assertEqual(len(generator.BLOCKS), 34)
        self.assertEqual(len(self.sources), 15)
        # No replacement body or bespoke no-modifier formula hides the actual Player path.
        self.assertIn('template <class T> void Player::ApplySpellMod(', self.harness)
        self.assertIn('Player* GetSpellModOwner() const override { return const_cast<Player*>(this); }', self.harness)
        self.assertIn('std::array<std::vector<SpellModifier*>, MAX_SPELLMOD> m_spellMods{};', self.harness)
        self.assertIn('float GetCombatReach() const override { return m_floatValues[UNIT_FIELD_COMBATREACH]; }', self.harness)
        self.assertIn('m_maxRange = maxRange; // need to save max range for some calculations', self.harness)

    def test_each_source_dependency_tamper_fails_before_compiler(self):
        with tempfile.TemporaryDirectory(prefix='spell-range-source-guard-') as directory:
            root = pathlib.Path(directory)
            compiler_marker = root / 'compiler-was-invoked'
            fake_compiler = root / 'compiler.py'
            fake_compiler.write_text('import pathlib\npathlib.Path(' + repr(str(compiler_marker)) +
                                     ').write_text("unexpected compiler invocation")\n')
            env = dict(os.environ, CXX=f'{sys.executable} {fake_compiler}')
            for name in self.sources:
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.symlink_to(SOURCE / name)
            for name in self.sources:
                with self.subTest(source=name):
                    target = root / name
                    target.unlink()
                    target.write_bytes((SOURCE / name).read_bytes() + b'\n// deliberate source guard probe\n')
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash: ' + name):
                        generator.guarded(root)
                    result = subprocess.run([sys.executable, str(HERE / 'generate_spell_range_fixture.py'),
                                             str(root), '--check'], env=env, capture_output=True, text=True)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn('Wrong pinned source hash: ' + name, result.stderr)
                    self.assertNotIn('Compile:', result.stdout)
                    self.assertFalse(compiler_marker.exists())
                    target.unlink()
                    target.symlink_to(SOURCE / name)
            self.assertEqual(generator.guarded(root), self.sources)

    def test_only_checkout_line_endings_may_differ(self):
        with tempfile.TemporaryDirectory(prefix='spell-range-crlf-') as directory:
            root = pathlib.Path(directory)
            for name, source in self.sources.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(source.encode('latin1').replace(b'\n', b'\r\n'))
            self.assertEqual(generator.guarded(root), self.sources)
            self.assertEqual(generator.source_harness(generator.guarded(root)), self.harness)

    def test_missing_compiler_fails_instead_of_skipping(self):
        with tempfile.TemporaryDirectory(prefix='spell-range-no-compiler-') as directory:
            env = dict(os.environ, CXX=str(pathlib.Path(directory) / 'missing-cxx'))
            result = subprocess.run([sys.executable, str(HERE / 'generate_spell_range_fixture.py'),
                                     str(SOURCE), '--check'], env=env, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('Verified exact-source', result.stdout)
            self.assertIn('missing-cxx', result.stderr)

    def test_fixture_schema_domain_uniqueness_and_result_coverage(self):
        self.assertEqual(len(self.rows), 2418)
        self.assertEqual(len(self.cases), len(self.rows))
        self.assertEqual(len(self.excluded), 12)
        self.assertEqual({row[15] for row in self.rows}, {'255', '89', '118'})
        self.assertEqual({row[1] for row in self.rows}, {'0', '1'})
        for row in self.rows:
            with self.subTest(case=row[:2]):
                self.assertEqual(len(row), 18)
                for field in row[2:15] + row[16:18]:
                    self.assertRegex(field, r'^[0-9a-f]{8}$')
                    self.assertTrue(math.isfinite(number(field)))
                self.assertTrue(all(abs(number(value)) <= 32760 for value in row[2:8]))
                self.assertTrue(0 <= number(row[8]) <= number(row[9]) <= 1e6)
                self.assertTrue(all(0 <= number(value) <= number('5368d4a5') for value in row[10:12]))
        self.assertEqual(self.cases['coordinate_bounds', '1'][2], '46fff000')
        self.assertEqual(self.cases['range_reach_bounds', '1'][10:12], ['5368d4a5'] * 2)

    def test_source_maximum_side_effect_occurs_only_on_success(self):
        for row in self.rows + self.excluded:
            self.assertEqual(row[16], '42f6e979')
            self.assertEqual(row[17], row[13] if row[15] == '255' else row[16], row[:2])
        # Numeric outcomes intentionally report a computed maximum even on rejected casts.
        rejected = self.cases['sqrt_rounds_back_to_max', '1']
        self.assertNotEqual(rejected[13], rejected[17])

    def test_all_three_axes_and_nextafter_edges_are_exercised(self):
        boundary = [row for row in self.rows if row[0].startswith('boundary_')]
        self.assertEqual(len(boundary), 1296)
        self.assertEqual({row[0].rsplit('_', 2)[1] for row in boundary}, {'-1', '0', '1'})
        self.assertEqual({row[0].rsplit('_', 1)[1] for row in boundary}, {'0', '1', '2'})
        for row in boundary:
            if row[1] != '1':
                continue
            parts = row[0].split('_')
            boundary_kind, neighbor = parts[4], int(parts[5])
            if boundary_kind == '1':
                self.assertEqual(row[15], '89' if neighbor > 0 else '255', row[:2])
            elif boundary_kind == '0' and number(row[8]) > 0:
                self.assertEqual(row[15], '118' if neighbor < 0 else '255', row[:2])
        pythagorean = self.cases['3d_pythagorean_edge', '1']
        self.assertEqual(pythagorean[14:16], ['43290000', '255'])

    def test_zero_minimum_and_nonzero_minimum_reach_order(self):
        zero_minimum = [row for row in self.rows if number(row[8]) == 0]
        self.assertTrue(all(row[12] == '00000000' for row in zero_minimum))
        row = self.cases['reach_sum_before_range', '1']
        self.assertEqual(row[12:16], ['3f800001', '3f800001', '3f800000', '118'])
        # Reassociating (row + caster) + target loses both half-ulp reaches.
        reassociated = rounded(rounded(number(row[9]) + number(row[10])) + number(row[11]))
        self.assertEqual(bits(reassociated), '3f800000')
        self.assertNotEqual(bits(reassociated), row[13])
        self.assertEqual(self.cases['signed_zero', '1'][12:15], ['00000000'] * 3)

    def test_release_tolerance_uses_reach_adjusted_maximum_and_caps_at_three(self):
        for value in (0x41efffff, 0x41f00000, 0x41f00001):
            name = 'tolerance_cap_' + str(value)
            strict, release = self.cases[name, '1'], self.cases[name, '0']
            self.assertEqual(strict[13], f'{value:08x}')
            self.assertEqual(release[13], '4203ffff' if value == 0x41efffff else '42040000')
        # Source max 33 receives 3, not base-row 30 plus an independently rounded percent.
        name = 'boundary_0_1106247680_1069547520_1_0_0'
        self.assertEqual(self.cases[name, '1'][13], '42040000')
        self.assertEqual(self.cases[name, '0'][13], '42100000')
        strict = self.cases['sqrt_rounds_back_to_max', '1']
        release = self.cases['sqrt_rounds_back_to_max', '0']
        self.assertEqual((strict[15], release[15]), ('89', '255'))
        self.assertEqual(strict[12], release[12])

    def test_squared_comparison_disagrees_with_rounded_sqrt(self):
        row = self.cases['sqrt_rounds_back_to_max', '1']
        self.assertEqual(row[14], '44800001')
        self.assertEqual(row[15], '89')
        self.assertEqual(rounded(math.sqrt(number(row[14]))), number(row[13]))

    def test_corpus_discriminates_fused_products_and_reassociated_distance(self):
        fused_differences = regrouped_differences = 0
        for row in self.rows:
            if row[1] != '1':
                continue
            dx, dy, dz = (rounded(number(row[i]) - number(row[i + 3])) for i in (2, 3, 4))
            xx, yy, zz = (rounded(d * d) for d in (dx, dy, dz))
            # Python binary64 exactly holds a binary32 product and the decisive binary32 rounding bits.
            fused = rounded(rounded(dx * dx + yy) + zz)
            regrouped = rounded(xx + rounded(yy + zz))
            fused_differences += bits(fused) != row[14]
            regrouped_differences += bits(regrouped) != row[14]
        self.assertGreater(fused_differences, 0)
        self.assertGreater(regrouped_differences, 0)

    def test_gradual_underflow_inputs_intermediates_and_ties_are_observable(self):
        subnormal_input = subnormal_squared = normal_input_subnormal_squared = 0
        for row in self.rows:
            values = [int(value, 16) & 0x7fffffff for value in row[2:12]]
            has_subnormal = any(0 < value < 0x00800000 for value in values)
            is_subnormal_squared = 0 < int(row[14], 16) < 0x00800000
            subnormal_input += has_subnormal
            subnormal_squared += is_subnormal_squared
            normal_input_subnormal_squared += is_subnormal_squared and not has_subnormal
        self.assertGreater(subnormal_input, 0)
        self.assertGreater(subnormal_squared, 0)
        self.assertGreater(normal_input_subnormal_squared, 0)
        self.assertEqual(self.cases['underflow_min_1', '1'][12:16],
                         ['00000001', '00000001', '00000000', '255'])
        self.assertEqual(self.cases['underflow_tolerance_1', '0'][13], '00000001')
        self.assertEqual(self.cases['underflow_reach_1', '1'][13], '00000002')

    def test_excluded_source_overflow_is_not_saturated_or_admitted(self):
        observations = {(row[0], row[1]): row for row in self.excluded}
        self.assertEqual(observations['overflow_distance_square', '1'][14:16], ['7f800000', '89'])
        self.assertEqual(observations['overflow_coordinate_subtraction', '1'][14:16], ['7f800000', '89'])
        # Source inf > inf is false: no saturation/validation is smuggled into the source oracle.
        self.assertEqual(observations['overflow_range_square', '1'][14:16], ['7f800000', '255'])
        self.assertEqual(observations['overflow_reach_sum', '1'][13:16], ['7f800000', '7f800000', '255'])
        nan = observations['nonfinite_coordinate', '1']
        self.assertTrue(math.isnan(number(nan[14])))
        self.assertEqual(nan[15], '255')
        for row in self.excluded:
            in_domain = (all(math.isfinite(number(v)) and abs(number(v)) <= 32760 for v in row[2:8])
                         and 0 <= number(row[8]) <= number(row[9]) <= 1e6
                         and all(0 <= number(v) <= number('5368d4a5') for v in row[10:12]))
            self.assertFalse(in_domain, row[:2])


if __name__ == '__main__':
    unittest.main(verbosity=2)
