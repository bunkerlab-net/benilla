#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Authored numeric bounds through the unchanged pinned original BIH builder."""
import argparse
import hashlib
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile

PIN='8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
HERE=Path(__file__).resolve().parent

def cases():
    yield 'empty',3,[]
    yield 'zero',3,[[0.0]*6]
    yield 'signed_zero',1,[[0.0]*6,[-0.0]*6]
    for count in [2,3,4,7,16,31,64,127]:
        for leaf in [1,3]:
            yield f'line_{count}_{leaf}',leaf,[[float(i*3),0.,0.,float(i*3+1),1.,1.] for i in range(count)]
            yield f'identical_{count}_{leaf}',leaf,[[0.,0.,0.,1.,1.,1.]]*count
            yield f'point_{count}_{leaf}',leaf,[[float(i),float(i),float(i)]*2 for i in range(count)]
    tiny=struct.unpack('<f',struct.pack('<I',1))[0]
    yield 'subnormal_points',1,[[i*tiny,0.,0.]*2 for i in range(16)]
    yield 'fuzzy_adjacent',1,[[i*0.000001,0.,0.,i*0.000001+0.0000001,0.,0.] for i in range(32)]
    yield 'extreme_finite',1,[[(-1.0 if i%2 else 1.0)*2.0**i,0.,0.]*2 for i in range(100)]
    yield 'center_ties',1,[[-float(i),-float(i),-float(i),float(i),float(i),float(i)] for i in range(32)]
    rng=random.Random(5875)
    for case in range(128):
        bounds=[]
        for _ in range(4+case%93):
            low=[float(rng.randrange(-200,201)) for _ in range(3)]
            high=[v+float(rng.randrange(0,17)) for v in low]
            bounds.append(low+high)
        yield f'random_{case}',1 if case%2 else 3,bounds
    for case in range(16):
        # Widely separated outer boxes with tightly clustered inner boxes.
        bounds=[[-1024.,-1024.,-1024.,-1023.,-1023.,-1023.],[1023.,1023.,1023.,1024.,1024.,1024.]]
        bounds += [[float(i%3),float(i%5),float(i%7),float(i%3)+0.01,float(i%5)+0.01,float(i%7)+0.01] for i in range(8+case)]
        yield f'cluster_{case}',1,bounds

def main():
    parser=argparse.ArgumentParser();parser.add_argument('source',type=Path);parser.add_argument('--check',action='store_true');args=parser.parse_args()
    assert sys.byteorder == 'little', 'Original binary fixture profile is little-endian'
    source=args.source.resolve()
    assert subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()==PIN
    subprocess.run(['git','-C',str(source),'diff','--exit-code','HEAD','--','src/game/vmap/BIH.cpp','src/game/vmap/BIH.h','dep/g3dlite'],check=True,stdout=subprocess.DEVNULL)
    harness=r'''
#include <cstdio>
#include <cfenv>
#include <limits>
#include <vector>
#include "BIH.h"
int main(int argc,char**argv){
 static_assert(sizeof(float)==4 && std::numeric_limits<float>::is_iec559);
 if(std::fesetround(FE_TONEAREST)!=0)return 7;
 if(argc!=3)return 1;
 FILE*f=fopen(argv[1],"rb");if(!f)return 2;
 uint32 count,leaf;if(fread(&count,4,1,f)!=1||fread(&leaf,4,1,f)!=1)return 3;
 std::vector<AABox> values;
 for(uint32 i=0;i<count;++i){float v[6];if(fread(v,4,6,f)!=6)return 4;values.emplace_back(Vector3(v[0],v[1],v[2]),Vector3(v[3],v[4],v[5]));}
 fclose(f);BIH bih;auto bounds=[](const AABox&v,AABox&out){out=v;};bih.build(values,bounds,leaf);
 FILE*out=fopen(argv[2],"wb");if(!out)return 5;bool ok=bih.writeToFile(out);fclose(out);return ok?0:6;
}
'''
    rows=['# name\tleaf\tinput-f32-le-hex\toriginal-bih-hex']
    with tempfile.TemporaryDirectory(prefix='benilla-source-bih-') as directory:
        root=Path(directory);(root/'Platform').mkdir()
        (root/'Platform/Define.h').write_text('#pragma once\n#include <cstdint>\nusing uint32=std::uint32_t;using uint64=std::uint64_t;using int32=std::int32_t;\n')
        (root/'probe.cpp').write_text(harness)
        vmap=source/'src/game/vmap';g3d=source/'dep/g3dlite'
        command=['g++','-std=c++17','-O0','-fno-fast-math','-ffp-contract=off','-fno-strict-aliasing','-ffunction-sections','-fdata-sections','-DNO_CORE_FUNCS','-I'+str(root),'-I'+str(vmap),'-I'+str(g3d),str(root/'probe.cpp'),str(vmap/'BIH.cpp')]
        command += [str(g3d/(name+'.cpp')) for name in ['Vector3','AABox','g3dmath']]
        command += ['-Wl,--gc-sections','-o',str(root/'probe')]
        subprocess.run(command,check=True,timeout=60)
        for name,leaf,bounds in cases():
            payload=b''.join(struct.pack('<6f',*b) for b in bounds)
            (root/'input').write_bytes(struct.pack('<2I',len(bounds),leaf)+payload)
            subprocess.run([str(root/'probe'),str(root/'input'),str(root/'output')],check=True,timeout=10)
            rows.append('\t'.join([name,str(leaf),payload.hex(),(root/'output').read_bytes().hex()]))
    output=('\n'.join(rows)+'\n').encode();target=HERE/'fixtures/source-bih.tsv'
    if args.check:
        assert target.read_bytes()==output,'Original BIH fixtures differ'
    else:
        target.write_bytes(output)
    print(f'{len(rows)-1} original BIH fixtures; sha256={hashlib.sha256(output).hexdigest()}')

if __name__=='__main__':main()
