#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Retain original source evidence and audit fixed pinned hashes."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from build_original import PIN
HERE=Path(__file__).resolve().parent
FILES=['contrib/mmap/src/MapBuilder.cpp','contrib/mmap/src/TerrainBuilder.cpp','contrib/mmap/src/VMapExtensions.cpp','contrib/mmap/src/MapBuilder.h','contrib/mmap/src/TerrainBuilder.h','contrib/extractor_scripts/config.json','contrib/extractor_scripts/offmesh.txt']

def manifest(source):
    paths=set(FILES)
    for folder in ['contrib/mmap/src','src/game/vmap','dep/recastnavigation/Recast','dep/recastnavigation/Detour','dep/g3dlite','dep/json']:
        paths.update(str(p.relative_to(source)) for p in (source/folder).rglob('*') if p.is_file() and p.suffix in ['.cpp','.h','.hpp'])
    paths.update(['src/game/Maps/GridMapDefines.h','src/game/MotionGenerators/MoveMapSharedDefines.h'])
    return {name:hashlib.sha256((source/name).read_bytes()).hexdigest() for name in sorted(paths)}

def evidence(source):
    assert subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()==PIN
    out={};intervals=[]
    for path in FILES:
        data=(source/path).read_bytes();out[Path(path).name+'.test-fixture']=data
        intervals.append({'source':path,'first_byte':0,'last_byte_exclusive':len(data),'first_line':1,'last_line':data.count(b'\n'),'sha256':hashlib.sha256(data).hexdigest()})
    out['source-intervals.json']=(json.dumps(intervals,indent=2)+'\n').encode()
    out['source-hashes.json']=(json.dumps(manifest(source),indent=2,sort_keys=True)+'\n').encode()
    return out

def check(source):
    for name,data in evidence(source).items():
        assert (HERE/'source'/name).read_bytes()==data,'pinned source evidence drift: '+name

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('source',type=Path);parser.add_argument('--check',action='store_true');a=parser.parse_args()
    if a.check:check(a.source)
    else:
        (HERE/'source').mkdir(exist_ok=True)
        for name,data in evidence(a.source).items():(HERE/'source'/name).write_bytes(data)
