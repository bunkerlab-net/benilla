#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate independent fixture integrity; generation verification is --check."""
import json
from pathlib import Path
import struct
import unittest
import zlib
from run_oracle import mesh_info,sha
HERE=Path(__file__).resolve().parent

class SourceOracleFixtures(unittest.TestCase):
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
        for case in ['ridge_wmo','ridge_neighbors_wmo']:
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
        for case,count in [('ridge_wmo',1),('ridge_neighbors_wmo',5)]:
            expected=manifest['cases'][case]['expected'];events=expected['terrain_file_events']
            self.assertEqual([e['basename'] for e in events],['0003130.map','0003131.map','0003129.map','0003230.map','0003030.map'])
            self.assertEqual(sum(e['opened'] for e in events),count)
            self.assertEqual(expected['output']['max_tiles'],count)
            self.assertEqual(len(expected['tile_union']),count)
        self.assertEqual(len(manifest['cases']['flat']['expected']['terrain_file_events']),1)

    def test_original_scalar_nearest_query_replay(self):
        base=HERE/'fixtures/scalar-nearest';manifest=json.loads((base/'manifest.json').read_text(encoding="utf-8"))
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
