#!/usr/bin/env python3
"""Exact original loaded-mesh warm_cache observer, independent of the production bridge."""
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
MANIFEST = HERE / 'warm_cache_source_hashes.sha256'
PACKAGE = HERE / 'warm_cache_source_fragments.test-fixture'
FIXTURES = HERE / 'fixtures/warm-cache'
FILES = ('precision.test-fixture', 'cases.test-fixture', 'lookups.test-fixture', 'scans.test-fixture', 'traces.test-fixture')
CPP = 'src/game/MotionGenerators/PathFinder.cpp'
HEADER = 'src/game/MotionGenerators/PathFinder.h'
MOVEMAP = 'src/game/MotionGenerators/MoveMap.cpp'
TILE_HEADER = 'dep/recastnavigation/Detour/Include/DetourNavMesh.h'
METHODS = (('dtPolyRef PathFinder::getPathPolyByPosition(', 306),
           ('dtPolyRef PathFinder::getPolyByLocation(', 337))
DETOUR_FILES = ('DetourAlloc', 'DetourAssert', 'DetourCommon', 'DetourNavMesh',
                'DetourNavMeshBuilder', 'DetourNavMeshQuery', 'DetourNode')
FIELDS = ('label geometry scope from to limit before_active before_storage start_ref start_distance start_origin start_appended '
          'end_ref end_distance end_origin end_appended phase direct_status direct_nodes after_active after_storage').split()
LOOKUP_FIELDS = ('label geometry scope point limit before_active before_storage selected distance origin appended active_ref active_distance '
                 'tail_checked tail_ref tail_distance near5_checked near5_status near5_ref near10_checked near10_status near10_ref after_storage').split()
SCAN_FIELDS = 'label geometry point max_distance refs selected_ref distance squared_distances'.split()
TRACE_FIELDS = 'label endpoint ordinal primitive polygon status distance_bits'.split()
SOURCE_FILES = {CPP, HEADER, MOVEMAP, 'LICENSE', 'dep/recastnavigation/License.txt'} | {
    f'dep/recastnavigation/Detour/Source/{name}.cpp' for name in DETOUR_FILES} | {
    f'dep/recastnavigation/Detour/Include/{name}.h' for name in (*DETOUR_FILES, 'DetourMath', 'DetourStatus')}
INPUT_FILES = {'authored.mmap', '0003232.mmtile', '0003231.mmtile'} | {
    'direct-corridor/'+name for name in ('params.test-fixture', 'long600.test-fixture', 'long1100.test-fixture',
    'grid00.test-fixture', 'grid10.test-fixture', 'grid01.test-fixture', 'grid11.test-fixture')}


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
    for name, expected in manifest_rows(HERE / 'warm_cache_input_hashes.sha256', INPUT_FILES).items():
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
    if repair_lifetime: flags += ['-DWARM_CACHE_REPAIR_TILE_LIFETIME=1']
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
            'scope': 'unchanged complete warm lookup methods; scoped selected-reference storage branch fragments; actual original loaded Detour and direct findPath1024',
            'excluded': ['uninitialized cold lookup', 'complete BuildPolyPath', 'BuildShortcut/NormalizePath', 'BuildPointPath', 'OwnerPathFinder', 'gameplay admission'],
            'source_hashes_sha256': hashlib.sha256(MANIFEST.read_bytes()).hexdigest(),
            'authored_input_hashes_sha256': hashlib.sha256((HERE / 'warm_cache_input_hashes.sha256').read_bytes()).hexdigest(),
            'harness_sha256': hashlib.sha256((work / 'oracle.cpp').read_bytes()).hexdigest(),
        }, indent=2) + '\n')
    return work / 'oracle'



def source_span(source, start, end):
    if source.count(start) != 1 or source.count(end) != 1:
        raise SystemExit('Expected unique unchanged branch bounds')
    a, b = source.index(start), source.index(end)
    if b <= a: raise SystemExit('Malformed unchanged branch bounds')
    return source[a:b]


def build_harness(sources):
    source, header = sources[CPP], sources[HEADER]
    fragments = []
    for signature, line in METHODS:
        if source.count(signature) != 1 or source[:source.index(signature)].count('\n')+1 != line:
            raise SystemExit('Pinned source location drift: ' + signature)
        fragments.append((CPP, exact_block(source, signature)))
    clear = exact_block(header, 'void clear()')
    bounds = source_span(header, 'static float NearPolySearchBound', '\nenum PathType')
    declarations = source_span(header, '        dtPolyRef getPathPolyByPosition(', '        bool HaveTile(')
    limits = source_span(header, '#ifdef ENABLE_PLAYERBOTS\n// This value is doubled', '\n#define SMOOTH_PATH_STEP_SIZE')
    if limits.count('#define MAX_POINT_PATH_LENGTH   74')!=1 or 'const float maxDist = 3.0f' not in declarations:
        raise SystemExit('Source warm lookup bounds differ')
    scan_cut = source_span(source, '    // look for startPoly/endPoly in current path', '    //else if (startPolyFound && !endPolyFound)')
    far = source_span(source, '    bool farFromPoly = ', '    if (farFromPoly)')
    same = source_span(source, '        m_pathPolyRefs[0] = startPoly;', '\n        m_type = farFromPoly ?')
    direct = source_span(source, '            dtResult = m_navMeshQuery->findPath(\n                    startPoly,', '        }\n        else\n        {\n            float hit = 0.0f;')
    original_tile = exact_block(sources[TILE_HEADER], 'struct dtMeshTile\n{')
    original_layout = original_tile.replace('struct dtMeshTile', 'struct OriginalTileLayout', 1)
    original_layout = original_layout.replace('dtMeshTile(const dtMeshTile&)', 'OriginalTileLayout(const OriginalTileLayout&)')
    original_layout = original_layout.replace('dtMeshTile& operator=(const dtMeshTile&)', 'OriginalTileLayout& operator=(const OriginalTileLayout&)')+';'
    harness = (HERE/'warm_cache_oracle.cpp').read_text()
    inserts = {'// ORIGINAL_TILE_LAYOUT': original_layout, '// ORIGINAL_SEARCH_BOUNDS': bounds,
               '// ORIGINAL_FAR_PREDICATE': far, '// ORIGINAL_CLEAR': clear, '// ORIGINAL_LOOKUP_DECLARATIONS': declarations, '// ORIGINAL_SAME_ASSIGNMENT': same,
               '// ORIGINAL_SCAN_AND_CUT': scan_cut, '// ORIGINAL_DIRECT_CALL': direct,
               '// ORIGINAL_LOOKUP_METHODS': '\n\n'.join(fragment for _,fragment in fragments)}
    for marker, value in inserts.items():
        if harness.count(marker)!=1: raise SystemExit('Expected one harness marker: '+marker)
        harness=harness.replace(marker,value)
    # Preserve complete methods and the surrounding complete source methods as review
    # evidence; compilation uses only the explicitly bounded storage fragments above.
    fragments += [(HEADER,clear),(HEADER,bounds),(HEADER,declarations),(HEADER,limits),(CPP,far),(CPP,scan_cut),(CPP,same),(CPP,direct),
                  (CPP,exact_block(source,'void PathFinder::BuildPolyPath(')),
                  (CPP,exact_block(source,'void PathFinder::BuildShortcut(')),
                  (CPP,exact_block(source,'PathFinder::PathFinder(const Unit* owner,')),
                  (MOVEMAP,exact_block(sources[MOVEMAP],'bool MMapManager::loadMapInstance('))]
    if fragments[-1][1].count('query->init(mmap->navMesh, 1024)')!=1:
        raise SystemExit('Source map node pool differs')
    notices = [source[:source.index('*/')+2],sources['dep/recastnavigation/Detour/Source/DetourNavMeshQuery.cpp'].split('#include',1)[0].rstrip()]
    package = '// CMaNGOS '+PIN+'; complete unchanged warm lookup methods; scoped storage fragments.\n'
    package += '// Full BuildPolyPath/BuildShortcut are review context, NOT compiled gameplay or point-path evidence.\n'
    package += '\n\n'.join(notices)+'\n'
    for name,fragment in fragments: package += f'\n// BEGIN {name}\n{fragment}\n// END {name}\n'
    package += '\n/* Original CMaNGOS LICENSE\n'+sources['LICENSE']+'\n*/\n'
    package += '\n/* Original Detour License.txt\n'+sources['dep/recastnavigation/License.txt']+'\n*/\n'
    return '\n\n'.join(notices)+'\n'+harness,package


def parse_table(data,fields):
    lines=data.decode('ascii').splitlines()
    if lines.count('# '+'\t'.join(fields))!=1: raise SystemExit('Warm-cache oracle schema differs')
    result=[];labels=set()
    for line in lines:
        if line.startswith('#'): continue
        values=line.split('\t')
        if len(values)!=len(fields): raise SystemExit('Warm-cache oracle schema differs')
        row=dict(zip(fields,values,strict=True))
        if not re.fullmatch('[a-z0-9_]+',row['label']): raise SystemExit('Invalid oracle label')
        key=(row['label'],row.get('endpoint'),row.get('ordinal'))
        if key in labels: raise SystemExit('Duplicate oracle label')
        labels.add(key);result.append(row)
    if not result: raise SystemExit('Empty oracle cases')
    return result


def ref_list(value,empty=False):
    if value=='-' and empty:return []
    result=[int(ref) for ref in value.split(',')]
    if any(not 0<=ref<(1<<64) for ref in result) or len(result)>4096:raise ValueError()
    return result


def finite_bits(value):
    if not re.fullmatch('[0-9a-f]{8}',value):raise ValueError()
    result=struct.unpack('!f',bytes.fromhex(value))[0]
    if not math.isfinite(result):raise ValueError()
    return result


def parse_cases(data):
    rows=parse_table(data,FIELDS)
    for row in rows:
        try:
            if row['geometry'] not in {'precision','existing','grid','long600','long1100'}:raise ValueError()
            if row['scope'] not in {'runtime','raw','excluded','helper'}:raise ValueError()
            limit=int(row['limit']);before=ref_list(row['before_storage']);after=ref_list(row['after_storage'])
            active=int(row['before_active']);end_active=int(row['after_active'])
            if not 1<=limit<=74 or not 0<=active<=limit or not 0<=end_active<=limit:raise ValueError()
            if len(before)<limit or len(after)<limit or any(not ref for ref in before[:active]) or any(not ref for ref in after[:end_active]):raise ValueError()
            if row['scope']!='helper' and limit!=74:raise ValueError()
            for name in ('from','to'):
                if len(row[name].split(','))!=3:raise ValueError()
                for word in row[name].split(','):finite_bits(word)
            for prefix in ('start','end'):
                if not 0<=int(row[prefix+'_ref'])<(1<<64):raise ValueError()
                if finite_bits(row[prefix+'_distance'])<0:raise ValueError()
                if row[prefix+'_origin'] not in {'active','tail','nearest5','nearest10','miss','excluded'}:raise ValueError()
                if row[prefix+'_appended'] not in {'0','1'}:raise ValueError()
            if row['phase'] not in {'same','cut','replacement','excluded'}:raise ValueError()
            if not 0<=int(row['direct_nodes'])<=1024:raise ValueError()
            if row['phase']=='replacement':
                if not re.fullmatch('[0-9a-f]{8}',row['direct_status']):raise ValueError()
                if row['scope']=='runtime' and row['direct_status']!='40000000':raise ValueError()
            elif row['direct_status']!='-':raise ValueError()
        except (ValueError,OverflowError):raise SystemExit('Invalid oracle row: '+row['label']) from None
    return rows



def parse_lookups(data):
    rows=parse_table(data,LOOKUP_FIELDS)
    for row in rows:
        try:
            if row['geometry']!='precision' or row['scope'] not in {'runtime','raw','excluded','helper'}:raise ValueError()
            limit=int(row['limit']);active=int(row['before_active']);before=ref_list(row['before_storage']);after=ref_list(row['after_storage'])
            if not 1<=limit<=74 or not 0<=active<=limit or len(before)<limit or any(not r for r in before[:active]):raise ValueError()
            if row['scope']!='helper' and limit!=74:raise ValueError()
            if len(row['point'].split(','))!=3:raise ValueError()
            for word in row['point'].split(','):finite_bits(word)
            for name in ('distance','active_distance','tail_distance'):
                if finite_bits(row[name])<0:raise ValueError()
            for name in ('appended','tail_checked','near5_checked','near10_checked'):
                if row[name] not in {'0','1'}:raise ValueError()
            for name in ('selected','active_ref','tail_ref','near5_ref','near10_ref'):
                if not 0<=int(row[name])<(1<<64):raise ValueError()
            for name in ('near5_status','near10_status'):
                if not re.fullmatch('[0-9a-f]{8}',row[name]):raise ValueError()
            if row['origin'] not in {'active','tail','nearest5','nearest10','miss','excluded'}:raise ValueError()
            expected=before+([int(row['selected'])] if row['appended']=='1' else [])
            if after!=expected or (row['appended']=='1' and row['selected']=='0'):raise ValueError()
            if row['origin']=='excluded' and row['scope']!='excluded':raise ValueError()
        except (ValueError,OverflowError):raise SystemExit('Invalid oracle row: '+row['label']) from None
    return rows


def parse_scans(data):
    rows=parse_table(data,SCAN_FIELDS)
    for row in rows:
        try:
            if row['geometry']!='precision':raise ValueError()
            if len(row['point'].split(','))!=3:raise ValueError()
            for word in row['point'].split(','):finite_bits(word)
            if finite_bits(row['max_distance']) not in {3,7}:raise ValueError()
            if finite_bits(row['distance'])<0:raise ValueError()
            refs=ref_list(row['refs']);distances=row['squared_distances'].split(',')
            if not refs or len(refs)!=len(distances):raise ValueError()
            if not 0<=int(row['selected_ref'])<(1<<64):raise ValueError()
            for word in distances:
                if word!='-' and finite_bits(word)<0:raise ValueError()
        except (ValueError,OverflowError):raise SystemExit('Invalid oracle row: '+row['label']) from None
    return rows


def parse_traces(data):
    rows=parse_table(data,TRACE_FIELDS);next_ordinal={}
    for row in rows:
        try:
            if row['endpoint'] not in {'0','1'}:raise ValueError()
            if row['primitive'] not in {'active','tail','nearest5','nearest10'}:raise ValueError()
            if not 0<=int(row['polygon'])<(1<<64):raise ValueError()
            if not re.fullmatch('[0-9a-f]{8}',row['status']):raise ValueError()
            if finite_bits(row['distance_bits'])<0:raise ValueError()
            key=(row['label'],row['endpoint']);expected=next_ordinal.get(key,0)
            if int(row['ordinal'])!=expected or expected>8194:raise ValueError()
            next_ordinal[key]=expected+1
        except (ValueError,OverflowError):raise SystemExit('Invalid oracle row: '+row['label']) from None
    return rows


def observe(binary,work):
    if len(set(FILES))!=len(FILES) or any(Path(name).name!=name or not name.endswith('.test-fixture') for name in FILES):
        raise SystemExit('Unsafe fixture output filename')
    output=work/'output';output.mkdir(exist_ok=True)
    run([str(binary),str(output),str(HERE/'fixtures')])
    if {path.name for path in output.iterdir()}!=set(FILES):raise SystemExit('Unexpected oracle output files')
    result={name:(output/name).read_bytes() for name in FILES}
    parse_cases(result['cases.test-fixture'])
    parse_lookups(result['lookups.test-fixture'])
    parse_scans(result['scans.test-fixture'])
    parse_traces(result['traces.test-fixture'])
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos',type=Path);parser.add_argument('--check',action='store_true')
    parser.add_argument('--sanitize',action='store_true');parser.add_argument('--repair-tile-lifetime',action='store_true')
    parser.add_argument('--receipt-dir',type=Path);args=parser.parse_args()
    sources=guarded_sources(args.mangos);guard_inputs();_,package=build_harness(sources)
    if args.check and PACKAGE.read_text(encoding='latin1')!=package:raise SystemExit('Verbatim warm-cache source package differs')
    with tempfile.TemporaryDirectory(prefix='warm-cache-source-') as temporary:
        work=Path(temporary);binary=compile_oracle(sources,work,args.sanitize,args.receipt_dir,args.repair_tile_lifetime)
        fixtures=observe(binary,work)
    if args.check:
        for name,data in fixtures.items():
            if (FIXTURES/name).read_bytes()!=data:raise SystemExit('Warm-cache fixture differs: '+name)
    else:
        PACKAGE.write_text(package,encoding='latin1');FIXTURES.mkdir(parents=True,exist_ok=True)
        for name,data in fixtures.items():(FIXTURES/name).write_bytes(data)
    for name,data in fixtures.items():print(f'{"Verified" if args.check else "Generated"} {name}: {len(data)} bytes; sha256={hashlib.sha256(data).hexdigest()}')
    if args.receipt_dir:(args.receipt_dir/'output_hashes.json').write_text(json.dumps({name:hashlib.sha256(data).hexdigest() for name,data in fixtures.items()},indent=2)+'\n')


if __name__=='__main__':main()
