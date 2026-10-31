#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent structural/provenance checks for the authored liquid source oracle."""
import hashlib
import os
import pathlib
import struct
import tempfile
from unittest import mock
import unittest

import wmo_liquid_generate as generator

ROOT = generator.HERE / 'fixtures/wmo_liquid_source'


def rows(name):
    return [line.split('\t') for line in (ROOT / name).read_text().splitlines()
            if line and not line.startswith('#')]


def chunks(data):
    result = {}
    at = 0
    while at < len(data):
        name = data[at:at+4][::-1]
        size, = struct.unpack_from('<I', data, at+4)
        end = at+8+size
        if end > len(data) or name in result:
            raise ValueError('invalid authored chunk')
        result[name] = data[at+8:end]
        at = end
    if at != len(data):
        raise ValueError('trailing authored data')
    return result


def raw_groups(data):
    triangles, count, root = struct.unpack_from('<3I', data, 8)
    at, groups = 20, []
    for _ in range(count):
        flags, group = struct.unpack_from('<2I', data, at)
        bounds = data[at+8:at+32]
        liquid, = struct.unpack_from('<I', data, at+32)
        at += 36
        payloads = {}
        for tag in [b'GRP ', b'INDX', b'VERT']:
            if data[at:at+4] != tag:
                raise ValueError('wrong raw chunk')
            size, = struct.unpack_from('<I', data, at+4)
            payloads[tag] = data[at+8:at+8+size]
            at += 8+size
        liquid_payload, declared = None, 0
        if liquid:
            if data[at:at+4] != b'LIQU':
                raise ValueError('missing raw liquid')
            declared, = struct.unpack_from('<I', data, at+4)
            x, y, w, h = struct.unpack_from('<4i', data, at+8)
            actual = 32+x*y*4+w*h
            liquid_payload = data[at+8:at+8+actual]
            if len(liquid_payload) != actual:
                raise ValueError('short raw liquid')
            at += 8+actual
        groups.append((flags, group, bounds, liquid, payloads, declared, liquid_payload))
    if at != len(data):
        raise ValueError('trailing raw bytes; advertised size must not drive cursor')
    return triangles, root, groups


class WmoLiquidFixtureTests(unittest.TestCase):
    def test_compiled_both_original_header_layouts_have_only_two_tail_bytes(self):
        layout = rows('layout.tsv')
        self.assertEqual([r[0] for r in layout], ['extractor', 'assembler'])
        for row in layout:
            self.assertEqual(list(map(int, row[1:])), [32, 4, 0, 4, 8, 12, 16, 20, 24, 28, 2])
        self.assertEqual(rows('padding.tsv'), [[generator.POLICY, '30', '0000', 'a55a', '1', '108', '72']])

    def test_policy_patch_initializes_only_undefined_tail(self):
        patch = (ROOT / 'padding-v1.patch.test-fixture').read_text()
        added = [line[1:] for line in patch.splitlines() if line.startswith('+') and not line.startswith('+++')]
        removed = [line for line in patch.splitlines() if line.startswith('-') and not line.startswith('---')]
        self.assertEqual(removed, [])
        self.assertEqual(added, generator.INITIALIZED.splitlines()[1:])
        metadata = dict(rows('provenance.tsv'))
        self.assertEqual(metadata['pin'], generator.PIN)
        self.assertEqual(metadata['policy'], generator.POLICY)
        self.assertEqual(metadata['padding_patch'], hashlib.sha256(patch.encode()).hexdigest())
        self.assertEqual(metadata['original_wmo.cpp'], 'fb2a22dec0ea146fd0e3712b501b8388af76a1c47e5dddd314b6d0c3a2b1ad21')
        source = (ROOT / 'source-fragments.cpp.test-fixture').read_text()
        self.assertIn(generator.INITIALIZED + '            f.read(hlq, 0x1E);', source)
        self.assertIn('fwrite(hlq, sizeof(WMOLiquidHeader), 1, output);', source)

    def test_authored_inputs_preserve_dimensions_heights_tiles_and_count_order(self):
        manifest = rows('cases.tsv')
        self.assertEqual(len(manifest), 20)
        for index, row in enumerate(manifest):
            self.assertEqual(len(row), 17)
            name, _, root_flags, group_flags, liquid_type, hole, trailing, upper = row[:8]
            root = chunks((ROOT / row[11]).read_bytes())
            self.assertEqual(root[b'MVER'], struct.pack('<I', 17))
            self.assertFalse(set(root) & {b'MODN', b'MODS', b'MODD'})
            self.assertEqual(struct.unpack_from('<I', root[b'MOHD'], 60)[0], int(root_flags))
            group_paths = row[12].split(';')
            self.assertEqual(len(group_paths), int(row[14]))
            triangles, root_id, groups = raw_groups((ROOT / row[13]).read_bytes())
            self.assertEqual((triangles, root_id, len(groups)), (int(row[15]), 8100+index, int(row[14])))
            self.assertEqual(sum(g[3] for g in groups), int(row[16]))
            for group_index, group_path in enumerate(group_paths):
                source = chunks((ROOT / group_path).read_bytes())
                self.assertEqual(source[b'MVER'], struct.pack('<I', 17))
                header, body = source[b'MOGP'][:68], chunks(source[b'MOGP'][68:])
                self.assertEqual(struct.unpack_from('<I', header, 8)[0], int(group_flags))
                self.assertEqual(struct.unpack_from('<I', header, 52)[0], int(liquid_type))
                actual = groups[group_index]
                self.assertEqual(actual[:3], (int(group_flags), 8200+index*2+group_index, header[12:36]))
                self.assertEqual(actual[4][b'GRP '], struct.pack('<2I', 1, 37))
                if group_index:
                    self.assertNotIn(b'MLIQ', body)
                    self.assertEqual(actual[3], 0)
                    continue
                liquid = body[b'MLIQ']
                self.assertEqual(len(liquid), 106)
                self.assertEqual(struct.unpack_from('<4i', liquid), (3, 3, 2, 2))
                self.assertEqual(struct.unpack_from('<H', liquid, 28)[0], 0x6abc)
                self.assertEqual(liquid[-4:], bytes([255]*4) if hole == '1' else bytes([0x0f, 0x12, 0xa1, 0x83]))
                if trailing == '1':
                    self.assertEqual(list(body)[-2:], [b'MLIQ', b'MOTV'])
                    self.assertEqual(actual[3], 0)
                    self.assertIsNone(actual[6])
                else:
                    self.assertEqual(list(body)[-1], b'MLIQ')
                    payload = actual[6]
                    self.assertEqual((actual[5], len(payload)), (108, 72))
                    self.assertEqual(payload[:28], liquid[:28])
                    self.assertEqual(payload[28:30], struct.pack('<H', int(row[9])))
                    self.assertEqual(payload[30:32], b'\0\0')
                    self.assertEqual(payload[32:68], b''.join(liquid[34+i*8:38+i*8] for i in range(9)))
                    self.assertEqual(payload[-4:], liquid[-4:])

    def test_original_type_precedence_and_signed_narrowing(self):
        types = {r[0]: (int(r[9]), int(r[10])) for r in rows('cases.tsv')}
        self.assertEqual(types, {
            'Water': (1, 1), 'Oceanbit': (2, 2), 'Ocean': (2, 2), 'Magma': (3, 3),
            'Slime': (4, 4), 'Naxx': (21, 21), 'Case': (4, 4), 'Known': (41, 41),
            'Signed': (32768, 4294934528), 'Wrapped': (1, 1), 'Fallback': (3, 3),
            'Legacy': (3, 3), 'Oldwater': (1, 1), 'Oldslime': (4, 4), 'Empty': (0, 0),
            'Reset': (0, 0), 'Unknown': (22, 22), 'Negative': (65535, 4294967295),
            'Zero': (0, 0), 'Overflow': (3, 3)})

    def test_original_query_corpus_covers_masks_floor_holes_and_transform(self):
        corpus = rows('liquid.tsv')
        self.assertEqual(len(corpus), 2205)
        self.assertTrue(all(len(r) == 23 and r[0] == 'wmo_liquid' for r in corpus))
        self.assertEqual(sum(int(r[8]) for r in corpus), 1904)
        self.assertEqual(sum(int(r[16]) for r in corpus), 342)
        self.assertEqual({int(r[15]) for r in corpus}, {0, 1, 2, 4, 8, 15, 16})
        self.assertEqual(len({r[1].split('_')[0] for r in corpus}), 21)
        indexed = {r[1]: r for r in corpus}
        self.assertEqual(indexed['Water_wet_a_mask0'][16], '1')
        self.assertEqual(indexed['Water_wet_a_mask1'][16], '0')
        self.assertEqual(indexed['Water_wet_a_mask1'][19], '1')
        self.assertEqual(indexed['Water_dry_upper_mask0'][12], '8201')
        self.assertEqual(indexed['Water_dry_upper_mask0'][19], '0')
        for case in ['Water_disabled_mask0', 'Water_outside_liquid_x_mask0']:
            self.assertEqual((indexed[case][8], indexed[case][16]), ('1', '0'))
        self.assertEqual(indexed['Water_floorless_mask0'][8], '0')
        self.assertTrue(any(r[1].startswith('Rotated_') and r[16] == '1' for r in corpus))
        self.assertEqual(len(rows('direct.tsv')), 64)
        self.assertEqual(sum(int(r[5]) for r in rows('direct.tsv')), 48)

    def test_product_set_is_closed_and_raw_counts_are_complete(self):
        cases = rows('cases.tsv')
        self.assertEqual({p.name for p in (ROOT / 'products').iterdir()},
                         {r[8] + '.vmo' for r in cases} | {'000.vmtree', '000_32_32.vmtile'})
        self.assertEqual({p.name for p in (ROOT / 'raw').iterdir()}, {r[0] + '.raw.vmap' for r in cases})
        tile = (ROOT / 'products/000_32_32.vmtile').read_bytes()
        self.assertEqual(tile[:8], b'VMAP_7.0')
        self.assertEqual(struct.unpack_from('<I', tile, 8)[0], 21)
        for row in cases:
            self.assertEqual((ROOT / 'products' / (row[8]+'.vmo')).read_bytes()[:8], b'VMAP_7.0')

    def test_wrong_source_hash_fails_before_compilation(self):
        with tempfile.TemporaryDirectory(prefix='wmo-liquid-source-guard-') as temp:
            root = pathlib.Path(temp)
            path = root / 'dep/g3dlite/AABox.cpp'
            path.parent.mkdir(parents=True)
            path.write_text('deliberately wrong authored source sentinel')
            with mock.patch.object(generator.subprocess, 'run') as run:
                with self.assertRaisesRegex(SystemExit, 'Wrong pinned source hash'):
                    generator.sources(root)
                run.assert_not_called()

    @unittest.skipUnless(os.environ.get('CMANGOS_SOURCE'), 'set CMANGOS_SOURCE for source hash guards')
    def test_all_original_source_guards_and_patch_hash_match(self):
        source = generator.sources(pathlib.Path(os.environ['CMANGOS_SOURCE']))
        fragments, collision, patch, patched = generator.fragments(source)
        self.assertEqual((ROOT / 'source-fragments.cpp.test-fixture').read_text(), fragments+collision)
        self.assertEqual((ROOT / 'padding-v1.patch.test-fixture').read_text(), patch)
        self.assertEqual(dict(rows('provenance.tsv'))['patched_wmo.cpp'], hashlib.sha256(patched.encode('latin1')).hexdigest())


if __name__ == '__main__':
    unittest.main()
