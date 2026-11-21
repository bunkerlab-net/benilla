#!/usr/bin/env python3
"""Exact original loaded-mesh point_path observer, independent of the production bridge."""
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
MANIFEST = HERE / 'point_path_source_hashes.sha256'
PACKAGE = HERE / 'point_path_source_fragments.test-fixture'
FIXTURES = HERE / 'fixtures/point-path'
FILES = ('cases.test-fixture', 'postprocess.test-fixture')
CPP = 'src/game/MotionGenerators/PathFinder.cpp'
HEADER = 'src/game/MotionGenerators/PathFinder.h'
MOVEMAP = 'src/game/MotionGenerators/MoveMap.cpp'
TILE_HEADER = 'dep/recastnavigation/Detour/Include/DetourNavMesh.h'
METHODS = (('dtPolyRef PathFinder::getPathPolyByPosition(',306),
           ('dtPolyRef PathFinder::getPolyByLocation(',337),
           ('void PathFinder::BuildPointPath(',795),
           ('void PathFinder::NormalizePath(',988),
           ('void PathFinder::BuildShortcut(',1005),
           ('uint32 PathFinder::fixupCorridor(',1141),
           ('bool PathFinder::getSteerTarget(',1189),
           ('dtStatus PathFinder::findSmoothPath(',1226),
           ('bool PathFinder::inRangeYZX(',1438),
           ('bool PathFinder::inRange(',1446),
           ('float PathFinder::dist3DSqr(',1452))
DETOUR_FILES = ('DetourAlloc', 'DetourAssert', 'DetourCommon', 'DetourNavMesh',
                'DetourNavMeshBuilder', 'DetourNavMeshQuery', 'DetourNode')
FIELDS = ('label geometry from to before_active before_storage optimize normalize ignore force previous_type phase '
          'selected_active selected_storage smooth_status smooth_points after_active after_storage path_type actual_end points').split()
RAW_FIELDS = ('label limit optimize normalize ignore force previous_type from to prior_points raw_status raw_points '
              'before_active before_storage after_active after_storage path_type actual_end points').split()
SOURCE_FILES = {CPP, HEADER, MOVEMAP, 'LICENSE', 'dep/recastnavigation/License.txt'} | {
    f'dep/recastnavigation/Detour/Source/{name}.cpp' for name in DETOUR_FILES} | {
    f'dep/recastnavigation/Detour/Include/{name}.h' for name in (*DETOUR_FILES, 'DetourMath', 'DetourStatus')}
G3D_FILES = ('dep/g3dlite/G3D/Array.h', 'dep/g3dlite/G3D/BinaryFormat.h', 'dep/g3dlite/G3D/Crypto.h', 'dep/g3dlite/G3D/EqualsTrait.h', 'dep/g3dlite/G3D/G3DGameUnits.h', 'dep/g3dlite/G3D/HashTrait.h', 'dep/g3dlite/G3D/MemoryManager.h', 'dep/g3dlite/G3D/PositionTrait.h', 'dep/g3dlite/G3D/Random.h', 'dep/g3dlite/G3D/ReferenceCount.h', 'dep/g3dlite/G3D/System.h', 'dep/g3dlite/G3D/Table.h', 'dep/g3dlite/G3D/Vector2.h', 'dep/g3dlite/G3D/Vector2int16.h', 'dep/g3dlite/G3D/Vector3.h', 'dep/g3dlite/G3D/debug.h', 'dep/g3dlite/G3D/debugAssert.h', 'dep/g3dlite/G3D/debugPrintf.h', 'dep/g3dlite/G3D/format.h', 'dep/g3dlite/G3D/g3dmath.h', 'dep/g3dlite/G3D/platform.h', 'dep/g3dlite/G3D/uint128.h', 'dep/g3dlite/license.cpp', 'src/game/Movement/MoveSplineInitArgs.h', 'src/game/World/World.h')
SOURCE_FILES |= set(G3D_FILES)
INPUT_FILES = {'authored.mmap', '0003232.mmtile', '0003231.mmtile',
               'direct-corridor/params.test-fixture', 'warm-cache/precision.test-fixture',
               'smoothing/params.test-fixture', 'smoothing/ground.test-fixture', 'smoothing/thin.test-fixture'}


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
    for name, expected in manifest_rows(HERE / 'point_path_input_hashes.sha256', INPUT_FILES).items():
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
    for name in ('dtNavMeshQuery::findPath(', 'dtNavMeshQuery::findStraightPath(', 'dtNavMeshQuery::moveAlongSurface(',
                 'G3D::Vector3::Vector3()', 'G3D::Vector3::squaredLength() const', 'dtCreateNavMeshData('):
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
        if name.startswith(('dep/recastnavigation/', 'dep/g3dlite/')):
            dest = work / name; dest.parent.mkdir(parents=True, exist_ok=True)
            if repair_lifetime and name == TILE_HEADER: content = repair_tile_header(content)
            dest.write_text(content, encoding='latin1')
    detour = work / 'dep/recastnavigation/Detour'
    flags = ['-std=c++20', '-O2', '-fno-inline', '-march=x86-64', '-mno-fma', '-fno-fast-math',
             '-ffp-contract=off', '-DNDEBUG', '-DDT_POLYREF64=1', '-Wall', '-Wextra', '-Werror', '-Wno-class-memaccess', '-Wno-deprecated-copy']
    if repair_lifetime: flags += ['-DPOINT_PATH_REPAIR_TILE_LIFETIME=1']
    if sanitize: flags += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
    command = [compiler, *flags, '-I' + str(detour / 'Include'), '-I' + str(work/'dep/g3dlite'), str(work / 'oracle.cpp'),
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
            'scope': 'complete original warm lookups, smoothing and BuildPointPath; selected-reference transition fragments; separate raw postprocess controls; actual G3D Vector3',
            'excluded': ['uninitialized cold lookup', 'complete BuildPolyPath', 'active NormalizePath world callbacks', 'OwnerPathFinder', 'gameplay admission'],
            'source_hashes_sha256': hashlib.sha256(MANIFEST.read_bytes()).hexdigest(),
            'authored_input_hashes_sha256': hashlib.sha256((HERE / 'point_path_input_hashes.sha256').read_bytes()).hexdigest(),
            'harness_sha256': hashlib.sha256((work / 'oracle.cpp').read_bytes()).hexdigest(),
        }, indent=2) + '\n')
    return work / 'oracle'



def source_span(source, start, end):
    if source.count(start) != 1 or source.count(end) != 1:
        raise SystemExit('Expected unique unchanged branch bounds')
    a,b=source.index(start),source.index(end)
    if b<=a:raise SystemExit('Malformed unchanged branch bounds')
    return source[a:b]


def build_harness(sources):
    source,header=sources[CPP],sources[HEADER]
    fragments=[]
    for signature,line in METHODS:
        if source.count(signature)!=1 or source[:source.index(signature)].count('\n')+1!=line:
            raise SystemExit('Pinned source location drift: '+signature)
        fragments.append((CPP,exact_block(source,signature)))
    methods='\n\n'.join(fragment for _,fragment in fragments)
    constants=source_span(header,'#ifdef ENABLE_PLAYERBOTS\n// This value is doubled','\n// bound box of poly search area')
    if '#define MAX_POINT_PATH_LENGTH   74' not in constants or '#define SKIP_POINT_LIMIT        6' not in constants or '#define LINE_FAULT              0.5f' not in constants:
        raise SystemExit('Source point-path bounds differ')
    getters=source_span(header,'        Vector3 getStartPosition()', '\n        PointsArray& getPath()')
    setters=source_span(header,'        void setStartPosition(', '\n        void NormalizePath();')
    clear=exact_block(header,'void clear()')
    bounds=source_span(header,'static float NearPolySearchBound','\nenum PathType')
    types=exact_block(header,'enum PathType')+';'
    declarations=source_span(header,'        dtPolyRef getPathPolyByPosition(', '        bool HaveTile(')
    same=source_span(source,'    // start and end are on same polygon','    // look for startPoly/endPoly in current path')
    scan=source_span(source,'    // look for startPoly/endPoly in current path','    //else if (startPolyFound && !endPolyFound)')
    direct=source_span(source,'            dtResult = m_navMeshQuery->findPath(\n                    startPoly,','        }\n        else\n        {\n            float hit = 0.0f;')
    classification=source_span(source,'    // by now we know what type of path we can get','    // generate the point-path out of our up-to-date poly-path')
    optimize=source_span(source,'        if (pointCount > 2 && sWorld.getConfig(CONFIG_BOOL_PATH_FIND_OPTIMIZE))','    }\n\n    if (pointCount < 2 || dtStatusFailed(dtResult))')
    terminal=source_span(source,'    if (pointCount < 2 || dtStatusFailed(dtResult))','\nvoid PathFinder::NormalizePath()').rstrip()
    if not terminal.endswith('}'):raise SystemExit('Original terminal bounds differ')
    terminal=terminal[:-1] # closing method brace, supplied by raw observer method
    tile=exact_block(sources[TILE_HEADER],'struct dtMeshTile\n{')
    layout=tile.replace('struct dtMeshTile','struct OriginalTileLayout',1).replace('dtMeshTile(const dtMeshTile&)','OriginalTileLayout(const OriginalTileLayout&)').replace('dtMeshTile& operator=(const dtMeshTile&)','OriginalTileLayout& operator=(const OriginalTileLayout&)')+';'
    harness=(HERE/'point_path_oracle.cpp').read_text()
    inserts={'// ORIGINAL_TILE_LAYOUT':layout,'// ORIGINAL_CONSTANTS':constants,'// ORIGINAL_PATH_TYPE':types,
             '// ORIGINAL_SEARCH_BOUNDS':bounds,'// ORIGINAL_CLEAR':clear,'// ORIGINAL_GETTERS':getters,
             '// ORIGINAL_SETTERS':setters,'// ORIGINAL_LOOKUP_DECLARATIONS':declarations,'// ORIGINAL_SAME':same,
             '// ORIGINAL_SCAN_AND_CUT':scan,'// ORIGINAL_DIRECT_CALL':direct,'// ORIGINAL_CLASSIFICATION':classification,
             '// ORIGINAL_RAW_OPTIMIZE_CONVERT':optimize,'// ORIGINAL_RAW_TERMINAL':terminal,'// ORIGINAL_METHODS':methods}
    for marker,value in inserts.items():
        if harness.count(marker)!=1:raise SystemExit('Expected one harness marker: '+marker)
        harness=harness.replace(marker,value)
    fragments += [(HEADER,x) for x in (constants,types,bounds,getters,setters,clear,declarations)]
    fragments += [(CPP,x) for x in (same,scan,direct,classification,optimize,terminal)]
    fragments += [(CPP,exact_block(source,'void PathFinder::BuildPolyPath(')),
                  (MOVEMAP,exact_block(sources[MOVEMAP],'bool MMapManager::loadMapInstance('))]
    if fragments[-1][1].count('query->init(mmap->navMesh, 1024)')!=1:raise SystemExit('Source map node pool differs')
    if 'typedef std::vector<Vector3> PointsArray;' not in sources['src/game/Movement/MoveSplineInitArgs.h']:
        raise SystemExit('Original point storage alias differs')
    for key in ('CONFIG_BOOL_PATH_FIND_OPTIMIZE,','CONFIG_BOOL_PATH_FIND_NORMALIZE_Z,'):
        if sources['src/game/World/World.h'].count(key)!=1:raise SystemExit('Original config key differs')
    for signature in ('inline Vector3::Vector3()','inline bool Vector3::operator!= (','inline Vector3 Vector3::operator- (const Vector3&','inline float Vector3::squaredMagnitude ()','inline float Vector3::squaredLength ()'):
        fragments.append(('dep/g3dlite/G3D/Vector3.h',exact_block(sources['dep/g3dlite/G3D/Vector3.h'],signature)))
    notices=[source[:source.index('*/')+2],sources['dep/recastnavigation/Detour/Source/DetourNavMeshQuery.cpp'].split('#include',1)[0].rstrip(),sources['dep/g3dlite/G3D/Vector3.h'].split('#ifndef',1)[0].rstrip()]
    package='// CMaNGOS '+PIN+'; unchanged full BuildPointPath, smoothing and warm helpers.\n'
    package+='// Complete BuildPolyPath is review context only; raw postprocessing rows are not loaded smoothing evidence.\n'+'\n\n'.join(notices)+'\n'
    for name,fragment in fragments:package+=f'\n// BEGIN {name}\n{fragment}\n// END {name}\n'
    for name in ('LICENSE','dep/recastnavigation/License.txt','dep/g3dlite/license.cpp'):
        package+='\n// BEGIN original license '+name+'\n'+sources[name]+'\n// END original license '+name+'\n'
    return '\n\n'.join(notices)+'\n'+harness,package


def parse_table(data,fields):
    try:lines=data.decode('ascii').splitlines()
    except UnicodeDecodeError:raise SystemExit('Point-path oracle schema differs') from None
    if lines.count('# '+'\t'.join(fields))!=1:raise SystemExit('Point-path oracle schema differs')
    rows=[];labels=set()
    for line in lines:
        if line.startswith('#'):continue
        values=line.split('\t')
        if len(values)!=len(fields):raise SystemExit('Point-path oracle schema differs')
        row=dict(zip(fields,values,strict=True))
        if not re.fullmatch('[a-z0-9_]+',row['label']) or row['label'] in labels:raise SystemExit('Invalid or duplicate oracle label')
        labels.add(row['label']);rows.append(row)
    if not rows:raise SystemExit('Empty oracle cases')
    return rows


def finite_bits(value):
    if not re.fullmatch('[0-9a-f]{8}',value):raise ValueError()
    number=struct.unpack('!f',bytes.fromhex(value))[0]
    if not math.isfinite(number):raise ValueError()
    return number


def ref_list(value):
    result=[int(v) for v in value.split(',')]
    if not result or len(result)>4096 or any(not 0<=r<1<<64 for r in result):raise ValueError()
    return result


def points(value,maximum=74):
    if value=='-':return []
    result=[]
    for item in value.split(';'):
        words=item.split(',')
        if len(words)!=3:raise ValueError()
        result.append(tuple(finite_bits(word) for word in words))
    if len(result)>maximum:raise ValueError()
    return result


def parse_cases(data,raw=False):
    rows=parse_table(data,RAW_FIELDS if raw else FIELDS)
    for row in rows:
        try:
            for flag in ('optimize','normalize','ignore','force'):
                if row[flag] not in {'0','1'}:raise ValueError()
            if row['normalize']=='1' and row['ignore']=='0':raise ValueError()
            if not 0<=int(row['previous_type'])<=63 or int(row['path_type']) not in {1,4,8,17,32}:raise ValueError()
            limit=int(row['limit']) if raw else 74
            if not 2<=limit<=74:raise ValueError()
            for key in ('before','after'):
                active=int(row[key+'_active']);storage=ref_list(row[key+'_storage'])
                if not 0<=active<=limit or len(storage)<limit or any(not r for r in storage[:active]):raise ValueError()
            for key in ('from','to','actual_end'):
                if len(points(row[key],1))!=1:raise ValueError()
            final=points(row['points'])
            if len(final)<2:raise ValueError()
            if raw:
                points(row['prior_points']);raw_points=points(row['raw_points'])
                if len(raw_points)>limit or not re.fullmatch('[0-9a-f]{8}',row['raw_status']):raise ValueError()
            else:
                if row['geometry'] not in {'precision','ground','thin','existing'} or row['phase'] not in {'same','cut','replacement'}:raise ValueError()
                active=int(row['selected_active']);storage=ref_list(row['selected_storage'])
                if not 1<=active<=74 or len(storage)<74 or any(not r for r in storage[:active]):raise ValueError()
                smooth=points(row['smooth_points'])
                if row['phase']=='same':
                    if row['smooth_status']!='-' or smooth:raise ValueError()
                elif not re.fullmatch('[0-9a-f]{8}',row['smooth_status']):raise ValueError()
        except (ValueError,OverflowError):raise SystemExit('Invalid oracle row: '+row['label']) from None
    return rows


def observe(binary,work):
    if len(set(FILES))!=len(FILES) or any(Path(name).name!=name or not name.endswith('.test-fixture') for name in FILES):
        raise SystemExit('Unsafe fixture output filename')
    output=work/'output';output.mkdir(exist_ok=True)
    run([str(binary),str(output),str(HERE/'fixtures')])
    if {p.name for p in output.iterdir()}!=set(FILES):raise SystemExit('Unexpected oracle output files')
    result={name:(output/name).read_bytes() for name in FILES}
    parse_cases(result['cases.test-fixture']);parse_cases(result['postprocess.test-fixture'],True)
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos',type=Path);parser.add_argument('--check',action='store_true')
    parser.add_argument('--sanitize',action='store_true');parser.add_argument('--repair-tile-lifetime',action='store_true')
    parser.add_argument('--receipt-dir',type=Path);args=parser.parse_args()
    sources=guarded_sources(args.mangos);guard_inputs();_,package=build_harness(sources)
    if args.check and PACKAGE.read_text(encoding='latin1')!=package:raise SystemExit('Verbatim point-path source package differs')
    with tempfile.TemporaryDirectory(prefix='point-path-source-') as temporary:
        work=Path(temporary);binary=compile_oracle(sources,work,args.sanitize,args.receipt_dir,args.repair_tile_lifetime)
        fixtures=observe(binary,work)
    if args.check:
        for name,data in fixtures.items():
            if (FIXTURES/name).read_bytes()!=data:raise SystemExit('Point-path fixture differs: '+name)
    else:
        PACKAGE.write_bytes(package.encode('latin1'));FIXTURES.mkdir(parents=True,exist_ok=True)
        for name,data in fixtures.items():(FIXTURES/name).write_bytes(data)
    for name,data in fixtures.items():print(f'{"Verified" if args.check else "Generated"} {name}: {len(data)} bytes; sha256={hashlib.sha256(data).hexdigest()}')
    if args.receipt_dir:(args.receipt_dir/'output_hashes.json').write_text(json.dumps({name:hashlib.sha256(data).hexdigest() for name,data in fixtures.items()},indent=2)+'\n')


if __name__=='__main__':main()
