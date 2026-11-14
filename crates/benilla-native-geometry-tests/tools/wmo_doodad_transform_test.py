#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Structural, guard, and reproducibility checks for the authored source oracle."""
import csv
import hashlib
import math
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import wmo_doodad_transform_generate as generator

FIXTURES = generator.TARGET


def table(name):
    text = (FIXTURES / name).read_text()
    rows = list(csv.DictReader(text.removeprefix('# ').splitlines(), delimiter='\t'))
    if any(None in row for row in rows):
        raise ValueError('Unexpected table column count')
    return {row['case']: row for row in rows}


def floating(value):
    return struct.unpack('<f', bytes.fromhex(value)[::-1])[0]


def float_bytes(row, fields):
    return b''.join(struct.pack('<I', int(row[name], 16)) for name in fields)


class AuthoredDoodadTransform(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.inputs = table('cases.tsv')
        cls.results = table('results.tsv')
        cls.matrices = table('matrices.tsv')

    def test_complete_unique_case_set_and_finite_authored_inputs(self):
        self.assertEqual(len(self.inputs), 85)
        self.assertEqual(self.inputs.keys(), self.results.keys())
        self.assertEqual(self.inputs.keys(), self.matrices.keys())
        fields = ['parent_px','parent_py','parent_pz','parent_rx','parent_ry','parent_rz',
                  'child_px','child_py','child_pz','quat_x','quat_y','quat_z','quat_w','child_scale']
        for row in self.inputs.values():
            self.assertTrue(all(math.isfinite(floating(row[key])) for key in fields))
        self.assertEqual(sum(int(row['child_emitted']) for row in self.results.values()), 79)
        self.assertEqual({name for name, row in self.results.items()
                          if row['child_emitted']=='1' and row['finite_transform']=='0'},
                         {'zero_quaternion','signed_zero_quaternion','underflow_quaternion','max_finite_parent_rotation'})

    def test_authored_bytes_encode_exact_input_bits(self):
        for name, row in self.inputs.items():
            folder=FIXTURES/name
            modf=(folder/'input.modf').read_bytes()
            modd=(folder/'input.modd').read_bytes()
            self.assertEqual(len(modf),64)
            self.assertEqual(len(modd),40)
            self.assertEqual(struct.unpack_from('<II',modf), (3,int(row['client_id'])))
            self.assertEqual(modf[8:32], float_bytes(row,['parent_px','parent_py','parent_pz','parent_rx','parent_ry','parent_rz']))
            self.assertEqual(struct.unpack_from('<4H',modf,56),
                             (int(row['parent_flags']),int(row['selected_set']),29,int(row['parent_scale'])))
            self.assertEqual(struct.unpack_from('<I',modd)[0],7)
            self.assertEqual(modd[4:36],float_bytes(row,['child_px','child_py','child_pz','quat_x','quat_y','quat_z','quat_w','child_scale']))
            self.assertEqual((folder/'input.modn').read_bytes(),b'unused\0'+bytes.fromhex(row['path_hex'])+b'\0')

    def test_exact_child_stream_metadata_and_output_bits(self):
        for name, row in self.results.items():
            data=(FIXTURES/name/'child.dir').read_bytes()
            if row['child_emitted']=='0':
                self.assertEqual(data,b'')
                continue
            source=self.inputs[name]
            self.assertEqual(struct.unpack_from('<4IHI',data),
                             (int(source['map']),int(source['tile_x']),int(source['tile_y']),
                              int(row['flags']),0,int(row['isolated_child_id'])))
            self.assertEqual(data[22:50],float_bytes(row,['px','py','pz','rx','ry','rz','scale']))
            nlen,=struct.unpack_from('<I',data,50)
            self.assertEqual(nlen,int(row['name_length']))
            self.assertEqual(data[54:],bytes.fromhex(row['name_hex']))
            self.assertEqual(len(data),54+nlen)
            self.assertEqual(len(data),int(row['record_length']))

    def test_early_return_preserves_unfixed_parent_state(self):
        for name, row in self.results.items():
            parent=(FIXTURES/name/'parent.dir').read_bytes()
            source=self.inputs[name]
            expected_parent=(int(source['parent_flags']) & 1)==0 and source['parent_vertices']!='0' and source['parent_exists']=='1'
            self.assertEqual(bool(parent),expected_parent)
            self.assertEqual(row['parent_emitted'],str(int(bool(parent))))
            if not expected_parent:
                self.assertEqual([row[k] for k in ['parent_px','parent_py','parent_pz']],
                                 [self.inputs[name][k] for k in ['parent_px','parent_py','parent_pz']])
                self.assertEqual(row['isolated_child_id'],'1')
            elif row['child_emitted']=='1':
                self.assertEqual(row['isolated_child_id'],'2')
            if parent:
                self.assertEqual(parent[22:34],float_bytes(row,['parent_px','parent_py','parent_pz']))
        zero=self.results['zero_origin']
        self.assertEqual([floating(zero[k]) for k in ['parent_px','parent_py','parent_pz']],
                         [floating('46855555'),floating('46855555'),27])
        identity=self.results['identity_mdx']
        self.assertEqual([floating(identity[k]) for k in ['parent_px','parent_py','parent_pz']], [30,10,20])
        self.assertEqual([floating(identity[k]) for k in ['px','py','pz']], [31,12,23])

    def test_mdx_and_mdl_keep_pre_rewrite_name_length_and_trailing_nul(self):
        for name in ['identity_mdx','identity_mdl']:
            row=self.results[name]
            self.assertEqual(int(row['name_length']),17)
            self.assertEqual(bytes.fromhex(row['name_hex']),b'Authored_Name.m2\0')
        row=self.results['identity_m2']
        self.assertEqual(int(row['name_length']),16)
        self.assertEqual(bytes.fromhex(row['name_hex']),b'Authored_Name.m2')

    def test_portable_acquisition_names_match_source_spelling(self):
        for suffix in ['mdx','mdl','m2']:
            source=self.inputs['portable_'+suffix]
            result=self.results['portable_'+suffix]
            self.assertEqual(bytes.fromhex(source['path_hex']),('Models\\Solid.'+suffix).encode())
            self.assertEqual(source['raw_basename'],'Solid.m2')
            self.assertEqual(bytes.fromhex(result['name_hex']),b'Solid.m2'+(b'\0' if suffix!='m2' else b''))

    def test_scale_is_unmodified_and_not_applied_to_local_position(self):
        fields=['px','py','pz','rx','ry','rz']
        expected=[self.results['identity_mdx'][k] for k in fields]
        for name, source in self.inputs.items():
            if name.startswith('scale_') or name=='ignored_parent_scale':
                result=self.results[name]
                self.assertEqual(result['scale'],source['child_scale'])
                self.assertEqual([result[k] for k in fields],expected)

    def test_worldspawn_needs_both_sentinel_coordinates(self):
        self.assertEqual(self.results['worldspawn']['flags'],'3')
        self.assertEqual(self.results['only_worldspawn_x']['flags'],'1')
        self.assertEqual(self.results['only_worldspawn_y']['flags'],'1')

    def test_original_quaternion_normalization_and_degenerate_results(self):
        fields=['rx','ry','rz']
        reference=[self.results['unit_quaternion'][key] for key in fields]
        self.assertEqual([self.results['negated_quaternion'][key] for key in fields],reference)
        self.assertEqual([self.results['large_finite_norm'][key] for key in fields],reference)
        self.assertEqual(self.results['tiny_nonzero_norm']['finite_transform'],'1')
        for name in ['zero_quaternion','signed_zero_quaternion','underflow_quaternion']:
            row=self.results[name]
            self.assertEqual(row['invalid'],'1')
            self.assertEqual(row['divide_by_zero'],'1')
            self.assertTrue(math.isnan(floating(row['rz'])))
        overflow=self.results['overflow_norm_finite_input']
        self.assertEqual(overflow['overflow'],'1')
        self.assertEqual(overflow['finite_transform'],'1')
        self.assertEqual([overflow[key] for key in fields], [self.results['identity_mdx'][key] for key in fields])

    def test_actual_gimbal_branches_and_rounding_neighbors(self):
        for suffix,expected in [('positive','3f800000'),('negative','bf800000')]:
            row=self.matrices['parent_gimbal_'+suffix]
            self.assertEqual(row['composed_m02'],expected)
            self.assertEqual(row['euler_unique'],'0')
            self.assertEqual(self.matrices['parent_gimbal_inside_'+suffix]['euler_unique'],'1')
            self.assertEqual(self.matrices['parent_gimbal_outside_'+suffix]['euler_unique'],'1')
            nonzero=self.matrices['gimbal_'+suffix+'_nonzero_x']
            self.assertEqual(nonzero['composed_m02'],expected)
            self.assertEqual(nonzero['euler_unique'],'0')
        self.assertEqual(self.matrices['math_gimbal_signed_zero']['composed_m10'],'80000000')
        self.assertEqual([self.results['math_gimbal_signed_zero'][key] for key in ['rx','ry','rz']],
                         ['42b40000','00000000','c3340000'])
        self.assertEqual(self.matrices['gimbal_positive_exact']['composed_m02'],'3f7fffff')
        self.assertEqual(self.matrices['gimbal_negative_exact']['composed_m02'],'bf7fffff')

    def test_guard_rejects_source_change_before_any_compilation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=pathlib.Path(temporary)
            content=b'guarded source\n'
            (root/'guarded.cpp').write_bytes(content)
            (root/'wmo_doodad_transform_source_hashes.sha256').write_text(hashlib.sha256(content).hexdigest()+'  guarded.cpp\n')
            with mock.patch.object(generator,'HERE',root):
                self.assertEqual(generator.sources(root),{'guarded.cpp':content.decode()})
                (root/'guarded.cpp').write_bytes(content+b'changed')
                with self.assertRaisesRegex(SystemExit,'Wrong pinned source hash'):
                    generator.sources(root)
        with mock.patch.object(sys,'argv',['generator','/unused']), \
             mock.patch.object(generator,'sources',side_effect=SystemExit('Wrong pinned source hash')), \
             mock.patch.object(generator.subprocess,'run') as compiler:
            with self.assertRaisesRegex(SystemExit,'Wrong pinned source hash'):
                generator.main()
            compiler.assert_not_called()


@unittest.skipUnless(os.environ.get('CMANGOS_SOURCE'),'set CMANGOS_SOURCE for original-source integration checks')
class PinnedSourceIntegration(unittest.TestCase):
    def test_extracted_methods_are_verbatim_guarded_source(self):
        source=generator.sources(pathlib.Path(os.environ['CMANGOS_SOURCE']).resolve())
        fragments=generator.fragments(source)
        self.assertEqual((FIXTURES/'source-fragments.cpp.test-fixture').read_bytes(),fragments.encode())
        for filename,signature in [('model.cpp','void Doodad::ExtractSet('),
                                   ('wmo.cpp','WMOInstance::WMOInstance('),
                                   ('vmapexport.cpp','uint32 GenerateUniqueObjectId(')]:
            self.assertIn(generator.method(source[generator.EXTRACTOR+filename],signature),fragments)

    def test_sanitized_rebuild_matches_every_byte_without_replacement(self):
        before={path.relative_to(FIXTURES):path.read_bytes() for path in FIXTURES.rglob('*') if path.is_file()}
        result=subprocess.run([sys.executable,str(generator.HERE/'wmo_doodad_transform_generate.py'),
                               os.environ['CMANGOS_SOURCE'],'--check','--sanitize'],
                              check=True,text=True,capture_output=True)
        self.assertIn('Verified 430 authored one-candidate WMO doodad oracle files',result.stdout)
        self.assertEqual(before,{path.relative_to(FIXTURES):path.read_bytes() for path in FIXTURES.rglob('*') if path.is_file()})


if __name__=='__main__':
    unittest.main()
