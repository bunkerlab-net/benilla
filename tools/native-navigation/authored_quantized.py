#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Original ConvertADT -f1 integer maps -> unchanged original TerrainBuilder."""
import argparse
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import zlib
from authored_ridge import generate
from build_original import build
from run_oracle import json_bytes,mesh_info,sha
HERE=Path(__file__).resolve().parent

def capture(source,work,output):
    exe=build(source,work/'build');output.mkdir(parents=True,exist_ok=True);manifest={}
    for name,scale,expected_flag in [('int8',1/64,4),('int16',1,2)]:
        writer=work/('quantized-writer-'+name);generate(source,writer,height_scale=scale,quantize=True)
        base=work/('quantized-'+name)
        for folder in ['maps','vmaps','mmaps']:(base/folder).mkdir(parents=True,exist_ok=True)
        data=(writer/'ridge.map').read_bytes();height_offset=struct.unpack_from('<I',data,16)[0]
        assert struct.unpack_from('<I',data,height_offset+4)[0]==expected_flag
        shutil.copyfile(writer/'ridge.map',base/'maps/0003130.map')
        env=os.environ.copy();env['BENILLA_ORACLE_COLLECT_ONLY']='1'
        subprocess.run([str(exe),str(base),'0','30','31',str(source/'contrib/extractor_scripts/config.json'),str(source/'contrib/extractor_scripts/offmesh.txt')],check=True,env=env,stdout=subprocess.DEVNULL,timeout=15)
        target=output/name;target.mkdir(exist_ok=True)
        shutil.copyfile(writer/'ridge.map',target/'input.map');shutil.copyfile(writer/'ridge.adt',target/'input.adt.test-fixture')
        result=json.loads((base/'source-result.json').read_text(encoding="utf-8"));(target/'expected.json').write_bytes(json_bytes(result))
        entry={'expected':result,'height_flags':expected_flag,'map_sha256':sha(data),'adt_sha256':sha((writer/'ridge.adt').read_bytes()),'writer_profile':'Original ConvertADT -f1; source option enabled by driver'}
        for array in ['source-terrain.bin','source-mesh.bin']:
            raw=(base/array).read_bytes();entry[array]=mesh_info(raw);(target/(array+'.zlib')).write_bytes(zlib.compress(raw,9))
        manifest[name]=entry
    (output/'manifest.json').write_bytes(json_bytes(manifest))

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('source',type=Path);p.add_argument('--work',type=Path,default=HERE/'.work/full');p.add_argument('--output',type=Path,default=HERE/'fixtures/quantized');a=p.parse_args();capture(a.source.resolve(),a.work.resolve(),a.output.resolve())
