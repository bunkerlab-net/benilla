#!/usr/bin/env python3
"""Fail-closed source guards and observation-fixture contract; no Rust/game assets.

Usage: test_ground_height_generate.py /path/to/pinned/mangos-classic
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import collections
import pathlib
import shutil
import struct
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import ground_height_generate as oracle

SOURCE = pathlib.Path(sys.argv.pop(1)).resolve() if len(sys.argv) > 1 else None


def rows(name):
    content = (oracle.HERE / 'fixtures' / name).read_text().splitlines()
    header = next(line[2:].split('\t') for line in content if line.startswith('# case\t'))
    return [dict(zip(header, line.split('\t'), strict=True))
            for line in content if not line.startswith('#')]


def finite_bits(raw):
    return (int(raw, 16) & 0x7f800000) != 0x7f800000


class GroundHeightOracleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if SOURCE is None:
            raise RuntimeError('Pinned source path required; never skip missing source')
        cls.source = oracle.guarded_sources(SOURCE)
        cls.finite = rows('ground-height.test-fixture')
        cls.nonfinite = rows('ground-height-nonfinite.test-fixture')

    def test_full_source_bodies_and_original_notices_are_packaged(self):
        _, _, _, package = oracle.source_fragments(self.source)
        self.assertEqual(package, oracle.PACKAGE.read_text())
        for name, signature in (
            ('src/game/Maps/Map.cpp', 'float Map::GetHeight('),
            ('src/game/Entities/Unit.cpp', 'void Unit::UpdateAllowedPositionZ('),
            ('src/game/vmap/DynamicTree.cpp', 'float DynamicMapTree::getHeight('),
        ):
            original = self.source[name]
            self.assertIn(oracle.exact_method(original, signature), package)
            self.assertIn(original[:original.index('*/') + 2], package)
        self.assertIn(self.source['src/game/vmap/RegularGrid.h'], package)
        self.assertNotIn('SCRIPTED', package)

    def test_each_new_full_file_hash_and_inherited_guards_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            for name in self.source:
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(SOURCE / name, target)
            oracle.guarded_sources(root)
            names = [line.split('  ', 1)[1] for line in oracle.EXTRA_MANIFEST.read_text().splitlines()]
            names += ['dep/g3dlite/Ray.cpp', 'dep/g3dlite/G3D/Ray.h',
                      'dep/g3dlite/g3dmath.cpp', 'src/game/vmap/MapTree.h']
            for name in names:
                with self.subTest(name=name):
                    target = root / name
                    original = target.read_bytes()
                    target.write_bytes(original + b'\n// intentional guard probe\n')
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                        oracle.guarded_sources(root)
                    target.write_bytes(original)
            (root / 'src/game/Maps/Map.cpp').unlink()
            with self.assertRaises(FileNotFoundError):
                oracle.guarded_sources(root)

    def test_package_drift_fails_before_compile_and_does_not_rewrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            package = pathlib.Path(temporary) / 'package.txt'
            package.write_text('intentional drift')
            with mock.patch.object(oracle, 'PACKAGE', package), \
                 mock.patch.object(oracle, 'run') as compiler, \
                 mock.patch.object(sys, 'argv', ['ground_height_generate.py', str(SOURCE), '--check']):
                with self.assertRaisesRegex(SystemExit, 'source package differs'):
                    oracle.main()
                compiler.assert_not_called()
            self.assertEqual(package.read_text(), 'intentional drift')

    def test_duplicate_source_or_harness_markers_fail_closed(self):
        with self.assertRaisesRegex(SystemExit, 'Expected one source method'):
            oracle.exact_method('void x() {}\nvoid x() {}\n', 'void x(')
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            original = (oracle.HERE / 'ground_height_oracle.cpp').read_text()
            (root / 'ground_height_oracle.cpp').write_text(original + '\n// PINNED_MAP_UNIT_METHODS\n')
            with mock.patch.object(oracle, 'HERE', root):
                with self.assertRaisesRegex(SystemExit, 'Expected one harness marker'):
                    oracle.build_harness(self.source)

    def test_absent_compiler_is_an_error_never_a_skip(self):
        with mock.patch.object(oracle.subprocess, 'run', side_effect=FileNotFoundError('g++ unavailable')):
            with self.assertRaises(FileNotFoundError):
                oracle.run(['g++', '--version'])

    def test_nonfinite_observations_are_separate(self):
        self.assertEqual(len(self.finite), 1422)
        self.assertEqual(len(self.nonfinite), 641)
        for row in self.finite:
            self.assertIn(row['domain'], ('finite', 'source_only_finite'))
            for field in ('x_bits', 'y_bits', 'z_bits', 'static_reply_bits'):
                self.assertTrue(finite_bits(row[field]), (row['case'], field))
            if row['dynamic_mode'] == 'scripted':
                self.assertTrue(finite_bits(row['dynamic_reply_bits']))
        for row in self.nonfinite:
            self.assertEqual(row['domain'], 'source_only_nonfinite')
            inputs = ['x_bits', 'y_bits', 'z_bits', 'static_reply_bits']
            if row['dynamic_mode'] == 'scripted':
                inputs.append('dynamic_reply_bits')
            self.assertTrue(any(not finite_bits(row[field]) for field in inputs))
            if row['dynamic_mode'] == 'empty':
                self.assertTrue(finite_bits(row['x_bits']))
                self.assertTrue(finite_bits(row['y_bits']))

    def test_exact_callback_forwarding_and_source_empty_miss(self):
        for row in self.finite + self.nonfinite:
            self.assertEqual(row['static_call_count'], '1')
            self.assertEqual(row['dynamic_call_count'], '1')
            self.assertEqual(row['static_use_vmap'], '1')
            self.assertEqual(row['static_search_bits'], '42480000' if row['swim'] == '1' else '41200000')
            for coordinate in ('x', 'y', 'z'):
                self.assertEqual(row[f'static_{coordinate}_bits'], row[f'{coordinate}_bits'])
            for coordinate in ('x', 'y'):
                self.assertEqual(row[f'dynamic_{coordinate}_bits'], row[f'{coordinate}_bits'])
            if row['op'] == 'ground':
                self.assertEqual(row['swim'], '0')
            if row['dynamic_mode'] == 'empty':
                self.assertEqual(row['dynamic_reply_bits'], 'ff800000')

    def test_signed_zero_operand_and_clamp_equality(self):
        zero = [row for row in self.finite if row['label'] == 'signed_zero_equal_operands_and_z']
        self.assertEqual(len(zero), 16)
        for row in zero:
            expected = row['static_reply_bits'] if row['op'] == 'map' else row['z_bits']
            self.assertEqual(row['result_bits'], expected)

    def test_strict_invalid_height_boundary(self):
        matching = [row for row in self.finite
                    if row['label'] == 'invalid_threshold_adjacent' and row['op'] == 'ground']
        self.assertEqual(len(matching), 9)
        for row in matching:
            height = struct.unpack('<f', bytes.fromhex(row['static_reply_bits'])[::-1])[0]
            expected = row['static_reply_bits'] if height > -100000 else row['z_bits']
            self.assertEqual(row['result_bits'], expected)

    def test_pinned_observed_order_controls(self):
        by_label = collections.Counter(row['label'] for row in self.finite)
        self.assertEqual(by_label['binary32_ordered_dynamic_arguments'], 180)
        self.assertEqual(by_label['ground_null_at_map'], 4)
        self.assertEqual(by_label['empty_grid_coordinates'], 48)
        self.assertEqual(by_label['bounded_nonassociative_dynamic_arguments'], 18)
        matched_bounded = matched_large = 0
        for row in self.finite:
            if row['label'] == 'bounded_nonassociative_dynamic_arguments' \
                    and row['static_reply_bits'] == '449cfd56' and row['z_bits'] == '457feb55':
                matched_bounded += 1
                self.assertEqual(row['dynamic_z_bits'], '458005aa')
                self.assertEqual(row['dynamic_range_bits'], '45318ca9')
                self.assertNotEqual(row['dynamic_range_bits'], '45318caa')
            if row['label'] == 'binary32_ordered_dynamic_arguments' \
                    and row['static_reply_bits'] == '4b800001' and row['z_bits'] == '4b800001':
                matched_large += 1
                # 16777218 + 2 rounds first; subtracting the static operand yields 2.
                self.assertEqual(row['dynamic_z_bits'], '4b800002')
                self.assertEqual(row['dynamic_range_bits'], '40000000')
        self.assertEqual(matched_bounded, 2)
        self.assertEqual(matched_large, 2)


if __name__ == '__main__':
    unittest.main(verbosity=2)
