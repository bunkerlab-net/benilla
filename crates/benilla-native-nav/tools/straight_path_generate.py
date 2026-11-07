#!/usr/bin/env python3
"""Exact original loaded-mesh straight_path observer, independent of the production bridge."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
import math
import os
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
MANIFEST = HERE / 'straight_path_source_hashes.sha256'
PACKAGE = HERE / 'straight_path_source_fragments.test-fixture'
FIXTURES = HERE / 'fixtures/straight-path'
FILES = ('cases.test-fixture', 'sequences.test-fixture', 'traces.test-fixture', 'domains.test-fixture')
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
FIELDS = ('label geometry scope from to collision_width optimize normalize ignore force previous_type before_active before_storage '
          'start_ref start_distance end_ref end_distance lookup_storage phase ray_status ray_hit ray_normal ray_active ray_storage adjusted_hit width_calls '
          'after_active after_storage path_type end actual_end points height_calls height_failures').split()
SEQUENCE_FIELDS = ('label first_case from to optimize before_active before_storage previous_type start_ref start_distance end_ref end_distance '
                   'lookup_storage phase direct_status smooth_status smooth_points after_active after_storage path_type end actual_end points').split()
TRACE_FIELDS = 'label ordinal primitive ref target status input output scalar count refs'.split()
DOMAIN_FIELDS = 'label classification invoked'.split()
SOURCE_FILES = {CPP, HEADER, MOVEMAP, 'LICENSE', 'dep/recastnavigation/License.txt'} | {
    f'dep/recastnavigation/Detour/Source/{name}.cpp' for name in DETOUR_FILES} | {
    f'dep/recastnavigation/Detour/Include/{name}.h' for name in (*DETOUR_FILES, 'DetourMath', 'DetourStatus')}
G3D_FILES = ('dep/g3dlite/G3D/Array.h', 'dep/g3dlite/G3D/BinaryFormat.h', 'dep/g3dlite/G3D/Crypto.h', 'dep/g3dlite/G3D/EqualsTrait.h', 'dep/g3dlite/G3D/G3DGameUnits.h', 'dep/g3dlite/G3D/HashTrait.h', 'dep/g3dlite/G3D/MemoryManager.h', 'dep/g3dlite/G3D/PositionTrait.h', 'dep/g3dlite/G3D/Random.h', 'dep/g3dlite/G3D/ReferenceCount.h', 'dep/g3dlite/G3D/System.h', 'dep/g3dlite/G3D/Table.h', 'dep/g3dlite/G3D/Vector2.h', 'dep/g3dlite/G3D/Vector2int16.h', 'dep/g3dlite/G3D/Vector3.h', 'dep/g3dlite/G3D/debug.h', 'dep/g3dlite/G3D/debugAssert.h', 'dep/g3dlite/G3D/debugPrintf.h', 'dep/g3dlite/G3D/format.h', 'dep/g3dlite/G3D/g3dmath.h', 'dep/g3dlite/G3D/platform.h', 'dep/g3dlite/G3D/uint128.h', 'dep/g3dlite/license.cpp', 'src/game/Movement/MoveSplineInitArgs.h', 'src/game/World/World.h')
SOURCE_FILES |= set(G3D_FILES) | {'src/game/MotionGenerators/TargetedMovementGenerator.cpp'}
INPUT_FILES = {'authored.mmap', '0003232.mmtile', '0003231.mmtile',
               'direct-corridor/params.test-fixture', 'warm-cache/precision.test-fixture',
               'smoothing/params.test-fixture', 'smoothing/ground.test-fixture', 'smoothing/thin.test-fixture',
               'direct-corridor/long600.test-fixture'}


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
    for name, expected in manifest_rows(HERE / 'straight_path_input_hashes.sha256', INPUT_FILES).items():
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
                 'G3D::Vector3::Vector3()', 'G3D::Vector3::squaredLength() const', 'dtCreateNavMeshData(',
                 'dtNavMeshQuery::raycast(', 'dtNavMeshQuery::getPolyHeight(', 'G3D::Vector3::magnitude() const',
                 'G3D::Vector3::operator*(float) const', 'G3D::Vector3::operator+=(G3D::Vector3 const&)'):
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


def verify_warnings(warnings, harness, filename):
    # Keep the original ignored getPolyHeight status assignment unchanged. Only
    # this exact warning, at this exact original statement, is non-fatal.
    statement = '                    dtStatus dtResult = m_navMeshQuery->getPolyHeight(poly, hitPos, &hitPos[1]);'
    if harness.count(statement) != 1:
        raise SystemExit('Original ignored-height statement drift')
    line = harness[:harness.index(statement)].count('\n') + 1
    expected = f"{filename}:{line}:30: warning: unused variable 'dtResult' [-Wunused-variable]"
    diagnostics = [row for row in warnings.splitlines() if 'warning:' in row or 'error:' in row]
    if diagnostics != [expected]:
        raise SystemExit('Unexpected source warning profile')


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
             '-ffp-contract=off', '-DNDEBUG', '-DDT_POLYREF64=1', '-Wall', '-Wextra', '-Werror',
             '-Wno-class-memaccess', '-Wno-deprecated-copy', '-Wno-error=unused-variable', '-fdiagnostics-color=never']
    if repair_lifetime: flags += ['-DSTRAIGHT_PATH_REPAIR_TILE_LIFETIME=1']
    if sanitize: flags += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
    command = [compiler, *flags, '-I' + str(detour / 'Include'), '-I' + str(work/'dep/g3dlite'), str(work / 'oracle.cpp'),
               *[str(detour / f'Source/{name}.cpp') for name in DETOUR_FILES], '-o', str(work / 'oracle')]
    compiled = run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env={**os.environ, 'LC_ALL': 'C'})
    warnings = compiled.stderr.decode()
    if warnings: print(warnings, file=sys.stderr, end='')
    verify_warnings(warnings, harness, work / 'oracle.cpp')
    symbols = run(['nm', '-C', str(work / 'oracle')], stdout=subprocess.PIPE).stdout.decode()
    verify_symbols(symbols)
    if receipts:
        receipts.mkdir(parents=True, exist_ok=True)
        shutil.copy2(work / 'oracle.cpp', receipts / 'oracle.cpp')
        (receipts / 'symbols.txt').write_text(symbols)
        (receipts / 'compiler_stderr.txt').write_text(warnings)
        shutil.copy2(work / TILE_HEADER, receipts / 'DetourNavMesh_observed.h')
        (receipts / 'profile.json').write_text(json.dumps({
            'cmangos_revision': PIN, 'compiler': version, 'target': target,
            'command': command, 'sanitize': sanitize, 'query_nodes': [1024],
            'tile_lifetime_profile': 'default-constructor-only-repaired' if repair_lifetime else 'unchanged-original-empirical',
            'observed_tile_header_sha256': hashlib.sha256((work / TILE_HEADER).read_bytes()).hexdigest(),
            'tile_lifetime_amendment': 'insert dtMeshTile() = default; only' if repair_lifetime else None,
            'scope': 'complete original warm lookup, straight/smooth BuildPointPath and smoothing methods; exact selected BuildPolyPath suffix; scalar full-width dependency; same-object fallback; actual G3D Vector3',
            'warning_allowance': 'LC_ALL=C; exact file/line/column and sole diagnostic for unchanged PathFinder.cpp:738 unused local dtStatus dtResult; legacy class-memaccess and deprecated-copy suppressed',
            'excluded': ['uninitialized cold lookup', 'complete BuildPolyPath', 'active NormalizePath world callbacks', 'OwnerPathFinder', 'gameplay admission'],
            'source_hashes_sha256': hashlib.sha256(MANIFEST.read_bytes()).hexdigest(),
            'authored_input_hashes_sha256': hashlib.sha256((HERE / 'straight_path_input_hashes.sha256').read_bytes()).hexdigest(),
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
        raise SystemExit('Source straight-path bounds differ')
    getters=source_span(header,'        Vector3 getStartPosition()', '\n        PointsArray& getPath()')
    setters=source_span(header,'        void setStartPosition(', '\n        void NormalizePath();')
    clear=exact_block(header,'void clear()')
    bounds=source_span(header,'static float NearPolySearchBound','\nenum PathType')
    types=exact_block(header,'enum PathType')+';'
    declarations=source_span(header,'        dtPolyRef getPathPolyByPosition(', '        bool HaveTile(')
    selected=source_span(source,'    // start and end are on same polygon','\nvoid PathFinder::BuildPointPath(').rstrip()
    if not selected.endswith('}'):raise SystemExit('Original selected suffix bounds differ')
    selected=selected[:-1]
    tile=exact_block(sources[TILE_HEADER],'struct dtMeshTile\n{')
    layout=tile.replace('struct dtMeshTile','struct OriginalTileLayout',1).replace('dtMeshTile(const dtMeshTile&)','OriginalTileLayout(const OriginalTileLayout&)').replace('dtMeshTile& operator=(const dtMeshTile&)','OriginalTileLayout& operator=(const OriginalTileLayout&)')+';'
    harness=(HERE/'straight_path_oracle.cpp').read_text()
    inserts={'// ORIGINAL_TILE_LAYOUT':layout,'// ORIGINAL_CONSTANTS':constants,'// ORIGINAL_PATH_TYPE':types,
             '// ORIGINAL_SEARCH_BOUNDS':bounds,'// ORIGINAL_CLEAR':clear,'// ORIGINAL_GETTERS':getters,
             '// ORIGINAL_SETTERS':setters,'// ORIGINAL_LOOKUP_DECLARATIONS':declarations,
             '// ORIGINAL_SELECTED_SUFFIX':selected,'// ORIGINAL_METHODS':methods}
    for marker,value in inserts.items():
        if harness.count(marker)!=1:raise SystemExit('Expected one harness marker: '+marker)
        harness=harness.replace(marker,value)
    fragments += [(HEADER,x) for x in (constants,types,bounds,getters,setters,clear,declarations)]
    fragments += [(CPP,selected)]
    fragments += [(CPP,exact_block(source,'void PathFinder::BuildPolyPath(')),
                  (MOVEMAP,exact_block(sources[MOVEMAP],'bool MMapManager::loadMapInstance('))]
    if fragments[-1][1].count('query->init(mmap->navMesh, 1024)')!=1:raise SystemExit('Source map node pool differs')
    if 'typedef std::vector<Vector3> PointsArray;' not in sources['src/game/Movement/MoveSplineInitArgs.h']:
        raise SystemExit('Original point storage alias differs')
    fragments.append(('src/game/MotionGenerators/TargetedMovementGenerator.cpp',exact_block(sources['src/game/MotionGenerators/TargetedMovementGenerator.cpp'],'bool ChaseMovementGenerator::DispatchSplineToPosition(')))
    for key in ('CONFIG_BOOL_PATH_FIND_OPTIMIZE,','CONFIG_BOOL_PATH_FIND_NORMALIZE_Z,'):
        if sources['src/game/World/World.h'].count(key)!=1:raise SystemExit('Original config key differs')
    for signature in ('inline Vector3::Vector3()','inline bool Vector3::operator!= (','inline Vector3 Vector3::operator- (const Vector3&','inline float Vector3::squaredMagnitude ()','inline float Vector3::squaredLength ()','inline float Vector3::magnitude()','inline Vector3 Vector3::operator*(float','inline Vector3& Vector3::operator+= ('):
        fragments.append(('dep/g3dlite/G3D/Vector3.h',exact_block(sources['dep/g3dlite/G3D/Vector3.h'],signature)))
    notices=[source[:source.index('*/')+2],sources['dep/recastnavigation/Detour/Source/DetourNavMeshQuery.cpp'].split('#include',1)[0].rstrip(),sources['dep/g3dlite/G3D/Vector3.h'].split('#ifndef',1)[0].rstrip()]
    package='// CMaNGOS '+PIN+'; unchanged full BuildPointPath, smoothing and warm helpers.\n'
    package+='// Complete BuildPolyPath/Chase are review context only; full width is an explicit external scalar.\n'+'\n\n'.join(notices)+'\n'
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
    if not re.fullmatch(r'[0-9]+(?:,[0-9]+)*',value):raise ValueError()
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


def parse_cases(data, sequence=False):
    rows=parse_table(data,SEQUENCE_FIELDS if sequence else FIELDS)
    for row in rows:
        try:
            for flag in (('optimize',) if sequence else ('optimize','normalize','ignore','force')):
                if row[flag] not in {'0','1'}:raise ValueError()
            if not sequence and (row['normalize']=='1' and row['ignore']=='0'):raise ValueError()
            if not 0<=int(row['previous_type'])<=63 or int(row['path_type']) not in {1,4,8,17,32}:raise ValueError()
            for key in ('before','after'):
                active=int(row[key+'_active']);storage=ref_list(row[key+'_storage'])
                if not 0<=active<=74 or len(storage)<74 or any(not r for r in storage[:active]):raise ValueError()
            if len(ref_list(row['lookup_storage']))<74:raise ValueError()
            for key in ('from','to','end','actual_end'):
                if len(points(row[key],1))!=1:raise ValueError()
            points(row['points'],1026)
            for key in ('start_ref','end_ref'):
                if not 0<=int(row[key])<1<<64:raise ValueError()
            for key in ('start_distance','end_distance'):
                if not 0<=finite_bits(row[key])<=7:raise ValueError()
            loaded=sequence or row['scope']=='loaded'
            if loaded and (row['start_ref']=='0' or row['end_ref']=='0'):raise ValueError()
            if sequence:
                if row['phase'] not in {'same','cut','replacement'}:raise ValueError()
                if not re.fullmatch('[a-z0-9_]+',row['first_case']):raise ValueError()
                for key in ('direct_status','smooth_status'):
                    if row[key]!='-' and not re.fullmatch('[0-9a-f]{8}',row[key]):raise ValueError()
                points(row['smooth_points'])
            else:
                if row['geometry'] not in {'precision','ground','thin','long600','existing'} or row['scope'] not in {'loaded','helper'}:raise ValueError()
                if row['phase'] not in {'same','cut','replacement','sample'}:raise ValueError()
                if (row['phase']=='sample') != (row['scope']=='helper'):raise ValueError()
                if not 0<=finite_bits(row['collision_width'])<=100000:raise ValueError()
                for key in ('width_calls','height_calls','height_failures'):
                    if not 0<=int(row[key])<=100000:raise ValueError()
                if int(row['height_failures'])>int(row['height_calls']):raise ValueError()
                if int(row['width_calls'])>1:raise ValueError()
                if row['ray_status']=='-':
                    if any(row[k]!='-' for k in ('ray_hit','ray_normal','ray_active','ray_storage','adjusted_hit')) or row['width_calls']!='0':raise ValueError()
                else:
                    if not re.fullmatch('[0-9a-f]{8}',row['ray_status']):raise ValueError()
                    finite_bits(row['ray_hit'])
                    if len(points(row['ray_normal'],1))!=1:raise ValueError()
                    n=int(row['ray_active']);refs=ref_list(row['ray_storage'])
                    if not 0<=n<=74 or len(refs)<74 or any(not r for r in refs[:n]):raise ValueError()
                    if row['adjusted_hit']!='-':finite_bits(row['adjusted_hit'])
        except (ValueError,OverflowError):raise SystemExit('Invalid oracle row: '+row['label']) from None
    return rows


def parse_auxiliary(data, trace):
    rows=parse_table(data,TRACE_FIELDS if trace else DOMAIN_FIELDS)
    for row in rows:
        try:
            if trace:
                if not 0<=int(row['ordinal'])<100000:raise ValueError()
                if row['primitive'] not in {'closest','nearest5','nearest10','raycast','height','direct','boundary','steer','move'}:raise ValueError()
                for key in ('ref','target'):
                    if not 0<=int(row[key])<1<<64:raise ValueError()
                if not re.fullmatch('[0-9a-f]{8}',row['status']):raise ValueError()
                points(row['input'],2);points(row['output'],74);finite_bits(row['scalar'])
                if not 0<=int(row['count'])<=74:raise ValueError()
                if row['refs']=='-':
                    if row['count']!='0':raise ValueError()
                elif len(ref_list(row['refs']))!=int(row['count']):raise ValueError()
            elif row['classification'] not in {'cold_uninitialized','active_bounds','invalid_reference','coordinate_domain','width_domain','normalization_unsupported','type_domain','sample_bound'} or row['invoked']!='0':raise ValueError()
        except (ValueError,OverflowError):raise SystemExit('Invalid auxiliary row: '+row['label']) from None
    return rows


def verify_products(result):
    cases=parse_cases(result['cases.test-fixture'])
    sequences=parse_cases(result['sequences.test-fixture'],True)
    traces=parse_auxiliary(result['traces.test-fixture'],True)
    domains=parse_auxiliary(result['domains.test-fixture'],False)
    by_case={row['label']:row for row in cases}
    by_sequence={row['label']:row for row in sequences}
    if set(by_case)&set(by_sequence) or (set(by_case)|set(by_sequence))&{r['label'] for r in domains}:
        raise SystemExit('Overlapping oracle product labels')
    for row in sequences:
        first=by_case.get(row['first_case'])
        if first is None or not int(first['path_type']) & (4|8):
            raise SystemExit('Invalid same-object continuation origin')
        for before,after in (('before_active','after_active'),('before_storage','after_storage'),('previous_type','path_type'),('from','from'),('to','to'),('optimize','optimize')):
            if row[before]!=first[after]:raise SystemExit('Same-object continuation state differs')
    events={label:[] for label in by_case|by_sequence}
    for row in traces:
        parent,_,index=row['label'].rpartition('_')
        if parent not in events or index!=f"{int(row['ordinal']):04d}" or int(row['ordinal'])!=len(events[parent]):
            raise SystemExit('Invalid oracle trace parent or order')
        events[parent].append(row)
    for case in cases:
        observed=events[case['label']]
        heights=[e for e in observed if e['primitive']=='height']
        if len(heights)!=int(case['height_calls']) or sum(bool(int(e['status'],16)&0x80000000) for e in heights)!=int(case['height_failures']):
            raise SystemExit('Height summary differs from trace')
        rays=[e for e in observed if e['primitive']=='raycast']
        if len(rays)!=(case['ray_status']!='-'):raise SystemExit('Ray summary differs from trace')
        if rays:
            ray=rays[0]
            for summary,event in (('ray_status','status'),('ray_hit','scalar'),('ray_normal','output'),('ray_active','count')):
                if case[summary]!=ray[event]:raise SystemExit('Ray summary differs from trace')
            if ref_list(case['ray_storage'])[:int(case['ray_active'])] != ([] if ray['refs']=='-' else ref_list(ray['refs'])):
                raise SystemExit('Ray storage differs from trace')
            hit=finite_bits(case['ray_hit']);width=finite_bits(case['collision_width'])
            expected='-' if case['ray_hit']=='7f7fffff' else struct.pack('!f',hit-width).hex()
            if case['adjusted_hit']!=expected or int(case['width_calls'])!=(expected!='-'):
                raise SystemExit('Original full-width subtraction differs')
    return cases,sequences,traces,domains


def observe(binary,work):
    if len(set(FILES))!=len(FILES) or any(Path(name).name!=name or not name.endswith('.test-fixture') for name in FILES):
        raise SystemExit('Unsafe fixture output filename')
    output=work/'output';output.mkdir(exist_ok=True)
    run([str(binary),str(output),str(HERE/'fixtures')])
    if {p.name for p in output.iterdir()}!=set(FILES):raise SystemExit('Unexpected oracle output files')
    result={name:(output/name).read_bytes() for name in FILES}
    verify_products(result)
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos',type=Path);parser.add_argument('--check',action='store_true')
    parser.add_argument('--sanitize',action='store_true');parser.add_argument('--repair-tile-lifetime',action='store_true')
    parser.add_argument('--receipt-dir',type=Path);args=parser.parse_args()
    sources=guarded_sources(args.mangos);guard_inputs();_,package=build_harness(sources)
    if args.check and PACKAGE.read_text(encoding='latin1')!=package:raise SystemExit('Verbatim straight-path source package differs')
    with tempfile.TemporaryDirectory(prefix='straight-path-source-') as temporary:
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
