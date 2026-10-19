#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Author a continuous terrain ridge ADT and run original guarded ConvertADT."""
import argparse
import importlib.util
from pathlib import Path
import shutil
import subprocess
import json
import sys
sys.dont_write_bytecode=True
HERE=Path(__file__).resolve().parent
TERRAIN=HERE.parents[1]/'crates/benilla-native-terrain/tools'

def module():
    spec=importlib.util.spec_from_file_location('original_terrain_fixture_author',TERRAIN/'generate_terrain_writer_fixture.py')
    m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m); return m

def height(x,y):
    return 4.0 + y/32.0 + max(0.0,32.0-abs(x-64.0))*1.25

OFFSETS={"ridge":(0,0),"ridge_xplus":(128,0),"ridge_xminus":(-128,0),"ridge_yplus":(0,128),"ridge_yminus":(0,-128)}

def cell_values(case,row,col):
    dx,dy=OFFSETS[case]
    values=[]
    for y in range(9):
        values += [height(dx+col*8+x,dy+row*8+y) for x in range(9)]
        if y<8: values += [height(dx+col*8+x+0.5,dy+row*8+y+0.5) for x in range(8)]
    return 0.0,values,1,0

def generate(source,out,height_scale=1.0,quantize=False):
    out.mkdir(parents=True,exist_ok=True); m=module()
    m.CASES.update({case:'Authored continuous broad ridge and gentle perpendicular slope' for case in OFFSETS})
    def scaled_values(case,row,col):
        base,values,area,holes=cell_values(case,row,col)
        return base*height_scale,[v*height_scale for v in values],area,holes
    m.cell_values=scaled_values
    for case in OFFSETS: (out/(case+'.adt')).write_bytes(m.authored_adt(case))
    for name,data in m.authored_common().items(): (out/name).write_bytes(data)
    sources,intervals,fragments,notice=m.extract(source)
    build=out/'writer-build';build.mkdir(exist_ok=True)
    for name,text in sources.items():
        if name==m.SYSTEM: continue
        relative=name.removeprefix('contrib/extractor/') if name.startswith('contrib/extractor/') else 'GridMapDefines.h'
        path=build/relative;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text)
    shutil.copyfile(TERRAIN/'fixture_mpq.h',build/'mpq_libmpq.h')
    harness=(TERRAIN/'terrain_writer_oracle.cpp').read_text(encoding="utf-8")
    if quantize:
        assert harness.count('CONF_allow_float_to_int = false;')==1
        harness=harness.replace('CONF_allow_float_to_int = false;','CONF_allow_float_to_int = true;')
    for marker,fragment in fragments.items():
        assert harness.count(marker)==1;harness=harness.replace(marker,fragment)
    (build/'oracle.cpp').write_text(notice+harness)
    command=['g++','-std=c++20','-O0','-fno-strict-aliasing','-fno-fast-math','-ffp-contract=off','-frounding-math','-fsanitize=undefined','-fno-sanitize=alignment','-fno-sanitize-recover=all',str(build/'oracle.cpp'),str(build/'loadlib/adt.cpp'),str(build/'loadlib/loadlib.cpp'),str(build/'dbcfile.cpp'),'-o',str(build/'oracle')]
    subprocess.run(command,check=True,timeout=60)
    subprocess.run([str(build/'oracle'),str(out.resolve()),*OFFSETS],check=True,timeout=20)
    (out/'writer-provenance.json').write_text(json.dumps({'source_hashes':m.HASHES,'source_intervals':intervals,'flags':command[1:10]},indent=2)+'\n')
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('source',type=Path);p.add_argument('output',type=Path);a=p.parse_args();generate(a.source,a.output)
