#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check verbatim extraction, fail-closed source guards and authored corpus coverage."""
import collections
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

import generate_mirror_timer_fixture as generator

SOURCE = pathlib.Path(sys.argv.pop(1)).resolve() if len(sys.argv) > 1 else None
HERE = pathlib.Path(__file__).resolve().parent


class MirrorTimerOracle(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if SOURCE is None:
            raise RuntimeError('Pass the pinned CMaNGOS source root')
        cls.sources = generator.guarded(SOURCE)
        cls.harness = generator.source_harness(cls.sources)
        cls.rows = [line.split('\t') for line in (HERE / 'fixtures/mirror-timer.tsv').read_text().splitlines()
                    if line and not line.startswith('#')]
        cls.cases = collections.defaultdict(list)
        for row in cls.rows:
            cls.cases[row[0]].append(row)

    def test_complete_original_methods_constructor_getters_and_helpers_are_verbatim(self):
        for method in generator.METHODS:
            self.assertEqual(self.harness.count(generator.block(self.sources[generator.PLAYER], method)), 1)
        for source, signature in ((generator.HEADER, 'class MirrorTimer\n'),
                                  (generator.TIMER, 'class ShortIntervalTimer\n'),
                                  (generator.COMMON, 'enum TimeConstants\n')):
            self.assertEqual(self.harness.count(generator.block(self.sources[source], signature, ';')), 1)
        self.assertNotIn('// PINNED_', self.harness)
        self.assertEqual(len(generator.METHODS), 9)
        # Only the MirrorTimer declaration is exposed for read-only observation.
        mirror = generator.block(self.sources[generator.HEADER], 'class MirrorTimer\n', ';')
        self.assertIn('#define private public\n' + mirror + '\n#undef private', self.harness)

    def test_each_source_dependency_tamper_fails_before_compiler_is_invoked(self):
        with tempfile.TemporaryDirectory(prefix='mirror-source-guard-') as directory:
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
                    result = subprocess.run([sys.executable, str(HERE / 'generate_mirror_timer_fixture.py'),
                                             str(root), '--check'], env=env, capture_output=True, text=True)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn('Wrong pinned source hash: ' + name, result.stderr)
                    self.assertNotIn('Compile:', result.stdout)
                    self.assertFalse(compiler_marker.exists())
                    target.unlink()
                    target.symlink_to(SOURCE / name)
            self.assertEqual(generator.guarded(root), self.sources)

    def test_missing_compiler_fails_instead_of_skipping(self):
        with tempfile.TemporaryDirectory(prefix='mirror-no-compiler-') as directory:
            env = dict(os.environ, CXX=str(pathlib.Path(directory) / 'missing-cxx'))
            result = subprocess.run([sys.executable, str(HERE / 'generate_mirror_timer_fixture.py'),
                                     str(SOURCE), '--check'], env=env, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('Verified exact-source', result.stdout)
            self.assertIn('missing-cxx', result.stderr)

    def test_each_sequence_starts_at_source_constructor_and_is_continuous(self):
        initial = ['-1'] + ['0'] * 11
        for name, rows in self.cases.items():
            self.assertEqual(rows[0][8:20], initial, name)
            timer_type = rows[0][2]
            for step, row in enumerate(rows):
                self.assertEqual(len(row), 32, name)
                self.assertEqual(int(row[1]), step, name)
                self.assertEqual(row[2], timer_type, name)
                if step:
                    self.assertEqual(row[8:20], rows[step - 1][20:32], name)
        self.assertEqual(len(self.rows), 9479)
        self.assertEqual(len(self.cases), 1346)

    def test_all_operations_types_statuses_and_numeric_boundaries_are_exercised(self):
        self.assertEqual({row[3] for row in self.rows}, {
            'inspect', 'start', 'start_remaining', 'stop', 'set_remaining',
            'set_duration', 'set_frozen', 'set_scale', 'fetch_status', 'update'})
        self.assertEqual({row[2] for row in self.rows}, {'0', '1', '2', '3'})
        self.assertEqual({row[7] for row in self.rows if row[3] == 'fetch_status'}, {'0', '1', '2'})
        self.assertEqual({row[7] for row in self.rows if row[3] == 'update'}, {'0', '1', 'undefined_abs'})
        scales = {int(row[4]) for row in self.rows if row[3] == 'set_scale'}
        self.assertTrue({-2147483648, -2147483647, -65536, -3, -1, 0, 1, 3, 65536, 2147483647} <= scales)
        differences = {int(row[4]) for row in self.rows if row[3] == 'update'}
        self.assertTrue({0, 1, 2, 1999, 2000, 2001, 65536, 2147483647,
                         2147483648, 4294967294, 4294967295} <= differences)

    def test_int_min_rejection_is_exactly_the_undefined_call_domain(self):
        rejected = inactive_min = frozen_min = 0
        for row in self.rows:
            expected = (row[3] == 'update' and row[8] == '-2147483648' and
                        row[11] == '1' and row[18] == '0')
            self.assertEqual(row[7] == 'undefined_abs', expected, row[:8])
            if expected:
                rejected += 1
                self.assertEqual(row[8:20], row[20:32])
            elif row[3] == 'update' and row[8] == '-2147483648':
                self.assertEqual(row[7], '1')
                self.assertEqual(row[8:20], row[20:32])
                inactive_min += row[11] == '0'
                frozen_min += row[11] == '1' and row[18] == '1'
        self.assertEqual(rejected, 249)
        self.assertGreater(inactive_min, 0)
        self.assertGreater(frozen_min, 0)

    def test_pulse_residual_subtracts_once_and_expiry_overshoot_is_discarded(self):
        pulse = self.cases['expiry_pulse_residual']
        self.assertEqual([pulse[step][28] for step in (8, 9, 10, 11)], ['4001', '2001', '1', '1'])
        self.assertEqual([pulse[step][7] for step in (8, 9, 10, 11)], ['0', '0', '0', '1'])
        self.assertEqual([pulse[step][7] for step in (2, 3, 5, 6, 7)], ['1', '0', '1', '1', '0'])
        overshoot = self.cases['expiry_overshoot_discarded']
        self.assertEqual(overshoot[1][7], '0')
        self.assertEqual(overshoot[1][28], '0')
        self.assertEqual(overshoot[2][7], '1')

    def test_effective_frozen_scale_zero_and_status_overwrites_are_observable(self):
        rows = self.cases['freeze_scale_and_status']
        self.assertEqual(rows[1][24], '1')  # two-argument Start retains frozen
        self.assertEqual(rows[3][8:20], rows[3][20:32])
        self.assertEqual(rows[21][20], '2')  # SetScale(0) does not store zero
        self.assertEqual(rows[21][24], '1')
        self.assertEqual(rows[21][30:32], ['0', '1'])  # regeneration defeats raw frozen
        self.assertEqual([rows[step][7] for step in (16, 18, 20)], ['2', '2', '0'])
        self.assertEqual(rows[31][22], '2')  # Start(current,max) overwrites FULL via thaw
        self.assertEqual(rows[31][24], '0')
        self.assertEqual(rows[34][7], '0')  # expiration itself leaves status unchanged

    def test_stop_retains_fields_and_regeneration_clears_pulse(self):
        rows = self.cases['setters_status_and_stop']
        self.assertEqual(rows[12][20:32], ['-1', '24', '2', '0', '1', '101', '0',
                                          '2000', '0', '101', '1', '0'])
        self.assertEqual(rows[14][8:20], rows[14][20:32])  # inactive zero-duration no-op
        self.assertEqual(rows[25][28], '1999')
        self.assertEqual(rows[27][26], '1')
        self.assertEqual(rows[27][28], '0')
        self.assertEqual(rows[28][23], '0')
        self.assertEqual(rows[30][8:20], rows[30][20:32])  # positive-scale Start fails

    def test_numeric_wrap_observations_distinguish_saturation_and_widening(self):
        rows = self.cases['unsigned_wrap']
        self.assertEqual(rows[2][26], '0')  # tracker MAX-1 + 2 wraps
        self.assertEqual(rows[4][26], '0')  # even an expired tracker can wrap
        self.assertEqual(rows[9][26], '0')  # 65536 * 65536 becomes zero
        self.assertEqual(rows[10][26], '65536')
        self.assertEqual(rows[14][28], '4294965294')
        self.assertEqual(rows[15][28], '1999')  # pulse accumulator wraps too
        setters = self.cases['setters_status_and_stop']
        self.assertEqual(setters[17][26], '4294967286')
        self.assertEqual(setters[17][29], '20')
        zero = self.cases['zero_duration']
        self.assertEqual([zero[step][7] for step in (1, 2, 3, 4)], ['1', '1', '1', '0'])
        self.assertEqual(zero[9][26], '4294967295')


if __name__ == '__main__':
    unittest.main(verbosity=2)
