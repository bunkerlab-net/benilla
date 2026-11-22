#!/usr/bin/env python3
"""Fail-closed exact-source tile-presence provenance and observation guards."""
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
import tile_presence_generate as oracle
ROOT = Path(sys.argv.pop(1)).resolve()


class TilePresenceOracleGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources=oracle.guarded_sources(ROOT)
        cls.data=(oracle.FIXTURES/'cases.test-fixture').read_bytes()
        cls.rows=oracle.parse_cases(cls.data)
        cls.temp=tempfile.TemporaryDirectory(prefix='test-tile-presence-');cls.addClassCleanup(cls.temp.cleanup)
        cls.work=Path(cls.temp.name);cls.binary=oracle.compile_oracle(cls.sources,cls.work)
        cls.observed=oracle.observe(cls.binary,cls.work)

    def named(self,name):return next(row for row in self.rows if row['label']==name)

    def test_source_headers_and_both_licenses_hashbound_and_crlf_normalized(self):
        self.assertEqual(set(self.sources),oracle.SOURCE_FILES);self.assertEqual(len(self.sources),21)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for name,source in self.sources.items():
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(source.encode('latin1').replace(b'\n',b'\r\n'))
            self.assertEqual(oracle.guarded_sources(root),self.sources)
            for name,source in self.sources.items():
                with self.subTest(name=name):
                    (root/name).write_text(source+'\n// drift\n',encoding='latin1')
                    with self.assertRaisesRegex(SystemExit,'Wrong pinned source hash'):oracle.guarded_sources(root)
                    (root/name).write_text(source,encoding='latin1')
            (root/oracle.CPP).unlink()
            with self.assertRaises(FileNotFoundError):oracle.guarded_sources(root)

    def test_malformed_duplicate_incomplete_and_unsafe_manifests(self):
        for filename,required in ((oracle.MANIFEST,oracle.SOURCE_FILES),(oracle.HERE/'tile_presence_input_hashes.sha256',oracle.INPUT_FILES)):
            valid=filename.read_text();first=next(line for line in valid.splitlines() if not line.startswith('#'))
            for bad in (valid+first+'\n',valid.replace(first,''),'bad extra columns\n',valid.replace(first.split()[1],'../escape'),valid.replace(first[:64],'z'*64)):
                with tempfile.TemporaryDirectory() as temporary:
                    path=Path(temporary)/'manifest';path.write_text(bad)
                    with self.assertRaisesRegex(SystemExit,'hash manifest'):oracle.manifest_rows(path,required)

    def test_unchanged_complete_have_tile_and_scalar_two_three_axis_coordinate_methods(self):
        harness,package=oracle.build_harness(self.sources)
        self.assertEqual(package,oracle.PACKAGE.read_text(encoding='latin1'))
        for name,methods in ((oracle.CPP,oracle.METHODS),(oracle.GRID,oracle.COORDINATES)):
            for signature,_ in methods:
                method=oracle.exact_block(self.sources[name],signature)
                self.assertIn(method,harness);self.assertIn(method,package)
        self.assertIn('float point[VERTEX_SIZE] = {p.y, p.z, p.x}',harness)
        self.assertIn('if (tx == -1 || ty == -1)',harness)
        self.assertIn('m_navMesh->getTileAt(tx, ty, 0)',harness)
        self.assertNotIn('bool PathFinder::calculate(',harness)
        self.assertIn('bool PathFinder::calculate(',package)
        self.assertIn(self.sources['LICENSE'],package)
        self.assertIn(self.sources['dep/recastnavigation/License.txt'],package)
        self.assertNotIn('benilla_nav_',harness);self.assertNotIn('benilla_source_',harness)
        for name,source in self.sources.items():
            if name.startswith('dep/recastnavigation/'):
                self.assertEqual((self.work/name).read_text(encoding='latin1'),source)

    def test_source_locations_missing_bodies_and_harness_markers_fail_closed(self):
        for text in ('void f() {} void f() {}','void other() {}'):
            with self.assertRaisesRegex(SystemExit,'Expected one source block'):oracle.exact_block(text,'void f()')
        for text in ('void f() {','void f()'):
            with self.assertRaisesRegex(SystemExit,'Unclosed source block'):oracle.exact_block(text,'void f()')
        for filename in (oracle.CPP,oracle.GRID):
            sources=dict(self.sources);sources[filename]='\n'+sources[filename]
            with self.assertRaisesRegex(SystemExit,'Pinned source location drift'):oracle.build_harness(sources)
        real=Path.read_text
        for change in (lambda s:s.replace('// ORIGINAL_HAVE_TILE',''),lambda s:s+'\n// ORIGINAL_HAVE_TILE\n'):
            def read(path,*args,**kwargs):
                value=real(path,*args,**kwargs)
                return change(value) if path.name=='tile_presence_oracle.cpp' else value
            with mock.patch.object(Path,'read_text',read):
                with self.assertRaisesRegex(SystemExit,'Expected one harness marker'):oracle.build_harness(self.sources)

    def test_package_drift_prevents_compilation_and_rewrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            package=Path(temporary)/'package.test-fixture';package.write_text('drift')
            with mock.patch.object(oracle,'PACKAGE',package),mock.patch.object(oracle,'compile_oracle') as compiler,mock.patch.object(sys,'argv',['oracle',str(ROOT),'--check']):
                with self.assertRaisesRegex(SystemExit,'source package differs'):oracle.main()
            compiler.assert_not_called();self.assertEqual(package.read_text(),'drift')

    def test_fixture_drift_fails_without_rewrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixtures=Path(temporary)
            for name in oracle.FILES:(fixtures/name).write_bytes(b'drift')
            with mock.patch.object(oracle,'FIXTURES',fixtures),mock.patch.object(oracle,'compile_oracle',return_value=self.binary),mock.patch.object(sys,'argv',['oracle',str(ROOT),'--check']):
                with self.assertRaisesRegex(SystemExit,'Tile-presence fixture differs'):oracle.main()
            self.assertTrue(all((fixtures/name).read_bytes()==b'drift' for name in oracle.FILES))

    def test_audited_compiler_and_symbols_production_bridge_exclusion(self):
        with mock.patch.object(oracle.shutil,'which',return_value=None):
            with self.assertRaisesRegex(SystemExit,'Missing required g\\+\\+'):oracle.compile_oracle(self.sources,self.work)
        with mock.patch.object(oracle,'run',return_value=subprocess.CompletedProcess([],0,b'wrong')):
            with self.assertRaisesRegex(SystemExit,'Unsupported audited compiler profile'):oracle.compile_oracle(self.sources,self.work)
        with self.assertRaisesRegex(SystemExit,'Missing unchanged source symbol'):oracle.verify_symbols('')
        symbols=oracle.run(['nm','-C',str(self.binary)],stdout=subprocess.PIPE).stdout.decode()
        for name in ('PathFinder::HaveTile(', 'MaNGOS::IsValidMapCoord(float, float, float)', 'dtNavMesh::calcTileLoc(', 'dtNavMesh::getTileAt('):
            with self.assertRaisesRegex(SystemExit,'Missing .* symbol'):oracle.verify_symbols(symbols.replace(name,'removed('))
        for bridge in ('benilla_nav_create(', 'benilla_source_tile_presence(', 'benilla_tile_probe(', 'benilla::Mesh::load('):
            with self.assertRaisesRegex(SystemExit,'Production bridge linked'):oracle.verify_symbols(symbols+bridge)

    def test_extended_precision_compilation_fails_closed(self):
        result=subprocess.run([shutil.which('g++'),'-std=c++20','-DDT_POLYREF64=1','-mfpmath=387',
                               '-I'+str(self.work/'dep/recastnavigation/Detour/Include'),'-fsyntax-only',
                               str(self.work/'oracle.cpp')],capture_output=True)
        self.assertNotEqual(result.returncode,0)
        self.assertIn(b'FLT_EVAL_METHOD == 0',result.stderr)

    def test_deliberate_floating_environment_mutations_rejected_before_output(self):
        for mode in ('round-down','ftz','daz','sse-round'):
            with self.subTest(mode=mode),tempfile.TemporaryDirectory() as temporary:
                output=Path(temporary)
                result=subprocess.run([str(self.binary),str(output),str(oracle.HERE/'fixtures'),mode],capture_output=True)
                self.assertEqual(result.returncode,2);self.assertIn(b'unsupported floating environment',result.stderr)
                self.assertEqual(list(output.iterdir()),[])

    def test_constructor_only_repair_preserves_layout_and_every_other_source(self):
        original=self.sources[oracle.TILE_HEADER];repaired=oracle.repair_tile_header(original)
        self.assertEqual(repaired.replace('\tdtMeshTile() = default;\n','',1),original)
        for bad in (original.replace('struct dtMeshTile\n{','struct dtMeshTile {'),repaired,original+'\nstruct dtMeshTile\n{\n'):
            with self.assertRaisesRegex(SystemExit,'Tile-lifetime repair insertion drift'):oracle.repair_tile_header(bad)
        work=self.work/'repaired';work.mkdir();binary=oracle.compile_oracle(self.sources,work,repair_lifetime=True)
        self.assertEqual(oracle.observe(binary,work),self.observed)
        for name,source in self.sources.items():
            if name.startswith('dep/recastnavigation/'):
                self.assertEqual((work/name).read_text(encoding='latin1'),repaired if name==oracle.TILE_HEADER else source)

    def test_all_authored_inputs_hashbound_and_shifted_product_changes_narrow(self):
        oracle.guard_inputs();real=Path.read_bytes
        for filename in oracle.INPUT_FILES:
            def read(path):
                data=real(path);return data+b'drift' if path.name==filename else data
            with mock.patch.object(Path,'read_bytes',read):
                with self.assertRaisesRegex(SystemExit,'Authored input fixture drift'):oracle.guard_inputs()
        original=(oracle.HERE/'fixtures/0003232.mmtile').read_bytes()
        params=(oracle.HERE/'fixtures/authored.mmap').read_bytes()
        for suffix,index in (('minus-one',1),('minus-two',2)):
            tile=self.observed['tile-'+suffix+'.test-fixture']
            self.assertEqual(tile[:28]+tile[32:],original[:28]+original[32:])
            self.assertEqual(struct.unpack_from('<i',tile,28)[0],-index)
            shifted=self.observed['params-'+suffix+'.test-fixture']
            self.assertEqual(shifted[4:],params[4:])
            self.assertEqual(struct.unpack_from('<f',shifted)[0],index*struct.unpack_from('<f',params,12)[0])
        layer=self.observed['tile-layer-one.test-fixture']
        self.assertEqual(layer[:36]+layer[40:],original[:36]+original[40:]);self.assertEqual(struct.unpack_from('<i',layer,36)[0],1)

    def test_schema_duplicate_labels_bad_domains_and_inconsistent_short_circuit_fail_closed(self):
        lines=self.data.decode().splitlines();header,row=lines[:2],lines[2]
        for data in ('\n'.join(lines[:1]+lines[2:]),'\n'.join(lines+[lines[1]]),'\n'.join(lines+[row]),lines[1]+'\n',lines[1]+'\n'+row+'\textra', '\n'.join(lines).replace('both_present','bad/name')):
            with self.assertRaises(SystemExit):oracle.parse_cases(data.encode())
        for field,value in (('geometry','invented'),('scope','invented'),('from','00000000'),('to','xyz,00000000,00000000'),('start_tx',str(1<<31)),('start_ty','+0'),('native_safe','2'),('start_valid','-'),('start_have','0'),('end_checked','0'),('start_layer0','0'),('pair_have','0'),('nearest_ref','0')):
            values=row.split('\t');values[oracle.FIELDS.index(field)]=value
            with self.subTest(field=field),self.assertRaises(SystemExit):oracle.parse_cases(('\n'.join(header+['\t'.join(values)])+'\n').encode())

    def test_reproduction_safe_output_names_and_extra_files(self):
        for name,data in self.observed.items():self.assertEqual(data,(oracle.FIXTURES/name).read_bytes())
        for names in (('evil.mmtile',),('../outside.test-fixture',),('x.test-fixture','x.test-fixture')):
            with mock.patch.object(oracle,'FILES',names),mock.patch.object(oracle,'run') as run:
                with self.assertRaisesRegex(SystemExit,'Unsafe fixture output filename'):oracle.observe(self.binary,self.work)
                run.assert_not_called()
        extra=self.work/'output/unexpected.mmtile';extra.write_bytes(b'wrong')
        try:
            with self.assertRaisesRegex(SystemExit,'Unexpected oracle output files'):oracle.observe(self.binary,self.work)
        finally:extra.unlink()

    def test_source_destination_first_and_start_end_short_circuit(self):
        row=self.named('invalid_destination_first')
        self.assertEqual((row['destination_valid'],row['start_valid'],row['start_checked'],row['end_checked']),('0','-','0','0'))
        for row in self.rows:
            if row['destination_valid']=='0':self.assertEqual(row['start_valid'],'-')
            if row['start_have']!='1':self.assertEqual(row['end_checked'],'0')
        self.assertEqual(self.named('both_present')['pair_have'],'1')
        self.assertEqual(self.named('left_only_end_missing')['end_checked'],'1')
        self.assertEqual(self.named('right_only_start_missing')['end_checked'],'0')

    def test_negative_sentinel_is_exactly_minus_one_and_layer_zero_only(self):
        a,b=self.named('minus_one_present_rejected'),self.named('minus_two_present_allowed')
        self.assertEqual((a['start_tx'],a['start_layer0'],a['pair_have']),('-1','1','0'))
        self.assertEqual((b['start_tx'],b['start_layer0'],b['pair_have']),('-2','1','1'))
        layer=self.named('layer_one_only');self.assertEqual((layer['scope'],layer['start_layer0'],layer['pair_have']),('raw','0','0'))

    def test_real_nearby_polygon_succeeds_without_owning_tile(self):
        row=self.named('nearby_poly_tile_absent')
        self.assertEqual(row['from'],'c3c80000,3f800000,00000000')
        self.assertEqual((row['geometry'],row['start_layer0'],row['pair_have'],row['nearest_status']),('left','0','0','40000000'))
        self.assertGreater(int(row['nearest_ref']),0)

    def test_binary32_rounding_edge_tiny_values_and_axis_mapping(self):
        self.assertEqual(self.named('rounding_y_3087007744')['start_tx'],'0')
        self.assertEqual(self.named('rounding_y_3087007743')['start_tx'],'1')
        for label in ('rounding_y_2147483649','rounding_y_2147483648','rounding_y_0','rounding_y_1'):
            self.assertEqual(self.named(label)['start_tx'],'1')
        self.assertEqual((self.named('axes_distinct')['start_tx'],self.named('axes_distinct')['start_ty']),('1','0'))
        self.assertEqual((self.named('axes_permuted')['start_tx'],self.named('axes_permuted')['start_ty']),('0','1'))
        self.assertEqual(self.named('tile_edge_1_0_0')['start_tx'],'-1')
        self.assertEqual(self.named('tile_edge_1_0_1')['start_tx'],'0')

    def test_coordinate_edges_and_height_domain_are_distinct(self):
        # Source float MAP_HALFSIZE followed by double -0.5 yields this exact word.
        for axis in (0,1):
            for sign in (0,2):
                for d in (0,1,2):
                    label=f'map_edge_{axis}_{sign}_{d}_end';row=self.named(label)
                    self.assertEqual(row['destination_valid'],'0' if (sign,d) in {(0,0),(2,2)} else '1')
                    if d==1:self.assertEqual(int(row['to'].split(',')[axis],16)&0x7fffffff,0x46855455)
        for row in self.rows:
            if row['label'].startswith('height_'):
                self.assertEqual(row['pair_valid'],'1');self.assertEqual(row['pair_have'],'1')
                self.assertEqual(row['native_safe'],'0' if row['scope']=='raw' else '1')

    def test_actual_autocrlf_checkout_preserves_exact_bytes(self):
        with tempfile.TemporaryDirectory(prefix='tile-presence-crlf-') as temporary:
            checkout=Path(temporary);expected={};repo=oracle.HERE.parents[2]
            files=list(oracle.HERE.glob('tile_presence_*'))+[repo/'.gitattributes',oracle.HERE/'.gitattributes']+list(oracle.FIXTURES.iterdir())
            files += [oracle.HERE/'fixtures'/name for name in oracle.INPUT_FILES]
            for source in files:
                if not source.is_file():continue
                relative=source.relative_to(repo);dest=checkout/relative;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,dest)
                expected[relative]=hashlib.sha256(source.read_bytes()).hexdigest()
            oracle.run(['git','init','-q',str(checkout)])
            oracle.run(['git','-C',str(checkout),'-c','core.autocrlf=true','add','.'])
            for relative in expected:(checkout/relative).unlink()
            oracle.run(['git','-C',str(checkout),'-c','core.autocrlf=true','checkout-index','--all','--force'])
            for relative,digest in expected.items():
                if relative.name!='.gitattributes':self.assertEqual(hashlib.sha256((checkout/relative).read_bytes()).hexdigest(),digest,str(relative))
            with mock.patch.object(oracle,'HERE',checkout/oracle.HERE.relative_to(repo)):oracle.guard_inputs()


if __name__=='__main__':unittest.main()
