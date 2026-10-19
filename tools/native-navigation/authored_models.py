#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Authored raw collision records, assembled/collected only by original source."""
import argparse
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import zlib
from build_original import build
from run_oracle import mesh_info,json_bytes,sha
HERE=Path(__file__).resolve().parent

def array(tag,count,payload):return tag+struct.pack('<II',4+len(payload),count)+payload

def group(number,liquid_type=None,all_hidden=False):
    vertices=[(-1.,-2.,-3.),(4.,0.,6.),(0.,5.,1.),(-0.,0.,-0.)]
    out=struct.pack('<II6fI',0x2000,number,-1,-2,-3,4,5,6,liquid_type is not None)
    out+=array(b'GRP ',1,struct.pack('<I',1))+array(b'INDX',3,struct.pack('<3H',0,1,2))
    out+=array(b'VERT',4,b''.join(struct.pack('<3f',*v) for v in vertices))
    if liquid_type is not None:
        # Rectangular2x3 tiles /3x4 vertices, source raw liquid ABI32bytes.
        header=struct.pack('<4I3fH2x',3,4,2,3,-2.5,1.25,0.,liquid_type)
        heights=struct.pack('<12f',*[7.0+y/2+x/4 for y in range(4) for x in range(3)])
        flags=bytes([0x0f]*6 if all_hidden else [0,0x0f,0x8f,0x80,1,0xff])
        payload=header+heights+flags;out+=b'LIQU'+struct.pack('<I',len(payload))+payload
    return out

def raw(groups):return b'VMAPs05\0'+struct.pack('<III',len(groups),len(groups),5875)+b''.join(groups)

def spawn(identifier,name,position=(16600.,16100.,4.),rotation=(0.,0.,0.),scale=1.,tile=(30,31),flags=4):
    result=struct.pack('<4IHI7f',0,*tile,flags,11,identifier,*position,*rotation,scale)
    if flags&4:result+=struct.pack('<6f',-100000,-100000,-100000,100000,100000,100000)
    return result+struct.pack('<I',len(name))+name

def cases():
    for name in [b'Hull.wmo',b'Foo.m2',b'Foo.M2',b'Foo.m2.wmo']:
        label='filename_'+name.decode().replace('.','_')
        yield label,spawn(90,name,rotation=(17.,-31.,67.),scale=1.5),{name.decode():raw([group(23),group(24)])}
    yield 'wmo_flagged_m2',spawn(90,b'Hull.wmo',flags=5,rotation=(17.,-31.,67.),scale=1.5),{'Hull.wmo':raw([group(23)])}
    yield 'signed_zero',spawn(90,b'Hull.wmo',position=(0.,-0.,-0.),rotation=(-0.,0.,-0.),scale=1.),{'Hull.wmo':raw([group(23)])}
    for liquid_type in [0,1,2,3,5,7]:
        yield 'rect_liquid_'+str(liquid_type),spawn(90,b'Hull.wmo',rotation=(17.,-31.,67.),scale=1.5),{'Hull.wmo':raw([group(23,liquid_type)])}
    yield 'rect_liquid_all_hidden',spawn(90,b'Hull.wmo'),{'Hull.wmo':raw([group(23,0,True)])}
    ids=[90,10,50,70,30,110,5]
    rows=[spawn(i,b'Foo.m2' if n%2 else b'Hull.wmo',position=(16500+n*20,16000+n*5,float(n)),rotation=(n*7.,n*-11.,n*13.),scale=1+n/8) for n,i in enumerate(ids)]
    rows+=[spawn(90,b'Decoy.wmo',position=(999,999,999),tile=(31,31)),spawn(90,b'Decoy.wmo',position=(999,999,999))]
    yield 'shared_slots_groups',b''.join(rows),{'Hull.wmo':raw([group(23,0),group(24,2)]),'Foo.m2':raw([group(25)]),'Decoy.wmo':raw([group(99)])}

def capture(source,work,output):
    exe=build(source,work/'build');output.mkdir(parents=True,exist_ok=True);manifest={}
    for case,records,models in cases():
        base=work/case
        for name in ['raw','maps','vmaps','mmaps']:(base/name).mkdir(parents=True,exist_ok=True)
        (base/'raw/dir_bin').write_bytes(records)
        for name,data in models.items():
            assert '/' not in name and '\0' not in name;(base/'raw'/name).write_bytes(data)
        subprocess.run([str(exe),'assemble',str(base/'raw'),str(base/'vmaps')],check=True,stdout=subprocess.DEVNULL,timeout=15)
        env=os.environ.copy();env['BENILLA_ORACLE_COLLECT_ONLY']='1'
        subprocess.run([str(exe),str(base),'0','30','31',str(source/'contrib/extractor_scripts/config.json'),str(source/'contrib/extractor_scripts/offmesh.txt')],check=True,env=env,stdout=subprocess.DEVNULL,timeout=15)
        result=json.loads((base/'source-result.json').read_text(encoding="utf-8"));assert result['vmap_loaded']
        target=output/case;target.mkdir(exist_ok=True)
        inputs={'dir_bin_hex':records.hex(),'raw_models':{n:d.hex() for n,d in models.items()},'products':{p.name:p.read_bytes().hex() for p in sorted((base/'vmaps').iterdir())}}
        (target/'inputs.json').write_bytes(json_bytes(inputs));(target/'expected.json').write_bytes(json_bytes(result))
        entry={'expected':result}
        for name in ['source-combined.bin','source-mesh.bin']:
            data=(base/name).read_bytes();entry[name]=mesh_info(data);(target/(name+'.zlib')).write_bytes(zlib.compress(data,9))
        manifest[case]=entry
        print(case,result['combined_vertices_before_clean'],result['combined_triangles'],result['liquid_vertices'],result['liquid_triangles'],flush=True)
    (output/'manifest.json').write_bytes(json_bytes(manifest))
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('source',type=Path);p.add_argument('--work',type=Path,default=HERE/'.work/models');p.add_argument('--output',type=Path,default=HERE/'fixtures/models');a=p.parse_args();capture(a.source.resolve(),a.work.resolve(),a.output.resolve())
