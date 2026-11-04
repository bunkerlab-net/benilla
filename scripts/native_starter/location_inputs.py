"""Checked first-entry source inputs, never a Profile or a location receipt.

CMaNGOS 8ec338a1 GridMap::getArea, ModelInstance::intersectPoint and
GroupModel::IsInsideObject. Nonexcluded groups require the full location owner.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import hashlib
import math
from pathlib import Path, PurePosixPath
import struct

from classic_export.core import check, decode_json, digest
from .dbc import binary32_bits, read_regular
from .location_trig import bits, f32, yaw_model_point

FORMAT = 'benilla-native-starter-location-input-v1'
POLICY = 'cmangos-8ec338a1-first-entry-grid-and-group-exclusion-v1'
PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
MAX_FILE = 32 * 1024 * 1024
MAX_TOTAL = 256 * 1024 * 1024
MAX_FILES = 4096
MAX_WORDS = 1048576


def require(value, reason):
    check(value, 'source_location.' + reason)


def fields(value, names, reason):
    require(type(value) is dict and set(value) == set(names), reason)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def hash_text(value):
    return (type(value) is str and len(value) == 64 and value != '0' * 64
            and all(c in '0123456789abcdef' for c in value))


def uint(value, width=32):
    return type(value) is int and 0 <= value < 1 << width


class Files:
    def __init__(self, root=None, values=None):
        self.root = Path(root) if root is not None else None
        require(self.root is None or self.root.is_dir() and not self.root.is_symlink(), 'input_directory')
        self.values = values
        self.names = set()
        self.work = 0

    def read(self, name, maximum=MAX_FILE):
        require(type(name) is str and '\\' not in name and name and
                not name.startswith('/') and all(p not in ('', '.', '..') for p in name.split('/')),
                'file_name')
        require(len(name.encode()) <= 512, 'file_name')
        self.names.add(name)
        require(len(self.names) <= MAX_FILES, 'file_count')
        if self.values is not None:
            require(name in self.values, 'file_required')
            data = self.values[name]
            require(type(data) is bytes and len(data) <= maximum, 'file_limit')
        else:
            path = self.root
            for part in PurePosixPath(name).parts:
                path = path / part
                require(not path.is_symlink(), 'symlink')
            data = read_regular(path, maximum)
        self.work += len(data)
        require(self.work <= MAX_TOTAL, 'aggregate_limit')
        return data

    def json(self, name):
        return decode_json(self.read(name, 2 * 1024 * 1024), 2 * 1024 * 1024)


def word(data, at):
    require(0 <= at <= len(data) - 4, 'binary_truncated')
    return struct.unpack_from('<I', data, at)[0]


def floats(data, at, count):
    require(0 <= at and at + count * 4 <= len(data), 'binary_truncated')
    result = struct.unpack_from('<' + 'f' * count, data, at)
    require(all(math.isfinite(v) for v in result), 'nonfinite')
    return result


def bounds(data, at):
    result = floats(data, at, 6)
    require(all(result[i] <= result[i+3] for i in range(3)), 'bounds')
    return result


def contains(bound, point):
    return all(bound[i] <= point[i] <= bound[i+3] for i in range(3))


def dbc(data, width, locales):
    require(data[:4] == b'WDBC', 'dbc_layout')
    count, fields_, stride, strings = (word(data, i) for i in (4, 8, 12, 16))
    require(count <= 65536 and strings <= 1024 * 1024, 'dbc_layout')
    end = 20 + count * stride
    require(fields_ == width and stride == width * 4 and strings and
            end + strings == len(data) and data[end] == data[-1] == 0, 'dbc_layout')
    rows = [struct.unpack_from('<' + 'I' * width, data, 20 + i * stride) for i in range(count)]
    require(len({r[0] for r in rows}) == count and all(r[0] != 0xffffffff for r in rows), 'dbc_layout')
    require(all(r[i] < strings for r in rows for i in locales), 'dbc_layout')
    return rows, data[end:]


def epoch(manifest, members, payloads, absent=()):
    h = hashlib.sha256(b'benilla-native-terrain-source-v1\0')
    def field(data):
        h.update(struct.pack('<Q', len(data)))
        h.update(data)
    field(manifest['source_policy'].encode())
    field(manifest['source_pin'].encode())
    h.update(struct.pack('<Q', len(members)))
    for member, payload in zip(members, payloads, strict=True):
        require(len(payload) == member['bytes'] and sha(payload) == member['sha256'], 'member_binding')
        field(member['name'].encode())
        field(member['winning_archive'].encode())
        field(payload)
    if absent:
        field(b'dual-resolver-absent-members-v1')
        h.update(struct.pack('<Q', len(absent)))
        for member in absent:
            field(b'absent')
            field(member['name'].encode())
    return h.hexdigest()


def grid_capture(files, prefix, position, label_names=False):
    manifest = files.json(prefix + '/location.json')
    fields(manifest, ('format', 'source_pin', 'source_policy', 'policy', 'map', 'source_epoch',
                      'terrain_epoch', 'catalog_epoch', 'terrain_members', 'terrain_absent_members',
                      'catalog_members', 'grids', 'areas', 'wmo_precedence_required',
                      'live_install_lease', 'gameplay_admitted'), 'capture_fields')
    require(uint(manifest['map']) and
            all(type(manifest[k]) is str for k in ('format', 'source_pin', 'source_policy', 'policy',
                                                  'source_epoch', 'terrain_epoch', 'catalog_epoch')) and
            all(type(manifest[k]) is bool for k in ('wmo_precedence_required', 'live_install_lease', 'gameplay_admitted')),
            'capture_fields')
    for key in ('terrain_members', 'catalog_members'):
        require(type(manifest[key]) is list, 'capture_fields')
        for member in manifest[key]:
            fields(member, ('name', 'winning_archive', 'bytes', 'sha256'), 'capture_fields')
            require(type(member['name']) is str and type(member['winning_archive']) is str and
                    type(member['sha256']) is str and uint(member['bytes'], 64), 'capture_fields')
    require(type(manifest['terrain_absent_members']) is list, 'capture_fields')
    for member in manifest['terrain_absent_members']:
        fields(member, ('name',), 'capture_fields')
        require(type(member['name']) is str, 'capture_fields')
    require(manifest['format'] == 'benilla-source-location-inputs-v1' and manifest['source_pin'] == PIN
            and manifest['source_policy'] == 'benilla-chain-and-cmangos-ad-fixed-five-exact-winner-v3'
            and manifest['policy'] == 'cmangos-8ec338a1-captured-grid-area-inputs-v1'
            and manifest['wmo_precedence_required'] is True and manifest['live_install_lease'] is False
            and manifest['gameplay_admitted'] is False,
            'capture_policy')
    members = manifest['terrain_members']
    require(len(members) <= MAX_FILES and len(manifest['terrain_absent_members']) <= MAX_FILES, 'member_limit')
    payloads = [files.read(prefix + f'/terrain-source-{i:04}.bin') for i in range(len(members))]
    require(epoch(manifest, members, payloads, manifest['terrain_absent_members']) == manifest['terrain_epoch'], 'terrain_epoch')
    source = dict(zip((m['name'] for m in members), payloads, strict=True))
    require(len(source) == len(members), 'duplicate_member')
    catalogs = [files.read(prefix + '/' + name) for name in ('AreaTable.dbc', 'WMOAreaTable.dbc', 'Map.dbc')]
    for name, payload in zip(('AreaTable.dbc', 'Map.dbc'), (catalogs[0], catalogs[2])):
        require(source.get('DBFilesClient\\' + name) == payload, 'catalog_source_binding')
    catalog_members = manifest['catalog_members']
    require(len(catalog_members) == 1 and catalog_members[0]['name'] == 'DBFilesClient\\WMOAreaTable.dbc', 'catalog_scope')
    require(epoch(manifest, catalog_members, [catalogs[1]]) == manifest['catalog_epoch'], 'catalog_epoch')
    require(sha(manifest['policy'].encode() + manifest['terrain_epoch'].encode() + manifest['catalog_epoch'].encode()) == manifest['source_epoch'], 'capture_epoch')
    areas, strings = dbc(catalogs[0], 25, range(11, 19))
    require(all(r[0] != 0xfffffffe for r in areas), 'dbc_layout')
    dbc(catalogs[1], 20, range(11, 19))
    maps, map_strings = dbc(catalogs[2], 42, range(4, 12))
    by_id = {r[0]: r for r in areas}
    map_row = next((r for r in maps if r[0] == manifest['map']), None)
    require(map_row is not None and map_row[1] < len(map_strings), 'map_required')
    directory = map_strings[map_row[1]:].split(b'\0')[0].decode()
    catalog = hashlib.sha256(b'cmangos-8ec338a1-source-location-dbc-v1\0')
    for payload in catalogs:
        catalog.update(struct.pack('<Q', len(payload)))
        catalog.update(payload)
    grids, seen = manifest['grids'], set()
    require(type(grids) is list and 0 < len(grids) <= 4096, 'grid_count')
    for grid in grids:
        fields(grid, ('tile', 'area_ids', 'area_flags', 'header_flags', 'grid_area'), 'capture_fields')
        fields(grid['tile'], ('x', 'y'), 'capture_fields')
        require(uint(grid['tile']['x'], 8) and uint(grid['tile']['y'], 8) and
                type(grid['area_ids']) is list and len(grid['area_ids']) == 256 and all(uint(v) for v in grid['area_ids']) and
                type(grid['area_flags']) is list and len(grid['area_flags']) == 256 and all(uint(v, 16) for v in grid['area_flags']) and
                uint(grid['header_flags'], 16) and uint(grid['grid_area'], 16), 'capture_fields')
        x, y = grid['tile']['x'], grid['tile']['y']
        require(type(x) is int and type(y) is int and 0 <= x < 64 and 0 <= y < 64 and (x, y) not in seen, 'grid_tile')
        seen.add((x, y))
        adt = source.get(f'World\\Maps\\{directory}\\{directory}_{y}_{x}.adt')
        require(adt is not None and adt[84:88] == b'NICM', 'raw_adt_required')
        ids = []
        for slot in range(256):
            at = word(adt, 92 + slot * 16)
            require(adt[at:at+4] == b'KNCM', 'raw_mcnk_required')
            ids.append(word(adt, at + 8 + 0x34))
        require(ids == grid['area_ids'], 'raw_area_binding')
        flags = [(by_id[area][3] & 65535) if area and area in by_id else 65535 for area in ids]
        require(flags == grid['area_flags'], 'area_flags_binding')
        terrain = files.read(prefix + f'/{manifest["map"]:03}{x:02}{y:02}.map')
        offset, size = word(terrain, 8), word(terrain, 12)
        require(terrain[offset:offset+4] == b'AREA' and offset + size <= len(terrain), 'area_section')
        header, uniform = struct.unpack_from('<HH', terrain, offset + 4)
        require(header == grid['header_flags'] and uniform == grid['grid_area'] and
                header == int(len(set(flags)) == 1) and size == (8 if header else 520), 'area_header')
        stored = [uniform] * 256 if header else list(struct.unpack_from('<256H', terrain, offset + 8))
        require(stored == flags, 'emitted_area_binding')
    x, y, z = map(f32, position)
    unit = f32(1600 / 3)
    gx, gy = (f32(32 - f32(value / unit)) for value in (x, y))
    require(0 <= gx < 64 and 0 <= gy < 64, 'position_outside_map')
    tile = math.floor(gx), math.floor(gy)
    grid = next((g for g in grids if (g['tile']['x'], g['tile']['y']) == tile), None)
    require(grid is not None, 'outside_capture')
    cell = int(f32(16 * gx)) & 15, int(f32(16 * gy)) & 15
    flag = grid['area_flags'][cell[0] * 16 + cell[1]]
    matches = [r for r in sorted(areas) if flag and r[3] == flag]
    row = next((r for r in matches if r[1] == manifest['map']), matches[-1] if matches else by_id.get(map_row[19]))
    chain = []
    while row is not None:
        require(row[0] not in [r['id'] for r in chain], 'parent_cycle')
        entry = {'id': row[0], 'parent': row[2]}
        if label_names:
            entry['name'] = strings[row[11]:].split(b'\0')[0].decode()
        chain.append(entry)
        if row[2] == 0:
            break
        require(row[2] in by_id, 'parent_required')
        row = by_id[row[2]]
    result = {'source_epoch': manifest['source_epoch'], 'catalog': catalog.hexdigest(), 'map': manifest['map'],
              'position_bits': [bits(v) for v in (x, y, z)], 'tile': list(tile), 'cell': list(cell),
              'area_flag': flag, 'grid_area_id': chain[0]['id'] if chain else 0,
              'grid_zone_id': (chain[0]['parent'] or chain[0]['id']) if chain else 0,
              'parent_chain': chain, 'wmo_precedence_required': True, 'gameplay_admitted': False}
    return result, catalogs


def verify_grid_capture(root, position):
    root = Path(root)
    return grid_capture(Files(root.parent), root.name, position, label_names=True)[0]


def read_bih(data, at):
    bounds(data, at)
    at += 24
    count = word(data, at)
    require(3 <= count <= MAX_WORDS and count % 3 == 0, 'bih_count')
    at += 4
    tree = [word(data, at + i * 4) for i in range(count)]
    at += count * 4
    count = word(data, at)
    require(count <= MAX_WORDS, 'bih_count')
    at += 4
    objects = [word(data, at + i * 4) for i in range(count)]
    at += count * 4
    require(sorted(objects) == list(range(count)), 'bih_objects')
    nodes, slots, pending = set(), set(), [(0, 0)]
    while pending:
        node, depth = pending.pop()
        require(depth <= 63 and node % 3 == 0 and node + 2 < len(tree) and node not in nodes, 'bih_structure')
        nodes.add(node)
        value = tree[node]
        axis, offset = value >> 30, value & 0x1fffffff
        if axis == 3:
            require(not value & (1 << 29) and tree[node+2] == 0 and offset + tree[node+1] <= count, 'bih_leaf')
            for slot in range(offset, offset + tree[node+1]):
                require(slot not in slots, 'bih_leaf')
                slots.add(slot)
        else:
            low, high = (struct.unpack('<f', struct.pack('<I', v))[0] for v in tree[node+1:node+3])
            if value & (1 << 29):
                require(math.isfinite(low) and math.isfinite(high) and low <= high, 'bih_clip')
                pending.append((offset, depth + 1))
            else:
                left, right = low != -math.inf, high != math.inf
                require((left or right) and (not left or math.isfinite(low)) and (not right or math.isfinite(high)), 'bih_clip')
                if right:
                    pending.append((offset + 3, depth + 1))
                if left:
                    pending.append((offset, depth + 1))
    require(len(nodes) == len(tree) // 3 and len(slots) == count, 'bih_coverage')
    return at, count


class CollisionIndex:
    def __init__(self, files, prefix):
        self.files, self.prefix = files, prefix
        self.manifest = files.json(prefix + '/index.json')
        manifest = self.manifest
        fields(manifest, ('format', 'map', 'collision_epoch', 'files', 'gameplay_admitted'), 'collision_fields')
        require(type(manifest['format']) is str and uint(manifest['map']) and type(manifest['collision_epoch']) is str
                and type(manifest['files']) is list and type(manifest['gameplay_admitted']) is bool, 'collision_fields')
        require(manifest['format'] == 'benilla-source-location-wmo-index-v1' and manifest['gameplay_admitted'] is False, 'collision_policy')
        require(hash_text(manifest['collision_epoch']), 'collision_epoch')
        self.rows = {}
        require(type(manifest['files']) is list and len(manifest['files']) <= MAX_FILES, 'collision_file_count')
        for row in manifest['files']:
            fields(row, ('file', 'bytes', 'sha256'), 'collision_fields')
            require(type(row['file']) is str and uint(row['bytes'], 64) and type(row['sha256']) is str, 'collision_fields')
            name = row['file']
            require(type(name) is str and PurePosixPath(name).name == name and name not in self.rows
                    and type(row['bytes']) is int and 0 <= row['bytes'] <= MAX_FILE and hash_text(row['sha256']), 'collision_file')
            self.rows[name] = row
        tree_name = f'{manifest["map"]:03}.vmtree'
        tree = self.read(tree_name)
        require(tree[:13] == b'VMAP_7.0\x01NODE', 'collision_tree')
        at, self.count = read_bih(tree, 13)
        require(tree[at:] == b'GOBJ', 'collision_tree')
        self.spawns = {}
        self.models = {}
        index = self.read('models.index')
        count, at = word(index, 0), 4
        require(count <= MAX_FILES, 'model_count')
        for _ in range(count):
            size = word(index, at); at += 4
            require(size <= 500 and at + size <= len(index), 'model_name')
            name = index[at:at+size]; at += size
            size = word(index, at); at += 4
            require(size <= 500 and at + size <= len(index), 'model_name')
            file = index[at:at+size].decode(); at += size
            require(name not in self.models and file in self.rows and file.endswith('.vmo'), 'model_binding')
            self.models[name] = file
        require(at == len(index), 'model_index')
        for name in self.rows:
            if name in (tree_name, 'models.index'):
                continue
            data = self.read(name)
            if name.endswith('.vmo'):
                continue
            require(name.endswith('.vmtile') and data[:8] == b'VMAP_7.0', 'collision_tile')
            count, at = word(data, 8), 12
            require(count <= MAX_WORDS, 'spawn_count')
            for _ in range(count):
                start, flags = at, word(data, at)
                require(not flags & ~7 and flags & 4 and not flags & 2, 'spawn_flags')
                position, rotation, scale = floats(data, at+10, 3), floats(data, at+22, 3), floats(data, at+34, 1)[0]
                bound, size = bounds(data, at+38), word(data, at+62)
                require(size <= 500 and at + 66 + size + 4 <= len(data), 'spawn_name')
                model = data[at+66:at+66+size]
                at += 66 + size
                slot = word(data, at); at += 4
                require(slot < self.count and model in self.models, 'spawn_slot')
                record = data[start:at]
                if slot in self.spawns:
                    require(self.spawns[slot][0] == record, 'duplicate_spawn_binding')
                else:
                    self.spawns[slot] = (record, flags, bound, model, position, rotation, scale)
            require(at == len(data), 'collision_tile')
        require(len(self.spawns) == self.count, 'complete_collision_slots')

    def read(self, name):
        require(name in self.rows, 'collision_file_required')
        data = self.files.read(self.prefix + '/' + name)
        row = self.rows[name]
        require(len(data) == row['bytes'] and sha(data) == row['sha256'], 'collision_file_binding')
        return data

    def candidates(self, position):
        mid = f32(f32(0.5 * 64) * f32(533.33333333))
        point = (f32(mid - f32(position[0])), f32(mid - f32(position[1])), f32(position[2]))
        wmos = [s for s in self.spawns.values() if not s[1] & 1]
        return point, wmos, [s for s in wmos if contains(s[2], point)]

    def summary(self, position):
        _, wmos, candidates = self.candidates(position)
        return {'collision_epoch': self.manifest['collision_epoch'], 'slots': self.count,
                'wmo_spawns': len(wmos), 'wmo_bounds_containing_position': len(candidates)}

    def exclude_groups(self, position):
        point, _, candidates = self.candidates(position)
        for spawn in candidates:
            local = yaw_model_point(point, spawn[4], spawn[5], spawn[6])
            data = self.read(self.models[spawn[3]])
            require(data[:12] == b'VMAP_7.0WMOD' and word(data, 12) == 8, 'world_model')
            if len(data) == 20:
                continue
            require(data[20:24] == b'GMOD', 'world_model')
            groups, at = word(data, 24), 28
            require(groups <= 262144, 'group_count')
            for _ in range(groups):
                bound = bounds(data, at); at += 32
                require(data[at:at+4] == b'VERT', 'model_vertices')
                size, count = word(data, at+4), word(data, at+8)
                require(count <= 262144 and size == 4 + count * 12 and at + 8 + size <= len(data), 'model_vertices')
                at += 8 + size
                if not count:
                    continue
                require(data[at:at+4] == b'TRIM', 'model_triangles')
                size, triangles = word(data, at+4), word(data, at+8)
                require(triangles <= 262144 and size == 4 + triangles * 12 and at + 8 + size <= len(data), 'model_triangles')
                at += 8 + size
                require(data[at:at+4] == b'MBIH', 'model_bih')
                at, count = read_bih(data, at+4)
                require(count == triangles and data[at:at+4] == b'LIQU', 'model_bih')
                liquid = word(data, at+4)
                require(liquid == 0, 'wmo_liquid_requires_owner')
                at += 8
                require(not triangles or not contains(bound, local), 'wmo_group_requires_owner')
            require(data[at:at+4] == b'GBIH', 'group_bih')
            at, count = read_bih(data, at+4)
            require(count == groups and at == len(data), 'group_bih')
        return self.summary(position)


def prepare(files, source, dbcs, input_name='input.json'):
    header = files.json(input_name)
    fields(header, ('format', 'policy', 'source_sha256', 'selection_sha256', 'position_bits',
                    'capture', 'capture_sha256', 'collision', 'collision_sha256', 'catalog', 'source_epoch'), 'input_fields')
    require(all(type(v) is str for k, v in header.items() if k != 'position_bits') and
            type(header['position_bits']) is list and len(header['position_bits']) == 3
            and all(uint(v) for v in header['position_bits']), 'input_fields')
    require(header['format'] == FORMAT and header['policy'] == POLICY, 'input_policy')
    require(header['source_sha256'] == digest(source) and header['selection_sha256'] == digest(source['selection']), 'source_binding')
    start = source['tables']['playercreateinfo'][0]
    position_bits = [binary32_bits(start[k]['decimal']) for k in ('position_x', 'position_y', 'position_z')]
    position = [struct.unpack('<f', struct.pack('<I', v))[0] for v in position_bits]
    require(header['position_bits'] == position_bits and start['map'] == source['selection']['map_id'], 'position_binding')
    require(all(source['selection']['min_milliunits'][i] <= v * 1000 <= source['selection']['max_milliunits'][i]
                for i, v in enumerate(position)), 'position_outside_selection')
    for key, filename in (('capture', 'location.json'), ('collision', 'index.json')):
        require(type(header[key]) is str and '/' not in header[key] and header[key] not in ('', '.', '..'), 'input_path')
        require(sha(files.read(header[key] + '/' + filename, 2 * 1024 * 1024)) == header[key + '_sha256'], 'input_manifest_binding')
    grid, catalogs = grid_capture(files, header['capture'], position)
    require(grid['map'] == start['map'] and grid['catalog'] == header['catalog'] and
            sha(catalogs[2]) == dbcs['Map'].sha256, 'catalog_binding')
    collision = CollisionIndex(files, header['collision'])
    require(collision.manifest['map'] == start['map'], 'collision_map')
    exclusion = collision.exclude_groups(position)
    combined = sha(b'benilla-genuine-resident-source-location-v1' + grid['source_epoch'].encode() + bytes.fromhex(exclusion['collision_epoch']))
    require(combined == header['source_epoch'], 'source_epoch_binding')
    return {'format': FORMAT, 'policy': POLICY, 'status': 'checked_first_entry_context',
            'source_sha256': digest(source), 'selection_sha256': digest(source['selection']),
            'input_sha256': digest(header), 'source_epoch': combined, 'catalog': grid['catalog'],
            'map': start['map'], 'position_bits': position_bits, 'area_flag': grid['area_flag'],
            'area_id': grid['grid_area_id'], 'zone_id': grid['grid_zone_id'], 'tile': grid['tile'],
            'cell': grid['cell'], 'wmo_exclusion': exclusion, 'runtime_export': None}


def load(path, source, dbcs):
    path = Path(path)
    return prepare(Files(path.parent), source, dbcs, path.name)
