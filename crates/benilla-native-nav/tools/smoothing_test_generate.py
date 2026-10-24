#!/usr/bin/env python3
"""Fail-closed provenance and behavior guards for original loaded smoothing observations."""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
sys.dont_write_bytecode = True
import smoothing_generate as oracle
ROOT = Path(sys.argv.pop(1)).resolve()


def rows():
    return [dict(zip(oracle.FIELDS, line.split('\t'), strict=True))
            for line in (oracle.FIXTURES / 'cases.test-fixture').read_text().splitlines()
            if not line.startswith('#')]


def points(row):
    return [[int(word,16) for word in point.split(',')] for point in row['points'].split(';')]


class SmoothingOracleGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = oracle.guarded_sources(ROOT)
        cls.rows = rows()
        cls.temporary = tempfile.TemporaryDirectory(prefix='test-smoothing-source-')
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.work = Path(cls.temporary.name)
        cls.binary = oracle.compile_oracle(cls.sources, cls.work)

    def named(self, name): return next(row for row in self.rows if row['label'] == name)

    def test_full_hashes_cover_every_compiled_source_header_and_license(self):
        self.assertEqual(len(self.sources), 20)
        for name in oracle.DETOUR_FILES:
            self.assertIn(f'dep/recastnavigation/Detour/Source/{name}.cpp', self.sources)
        self.assertIn('LICENSE', self.sources)
        self.assertIn('dep/recastnavigation/License.txt', self.sources)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, source in self.sources.items():
                p = root / name; p.parent.mkdir(parents=True, exist_ok=True)
                p.write_bytes(source.encode('latin1').replace(b'\n',b'\r\n'))
            self.assertEqual(oracle.guarded_sources(root), self.sources)
            for name, source in self.sources.items():
                with self.subTest(name=name):
                    (root / name).write_text(source + '\n// drift\n', encoding='latin1')
                    with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                        oracle.guarded_sources(root)
                    (root / name).write_text(source, encoding='latin1')
            (root / oracle.CPP).unlink()
            with self.assertRaises(FileNotFoundError): oracle.guarded_sources(root)

    def test_complete_exact_methods_macros_notices_and_no_cold_lookup(self):
        harness, package = oracle.build_harness(self.sources)
        self.assertEqual(package, oracle.PACKAGE.read_text(encoding='latin1'))
        for signature, _ in oracle.METHODS:
            method = oracle.exact_block(self.sources[oracle.CPP], signature)
            self.assertIn(method, harness); self.assertIn(method, package)
        self.assertNotIn('getPolyByLocation(', harness)
        self.assertNotIn('getPathPolyByPosition(', harness)
        self.assertNotIn('benilla_nav_', harness)
        self.assertIn('return nsmoothPath < m_pointPathLimit ? DT_SUCCESS : DT_FAILURE;', harness)
        self.assertIn('result[1] += 0.5f;', harness)
        self.assertIn('#define MAX_POINT_PATH_LENGTH   74', package)
        for name in (oracle.CPP, oracle.HEADER):
            source = self.sources[name]
            self.assertIn(source[:source.index('*/') + 2], package)
        self.assertIn(self.sources['LICENSE'], package)
        self.assertIn(self.sources['dep/recastnavigation/License.txt'], package)

    def test_duplicate_unclosed_block_and_shifted_method_fail_closed(self):
        with self.assertRaisesRegex(SystemExit, 'Expected one source block'):
            oracle.exact_block('void f() {} void f() {}','void f()')
        with self.assertRaisesRegex(SystemExit, 'Unclosed source block'):
            oracle.exact_block('void f() {','void f()')
        sources = dict(self.sources); sources[oracle.CPP] = '\n' + sources[oracle.CPP]
        with self.assertRaisesRegex(SystemExit, 'Pinned source location drift'): oracle.build_harness(sources)

    def test_missing_or_duplicate_harness_markers_fail_closed(self):
        real = Path.read_text
        for mutate in (lambda s:s.replace('// PINNED_METHODS',''),lambda s:s+'\n// PINNED_CONSTANTS\n'):
            def read(path,*args,**kwargs):
                text=real(path,*args,**kwargs)
                return mutate(text) if path.name=='smoothing_oracle.cpp' else text
            with mock.patch.object(Path,'read_text',read):
                with self.assertRaisesRegex(SystemExit,'Expected one harness marker'):
                    oracle.build_harness(self.sources)

    def test_source_package_drift_fails_before_compilation_without_writing(self):
        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary)/'package.test-fixture';package.write_text('drift')
            with mock.patch.object(oracle,'PACKAGE',package),mock.patch.object(oracle,'compile_oracle') as compiler, \
                 mock.patch.object(sys,'argv',['oracle',str(ROOT),'--check']):
                with self.assertRaisesRegex(SystemExit,'source package differs'): oracle.main()
            compiler.assert_not_called();self.assertEqual(package.read_text(),'drift')

    def test_fixture_drift_fails_without_writing(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixtures=Path(temporary)
            for name in oracle.FILES:(fixtures/name).write_bytes(b'drift')
            with mock.patch.object(oracle,'FIXTURES',fixtures), \
                 mock.patch.object(oracle,'compile_oracle',return_value=self.binary), \
                 mock.patch.object(sys,'argv',['oracle',str(ROOT),'--check']):
                with self.assertRaisesRegex(SystemExit,'Smoothing fixture differs'):oracle.main()
            self.assertTrue(all((fixtures/name).read_bytes()==b'drift' for name in oracle.FILES))

    def test_missing_wrong_or_failed_compiler_and_symbols_fail_closed(self):
        with mock.patch.object(oracle.shutil,'which',return_value=None):
            with self.assertRaisesRegex(SystemExit,'Missing required g\\+\\+'):
                oracle.compile_oracle(self.sources,self.work)
        with mock.patch.object(oracle,'run',return_value=subprocess.CompletedProcess([],0,b'wrong')):
            with self.assertRaisesRegex(SystemExit,'Unsupported audited compiler profile'):
                oracle.compile_oracle(self.sources,self.work)
        with mock.patch.object(oracle,'run',side_effect=subprocess.CalledProcessError(1,'g++')):
            with self.assertRaises(subprocess.CalledProcessError):oracle.compile_oracle(self.sources,self.work)
        with self.assertRaisesRegex(SystemExit,'Missing unchanged source symbol'):oracle.verify_symbols('')
        with self.assertRaisesRegex(SystemExit,'Missing actual Detour symbol'):
            oracle.verify_symbols(' '.join(signature.split(' ',1)[1] for signature,_ in oracle.METHODS))
        symbols = oracle.run(['nm','-C',str(self.binary)],stdout=subprocess.PIPE).stdout.decode()
        with self.assertRaisesRegex(SystemExit,'Production bridge linked'):oracle.verify_symbols(symbols+' benilla_nav_create(')

    def test_tile_lifetime_repair_is_one_exact_declaration_only(self):
        original=self.sources[oracle.TILE_HEADER]
        repaired=oracle.repair_tile_header(original)
        self.assertEqual(repaired.replace('\tdtMeshTile() = default;\n','',1),original)
        self.assertEqual(repaired.count('dtMeshTile() = default;'),1)
        for bad in (original.replace('struct dtMeshTile\n{','struct dtMeshTile {'),repaired,original+'\nstruct dtMeshTile\n{\n'):
            with self.assertRaisesRegex(SystemExit,'Tile-lifetime repair insertion drift'):
                oracle.repair_tile_header(bad)

    def test_lifetime_only_companion_reproduces_original_rows_and_geometry(self):
        work=self.work/'lifetime';work.mkdir()
        binary=oracle.compile_oracle(self.sources,work,repair_lifetime=True)
        observed=oracle.observe(binary,work)
        for name,data in observed.items():self.assertEqual(data,(oracle.FIXTURES/name).read_bytes())
        self.assertEqual((work/oracle.TILE_HEADER).read_text(encoding='latin1'),oracle.repair_tile_header(self.sources[oracle.TILE_HEADER]))

    def test_reused_authored_input_hash_guard(self):
        oracle.guard_inputs()
        real = Path.read_bytes
        def read(path):
            data=real(path)
            return data+b'drift' if path.name=='0003232.mmtile' else data
        with mock.patch.object(Path,'read_bytes',read):
            with self.assertRaisesRegex(SystemExit,'Authored input fixture drift'):oracle.guard_inputs()

    def test_exact_reproduction_and_authored_native_bytes(self):
        observed=oracle.observe(self.binary,self.work)
        for name,data in observed.items():self.assertEqual(data,(oracle.FIXTURES/name).read_bytes())
        self.assertEqual(len(self.rows),27)
        params=observed['params.test-fixture'];tile=observed['ground.test-fixture']
        self.assertEqual(len(params),28)
        self.assertEqual(struct.unpack_from('<III',tile),(0x4d4d4150,7,8))
        self.assertEqual(struct.unpack_from('<I',tile,12)[0]+20,len(tile))
        self.assertTrue(all(name.endswith('.test-fixture') for name in oracle.FILES))

    def test_original_vertical_endpoints_are_not_nearest_snaps(self):
        row=self.named('flat_vertical_offsets'); pts=points(row)
        self.assertEqual(row['bounded_from'],row['from'])
        self.assertEqual(row['bounded_to'],row['to'])
        self.assertNotEqual(row['from'],row['from_nearest'])
        self.assertNotEqual(row['to'],row['to_nearest'])
        self.assertEqual(pts[0],[int(x,16) for x in row['from'].split(',')])
        self.assertEqual(pts[-1],[int(x,16) for x in row['to'].split(',')])
        self.assertEqual(pts[1][2],0x3f000000) # original +0.5 surface adjustment
        self.assertEqual(self.named('far_lookup')['from_attempts'],'2')

    def test_target_clamp_and_source_defined_ignored_height_failure(self):
        lower=self.named('flat_target_lower_boundary_clamp')
        upper=self.named('flat_target_boundary_clamp')
        for row in (lower,upper):
            self.assertNotEqual(row['to'],row['bounded_to'])
            self.assertEqual(row['stop'],'target_reached')
            self.assertEqual(row['smooth_status'],'40000000')
        self.assertEqual(lower['height_failures'],'0')
        self.assertEqual(upper['height_failures'],'1')
        self.assertEqual(upper['height_status'],'80000008')
        self.assertEqual(sum(int(row['height_failures']) for row in self.rows if row['geometry']!='thin'),1)

    def test_steering_success_details_and_multi_portal_slopes(self):
        row=self.named('bend_many_corners')
        self.assertEqual(row['probe_steer_status'],'40000010')
        self.assertEqual(row['probe_steer_count'],'3')
        self.assertEqual(row['full_steer_count'],'8')
        self.assertEqual(row['full_steer_status'],'40000000')
        self.assertEqual(row['steer_buffer_calls'],'18')
        self.assertEqual(len(row['corridor'].split(',')),15)
        self.assertEqual(row['stop'],'target_reached')
        self.assertEqual(len(self.named('slope_multi_portal')['corridor'].split(',')),8)
        self.assertEqual(len(self.named('cross_tile_slope')['corridor'].split(',')),2)

    def test_early_stop_and_point_limit_are_not_conflated_with_success(self):
        for label in ('coincident_early_stop','vertical_only_early_stop','slop_early_stop'):
            row=self.named(label)
            self.assertEqual(row['smooth_status'],'40000000');self.assertEqual(row['stop'],'no_steer_target')
            self.assertEqual(len(points(row)),1);self.assertEqual(row['move_calls'],'0')
        for label,limit in (('flat_limit2',2),('long_limit10',10),('long_limit74',74),('bend_limit16',16)):
            row=self.named(label)
            self.assertEqual(row['smooth_status'],'80000000');self.assertEqual(row['stop'],'point_limit')
            self.assertEqual(len(points(row)),limit)
        row=self.named('short_reaches_at_limit2')
        self.assertEqual(row['smooth_status'],'80000000');self.assertEqual(row['stop'],'target_reached')

    def test_corridor_rejections_and_final_storage(self):
        for label in ('bend_corridor_limit2','unreachable_partial_corridor'):
            row=self.named(label)
            self.assertNotEqual(row['corridor_status'],'40000000')
            self.assertEqual(row['stop'],'corridor_rejected');self.assertEqual(row['points'],'-')
        for row in self.rows:
            self.assertEqual(len(row['final_storage'].split(',')),74)
            if row['smooth_status']!='00000000':
                final=row['final_corridor'].split(',')
                self.assertEqual(row['final_storage'].split(',')[:len(final)],final)

    def test_actual_visited_truncation_and_exact_corridor_capacity(self):
        for label in ('thin_visited_buffer','thin_visited_buffer_reverse'):
            row=self.named(label)
            self.assertEqual(len(row['corridor'].split(',')),40)
            self.assertEqual(row['move_buffer_calls'],'73')
            self.assertEqual(row['move_status'],'40000010')
            self.assertEqual(row['height_failures'],'37')
            self.assertEqual(len(points(row)),74)
            self.assertEqual(row['smooth_status'],'80000000')
            self.assertEqual(row['stop'],'point_limit')
        row=self.named('exact_fit_two_polygons_limit2')
        self.assertEqual(len(row['corridor'].split(',')),2)
        self.assertEqual(row['corridor_status'],'40000000')
        self.assertEqual(row['smooth_status'],'80000000')

    def test_fixup_duplicate_order_and_truncation_are_source_observed(self):
        rs={row[0]:row for row in (line.split('\t') for line in (oracle.FIXTURES/'fixup.test-fixture').read_text().splitlines() if not line.startswith('#'))}
        self.assertEqual(len(rs),9)
        self.assertEqual(rs['earliest_duplicate_visited'][-1],'8,2,7,2,3,4')
        self.assertEqual(rs['furthest_duplicate_path'][-1],'8,2,4')
        self.assertEqual(rs['truncate_tail'][-1],'10,9,2,3')
        for name in ('req_equals_limit','req_exceeds_limit'):self.assertEqual(rs[name][-1],'1,2,3,4')

    def test_inrange_strict_float_boundaries(self):
        rs={row[0]:row[-1] for row in (line.split('\t') for line in (oracle.FIXTURES/'range.test-fixture').read_text().splitlines() if not line.startswith('#'))}
        self.assertEqual(len(rs),12)
        for label in ('horizontal_below_slop','vertical_below_height','steer_height_below1000','both_horizontal_axes','signed_zero'):self.assertEqual(rs[label],'1')
        for label in ('horizontal_equal_slop','horizontal_above_slop','vertical_equal_height','vertical_negative_equal','steer_height_equal1000','diagonal_outside','zero_radius'):self.assertEqual(rs[label],'0')


if __name__ == '__main__': unittest.main()
