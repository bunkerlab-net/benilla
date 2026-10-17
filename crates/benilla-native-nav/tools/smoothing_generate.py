#!/usr/bin/env python3
"""Exact original loaded-mesh smoothing observer, independent of the production bridge."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
MANIFEST = HERE / 'smoothing_source_hashes.sha256'
PACKAGE = HERE / 'smoothing_source_fragments.test-fixture'
FIXTURES = HERE / 'fixtures/smoothing'
FILES = ('params.test-fixture', 'ground.test-fixture', 'thin.test-fixture', 'cases.test-fixture', 'fixup.test-fixture', 'range.test-fixture')
METHODS = (
    ('uint32 PathFinder::fixupCorridor(', 1141),
    ('bool PathFinder::getSteerTarget(', 1189),
    ('dtStatus PathFinder::findSmoothPath(', 1226),
    ('bool PathFinder::inRangeYZX(', 1438),
)
CPP = 'src/game/MotionGenerators/PathFinder.cpp'
HEADER = 'src/game/MotionGenerators/PathFinder.h'
TILE_HEADER = 'dep/recastnavigation/Detour/Include/DetourNavMesh.h'
DETOUR_FILES = ('DetourAlloc', 'DetourAssert', 'DetourCommon', 'DetourNavMesh',
                'DetourNavMeshBuilder', 'DetourNavMeshQuery', 'DetourNode')
FIELDS = ('label geometry from to limit from_status from_ref from_nearest from_over from_attempts '
          'to_status to_ref to_nearest to_over to_attempts corridor_status corridor bounded_from bounded_to '
          'probe_steer_status probe_steer_count smooth_status stop points boundary_calls steer_calls '
          'move_calls height_calls steer_buffer_calls boundary_status steer_status move_status height_status '
          'full_steer_status full_steer_count final_storage final_corridor height_failures move_buffer_calls').split()


def guarded_sources(root):
    result = {}
    for row in MANIFEST.read_text().splitlines():
        if not row or row.startswith('#'): continue
        expected, name = row.split()
        raw = (root / name).read_bytes().replace(b'\r\n', b'\n')
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit('Wrong pinned source hash: ' + name)
        result[name] = raw.decode('latin1')
    return result


def guard_inputs():
    for row in (HERE / 'smoothing_input_hashes.sha256').read_text().splitlines():
        if not row or row.startswith('#'): continue
        expected, name = row.split()
        if hashlib.sha256((HERE / 'fixtures' / name).read_bytes()).hexdigest() != expected:
            raise SystemExit('Authored input fixture drift: ' + name)


def exact_block(source, signature):
    if source.count(signature) != 1: raise SystemExit('Expected one source block: ' + signature)
    start = source.index(signature)
    depth = 0
    for end in range(source.index('{', start), len(source)):
        depth += (source[end] == '{') - (source[end] == '}')
        if depth == 0: return source[start:end + 1]
    raise SystemExit('Unclosed source block: ' + signature)


def build_harness(sources):
    text = sources[HEADER]
    begin = text.index('#ifdef ENABLE_PLAYERBOTS')
    end = text.index('#define INVALID_POLYREF   0') + len('#define INVALID_POLYREF   0')
    constants = text[begin:end]
    fragments = [(HEADER, constants)]
    methods = []
    for signature, line in METHODS:
        original = sources[CPP]
        method = exact_block(original, signature)
        if original[:original.index(signature)].count('\n') + 1 != line:
            raise SystemExit('Pinned source location drift: ' + signature)
        fragments.append((CPP, method)); methods.append(method)
    original_tile = exact_block(sources[TILE_HEADER], 'struct dtMeshTile\n{')
    original_layout = original_tile.replace('struct dtMeshTile', 'struct OriginalTileLayout', 1)
    original_layout = original_layout.replace('dtMeshTile(const dtMeshTile&)', 'OriginalTileLayout(const OriginalTileLayout&)')
    original_layout = original_layout.replace('dtMeshTile& operator=(const dtMeshTile&)', 'OriginalTileLayout& operator=(const OriginalTileLayout&)') + ';'
    harness = (HERE / 'smoothing_oracle.cpp').read_text()
    for marker, replacement in (('// PINNED_CONSTANTS', constants), ('// PINNED_METHODS', '\n\n'.join(methods)), ('// ORIGINAL_TILE_LAYOUT', original_layout)):
        if harness.count(marker) != 1: raise SystemExit('Expected one harness marker: ' + marker)
        harness = harness.replace(marker, replacement)
    notices = [sources[name][:sources[name].index('*/') + 2] for name in (CPP, HEADER)]
    package = '// CMaNGOS ' + PIN + '; unchanged complete smoothing methods.\n'
    package += '// Loaded authored mesh observer only; no owner routing, normalization, or gameplay admission.\n'
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
    for name in ('dtNavMeshQuery::findPath(', 'dtNavMeshQuery::findStraightPath(',
                 'dtNavMeshQuery::moveAlongSurface(', 'dtCreateNavMeshData('):
        if name not in symbols: raise SystemExit('Missing actual Detour symbol: ' + name)
    if 'benilla_nav_' in symbols: raise SystemExit('Production bridge linked into source observer')


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
    if repair_lifetime: flags += ['-DSMOOTHING_REPAIR_TILE_LIFETIME=1']
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
            'command': command, 'sanitize': sanitize, 'query_nodes': 512,
            'tile_lifetime_profile': 'default-constructor-only-repaired' if repair_lifetime else 'unchanged-original-empirical',
            'observed_tile_header_sha256': hashlib.sha256((work / TILE_HEADER).read_bytes()).hexdigest(),
            'tile_lifetime_amendment': 'insert dtMeshTile() = default; only' if repair_lifetime else None,
            'scope': 'unchanged full source smoothing; actual original Detour; authored ground-only geometry',
            'excluded': ['cold getPolyByLocation', 'OwnerPathFinder', 'OptimizePath', 'NormalizePath', 'gameplay admission'],
            'source_hashes_sha256': hashlib.sha256(MANIFEST.read_bytes()).hexdigest(),
        }, indent=2) + '\n')
    return work / 'oracle'


def observe(binary, work):
    output = work / 'output'; output.mkdir(exist_ok=True)
    run([str(binary), str(output), str(HERE / 'fixtures')])
    result = {name: (output / name).read_bytes() for name in FILES}
    for line in result['cases.test-fixture'].decode().splitlines():
        if line.startswith('#'): continue
        if len(line.split('\t')) != len(FIELDS): raise SystemExit('Smoothing oracle schema differs')
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
        raise SystemExit('Verbatim smoothing source package differs')
    with tempfile.TemporaryDirectory(prefix='smoothing-source-') as temporary:
        work = Path(temporary)
        binary = compile_oracle(sources, work, args.sanitize, args.receipt_dir, args.repair_tile_lifetime)
        fixtures = observe(binary, work)
    if args.check:
        for name, data in fixtures.items():
            if (FIXTURES / name).read_bytes() != data: raise SystemExit('Smoothing fixture differs: ' + name)
    else:
        PACKAGE.write_text(package, encoding='latin1')
        FIXTURES.mkdir(parents=True, exist_ok=True)
        for name, data in fixtures.items(): (FIXTURES / name).write_bytes(data)
    for name, data in fixtures.items():
        print(f'{"Verified" if args.check else "Generated"} {name}: {len(data)} bytes; sha256={hashlib.sha256(data).hexdigest()}')
    if args.receipt_dir:
        (args.receipt_dir / 'output_hashes.json').write_text(json.dumps({name: hashlib.sha256(data).hexdigest() for name, data in fixtures.items()}, indent=2) + '\n')


if __name__ == '__main__': main()
