#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run authored unchanged CMaNGOS MoveSpline bodies in the pinned glibc 2.36 builder."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from source_transports_generate import bits, method, PIN

HERE = Path(__file__).resolve().parent
IMAGE = 'sha256:7f8acd0cd855b63046cfdf49f21ed319ae6b981758f5fafe2100aed8b4ef21e8'


def cases():
    rows = []
    bent = [[0, 0, 9], [3, 4, 9], [-2, 8, 12], [10, -4, 1]]
    def add(name, path=bent, flags=0, speed=5, offset=0, steps=None, kind='M'):
        rows.append(dict(name=name, path=[[bits(x) for x in p] for p in path], flags=flags,
                         velocity=bits(speed), id=17, offset=offset, facing=[bits(12), bits(-8), bits(2)],
                         steps=steps if steps is not None else [[0, x] for x in (0, 1, 999, 1, 1, 100000, 0)], kind=kind))
    add('linear')
    add('catmull', flags=0x200)
    add('zero_middle', path=[[0,0,0],[1,0,0],[1,0,0],[2,0,0]], speed=1000,
        steps=[[0,x] for x in (0,1,1,0,1,0)])
    add('zero_all', path=[[2,3,4]]*4)
    add('tiny_segments', path=[[0,0,0],[0.0001,0,0],[0.0002,0,0],[0.0003,0,0]], speed=1000)
    add('round_compound', path=[[0,0,0],[16777216,0,0],[16777218,0,0],[16777220,0,0]], speed=1000,
        steps=[[0,16777216],[0,1],[0,2],[0,0xffffffff]])
    add('negative_offset', offset=-11)
    add('interrupt', steps=[[0,1500],[1,0],[0,9999]])
    add('finalize', flags=0x200, steps=[[0,555],[2,0],[0,0]])
    add('stop_reinitialize', steps=[[0,777],[3,0]])
    for flags in (0,0x200):
        for enter in (0,0x200000):
            add(f'cycle_{flags}_{enter}', flags=flags|0x100000|enter,
                steps=[[0,x] for x in (1,10000,99999,0,1000000)])
    add('cycle_three_enter', path=[[0,0,0],[1,0,0],[2,1,0]], flags=0x300000, steps=[[0,1000],[0,1],[0,10000]])
    add('zero_cycle', path=[[2,3,4]]*4, flags=0x300000, steps=[[0,0],[0,1],[0,7]])
    for flag in (0x10000,0x20000,0x40000,0x70000):
        add(f'facing_{flag}', flags=flag, steps=[[0,111],[0,100000],[0,1]])
    add('facing_signed_zero', path=[[0,0,0],[-1,-0.0,0]], steps=[[0,1],[0,999]])
    add('fall_short', path=[[0,0,20],[1,2,19],[2,4,0]], flags=2, steps=[[0,x] for x in (0,1,320,100,1000,10000)])
    add('fall_terminal', path=[[0,0,1000],[3,4,900],[5,6,-1000]], flags=2, steps=[[0,x] for x in (1,3000,117,1,10000,100000)])
    add('fall_zero_repair', path=[[1,2,3]]*3, flags=2, steps=[[0,0],[0,1],[0,0]])
    add('fall_cycle_repair', path=[[1,2,3]]*3, flags=0x100002, steps=[[0,0],[0,999],[0,1],[0,1]])
    add('fall_upward', path=[[0,0,10],[1,0,11],[2,0,12]], flags=2)
    add('signed_duration_valid', path=[[0,0,0],[2147483520,0,0]], speed=1000, steps=[[0,1],[0,0xffffffff]])
    for flags in (0,0x200,0x100000,0x300200):
        add(f'spline_{flags}', flags=flags, kind='S', steps=[bits(x) for x in (0,0.125,0.25,0.5,0.75,1)])
    add('spline_exact_boundary', path=[[0,0,0],[5,0,0],[10,0,0]], kind='S', steps=[bits(x) for x in (0,0.5,1)])
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('evidence', type=Path)
    args = parser.parse_args()
    assert subprocess.check_output(['git','-C',str(args.source),'rev-parse','HEAD'], text=True).strip() == PIN
    work = args.evidence.resolve()
    work.mkdir(parents=True, exist_ok=True)
    hashes = {}
    def source(name):
        raw = (args.source/name).read_bytes()
        assert raw == subprocess.check_output(['git','-C',str(args.source),'show',f'{PIN}:{name}']), name
        hashes[name] = hashlib.sha256(raw).hexdigest()
        return raw.decode()
    common = '''#pragma once
#include <cstdint>
#include <vector>
#include <string>
#include <limits>
#include <cmath>
#include <stdexcept>
using uint8=uint8_t; using uint32=uint32_t; using int32=int32_t; using uint64=uint64_t;
#define MANGOS_ASSERT(x) do { if (!(x)) throw std::runtime_error(#x); } while (0)
namespace G3D { class Vector2; class Vector3; class Vector4; }
namespace Movement { using G3D::Vector2; using G3D::Vector3; using G3D::Vector4; }
'''
    (work/'Common.h').write_text(common)
    typedefs = source('src/game/Movement/typedefs.h')
    (work/'typedefs.h').write_text(common + '\nnamespace Movement {\n' + method(typedefs,'inline uint32 SecToMS(') + '\n' + method(typedefs,'inline float MSToSec(') + '\n}\n')
    for name in ('spline.h','spline.impl.h','MoveSplineInitArgs.h','MoveSplineFlag.h','MoveSpline.h'):
        text = source('src/game/Movement/'+name)
        # Visibility only; numerical bodies, member layout and templates are unchanged.
        if name == 'MoveSpline.h': text = text.replace('protected:', 'public:')
        (work/name).write_text(text)
    spline = source('src/game/Movement/spline.cpp')
    spline = spline[spline.index('namespace Movement'):spline.index('std::string SplineBase::ToString()')] + '\n}\n'
    move = source('src/game/Movement/MoveSpline.cpp')
    util = source('src/game/Movement/util.cpp')
    util = util[util.index('namespace Movement'):util.index('#define STR')] + '\n}\n'
    vector = source('dep/g3dlite/Vector4.cpp')
    matrix = source('dep/g3dlite/Matrix4.cpp')
    for name in ('Vector3','Vector4','Matrix4'):
        source('dep/g3dlite/G3D/'+name+'.h')
    body = '''#include "Common.h"
#include <G3D/Vector3.h>
#include <G3D/Vector4.h>
#include <G3D/Matrix4.h>
#include "MoveSpline.h"
#include <iostream>
#include <sstream>
#include <cstring>
#include <cfenv>
#include <xmmintrin.h>
#include <algorithm>
using namespace G3D;
struct Unit { std::string GetGuidStr() { return "oracle"; } };
struct Log { template<class... T> void outError(T...) {} } sLog;
'''
    body += 'namespace G3D {\n' + method(vector,'Vector4 Vector4::operator*(const Matrix4& M) const') + '\n' + method(matrix,'Matrix4::Matrix4(\n    float r1c1') + '\n}\n'
    body += spline + util
    body += move[move.index('namespace Movement'):move.index('/// ============================================================================================')]+ '\n}\n'
    signatures = ['bool MoveSplineInitArgs::Validate(', 'MoveSpline::UpdateResult MoveSpline::_updateState(',
                  'void MoveSpline::_Finalize()', 'const Vector3 MoveSpline::FinalDestination()', 'int32 MoveSpline::currentPathIdx()']
    body += 'namespace Movement {\n' + '\n'.join(method(move,sig) for sig in signatures) + '\n}\n'
    body += (HERE/'source_spline_oracle.cpp').read_text()
    (work/'oracle.cpp').write_text(body)
    corpus = cases()
    stdin = []
    for c in corpus:
        stdin.append(' '.join(map(str,[c['kind'],c['name'],c['flags'],c['velocity'],c['id'],c['offset'],*c['facing'],len(c['path'])])))
        stdin.extend(' '.join(map(str,p)) for p in c['path'])
        stdin.append(str(len(c['steps'])))
        stdin.extend(' '.join(map(str,step)) if isinstance(step,list) else str(step) for step in c['steps'])
    inputs = ('\n'.join(stdin)+'\n').encode()
    (work/'inputs.txt').write_bytes(inputs)
    actual = subprocess.check_output(['docker','image','inspect',IMAGE,'--format','{{.Id}}'],text=True).strip()
    assert actual == IMAGE
    base = ['docker','run','--rm','-i','--network','none','--user',f'{os.getuid()}:{os.getgid()}',
            '--mount',f'type=bind,src={args.source.resolve()},dst=/source,readonly',
            '--mount',f'type=bind,src={work},dst=/evidence',IMAGE]
    command = ['g++','-std=c++17','-O3','-DNDEBUG','-ffp-contract=off','-fno-fast-math',
               '-fno-omit-frame-pointer','-g','-I/evidence','-I/source/dep/g3dlite','/evidence/oracle.cpp','-o','/evidence/oracle']
    subprocess.run(base+command,check=True)
    result = subprocess.check_output(base+['/evidence/oracle'],input=inputs)
    expected_rows = sum(len(c['steps'])+(2 if c['kind']=='M' else 0) for c in corpus)
    assert len(result.splitlines()) == expected_rows
    sanitizer = command[:-2] + ['-fsanitize=undefined,float-cast-overflow','-fno-sanitize-recover=all','-o','/evidence/oracle-ubsan']
    subprocess.run(base+sanitizer,check=True)
    ub = []
    for case in range(8):
        run = subprocess.run(base+['/evidence/oracle-ubsan','--ub',str(case)],capture_output=True,text=True)
        assert run.returncode != 0 and 'runtime error:' in run.stderr, (case,run)
        ub.append(dict(case=case,returncode=run.returncode,stderr=run.stderr))
    asan_command = command[:-2] + ['-fsanitize=address','-fno-sanitize-recover=all','-o','/evidence/oracle-asan']
    subprocess.run(base+asan_command,check=True)
    run = subprocess.run(base+['/evidence/oracle-asan','--ub','8'],capture_output=True,text=True)
    assert run.returncode != 0 and 'heap-buffer-overflow' in run.stderr, run
    asan = dict(case=8,returncode=run.returncode,kind='heap-buffer-overflow')
    fixture = HERE/'fixtures/source_spline'
    fixture.mkdir(parents=True,exist_ok=True)
    (fixture/'cases.json').write_text(json.dumps(corpus,separators=(',',':'))+'\n')
    (fixture/'expected.tsv').write_bytes(result)
    receipt = dict(image=IMAGE,core_revision=PIN,source_sha256=hashes,compile_command=command,
                   oracle_sha256=hashlib.sha256(body.encode()).hexdigest(),inputs_sha256=hashlib.sha256(inputs).hexdigest(),
                   output_sha256=hashlib.sha256(result).hexdigest(),binary_sha256=hashlib.sha256((work/'oracle').read_bytes()).hexdigest(),
                   cases=len(corpus),rows=expected_rows,ubsan=ub,asan=asan,scope='Authored source numerical components, not sealed F1 execution')
    (fixture/'provenance.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    main()
