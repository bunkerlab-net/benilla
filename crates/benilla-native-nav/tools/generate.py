#!/usr/bin/env python3
"""Authored MMap bytes + direct pinned Detour differential oracle; never game assets."""
import argparse, hashlib, pathlib, subprocess, tempfile
HERE=pathlib.Path(__file__).resolve().parent
p=argparse.ArgumentParser();p.add_argument('--source',type=pathlib.Path,required=True);p.add_argument('--check',action='store_true');a=p.parse_args()
source=a.source/'dep/recastnavigation'
for line in (HERE.parent/'native/UPSTREAM-SHA256SUMS').read_text().splitlines():
    digest,name=line.split();assert hashlib.sha256((source/name).read_bytes()).hexdigest()==digest,name
with tempfile.TemporaryDirectory() as temp:
    temp=pathlib.Path(temp);exe=temp/'oracle'
    files=['DetourAlloc','DetourAssert','DetourCommon','DetourNavMesh','DetourNavMeshBuilder','DetourNavMeshQuery','DetourNode']
    subprocess.run(['c++','-std=c++20','-O2','-DDT_POLYREF64=1','-fno-fast-math','-ffp-contract=off','-I'+str(source/'Detour/Include'),str(HERE/'fixture.cpp'),*[str(source/'Detour/Source'/f'{f}.cpp') for f in files],'-o',str(exe)],check=True)
    subprocess.run([str(exe),str(temp)],check=True)
    for name in ['authored.mmap','0003232.mmtile','0003231.mmtile','0003232-odd.mmtile','long300.mmtile','long600.mmtile','oracle.tsv']:
        data=(temp/name).read_bytes();dest=HERE/'fixtures'/name
        if a.check:assert dest.read_bytes()==data,name
        else:dest.write_bytes(data)
        print(name,len(data),hashlib.sha256(data).hexdigest())
