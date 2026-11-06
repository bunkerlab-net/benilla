#!/usr/bin/env python3
"""Extract unchanged source transport/spline bodies and run the F0 builder offline."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess

PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
IMAGE = 'sha256:5397aff8205e0d96686d9a74bd0c9542830d170c167c8d242a3c13e9f269e9a8'
HERE = Path(__file__).resolve().parent


def method(source, signature):
    assert source.count(signature) == 1, signature
    begin = source.index(signature)
    depth = 0
    for end in range(source.index('{', begin), len(source)):
        depth += (source[end] == '{') - (source[end] == '}')
        if depth == 0:
            return source[begin:end+1]
    raise RuntimeError(signature)


def bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def cases():
    def path(name, count, speed=30, accel=1, stop=(), teleport=(), maps=None, path_id=1, period=None):
        nodes = []
        for i in range(count):
            nodes.append({'id': i+1, 'index': i, 'map': 0 if maps is None else maps[i],
                          'position': [bits(i*137.25), bits((i % 3)*17.125), bits(i*0.75)],
                          'flags': 1 if i in teleport else 2 if i in stop else 0,
                          'delay': 3+i if i in stop else 0})
        return {'name': name, 'entry': 42, 'path': path_id, 'speed': speed,
                'accel': accel, 'nodes': nodes, 'period': period}
    return [path('short', 7, speed=500, stop=(1,5)),
            path('cruise', 11, speed=20, accel=3, stop=(1,5,9)),
            path('consecutive', 8, stop=(1,2,6)),
            path('no_stop', 7),
            path('teleport', 12, stop=(1,6,10), teleport=(4,)),
            path('one_control_leg', 7, stop=(1,5), teleport=(2,)),
            path('map_change', 12, stop=(1,6,10), maps=[0]*5+[1]*7),
            path('period', 7, stop=(1,5), period=123456),
            path('feathermoon', 18, stop=(1,16), path_id=303),
            path('teldrassil', 18, stop=(1,16), path_id=293),
            path('single_frame', 3, stop=(1,))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('evidence', type=Path)
    parser.add_argument('--private-cases',type=Path)
    args = parser.parse_args()
    assert subprocess.check_output(['git', '-C', str(args.source), 'rev-parse', 'HEAD'], text=True).strip() == PIN
    work = args.evidence.resolve()/'template-oracle'
    work.mkdir(parents=True, exist_ok=True)
    retained = {}
    def source(name):
        raw = (args.source/name).read_bytes()
        retained[name] = hashlib.sha256(raw).hexdigest()
        return raw.decode()
    spline_header = source('src/game/Movement/spline.h')
    spline_impl = source('src/game/Movement/spline.impl.h')
    spline = source('src/game/Movement/spline.cpp')
    transport = source('src/game/Maps/TransportMgr.cpp')
    transport_header = source('src/game/Maps/TransportMgr.h')
    entities = source('src/game/Entities/Transports.cpp')
    node_header = source('src/game/Server/DBCStructure.h')
    vector4 = source('dep/g3dlite/Vector4.cpp')
    matrix4 = source('dep/g3dlite/Matrix4.cpp')
    source('dep/g3dlite/G3D/Vector3.h')
    source('dep/g3dlite/G3D/Vector4.h')
    source('dep/g3dlite/G3D/Matrix4.h')
    orientation = method(source('src/game/Maps/MapManager.h'), 'static float NormalizeOrientation(float o)')
    common = source('src/shared/Common.h')
    pi_line = next(line for line in common.splitlines() if 'define M_PI_F ' in line)
    (work/'Common.h').write_text('''#pragma once
#include <cstdint>
#include <vector>
#include <string>
#include <limits>
#include <cmath>
#include <stdexcept>
using uint8=uint8_t; using uint32=uint32_t; using int32=int32_t;
#define MANGOS_ASSERT(x) do { if (!(x)) throw std::runtime_error(#x); } while (0)
namespace G3D { class Vector2; class Vector3; class Vector4; }
namespace Movement { using G3D::Vector2; using G3D::Vector3; using G3D::Vector4; typedef std::vector<Vector3> PointsArray; }
''')
    (work/'typedefs.h').write_text('#include "Common.h"\n')
    (work/'spline.h').write_text(spline_header)
    (work/'spline.impl.h').write_text(spline_impl)
    # Discard only unrelated string formatting. All spline evaluation, length and initializer bodies remain unchanged.
    spline = spline[spline.index('namespace Movement'):spline.index('std::string SplineBase::ToString()')] + '\n}\n'
    body = '''#include "Common.h"
#include <G3D/Vector3.h>
#include <G3D/Vector4.h>
#include <G3D/Matrix4.h>
#include "spline.h"
#include <set>
#include <map>
#include <iostream>
#include <iomanip>
#include <cstring>
#include <cfenv>
#include <xmmintrin.h>
using namespace G3D;
constexpr uint32 IN_MILLISECONDS=1000;
'''
    body += pi_line + '\n'
    body += 'namespace G3D {\n' + method(vector4, 'Vector4 Vector4::operator*(const Matrix4& M) const')
    body += '\n' + method(matrix4, 'Matrix4::Matrix4(\n    float r1c1') + '\n}\n'
    body += spline
    body += method(node_header, 'struct TaxiPathNodeEntry\n') + ';\n'
    body += 'using TaxiPathNodeList=std::vector<TaxiPathNodeEntry const*>;\nstd::vector<TaxiPathNodeList> sTaxiPathNodesByPath;\n'
    body += 'using TransportSpline=Movement::Spline<double>;\n'
    body += method(transport_header, 'struct KeyFrame\n') + ';\n'
    body += 'using KeyFrameVec=std::vector<KeyFrame>;\n'
    body += method(transport_header, 'struct TransportTemplate\n') + ';\n'
    body += 'struct GameObjectInfo { struct { uint32 taxiPathId, moveSpeed, accelRate; } moTransport; };\n'
    body += 'struct MapInfo { bool Instanceable() const { return false; } }; struct MapStore { MapInfo info; MapInfo const* LookupEntry(uint32) { return &info; } } sMapStore;\n'
    body += 'struct MapManager {\n' + orientation + '\n};\n'
    body += 'struct TransportMgr { bool GenerateWaypoints(GameObjectInfo const*, TransportTemplate&); };\n'
    body += method(transport, 'class SplineRawInitializer\n') + ';\n'
    body += method(transport, 'bool TransportMgr::GenerateWaypoints(') + '\n'
    body += method(entities, 'TransportTemplate::~TransportTemplate()') + '\n'
    body += (HERE/'source_transports_oracle.cpp').read_text()
    (work/'oracle.cpp').write_text(body)
    corpus = json.loads(args.private_cases.read_text()) if args.private_cases else cases()
    (work/'cases.json').write_text(json.dumps(corpus, separators=(',',':'))+'\n')
    stdin = []
    for case in corpus:
        stdin.append(f"{case['name']} {case['entry']} {case['path']} {case['speed']} {case['accel']} {len(case['nodes'])} {int(case['period'] is not None)} {case['period'] or 0}")
        for node in case['nodes']:
            stdin.append(' '.join(map(str,[node['index'],node['map'],*node['position'],node['flags'],node['delay']])))
    (work/'inputs.txt').write_text('\n'.join(stdin)+'\n')
    compile_args = ['g++','-std=c++17','-O3','-DNDEBUG','-ffp-contract=off','-fno-fast-math','-fno-omit-frame-pointer','-g',
                    '-I/evidence','-I/source/dep/g3dlite','/evidence/oracle.cpp','-o','/evidence/oracle']
    base = ['docker','run','--rm','-i','--network=none','--user',f'{__import__("os").getuid()}:{__import__("os").getgid()}',
            '--mount',f'type=bind,src={args.source.resolve()},dst=/source,readonly',
            '--mount',f'type=bind,src={work},dst=/evidence',IMAGE]
    subprocess.run(base+compile_args,check=True)
    result = subprocess.check_output(base+['/evidence/oracle'],input=(work/'inputs.txt').read_bytes())
    assert len(result.splitlines()) == len(corpus), 'Oracle did not consume every case'
    assert [line.split()[0].decode() for line in result.splitlines()] == [case['name'] for case in corpus]
    fixtures=HERE/'fixtures/source_transports'
    if args.private_cases:
        (args.evidence.resolve()/'stock-transport-expected.tsv').write_bytes(result)
    else:
        fixtures.mkdir(parents=True,exist_ok=True)
        (fixtures/'cases.json').write_bytes((work/'cases.json').read_bytes())
        (fixtures/'expected.tsv').write_bytes(result)
    receipt={'image':IMAGE,'core_revision':PIN,'compile_command':compile_args,'source_sha256':retained,
             'oracle_sha256':hashlib.sha256(body.encode()).hexdigest(),'output_sha256':hashlib.sha256(result).hexdigest(),
             'cases':len(corpus),'scope':'Unsealed raw stock component fixtures' if args.private_cases else 'Authored source component fixtures, not sealed F1 acceptance'}
    if not args.private_cases:
        (fixtures/'provenance.json').write_text(json.dumps(receipt,indent=2)+'\n')
    (work/'oracle-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(receipt))

if __name__ == '__main__':
    main()
