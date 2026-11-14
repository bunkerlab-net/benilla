#!/usr/bin/env python3
"""Measure existing 2.41 fixtures against the actual F0 glibc 2.36 builder."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from generate_facing_fixture import IMAGE, PIN, extract as method

HERE=Path(__file__).resolve().parent

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source',type=Path)
    parser.add_argument('evidence',type=Path)
    parser.add_argument('--baseline-facing',type=Path,required=True)
    args=parser.parse_args()
    assert subprocess.check_output(['git','-C',str(args.source),'rev-parse','HEAD'],text=True).strip()==PIN
    work=args.evidence.resolve(); work.mkdir(parents=True,exist_ok=True)
    harness=(HERE/'facing_oracle.cpp').read_text()
    obj=(args.source/'src/game/Entities/Object.cpp').read_text()
    funcs='\n'.join(method(obj,s) for s in ['float WorldObject::GetAngleAt(float x, float y, float ox, float oy)',
        'float WorldObject::GetAngleAt(float x, float y, const WorldObject* obj) const',
        'bool WorldObject::HasInArcAt(float x, float y, float o, const WorldObject* target, float arc'])
    normalize=method((args.source/'src/game/Maps/MapManager.h').read_text(),'static float NormalizeOrientation(float o)')
    pi=next(line for line in (args.source/'src/shared/Common.h').read_text().splitlines() if 'define M_PI_F ' in line)
    harness=harness.replace('PI_DEFINITION',pi).replace('MAP_BODY',normalize).replace('FUNCTIONS',funcs)
    (work/'facing-audit.cpp').write_text(harness)
    generic=r'''#include <cmath>
#include <cfenv>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <iomanip>
#include <xmmintrin.h>
static uint32_t bits(float f) { uint32_t u; std::memcpy(&u,&f,4); return u; }
static uint64_t bits(double f) { uint64_t u; std::memcpy(&u,&f,8); return u; }
int main(int argc,char**) {
 if (std::fegetround()!=FE_TONEAREST || (_mm_getcsr() & ((1u<<15)|(1u<<6)))) return 2;
 uint32_t a;
 float (*volatile sine)(float)=::sinf;
 float (*volatile cosine)(float)=::cosf;
 double (*volatile arctan)(double)=::atan;
 while (std::cin>>std::hex>>a) {
  float f; std::memcpy(&f,&a,4);
  std::cout<<std::hex<<a;
  if (argc>1) { double d=arctan(double(f)); std::cout<<" "<<bits(d)<<" "<<bits(float(d)); }
  else std::cout<<" "<<bits(sine(f))<<" "<<bits(cosine(f));
  std::cout<<"\n";
 }
}
'''
    (work/'scalar-audit.cpp').write_text(generic)
    base=['docker','run','--rm','-i','--network=none','--user',f'{__import__("os").getuid()}:{__import__("os").getgid()}',
          '--mount',f'type=bind,src={work},dst=/evidence',IMAGE]
    flags=['g++','-std=c++17','-O3','-DNDEBUG','-ffp-contract=off','-fno-fast-math','-fno-omit-frame-pointer','-g']
    for name in ['facing','scalar']:
        subprocess.run(base+flags+[f'/evidence/{name}-audit.cpp','-o',f'/evidence/{name}-audit'],check=True)
    facing=[line.split() for line in args.baseline_facing.read_text().splitlines() if line and not line.startswith('#')]
    inputs='\n'.join(' '.join(row[1:-2]) for row in facing)+'\n'
    output=subprocess.check_output(base+['/evidence/facing-audit'],input=inputs.encode()).decode().splitlines()
    assert len(output)==len(facing)
    mismatches=[]; counts={}
    new_rows=[]
    for row,line in zip(facing,output):
        actual=line.split()[0]
        stat=counts.setdefault(row[1],{'inputs':0,'different':0})
        stat['inputs']+=1
        if int(actual,16)!=int(row[-2],16):
            stat['different']+=1; mismatches.append({'name':row[0],'operation':row[1],'inputs':row[2:-2],'old':row[-2],'new':actual})
        new_rows.append(' '.join(row[:-2]+[actual,'0']))
    (work/'facing236.tsv').write_text('# Pinned glibc 2.36 source-body results. No CORE-MATH rare-branch marker applies.\n'+'\n'.join(new_rows)+'\n')
    atan=[line.split() for line in (HERE/'fixtures/source_atan/atan.tsv').read_text().splitlines() if line and not line.startswith('#')]
    output=subprocess.check_output(base+['/evidence/scalar-audit','atan'],input=('\n'.join(row[1] for row in atan)+'\n').encode()).decode().splitlines()
    assert len(output)==len(atan)
    atan_different=[]
    for row,line in zip(atan,output):
        fields=line.split()
        if [int(x,16) for x in fields[1:]]!=[int(x,16) for x in row[2:]]: atan_different.append({'name':row[0],'input':row[1],'old':row[2:],'new':fields[1:]})
    trig_file=HERE/'../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/trig-cases.test-fixture'
    trig=[line.split() for line in trig_file.read_text().splitlines() if line and not line.startswith('#')]
    output=subprocess.check_output(base+['/evidence/scalar-audit'],input=('\n'.join(row[0] for row in trig)+'\n').encode()).decode().splitlines()
    assert len(output)==len(trig)
    trig_different=[{'old':row,'new':line.split()} for row,line in zip(trig,output) if [int(x,16) for x in row]!=[int(x,16) for x in line.split()]]
    # Strided raw binary32 samples cover every exponent through the widest admitted assembler range.
    angles=sorted(set([0,0x80000000,0x40c90fdb,0xc0c90fdb,0x4196cbe4,0xc196cbe4,0x41fb53d1,0xc1fb53d1]+
        [m|sign for m in range(0,0x41fb53d2,4093) for sign in [0,0x80000000]]))
    native=subprocess.check_output(base+['/evidence/scalar-audit'],input=('\n'.join(f'{x:08x}' for x in angles)+'\n').encode())
    assert len(native.splitlines())==len(angles)
    (work/'trig236-grid.tsv').write_bytes(native)
    report={'image':IMAGE,'flags':flags,'core_revision':PIN,'facing':counts,'facing_differences':mismatches,
        'atan_inputs':len(atan),'atan_differences':atan_different,'trig_inputs':len(trig),'trig_differences':trig_different,
        'baseline_facing_sha256':hashlib.sha256(args.baseline_facing.read_bytes()).hexdigest(),
        'trig_grid_inputs':len(angles),'grid_stride':4093,'scope':'Component corpus and strided ranges, not all possible inputs or sealed F1 reached-input acceptance'}
    (work/'libm236-audit.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k not in ['facing_differences','atan_differences','trig_differences']}))
    print('facing differences',len(mismatches),'atan differences',len(atan_different),'trig differences',len(trig_different))

if __name__=='__main__': main()
