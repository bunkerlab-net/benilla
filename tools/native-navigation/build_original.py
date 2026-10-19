#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compile untouched original navigation generator translation units."""
import argparse
from pathlib import Path
import subprocess
import json
import hashlib
import os
from concurrent.futures import ThreadPoolExecutor

PIN='8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
HERE=Path(__file__).resolve().parent

def build(source, work):
    source=source.resolve(); work=work.resolve(); work.mkdir(parents=True,exist_ok=True)
    assert subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()==PIN
    paths=['contrib/mmap','contrib/extractor_scripts/config.json','contrib/extractor_scripts/offmesh.txt','src/game/vmap','src/game/Maps/GridMapDefines.h','src/game/MotionGenerators/MoveMapSharedDefines.h','dep/g3dlite','dep/recastnavigation','dep/json']
    subprocess.run(['git','-C',str(source),'diff','--exit-code','HEAD','--',*paths],check=True)
    from retain_source import check
    check(source)
    (work/'Platform').mkdir(exist_ok=True)
    (work/'Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
''')
    vmap=source/'src/game/vmap'; g3d=source/'dep/g3dlite'; recast=source/'dep/recastnavigation'; mmap=source/'contrib/mmap/src'
    sources=[mmap/(n+'.cpp') for n in ['MapBuilder','TerrainBuilder','IntermediateValues','VMapExtensions']]
    sources += [vmap/(n+'.cpp') for n in ['TileAssembler','WorldModel','BIH','ModelInstance','VMapManager2','MapTree']]
    sources += [g3d/(n+'.cpp') for n in ['Vector3','AABox','g3dmath','Matrix3','Ray','CollisionDetection','MemoryManager','ReferenceCount','System','debugAssert','format','prompt','Log','FileSystem','fileutils','stringutils','Crypto']]
    sources += sorted((recast/'Recast/Source').glob('*.cpp'))+sorted((recast/'Detour/Source').glob('*.cpp'))
    includes=[work,vmap,source/'src/game',g3d,mmap,recast,recast/'Recast/Include',recast/'Detour/Include',source/'dep/json']
    flags=['g++','-std=c++17','-O2','-fno-fast-math','-ffp-contract=off','-fno-strict-aliasing','-fno-access-control','-ffunction-sections','-fdata-sections','-DNO_CORE_FUNCS','-DMMAP_GENERATOR','-DDT_POLYREF64',*['-I'+str(p) for p in includes]]
    sanitizer=['-fsanitize=alignment','-fno-sanitize-recover=alignment'] if os.environ.get('BENILLA_ORACLE_SANITIZE_ALIGNMENT')=='1' else []
    flags+=sanitizer
    sources=[HERE/'oracle_driver.cpp',*sources]
    objects=[work/(str(i)+'-'+p.stem+'.o') for i,p in enumerate(sources)]
    commands=[flags+['-c',str(p),'-o',str(o)] for p,o in zip(sources,objects)]
    link=['g++',*sanitizer,*[str(p) for p in objects],'-Wl,--gc-sections','-Wl,--wrap=fopen','-Wl,--wrap=_Z17rcMergePolyMeshesP9rcContextPP10rcPolyMeshiRS1_','-pthread','-lz','-o',str(work/'original-navigation')]
    (work/'compile-command.json').write_text(json.dumps(commands+[link],indent=2)+'\n')
    def compile_one(args):
        src,obj,command=args
        stamp=obj.with_suffix('.json'); identity=hashlib.sha256(src.read_bytes()+json.dumps(flags).encode()).hexdigest()
        if not obj.exists() or not stamp.exists() or stamp.read_text(encoding="utf-8")!=identity:
            subprocess.run(command,check=True,timeout=180); stamp.write_text(identity)
    with ThreadPoolExecutor(max_workers=3) as executor:
        list(executor.map(compile_one,zip(sources,objects,commands)))
    subprocess.run(link,check=True,timeout=30)
    return work/'original-navigation'
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__); parser.add_argument('source',type=Path); parser.add_argument('work',type=Path)
    args=parser.parse_args(); build(args.source,args.work)
