#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Guard the additional swimming oracle extraction; argument is pinned source root."""
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import swimming_allowed_z_generate as oracle

ROOT = pathlib.Path(sys.argv.pop(1)).resolve()


class SwimmingSourceGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = oracle.base.guarded_sources(ROOT)

    def test_full_source_methods_and_default_declaration_are_verbatim(self):
        harness, package = oracle.build(ROOT)
        for file, signature in (
            ('src/game/Maps/Map.cpp', 'float Map::GetHeight('),
            ('src/game/Entities/Unit.cpp', 'void Unit::UpdateAllowedPositionZ('),
            ('src/game/Maps/GridMap.cpp', 'float TerrainInfo::GetWaterOrGroundLevel('),
        ):
            source = (ROOT / file).read_text()
            method = oracle.base.exact_method(source, signature)
            self.assertIn(method, harness)
            self.assertIn(method, package)
        declaration = 'GridMapLiquidStatus getLiquidStatus(float x, float y, float z, uint8 ReqLiquidType, GridMapLiquidData* data = nullptr, float collisionHeight = 2.03128f) const;'
        self.assertIn(declaration, harness)
        self.assertIn(declaration, package)
        self.assertEqual(package, oracle.PACKAGE.read_text())

    def test_each_new_full_file_hash_rejects_one_byte_mutation(self):
        entries = [line.split('  ', 1)[1] for line in oracle.EXTRA.read_text().splitlines()]
        for changed in entries:
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as temporary:
                root = pathlib.Path(temporary)
                for name in entries:
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes((ROOT / name).read_bytes() + (b'\n' if name == changed else b''))
                with mock.patch.object(oracle.base, 'guarded_sources', return_value=dict(self.source)):
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                        oracle.build(root)

    def test_missing_source_is_not_a_skip(self):
        with tempfile.TemporaryDirectory() as temporary:
            with mock.patch.object(oracle.base, 'guarded_sources', return_value=dict(self.source)):
                with self.assertRaises(FileNotFoundError):
                    oracle.build(pathlib.Path(temporary))

    def test_duplicate_or_missing_harness_markers_reject(self):
        harness = (oracle.HERE / 'swimming_allowed_z_oracle.cpp').read_text()
        for replacement in ['', '// PINNED_METHODS\n// PINNED_METHODS']:
            with self.subTest(replacement=replacement), tempfile.TemporaryDirectory() as temporary:
                here = pathlib.Path(temporary)
                (here / 'swimming_allowed_z_oracle.cpp').write_text(harness.replace('// PINNED_METHODS', replacement))
                with mock.patch.object(oracle, 'HERE', here):
                    with self.assertRaisesRegex(SystemExit, 'Missing/duplicate marker'):
                        oracle.build(ROOT)

    def test_fixture_has_dry_and_all_nonzero_status_controls_without_conflating_heights(self):
        rows = [line.split('\t') for line in oracle.FIXTURE.read_text().splitlines() if not line.startswith('#')]
        self.assertEqual(len(rows), 865)
        self.assertEqual({int(row[4]) for row in rows}, {0, 1, 2, 4, 8, 15})
        self.assertTrue(any(row[5] != row[14] and row[7] == '1' for row in rows))
        for row in rows:
            self.assertEqual(len(row), 17)
            self.assertEqual(row[8], '42480000')
            self.assertEqual(int(row[11]), int(row[7]))
            if row[7] == '1':
                self.assertEqual(row[12], row[2])
                self.assertEqual(row[13], '15')
                self.assertEqual(row[14], '4002007e')


unittest.main()
