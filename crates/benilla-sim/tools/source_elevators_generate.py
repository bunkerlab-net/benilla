#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extract pinned G3D constructor and record authored elevator component bits."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
from source_transports_generate import method, PIN

IMAGE = 'sha256:7f8acd0cd855b63046cfdf49f21ed319ae6b981758f5fafe2100aed8b4ef21e8'


def cases():
    def bits(x):
        return struct.unpack('<I', struct.pack('<f', x))[0]
    result = []
    for a, b in [([1.25, -2.5, 3.75], [9.5, 8.25, -7.125]),
                 ([1.25, -2.5, 3.75], [1.25, -2.5, 3.75]),
                 ([-0.0, 0.0, -0.0], [0.0, -0.0, 0.0])]:
        for rotation in [[0, 0, 0, 1], [0, 0, 1, 1], [1, 2, 3, 4],
                         [0.125, -0.25, 0.375, -0.5], [1e-10, 2e-10, -3e-10, 4e-10]]:
            for progress in [100, 101, 333, 699, 700, 701, 0xffffffff]:
                result.append(list(map(bits, a + b)) + [progress, 100, 700]
                              + list(map(bits, rotation + [100.5, -200.25, 10.125])))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('evidence', type=Path)
    parser.add_argument('--prepare-only', action='store_true')
    args = parser.parse_args()
    assert subprocess.check_output(['git', '-C', str(args.source), 'rev-parse', 'HEAD'], text=True).strip() == PIN
    args.evidence.mkdir(parents=True, exist_ok=True)
    hashes = {}
    for name in ['dep/g3dlite/Matrix3.cpp', 'dep/g3dlite/G3D/Quat.h',
                 'dep/g3dlite/G3D/Matrix3.h', 'dep/g3dlite/G3D/Vector3.h',
                 'dep/g3dlite/G3D/g3dmath.h', 'src/game/Entities/Transports.cpp']:
        hashes[name] = hashlib.sha256((args.source/name).read_bytes()).hexdigest()
    constructor = method((args.source/'dep/g3dlite/Matrix3.cpp').read_text(), 'Matrix3::Matrix3(const Quat& _q)')
    constructor += '\n' + method((args.source/'dep/g3dlite/Matrix3.cpp').read_text(), 'void Matrix3::set(')
    body = '''#include <G3D/Vector3.h>
#include <G3D/Matrix3.h>
#include <G3D/Quat.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
namespace G3D {
''' + constructor + '''
}
static float f(uint32_t u) { float x; std::memcpy(&x,&u,4); return x; }
static uint32_t u(float x) { uint32_t v; std::memcpy(&v,&x,4); return v; }
int main(int argc, char** argv) {
    if(argc != 3 || !std::freopen(argv[1],"r",stdin) || !std::freopen(argv[2],"w",stdout)) return 2;
    uint32_t a[16];
    while(std::scanf("%x", &a[0]) == 1) {
        for(int i=1;i<16;++i) if(std::scanf("%x", &a[i]) != 1) return 3;
        G3D::Vector3 prev(f(a[0]),f(a[1]),f(a[2])), next(f(a[3]),f(a[4]),f(a[5]));
        G3D::Vector3 pos;
        if(prev == next) pos=prev;
        else { float t=float(a[6]-a[7])/float(a[8]-a[7]); pos=prev.lerp(next,t); }
        G3D::Quat q(f(a[9]),f(a[10]),f(a[11]),f(a[12]));
        pos=pos*q;
        pos.y=-pos.y;
        pos+=G3D::Vector3(f(a[13]),f(a[14]),f(a[15]));
        for(auto word:a) std::printf("%08x ",word);
        std::printf("%08x %08x %08x\\n",u(pos.x),u(pos.y),u(pos.z));
    }
}
'''
    source = args.evidence/'oracle.cpp'
    source.write_text(body)
    inputs = args.evidence/'inputs.tsv'
    inputs.write_text(''.join(' '.join(f'{x:08x}' for x in case)+'\n' for case in cases()))
    command = ['g++', '-std=c++17', '-O3', '-DNDEBUG', '-ffp-contract=off', '-fno-fast-math',
               '-fno-omit-frame-pointer', '-g', '-I/source/dep/g3dlite',
               '/evidence/oracle.cpp', '-o', '/evidence/oracle']
    receipt = {'image': IMAGE, 'compile': command, 'sources': hashes, 'cases': len(cases()),
               'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
               'inputs_sha256': hashlib.sha256(inputs.read_bytes()).hexdigest()}
    if not args.prepare_only:
        assert subprocess.check_output(['docker', 'image', 'inspect', '--format', '{{.Id}}', 'benilla-cmangos-reference:f1-proven-20261106'], text=True).strip() == IMAGE
        subprocess.run(['docker', 'run', '--rm', '--network', 'none', '-v', f'{args.source.resolve()}:/source:ro',
                        '-v', f'{args.evidence.resolve()}:/evidence', IMAGE,
                        'bash', '-lc', ' '.join(command)+' && /evidence/oracle /evidence/inputs.tsv /evidence/expected.tsv'], check=True)
        result = (args.evidence/'expected.tsv').read_text().splitlines()
        assert len(result) == len(cases())
        assert [line.split()[:16] for line in result] == [[f'{x:08x}' for x in row] for row in cases()]
        for name in ['oracle', 'expected.tsv']:
            receipt[name+'_sha256'] = hashlib.sha256((args.evidence/name).read_bytes()).hexdigest()
    (args.evidence/'oracle-receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    main()
