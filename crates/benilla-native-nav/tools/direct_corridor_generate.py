#!/usr/bin/env python3
"""Exact original loaded-mesh direct_corridor observer, independent of the production bridge."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
import math
import re
import struct
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
MANIFEST = HERE / 'direct_corridor_source_hashes.sha256'
PACKAGE = HERE / 'direct_corridor_source_fragments.test-fixture'
FIXTURES = HERE / 'fixtures/direct-corridor'
FILES = ('params.test-fixture', 'long600.test-fixture', 'long1100.test-fixture',
         'grid00.test-fixture', 'grid10.test-fixture', 'grid01.test-fixture', 'grid11.test-fixture', 'cases.test-fixture')
CPP = 'dep/recastnavigation/Detour/Source/DetourNavMeshQuery.cpp'
HEADER = 'src/game/MotionGenerators/PathFinder.h'
MOVEMAP = 'src/game/MotionGenerators/MoveMap.cpp'
TILE_HEADER = 'dep/recastnavigation/Detour/Include/DetourNavMesh.h'
METHODS = (('dtStatus dtNavMeshQuery::findPath(', 973),
           ('dtStatus dtNavMeshQuery::getPathToNode(', 1167),
           ('dtStatus dtNavMeshQuery::initSlicedFindPath(', 1216),
           ('dtStatus dtNavMeshQuery::updateSlicedFindPath(', 1280),
           ('dtStatus dtNavMeshQuery::finalizeSlicedFindPath(', 1497))
DETOUR_FILES = ('DetourAlloc', 'DetourAssert', 'DetourCommon', 'DetourNavMesh',
                'DetourNavMeshBuilder', 'DetourNavMeshQuery', 'DetourNode')
FIELDS = ('label geometry from to capacity nodes from_status from_ref from_nearest from_over '
          'to_status to_ref to_nearest to_over direct_status direct_corridor direct_nodes '
          'direct_state_nodes sliced_status sliced_corridor sliced_nodes sliced_iterations').split()


SOURCE_FILES = {CPP, HEADER, MOVEMAP, 'src/game/MotionGenerators/PathFinder.cpp', 'LICENSE',
                'dep/recastnavigation/License.txt'} | {
    f'dep/recastnavigation/Detour/Source/{name}.cpp' for name in DETOUR_FILES} | {
    f'dep/recastnavigation/Detour/Include/{name}.h' for name in (*DETOUR_FILES, 'DetourMath', 'DetourStatus')}
INPUT_FILES = {'authored.mmap', '0003232.mmtile', '0003231.mmtile'}


def manifest_rows(path, required):
    result = {}
    for row in path.read_text().splitlines():
        if not row or row.startswith('#'): continue
        columns = row.split()
        if len(columns) != 2: raise SystemExit('Malformed hash manifest: ' + str(path))
        digest, name = columns
        if not re.fullmatch('[0-9a-f]{64}', digest) or name not in required or name in result:
            raise SystemExit('Malformed hash manifest: ' + str(path))
        result[name] = digest
    if set(result) != required: raise SystemExit('Incomplete hash manifest: ' + str(path))
    return result


def guarded_sources(root):
    result = {}
    for name, expected in manifest_rows(MANIFEST, SOURCE_FILES).items():
        raw = (root / name).read_bytes().replace(b'\r\n', b'\n')
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit('Wrong pinned source hash: ' + name)
        result[name] = raw.decode('latin1')
    return result


def guard_inputs():
    for name, expected in manifest_rows(HERE / 'direct_corridor_input_hashes.sha256', INPUT_FILES).items():
        if hashlib.sha256((HERE / 'fixtures' / name).read_bytes()).hexdigest() != expected:
            raise SystemExit('Authored input fixture drift: ' + name)


def exact_block(source, signature):
    if source.count(signature) != 1: raise SystemExit('Expected one source block: ' + signature)
    start = source.index(signature)
    depth = 0
    opening = source.find('{', start)
    if opening < 0: raise SystemExit('Unclosed source block: ' + signature)
    for end in range(opening, len(source)):
        depth += (source[end] == '{') - (source[end] == '}')
        if depth == 0: return source[start:end + 1]
    raise SystemExit('Unclosed source block: ' + signature)


def build_harness(sources):
    fragments = []
    for signature, line in METHODS:
        original = sources[CPP]
        method = exact_block(original, signature)
        if original[:original.index(signature)].count('\n') + 1 != line:
            raise SystemExit('Pinned source location drift: ' + signature)
        fragments.append((CPP, method))
    original = sources[MOVEMAP]
    signature = 'bool MMapManager::loadMapInstance('
    if original[:original.index(signature)].count('\n') + 1 != 356:
        raise SystemExit('Pinned source location drift: ' + signature)
    method = exact_block(original, signature)
    if method.count('query->init(mmap->navMesh, 1024)') != 1:
        raise SystemExit('Source map node pool differs')
    fragments.append((MOVEMAP, method))
    original_tile = exact_block(sources[TILE_HEADER], 'struct dtMeshTile\n{')
    original_layout = original_tile.replace('struct dtMeshTile', 'struct OriginalTileLayout', 1)
    original_layout = original_layout.replace('dtMeshTile(const dtMeshTile&)', 'OriginalTileLayout(const OriginalTileLayout&)')
    original_layout = original_layout.replace('dtMeshTile& operator=(const dtMeshTile&)', 'OriginalTileLayout& operator=(const OriginalTileLayout&)') + ';'
    harness = (HERE / 'direct_corridor_oracle.cpp').read_text()
    marker = '// ORIGINAL_TILE_LAYOUT'
    if harness.count(marker) != 1: raise SystemExit('Expected one harness marker: ' + marker)
    harness = harness.replace(marker, original_layout)
    notices = [sources[CPP].split('#include', 1)[0].rstrip(),
               sources[MOVEMAP][:sources[MOVEMAP].index('*/') + 2]]
    package = '// CMaNGOS ' + PIN + '; unchanged original direct and sliced query controls.\n'
    package += '// Original complete Detour translation units are compiled independently of the production bridge.\n'
    package += '\n\n'.join(notices) + '\n'
    for name, fragment in fragments:
        package += f'\n// BEGIN {name}\n{fragment}\n// END {name}\n'
    package += '\n/* Original CMaNGOS LICENSE\n' + sources['LICENSE'] + '\n*/\n'
    package += '\n/* Original Detour License.txt\n' + sources['dep/recastnavigation/License.txt'] + '\n*/\n'
    return '\n\n'.join(notices) + '\n' + harness, package


def run(command, **kwargs):
    print('+ ' + shlex.join(map(str, command)), file=sys.stderr, flush=True)
    return subprocess.run(command, check=True, **kwargs)


def verify_symbols(symbols):
    for signature, _ in METHODS:
        name = signature.split(' ', 1)[1].split('(')[0] + '('
        if name not in symbols: raise SystemExit('Missing unchanged source symbol: ' + name)
    for name in ('dtNavMeshQuery::findPath(', 'dtNavMeshQuery::initSlicedFindPath(',
                 'dtNavMeshQuery::finalizeSlicedFindPath(', 'dtCreateNavMeshData('):
        if name not in symbols: raise SystemExit('Missing actual Detour symbol: ' + name)
    if 'benilla_nav_' in symbols or 'benilla_source_' in symbols: raise SystemExit('Production bridge linked into source observer')


def repair_tile_header(original):
    marker = 'struct dtMeshTile\n{\n'
    if original.count(marker) != 1 or 'dtMeshTile()' in original:
        raise SystemExit('Tile-lifetime repair insertion drift')
    repaired = original.replace(marker, marker + '\tdtMeshTile() = default;\n', 1)
    if repaired.replace('\tdtMeshTile() = default;\n', '', 1) != original:
        raise SystemExit('Tile-lifetime repair changed unrelated source')
    return repaired


def compile_oracle(sources, work, sanitize=False, receipts=None, repair_lifetime=False):
    compiler = shutil.which('g++')
    if not compiler: raise SystemExit('Missing required g++ compiler')
    version = run([compiler, '-dumpfullversion'], stdout=subprocess.PIPE).stdout.decode().strip()
    target = run([compiler, '-dumpmachine'], stdout=subprocess.PIPE).stdout.decode().strip()
    if version != '14.2.0' or target != 'x86_64-linux-gnu':
        raise SystemExit('Unsupported audited compiler profile: ' + version + ' ' + target)
    harness, _ = build_harness(sources)
    (work / 'oracle.cpp').write_text(harness, encoding='latin1')
    for name, content in sources.items():
        if name.startswith('dep/recastnavigation/'):
            dest = work / name; dest.parent.mkdir(parents=True, exist_ok=True)
            if repair_lifetime and name == TILE_HEADER: content = repair_tile_header(content)
            dest.write_text(content, encoding='latin1')
    detour = work / 'dep/recastnavigation/Detour'
    flags = ['-std=c++20', '-O2', '-fno-inline', '-march=x86-64', '-mno-fma', '-fno-fast-math',
             '-ffp-contract=off', '-DDT_POLYREF64=1', '-Wall', '-Wextra', '-Werror', '-Wno-class-memaccess']
    if repair_lifetime: flags += ['-DDIRECT_CORRIDOR_REPAIR_TILE_LIFETIME=1']
    if sanitize: flags += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
    command = [compiler, *flags, '-I' + str(detour / 'Include'), str(work / 'oracle.cpp'),
               *[str(detour / f'Source/{name}.cpp') for name in DETOUR_FILES], '-o', str(work / 'oracle')]
    run(command)
    symbols = run(['nm', '-C', str(work / 'oracle')], stdout=subprocess.PIPE).stdout.decode()
    verify_symbols(symbols)
    if receipts:
        receipts.mkdir(parents=True, exist_ok=True)
        shutil.copy2(work / 'oracle.cpp', receipts / 'oracle.cpp')
        (receipts / 'symbols.txt').write_text(symbols)
        shutil.copy2(work / TILE_HEADER, receipts / 'DetourNavMesh_observed.h')
        (receipts / 'profile.json').write_text(json.dumps({
            'cmangos_revision': PIN, 'compiler': version, 'target': target,
            'command': command, 'sanitize': sanitize, 'query_nodes': [1024, 512],
            'tile_lifetime_profile': 'default-constructor-only-repaired' if repair_lifetime else 'unchanged-original-empirical',
            'observed_tile_header_sha256': hashlib.sha256((work / TILE_HEADER).read_bytes()).hexdigest(),
            'tile_lifetime_amendment': 'insert dtMeshTile() = default; only' if repair_lifetime else None,
            'scope': 'unchanged original direct findPath; 1024 map pool; authored loaded ground-only geometry',
            'excluded': ['cold getPolyByLocation', 'OwnerPathFinder', 'OptimizePath', 'NormalizePath', 'gameplay admission'],
            'source_hashes_sha256': hashlib.sha256(MANIFEST.read_bytes()).hexdigest(),
            'authored_input_hashes_sha256': hashlib.sha256((HERE / 'direct_corridor_input_hashes.sha256').read_bytes()).hexdigest(),
            'harness_sha256': hashlib.sha256((work / 'oracle.cpp').read_bytes()).hexdigest(),
        }, indent=2) + '\n')
    return work / 'oracle'


def parse_cases(data):
    lines = data.decode('ascii').splitlines()
    expected = '# ' + '\t'.join(FIELDS)
    if lines.count(expected) != 1: raise SystemExit('Direct-corridor oracle schema differs')
    result = []
    labels = set()
    for line in lines:
        if line.startswith('#'): continue
        if len(line.split('\t')) != len(FIELDS): raise SystemExit('Direct-corridor oracle schema differs')
        row = dict(zip(FIELDS, line.split('\t'), strict=True))
        if row['label'] in labels or not re.fullmatch('[a-z0-9_]+', row['label']):
            raise SystemExit('Invalid or duplicate oracle label')
        labels.add(row['label'])
        try:
            if row['geometry'] not in {'existing', 'grid', 'long600', 'long1100'}: raise ValueError()
            capacity, nodes = int(row['capacity']), int(row['nodes'])
            if not 1 <= capacity <= 1100 or nodes not in {512, 1024}: raise ValueError()
            for name in ('from', 'to', 'from_nearest', 'to_nearest'):
                if not re.fullmatch('[0-9a-f]{8},[0-9a-f]{8},[0-9a-f]{8}', row[name]): raise ValueError()
                if not all(math.isfinite(struct.unpack('!f', bytes.fromhex(word))[0]) for word in row[name].split(',')): raise ValueError()
            for name in ('from_over', 'to_over'):
                if row[name] not in {'0', '1'}: raise ValueError()
            for name in ('from_ref', 'to_ref'):
                if not 0 < int(row[name]) < (1 << 64): raise ValueError()
            for name in ('from_status', 'to_status', 'direct_status', 'sliced_status'):
                if not re.fullmatch('[0-9a-f]{8}', row[name]): raise ValueError()
            for name in ('direct_nodes', 'direct_state_nodes', 'sliced_nodes'):
                if not 0 <= int(row[name]) <= nodes: raise ValueError()
            if not 0 <= int(row['sliced_iterations']) < 100000: raise ValueError()
            for name in ('direct_corridor', 'sliced_corridor'):
                refs = [] if row[name] == '-' else [int(ref) for ref in row[name].split(',')]
                if not 0 < len(refs) <= capacity or any(not 0 < ref < (1 << 64) for ref in refs): raise ValueError()
        except ValueError:
            raise SystemExit('Invalid oracle row: ' + row['label']) from None
        result.append(row)
    if not result: raise SystemExit('Empty oracle cases')
    return result


def observe(binary, work):
    if len(set(FILES)) != len(FILES) or any(Path(name).name != name or not name.endswith('.test-fixture') for name in FILES):
        raise SystemExit('Unsafe fixture output filename')
    output = work / 'output'; output.mkdir(exist_ok=True)
    run([str(binary), str(output), str(HERE / 'fixtures')])
    if {path.name for path in output.iterdir()} != set(FILES): raise SystemExit('Unexpected oracle output files')
    result = {name: (output / name).read_bytes() for name in FILES}
    parse_cases(result['cases.test-fixture'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos', type=Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true')
    parser.add_argument('--repair-tile-lifetime', action='store_true')
    parser.add_argument('--receipt-dir', type=Path)
    args = parser.parse_args()
    sources = guarded_sources(args.mangos)
    guard_inputs()
    _, package = build_harness(sources)
    if args.check and PACKAGE.read_text(encoding='latin1') != package:
        raise SystemExit('Verbatim direct-corridor source package differs')
    with tempfile.TemporaryDirectory(prefix='direct_corridor-source-') as temporary:
        work = Path(temporary)
        binary = compile_oracle(sources, work, args.sanitize, args.receipt_dir, args.repair_tile_lifetime)
        fixtures = observe(binary, work)
    if args.check:
        for name, data in fixtures.items():
            if (FIXTURES / name).read_bytes() != data: raise SystemExit('Direct-corridor fixture differs: ' + name)
    else:
        PACKAGE.write_text(package, encoding='latin1')
        FIXTURES.mkdir(parents=True, exist_ok=True)
        for name, data in fixtures.items(): (FIXTURES / name).write_bytes(data)
    for name, data in fixtures.items():
        print(f'{"Verified" if args.check else "Generated"} {name}: {len(data)} bytes; sha256={hashlib.sha256(data).hexdigest()}')
    if args.receipt_dir:
        (args.receipt_dir / 'output_hashes.json').write_text(json.dumps({name: hashlib.sha256(data).hexdigest() for name, data in fixtures.items()}, indent=2) + '\n')


if __name__ == '__main__': main()
