#!/usr/bin/env python3
"""Guards for the pinned first-candidate source observations; authored inputs only."""
# SPDX-License-Identifier: GPL-3.0-or-later
import pathlib
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import near_point_primary_generate as oracle

MANGOS = pathlib.Path(sys.argv.pop(1)).resolve()


def read_rows():
    fields = ('case', 'label') + oracle.INPUT_FIELDS + oracle.OUTPUT_FIELDS
    numeric_hex = set(oracle.FLOAT_FIELDS) | {
        'result_x', 'result_y', 'result_z', 'ground_x', 'ground_y', 'ground_z',
        'los_src_x', 'los_src_y', 'los_src_z', 'los_dest_x', 'los_dest_y', 'los_dest_z'}
    result = []
    for line in oracle.FIXTURE.read_text().splitlines():
        if line.startswith('#'):
            continue
        items = line.split('\t')
        if len(items) != len(fields):
            raise AssertionError('Fixture schema mismatch')
        row = dict(zip(fields, items, strict=True))
        for field in fields:
            if field in numeric_hex:
                row[field] = int(row[field], 16)
            elif field not in ('label', 'outcome', 'trace'):
                row[field] = int(row[field])
        result.append(row)
    return result


class PrimaryOracleGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = oracle.guarded_sources(MANGOS)
        cls.rows = read_rows()
        cls.temporary = tempfile.TemporaryDirectory(prefix='test-primary-source-')
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.binary = oracle.compile_oracle(cls.source, pathlib.Path(cls.temporary.name), False)

    def named(self, name):
        return [row for row in self.rows if row['label'] == name]

    def test_full_file_hash_guards_and_missing_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            for name in self.source:
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(MANGOS / name, target)
            self.assertEqual(len(self.source), 9)
            for name in self.source:
                with self.subTest(name=name):
                    path = root / name
                    raw = path.read_bytes()
                    path.write_bytes(raw + b'\n/* intentional guard probe */\n')
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                        oracle.guarded_sources(root)
                    path.write_bytes(raw)
            (root / 'src/game/Maps/ObjectPosSelector.h').unlink()
            with self.assertRaises(FileNotFoundError):
                oracle.guarded_sources(root)

    def test_exact_complete_methods_header_notices_and_source_order(self):
        harness, package = oracle.build_harness(self.source)
        self.assertEqual(package, oracle.PACKAGE.read_text(encoding='latin1'))
        for name, signature in (
            ('src/game/Entities/Object.cpp', 'void WorldObject::GetNearPointAt('),
            ('src/game/Entities/Object.cpp', 'bool WorldObject::IsWithinLOS('),
            ('src/game/Entities/Unit.cpp', 'void Unit::UpdateAllowedPositionZ('),
            ('src/game/Maps/Map.cpp', 'bool Map::IsInLineOfSight('),
            ('src/game/Maps/ObjectPosSelector.cpp', 'ObjectPosSelector::ObjectPosSelector('),
            ('src/game/Maps/ObjectPosSelector.cpp', 'bool ObjectPosSelector::CheckOriginalAngle('),
        ):
            method = oracle.exact_method(self.source[name], signature)
            self.assertIn(method, harness)
            self.assertIn(method, package)
        self.assertIn(self.source['src/game/Maps/ObjectPosSelector.h'], package)
        for text in self.source.values():
            self.assertIn(text[:text.index('*/') + 2], package)
        near = oracle.exact_method(self.source['src/game/Entities/Object.cpp'],
                                   'void WorldObject::GetNearPointAt(')
        tokens = ('GetNearPoint2dAt(', 'const float init_z = z = posZ;',
                  'if (!sWorld.getConfig(', 'const float dist = distance2d + searcher_bounding_radius + GetObjectBoundingRadius();',
                  'ObjectPosSelector selector(', '//    Cell::VisitAllObjects(',
                  'if (selector.CheckOriginalAngle())',
                  'if (std::abs(init_z - z) < dist && IsWithinLOS(x, y, z))',
                  'selector.InitializeAngle();')
        positions = [near.index(token) for token in tokens]
        self.assertEqual(positions, sorted(positions))
        self.assertIn('bool ignoreM2Model = false', package)
        self.assertIn('setConfig(CONFIG_BOOL_DETECT_POS_COLLISION, "DetectPosCollision", true);', package)
        check = oracle.exact_method(self.source['src/game/Maps/ObjectPosSelector.cpp'],
                                    'bool ObjectPosSelector::CheckOriginalAngle(')
        self.assertNotIn('m_searchedForReqHAngle', check)
        self.assertEqual(check.count('.empty()'), 2)

    def test_package_drift_fails_before_compilation_without_overwrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = pathlib.Path(temporary) / 'package.txt'
            path.write_text('intentional package drift')
            with mock.patch.object(oracle, 'PACKAGE', path), \
                 mock.patch.object(oracle, 'compile_oracle') as compiler, \
                 mock.patch.object(sys, 'argv', ['oracle.py', str(MANGOS), '--check']):
                with self.assertRaisesRegex(SystemExit, 'source package differs'):
                    oracle.main()
                compiler.assert_not_called()
            self.assertEqual(path.read_text(), 'intentional package drift')

    def test_duplicate_missing_source_and_marker_guards(self):
        for text in ('void f() {}\nvoid f() {}', 'void g() {}'):
            with self.assertRaisesRegex(SystemExit, 'Expected one source method'):
                oracle.exact_method(text, 'void f(')
        with self.assertRaisesRegex(SystemExit, 'Unclosed source method'):
            oracle.exact_method('void f() {', 'void f(')
        for text in ('#define A 1\n#define A 2', '#define B 1'):
            with self.assertRaisesRegex(SystemExit, 'Expected one source declaration'):
                oracle.exact_line(text, r'^#define A .*$')
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            original = (oracle.HERE / 'near_point_primary_oracle.cpp').read_text()
            (root / 'near_point_primary_oracle.cpp').write_text(original + '\n// PINNED_CONSTANTS\n')
            with mock.patch.object(oracle, 'HERE', root):
                with self.assertRaisesRegex(SystemExit, 'Expected one harness marker'):
                    oracle.build_harness(self.source)

    def test_missing_compiler_is_error_never_skip(self):
        with mock.patch.object(oracle.subprocess, 'run', side_effect=FileNotFoundError('g++ missing')):
            with self.assertRaises(FileNotFoundError):
                oracle.run(['g++', '--version'])

    def test_fixture_reproduction_and_case_schema(self):
        self.assertEqual(oracle.observe(self.binary, oracle.cases()), oracle.FIXTURE.read_bytes())
        self.assertEqual(len(self.rows), 301)
        self.assertEqual([r['case'] for r in self.rows], list(range(301)))
        self.assertTrue(all(path.suffix == '.test-fixture' for path in oracle.FIXTURE.parent.iterdir()))

    def test_order_and_short_circuit_counts(self):
        traces = set()
        for row in self.rows:
            with self.subTest(case=row['case']):
                traces.add(row['trace'])
                self.assertEqual((row['xy_calls'], row['ground_calls'], row['config_calls']), (1, 1, 1))
                self.assertEqual((row['ground_x'], row['ground_y'], row['ground_z']),
                                 (row['candidate_x'], row['candidate_y'], row['pos_z']))
                self.assertEqual((row['result_x'], row['result_y']), (row['candidate_x'], row['candidate_y']))
                self.assertEqual((row['ground_swim'], row['ignore_m2']), (0, 0))
                if not row['detect']:
                    self.assertEqual(row['outcome'], 'collision-disabled')
                    self.assertEqual(row['trace'], 'ICH')
                    self.assertEqual((row['radius_calls'], row['static_calls'], row['dynamic_calls'], row['fallback_calls']), (0, 0, 0, 0))
                else:
                    self.assertEqual(row['radius_calls'], 1)
                    if not row['static_calls']:
                        self.assertEqual(row['trace'], 'ICRHF')
                        self.assertEqual(row['dynamic_calls'], 0)
                    elif not row['static_los']:
                        self.assertEqual(row['trace'], 'ICRHPSF')
                        self.assertEqual(row['dynamic_calls'], 0)
                    else:
                        self.assertEqual(row['dynamic_calls'], 1)
                        self.assertEqual(row['trace'], 'ICRHPSD' + ('' if row['dynamic_los'] else 'F'))
                    self.assertEqual(row['fallback_calls'], int(row['outcome'] == 'fallback-required'))
        self.assertEqual(traces, {'ICH', 'ICRHF', 'ICRHPSF', 'ICRHPSDF', 'ICRHPSD'})

    def test_strict_distance_and_left_associative_sum(self):
        for row in self.named('strict_height_difference_boundary'):
            magnitude = row['ground'] & 0x7fffffff
            self.assertEqual(row['outcome'], 'primary-accepted' if magnitude < oracle.bits(4) else 'fallback-required')
            self.assertEqual(row['static_calls'], int(magnitude < oracle.bits(4)))
        row, = self.named('left_associated_distance_sum')
        self.assertEqual(row['ground'], oracle.bits(1))
        self.assertEqual(row['outcome'], 'fallback-required')
        self.assertEqual(row['static_calls'], 0)
        # Diagnostic alternate grouping, never an oracle expected-value producer.
        self.assertEqual(oracle.bits(1 + oracle.value(0x33800000) * 2), 0x3f800001)
        rows = self.named('ordered_sum_and_zero_radius')
        zero = [r for r in rows if r['distance'] & 0x7fffffff == 0 and
                r['searcher_radius'] & 0x7fffffff == 0 and r['target_radius'] & 0x7fffffff == 0]
        self.assertTrue(zero)
        self.assertTrue(all(r['outcome'] == 'fallback-required' and not r['static_calls'] for r in zero))

    def test_invalid_ground_and_signed_zero_are_not_invented_support_rejections(self):
        for label in ('invalid_ground_still_accepts', 'invalid_threshold_still_accepts'):
            row, = self.named(label)
            self.assertEqual(row['result_z'], row['pos_z'])
            self.assertEqual(row['outcome'], 'primary-accepted')
        row, = self.named('no_extra_support_tolerance')
        self.assertEqual(row['result_z'], oracle.bits(3.5))
        self.assertEqual(row['outcome'], 'primary-accepted')
        for row in self.named('signed_zero_preservation'):
            self.assertEqual(row['result_z'], row['pos_z'])

    def test_los_origin_eye_endpoint_raw_adjusted_z_and_ignore_m2_false(self):
        for row in self.rows:
            if row['static_calls']:
                self.assertEqual((row['los_src_x'], row['los_src_y']), (row['target_x'], row['target_y']))
                self.assertEqual(row['los_src_z'], oracle.bits(oracle.value(row['target_z']) + oracle.value(row['collision_height'])))
                self.assertEqual((row['los_dest_x'], row['los_dest_y'], row['los_dest_z']),
                                 (row['candidate_x'], row['candidate_y'], row['result_z']))
                self.assertEqual(row['ignore_m2'], 0)
        row, = self.named('actual_target_pose_is_los_origin')
        self.assertNotEqual(row['los_src_x'], row['pos_x'])
        row, = self.named('endpoint_is_adjusted_feet')
        self.assertEqual((row['los_src_z'], row['los_dest_z']), (oracle.bits(11.75), oracle.bits(12)))

    def test_authored_wrapper_domain_rejections_and_partial_record(self):
        baseline = oracle.cases()[0][1]
        for field in range(14):
            for invalid in (0x7f800000, 0xff800000, 0x7fc12345):
                row = list(baseline)
                row[field] = invalid
                self.assertEqual(subprocess.run([str(self.binary)], input=struct.pack('<17I', *row),
                                               stdout=subprocess.PIPE).returncode, 3)
        for field in (3, 4, 5, 13):
            row = list(baseline)
            row[field] = oracle.bits(-1)
            self.assertEqual(subprocess.run([str(self.binary)], input=struct.pack('<17I', *row),
                                           stdout=subprocess.PIPE).returncode, 3)
        for field in (14, 15, 16):
            row = list(baseline)
            row[field] = 2
            self.assertEqual(subprocess.run([str(self.binary)], input=struct.pack('<17I', *row),
                                           stdout=subprocess.PIPE).returncode, 4)
        self.assertEqual(subprocess.run([str(self.binary)], input=b'partial', stdout=subprocess.PIPE).returncode, 6)


if __name__ == '__main__':
    unittest.main(verbosity=2)
