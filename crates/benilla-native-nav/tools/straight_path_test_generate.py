#!/usr/bin/env python3
"""Fail-closed unchanged straight-line source observer and corpus guards."""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import hashlib
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
sys.dont_write_bytecode = True
import straight_path_generate as oracle
ROOT = Path(sys.argv.pop(1)).resolve()


class StraightPathOracleGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = oracle.guarded_sources(ROOT)
        cls.products = {name: (oracle.FIXTURES / name).read_bytes() for name in oracle.FILES}
        cls.rows, cls.sequences, cls.traces, cls.domains = oracle.verify_products(cls.products)
        cls.temp = tempfile.TemporaryDirectory(prefix='test-straight-path-')
        cls.addClassCleanup(cls.temp.cleanup)
        cls.work = Path(cls.temp.name)
        cls.binary = oracle.compile_oracle(cls.sources, cls.work)
        cls.observed = oracle.observe(cls.binary, cls.work)

    def named(self, name, sequence=False):
        return next(row for row in (self.sequences if sequence else self.rows) if row['label'] == name)

    def events(self, name, primitive=None):
        return [row for row in self.traces if row['label'].rpartition('_')[0] == name and
                (primitive is None or row['primitive'] == primitive)]

    @staticmethod
    def table(fields, row):
        return ('# ' + '\t'.join(fields) + '\n' + '\t'.join(row[f] for f in fields) + '\n').encode()

    def changed_product(self, filename, fields, index, field, value):
        rows = oracle.parse_table(self.products[filename], fields)
        rows[index][field] = value
        data = b'# ' + '\t'.join(fields).encode() + b'\n'
        data += b''.join(('\t'.join(row[f] for f in fields) + '\n').encode() for row in rows)
        return self.products | {filename: data}

    def test_source_hash_closure_lf_crlf_and_every_file_drift(self):
        self.assertEqual(set(self.sources), oracle.SOURCE_FILES)
        self.assertEqual(len(self.sources), 47)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, source in self.sources.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(source.encode('latin1').replace(b'\n', b'\r\n'))
            self.assertEqual(oracle.guarded_sources(root), self.sources)
            for name, source in self.sources.items():
                with self.subTest(name=name):
                    (root / name).write_bytes((source + '\n// drift\n').encode('latin1'))
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                        oracle.guarded_sources(root)
                    (root / name).write_bytes(source.encode('latin1'))
            (root / oracle.CPP).unlink()
            with self.assertRaises(FileNotFoundError):
                oracle.guarded_sources(root)

    def test_hash_manifests_fail_closed(self):
        for filename, required in ((oracle.MANIFEST, oracle.SOURCE_FILES),
                                   (oracle.HERE / 'straight_path_input_hashes.sha256', oracle.INPUT_FILES)):
            valid = filename.read_text()
            first = next(line for line in valid.splitlines() if not line.startswith('#'))
            variants = (valid + first + '\n', valid.replace(first, ''), 'bad row surplus\n',
                        valid.replace(first.split()[1], '../escape'), valid.replace(first[:64], 'z' * 64))
            for bad in variants:
                with tempfile.TemporaryDirectory() as temporary:
                    manifest = Path(temporary) / 'manifest'
                    manifest.write_text(bad)
                    with self.assertRaisesRegex(SystemExit, 'hash manifest'):
                        oracle.manifest_rows(manifest, required)

    def test_exact_source_methods_actual_g3d_and_licenses(self):
        harness, package = oracle.build_harness(self.sources)
        self.assertEqual(package, oracle.PACKAGE.read_text(encoding='latin1'))
        for signature, _ in oracle.METHODS:
            method = oracle.exact_block(self.sources[oracle.CPP], signature)
            self.assertIn(method, harness)
            self.assertIn(method, package)
        self.assertIn(oracle.exact_block(self.sources[oracle.HEADER], 'void clear()'), harness)
        self.assertIn('#include "G3D/Vector3.h"', harness)
        self.assertIn('inline Vector3::Vector3() : x(0.0f), y(0.0f), z(0.0f)', package)
        self.assertNotIn('void PathFinder::BuildPolyPath(', harness)
        self.assertIn('void PathFinder::BuildPolyPath(', package)
        self.assertIn('bool ChaseMovementGenerator::DispatchSplineToPosition(', package)
        self.assertIn('query->init(mmap->navMesh, 1024)', package)
        self.assertNotIn('benilla_nav_', harness)
        self.assertNotIn('benilla_source_', harness)
        for name in ('LICENSE', 'dep/recastnavigation/License.txt', 'dep/g3dlite/license.cpp'):
            self.assertIn(self.sources[name], package)
        for name, source in self.sources.items():
            if name.startswith(('dep/recastnavigation/', 'dep/g3dlite/')):
                self.assertEqual((self.work / name).read_bytes(), source.encode('latin1'))
        for statement in ('hit = hit - m_sourceUnit->GetCollisionWidth();', 'if (hit < 0.1f)',
                          '(startPos - hitPosVec).squaredMagnitude() > 0.01)',
                          'if (pathLength > 0.1f)', 'prevVec += diffVec;',
                          'if (pointCount == m_pointPathLimit)'):
            self.assertIn(statement, harness)

    def test_source_locations_bounds_config_and_harness_markers(self):
        for bad in ('void f() {} void f() {}', 'void other() {}'):
            with self.assertRaisesRegex(SystemExit, 'Expected one source block'):
                oracle.exact_block(bad, 'void f()')
        for bad in ('void f() {', 'void f()'):
            with self.assertRaisesRegex(SystemExit, 'Unclosed source block'):
                oracle.exact_block(bad, 'void f()')
        for source, start, end in (('a b a', 'a', 'b'), ('b a', 'a', 'b'), ('abc', 'missing', 'b')):
            with self.assertRaises(SystemExit):
                oracle.source_span(source, start, end)
        changes = [(oracle.CPP, lambda s: '\n' + s, 'location drift'),
                   (oracle.HEADER, lambda s: s.replace('#define LINE_FAULT              0.5f', 'wrong'), 'bounds differ'),
                   (oracle.MOVEMAP, lambda s: s.replace('query->init(mmap->navMesh, 1024)', 'query->init(mmap->navMesh, 512)'), 'node pool differs'),
                   ('src/game/Movement/MoveSplineInitArgs.h', lambda s: s.replace('typedef std::vector<Vector3> PointsArray;', 'wrong'), 'storage alias differs'),
                   ('src/game/World/World.h', lambda s: s.replace('CONFIG_BOOL_PATH_FIND_NORMALIZE_Z,', 'wrong'), 'config key differs')]
        for name, change, error in changes:
            with self.assertRaisesRegex(SystemExit, error):
                oracle.build_harness(self.sources | {name: change(self.sources[name])})
        real = Path.read_text
        for change in (lambda s: s.replace('// ORIGINAL_METHODS', ''), lambda s: s + '\n// ORIGINAL_METHODS\n'):
            def read(path, *args, **kwargs):
                value = real(path, *args, **kwargs)
                return change(value) if path.name == 'straight_path_oracle.cpp' else value
            with mock.patch.object(Path, 'read_text', read):
                with self.assertRaisesRegex(SystemExit, 'Expected one harness marker'):
                    oracle.build_harness(self.sources)

    def test_compiler_symbols_and_exact_warning_profile(self):
        with mock.patch.object(oracle.shutil, 'which', return_value=None):
            with self.assertRaisesRegex(SystemExit, 'Missing required g\\+\\+'):
                oracle.compile_oracle(self.sources, self.work)
        with mock.patch.object(oracle, 'run', return_value=subprocess.CompletedProcess([], 0, b'wrong')):
            with self.assertRaisesRegex(SystemExit, 'Unsupported audited compiler profile'):
                oracle.compile_oracle(self.sources, self.work)
        with mock.patch.object(oracle, 'run', side_effect=subprocess.CalledProcessError(1, 'g++')):
            with self.assertRaises(subprocess.CalledProcessError):
                oracle.compile_oracle(self.sources, self.work)
        with self.assertRaisesRegex(SystemExit, 'Missing unchanged source symbol'):
            oracle.verify_symbols('')
        symbols = oracle.run(['nm', '-C', str(self.binary)], stdout=subprocess.PIPE).stdout.decode()
        for name in ('dtNavMeshQuery::raycast(', 'dtNavMeshQuery::getPolyHeight(',
                     'G3D::Vector3::magnitude() const', 'G3D::Vector3::operator+=(G3D::Vector3 const&)'):
            with self.assertRaisesRegex(SystemExit, 'Missing actual Detour symbol'):
                oracle.verify_symbols(symbols.replace(name, 'removed('))
        for name in ('benilla_nav_create(', 'benilla_source_straight_path('):
            with self.assertRaisesRegex(SystemExit, 'Production bridge linked'):
                oracle.verify_symbols(symbols + name)
        harness, _ = oracle.build_harness(self.sources)
        statement = '                    dtStatus dtResult = m_navMeshQuery->getPolyHeight(poly, hitPos, &hitPos[1]);'
        line = harness[:harness.index(statement)].count('\n') + 1
        good = f"oracle.cpp:{line}:30: warning: unused variable 'dtResult' [-Wunused-variable]\n"
        oracle.verify_warnings(good, harness, 'oracle.cpp')
        for bad in ('', good + good, good.replace('oracle.cpp:', 'other.cpp:'), good.replace(':30:', ':31:'),
                    good.replace(f':{line}:', f':{line + 1}:'), good.replace('dtResult', 'another'),
                    good + 'unexpected: error: bad\n'):
            with self.assertRaisesRegex(SystemExit, 'Unexpected source warning profile'):
                oracle.verify_warnings(bad, harness, 'oracle.cpp')
        with self.assertRaisesRegex(SystemExit, 'ignored-height statement drift'):
            oracle.verify_warnings(good, harness.replace(statement, ''), 'oracle.cpp')

    def test_only_default_constructor_repair_identical_outputs(self):
        original = self.sources[oracle.TILE_HEADER]
        repaired = oracle.repair_tile_header(original)
        self.assertEqual(repaired.replace('\tdtMeshTile() = default;\n', '', 1), original)
        for bad in (original.replace('struct dtMeshTile\n{', 'struct dtMeshTile {'), repaired,
                    original + '\nstruct dtMeshTile\n{\n'):
            with self.assertRaisesRegex(SystemExit, 'repair insertion drift'):
                oracle.repair_tile_header(bad)
        work = self.work / 'repaired'
        work.mkdir()
        binary = oracle.compile_oracle(self.sources, work, repair_lifetime=True)
        self.assertEqual(oracle.observe(binary, work), self.observed)
        for name, source in self.sources.items():
            if name.startswith(('dep/recastnavigation/', 'dep/g3dlite/')):
                expected = repaired if name == oracle.TILE_HEADER else source
                self.assertEqual((work / name).read_bytes(), expected.encode('latin1'))

    def test_inputs_safe_outputs_and_all_observed_bytes(self):
        oracle.guard_inputs()
        real = Path.read_bytes
        def read(path):
            data = real(path)
            return data + b'drift' if path.name == 'precision.test-fixture' else data
        with mock.patch.object(Path, 'read_bytes', read):
            with self.assertRaisesRegex(SystemExit, 'Authored input fixture drift'):
                oracle.guard_inputs()
        self.assertEqual(self.observed, self.products)
        for names in (('bad.mmtile',), ('../escape.test-fixture',), ('x.test-fixture', 'x.test-fixture')):
            with mock.patch.object(oracle, 'FILES', names), mock.patch.object(oracle, 'run') as run:
                with self.assertRaisesRegex(SystemExit, 'Unsafe fixture output filename'):
                    oracle.observe(self.binary, self.work)
                run.assert_not_called()
        extra = self.work / 'output/extra.mmtile'
        extra.write_bytes(b'bad')
        try:
            with self.assertRaisesRegex(SystemExit, 'Unexpected oracle output files'):
                oracle.observe(self.binary, self.work)
        finally:
            extra.unlink()

    def test_check_never_rewrites_drift(self):
        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary) / 'source.test-fixture'
            package.write_text('drift')
            with mock.patch.object(oracle, 'PACKAGE', package), mock.patch.object(oracle, 'compile_oracle') as compile, mock.patch.object(sys, 'argv', ['oracle', str(ROOT), '--check']):
                with self.assertRaisesRegex(SystemExit, 'source package differs'):
                    oracle.main()
            compile.assert_not_called()
            self.assertEqual(package.read_text(), 'drift')
            fixtures = Path(temporary) / 'fixtures'
            fixtures.mkdir()
            for name in oracle.FILES:
                (fixtures / name).write_bytes(b'drift')
            with mock.patch.object(oracle, 'FIXTURES', fixtures), mock.patch.object(oracle, 'compile_oracle', return_value=self.binary), mock.patch.object(oracle, 'observe', return_value=self.products), mock.patch.object(sys, 'argv', ['oracle', str(ROOT), '--check']):
                with self.assertRaisesRegex(SystemExit, 'fixture differs'):
                    oracle.main()
            self.assertTrue(all((fixtures / name).read_bytes() == b'drift' for name in oracle.FILES))

    def test_all_table_schemas_and_values_fail_closed(self):
        contracts = [('cases.test-fixture', oracle.FIELDS, lambda b: oracle.parse_cases(b)),
                     ('sequences.test-fixture', oracle.SEQUENCE_FIELDS, lambda b: oracle.parse_cases(b, True)),
                     ('traces.test-fixture', oracle.TRACE_FIELDS, lambda b: oracle.parse_auxiliary(b, True)),
                     ('domains.test-fixture', oracle.DOMAIN_FIELDS, lambda b: oracle.parse_auxiliary(b, False))]
        for filename, fields, parse in contracts:
            lines = self.products[filename].decode().splitlines()
            header, row = lines[:2]
            for bad in (row, header + '\n' + header + '\n' + row, header + '\n' + row + '\n' + row,
                        header + '\n', header + '\n' + row + '\textra', '\xff'):
                with self.subTest(filename=filename, bad=bad[:40]):
                    with self.assertRaises(SystemExit):
                        parse(bad.encode('latin1'))
        changes = {'optimize': '2', 'normalize': '1', 'ignore': '2', 'force': 'yes', 'previous_type': '64',
                   'path_type': '2', 'from': '7fc00000,00000000,00000000', 'to': '-', 'actual_end': '0',
                   'before_active': '75', 'before_storage': '0', 'after_active': '-1', 'after_storage': str(1 << 64),
                   'lookup_storage': '0', 'start_ref': '0', 'start_distance': 'bf800000', 'geometry': 'world',
                   'scope': 'cold', 'phase': 'sample', 'collision_width': '7f800000', 'width_calls': '2',
                   'ray_status': 'success', 'ray_hit': '7f800000', 'ray_normal': '-', 'ray_active': '75',
                   'ray_storage': '0', 'adjusted_hit': 'nan', 'height_failures': '99999',
                   'points': ';'.join(['00000000,00000000,00000000'] * 1027)}
        for field, value in changes.items():
            with self.subTest(field=field):
                row = self.rows[0] | {field: value}
                with self.assertRaisesRegex(SystemExit, 'Invalid oracle row'):
                    oracle.parse_cases(self.table(oracle.FIELDS, row))
        for field, value in {'first_case': '../bad', 'direct_status': 'ok', 'smooth_status': 'ok',
                             'smooth_points': ';'.join(['00000000,00000000,00000000'] * 75)}.items():
            with self.assertRaisesRegex(SystemExit, 'Invalid oracle row'):
                oracle.parse_cases(self.table(oracle.SEQUENCE_FIELDS, self.sequences[0] | {field: value}), True)
        for field, value in {'ordinal': '-1', 'primitive': 'fake', 'ref': str(1 << 64), 'status': 'ok',
                             'count': '1', 'input': 'bad', 'scalar': '7f800000', 'refs': '-1'}.items():
            with self.assertRaisesRegex(SystemExit, 'Invalid auxiliary row'):
                oracle.parse_auxiliary(self.table(oracle.TRACE_FIELDS, self.traces[0] | {field: value}), True)
        for field, value in (('classification', 'unknown'), ('invoked', '1')):
            with self.assertRaisesRegex(SystemExit, 'Invalid auxiliary row'):
                oracle.parse_auxiliary(self.table(oracle.DOMAIN_FIELDS, self.domains[0] | {field: value}), False)

    def test_cross_product_origin_order_counts_and_width_guards(self):
        mutations = [('sequences.test-fixture', oracle.SEQUENCE_FIELDS, 0, 'first_case', 'missing'),
                     ('sequences.test-fixture', oracle.SEQUENCE_FIELDS, 0, 'previous_type', '8'),
                     ('traces.test-fixture', oracle.TRACE_FIELDS, 0, 'label', 'missing_0000'),
                     ('traces.test-fixture', oracle.TRACE_FIELDS, 0, 'ordinal', '1'),
                     ('cases.test-fixture', oracle.FIELDS, 0, 'height_calls', '3'),
                     ('cases.test-fixture', oracle.FIELDS, 0, 'ray_hit', '3f000000'),
                     ('cases.test-fixture', oracle.FIELDS, 0, 'width_calls', '1'),
                     ('cases.test-fixture', oracle.FIELDS, 6, 'adjusted_hit', '3e800000')]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                with self.assertRaises(SystemExit):
                    oracle.verify_products(self.changed_product(*mutation))

    def test_output_coverage_same_cut_replacement_and_ignored_optimization(self):
        self.assertEqual([len(self.rows), len(self.sequences), len(self.traces), len(self.domains)], [35, 5, 1568, 10])
        for name, phase in (('replacement_no_hit', 'replacement'), ('cut_no_raycast', 'cut'), ('same_no_sampling', 'same')):
            row = self.named(name)
            self.assertEqual(row['phase'], phase)
            self.assertEqual(row['path_type'], '1')
        same = self.named('same_no_sampling')
        self.assertEqual((same['after_active'], same['height_calls'], same['ray_status']), ('1', '0', '-'))
        self.assertEqual(same['points'], same['from'] + ';' + same['to'])
        self.assertEqual(self.named('same_zero_length')['points'].split(';')[0], self.named('same_zero_length')['points'].split(';')[1])
        base = self.named('snake_hit_then_smooth_incomplete')
        optimized = self.named('snake_hit_then_smooth_optimized')
        for key in oracle.FIELDS:
            if key not in {'label', 'optimize'}:
                self.assertEqual(base[key], optimized[key], key)
        ignored = self.named('ignored_normalization')
        self.assertEqual((ignored['optimize'], ignored['normalize'], ignored['ignore']), ('1', '1', '1'))
        self.assertEqual(ignored['points'], self.named('cut_no_raycast')['points'])

    def test_full_width_hit_boundary_and_final_incomplete_override(self):
        row = self.named('hit_full_width')
        self.assertEqual((row['ray_hit'], row['collision_width'], row['adjusted_hit']), ('3e800000', '3e000000', '3e000000'))
        self.assertEqual(oracle.points(row['end']), [(7.5, 5.0, 0.0)])
        self.assertEqual([self.named('hit_adjusted_' + suffix)['path_type'] for suffix in ('below', 'equal', 'above')], ['8', '4', '4'])
        self.assertEqual([self.named('hit_adjusted_' + suffix)['adjusted_hit'] for suffix in ('below', 'equal', 'above')], ['3dcccccc', '3dcccccd', '3dccccce'])
        forced = self.named('hit_full_width_force_clipped_end')
        self.assertEqual((forced['force'], forced['previous_type'], forced['path_type']), ('1', '4', '4'))
        self.assertEqual(forced['points'], row['points'])
        self.assertEqual(forced['actual_end'], forced['end'])
        self.assertNotEqual(forced['end'], forced['to'])

    def test_strict_double_distance_threshold_and_initialized_ignored_height(self):
        def f32(value):
            return struct.unpack('!f', struct.pack('!f', value))[0]
        for suffix, comparison, kind in (('below', self.assertLess, '8'), ('above', self.assertGreater, '4')):
            row = self.named('hit_distance_' + suffix)
            height = self.events(row['label'], 'height')[0]
            start = oracle.points(row['from'])[0]
            hit = oracle.points(height['output'])[0]
            differences = [f32(a - b) for a, b in zip(start, hit)]
            square = f32(f32(f32(differences[0] ** 2) + f32(differences[1] ** 2)) + f32(differences[2] ** 2))
            comparison(square, 0.01)
            self.assertEqual(row['path_type'], kind)
        height = next(e for e in self.events('hit_boundary_ignored_height', 'height') if e['status'] == '80000008')
        self.assertEqual(height['input'], height['output'])
        self.assertEqual(height['scalar'], '3f000000')
        self.assertEqual(self.named('hit_boundary_ignored_height')['path_type'], '4')
        for height in self.events('ray_buffer_no_hit_sampling_appends', 'height'):
            self.assertEqual(height['input'], height['output'])
            self.assertEqual(height['status'], '80000008')

    def test_strict_sampling_threshold_merge_and_mutable_height(self):
        self.assertEqual([self.named('sample_length_' + suffix)['path_type'] for suffix in ('below', 'equal', 'above')], ['8', '8', '1'])
        duplicated = self.named('sample_four_duplicate_endpoint')['points'].split(';')
        self.assertEqual(len(duplicated), 3)
        self.assertEqual(duplicated[-1], duplicated[-2])
        self.assertEqual(len(oracle.points(self.named('sample_eight_merge')['points'])), 3)
        self.assertEqual([len(oracle.points(self.named('sample_remainder_' + suffix)['points'])) for suffix in ('below', 'above')], [3, 4])
        heights = self.events('slope_sample_mutable_height', 'height')
        self.assertEqual(len(heights), 15)
        # The next original sample starts from the prior height-mutated prevVec.
        first_input = oracle.points(heights[0]['input'])[0][2]
        first_output = oracle.points(heights[0]['output'])[0][2]
        second_input = oracle.points(heights[1]['input'])[0][2]
        self.assertGreater(first_input, 2)
        self.assertLess(first_output, 0.5)
        self.assertLess(second_input, 0.5)
        self.assertGreater(second_input, first_output)

    def test_source_count_equality_not_capacity_truncation(self):
        for count, kind, actual, active in ((73, '1', 73, '1'), (74, '32', 2, '0'), (75, '1', 75, '1')):
            row = self.named(f'sample_count_{count}')
            self.assertEqual((row['path_type'], len(oracle.points(row['points'], 1026)), row['after_active']), (kind, actual, active))
            self.assertEqual(row['before_storage'], row['after_storage'])
            self.assertEqual(int(row['height_calls']), count - 2)

    def test_retained_tail_ray_buffer_and_same_object_fallback(self):
        row = self.named('ray_buffer_no_hit_sampling_appends')
        self.assertEqual((row['ray_status'], row['ray_hit'], row['ray_active'], row['after_active']), ('40000010', '7f7fffff', '74', '74'))
        self.assertEqual(len(oracle.ref_list(row['before_storage'])), 74)
        self.assertEqual(len(oracle.ref_list(row['after_storage'])), 76)
        self.assertEqual(row['width_calls'], '0')
        self.assertEqual(self.named('replacement_no_hit_prior_incomplete')['path_type'], '4')
        base = self.named('snake_hit_then_smooth_incomplete_smooth', True)
        rejected = self.named('snake_reject_then_smooth_normal_smooth', True)
        optimized = self.named('snake_hit_then_smooth_optimized_smooth', True)
        self.assertEqual((base['path_type'], rejected['path_type'], optimized['path_type']), ('4', '1', '4'))
        self.assertEqual(base['smooth_points'], optimized['smooth_points'])
        self.assertEqual((len(oracle.points(base['points'])), len(oracle.points(optimized['points']))), (9, 5))
        self.assertEqual(self.named('ray_no_hit_other_layer_smooth', True)['phase'], 'same')
        self.assertTrue(all(row['invoked'] == '0' for row in self.domains))

    def test_actual_crlf_git_checkout_preserves_bytes_without_commit(self):
        with tempfile.TemporaryDirectory(prefix='straight-path-crlf-') as temporary:
            checkout = Path(temporary)
            expected = {}
            files = list(oracle.HERE.glob('straight_path_*')) + [oracle.HERE / '.gitattributes'] + list(oracle.FIXTURES.iterdir())
            files += [oracle.HERE / 'fixtures' / name for name in oracle.INPUT_FILES]
            for source in files:
                if not source.is_file():
                    continue
                relative = source.relative_to(oracle.HERE)
                dest = checkout / relative
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, dest)
                expected[relative] = hashlib.sha256(source.read_bytes()).hexdigest()
            oracle.run(['git', 'init', '-q', str(checkout)])
            oracle.run(['git', '-C', str(checkout), '-c', 'core.autocrlf=true', 'add', '.'])
            for relative in expected:
                (checkout / relative).unlink()
            oracle.run(['git', '-C', str(checkout), '-c', 'core.autocrlf=true', 'checkout-index', '--all', '--force'])
            for relative, digest in expected.items():
                if relative.name != '.gitattributes':
                    self.assertEqual(hashlib.sha256((checkout / relative).read_bytes()).hexdigest(), digest, str(relative))
            with mock.patch.object(oracle, 'HERE', checkout):
                oracle.guard_inputs()


if __name__ == '__main__':
    unittest.main()
