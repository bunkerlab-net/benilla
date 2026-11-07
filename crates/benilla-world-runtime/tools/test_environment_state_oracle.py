#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Source guards and authored corpus checks; pass the pinned source root."""
import importlib.util
import pathlib
import struct
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
HERE = pathlib.Path(__file__).resolve().parent
SOURCE = pathlib.Path(sys.argv.pop(1)).resolve() if len(sys.argv) > 1 else None
spec = importlib.util.spec_from_file_location('environment_generator', HERE / 'generate_environment_state_fixture.py')
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)


class SourceOracleChecks(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if SOURCE is None:
            raise RuntimeError('Pass the pinned CMaNGOS source root')
        cls.source = generator.guarded(SOURCE)
        cls.harness = generator.source_harness(cls.source)
        cls.rows = [line.split('\t') for line in (HERE / 'fixtures/environment-state.tsv').read_text().splitlines()
                    if line and not line.startswith('#')]

    def test_complete_original_environment_bodies_are_verbatim(self):
        for signature in ('void Player::SetEnvironmentFlags(', 'void Player::UpdateTerainEnvironmentFlags('):
            original = generator.block(self.source[generator.PLAYER], signature)
            self.assertEqual(self.harness.count(original), 1)
        self.assertNotIn('// PINNED_', self.harness)

    def test_source_timer_and_gate_slices_are_verbatim(self):
        original = self.source[generator.PLAYER]
        timer = generator.block(original, '    if (m_positionStatusUpdateTimer)\n')
        self.assertIn(timer, self.harness)
        position = generator.block(original, 'bool Player::SetPosition(')
        gate = position[position.index('    if (m_positionStatusUpdateTimer)'):
                        position.index('\n    // code block for outdoor state and area-explore check')]
        self.assertIn(gate, self.harness)
        self.assertIn('m_positionStatusUpdateTimer = 100;', gate)
        self.assertNotIn('CheckAreaExploreAndOutdoor();', self.harness)

    def test_every_source_dependency_tamper_is_rejected_before_compilation(self):
        with tempfile.TemporaryDirectory(prefix='environment-source-guard-') as directory:
            root = pathlib.Path(directory)
            for name in self.source:
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.symlink_to(SOURCE / name)
            for name in self.source:
                target = root / name
                target.unlink()
                target.write_bytes((SOURCE / name).read_bytes() + b'\n// deliberate guard probe\n')
                with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash: ' + name):
                    generator.guarded(root)
                target.unlink()
                target.symlink_to(SOURCE / name)
            self.assertEqual(generator.guarded(root), self.source)

    def test_complete_wdbc_layout_and_original_lookup_observations(self):
        observed = [line.split('\t') for line in (HERE / 'fixtures/environment-state-dbc.tsv').read_text().splitlines()
                    if line and not line.startswith('#')]
        self.assertEqual(len(observed), 20)
        for store in range(2):
            data = (HERE / f'fixtures/environment-state/store{store}/LiquidType.dbc.test-fixture').read_bytes()
            magic, count, fields, size, strings = struct.unpack_from('<5I', data)
            self.assertEqual((magic, fields, size, strings), (0x43424457, 4, 16, 1))
            self.assertEqual(len(data), 20 + count * size + strings)
            self.assertEqual(data[-1:], b'\0')
            rows = dict((row[0], row) for row in struct.iter_unpack('<4I', data[20:-1]))
            self.assertEqual(len(rows), count)
            self.assertEqual(0 in rows, store == 0)
            for observation in observed:
                if int(observation[0]) != store:
                    continue
                key = int(observation[1])
                self.assertEqual(observation[2] == '1', key in rows)
                if key in rows:
                    self.assertEqual(tuple(map(int, observation[3:])), rows[key])
                else:
                    self.assertEqual(observation[3:], ['-'] * 4)

    def test_ordered_sequences_are_continuous_and_timer_has_no_query(self):
        previous = {}
        for row in self.rows:
            self.assertEqual(len(row), 22)
            key = (row[0], row[2])
            if key in previous:
                prior = previous[key]
                self.assertEqual(int(row[1]), int(prior[1]) + 1)
                self.assertEqual(row[14:17], prior[17:20])
            else:
                self.assertEqual(row[1], '0')
            if row[3] == 'update':
                self.assertEqual(row[20:22], ['0', '-'])
            previous[key] = row
        self.assertGreater(len(self.rows), 1000)

    def test_threshold_corpus_distinguishes_double_from_float_expression(self):
        def decode(value):
            return struct.unpack('<f', struct.pack('<I', int(value, 16)))[0]
        def f32(value):
            return struct.unpack('<f', struct.pack('<f', value))[0]
        different = 0
        for row in self.rows:
            if not row[0].startswith('high_liquid_double_threshold_'):
                continue
            level, depth, collision = map(decode, row[9:12])
            source_threshold = depth + collision * 0.75
            narrowed_threshold = f32(depth + f32(collision * 0.75))
            if (level > source_threshold) != (level > narrowed_threshold):
                different += 1
        self.assertGreater(different, 0)


if __name__ == '__main__':
    unittest.main(verbosity=2)
