#!/usr/bin/env python3
"""Fail-closed independent warm source-oracle provenance, input, and observation guards."""
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
import warm_cache_generate as oracle
ROOT = Path(sys.argv.pop(1)).resolve()


class WarmCacheOracleGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources=oracle.guarded_sources(ROOT)
        cls.data=(oracle.FIXTURES/'cases.test-fixture').read_bytes()
        cls.rows=oracle.parse_cases(cls.data)
        cls.lookups=oracle.parse_lookups((oracle.FIXTURES/'lookups.test-fixture').read_bytes())
        cls.scans=oracle.parse_scans((oracle.FIXTURES/'scans.test-fixture').read_bytes())
        cls.traces=oracle.parse_traces((oracle.FIXTURES/'traces.test-fixture').read_bytes())
        cls.temp=tempfile.TemporaryDirectory(prefix='test-warm-cache-');cls.addClassCleanup(cls.temp.cleanup)
        cls.work=Path(cls.temp.name);cls.binary=oracle.compile_oracle(cls.sources,cls.work)
        cls.observed=oracle.observe(cls.binary,cls.work)

    def named(self,name,rows=None):return next(row for row in (self.rows if rows is None else rows) if row['label']==name)
    def lookup(self,name):return self.named(name,self.lookups)
    def scan(self,name):return self.named(name,self.scans)
    def refs(self,row,name='after_storage'):return oracle.ref_list(row[name])
    def mutate(self,kind,field,value):
        filename,fields,parser=kind
        data=(oracle.FIXTURES/filename).read_text().splitlines()
        index=next(i for i,line in enumerate(data) if not line.startswith('#'))
        values=data[index].split('\t');values[fields.index(field)]=value;data[index]='\t'.join(values)
        return parser,'\n'.join(data).encode()

    def test_every_original_source_header_license_is_hashbound_and_crlf_normalized(self):
        self.assertEqual(set(self.sources),oracle.SOURCE_FILES);self.assertEqual(len(self.sources),21)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for name,source in self.sources.items():
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(source.encode('latin1').replace(b'\n',b'\r\n'))
            self.assertEqual(oracle.guarded_sources(root),self.sources)
            for name,source in self.sources.items():
                with self.subTest(name=name):
                    (root/name).write_text(source+'\n// source drift\n',encoding='latin1')
                    with self.assertRaisesRegex(SystemExit,'Wrong pinned source hash'):oracle.guarded_sources(root)
                    (root/name).write_text(source,encoding='latin1')
            (root/oracle.CPP).unlink()
            with self.assertRaises(FileNotFoundError):oracle.guarded_sources(root)

    def test_duplicate_incomplete_malformed_unsafe_manifest_fails_closed(self):
        for filename,required in ((oracle.MANIFEST,oracle.SOURCE_FILES),(oracle.HERE/'warm_cache_input_hashes.sha256',oracle.INPUT_FILES)):
            valid=filename.read_text();first=next(line for line in valid.splitlines() if not line.startswith('#'))
            for bad in (valid+first+'\n',valid.replace(first,''),'bad row surplus\n',valid.replace(first.split()[1],'../escape'),valid.replace(first[:64],'z'*64)):
                with tempfile.TemporaryDirectory() as temporary:
                    path=Path(temporary)/'manifest';path.write_text(bad)
                    with self.assertRaisesRegex(SystemExit,'hash manifest'):oracle.manifest_rows(path,required)

    def test_unchanged_complete_lookup_bodies_clear_bounds_and_selected_fragments(self):
        harness,package=oracle.build_harness(self.sources)
        self.assertEqual(package,oracle.PACKAGE.read_text(encoding='latin1'))
        for signature,_ in oracle.METHODS:
            method=oracle.exact_block(self.sources[oracle.CPP],signature)
            self.assertIn(method,harness);self.assertIn(method,package)
        self.assertIn(oracle.exact_block(self.sources[oracle.HEADER],'void clear()'),harness)
        self.assertIn('m_pathPolyRefs.data() + pathStartIndex, m_polyLength * sizeof(dtPolyRef)',harness)
        self.assertIn('for (pathEndIndex = m_polyLength - 1; pathEndIndex > pathStartIndex; --pathEndIndex)',harness)
        self.assertIn('query->init(mmap->navMesh, 1024)',package)
        self.assertIn('const float maxDist = 3.0f',harness)
        self.assertIn('dtResult = m_navMeshQuery->findPath(',harness)
        self.assertNotIn('void PathFinder::BuildPolyPath(',harness)
        self.assertNotIn('void PathFinder::BuildShortcut(',harness)
        self.assertIn('void PathFinder::BuildPolyPath(',package)
        self.assertIn(self.sources['LICENSE'],package)
        self.assertIn(self.sources['dep/recastnavigation/License.txt'],package)
        self.assertNotIn('benilla_nav_',harness);self.assertNotIn('benilla_source_',harness)
        for name,source in self.sources.items():
            if name.startswith('dep/recastnavigation/'):
                self.assertEqual((self.work/name).read_text(encoding='latin1'),source)

    def test_source_bounds_locations_and_missing_harness_markers_fail_closed(self):
        for text in ('void f() {} void f() {}','void other() {}'):
            with self.assertRaisesRegex(SystemExit,'Expected one source block'):oracle.exact_block(text,'void f()')
        for text in ('void f() {','void f()'):
            with self.assertRaisesRegex(SystemExit,'Unclosed source block'):oracle.exact_block(text,'void f()')
        for source,start,end in (('a b a','a','b'),('b a','a','b'),('abc','missing','b')):
            with self.assertRaises(SystemExit):oracle.source_span(source,start,end)
        sources=dict(self.sources);sources[oracle.CPP]='\n'+sources[oracle.CPP]
        with self.assertRaisesRegex(SystemExit,'Pinned source location drift'):oracle.build_harness(sources)
        sources=dict(self.sources);sources[oracle.HEADER]=sources[oracle.HEADER].replace('#define MAX_POINT_PATH_LENGTH   74','#define MAX_POINT_PATH_LENGTH   73')
        with self.assertRaisesRegex(SystemExit,'Source warm lookup bounds differ'):oracle.build_harness(sources)
        sources=dict(self.sources);sources[oracle.MOVEMAP]=sources[oracle.MOVEMAP].replace('query->init(mmap->navMesh, 1024)','query->init(mmap->navMesh, 512)')
        with self.assertRaisesRegex(SystemExit,'Source map node pool differs'):oracle.build_harness(sources)
        real=Path.read_text
        for change in (lambda s:s.replace('// ORIGINAL_LOOKUP_METHODS',''),lambda s:s+'\n// ORIGINAL_LOOKUP_METHODS\n'):
            def read(path,*args,**kwargs):
                value=real(path,*args,**kwargs)
                return change(value) if path.name=='warm_cache_oracle.cpp' else value
            with mock.patch.object(Path,'read_text',read):
                with self.assertRaisesRegex(SystemExit,'Expected one harness marker'):oracle.build_harness(self.sources)

    def test_package_drift_prevents_compile_and_does_not_rewrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            package=Path(temporary)/'package.test-fixture';package.write_text('drift')
            with mock.patch.object(oracle,'PACKAGE',package),mock.patch.object(oracle,'compile_oracle') as compiler, mock.patch.object(sys,'argv',['oracle',str(ROOT),'--check']):
                with self.assertRaisesRegex(SystemExit,'source package differs'):oracle.main()
            compiler.assert_not_called();self.assertEqual(package.read_text(),'drift')

    def test_fixture_drift_fails_without_rewriting(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixtures=Path(temporary)
            for name in oracle.FILES:(fixtures/name).write_bytes(b'drift')
            with mock.patch.object(oracle,'FIXTURES',fixtures),mock.patch.object(oracle,'compile_oracle',return_value=self.binary),mock.patch.object(sys,'argv',['oracle',str(ROOT),'--check']):
                with self.assertRaisesRegex(SystemExit,'Warm-cache fixture differs'):oracle.main()
            self.assertTrue(all((fixtures/name).read_bytes()==b'drift' for name in oracle.FILES))

    def test_compiler_profile_symbols_and_production_bridge_exclusion(self):
        with mock.patch.object(oracle.shutil,'which',return_value=None):
            with self.assertRaisesRegex(SystemExit,'Missing required g\\+\\+'):oracle.compile_oracle(self.sources,self.work)
        with mock.patch.object(oracle,'run',return_value=subprocess.CompletedProcess([],0,b'wrong')):
            with self.assertRaisesRegex(SystemExit,'Unsupported audited compiler profile'):oracle.compile_oracle(self.sources,self.work)
        with mock.patch.object(oracle,'run',side_effect=subprocess.CalledProcessError(1,'g++')):
            with self.assertRaises(subprocess.CalledProcessError):oracle.compile_oracle(self.sources,self.work)
        with self.assertRaisesRegex(SystemExit,'Missing unchanged source symbol'):oracle.verify_symbols('')
        symbols=oracle.run(['nm','-C',str(self.binary)],stdout=subprocess.PIPE).stdout.decode()
        for name in ('dtNavMeshQuery::findPath(', 'dtCreateNavMeshData('):
            with self.assertRaisesRegex(SystemExit,'Missing actual Detour symbol'):oracle.verify_symbols(symbols.replace(name,'removed('))
        for bridge in ('benilla_nav_create(', 'benilla_source_warm_create('):
            with self.assertRaisesRegex(SystemExit,'Production bridge linked'):oracle.verify_symbols(symbols+bridge)

    def test_constructor_only_repair_preserves_layout_and_every_other_source(self):
        original=self.sources[oracle.TILE_HEADER];repaired=oracle.repair_tile_header(original)
        self.assertEqual(repaired.replace('\tdtMeshTile() = default;\n','',1),original)
        self.assertEqual(repaired.count('dtMeshTile() = default;'),1)
        for bad in (original.replace('struct dtMeshTile\n{','struct dtMeshTile {'),repaired,original+'\nstruct dtMeshTile\n{\n'):
            with self.assertRaisesRegex(SystemExit,'Tile-lifetime repair insertion drift'):oracle.repair_tile_header(bad)
        work=self.work/'repaired';work.mkdir();binary=oracle.compile_oracle(self.sources,work,repair_lifetime=True)
        self.assertEqual(oracle.observe(binary,work),self.observed)
        for name,source in self.sources.items():
            if name.startswith('dep/recastnavigation/'):
                self.assertEqual((work/name).read_text(encoding='latin1'),repaired if name==oracle.TILE_HEADER else source)

    def test_reused_inputs_are_hashbound_and_binary_product_is_source_written(self):
        oracle.guard_inputs();real=Path.read_bytes
        def read(path):
            data=real(path);return data+b'drift' if path.name=='long1100.test-fixture' else data
        with mock.patch.object(Path,'read_bytes',read):
            with self.assertRaisesRegex(SystemExit,'Authored input fixture drift'):oracle.guard_inputs()
        precision=self.observed['precision.test-fixture']
        self.assertEqual(struct.unpack_from('<III',precision),(0x4d4d4150,7,8))
        self.assertEqual(struct.unpack_from('<I',precision,12)[0]+20,len(precision))
        self.assertEqual(struct.unpack_from('<ii',precision,28),(1,1))
        self.assertEqual(len(precision),880)

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

    def test_bad_schemas_duplicates_and_all_numeric_domains_fail_closed(self):
        kinds=[('cases.test-fixture',oracle.FIELDS,oracle.parse_cases),('lookups.test-fixture',oracle.LOOKUP_FIELDS,oracle.parse_lookups),('scans.test-fixture',oracle.SCAN_FIELDS,oracle.parse_scans),('traces.test-fixture',oracle.TRACE_FIELDS,oracle.parse_traces)]
        for filename,fields,parser in kinds:
            lines=(oracle.FIXTURES/filename).read_text().splitlines();header,row=lines[1:3]
            for bad in ('\n'.join(lines[:1]+lines[2:]),'\n'.join(lines+[header]),'\n'.join(lines+[row]),header+'\n',header+'\n'+row+'\textra'):
                with self.assertRaises(SystemExit):parser(bad.encode())
        changes=[(0,'limit','0'),(0,'limit','75'),(0,'before_active','75'),(0,'before_storage','0'),(0,'after_active','75'),(0,'direct_nodes','1025'),(0,'from','7fc00000,00000000,00000000'),(0,'scope','invented'),(0,'start_origin','invented'),(0,'phase','invented'),(0,'start_appended','2'),
                 (1,'point','7f800000,00000000,00000000'),(1,'limit','75'),(1,'before_active','75'),(1,'appended','2'),(1,'after_storage','0'),(1,'near5_status','success'),
                 (2,'max_distance','3f800000'),(2,'refs','0,0,0'),(2,'distance','7fc00000'),(2,'selected_ref',str(1<<64)),
                 (3,'endpoint','2'),(3,'ordinal','1'),(3,'primitive','scripted'),(3,'polygon',str(1<<64)),(3,'distance_bits','7fc00000')]
        for kind,field,value in changes:
            with self.subTest(field=field,value=value):
                parser,data=self.mutate(kinds[kind],field,value)
                with self.assertRaisesRegex(SystemExit,'Invalid oracle row'):parser(data)

    def test_squared_threshold_adjacent_words_and_sqrt_return_bits(self):
        for threshold in (1,3,7):
            center=struct.unpack('!I',struct.pack('!f',threshold))[0]
            for delta,suffix in ((-1,'below'),(0,'at'),(1,'above')):
                row=self.scan(f'squared{threshold}_{suffix}')
                self.assertEqual(int(row['squared_distances'].split(',')[0],16),center+delta)
                if threshold!=1:self.assertEqual(row['selected_ref']=='0',delta>0)
        self.assertEqual(self.scan('squared1_below')['selected_ref'],self.scan('squared1_below')['refs'].split(',')[0])
        self.assertEqual(self.scan('squared1_at')['selected_ref'],self.scan('squared1_at')['refs'].split(',')[1])
        self.assertEqual(self.scan('squared3_at')['distance'],'3fddb3d7')
        self.assertEqual(self.scan('squared7_at')['distance'],'402953fd')

    def test_ordered_ties_and_early_break_can_ignore_a_closer_polygon(self):
        for label in ('ordered_tie_ab','ordered_tie_ba'):
            row=self.scan(label);self.assertEqual(row['selected_ref'],row['refs'].split(',')[0]);self.assertEqual(row['squared_distances'],'3f800000,3f800000')
        row=self.scan('early_break_before_closer')
        self.assertEqual(row['selected_ref'],row['refs'].split(',')[0])
        a,b=[int(v,16) for v in row['squared_distances'].split(',')];self.assertLess(b,a)
        self.assertNotEqual(row['selected_ref'],self.scan('closer_order_reversed')['selected_ref'])

    def test_tail_overwrites_comparison_and_genuine_smaller_append(self):
        self.assertEqual(self.lookup('active_before_tail')['origin'],'active')
        self.assertEqual(self.lookup('active_before_tail')['tail_checked'],'0')
        self.assertEqual(self.lookup('warm_append_smaller')['appended'],'1')
        self.assertEqual(self.lookup('nearest_equal_no_append')['appended'],'0')
        self.assertEqual(self.lookup('tail_overwrites_prevents_append')['appended'],'0')
        row=self.lookup('tail_overwrites_appends_duplicate');self.assertEqual(row['appended'],'1')
        self.assertEqual(row['distance'],row['active_distance'])
        self.assertEqual(self.refs(row)[0],self.refs(row)[-1])
        # Every observed finite cache miss already wrote a positive comparison distance.
        # The zero-disjunct is unreachable here; no initialized-cold witness is fabricated.
        for row in self.lookups:
            if row['origin'] in {'nearest5','nearest10','miss'}:
                self.assertGreater(oracle.finite_bits(row['tail_distance'] if row['tail_checked']=='1' else row['active_distance']),0)

    def test_nearest5_10_actual_fallback_and_miss(self):
        self.assertEqual(self.lookup('nearest5_boundary')['origin'],'nearest5')
        for label in ('nearest10_fallback','nearest10_boundary'):
            row=self.lookup(label);self.assertEqual(row['origin'],'nearest10');self.assertEqual(row['near5_ref'],'0');self.assertNotEqual(row['near10_ref'],'0')
        row=self.lookup('nearest10_miss');self.assertEqual(row['origin'],'miss');self.assertEqual(row['selected'],'0');self.assertEqual(row['appended'],'0')

    def test_far_distance_strict_seven_is_separate_from_squared_tail_threshold(self):
        for offset,suffix in ((-1,'below'),(0,'at'),(1,'above')):
            row=self.lookup('far_distance7_'+suffix)
            self.assertEqual(row['origin'],'nearest10');self.assertEqual(int(row['distance'],16),0x40e00000+offset)
            case=self.named('far_distance7_'+suffix)
            self.assertEqual(case['scope'],'excluded' if offset>0 else 'runtime')
            self.assertEqual(case['phase'],'excluded' if offset>0 else 'replacement')

    def test_sequential_append_then_tail_hit_and_full_storage_transitions(self):
        row=self.named('start_append_then_end_tail_hit');self.assertEqual((row['start_origin'],row['start_appended'],row['end_origin']),('nearest5','1','tail'))
        self.assertEqual(len(self.refs(row)),75);self.assertEqual(self.refs(row)[-1],int(row['start_ref']))
        same=self.named('same_polygon_clear_retains_storage');self.assertEqual(same['after_active'],'1');self.assertEqual(self.refs(same),self.refs(same,'before_storage'))
        cut=self.named('cut_first_start_last_end_duplicates');before=self.refs(cut,'before_storage');after=self.refs(cut)
        self.assertEqual(cut['after_active'],'6');self.assertEqual(after[:6],before[1:7]);self.assertEqual(after[6:],before[6:])
        row=self.named('replacement_retains_tail');before=self.refs(row,'before_storage');after=self.refs(row)
        self.assertEqual(row['phase'],'replacement');self.assertEqual(after[2:len(before)],before[2:]);self.assertEqual(after[-1],int(row['start_ref']))
        self.assertEqual(self.named('reverse_order_replaces')['phase'],'replacement')

    def test_cold_uninitialized_path_excluded_and_lower_limit_helper_only(self):
        row=self.lookup('cold_without_tail_excluded');self.assertEqual(row['origin'],'excluded');self.assertEqual(row['near5_checked'],'0');self.assertEqual(row['before_storage'],row['after_storage'])
        self.assertFalse(any(row['label']=='cold_without_tail_excluded' for row in self.traces))
        lower=self.lookup('lower_limit_exposes_zero_gap_helper_only');self.assertEqual((lower['scope'],lower['limit'],lower['origin']),('helper','2','tail'))
        self.assertTrue(all(row['limit']=='74' for row in self.rows))
        self.assertEqual(self.lookup('inactive_gap_not_scanned')['origin'],'nearest5')
        self.assertEqual(self.lookup('zero_inactive_tail_skipped')['origin'],'tail')

    def test_actual_original_direct_partial_truncated_outnodes_are_not_operational_caps(self):
        expected={'disconnected_partial':('40000040','2'),'long600_truncated':('40000010','600'),'long1100_out_of_nodes':('40000070','1024'),'seventyfive_truncated':('40000010','75')}
        for label,(status,nodes) in expected.items():
            row=self.named(label);self.assertEqual((row['scope'],row['direct_status'],row['direct_nodes']),('raw',status,nodes))
        exact=self.named('seventyfour_exact_capacity');self.assertEqual((exact['scope'],exact['direct_status'],exact['after_active']),('runtime','40000000','74'))
        grid=self.named('cross_side_original_direct_order');base=4503599627370496
        self.assertEqual(self.refs(grid)[:int(grid['after_active'])],[base,base+(1<<31),base+(3<<31)])

    def test_trace_observes_actual_original_calls_and_start_end_order(self):
        rows=[row for row in self.traces if row['label']=='start_append_then_end_tail_hit']
        self.assertEqual([(row['endpoint'],row['primitive']) for row in rows],[('0','active'),('0','nearest5'),('1','active'),('1','tail')])
        rows=[row for row in self.traces if row['label']=='active_before_tail' and row['endpoint']=='0']
        self.assertEqual(len(rows),1);self.assertEqual(rows[0]['primitive'],'active')
        rows=[row for row in self.traces if row['label']=='nearest10_fallback' and row['endpoint']=='0']
        self.assertEqual([row['primitive'] for row in rows],['active','nearest5','nearest10'])
        self.assertEqual(rows[-2]['polygon'],'0');self.assertEqual(rows[-2]['distance_bits'],'00000000')

    def test_actual_autocrlf_checkout_preserves_all_input_output_and_source_bytes(self):
        with tempfile.TemporaryDirectory(prefix='warm-cache-crlf-') as temporary:
            checkout=Path(temporary);expected={}
            files=list(oracle.HERE.glob('warm_cache_*'))+[oracle.HERE/'.gitattributes']+list(oracle.FIXTURES.iterdir())
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
