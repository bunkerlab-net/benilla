#!/usr/bin/env python3
"""Fail-closed guards for selected cold same-polygon compatibility evidence."""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
sys.dont_write_bytecode = True
import cold_same_poly_generate as oracle
ROOT = Path(sys.argv.pop(1)).resolve()


class ColdSamePolygonGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = oracle.point.guarded_sources(ROOT)
        oracle.guard_inputs()
        cls.data = (oracle.FIXTURES / 'cases.test-fixture').read_bytes()
        cls.rows = oracle.parse_cases(cls.data)

    def named(self, label):
        return next(row for row in self.rows if row['label'] == label)

    def test_exact_source_fragments_and_only_two_distance_locals_repaired(self):
        harness, package = oracle.build_harness(self.sources)
        self.assertEqual(package, oracle.PACKAGE.read_text())
        source = self.sources[oracle.point.CPP]
        prefix = oracle.point.source_span(source, oracle.cold.ORIGINAL, '    dtStatus dtResult;')
        repaired = oracle.cold.repair_distance_initialization(prefix)
        self.assertEqual(repaired.replace(oracle.cold.REPAIRED, oracle.cold.ORIGINAL, 1), prefix)
        body = oracle.point.exact_block(harness, 'void PathFinder::compatibility_same_polygon(')
        self.assertIn(oracle.cold.REPAIRED, body)
        self.assertNotIn(oracle.cold.ORIGINAL, body)
        self.assertNotIn('m_defaultNavMeshQuery', harness)
        self.assertNotIn('void PathFinder::BuildPolyPath(', harness)
        for signature, _ in oracle.point.METHODS:
            self.assertIn(oracle.point.exact_block(source, signature), harness)
        same = oracle.point.source_span(source, '    // start and end are on same polygon',
                                       '    // look for startPoly/endPoly in current path')
        self.assertIn(same, body)
        constructor = oracle.point.exact_block(source, 'PathFinder::PathFinder(const Unit* owner,')
        initializers = oracle.point.source_span(constructor,
            '    m_pointPathLimit(MAX_POINT_PATH_LENGTH)', ' m_sourceUnit(owner)')[:-1]
        self.assertIn(initializers, harness)
        cache_members = oracle.point.source_span(self.sources[oracle.point.HEADER],
            '        uint32         m_pointPathLimit;', '        Vector3        m_startPosition;')
        self.assertIn(cache_members, harness)
        self.assertIn(cache_members, package)
        self.assertIn(constructor, package)

    def test_source_query_capacity_and_explicit_normalization_inputs(self):
        harness, _ = oracle.build_harness(self.sources)
        self.assertIn('query->init(mmap->navMesh, 1024)', self.sources[oracle.point.MOVEMAP])
        self.assertIn('const auto nodes=mesh.query.getNodePool()->getMaxNodes();', harness)
        self.assertIn('require(nodes==1024,', harness)
        self.assertIn('!normalize_z&&!ignore_normalization', harness)
        self.assertIn('path.m_sourceUnit!=nullptr', harness)
        self.assertEqual({row['query_nodes'] for row in self.rows}, {'1024'})
        self.assertEqual({row['normalize_z'] for row in self.rows}, {'0'})
        self.assertEqual({row['ignore_normalization'] for row in self.rows}, {'0'})

    def test_cold_constructor_zero_append_and_retained_tail_terminal(self):
        self.assertEqual(len(self.rows), 4)
        for row in self.rows:
            with self.subTest(row=row['label']):
                ref = int(row['start_ref'])
                before = oracle.point.ref_list(row['constructor_storage'])
                after_start = oracle.point.ref_list(row['after_start_storage'])
                lookup = oracle.point.ref_list(row['lookup_storage'])
                after = oracle.point.ref_list(row['after_storage'])
                self.assertEqual(row['constructor_active'], '0')
                self.assertEqual(before, [0] * 74)
                self.assertEqual(after_start, [0] * 74 + [ref])
                self.assertEqual(lookup, after_start)
                self.assertEqual(after, [ref] + [0] * 73 + [ref])
                self.assertEqual(row['after_active'], '1')
                self.assertEqual(row['path_type'], '1')
                self.assertEqual(row['end_ref'], row['start_ref'])

    def test_exact_xyz_points_actual_end_and_nonzero_distances(self):
        for row in self.rows:
            self.assertEqual(row['points'], row['from'] + ';' + row['to'])
            self.assertEqual(row['actual_end'], row['to'])
        vertical = self.named('cold_same_vertical')
        self.assertEqual(vertical['from'], '40a00000,40a00000,00000000')
        self.assertEqual(vertical['to'], '40a00000,40c00000,3f400000')
        self.assertEqual(vertical['end_distance'], '3f400000')
        near10 = self.named('cold_same_nearest10')
        self.assertEqual(near10['start_distance'], '40c00000')
        self.assertEqual(near10['end_distance'], '00000000')
        self.assertEqual(self.named('cold_same_tail')['start_distance'], '00000000')

    def test_schema_rejects_unadmitted_policy_flags_query_and_cache(self):
        text = self.data.decode('ascii')
        lines = text.splitlines()
        row_index = next(i for i, line in enumerate(lines) if not line.startswith('#'))
        values = lines[row_index].split('\t')
        for field, invalid in (('oracle', 'original-parity'), ('normalize_z', '1'),
                               ('ignore_normalization', '1'), ('query_nodes', '512'),
                               ('constructor_active', '1'), ('constructor_storage', '0'),
                               ('path_type', '8'), ('points', '-'), ('end_ref', '0')):
            changed = values.copy(); changed[oracle.FIELDS.index(field)] = invalid
            attempt = lines.copy(); attempt[row_index] = '\t'.join(changed)
            with self.subTest(field=field), self.assertRaises(SystemExit):
                oracle.parse_cases(('\n'.join(attempt) + '\n').encode('ascii'))

    def test_no_extra_source_repairs_or_production_bridge_in_compiler(self):
        # The compile wrapper must copy every original header byte-for-byte;
        # neither the inherited optional tile-repair lane nor production sources run.
        with tempfile.TemporaryDirectory(prefix='cold-same-guard-') as directory:
            work = Path(directory)
            symbols = '\n'.join(signature.split(' ', 1)[1].split('(')[0] + '('
                                for signature, _ in oracle.point.METHODS)
            symbols += '\ndtNavMeshQuery::findPath(\ndtNavMeshQuery::findStraightPath(\ndtNavMeshQuery::moveAlongSurface(\n'
            symbols += 'G3D::Vector3::Vector3()\nG3D::Vector3::squaredLength() const\ndtCreateNavMeshData(\n'
            symbols += 'PathFinder::compatibility_same_polygon(\n'
            commands = []
            def fake_run(command, **_kwargs):
                commands.append(command)
                output = b''
                if '-dumpfullversion' in command: output = b'14.2.0\n'
                elif '-dumpmachine' in command: output = b'x86_64-linux-gnu\n'
                elif command[0] == 'nm': output = symbols.encode()
                return mock.Mock(stdout=output)
            with mock.patch.object(oracle.point, 'run', side_effect=fake_run), \
                 mock.patch.object(oracle.shutil, 'which', return_value='/usr/bin/g++'), \
                 mock.patch.object(oracle.point, 'repair_tile_header', side_effect=AssertionError('unapproved tile repair')):
                oracle.compile_oracle(self.sources, work)
            for name, content in self.sources.items():
                if name.startswith(('dep/recastnavigation/', 'dep/g3dlite/')):
                    self.assertEqual((work / name).read_bytes(), content.encode('latin1'), name)
            compile_command = next(command for command in commands if '-std=c++20' in command)
            self.assertFalse(any('REPAIR_TILE' in word or 'bridge.cpp' in word for word in compile_command))
            self.assertEqual({Path(word).name for word in compile_command if word.endswith('.cpp')},
                             {'oracle.cpp'} | {name + '.cpp' for name in oracle.point.DETOUR_FILES})

    def test_original_undefined_cold_remains_excluded(self):
        warm_rows = oracle.cold.warm.parse_cases((oracle.cold.warm.FIXTURES / 'cases.test-fixture').read_bytes())
        excluded = next(row for row in warm_rows if row['label'] == 'cold_without_tail_excluded')
        self.assertEqual(excluded['scope'], 'excluded')
        self.assertTrue(all(row['oracle'] == 'compatibility-zero-v1' for row in self.rows))
        self.assertIn(b'raw-storage lifetime caveat remains', self.data)


if __name__ == '__main__': unittest.main()
