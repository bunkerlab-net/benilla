#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent structural checks for exact-source authored dry WMO fixtures."""
import hashlib
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import generate_wmo_vmap_fixture as generator

FIXTURES = generator.HERE / 'fixtures/wmo_vmap_source'


def chunks(data):
    result = {}
    at = 0
    while at < len(data):
        name = data[at:at+4][::-1]
        size, = struct.unpack_from('<I', data, at+4)
        payload = data[at+8:at+8+size]
        if len(payload) != size or name in result:
            raise ValueError('Invalid authored chunk')
        result[name] = payload
        at += 8+size
    if at != len(data):
        raise ValueError('Trailing authored bytes')
    return result


def source_group(index):
    outer = chunks((FIXTURES / f'authored-wmo_{index:03}.group.test-fixture').read_bytes())
    if outer[b'MVER'] != struct.pack('<I', 17):
        raise ValueError('Authored WMO version')
    header = outer[b'MOGP'][:68]
    return header, chunks(outer[b'MOGP'][68:])


def raw_groups():
    data = (FIXTURES / 'authored-wmo.raw.vmap').read_bytes()
    triangle_count, group_count, root_id = struct.unpack_from('<3I', data, 8)
    at = 20
    groups = []
    for _ in range(group_count):
        flags, group_id = struct.unpack_from('<2I', data, at)
        bound = data[at+8:at+32]
        liquid, = struct.unpack_from('<I', data, at+32)
        at += 36
        payload = {}
        for expected in [b'GRP ', b'INDX', b'VERT']:
            name = data[at:at+4]
            if name != expected:
                raise ValueError(f'Wrong raw chunk: {name}')
            size, = struct.unpack_from('<I', data, at+4)
            payload[name] = data[at+8:at+8+size]
            at += 8+size
        groups.append((flags, group_id, bound, liquid, payload))
    if at != len(data):
        raise ValueError('Trailing raw bytes')
    return data, triangle_count, root_id, groups


class AuthoredWmoFixtures(unittest.TestCase):
    def test_no_doodad_root_and_every_declared_group_are_authored(self):
        root = chunks((FIXTURES / 'authored-wmo.root.test-fixture').read_bytes())
        self.assertEqual(root[b'MVER'], struct.pack('<I', 17))
        self.assertEqual(len(root[b'MOHD']), 64)
        self.assertEqual(struct.unpack_from('<I', root[b'MOHD'], 4)[0], 6)
        self.assertEqual(struct.unpack_from('<3I', root[b'MOHD'], 16), (0, 0, 0))
        self.assertTrue({b'MODN', b'MODS', b'MODD'}.isdisjoint(root))
        self.assertEqual(root[b'MOGN'], b'floor\0upper\0empty\0antiportal\0')
        for index in range(6):
            header, group = source_group(index)
            self.assertEqual(len(header), 68)
            self.assertEqual(set(group), {b'MOPY', b'MOVI', b'MOVT', b'MOBA'})
            self.assertEqual(struct.unpack_from('<I', header, 56)[0], 701+index)
        self.assertEqual(struct.unpack_from('<I', source_group(3)[0], 8)[0], 0x80)
        self.assertEqual(struct.unpack_from('<I', source_group(4)[0], 8)[0], 0x4000000)
        self.assertEqual(struct.unpack_from('<I', source_group(5)[0], 0)[0], 18)

    def test_source_root_counts_and_ids_preserve_empty_retained_group(self):
        data, triangles, root_id, groups = raw_groups()
        self.assertEqual(data[:8], b'VMAPs05\0')
        self.assertEqual((triangles, len(groups), root_id), (192, 3, 4242))
        self.assertEqual([group[1] for group in groups], [701, 702, 703])
        self.assertEqual(len(data), 7472)
        self.assertEqual([struct.unpack_from('<I', group[4][b'INDX'])[0]//3 for group in groups], [160, 32, 0])
        self.assertEqual([struct.unpack_from('<I', group[4][b'VERT'])[0] for group in groups], [480, 25, 0])
        for index, (flags, _, bounds, liquid, _) in enumerate(groups):
            header, _ = source_group(index)
            self.assertEqual(flags, struct.unpack_from('<I', header, 8)[0])
            self.assertEqual(bounds, header[12:36])
            self.assertNotEqual(bounds, bytes(24))
            self.assertEqual(liquid, 0)

    def test_every_small_mask_and_original_ascending_vertex_remap(self):
        _, _, _, groups = raw_groups()
        all_masks = source_group(0)[1][b'MOPY'][::2]
        self.assertEqual(all_masks, bytes(range(256)))
        for index, group in enumerate(groups):
            _, chunks_ = source_group(index)
            source_indices = struct.unpack('<'+'H'*(len(chunks_[b'MOVI'])//2), chunks_[b'MOVI'])
            source_vertices = [chunks_[b'MOVT'][at:at+12] for at in range(0,len(chunks_[b'MOVT']),12)]
            selected = [mask_index for mask_index, mask in enumerate(chunks_[b'MOPY'][::2])
                        if mask & 8 or (mask & 32 and not mask & 4)]
            original = [value for triangle in selected for value in source_indices[3*triangle:3*triangle+3]]
            used = sorted(set(original))
            remap = {value: offset for offset, value in enumerate(used)}
            expected_indices = [remap[value] for value in original]
            expected = struct.pack('<I', len(original)) + struct.pack('<'+'H'*len(original), *expected_indices)
            self.assertEqual(group[4][b'INDX'], expected)
            self.assertEqual(group[4][b'VERT'], struct.pack('<I', len(used))+b''.join(source_vertices[value] for value in used))
            if index == 0:
                self.assertEqual(expected_indices[:3], [2,1,0])
                self.assertNotEqual(used, list(dict.fromkeys(original)))
                self.assertNotIn(0x24, selected)
                self.assertIn(0x0c, selected)
                self.assertEqual(len(selected),160)

    def test_grp_preserves_moba_word_eight_instead_of_selected_count(self):
        _, _, _, groups = raw_groups()
        for index, expected in enumerate([(12,345,65535),(96,17),(3,)]):
            moba = source_group(index)[1][b'MOBA']
            words = tuple(struct.unpack_from('<H', moba, at+16)[0] for at in range(0,len(moba),24))
            self.assertEqual(words, expected)
            self.assertEqual(groups[index][4][b'GRP '], struct.pack('<I', len(words))+struct.pack('<'+'I'*len(words),*words))

    def test_original_assembly_serializes_ids_and_nonzero_bounds(self):
        serialized = (FIXTURES/'products/Authored.wmo.vmo').read_bytes()
        self.assertEqual(serialized[:8],b'VMAP_7.0')
        self.assertEqual(serialized[20:24],b'GMOD')
        self.assertEqual(struct.unpack_from('<I',serialized,24)[0],3)
        self.assertEqual(serialized[28:52],source_group(0)[0][12:36])
        self.assertIn(b'GBIH',serialized)
        self.assertIn(b'MBIH',serialized)
        tile = (FIXTURES/'products/000_32_32.vmtile').read_bytes()
        self.assertEqual(struct.unpack_from('<I',tile,8)[0],2)
        at=12
        for expected_id, expected_adt in [(201,13),(202,29)]:
            self.assertEqual(struct.unpack_from('<IHI',tile,at),(4,expected_adt,expected_id))
            size,=struct.unpack_from('<I',tile,at+62)
            self.assertEqual(tile[at+66:at+66+size],b'Authored.wmo')
            at+=70+size
        self.assertEqual(at,len(tile))

    def test_query_corpus_has_hits_misses_skip_masks_and_rotated_placements(self):
        rows={name:[line.split('\t') for line in (FIXTURES/(name+'.tsv')).read_text().splitlines()
                    if not line.startswith('#')] for name in ['rays','heights','locations']}
        self.assertEqual([len(rows[name]) for name in rows],[2208,540,540])
        self.assertEqual({len(row) for row in rows['rays']},{12})
        self.assertEqual({len(row) for row in rows['locations']},{14})
        self.assertEqual(sum(row[10]=='1' for row in rows['rays']),1350)
        self.assertEqual(sum(row[5]!='7f800000' for row in rows['heights']),171)
        self.assertEqual(sum(row[7]=='1' for row in rows['locations']),170)
        self.assertEqual({row[9] for row in rows['locations'] if row[7]=='1'},{'201','202'})
        self.assertEqual({row[10] for row in rows['locations'] if row[7]=='1'},{'4242'})
        self.assertEqual({row[11] for row in rows['locations'] if row[7]=='1'},{'701','702'})
        for left,right in zip(rows['rays'][::2],rows['rays'][1::2]):
            self.assertEqual(left[:9]+left[10:],right[:9]+right[10:])
            self.assertEqual((left[9],right[9]),('0','1'))

    def test_changed_extractor_source_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=pathlib.Path(temporary); name='guarded.cpp'; original=b'original source\n'
            (root/'wmo_vmap_source_hashes.sha256').write_text(f'{hashlib.sha256(original).hexdigest()}  {name}\n')
            (root/name).write_bytes(original)
            with mock.patch.object(generator,'HERE',root), mock.patch.object(generator,'guarded_sources',return_value={}):
                self.assertEqual(generator.sources(root)[name],original.decode())
                (root/name).write_bytes(original+b'changed\n')
                with self.assertRaisesRegex(SystemExit,'Wrong pinned source hash'):
                    generator.sources(root)

    def test_hash_mismatch_prevents_compilation(self):
        with mock.patch.object(sys,'argv',['generator','/unused']), \
             mock.patch.object(generator,'sources',side_effect=SystemExit('Wrong pinned source hash')), \
             mock.patch.object(generator.subprocess,'run') as compiler:
            with self.assertRaisesRegex(SystemExit,'Wrong pinned source hash'):
                generator.main()
            compiler.assert_not_called()


@unittest.skipUnless(os.environ.get('CMANGOS_SOURCE'),'set CMANGOS_SOURCE for original-source rebuild checks')
class PinnedSourceIntegration(unittest.TestCase):
    def test_extracted_methods_are_verbatim_pinned_source(self):
        root=pathlib.Path(os.environ['CMANGOS_SOURCE']).resolve()
        source=generator.sources(root)
        extracted,collision=generator.fragments(source)
        self.assertEqual((FIXTURES/'source-fragments.cpp.test-fixture').read_bytes(),(extracted+collision).encode())
        for signature in ['WMORoot::WMORoot(', 'bool WMORoot::open(',
                          'bool WMORoot::ConvertToVMAPRootWmo(', 'WMOGroup::WMOGroup(',
                          'bool WMOGroup::open(', 'int WMOGroup::ConvertToVMAPGroupWmo(',
                          'bool WMOGroup::ShouldSkip(']:
            self.assertIn(generator.method(source[generator.EXTRACTOR+'wmo.cpp'],signature),extracted)
        self.assertIn(generator.method(source[generator.EXTRACTOR+'vmapexport.cpp'],'bool ExtractSingleWmo('),extracted)
        captured = generator.captured_fragments(source)
        self.assertIn(generator.method(source[generator.EXTRACTOR+'vmapexport.cpp'], 'uint32 GenerateUniqueObjectId('), captured)
        self.assertIn(generator.method(source[generator.EXTRACTOR+'wmo.cpp'], 'WMOInstance::WMOInstance('), captured)
        self.assertEqual((generator.CAPTURED/'source-fragments.cpp.test-fixture').read_bytes(),
                         (extracted+captured+collision).encode())

    def test_every_generated_fixture_matches_without_replacement(self):
        before={path.relative_to(FIXTURES):path.read_bytes() for path in FIXTURES.rglob('*') if path.is_file()}
        captured_before={path.relative_to(generator.CAPTURED):path.read_bytes() for path in generator.CAPTURED.rglob('*') if path.is_file()}
        result=subprocess.run([sys.executable,str(generator.HERE/'generate_wmo_vmap_fixture.py'),
                               os.environ['CMANGOS_SOURCE'],'--check'],check=True,text=True,capture_output=True)
        self.assertIn('Verified 15 authored WMO oracle files',result.stdout)
        self.assertIn('Could not open all Group file for: Authored.wmo',result.stdout)
        after={path.relative_to(FIXTURES):path.read_bytes() for path in FIXTURES.rglob('*') if path.is_file()}
        self.assertEqual(before,after)
        captured_after={path.relative_to(generator.CAPTURED):path.read_bytes() for path in generator.CAPTURED.rglob('*') if path.is_file()}
        self.assertEqual(captured_before,captured_after)



class CapturedWmoFixtures(unittest.TestCase):
    def read(self, name):
        return (generator.HERE / 'fixtures/captured_wmo_source' / name).read_bytes()

    def test_capture_has_real_triangle_count_and_natural_source_identity(self):
        data = self.read('hull.raw.vmap')
        self.assertEqual(data[:8], b'VMAPs05\0')
        self.assertEqual(struct.unpack_from('<3I', data, 8), (1, 1, 5875))
        self.assertEqual(struct.unpack_from('<2I', data, 20), (0x2000, 23))
        self.assertEqual(struct.unpack_from('<6f', data, 28), (-1, -2, -3, 4, 5, 6))
        self.assertEqual(struct.unpack_from('<I', data, 52)[0], 0)
        at = 56
        payloads = {}
        for name in [b'GRP ', b'INDX', b'VERT']:
            self.assertEqual(data[at:at+4], name)
            size, = struct.unpack_from('<I', data, at+4)
            payloads[name] = data[at+8:at+8+size]
            self.assertEqual(len(payloads[name]), size)
            at += 8 + size
        self.assertEqual(at, len(data))
        self.assertEqual(payloads[b'INDX'], struct.pack('<I3H', 3, 0, 1, 2))
        self.assertEqual(payloads[b'VERT'], struct.pack('<I9f', 3, -1, -2, -3, 4, 0, 6, 0, 5, 1))

    def test_source_modf_conversion_allocates_the_first_generated_identity(self):
        modf = self.read('hull.modf.test-fixture')
        self.assertEqual(len(modf), 64)
        self.assertEqual(struct.unpack_from('<2I', modf), (0, 0x89abcdef))
        self.assertEqual(struct.unpack_from('<3f', modf, 8), (16100, 4, 16600))
        self.assertEqual(struct.unpack_from('<3f', modf, 20), (0, 0, 0))
        self.assertEqual(struct.unpack_from('<6f', modf, 32), (16098, 1, 16599, 16105, 10, 16604))
        self.assertEqual(struct.unpack_from('<4H', modf, 56), (0, 0, 11, 1024))
        record = self.read('dir_bin')
        self.assertEqual(struct.unpack_from('<4I', record), (0, 30, 31, 4))
        self.assertEqual(struct.unpack_from('<HI', record, 16), (11, 1))
        self.assertEqual(struct.unpack_from('<3f', record, 22), (16600, 16100, 4))
        self.assertEqual(struct.unpack_from('<3f', record, 34), (0, 0, 0))
        self.assertEqual(struct.unpack_from('<f', record, 46), (1,))
        self.assertEqual(struct.unpack_from('<6f', record, 50), (16599, 16098, 1, 16604, 16105, 10))
        self.assertEqual(struct.unpack_from('<I', record, 74)[0], 8)
        self.assertEqual(record[78:], b'Hull.wmo')


if __name__ == '__main__':
    unittest.main()
