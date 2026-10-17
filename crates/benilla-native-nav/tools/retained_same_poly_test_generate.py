#!/usr/bin/env python3
"""Fail-closed source, live-sequence and original retained-distance guards."""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import json
import sys
import tempfile
import unittest
from unittest import mock
sys.dont_write_bytecode = True
import retained_same_poly_generate as oracle
ROOT = Path(sys.argv.pop(1)).resolve()


class RetainedSamePolygonGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = oracle.point.guarded_sources(ROOT)
        oracle.guard_inputs()
        cls.data = (oracle.FIXTURES / 'cases.test-fixture').read_bytes()
        cls.rows = oracle.parse_cases(cls.data)

    def named(self, label):
        return next(row for row in self.rows if row['label'] == label)

    def test_original_retained_prefix_and_unchanged_cold_repair(self):
        harness, package = oracle.build_harness(self.sources)
        self.assertEqual(package, oracle.PACKAGE.read_text())
        source = self.sources[oracle.point.CPP]
        prefix = oracle.point.source_span(source, oracle.cold.ORIGINAL, '    dtStatus dtResult;')
        retained = oracle.point.exact_block(harness, 'void PathFinder::original_retained_same_polygon(')
        self.assertIn('require(m_polyLength>0,', retained)
        self.assertIn('m_navMesh->isValidPolyRef(m_pathPolyRefs[i])', retained)
        self.assertIn(prefix, retained.replace('    evidence.afterStart = m_pathPolyRefs;\n', '', 1))
        self.assertIn(oracle.cold.ORIGINAL, retained)
        self.assertNotIn(oracle.cold.REPAIRED, retained)
        cold = oracle.point.exact_block(harness, 'void PathFinder::compatibility_same_polygon(')
        self.assertIn(oracle.cold.REPAIRED, cold)
        self.assertNotIn(oracle.cold.ORIGINAL, cold)
        original_cold_harness, cold_package = oracle.cold_same.build_harness(self.sources)
        self.assertEqual(cold_package, oracle.cold_same.PACKAGE.read_text())
        self.assertEqual(cold, oracle.point.exact_block(original_cold_harness, 'void PathFinder::compatibility_same_polygon('))
        for signature, _ in oracle.point.METHODS:
            self.assertIn(oracle.point.exact_block(source, signature), harness)
        same = oracle.point.source_span(source, '    // start and end are on same polygon',
                                       '    // look for startPoly/endPoly in current path')
        self.assertIn(same, retained)
        self.assertNotIn('m_defaultNavMeshQuery', harness)
        self.assertNotIn('void PathFinder::BuildPolyPath(', harness)

    def test_exactly_one_live_pathfinder_and_query_across_recorded_sequence(self):
        harness, _ = oracle.build_harness(self.sources)
        main = oracle.point.exact_block(harness, 'int main(int argc,char** argv)')
        self.assertEqual(main.count('Loaded mesh('), 1)
        self.assertEqual(main.count('PathFinder path('), 1)
        self.assertIn('path.m_straightLine=true;', main)
        self.assertIn('dtNavMeshQuery* const live_query=path.m_navMeshQuery;', main)
        self.assertIn('path.m_navMeshQuery==live_query&&live_query==&mesh.query', main)
        step = oracle.point.exact_block(harness, 'static ColdEvidence retained_same_step(')
        self.assertNotIn('PathFinder path(', step)
        self.assertIn('if(step==0)path.compatibility_same_polygon(', step)
        self.assertIn('else path.original_retained_same_polygon(', step)
        self.assertIn('require(step==0?active==0:active>0,', step)
        self.assertEqual(len(self.rows), 11)
        self.assertEqual(self.rows[0]['oracle'], 'compatibility-zero-v1')
        self.assertEqual(oracle.point.ref_list(self.rows[0]['before_storage']), [0] * 74)
        for previous, current in zip(self.rows, self.rows[1:]):
            self.assertEqual(current['oracle'], 'original-retained-defined')
            self.assertEqual(current['before_active'], previous['after_active'])
            self.assertEqual(current['before_storage'], previous['after_storage'])
            self.assertEqual(int(current['step']), int(previous['step']) + 1)

    def test_query1024_explicit_normalization_and_exact_xyz_terminals(self):
        harness, _ = oracle.build_harness(self.sources)
        self.assertIn('query->init(mmap->navMesh, 1024)', self.sources[oracle.point.MOVEMAP])
        step = oracle.point.exact_block(harness, 'static ColdEvidence retained_same_step(')
        self.assertIn('const auto nodes=path.m_navMeshQuery->getNodePool()->getMaxNodes();', step)
        self.assertIn('require(nodes==1024,', step)
        self.assertIn('!normalize_z&&!ignore_normalization', step)
        self.assertIn('path.m_sourceUnit!=nullptr', step)
        self.assertEqual(step.count('require(path.m_straightLine,'), 2)
        for row in self.rows:
            with self.subTest(label=row['label']):
                self.assertEqual(row['query_nodes'], '1024')
                self.assertEqual(row['normalize_z'], '0')
                self.assertEqual(row['ignore_normalization'], '0')
                self.assertEqual(row['start_ref'], row['end_ref'])
                self.assertEqual(row['after_active'], '1')
                self.assertEqual(row['path_type'], '1')
                self.assertEqual(row['actual_end'], row['to'])
                self.assertEqual(row['points'], row['from'] + ';' + row['to'])
        vertical = self.named('retained_active_vertical')
        self.assertEqual(vertical['to'], '40a00000,40e00000,3f400000')
        self.assertEqual(vertical['end_distance'], '3f400000')
        identical = self.named('retained_identical')
        self.assertEqual(identical['from'], identical['to'])
        self.assertEqual(len(oracle.point.points(identical['points'])), 2)

    def test_active_tail_equality_and_nearest_distance_cases(self):
        for label in ('retained_tail_squared4', 'retained_new_tail_squared4'):
            row = self.named(label)
            self.assertEqual(row['start_distance'], '40000000')
            self.assertEqual(row['end_distance'], '40000000')
            self.assertGreater(oracle.point.finite_bits(row['start_distance']) ** 2, 3)
            self.assertLessEqual(oracle.point.finite_bits(row['start_distance']) ** 2, 7)
            self.assertEqual(row['before_storage'], row['after_start_storage'])
            self.assertEqual(row['before_storage'], row['lookup_storage'])
        equal = self.named('retained_nearest_equal')
        self.assertEqual(equal['start_distance'], '40400000')
        self.assertEqual(equal['end_distance'], '40400000')
        self.assertEqual(equal['lookup_storage'], equal['before_storage'])
        self.assertEqual(self.named('retained_active_squared3_equal')['start_distance'], '3fddb3d7')
        self.assertEqual(self.named('retained_tail_squared7_equal')['start_distance'], '402953fd')
        far = self.named('retained_nearest10_equal')
        self.assertEqual(far['start_distance'], '40c00000')
        self.assertEqual(far['before_storage'], far['lookup_storage'])

    def test_new_polygon_append_end_tail_and_return_prior_storage(self):
        initial = self.rows[0]
        a = int(initial['start_ref']); b = a + 1
        self.assertEqual(oracle.point.ref_list(initial['after_storage']), [a] + [0] * 73 + [a])
        append = self.named('retained_new_polygon')
        self.assertEqual(int(append['start_ref']), b)
        before = oracle.point.ref_list(append['before_storage'])
        self.assertEqual(before, [a] + [0] * 73 + [a])
        self.assertEqual(oracle.point.ref_list(append['after_start_storage']), before + [b])
        self.assertEqual(append['lookup_storage'], append['after_start_storage'])
        self.assertEqual(oracle.point.ref_list(append['after_storage']), [b] + [0] * 73 + [a, b])
        reuse = self.named('retained_new_tail_squared4')
        self.assertEqual(int(reuse['start_ref']), b)
        self.assertEqual(reuse['before_storage'], reuse['after_storage'])
        prior = self.named('retained_return_prior')
        self.assertEqual(oracle.point.ref_list(prior['before_storage']), [b] + [0] * 73 + [a, b])
        self.assertEqual(prior['before_storage'], prior['lookup_storage'])
        self.assertEqual(oracle.point.ref_list(prior['after_storage']), [a] + [0] * 73 + [a, b])

    def test_schema_rejects_reset_sequence_policy_and_changed_terminal(self):
        lines = self.data.decode('ascii').splitlines()
        indexes = [i for i, line in enumerate(lines) if not line.startswith('#')]
        row_index = indexes[1]
        values = lines[row_index].split('\t')
        for field, invalid in (('sequence', 'recreated'), ('step', '0'), ('oracle', 'compatibility-zero-v1'),
                               ('normalize_z', '1'), ('ignore_normalization', '1'), ('query_nodes', '512'),
                               ('before_active', '0'), ('before_storage', self.rows[0]['before_storage']),
                               ('after_start_storage', '0'), ('lookup_storage', self.rows[0]['before_storage']),
                               ('after_active', '0'), ('after_storage', self.rows[0]['before_storage']),
                               ('path_type', '8'), ('actual_end', self.rows[1]['from']), ('points', '-'), ('end_ref', '0')):
            changed = values.copy(); changed[oracle.FIELDS.index(field)] = invalid
            attempt = lines.copy(); attempt[row_index] = '\t'.join(changed)
            with self.subTest(field=field), self.assertRaises(SystemExit):
                oracle.parse_cases(('\n'.join(attempt) + '\n').encode('ascii'))
        with self.assertRaises(SystemExit):
            oracle.parse_cases(('\n'.join(lines[:indexes[2]]) + '\n').encode('ascii'))

    def test_shared_compiler_copies_original_dependencies_without_extra_repairs(self):
        with tempfile.TemporaryDirectory(prefix='retained-same-guard-') as directory:
            work = Path(directory)
            symbols = '\n'.join(signature.split(' ', 1)[1].split('(')[0] + '('
                                for signature, _ in oracle.point.METHODS)
            symbols += '\ndtNavMeshQuery::findPath(\ndtNavMeshQuery::findStraightPath(\ndtNavMeshQuery::moveAlongSurface(\n'
            symbols += 'G3D::Vector3::Vector3()\nG3D::Vector3::squaredLength() const\ndtCreateNavMeshData(\n'
            symbols += 'PathFinder::compatibility_same_polygon(\nPathFinder::original_retained_same_polygon(\n'
            commands = []
            def fake_run(command, **_kwargs):
                commands.append(command)
                output = b''
                if '-dumpfullversion' in command: output = b'14.2.0\n'
                elif '-dumpmachine' in command: output = b'x86_64-linux-gnu\n'
                elif command[0] == 'nm': output = symbols.encode()
                return mock.Mock(stdout=output)
            with mock.patch.object(oracle.point, 'run', side_effect=fake_run), \
                 mock.patch.object(oracle.cold_same.shutil, 'which', return_value='/usr/bin/g++'), \
                 mock.patch.object(oracle.point, 'repair_tile_header', side_effect=AssertionError('unapproved tile repair')):
                oracle.compile_oracle(self.sources, work, receipts=work / 'receipts')
            profile = json.loads((work / 'receipts/profile.json').read_text())
            self.assertIs(profile['straight_line'], True)
            for name, content in self.sources.items():
                if name.startswith(('dep/recastnavigation/', 'dep/g3dlite/')):
                    self.assertEqual((work / name).read_bytes(), content.encode('latin1'), name)
            command = next(command for command in commands if '-std=c++20' in command)
            self.assertFalse(any('REPAIR_TILE' in word or 'bridge.cpp' in word for word in command))
            self.assertEqual({Path(word).name for word in command if word.endswith('.cpp')},
                             {'oracle.cpp'} | {name + '.cpp' for name in oracle.point.DETOUR_FILES})
        self.assertIn(b'raw-storage lifetime caveat remains', self.data)


if __name__ == '__main__': unittest.main()
