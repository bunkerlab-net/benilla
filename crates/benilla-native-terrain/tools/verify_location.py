#!/usr/bin/env python3
"""Independently verify private captured area grids. This does not bypass VMaps."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess


def word(data, at):
    return struct.unpack_from('<I', data, at)[0]


def dbc(data, fields):
    assert data[:4] == b'WDBC'
    count, width, stride, strings = struct.unpack_from('<4I', data, 4)
    assert width == fields and stride == fields * 4
    end = 20 + count * stride
    assert end + strings == len(data) and data[end] == data[-1] == 0
    return [struct.unpack_from('<' + 'I' * fields, data, 20 + i * stride) for i in range(count)], data[end:]


def epoch(manifest, members, payloads, absent=()):
    digest = hashlib.sha256(b'benilla-native-terrain-source-v1\0')
    def field(data):
        digest.update(struct.pack('<Q', len(data)))
        digest.update(data)
    field(manifest['source_policy'].encode())
    field(manifest['source_pin'].encode())
    digest.update(struct.pack('<Q', len(members)))
    for member, payload in zip(members, payloads, strict=True):
        assert len(payload) == member['bytes']
        assert hashlib.sha256(payload).hexdigest() == member['sha256']
        field(member['name'].encode())
        field(member['winning_archive'].encode())
        field(payload)
    if absent:
        field(b'dual-resolver-absent-members-v1')
        digest.update(struct.pack('<Q', len(absent)))
        for member in absent:
            field(b'absent')
            field(member['name'].encode())
    return digest.hexdigest()


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def verify(root, position):
    manifest = json.loads((root / 'location.json').read_text())
    assert manifest['format'] == 'benilla-source-location-inputs-v1'
    assert manifest['source_pin'] == '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
    assert manifest['wmo_precedence_required'] is True
    members = manifest['terrain_members']
    payloads = [(root / f'terrain-source-{i:04}.bin').read_bytes() for i in range(len(members))]
    assert epoch(manifest, members, payloads, manifest['terrain_absent_members']) == manifest['terrain_epoch']
    catalogs = [(root / row['name'].split('\\')[-1]).read_bytes() for row in manifest['catalog_members']]
    assert epoch(manifest, manifest['catalog_members'], catalogs) == manifest['catalog_epoch']
    assert hashlib.sha256(manifest['policy'].encode() + manifest['terrain_epoch'].encode() + manifest['catalog_epoch'].encode()).hexdigest() == manifest['source_epoch']
    source = dict(zip((m['name'] for m in members), payloads, strict=True))
    areas, strings = dbc((root / 'AreaTable.dbc').read_bytes(), 25)
    by_id = {r[0]: r for r in areas}
    assert len(by_id) == len(areas)
    maps, map_strings = dbc((root / 'Map.dbc').read_bytes(), 42)
    map_row = next(r for r in maps if r[0] == manifest['map'])
    directory = map_strings[map_row[1]:].split(b'\0')[0].decode()
    dbc((root / 'WMOAreaTable.dbc').read_bytes(), 20)
    catalog = hashlib.sha256(b'cmangos-8ec338a1-source-location-dbc-v1\0')
    for name in ('AreaTable.dbc', 'WMOAreaTable.dbc', 'Map.dbc'):
        payload = (root / name).read_bytes()
        catalog.update(struct.pack('<Q',len(payload)))
        catalog.update(payload)
    for grid in manifest['grids']:
        x, y = grid['tile']['x'], grid['tile']['y']
        adt = source[f'World\\Maps\\{directory}\\{directory}_{y}_{x}.adt']
        assert adt[84:88] == b'NICM'
        ids = []
        for slot in range(256):
            at = word(adt, 92 + slot * 16)
            assert adt[at:at + 4] == b'KNCM'
            ids.append(word(adt, at + 8 + 0x34))
        assert ids == grid['area_ids']
        flags = [(by_id[area][3] & 65535) if area and area in by_id else 65535 for area in ids]
        assert flags == grid['area_flags']
        terrain = (root / f'{manifest["map"]:03}{x:02}{y:02}.map').read_bytes()
        offset, size = struct.unpack_from('<II', terrain, 8)
        assert terrain[offset:offset + 4] == b'AREA'
        header, uniform = struct.unpack_from('<HH', terrain, offset + 4)
        assert header == grid['header_flags'] and uniform == grid['grid_area']
        assert header == int(len(set(flags)) == 1)
        assert size == (8 if header else 520)
        stored = [uniform] * 256 if header else list(struct.unpack_from('<256H', terrain, offset + 8))
        assert stored == flags
    x, y, z = map(f32, position)
    size = f32(1600 / 3)
    gx, gy = (f32(32 - f32(value / size)) for value in (x, y))
    tile = (math.floor(gx), math.floor(gy))
    grid = next((g for g in manifest['grids'] if (g['tile']['x'], g['tile']['y']) == tile), None)
    if grid is None:
        raise ValueError('location.outside_capture')
    cell = (int(f32(16 * gx)) & 15, int(f32(16 * gy)) & 15)
    flag = grid['area_flags'][cell[0] * 16 + cell[1]]
    matches = [r for r in sorted(areas) if flag and r[3] == flag]
    row = next((r for r in matches if r[1] == manifest['map']), matches[-1] if matches else by_id.get(map_row[19]))
    chain = []
    while row is not None:
        assert row[0] not in [r['id'] for r in chain]
        chain.append({'id': row[0], 'parent': row[2], 'name': strings[row[11]:].split(b'\0')[0].decode()})
        if row[2] == 0:
            break
        row = by_id[row[2]]
    return {'source_epoch': manifest['source_epoch'], 'catalog': catalog.hexdigest(), 'map': manifest['map'], 'position_bits': [word(struct.pack('<f', n), 0) for n in (x,y,z)], 'tile': tile, 'cell': cell, 'area_flag': flag, 'grid_area_id': chain[0]['id'] if chain else 0, 'grid_zone_id': chain[0]['parent'] or chain[0]['id'] if chain else 0, 'parent_chain': chain, 'wmo_precedence_required': True, 'gameplay_admitted': False}


def verify_wmo_exclusion(root, position, oracle=None):
    """Complete tiled-map slots, then source bounds or original-source geometry."""
    manifest = json.loads((root / 'index.json').read_text())
    assert manifest['format'] == 'benilla-source-location-wmo-index-v1'
    assert manifest['map'] == 0
    files = {}
    for row in manifest['files']:
        name = row['file']
        assert Path(name).name == name and name not in files
        data = (root / name).read_bytes()
        assert len(data) == row['bytes']
        assert hashlib.sha256(data).hexdigest() == row['sha256']
        files[name] = data
    tree = files.pop('000.vmtree')
    assert tree[:13] == b'VMAP_7.0\x01NODE'
    at = 13 + 24
    at += 4 + word(tree, at) * 4
    count = word(tree, at)
    at += 4
    objects = struct.unpack_from('<' + 'I' * count, tree, at)
    assert sorted(objects) == list(range(count))
    at += count * 4
    assert tree[at:] == b'GOBJ'
    slots = {}
    for name, data in files.items():
        if name == 'models.index' or name.endswith('.vmo'):
            continue
        assert name.endswith('.vmtile') and data[:8] == b'VMAP_7.0'
        records = word(data, 8)
        at = 12
        for _ in range(records):
            start = at
            flags = word(data, at)
            assert flags & ~7 == 0 and flags & 4
            bounds = struct.unpack_from('<6f', data, at + 38)
            assert all(math.isfinite(v) for v in bounds)
            assert all(bounds[i] <= bounds[i+3] for i in range(3))
            name_bytes = word(data, at + 62)
            at += 66 + name_bytes
            slot = word(data, at)
            at += 4
            assert slot < count
            record = data[start:at]
            if slot in slots:
                assert slots[slot][0] == record
            slots[slot] = (record, flags, bounds)
        assert at == len(data)
    assert sorted(slots) == list(range(count))
    mid = f32(f32(0.5 * 64) * f32(533.33333333))
    point = [f32(mid - f32(position[0])), f32(mid - f32(position[1])), f32(position[2])]
    wmos = 0
    candidates = 0
    for _, flags, bounds in slots.values():
        if flags & 1:
            continue
        wmos += 1
        if all(bounds[i] <= point[i] <= bounds[i+3] for i in range(3)):
            candidates += 1
    result = {'collision_epoch': manifest['collision_epoch'], 'slots': count,
              'wmo_spawns': wmos, 'wmo_bounds_containing_position': candidates}
    if candidates:
        if oracle is None:
            raise ValueError('location.wmo_candidate_requires_geometry')
        run = subprocess.run([str(oracle), str(root), *map(str, position)],
                             check=True, text=True, capture_output=True)
        observed = json.loads(run.stdout)
        assert observed['position_bits'] == [word(struct.pack('<f',n),0) for n in position]
        assert observed['slots'] == count and observed['wmo_candidates'] == candidates
        if observed['raw_hit'] is not False:
            raise ValueError('location.wmo_override_requires_resolution')
        result['original_source'] = observed
        result['oracle_sha256'] = hashlib.sha256(oracle.read_bytes()).hexdigest()
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('capture', type=Path)
    parser.add_argument('--position', nargs=3, type=float, required=True)
    parser.add_argument('--collision-index', type=Path)
    parser.add_argument('--wmo-oracle', type=Path)
    args = parser.parse_args()
    result = verify(args.capture, args.position)
    if args.collision_index:
        result['wmo_exclusion'] = verify_wmo_exclusion(args.collision_index, args.position, args.wmo_oracle)
        result['wmo_precedence_required'] = False
        result['combined_source_epoch'] = hashlib.sha256(
            b'benilla-genuine-resident-source-location-v1' +
            result['source_epoch'].encode() +
            bytes.fromhex(result['wmo_exclusion']['collision_epoch'])
        ).hexdigest()
    print(json.dumps(result, indent=2))
