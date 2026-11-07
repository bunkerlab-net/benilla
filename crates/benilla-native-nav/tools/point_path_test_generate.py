#!/usr/bin/env python3
"""Fail-closed original loaded point-path and raw postprocessing oracle guards."""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path
import hashlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
sys.dont_write_bytecode=True
import point_path_generate as oracle
ROOT=Path(sys.argv.pop(1)).resolve()


class PointPathOracleGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources=oracle.guarded_sources(ROOT)
        cls.rows=oracle.parse_cases((oracle.FIXTURES/'cases.test-fixture').read_bytes())
        cls.raw=oracle.parse_cases((oracle.FIXTURES/'postprocess.test-fixture').read_bytes(),True)
        cls.temp=tempfile.TemporaryDirectory(prefix='test-point-path-');cls.addClassCleanup(cls.temp.cleanup)
        cls.work=Path(cls.temp.name);cls.binary=oracle.compile_oracle(cls.sources,cls.work)
        cls.observed=oracle.observe(cls.binary,cls.work)

    def named(self,name,raw=False):return next(r for r in (self.raw if raw else self.rows) if r['label']==name)

    def test_complete_source_closure_licenses_and_lf_drift_guard(self):
        self.assertEqual(set(self.sources),oracle.SOURCE_FILES)
        self.assertEqual(len(self.sources),46)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for name,source in self.sources.items():
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(source.encode('latin1').replace(b'\n',b'\r\n'))
            self.assertEqual(oracle.guarded_sources(root),self.sources)
            for name,source in self.sources.items():
                with self.subTest(name=name):
                    (root/name).write_bytes((source+'\n// drift\n').encode('latin1'))
                    with self.assertRaisesRegex(SystemExit,'Wrong pinned source hash'):oracle.guarded_sources(root)
                    (root/name).write_bytes(source.encode('latin1'))
            (root/oracle.CPP).unlink()
            with self.assertRaises(FileNotFoundError):oracle.guarded_sources(root)

    def test_manifests_reject_duplicate_missing_unsafe_and_malformed_inputs(self):
        for filename,required in ((oracle.MANIFEST,oracle.SOURCE_FILES),(oracle.HERE/'point_path_input_hashes.sha256',oracle.INPUT_FILES)):
            valid=filename.read_text();first=next(x for x in valid.splitlines() if not x.startswith('#'))
            for bad in (valid+first+'\n',valid.replace(first,''),'bad row surplus\n',valid.replace(first.split()[1],'../escape'),valid.replace(first[:64],'z'*64)):
                with tempfile.TemporaryDirectory() as temporary:
                    manifest=Path(temporary)/'manifest';manifest.write_text(bad)
                    with self.assertRaisesRegex(SystemExit,'hash manifest'):oracle.manifest_rows(manifest,required)

    def test_full_original_methods_and_actual_g3d_are_unchanged(self):
        harness,package=oracle.build_harness(self.sources)
        self.assertEqual(package,oracle.PACKAGE.read_text(encoding='latin1'))
        for signature,_ in oracle.METHODS:
            method=oracle.exact_block(self.sources[oracle.CPP],signature)
            self.assertIn(method,harness);self.assertIn(method,package)
        self.assertIn(oracle.exact_block(self.sources[oracle.HEADER],'void clear()'),harness)
        self.assertIn('tempPathPoints[i] = emptyVec;',harness)
        self.assertIn('tempPathPoints[i - 1]',harness)
        self.assertIn('dist3DSqr(getActualEndPosition(), getEndPosition()) < 0.3f *',harness)
        self.assertIn('!(m_type & PATHFIND_INCOMPLETE)',harness)
        self.assertIn('#include "G3D/Vector3.h"',harness)
        self.assertIn('inline Vector3::Vector3() : x(0.0f), y(0.0f), z(0.0f)',package)
        self.assertNotIn('void PathFinder::BuildPolyPath(',harness)
        self.assertIn('void PathFinder::BuildPolyPath(',package)
        self.assertIn('query->init(mmap->navMesh, 1024)',package)
        self.assertNotIn('benilla_nav_',harness);self.assertNotIn('benilla_source_',harness)
        for name in ('LICENSE','dep/recastnavigation/License.txt','dep/g3dlite/license.cpp'):
            self.assertIn(self.sources[name],package)
        for name,source in self.sources.items():
            if name.startswith(('dep/recastnavigation/','dep/g3dlite/')):
                self.assertEqual((self.work/name).read_bytes(),source.encode('latin1'))

    def test_source_locations_fragments_bounds_and_config_names_fail_closed(self):
        for bad in ('void f() {} void f() {}','void other() {}'):
            with self.assertRaisesRegex(SystemExit,'Expected one source block'):oracle.exact_block(bad,'void f()')
        for bad in ('void f() {','void f()'):
            with self.assertRaisesRegex(SystemExit,'Unclosed source block'):oracle.exact_block(bad,'void f()')
        for source,start,end in (('a b a','a','b'),('b a','a','b'),('abc','missing','b')):
            with self.assertRaises(SystemExit):oracle.source_span(source,start,end)
        changes=[(oracle.CPP,lambda s:'\n'+s,'location drift'),
                 (oracle.HEADER,lambda s:s.replace('#define LINE_FAULT              0.5f','#define LINE_FAULT              0.6f'),'bounds differ'),
                 (oracle.MOVEMAP,lambda s:s.replace('query->init(mmap->navMesh, 1024)','query->init(mmap->navMesh, 512)'),'node pool differs'),
                 ('src/game/Movement/MoveSplineInitArgs.h',lambda s:s.replace('typedef std::vector<Vector3> PointsArray;','wrong'),'storage alias differs'),
                 ('src/game/World/World.h',lambda s:s.replace('CONFIG_BOOL_PATH_FIND_OPTIMIZE,','wrong'),'config key differs')]
        for name,change,error in changes:
            sources=dict(self.sources);sources[name]=change(sources[name])
            with self.assertRaisesRegex(SystemExit,error):oracle.build_harness(sources)
        real=Path.read_text
        for change in (lambda s:s.replace('// ORIGINAL_METHODS',''),lambda s:s+'\n// ORIGINAL_METHODS\n'):
            def read(path,*args,**kwargs):
                value=real(path,*args,**kwargs);return change(value) if path.name=='point_path_oracle.cpp' else value
            with mock.patch.object(Path,'read_text',read):
                with self.assertRaisesRegex(SystemExit,'Expected one harness marker'):oracle.build_harness(self.sources)

    def test_compiler_profile_and_symbols_exclude_production(self):
        with mock.patch.object(oracle.shutil,'which',return_value=None):
            with self.assertRaisesRegex(SystemExit,'Missing required g\\+\\+'):oracle.compile_oracle(self.sources,self.work)
        with mock.patch.object(oracle,'run',return_value=subprocess.CompletedProcess([],0,b'wrong')):
            with self.assertRaisesRegex(SystemExit,'Unsupported audited compiler profile'):oracle.compile_oracle(self.sources,self.work)
        with mock.patch.object(oracle,'run',side_effect=subprocess.CalledProcessError(1,'g++')):
            with self.assertRaises(subprocess.CalledProcessError):oracle.compile_oracle(self.sources,self.work)
        with self.assertRaisesRegex(SystemExit,'Missing unchanged source symbol'):oracle.verify_symbols('')
        symbols=oracle.run(['nm','-C',str(self.binary)],stdout=subprocess.PIPE).stdout.decode()
        for name in ('dtNavMeshQuery::findPath(', 'dtCreateNavMeshData(', 'G3D::Vector3::squaredLength() const'):
            with self.assertRaisesRegex(SystemExit,'Missing actual Detour symbol'):oracle.verify_symbols(symbols.replace(name,'removed('))
        for name in ('benilla_nav_create(', 'benilla_source_point_path('):
            with self.assertRaisesRegex(SystemExit,'Production bridge linked'):oracle.verify_symbols(symbols+name)

    def test_default_constructor_only_repair_has_identical_outputs(self):
        original=self.sources[oracle.TILE_HEADER];repaired=oracle.repair_tile_header(original)
        self.assertEqual(repaired.replace('\tdtMeshTile() = default;\n','',1),original)
        for bad in (original.replace('struct dtMeshTile\n{','struct dtMeshTile {'),repaired,original+'\nstruct dtMeshTile\n{\n'):
            with self.assertRaisesRegex(SystemExit,'repair insertion drift'):oracle.repair_tile_header(bad)
        work=self.work/'repaired';work.mkdir();binary=oracle.compile_oracle(self.sources,work,repair_lifetime=True)
        self.assertEqual(oracle.observe(binary,work),self.observed)
        for name,source in self.sources.items():
            if name.startswith(('dep/recastnavigation/','dep/g3dlite/')):
                expected=repaired if name==oracle.TILE_HEADER else source
                self.assertEqual((work/name).read_bytes(),expected.encode('latin1'))

    def test_verified_authored_inputs_safe_names_and_output_bytes(self):
        oracle.guard_inputs();real=Path.read_bytes
        def read(path):
            data=real(path);return data+b'drift' if path.name=='ground.test-fixture' else data
        with mock.patch.object(Path,'read_bytes',read):
            with self.assertRaisesRegex(SystemExit,'Authored input fixture drift'):oracle.guard_inputs()
        for name,data in self.observed.items():self.assertEqual(data,(oracle.FIXTURES/name).read_bytes())
        for names in (('bad.mmtile',),('../escape.test-fixture',),('x.test-fixture','x.test-fixture')):
            with mock.patch.object(oracle,'FILES',names),mock.patch.object(oracle,'run') as run:
                with self.assertRaisesRegex(SystemExit,'Unsafe fixture output filename'):oracle.observe(self.binary,self.work)
                run.assert_not_called()
        extra=self.work/'output/extra.mmtile';extra.write_bytes(b'bad')
        try:
            with self.assertRaisesRegex(SystemExit,'Unexpected oracle output files'):oracle.observe(self.binary,self.work)
        finally:extra.unlink()

    def test_package_and_fixture_drift_never_rewrites_in_check_mode(self):
        with tempfile.TemporaryDirectory() as temporary:
            package=Path(temporary)/'source.test-fixture';package.write_text('drift')
            with mock.patch.object(oracle,'PACKAGE',package),mock.patch.object(oracle,'compile_oracle') as compile,mock.patch.object(sys,'argv',['oracle',str(ROOT),'--check']):
                with self.assertRaisesRegex(SystemExit,'source package differs'):oracle.main()
            compile.assert_not_called();self.assertEqual(package.read_text(),'drift')
            fixtures=Path(temporary)/'fixtures';fixtures.mkdir()
            for name in oracle.FILES:(fixtures/name).write_bytes(b'drift')
            with mock.patch.object(oracle,'FIXTURES',fixtures),mock.patch.object(oracle,'compile_oracle',return_value=self.binary),mock.patch.object(sys,'argv',['oracle',str(ROOT),'--check']):
                with self.assertRaisesRegex(SystemExit,'fixture differs'):oracle.main()
            self.assertTrue(all((fixtures/name).read_bytes()==b'drift' for name in oracle.FILES))

    def test_malformed_schema_bounds_and_domains_fail_closed(self):
        for filename,fields,raw in (('cases.test-fixture',oracle.FIELDS,False),('postprocess.test-fixture',oracle.RAW_FIELDS,True)):
            lines=(oracle.FIXTURES/filename).read_text().splitlines();header,row=lines[1:3]
            for bad in ('\n'.join(lines[:1]+lines[2:]),'\n'.join(lines+[header]),'\n'.join(lines+[row]),header+'\n',header+'\n'+row+'\textra'):
                with self.assertRaises(SystemExit):oracle.parse_cases(bad.encode(),raw)
            common={'optimize':'2','normalize':'1','force':'yes','previous_type':'64','path_type':'2','from':'7fc00000,00000000,00000000','to':'0','actual_end':'7f800000,00000000,00000000','points':'-','before_active':'75','before_storage':'0','after_active':'75','after_storage':str(1<<64)}
            changes=common|({'limit':'1','raw_status':'success','raw_points':';'.join(['00000000,00000000,00000000']*75),'prior_points':'bad'} if raw else {'geometry':'world','phase':'cold','selected_active':'0','selected_storage':'0','smooth_status':'success','smooth_points':'bad'})
            for field,value in changes.items():
                with self.subTest(filename=filename,field=field):
                    values=row.split('\t');values[fields.index(field)]=value
                    with self.assertRaisesRegex(SystemExit,'Invalid oracle row'):oracle.parse_cases((header+'\n'+'\t'.join(values)).encode(),raw)

    def test_loaded_original_same_cut_replacement_and_failure_order(self):
        self.assertEqual(len(self.rows),23)
        self.assertEqual(len(self.raw),25)
        for name in ('replacement_conversion','replacement_optimize','cross_tile_failure'):
            self.assertEqual(self.named(name)['phase'],'replacement')
        same=self.named('same_polygon_source_order')
        self.assertEqual((same['path_type'],same['smooth_status'],same['after_active']),('1','-','1'))
        self.assertEqual(same['actual_end'],same['to'])
        self.assertEqual(self.named('prior_incomplete_retained')['path_type'],'4')
        self.assertEqual(self.named('prior_short_classified_normal')['path_type'],'1')
        self.assertEqual(self.named('prior_incomplete_force_endpoint')['path_type'],'17')
        failure=self.named('no_steer_vertical_cached_cut')
        self.assertEqual((failure['smooth_status'],len(oracle.points(failure['smooth_points'])),failure['path_type'],failure['after_active']),('40000000',1,'8','0'))
        for name in ('cross_tile_failure','cross_tile_failure_optimized'):
            r=self.named(name);self.assertEqual((r['smooth_status'],len(oracle.points(r['smooth_points'])),r['path_type'],r['after_active']),('80000000',74,'8','0'))
        for r in self.rows:
            self.assertEqual(r['selected_storage'],r['after_storage'])

    def test_loaded_force_clamp_preserves_or_clears_active_only(self):
        near=self.named('force_near_horizontal_replace_endpoint');far=self.named('force_far_horizontal_shortcut')
        self.assertEqual((near['path_type'],near['after_active']),('17','2'))
        self.assertEqual((far['path_type'],far['after_active']),('17','0'))
        self.assertEqual(near['actual_end'],near['to']);self.assertEqual(far['actual_end'],far['to'])
        self.assertEqual(far['points'],far['from']+';'+far['to'])
        for name in ('slope','snake'):
            a,b=self.named(name+'_raw'),self.named(name+'_optimize')
            self.assertEqual(a['smooth_points'],b['smooth_points'])
            self.assertLess(len(oracle.points(b['points'])),len(oracle.points(a['points'])))

    def test_raw_strict_line_fault_skip_six_and_mutated_previous(self):
        self.assertEqual([len(oracle.points(self.named('line_fault_'+suffix,True)['points'])) for suffix in ('below','equal','above')],[2,3,3])
        self.assertEqual(len(oracle.points(self.named('skip_limit_six',True)['points'])),4)
        points=oracle.points(self.named('mutated_previous_not_original_collinearity',True)['points'])
        self.assertEqual(points,[(1,1,1),(3,1,1),(5,1,1)])
        self.assertEqual(self.named('raw_short_equal_limit',True)['path_type'],'32')
        self.assertEqual(self.named('raw_failure_precedes_short',True)['path_type'],'8')
        self.assertEqual(self.named('raw_optimize_before_limit',True)['path_type'],'1')
        self.assertEqual(self.named('raw_failure_after_optimize',True)['path_type'],'8')

    def test_zero_sentinel_reads_prior_or_original_default_initialized_points(self):
        fresh=self.named('zero_input_fresh_default',True);prior=self.named('zero_input_prior_survives',True)
        self.assertEqual(fresh['actual_end'],'00000000,00000000,00000000')
        self.assertEqual(prior['actual_end'],prior['prior_points'].split(';')[-1])
        self.assertEqual(prior['points'],self.named('signed_zero_input_prior_survives',True)['points'])
        self.assertEqual(self.named('original_zero_input_raw_observation')['actual_end'],fresh['actual_end'])

    def test_force_ratio_and_inrange_are_strict_and_type_sensitive(self):
        for suffix,active in (('below','1'),('equal','0'),('above','0')):
            row=self.named('force_ratio_'+suffix,True)
            self.assertEqual((row['path_type'],row['after_active']),('17',active))
            self.assertEqual(row['actual_end'],row['to'])
        for axis in ('radius','height'):
            self.assertEqual(self.named('range_'+axis+'_below',True)['path_type'],'1')
            for suffix in ('equal','above'):self.assertEqual(self.named('range_'+axis+'_'+suffix,True)['path_type'],'17')
        self.assertEqual(self.named('force_incomplete_even_in_range',True)['path_type'],'17')
        self.assertEqual(self.named('force_normal_already_in_range',True)['path_type'],'1')

    def test_actual_crlf_git_checkout_preserves_exact_bytes(self):
        with tempfile.TemporaryDirectory(prefix='point-path-crlf-') as temporary:
            checkout=Path(temporary);expected={}
            files=list(oracle.HERE.glob('point_path_*'))+[oracle.HERE/'.gitattributes']+list(oracle.FIXTURES.iterdir())
            files += [oracle.HERE/'fixtures'/name for name in oracle.INPUT_FILES]
            for source in files:
                if not source.is_file():continue
                relative=source.relative_to(oracle.HERE);dest=checkout/relative;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,dest)
                expected[relative]=hashlib.sha256(source.read_bytes()).hexdigest()
            oracle.run(['git','init','-q',str(checkout)])
            oracle.run(['git','-C',str(checkout),'-c','core.autocrlf=true','add','.'])
            for relative in expected:(checkout/relative).unlink()
            oracle.run(['git','-C',str(checkout),'-c','core.autocrlf=true','checkout-index','--all','--force'])
            for relative,digest in expected.items():
                if relative.name!='.gitattributes':self.assertEqual(hashlib.sha256((checkout/relative).read_bytes()).hexdigest(),digest,str(relative))
            with mock.patch.object(oracle,'HERE',checkout):oracle.guard_inputs()


if __name__=='__main__':unittest.main()
