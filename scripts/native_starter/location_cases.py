"""Original authored MCIN/MCNK and VMAP framing for the shared reader corpus.

These are not installation assets, raw-source certificates or runtime receipts.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import copy
import hashlib
import struct
from . import location_inputs as loc
from .dbc import parse_table
from classic_export.core import canonical, decode_json, digest, ExportError


def dbc(width, rows, strings=b'\0'):
    return b'WDBC' + struct.pack('<4I', len(rows), width, width * 4, len(strings)) + b''.join(
        struct.pack('<' + 'I' * width, *row) for row in rows) + strings


def bih(bound, count=1):
    return struct.pack('<6f4I', *bound, 3, 0xc0000000, count, 0) + struct.pack('<I', count) + b''.join(
        struct.pack('<I', i) for i in range(count))


def bundle(source=None):
    source = copy.deepcopy(source) if source is not None else {
        'selection': {'map_id': 0, 'min_milliunits': [-10000]*3, 'max_milliunits': [10000]*3},
        'tables': {'playercreateinfo': [{'map': 0, **{k: {'decimal': '0'} for k in ('position_x', 'position_y', 'position_z')}}]}}
    area = [0] * 25
    area[0], area[2], area[3] = 9, 12, 125
    zone = [0] * 25
    zone[0], zone[3] = 12, 12
    maps = [0] * 42
    maps[1], maps[19] = 1, 12
    catalogs = [dbc(25, [area, zone]), dbc(20, []), dbc(42, [maps], b'\0AuthorMap\0')]
    adt = bytearray(4188 + 136 * 256)
    adt[:12] = b'REVM' + struct.pack('<2I', 4, 18)
    adt[12:20] = b'RDHM' + struct.pack('<I', 64)
    adt[84:92] = b'NICM' + struct.pack('<I', 4096)
    for i in range(256):
        at = 4188 + i * 136
        struct.pack_into('<4I', adt, 92 + i * 16, at, 136, 0, 0)
        adt[at:at+8] = b'KNCM' + struct.pack('<I', 128)
        struct.pack_into('<I', adt, at + 8 + 0x34, 9)
    payloads = [catalogs[0], catalogs[2], bytes(adt)]
    names = ['DBFilesClient\\AreaTable.dbc', 'DBFilesClient\\Map.dbc', 'World\\Maps\\AuthorMap\\AuthorMap_32_32.adt']
    members = [{'name': n, 'winning_archive': 'authored.MPQ', 'bytes': len(p), 'sha256': loc.sha(p)} for n, p in zip(names, payloads)]
    capture = {'format': 'benilla-source-location-inputs-v1', 'source_pin': loc.PIN,
               'source_policy': 'benilla-chain-and-cmangos-ad-fixed-five-exact-winner-v3',
               'policy': 'cmangos-8ec338a1-captured-grid-area-inputs-v1', 'map': 0,
               'terrain_members': members, 'terrain_absent_members': [],
               'catalog_members': [{'name': 'DBFilesClient\\WMOAreaTable.dbc', 'winning_archive': 'authored.MPQ',
                                    'bytes': len(catalogs[1]), 'sha256': loc.sha(catalogs[1])}],
               'grids': [{'tile': {'x': 32, 'y': 32}, 'area_ids': [9]*256, 'area_flags': [125]*256,
                          'header_flags': 1, 'grid_area': 125}], 'areas': [],
               'wmo_precedence_required': True, 'live_install_lease': False, 'gameplay_admitted': False}
    capture['terrain_epoch'] = loc.epoch(capture, members, payloads)
    capture['catalog_epoch'] = loc.epoch(capture, capture['catalog_members'], [catalogs[1]])
    capture['source_epoch'] = loc.sha(capture['policy'].encode() + capture['terrain_epoch'].encode() + capture['catalog_epoch'].encode())
    terrain = bytearray(24)
    struct.pack_into('<2I', terrain, 8, 24, 8)
    terrain.extend(b'AREA' + struct.pack('<2H', 1, 125))
    mid = loc.f32(loc.f32(0.5 * 64) * loc.f32(533.33333333))
    outer = (-100000.,)*3 + (100000.,)*3
    group = (1., 1., 1., 2., 2., 2.)
    model = b'VMAP_7.0WMOD' + struct.pack('<2I', 8, 42) + b'GMOD' + struct.pack('<I', 1)
    model += struct.pack('<6f2I', *group, 0, 24)
    model += b'VERT' + struct.pack('<2I9f', 40, 3, 1.,1.,1.,2.,1.,1.,1.,2.,1.)
    model += b'TRIM' + struct.pack('<5I', 16, 1, 0, 1, 2) + b'MBIH' + bih(group)
    model += b'LIQU' + struct.pack('<I', 0) + b'GBIH' + bih(group)
    tile = b'VMAP_7.0' + struct.pack('<I', 1)
    tile += struct.pack('<IHI7f6fI', 4, 0, 7, mid,mid,0.,0.,0.,0.,1.,*outer,5) + b'a.wmo' + struct.pack('<I', 0)
    values = {'capture/location.json': canonical(capture), 'capture/0003232.map': bytes(terrain),
              'collision/000.vmtree': b'VMAP_7.0\x01NODE' + bih(outer) + b'GOBJ',
              'collision/000_32_32.vmtile': tile, 'collision/a.vmo': model,
              'collision/models.index': struct.pack('<2I', 1, 5) + b'a.wmo' + struct.pack('<I', 5) + b'a.vmo'}
    for i, p in enumerate(payloads):
        values[f'capture/terrain-source-{i:04}.bin'] = p
    for name, p in zip(('AreaTable.dbc', 'WMOAreaTable.dbc', 'Map.dbc'), catalogs):
        values['capture/' + name] = p
    index = {'format': 'benilla-source-location-wmo-index-v1', 'map': 0, 'collision_epoch': '11'*32,
             'files': [{'file': name.removeprefix('collision/'), 'bytes': len(p), 'sha256': loc.sha(p)}
                       for name, p in values.items() if name.startswith('collision/')], 'gameplay_admitted': False}
    values['collision/index.json'] = canonical(index)
    catalog_hash = hashlib.sha256(b'cmangos-8ec338a1-source-location-dbc-v1\0')
    for p in catalogs:
        catalog_hash.update(struct.pack('<Q', len(p)))
        catalog_hash.update(p)
    header = {'format': loc.FORMAT, 'policy': loc.POLICY, 'source_sha256': digest(source),
              'selection_sha256': digest(source['selection']), 'position_bits': [0,0,0],
              'capture': 'capture', 'capture_sha256': loc.sha(values['capture/location.json']),
              'collision': 'collision', 'collision_sha256': loc.sha(values['collision/index.json']),
              'catalog': catalog_hash.hexdigest(),
              'source_epoch': loc.sha(b'benilla-genuine-resident-source-location-v1' + capture['source_epoch'].encode() + bytes.fromhex(index['collision_epoch']))}
    values['input.json'] = canonical(header)
    return source, catalogs[2], values


def rebind_collision(values):
    index = decode_json(values['collision/index.json'], 2*1024*1024)
    for row in index['files']:
        p = values['collision/' + row['file']]
        row.update(bytes=len(p), sha256=loc.sha(p))
    values['collision/index.json'] = canonical(index)
    header = decode_json(values['input.json'], 65536)
    header['collision_sha256'] = loc.sha(values['collision/index.json'])
    values['input.json'] = canonical(header)


def cases():
    result = []
    for name in ('group-excluded', 'group-reached', 'missing-slot', 'source-binding', 'source-epoch',
                 'position-outside-box', 'boolean-position', 'non-yaw-transform', 'file-hash', 'raw-area-binding', 'narrowed-flags'):
        source, maps, values = bundle()
        header = decode_json(values['input.json'], 65536)
        if name == 'group-reached':
            p = bytearray(values['collision/a.vmo'])
            struct.pack_into('<6f', p, 28, -1.,-1.,-1.,1.,1.,1.)
            values['collision/a.vmo'] = bytes(p)
            rebind_collision(values)
        elif name == 'missing-slot':
            values['collision/000_32_32.vmtile'] = b'VMAP_7.0' + struct.pack('<I', 0)
            rebind_collision(values)
        elif name == 'source-binding':
            header['source_sha256'] = '0'*64
        elif name == 'source-epoch':
            header['source_epoch'] = '22'*32
        elif name == 'position-outside-box':
            source['selection']['min_milliunits'][0] = 1
            header.update(source_sha256=digest(source), selection_sha256=digest(source['selection']))
        elif name == 'boolean-position':
            header['position_bits'][0] = False
        elif name == 'non-yaw-transform':
            p = bytearray(values['collision/000_32_32.vmtile'])
            struct.pack_into('<f', p, 12 + 22, 1.)
            values['collision/000_32_32.vmtile'] = bytes(p)
            rebind_collision(values)
        elif name == 'file-hash':
            values['collision/a.vmo'] += b'\0'
        elif name == 'raw-area-binding':
            capture = decode_json(values['capture/location.json'], 2*1024*1024)
            capture['grids'][0]['area_ids'][0] = 12
            values['capture/location.json'] = canonical(capture)
            header['capture_sha256'] = loc.sha(values['capture/location.json'])
        elif name == 'narrowed-flags':
            capture = decode_json(values['capture/location.json'], 2*1024*1024)
            areas = bytearray(values['capture/AreaTable.dbc'])
            struct.pack_into('<I', areas, 20 + 3*4, 65537)
            values['capture/AreaTable.dbc'] = values['capture/terrain-source-0000.bin'] = bytes(areas)
            adt = bytearray(values['capture/terrain-source-0002.bin'])
            for i in range(1, 256):
                struct.pack_into('<I', adt, 4188 + i*136 + 8 + 0x34, 0)
            values['capture/terrain-source-0002.bin'] = bytes(adt)
            flags = [1] + [65535]*255
            capture['grids'][0].update(area_ids=[9]+[0]*255, area_flags=flags, header_flags=0, grid_area=0)
            terrain = bytearray(24)
            struct.pack_into('<2I', terrain, 8, 24, 520)
            values['capture/0003232.map'] = bytes(terrain) + b'AREA' + struct.pack('<258H', 0, 0, *flags)
            payloads = [values[f'capture/terrain-source-{i:04}.bin'] for i in range(3)]
            for row, p in zip(capture['terrain_members'], payloads):
                row.update(bytes=len(p), sha256=loc.sha(p))
            capture['terrain_epoch'] = loc.epoch(capture, capture['terrain_members'], payloads)
            capture['source_epoch'] = loc.sha(capture['policy'].encode() + capture['terrain_epoch'].encode() + capture['catalog_epoch'].encode())
            values['capture/location.json'] = canonical(capture)
            header['capture_sha256'] = loc.sha(values['capture/location.json'])
            header['source_epoch'] = loc.sha(b'benilla-genuine-resident-source-location-v1' + capture['source_epoch'].encode() + bytes.fromhex('11'*32))
            catalog = hashlib.sha256(b'cmangos-8ec338a1-source-location-dbc-v1\0')
            for key in ('AreaTable.dbc', 'WMOAreaTable.dbc', 'Map.dbc'):
                p = values['capture/' + key]
                catalog.update(struct.pack('<Q', len(p)))
                catalog.update(p)
            header['catalog'] = catalog.hexdigest()
        if name not in ('group-reached', 'missing-slot', 'non-yaw-transform'):
            values['input.json'] = canonical(header)
        case = {'name': name, 'source': source, 'map_dbc': maps.hex(),
                'files': {name: data.hex() for name, data in values.items()}}
        try:
            case['result'] = loc.prepare(loc.Files(values=values), source, {'Map': parse_table('Map', maps)})
        except ExportError as error:
            case['diagnostic'] = str(error)
        result.append(case)
    return {'format': 'benilla-source-location-reader-cases-v1', 'cases': result}
