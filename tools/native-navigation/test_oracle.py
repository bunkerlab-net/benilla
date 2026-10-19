#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate independent fixture integrity; generation verification is --check."""
import json
from pathlib import Path
import struct
import unittest
import zlib
from run_oracle import check_generation,mesh_info,sha
from authored_models import cases as model_cases
HERE=Path(__file__).resolve().parent

class SourceOracleFixtures(unittest.TestCase):
    def test_two_tile_seam_uses_one_authored_interior_hole(self):
        base=HERE/'fixtures/two-tile-seam';manifest=json.loads((base/'manifest.json').read_bytes())
        feature=manifest['authored_feature']
        self.assertEqual((feature['mcnk_row'],feature['mcnk_col'],feature['hole_mask']),(8,8,1))
        self.assertTrue(feature['height_function_unchanged'])
        for extension in ['adt','map']:
            new=(base/feature[extension+'_fixture']).read_bytes()
            old=(HERE/'fixtures/authored'/('ridge_xminus.'+extension+('.test-fixture' if extension=='adt' else ''))).read_bytes()
            self.assertEqual(len(new),len(old));self.assertEqual(sha(new),feature[extension+'_sha256'])
            cell=8*16+8
            offset=struct.unpack_from('<I',old,92+16*cell)[0]+8+60 if extension=='adt' else struct.unpack_from('<I',old,32)[0]+2*cell
            self.assertEqual([(index,a,b) for index,(a,b) in enumerate(zip(old,new)) if a!=b],[(offset,0,1)])
        self.assertEqual(sha((base/'authored/writer-provenance.json').read_bytes()),manifest['writer_provenance_sha256'])
        negative=json.loads((base/'neighbor-without-hole-expected.json').read_bytes())
        self.assertFalse(negative['output']['emitted']);self.assertEqual(negative['output']['merged_polygons'],625)
        self.assertEqual(sha((base/'neighbor-without-hole-expected.json').read_bytes()),manifest['negative_control']['expected_sha256'])

    def test_two_original_emitted_tiles_preserve_generation_and_query_inputs(self):
        from capture_two_tile_seam import check_crossing
        base=HERE/'fixtures/two-tile-seam';manifest=json.loads((base/'manifest.json').read_bytes())
        self.assertEqual(set(manifest['products']),{'000.mmap','0003130.mmtile','0003129.mmtile'})
        for filename,expected in manifest['products'].items():
            raw=(base/filename).read_bytes();self.assertEqual({'bytes':len(raw),'sha256':sha(raw)},expected)
        for name,x in [('selected',30),('neighbor',29)]:
            entry=manifest['tiles'][name]
            self.assertEqual(entry['original_tile'],{'map_id':0,'x':x,'y':31})
            self.assertEqual(entry['production_tile_id'],{'map_id':0,'x':31,'y':x})
            self.assertEqual(json.loads((base/name/'expected.json').read_bytes()),entry['expected'])
            for filename in ['source-terrain.bin','source-combined.bin','source-mesh.bin']:
                self.assertEqual(mesh_info(zlib.decompress((base/name/(filename+'.zlib')).read_bytes())),entry[filename])
            raw=zlib.decompress((base/name/'helper-request.bin.zlib').read_bytes())
            self.assertEqual({'bytes':len(raw),'sha256':sha(raw)},entry['helper_request'])
            self.assertEqual(struct.unpack_from('<4I',raw,8),(0,x,31,1))
            self.assertEqual(raw[32:60],(base/'000.mmap').read_bytes())
            self.assertTrue(raw.endswith(zlib.decompress((base/name/'source-mesh.bin.zlib').read_bytes())))
        prior=HERE/'fixtures/ridge_xminus_wmo'
        self.assertEqual((base/'0003130.mmtile').read_bytes(),(prior/'0003130.mmtile').read_bytes())
        self.assertEqual((base/'selected/expected.json').read_bytes(),(prior/'expected.json').read_bytes())
        for filename in ['source-terrain.bin','source-combined.bin','source-mesh.bin','helper-request.bin']:
            self.assertEqual((base/'selected'/(filename+'.zlib')).read_bytes(),(prior/(filename+'.zlib')).read_bytes())
        query_input=json.loads((base/'query-input.json').read_bytes())
        result=json.loads((base/'expected.json').read_bytes());check_crossing(result)
        self.assertEqual(sha((base/'query-input.json').read_bytes()),manifest['query_input_sha256'])
        self.assertEqual(sha((base/'expected.json').read_bytes()),manifest['expected_sha256'])
        for request,result in zip(query_input['queries'],result['queries']):
            a=manifest['tiles']['selected']['expected']['output']['queries'][request['selected_source_query_index']]['nearest_bits']
            b=manifest['tiles']['neighbor']['expected']['output']['queries'][request['neighbor_source_query_index']]['nearest_bits']
            if request['name'].endswith('reverse'):a,b=b,a
            self.assertEqual(request['start_bits'],a);self.assertEqual(request['end_bits'],b)
            self.assertEqual(result['start_bits'],a);self.assertEqual(result['end_bits'],b)

    def test_original_slab_predicate_boundary_capture(self):
        base=HERE/'fixtures/slab-predicates';manifest=json.loads((base/'manifest.json').read_bytes())
        raw=(base/'expected.json').read_bytes();self.assertEqual(sha(raw),manifest['expected_sha256'])
        cases={row['name']:row for row in json.loads(raw)['cases']}
        self.assertEqual(len(cases),28);self.assertEqual(sum(row['bounded_finite_scope'] for row in cases.values()),25)
        self.assertEqual(manifest['source_sha256'],json.loads((HERE/'source/source-hashes.json').read_bytes())[manifest['source_file']])
        for stem in ['normal_positive','normal_negative','horizontal_shrink','vertical_positive','vertical_negative']:
            self.assertEqual([cases[stem+'_'+suffix]['connection_count'] for suffix in ['below','equal','above']],[1,1,0])
        for name in ['horizontal_touching','target_climb_one_rejects']:
            self.assertEqual(cases[name]['connection_count'],0)
        for name in ['target_climb_two_accepts_reverse','crossing_slopes','reversed_source','reversed_target','reversed_both','side_zero','side_two','side_six_reversed']:
            self.assertEqual(cases[name]['connection_count'],1)
        self.assertFalse(cases['unsupported_horizontal_overflow']['bounded_finite_scope'])
        self.assertEqual(cases['unsupported_horizontal_overflow']['connection_count'],1)

    def test_model_case_labels_preserve_authored_inputs_on_case_insensitive_filesystems(self):
        base = HERE / 'fixtures/models'
        manifest = json.loads((base / 'manifest.json').read_bytes())
        cases = list(model_cases())
        self.assertEqual({name for name, _, _ in cases}, set(manifest))
        self.assertEqual(len({name.casefold() for name, _, _ in cases}), len(cases))
        for name, records, models in cases:
            with self.subTest(case=name):
                inputs = json.loads((base / name / 'inputs.json').read_bytes())
                self.assertEqual(bytes.fromhex(inputs['dir_bin_hex']), records)
                self.assertEqual({key: bytes.fromhex(value) for key, value in inputs['raw_models'].items()}, models)

    def test_all_raw_arrays_match_recorded_original_hashes(self):
        for manifest_path in [HERE/'fixtures/manifest.json',HERE/'fixtures/models/manifest.json',HERE/'fixtures/quantized/manifest.json']:
            manifest=json.loads(manifest_path.read_text(encoding="utf-8"));cases=manifest.get('cases',manifest)
            for case,entry in cases.items():
                for name,expected in entry.items():
                    if name.endswith('.bin'):
                        with self.subTest(case=case,array=name):
                            raw=zlib.decompress((manifest_path.parent/case/(name+'.zlib')).read_bytes())
                            self.assertEqual(mesh_info(raw),expected)

    def test_actual_original_positive_and_exact625_negative(self):
        manifest=json.loads((HERE/'fixtures/manifest.json').read_text(encoding="utf-8"))
        for case in ['ridge_wmo','ridge_neighbors_wmo','ridge_xminus_wmo']:
            entry=manifest['cases'][case];base=HERE/'fixtures'/case;expected=entry['expected']['output']
            self.assertTrue(expected['emitted']);self.assertEqual(expected['merged_polygons'],634)
            self.assertEqual(expected['areas'],{'10':2,'11':632})
            for name,description in entry['products'].items():
                data=(base/name).read_bytes();self.assertEqual(len(data),description['bytes']);self.assertEqual(sha(data),description['sha256'])
            raw=zlib.decompress((base/'helper-request.bin.zlib').read_bytes())
            self.assertEqual(sha(raw),entry['helper_request']['sha256']);self.assertEqual(raw[:8],b'BNAVIN01')
            self.assertEqual(struct.unpack_from('<4I',raw,8),(0,30,31,1))
            self.assertTrue(all(q['poly_ref'] for q in expected['queries']))
            self.assertTrue(all(p['corridor'][-1]==p['end_ref'] for p in expected['paths']))
        negative=manifest['cases']['ridge_only']['expected']['output']
        self.assertEqual(negative['merged_polygons'],625);self.assertFalse(negative['emitted'])
        self.assertFalse((HERE/'fixtures/ridge_only/0003130.mmtile').exists())

    def test_source_neighbor_union_and_open_order(self):
        manifest=json.loads((HERE/'fixtures/manifest.json').read_text(encoding="utf-8"))
        for case,count in [('ridge_wmo',1),('ridge_neighbors_wmo',5),('ridge_xminus_wmo',2)]:
            expected=manifest['cases'][case]['expected'];events=expected['terrain_file_events']
            self.assertEqual([e['basename'] for e in events],['0003130.map','0003131.map','0003129.map','0003230.map','0003030.map'])
            self.assertEqual(sum(e['opened'] for e in events),count)
            self.assertEqual(expected['output']['max_tiles'],count)
            self.assertEqual(len(expected['tile_union']),count)
        self.assertEqual(len(manifest['cases']['flat']['expected']['terrain_file_events']),1)

    def test_original_single_neighbor_changes_selected_output_without_moving_origin(self):
        base=HERE/'fixtures';manifest=json.loads((base/'manifest.json').read_text(encoding='utf-8'))
        case='ridge_xminus_wmo';entry=manifest['cases'][case]
        expected=json.loads((base/case/'expected.json').read_text(encoding='utf-8'))
        selected=manifest['cases']['ridge_wmo']['expected']
        self.assertEqual(expected,entry['expected']);check_generation(case,expected)
        self.assertEqual(entry['neighbors'],['xminus'])
        self.assertEqual(set(entry['maps']),{'0003130.map','0003129.map'})
        self.assertEqual(expected['tile_union'],[[29,31],[30,31]])
        self.assertEqual([e['opened'] for e in expected['terrain_file_events']],[True,False,True,False,False])
        self.assertEqual(expected['terrain_vertices_before_clean'],66050)
        self.assertEqual(expected['terrain_triangles'],66048)
        self.assertEqual(expected['terrain_triangles']-selected['terrain_triangles'],512)
        self.assertEqual(expected['combined_vertices_before_clean'],66053)
        self.assertEqual((expected['solid_vertices'],expected['solid_triangles']),(33414,66049))
        self.assertEqual((expected['liquid_vertices'],expected['liquid_triangles']),(0,0))
        for key in ['bounds_min_bits','bounds_max_bits','config']:
            self.assertEqual(expected[key],selected[key])
        for key in ['params_origin_bits','max_polys','tile_x','tile_y']:
            self.assertEqual(expected['output'][key],selected['output'][key])
        self.assertEqual(expected['output']['max_tiles'],2)
        self.assertEqual(set(entry['products']),{'000.mmap','0003130.mmtile'})
        old_map=(base/'ridge_wmo/000.mmap').read_bytes();new_map=(base/case/'000.mmap').read_bytes()
        self.assertEqual(new_map[:20],old_map[:20]);self.assertEqual(new_map[24:],old_map[24:])
        self.assertEqual(struct.unpack_from('<I',old_map,20),(1,))
        self.assertEqual(struct.unpack_from('<I',new_map,20),(2,))
        self.assertNotEqual((base/case/'0003130.mmtile').read_bytes(),(base/'ridge_wmo/0003130.mmtile').read_bytes())
        self.assertEqual(entry['products']['0003130.mmtile']['bytes'],107052)
        for name in ['source-terrain.bin','source-combined.bin','source-mesh.bin','helper-request.bin']:
            self.assertNotEqual(zlib.decompress((base/case/(name+'.zlib')).read_bytes()),
                                zlib.decompress((base/'ridge_wmo'/(name+'.zlib')).read_bytes()))
        raw=zlib.decompress((base/case/'helper-request.bin.zlib').read_bytes())
        self.assertEqual(raw[32:60],new_map)
        self.assertTrue(raw.endswith(zlib.decompress((base/case/'source-mesh.bin.zlib').read_bytes())))
        self.assertEqual(len(expected['output']['queries']),25)
        self.assertEqual(len(expected['output']['paths']),24)
        for query in expected['output']['queries']:
            self.assertEqual(query['status'],0x40000000)
            self.assertEqual(query['height_status'],0x40000000)
        for path in expected['output']['paths']:
            self.assertEqual(path['status'],0x40000000)
            self.assertEqual(path['straight_status'],0x40000000)
        provenance=entry['input_provenance']
        self.assertEqual(provenance['absent_original_map_filenames'],['0003131.map','0003230.map','0003030.map'])
        self.assertEqual([row['original_tile'] for row in provenance['terrain_inputs']],
                         [{'map_id':0,'x':30,'y':31},{'map_id':0,'x':29,'y':31}])
        self.assertEqual([row['production_tile_id'] for row in provenance['terrain_inputs']],
                         [{'map_id':0,'x':31,'y':30},{'map_id':0,'x':31,'y':29}])
        for row in provenance['terrain_inputs']:
            for kind in ['adt','map']:
                self.assertEqual(sha((base/row[kind+'_fixture']).read_bytes()),row[kind+'_sha256'])
            self.assertEqual(entry['maps'][row['original_map_filename']],row['map_sha256'])
        self.assertEqual(sha((base/'authored/writer-provenance.json').read_bytes()),provenance['writer_provenance_sha256'])
        self.assertEqual(sha((HERE/'source/source-hashes.json').read_bytes()),provenance['source_hashes_sha256'])
        self.assertEqual(provenance['vmap_products_sha256'],manifest['cases']['liquid_full_types_wmo']['input_provenance']['vmap_products_sha256'])

    def test_original_generated_liquid_retains_ground_only_misses(self):
        manifest=json.loads((HERE/'fixtures/manifest.json').read_text(encoding="utf-8"))
        case='liquid_full_types_wmo';entry=manifest['cases'][case];base=HERE/'fixtures'/case
        expected=json.loads((base/'expected.json').read_text(encoding="utf-8"));output=expected['output']
        self.assertEqual(expected,entry['expected']);check_generation(case,expected)
        self.assertEqual((expected['solid_vertices'],expected['solid_triangles']),(3,1))
        self.assertEqual((expected['liquid_vertices'],expected['liquid_triangles']),(16641,32768))
        self.assertEqual(expected['liquid_triangle_areas'],{'8':4736,'9':28032})
        self.assertEqual(output['poly_count'],998)
        self.assertEqual(output['polygon_flags'],{'4':770,'8':228})
        for key in ['mesh_init_status','add_tile_status','query_init_status']:
            self.assertEqual(output[key],0x40000000)
        for query in output['queries']:
            self.assertEqual(query['nearest_bits'],['00000000']*3)
            self.assertNotIn('height_status',query)
        for name,description in entry['products'].items():
            data=(base/name).read_bytes();self.assertEqual(len(data),description['bytes']);self.assertEqual(sha(data),description['sha256'])
        raw=zlib.decompress((base/'helper-request.bin.zlib').read_bytes())
        self.assertEqual(len(raw),entry['helper_request']['bytes']);self.assertEqual(sha(raw),entry['helper_request']['sha256'])
        self.assertEqual(raw[:8],b'BNAVIN01');self.assertEqual(struct.unpack_from('<4I',raw,8),(0,30,31,1))
        self.assertEqual(raw[32:60],(base/'000.mmap').read_bytes())
        self.assertTrue(raw.endswith(zlib.decompress((base/'source-mesh.bin.zlib').read_bytes())))
        provenance=entry['input_provenance'];root=HERE.parents[1]
        for kind in ['adt','map']:
            self.assertEqual(sha((root/provenance[kind+'_fixture']).read_bytes()),provenance[kind+'_sha256'])
        self.assertEqual(provenance['production_tile_id'],{'map_id':0,'x':31,'y':30})
        self.assertEqual(provenance['original_tile'],{'map_id':0,'x':30,'y':31})
        self.assertEqual(sha((HERE/'fixtures/authored/writer-provenance.json').read_bytes()),provenance['writer_provenance_sha256'])
        self.assertEqual(sha((HERE/'source/source-hashes.json').read_bytes()),provenance['source_hashes_sha256'])

    def test_original_single_neighbor_changes_one_edge_but_preserves_sampled_routes(self):
        base=HERE/'fixtures'
        def sections(case):
            raw=(base/case/'0003130.mmtile').read_bytes()[20:]
            header=struct.unpack_from('<15I',raw)
            self.assertEqual(header[:2],(0x444e4156,7))
            offset=100;result={'max_links':header[8]}
            for name,size in [('vertices',header[7]*12),('polygons',header[6]*32),
                              ('links',header[8]*16),('detail_meshes',header[9]*12),
                              ('detail_vertices',header[10]*12),('detail_triangles',header[11]*4),
                              ('bv_nodes',header[12]*16)]:
                result[name]=raw[offset:offset+size];offset+=size
            self.assertEqual(offset,len(raw))
            result['portal_sides']=[]
            for index in range(header[6]):
                polygon=result['polygons'][index*32:(index+1)*32]
                for neighbor in struct.unpack_from('<6H',polygon,16)[:polygon[30]]:
                    if neighbor&0x8000:result['portal_sides'].append(neighbor&0x7fff)
            return result
        old=sections('ridge_wmo');new=sections('ridge_xminus_wmo')
        self.assertEqual(old['portal_sides'],[])
        self.assertEqual(new['portal_sides'],[0]*25)
        self.assertEqual((old['max_links'],new['max_links']),(2530,2580))
        self.assertEqual(len(new['links'])-len(old['links']),800)
        for name in ['vertices','polygons','bv_nodes']:
            self.assertNotEqual(new[name],old[name])
        vertices=list(zip(struct.iter_unpack('<3I',old['vertices']),struct.iter_unpack('<3I',new['vertices'])))
        changed=[(before,after) for before,after in vertices if before!=after]
        self.assertEqual(len(changed),26)
        self.assertTrue(all(before[1:]==after[1:] for before,after in changed))
        for name in ['detail_meshes','detail_vertices','detail_triangles']:
            self.assertEqual(new[name],old[name])
        old_expected=json.loads((base/'ridge_wmo/expected.json').read_text(encoding='utf-8'))
        new_expected=json.loads((base/'ridge_xminus_wmo/expected.json').read_text(encoding='utf-8'))
        for key in ['queries','paths']:
            self.assertEqual(new_expected['output'][key],old_expected['output'][key])

    def test_ground_corridor_requirement_remains_strict_for_ridge_cases(self):
        manifest=json.loads((HERE/'fixtures/manifest.json').read_text(encoding="utf-8"))
        for case in ['ridge_wmo','ridge_neighbors_wmo','ridge_xminus_wmo']:
            expected=manifest['cases'][case]['expected'];check_generation(case,expected)
            expected['output']['paths']=[]
            with self.assertRaises(AssertionError):check_generation(case,expected)

    def test_original_scalar_nearest_query_replay(self):
        base=HERE/'fixtures/scalar-nearest';manifest=json.loads((base/'manifest.json').read_text(encoding="utf-8"))
        self.assertEqual(set(manifest['cases']),{'ridge_wmo','ridge_neighbors_wmo','ridge_xminus_wmo'})
        for case,provenance in manifest['cases'].items():
            path=base/(case+'.json');self.assertEqual(sha(path.read_bytes()),provenance['expected_sha256'])
            original=HERE/'fixtures'/case
            for name,digest in provenance['original_files_sha256'].items():self.assertEqual(sha((original/name).read_bytes()),digest)
            measured=json.loads(path.read_text(encoding="utf-8"));self.assertEqual(measured['extents_bits'],['40a00000']*3)
            inputs=json.loads((original/'expected.json').read_text(encoding="utf-8"))['output']['queries']
            self.assertEqual(len(measured['queries']),25)
            for row in measured['queries']:
                self.assertEqual(row['point_bits'],inputs[row['source_query_index']]['nearest_bits'])
                self.assertEqual(row['status'],0x40000000);self.assertTrue(row['poly_ref'])
                self.assertEqual(row['input_height_status'],0x40000000)
                self.assertEqual(row['nearest_height_status'],0x40000000)

    def test_original_config_key_collision_and_narrowing(self):
        records=json.loads((HERE/'fixtures/configs/authored-config-expected.json').read_text(encoding="utf-8"))
        pair=[r for r in records if r['map_id']==1]
        self.assertEqual(len(pair),2);self.assertEqual(pair[0]['merged_json'],pair[1]['merged_json'])
        for row in pair:
            self.assertEqual(row['walkable_climb'],7);self.assertEqual(row['walkable_radius'],3)
            self.assertEqual(row['min_region_area_squared'],81)
            self.assertNotIn('ignored_object',row['merged_json'])

if __name__=='__main__':unittest.main()
