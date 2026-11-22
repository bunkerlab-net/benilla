#!/usr/bin/env python3
"""Record reference-image IFUNC resolution and explicit FMA-disabled comparisons."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import json
import os
from pathlib import Path
import subprocess
from generate_facing_fixture import IMAGE

CPP=r'''#include <cmath>
#include <cfenv>
#include <cstdint>
#include <cstring>
#include <dlfcn.h>
#include <iostream>
#include <cstdio>
#include <xmmintrin.h>
int main() {
 if (std::fegetround()!=FE_TONEAREST || (_mm_getcsr() & ((1u<<15)|(1u<<6)))) return 2;
 void* library=dlopen("libm.so.6",RTLD_NOW);
 if (!library) return 3;
 for (auto name:{"atan","atanf","atan2f","sinf","cosf","sqrtf","fmodf"}) {
  void* pointer=dlsym(library,name); Dl_info info{};
  if (!pointer || !dladdr(pointer,&info)) return 4;
  std::fprintf(stderr,"%s %s %lx %s\n",name,info.dli_fname,
    uintptr_t(pointer)-uintptr_t(info.dli_fbase),info.dli_sname?info.dli_sname:"hidden");
 }
 auto atan_function=reinterpret_cast<double(*)(double)>(dlsym(library,"atan"));
 uint32_t input;
 while (std::cin>>std::hex>>input) {
  float f; std::memcpy(&f,&input,4);
  double result=atan_function(double(f)); uint64_t output; std::memcpy(&output,&result,8);
  float narrowed=float(result);uint32_t single;std::memcpy(&single,&narrowed,4);
  std::cout<<std::hex<<input<<" "<<output<<" "<<single<<"\n";
 }
}
'''

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('evidence',type=Path);args=p.parse_args()
    root=args.evidence.resolve();root.mkdir(parents=True,exist_ok=True)
    (root/'dispatch.cpp').write_text(CPP)
    base=['docker','run','--rm','-i','--network=none','--user',f'{os.getuid()}:{os.getgid()}',
          '--mount',f'type=bind,src={root},dst=/evidence']
    flags=['g++','-std=c++17','-O3','-DNDEBUG','-ffp-contract=off','-fno-fast-math','-fno-omit-frame-pointer','-g','/evidence/dispatch.cpp','-ldl','-o','/evidence/dispatch']
    subprocess.run(base+[IMAGE]+flags,check=True)
    here=Path(__file__).resolve().parent
    rows=[line.split() for line in (here/'fixtures/source_atan/atan.tsv').read_text().splitlines() if line and not line.startswith('#')]
    inputs=('\n'.join(row[1] for row in rows)+'\n').encode()
    results={};symbols={}
    for name,env in [('default',[]),('without_fma_avx2',['-e','GLIBC_TUNABLES=glibc.cpu.hwcaps=-FMA,-AVX2'])]:
        run=subprocess.run(base+env+[IMAGE,'/evidence/dispatch'],input=inputs,capture_output=True,check=True)
        (root/f'dispatch-{name}.tsv').write_bytes(run.stdout)
        (root/f'dispatch-{name}-symbols.txt').write_bytes(run.stderr)
        values=[line.split() for line in run.stdout.decode().splitlines()]
        assert len(values)==len(rows)
        results[name]=values
        symbols[name]=[dict(zip(['function','library','offset','nearest_export'],line.split())) for line in run.stderr.decode().splitlines()]
        atan=next(s for s in symbols[name] if s['function']=='atan')
        address=int(atan['offset'],16)
        disasm=subprocess.check_output(base+[IMAGE,'objdump','-d',f'--start-address={address}',f'--stop-address={address+2048}',atan['library']])
        (root/f'dispatch-{name}-atan.disassembly').write_bytes(disasm)
    differences=[]
    for before,after,original in zip(results['default'],results['without_fma_avx2'],rows):
        if before!=after: differences.append({'name':original[0],'input':original[1],'default':before[1:],'without_fma_avx2':after[1:],'existing_non_fma_fixture':original[2:]})
    flags_line=next(line for line in Path('/proc/cpuinfo').read_text().splitlines() if line.startswith('flags'))
    (root/'cpuinfo-flags.txt').write_text(flags_line+'\n')
    library_hash=subprocess.check_output(base+[IMAGE,'sha256sum','/lib/x86_64-linux-gnu/libm.so.6'],text=True).strip()
    report={'image':IMAGE,'command':flags,'cpu_flags':flags_line.split(':',1)[1].split(),'libm_sha256':library_hash,
        'symbols':symbols,'inputs':len(rows),'variant_differences':differences,
        'status':'Observed current same-host dispatch. F0/F1 owner binding remains required before reference admission.'}
    (root/'libm-dispatch.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'symbols':symbols,'inputs':len(rows),'variant_differences':differences,'libm_sha256':library_hash}))

if __name__=='__main__':main()
