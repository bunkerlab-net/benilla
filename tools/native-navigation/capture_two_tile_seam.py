#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate two original MMTiles and measure actual original Detour seam routes.

Only the interior hole is new authored input. ConvertADT, TerrainBuilder,
MapBuilder, Recast and Detour remain pinned original source. Every retained
route is resolved back to both loaded tile references, in both directions.
"""
import argparse
import json
import os
from pathlib import Path
import resource
import shutil
import subprocess
import sys
import time
import zlib
from authored_ridge import generate
from build_original import build, PIN
from run_oracle import json_bytes, mesh_info, request, sha, VMAP

HERE = Path(__file__).resolve().parent
SUCCESS = 0x40000000
TILES = [('selected', 30, 'ridge'), ('neighbor', 29, 'ridge_xminus')]


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (1024 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_CPU, (120, 120))


def run(command, log, seconds=120):
    started = time.monotonic()
    env = os.environ.copy()
    for name in ['BENILLA_ORACLE_QUERY_DETAILS', 'BENILLA_ORACLE_COLLECT_ONLY']:
        env.pop(name, None)
    with log.open('w') as output:
        subprocess.run(list(map(str, command)), check=True, stdout=output,
                       stderr=subprocess.STDOUT, env=env, timeout=seconds,
                       preexec_fn=limits)
    return {'wall_seconds': round(time.monotonic() - started, 6),
            'address_space_limit_bytes': 1024 * 1024 * 1024,
            'cpu_limit_seconds': 120, 'timeout_seconds': seconds}


def check_crossing(result):
    assert result['mesh_init_status'] == result['query_init_status'] == SUCCESS
    assert result['max_tiles'] == 2
    assert result['query_filter'] == {'include_flags': 1, 'exclude_flags': 0}
    assert result['extents_bits'] == ['40a00000'] * 3
    tiles = result['loaded_tiles']
    assert [t['filename'] for t in tiles] == ['0003130.mmtile', '0003129.mmtile']
    assert [(t['tile_x'], t['tile_y']) for t in tiles] == [(0, 0), (1, 0)]
    assert [t['poly_count'] for t in tiles] == [634, 632]
    assert all(t['add_tile_status'] == SUCCESS for t in tiles)
    refs = {t['tile_ref'] for t in tiles}
    assert len(refs) == 2 and len(result['queries']) == 6
    for row in result['queries']:
        assert all(row[key] == SUCCESS for key in ['start_status', 'end_status', 'status', 'straight_status'])
        assert row['start_over_poly'] and row['end_over_poly']
        assert row['corridor'][0] == row['start_ref'] and row['corridor'][-1] == row['end_ref']
        assert len(row['corridor']) == len(row['corridor_tile_refs']) == len(row['corridor_tile_coordinates']) == 10
        assert set(row['corridor_tile_refs']) == refs
        assert {tuple(p) for p in row['corridor_tile_coordinates']} == {(0, 0), (1, 0)}
        assert row['corridor_tile_refs'][0] != row['corridor_tile_refs'][-1]
        assert row['straight_flags'] == [1, 2]


def capture(source, work, output):
    work = work / 'two-tile-seam'
    work.mkdir(parents=True, exist_ok=True)
    output.mkdir(parents=True, exist_ok=True)
    exe = build(source, work.parent / 'build')
    authored = work / 'authored-input'
    generate(source, authored, interior_hole=True)
    # The new feature does not alter the already-retained selected ADT/map.
    for extension in ['adt', 'map']:
        suffix = '.test-fixture' if extension == 'adt' else ''
        assert (authored / ('ridge.' + extension)).read_bytes() == (HERE / 'fixtures/authored' / ('ridge.' + extension + suffix)).read_bytes()
    vmaps = None
    for line in VMAP.read_text(encoding='utf-8').splitlines():
        if line.startswith('captured_wmo\t'):
            vmaps = json.loads(line.split('\t')[3])
    assert vmaps is not None and set(vmaps) == {'000.vmtree', '000_30_31.vmtile', 'Hull.wmo.vmo'}
    manifest = {'schema': 1, 'source_pin': PIN,
                'scope': 'Two independently emitted original MMTiles; original Detour six complete bidirectional cross-edge routes',
                'authored_feature': {'terrain': 'ridge_xminus', 'mcnk_row': 8, 'mcnk_col': 8, 'hole_mask': 1,
                                     'all_other_hole_masks': 0, 'height_function_unchanged': True},
                'source_hashes_sha256': sha((HERE / 'source/source-hashes.json').read_bytes()),
                'vmap_products_sha256': {name: sha(bytes.fromhex(data)) for name, data in vmaps.items()},
                'tiles': {}, 'products': {}}
    results = {}
    resources = {}
    for name, x, terrain in TILES + [('neighbor_without_hole', 29, 'ridge_xminus')]:
        base = work / name
        if base.exists():
            shutil.rmtree(base)
        for folder in ['maps', 'vmaps', 'mmaps']:
            (base / folder).mkdir(parents=True, exist_ok=True)
        for _, tx, source_name in TILES:
            terrain_path = authored / (source_name + '.map')
            if name == 'neighbor_without_hole' and tx == 29:
                terrain_path = HERE / 'fixtures/authored/ridge_xminus.map'
            shutil.copyfile(terrain_path, base / 'maps' / f'00031{tx:02}.map')
        for filename, data in vmaps.items():
            (base / 'vmaps' / filename).write_bytes(bytes.fromhex(data))
        command = [exe, base, '0', str(x), '31', source / 'contrib/extractor_scripts/config.json', source / 'contrib/extractor_scripts/offmesh.txt']
        resources[name] = run(command, base / 'run.log')
        result = json.loads((base / 'source-result.json').read_bytes())
        assert result['tile_union'] == [[29, 31], [30, 31]]
        if name == 'neighbor_without_hole':
            assert result['output']['merged_polygons'] == 625 and not result['output']['emitted']
            assert not (base / 'mmaps/0003129.mmtile').exists()
            (output / 'neighbor-without-hole-expected.json').write_bytes(json_bytes(result))
            manifest['negative_control'] = {'map_fixture': '../authored/ridge_xminus.map',
                'map_sha256': sha((base / 'maps/0003129.map').read_bytes()),
                'expected_file': 'neighbor-without-hole-expected.json', 'expected_sha256': sha(json_bytes(result)),
                'merged_polygons': 625, 'emitted': False}
            continue
        assert result['output']['emitted'] and result['output']['merged_polygons'] == (634 if x == 30 else 632)
        results[name] = result
        target = output / name
        target.mkdir(exist_ok=True)
        entry = {'original_tile': {'map_id': 0, 'x': x, 'y': 31},
                 'production_tile_id': {'map_id': 0, 'x': 31, 'y': x},
                 'expected': result, 'maps_sha256': {p.name: sha(p.read_bytes()) for p in sorted((base / 'maps').iterdir())}}
        for filename in ['source-terrain.bin', 'source-combined.bin', 'source-mesh.bin']:
            raw = (base / filename).read_bytes()
            entry[filename] = mesh_info(raw)
            (target / (filename + '.zlib')).write_bytes(zlib.compress(raw, 9))
        raw = request(base, result, tile_x=x)
        (target / 'helper-request.bin.zlib').write_bytes(zlib.compress(raw, 9))
        entry['helper_request'] = {'bytes': len(raw), 'sha256': sha(raw)}
        (target / 'expected.json').write_bytes(json_bytes(result))
        for path in sorted((base / 'mmaps').iterdir()):
            raw = path.read_bytes()
            if (output / path.name).exists():
                assert (output / path.name).read_bytes() == raw, 'original map params differ across tiles'
            (output / path.name).write_bytes(raw)
            manifest['products'][path.name] = {'bytes': len(raw), 'sha256': sha(raw)}
        manifest['tiles'][name] = entry
    # The interior hole lies outside the strip used to generate the selected tile.
    old = HERE / 'fixtures/ridge_xminus_wmo'
    assert results['selected'] == json.loads((old / 'expected.json').read_bytes())
    assert (output / '0003130.mmtile').read_bytes() == (old / '0003130.mmtile').read_bytes()
    for filename in ['source-terrain.bin', 'source-combined.bin', 'source-mesh.bin', 'helper-request.bin']:
        assert (output / 'selected' / (filename + '.zlib')).read_bytes() == (old / (filename + '.zlib')).read_bytes()
    manifest['selected_unchanged_from'] = '../ridge_xminus_wmo'
    queries = []
    for row in [0, 2, 4]:
        start_index, end_index = row * 5 + 4, row * 5
        start = results['selected']['output']['queries'][start_index]['nearest_bits']
        end = results['neighbor']['output']['queries'][end_index]['nearest_bits']
        for direction, a, b in [('forward', start, end), ('reverse', end, start)]:
            queries.append({'name': f'row_{row+1}_{direction}', 'start_bits': a, 'end_bits': b,
                            'selected_source_query_index': start_index, 'neighbor_source_query_index': end_index})
    query_input = {'schema': 1, 'point_source': 'Exact original single-tile nearest points, queried again with scalar half-extents 5', 'queries': queries}
    (output / 'query-input.json').write_bytes(json_bytes(query_input))
    query_base = work / 'combined-query'
    (query_base / 'mmaps').mkdir(parents=True, exist_ok=True)
    for filename in manifest['products']:
        shutil.copyfile(output / filename, query_base / 'mmaps' / filename)
    resources['combined_query'] = run([exe, 'two_tile_queries', query_base, output / 'query-input.json', output / 'expected.json'], query_base / 'run.log', seconds=15)
    check_crossing(json.loads((output / 'expected.json').read_bytes()))
    manifest['query_input_sha256'] = sha((output / 'query-input.json').read_bytes())
    manifest['expected_sha256'] = sha((output / 'expected.json').read_bytes())
    target = output / 'authored'
    target.mkdir(exist_ok=True)
    for extension in ['adt', 'map']:
        filename = 'ridge_xminus_hole.' + extension + ('.test-fixture' if extension == 'adt' else '')
        shutil.copyfile(authored / ('ridge_xminus.' + extension), target / filename)
        manifest['authored_feature'][extension + '_fixture'] = 'authored/' + filename
        manifest['authored_feature'][extension + '_sha256'] = sha((target / filename).read_bytes())
    shutil.copyfile(authored / 'writer-provenance.json', target / 'writer-provenance.json')
    manifest['writer_provenance_sha256'] = sha((target / 'writer-provenance.json').read_bytes())
    (output / 'manifest.json').write_bytes(json_bytes(manifest))
    (work / 'resources.json').write_bytes(json_bytes(resources))
    print('Verified two emitted original tiles and six bidirectional cross-edge routes', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('--work', type=Path, default=HERE / '.work/two-tile')
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    output = args.work.resolve() / 'two-tile-seam-fixtures'
    if output.exists():
        shutil.rmtree(output)
    capture(args.source.resolve(), args.work.resolve(), output)
    retained = HERE / 'fixtures/two-tile-seam'
    files = lambda folder: {str(p.relative_to(folder)): p.read_bytes() for p in folder.rglob('*') if p.is_file()}
    if args.check:
        assert files(output) == files(retained), 'two-tile source replay differs'
    else:
        assert not retained.exists(), 'refuse to overwrite existing seam fixtures without --check'
        shutil.copytree(output, retained)


if __name__ == '__main__':
    main()
