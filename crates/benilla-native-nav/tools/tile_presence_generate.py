#!/usr/bin/env python3
"""Exact original loaded-mesh HaveTile observer, independent of the production bridge."""
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
MANIFEST = HERE / 'tile_presence_source_hashes.sha256'
PACKAGE = HERE / 'tile_presence_source_fragments.test-fixture'
FIXTURES = HERE / 'fixtures/tile-presence'
FILES = ('cases.test-fixture', 'params-minus-one.test-fixture', 'tile-minus-one.test-fixture',
         'params-minus-two.test-fixture', 'tile-minus-two.test-fixture', 'tile-layer-one.test-fixture')
CPP = 'src/game/MotionGenerators/PathFinder.cpp'
HEADER = 'src/game/MotionGenerators/PathFinder.h'
GRID = 'src/game/Maps/GridDefines.h'
TILE_HEADER = 'dep/recastnavigation/Detour/Include/DetourNavMesh.h'
METHODS = (('bool PathFinder::HaveTile(', 1122),)
COORDINATES = (('inline bool IsValidMapCoord(float c)', 177),
               ('inline bool IsValidMapCoord(float x, float y)', 182),
               ('inline bool IsValidMapCoord(float x, float y, float z)', 187))
DETOUR_FILES = ('DetourAlloc', 'DetourAssert', 'DetourCommon', 'DetourNavMesh',
                'DetourNavMeshBuilder', 'DetourNavMeshQuery', 'DetourNode')
FIELDS = ('label geometry scope from to destination_valid start_valid pair_valid native_safe '
          'start_checked start_tx start_ty start_layer0 start_have end_checked end_tx end_ty '
          'end_layer0 end_have pair_have nearest_status nearest_ref').split()
SOURCE_FILES = {CPP, HEADER, GRID, 'LICENSE', 'dep/recastnavigation/License.txt'} | {
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
    for name, expected in manifest_rows(HERE / 'tile_presence_input_hashes.sha256', INPUT_FILES).items():
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


def run(command, **kwargs):
    print('+ ' + shlex.join(map(str, command)), file=sys.stderr, flush=True)
    return subprocess.run(command, check=True, **kwargs)


def verify_symbols(symbols):
    for name in ('PathFinder::HaveTile(', 'MaNGOS::IsValidMapCoord(float)', 'MaNGOS::IsValidMapCoord(float, float)', 'MaNGOS::IsValidMapCoord(float, float, float)'):
        if name not in symbols: raise SystemExit('Missing unchanged source symbol: ' + name)
    for name in ('dtNavMesh::calcTileLoc(', 'dtNavMesh::getTileAt(', 'dtNavMeshQuery::findNearestPoly('):
        if name not in symbols: raise SystemExit('Missing actual Detour symbol: ' + name)
    if 'benilla_' in symbols or 'benilla::' in symbols: raise SystemExit('Production bridge linked into source observer')


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
             '-ffp-contract=off', '-frounding-math', '-DDT_POLYREF64=1', '-Wall', '-Wextra', '-Werror', '-Wno-class-memaccess']
    if repair_lifetime: flags += ['-DTILE_PRESENCE_REPAIR_TILE_LIFETIME=1']
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
            'command': command, 'sanitize': sanitize, 'query_nodes': [1024],
            'tile_lifetime_profile': 'default-constructor-only-repaired' if repair_lifetime else 'unchanged-original-empirical',
            'observed_tile_header_sha256': hashlib.sha256((work / TILE_HEADER).read_bytes()).hexdigest(),
            'tile_lifetime_amendment': 'insert dtMeshTile() = default; only' if repair_lifetime else None,
            'scope': 'unchanged complete HaveTile and IsValidMapCoord scalar/two/three-axis methods; actual original loaded Detour; explicitly authored nontransport wrapper',
            'excluded': ['full calculate', 'source owner construction', 'transport ownership', 'BuildPolyPath', 'gameplay admission'],
            'floating_environment': 'FE_TONEAREST, MXCSR FTZ=0/DAZ=0/rounding=nearest; deliberate mutation negative tests',
            'source_hashes_sha256': hashlib.sha256(MANIFEST.read_bytes()).hexdigest(),
            'authored_input_hashes_sha256': hashlib.sha256((HERE / 'tile_presence_input_hashes.sha256').read_bytes()).hexdigest(),
            'harness_sha256': hashlib.sha256((work / 'oracle.cpp').read_bytes()).hexdigest(),
        }, indent=2) + '\n')
    return work / 'oracle'


def build_harness(sources):
    fragments=[]
    for name,methods in ((CPP,METHODS),(GRID,COORDINATES)):
        source=sources[name]
        for signature,line in methods:
            if source.count(signature)!=1 or source[:source.index(signature)].count('\n')+1!=line:
                raise SystemExit('Pinned source location drift: '+signature)
            fragments.append((name,exact_block(source,signature)))
    defines=[]
    for name in ('MAX_NUMBER_OF_GRIDS','SIZE_OF_GRIDS','MAP_SIZE','MAP_HALFSIZE'):
        matches=re.findall(r'^#define '+name+r'\s+[^\n]+',sources[GRID],flags=re.M)
        if len(matches)!=1:raise SystemExit('Coordinate define drift: '+name)
        defines.append(matches[0])
    original_tile=exact_block(sources[TILE_HEADER],'struct dtMeshTile\n{')
    layout=original_tile.replace('struct dtMeshTile','struct OriginalTileLayout',1)
    layout=layout.replace('dtMeshTile(const dtMeshTile&)','OriginalTileLayout(const OriginalTileLayout&)')
    layout=layout.replace('dtMeshTile& operator=(const dtMeshTile&)','OriginalTileLayout& operator=(const OriginalTileLayout&)')+';'
    harness=(HERE/'tile_presence_oracle.cpp').read_text()
    inserts={'// ORIGINAL_TILE_LAYOUT':layout,'// ORIGINAL_HAVE_TILE':fragments[0][1],
             '// ORIGINAL_COORDINATE_METHODS':'\n\n'.join(text for _,text in fragments[1:]),
             '// ORIGINAL_COORDINATE_DEFINES':'\n'.join(defines)}
    for marker,value in inserts.items():
        if harness.count(marker)!=1:raise SystemExit('Expected one harness marker: '+marker)
        harness=harness.replace(marker,value)
    source=sources[CPP]
    start=source.index('    if (!MaNGOS::IsValidMapCoord(dest.x, dest.y, dest.z))')
    end=source.index('\n#ifndef ENABLE_PLAYERBOTS',start)
    context=source[start:end]
    if context.count('IsValidMapCoord(')!=2:raise SystemExit('Destination-first validity context drift')
    fragments += [(GRID,'\n'.join(defines)),(CPP,context),(CPP,exact_block(source,'bool PathFinder::calculate(Vector3 const&')),
                  (TILE_HEADER,original_tile)]
    notices=[sources[name][:sources[name].index('*/')+2] for name in (CPP,GRID)]
    notices.append(sources['dep/recastnavigation/Detour/Source/DetourNavMesh.cpp'].split('#include',1)[0].rstrip())
    package='// CMaNGOS '+PIN+'; complete unchanged HaveTile/coordinate methods.\n'
    package+='// Complete calculate is review context only, never compiled; no source owner is constructed.\n'
    package+='\n\n'.join(notices)+'\n'
    for name,fragment in fragments:package+=f'\n// BEGIN {name}\n{fragment}\n// END {name}\n'
    package+='\n/* Original CMaNGOS LICENSE\n'+sources['LICENSE']+'\n*/\n'
    package+='\n/* Original Detour License.txt\n'+sources['dep/recastnavigation/License.txt']+'\n*/\n'
    return '\n\n'.join(notices)+'\n'+harness,package


def parse_cases(data):
    try:lines=data.decode('ascii').splitlines()
    except UnicodeError:raise SystemExit('Tile-presence oracle schema differs') from None
    header='# '+'\t'.join(FIELDS)
    if lines.count(header)!=1:raise SystemExit('Tile-presence oracle schema differs')
    result=[];labels=set()
    for line in lines:
        if line.startswith('#'):continue
        values=line.split('\t')
        if len(values)!=len(FIELDS):raise SystemExit('Tile-presence oracle schema differs')
        row=dict(zip(FIELDS,values,strict=True));label=row['label']
        try:
            if not re.fullmatch('[a-z0-9_]+',label) or label in labels:raise ValueError()
            labels.add(label)
            if row['geometry'] not in {'both','left','right','empty','minus_one','minus_two','layer_one'}:raise ValueError()
            if row['scope'] not in {'runtime','raw'}:raise ValueError()
            for field in ('from','to'):
                words=row[field].split(',')
                if len(words)!=3 or any(not re.fullmatch('[0-9a-f]{8}',word) for word in words):raise ValueError()
            for field in ('destination_valid','pair_valid','native_safe','start_checked','end_checked','pair_have'):
                if row[field] not in {'0','1'}:raise ValueError()
            if row['start_valid'] not in ({'-'} if row['destination_valid']=='0' else {'0','1'}):raise ValueError()
            if row['pair_valid']!=str(int(row['destination_valid']=='1' and row['start_valid']=='1')):raise ValueError()
            checked=row['pair_valid']=='1' and (row['native_safe']=='1' or row['scope']=='raw')
            if row['start_checked']!=str(int(checked)):raise ValueError()
            if row['end_checked']!=str(int(row['start_have']=='1')):raise ValueError()
            for prefix in ('start','end'):
                fields=[prefix+'_'+suffix for suffix in ('tx','ty','layer0','have')]
                if row[prefix+'_checked']=='0':
                    if any(row[field]!='-' for field in fields):raise ValueError()
                else:
                    for field in fields[:2]:
                        if not re.fullmatch('-?(0|[1-9][0-9]*)',row[field]) or not -(1<<31)<=int(row[field])<(1<<31):raise ValueError()
                    if any(row[field] not in {'0','1'} for field in fields[2:]):raise ValueError()
                    expected=row[prefix+'_layer0']=='1' and row[prefix+'_tx']!='-1' and row[prefix+'_ty']!='-1'
                    if row[prefix+'_have']!=str(int(expected)):raise ValueError()
            if row['pair_have']!=str(int(row['start_have']=='1' and row['end_have']=='1')):raise ValueError()
            if row['nearest_status']=='-':
                if row['nearest_ref']!='-':raise ValueError()
            elif not re.fullmatch('[0-9a-f]{8}',row['nearest_status']) or not re.fullmatch('[0-9]+',row['nearest_ref']) or not 0<int(row['nearest_ref'])<(1<<64):raise ValueError()
        except (ValueError,OverflowError):raise SystemExit('Invalid oracle row: '+label) from None
        result.append(row)
    if not result:raise SystemExit('Empty oracle cases')
    return result


def observe(binary,work):
    if len(set(FILES))!=len(FILES) or any(Path(name).name!=name or not name.endswith('.test-fixture') for name in FILES):
        raise SystemExit('Unsafe fixture output filename')
    output=work/'output';output.mkdir(exist_ok=True)
    run([str(binary),str(output),str(HERE/'fixtures')])
    if {path.name for path in output.iterdir()}!=set(FILES):raise SystemExit('Unexpected oracle output files')
    result={name:(output/name).read_bytes() for name in FILES}
    parse_cases(result['cases.test-fixture'])
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos',type=Path);parser.add_argument('--check',action='store_true')
    parser.add_argument('--sanitize',action='store_true');parser.add_argument('--repair-tile-lifetime',action='store_true')
    parser.add_argument('--receipt-dir',type=Path);args=parser.parse_args()
    sources=guarded_sources(args.mangos);guard_inputs();_,package=build_harness(sources)
    if args.check and PACKAGE.read_text(encoding='latin1')!=package:raise SystemExit('Verbatim tile-presence source package differs')
    with tempfile.TemporaryDirectory(prefix='tile-presence-source-') as temporary:
        work=Path(temporary);binary=compile_oracle(sources,work,args.sanitize,args.receipt_dir,args.repair_tile_lifetime)
        fixtures=observe(binary,work)
    if args.check:
        for name,data in fixtures.items():
            if (FIXTURES/name).read_bytes()!=data:raise SystemExit('Tile-presence fixture differs: '+name)
    else:
        PACKAGE.write_text(package,encoding='latin1');FIXTURES.mkdir(parents=True,exist_ok=True)
        for name,data in fixtures.items():(FIXTURES/name).write_bytes(data)
    for name,data in fixtures.items():print(f'{"Verified" if args.check else "Generated"} {name}: {len(data)} bytes; sha256={hashlib.sha256(data).hexdigest()}')
    if args.receipt_dir:
        for manifest in (MANIFEST,HERE/'tile_presence_input_hashes.sha256'):shutil.copy2(manifest,args.receipt_dir/manifest.name)
        (args.receipt_dir/'output_hashes.json').write_text(json.dumps({name:hashlib.sha256(data).hexdigest() for name,data in fixtures.items()},indent=2)+'\n')


if __name__=='__main__':main()
