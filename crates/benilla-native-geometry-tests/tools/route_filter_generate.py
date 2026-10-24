#!/usr/bin/env python3
"""Observe unchanged source creature filters with explicitly authored scalar callbacks."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
from pathlib import Path
import re
import shlex
import shutil
import struct
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
MANIFEST = HERE / 'route_filter_source_hashes.sha256'
PACKAGE = HERE / 'route_filter_source_fragments.test-fixture'
FIXTURE = HERE / 'fixtures/route_filter/cases.test-fixture'
INPUT_FIELDS = ('walk', 'swim', 'height', 'grid', 'x', 'y', 'z',
                'status0', 'type0', 'status1', 'type1', 'status2', 'type2')
OUTPUT_FIELDS = ('include', 'exclude', 'initial_include', 'include_calls', 'exclude_calls',
                 'read_calls', 'height_calls', 'grid_calls', 'liquid_calls', 'masks', 'trace')
METHODS = (
    ('src/game/MotionGenerators/PathFinder.cpp', 'void PathFinder::createFilter()', 1030),
    ('src/game/MotionGenerators/PathFinder.cpp', 'void PathFinder::updateFilter()', 1088),
    ('src/game/MotionGenerators/PathFinder.cpp', 'NavTerrainFlag PathFinder::getNavTerrain(', 1103),
    ('src/game/Entities/Unit.cpp', 'bool Unit::IsInWater()', 4467),
    ('src/game/Entities/Unit.cpp', 'bool Unit::IsUnderwater()', 4477),
    ('src/game/Maps/GridMap.cpp', 'bool TerrainInfo::CanCheckLiquidLevel(', 797),
    ('src/game/Maps/GridMap.cpp', 'bool TerrainInfo::IsInWater(', 1130),
    ('src/game/Maps/GridMap.cpp', 'bool TerrainInfo::IsUnderWater(', 1163),
)


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


def exact_block(source, signature):
    if source.count(signature) != 1: raise SystemExit('Expected one source block: ' + signature)
    start = source.index(signature)
    depth = 0
    for end in range(source.index('{', start), len(source)):
        depth += (source[end] == '{') - (source[end] == '}')
        if depth == 0: return source[start:end + 1]
    raise SystemExit('Unclosed source block: ' + signature)


def build_harness(sources):
    fragments = []
    for signature in ('enum NavArea\n', 'enum NavTerrainFlag\n'):
        name = 'src/game/MotionGenerators/MoveMapSharedDefines.h'
        fragments.append((name, exact_block(sources[name], signature) + ';'))
    name = 'src/game/Entities/ObjectGuid.h'
    fragments.append((name, exact_block(sources[name], 'enum TypeID\n') + ';'))
    name = 'src/game/Maps/GridMapDefines.h'
    for signature in ('enum GridMapLiquidStatus\n', 'struct GridMapLiquidData\n'):
        fragments.append((name, exact_block(sources[name], signature) + ';'))
    for macro in ('MAP_LIQUID_TYPE_NO_WATER', 'MAP_LIQUID_TYPE_MAGMA', 'MAP_LIQUID_TYPE_OCEAN',
                  'MAP_LIQUID_TYPE_SLIME', 'MAP_LIQUID_TYPE_WATER', 'MAP_ALL_LIQUIDS'):
        matches = re.findall(r'^#define ' + macro + r'\s+.*$', sources[name], flags=re.M)
        if len(matches) != 1: raise SystemExit('Expected one source macro: ' + macro)
        fragments.append((name, matches[0]))
    declarations = '\n\n'.join(text for _, text in fragments)
    methods = []
    for name, signature, line in METHODS:
        source = sources[name]
        method = exact_block(source, signature)
        if source[:source.index(signature)].count('\n') + 1 != line:
            raise SystemExit('Pinned source location drift: ' + signature)
        fragments.append((name, method)); methods.append(method)
    harness = (HERE / 'route_filter_oracle.cpp').read_text()
    for marker, text in (('// PINNED_DECLARATIONS', declarations), ('// PINNED_METHODS', '\n\n'.join(methods))):
        if harness.count(marker) != 1: raise SystemExit('Expected one harness marker: ' + marker)
        harness = harness.replace(marker, text)
    notices = []
    for name, source in sources.items():
        # GridMapDefines.h has no leading copyright block in the pinned original.
        if source.lstrip().startswith('/*'): notices.append(source[:source.index('*/') + 2])
    package = '// CMaNGOS ' + PIN + '; unchanged source fragments.\n'
    package += '// Test-only scalar observer; no loaded geometry or route admission proof.\n'
    package += '\n\n'.join(notices) + '\n'
    for name, fragment in fragments:
        package += f'\n// BEGIN {name}\n{fragment}\n// END {name}\n'
    return '\n\n'.join(notices) + '\n' + harness, package


def run(command, **kwargs):
    print('+ ' + shlex.join(map(str, command)), file=sys.stderr, flush=True)
    return subprocess.run(command, check=True, **kwargs)


def verify_symbols(symbols):
    for _, signature, _ in METHODS:
        name = signature.split(' ', 1)[1].split('(')[0] + '('
        if name not in symbols: raise SystemExit('Missing unchanged source symbol: ' + name)


def compile_oracle(sources, work, sanitize=False, receipts=None):
    compiler = shutil.which('g++')
    if not compiler: raise SystemExit('Missing required g++ compiler')
    version = run([compiler, '-dumpfullversion'], stdout=subprocess.PIPE).stdout.decode().strip()
    target = run([compiler, '-dumpmachine'], stdout=subprocess.PIPE).stdout.decode().strip()
    if version != '14.2.0' or target != 'x86_64-linux-gnu':
        raise SystemExit('Unsupported audited compiler profile: ' + version + ' ' + target)
    harness, _ = build_harness(sources)
    (work / 'oracle.cpp').write_text(harness, encoding='latin1')
    flags = ['-std=c++20', '-O2', '-fno-inline', '-march=x86-64', '-mno-fma', '-fno-fast-math',
             '-ffp-contract=off', '-Wall', '-Wextra', '-Werror']
    if sanitize: flags += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
    command = [compiler, *flags, str(work / 'oracle.cpp'), '-o', str(work / 'oracle')]
    run(command)
    symbols = run(['nm', '-C', str(work / 'oracle')], stdout=subprocess.PIPE).stdout.decode()
    verify_symbols(symbols)
    if receipts:
        receipts.mkdir(parents=True, exist_ok=True)
        shutil.copy2(work / 'oracle.cpp', receipts / 'oracle.cpp')
        (receipts / 'symbols.txt').write_text(symbols)
        (receipts / 'profile.json').write_text(json.dumps({
            'cmangos_revision': PIN, 'compiler': version, 'target': target,
            'command': command, 'sanitize': sanitize,
            'scope': 'non-PLAYERBOTS creature filter; authored scalar callbacks; no loaded route proof',
        }, indent=2) + '\n')
    return work / 'oracle'


def bits(value): return struct.unpack('<I', struct.pack('<f', value))[0]


def cases():
    rows = []
    def add(label, **changes):
        values = dict(zip(INPUT_FIELDS, (1, 0, 0, 1, bits(3.25), bits(-7.5), bits(11), 0, 0, 0, 0, 0, 0)))
        values.update(changes)
        rows.append((label, tuple(values[field] for field in INPUT_FIELDS)))
    for walk in (0, 1):
        for swim in (0, 1): add(f'dry_capabilities_{walk}_{swim}', walk=walk, swim=swim)
    add('height_enabled_grid_present', height=1)
    add('height_enabled_no_grid', height=1, grid=0)
    add('cannot_check_no_grid', grid=0, status0=8, type0=8, status1=8, type1=8)
    add('signed_zero_owner_position', x=0x80000000, y=0, z=0x80000000)
    for status in (1, 2, 4, 8, 15):
        for kind in (1, 2, 4, 8, 0, 3, 10, 16, 32, 63):
            add(f'first_wet_{status}_type_{kind}', status0=status, type0=kind, status1=8, type1=kind)
    add('first_wet_nav_no_water', status0=4, type0=8, status1=0, type1=63)
    for status in (1, 2, 4, 8, 15):
        for kind in (1, 2, 4, 8, 10):
            add(f'second_status_{status}_type_{kind}', status1=status, type1=kind, status2=8, type2=kind)
    add('second_underwater_nav_no_water', status1=8, type1=8, status2=0, type2=63)
    add('wet_swim_only_initial_flags', walk=0, swim=1, status0=4, type0=8, status1=8, type1=8)
    add('wet_both_capabilities', walk=1, swim=1, status0=4, type0=1, status1=8, type1=1)
    return rows


def observe(binary, rows):
    data = b''.join(struct.pack('<13I', *words) for _, words in rows)
    output = run([str(binary)], input=data, stdout=subprocess.PIPE).stdout.decode().splitlines()
    if len(output) != len(rows): raise SystemExit('Oracle row count differs')
    result = '# Authored scalar callbacks; source observation only; no route/admission proof.\n'
    result += '# ' + '\t'.join(('case', 'label') + INPUT_FIELDS + OUTPUT_FIELDS) + '\n'
    for index, ((label, words), values) in enumerate(zip(rows, output, strict=True)):
        if len(values.split('\t')) != len(OUTPUT_FIELDS): raise SystemExit('Oracle output schema differs')
        fields = [f'{value:08x}' if field in ('x', 'y', 'z') else str(value)
                  for field, value in zip(INPUT_FIELDS, words, strict=True)]
        result += '\t'.join((str(index), label, *fields, values)) + '\n'
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos', type=Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true')
    parser.add_argument('--receipt-dir', type=Path)
    args = parser.parse_args()
    sources = guarded_sources(args.mangos)
    _, package = build_harness(sources)
    if args.check and PACKAGE.read_text(encoding='latin1') != package:
        raise SystemExit('Verbatim route-filter source package differs')
    with tempfile.TemporaryDirectory(prefix='route-filter-source-') as temporary:
        binary = compile_oracle(sources, Path(temporary), args.sanitize, args.receipt_dir)
        fixture = observe(binary, cases())
    if args.check:
        if FIXTURE.read_text() != fixture: raise SystemExit('Route-filter fixture differs')
    else:
        PACKAGE.write_text(package, encoding='latin1')
        FIXTURE.parent.mkdir(parents=True, exist_ok=True); FIXTURE.write_text(fixture)
    print(f'{"Verified" if args.check else "Generated"} route filter: {len(cases())} rows; '
          f'sha256={hashlib.sha256(fixture.encode()).hexdigest()}')


if __name__ == '__main__': main()
