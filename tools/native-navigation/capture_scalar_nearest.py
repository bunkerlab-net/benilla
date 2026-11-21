#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Original Detour nearest queries with scalar 5 extents and original on-surface inputs.

This adds separate evidence; it does not replace the original wide-height queries
or mutate any original generator fixture. Inputs are original nearest-point bits,
then the original query is actually executed again with half-extents (5,5,5).
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
from build_original import build,PIN
HERE=Path(__file__).resolve().parent

def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def encoded(value):return (json.dumps(value,indent=2,sort_keys=True)+'\n').encode()

def capture(source,work,fixtures,output):
    exe=build(source,work/'build');output.mkdir(parents=True,exist_ok=True)
    manifest={'schema':1,'source_pin':PIN,'scope':'Original Detour findNearestPoly and getPolyHeight; original on-surface inputs; scalar half-extents 5','cases':{}}
    for case in ['ridge_wmo','ridge_neighbors_wmo','ridge_xminus_wmo']:
        original=fixtures/case;base=work/('scalar-'+case);(base/'mmaps').mkdir(parents=True,exist_ok=True)
        inputs={name:sha(original/name) for name in ['000.mmap','0003130.mmtile','expected.json']}
        for name in ['000.mmap','0003130.mmtile']:shutil.copyfile(original/name,base/'mmaps'/name)
        target=output/(case+'.json')
        subprocess.run([str(exe),'scalar_queries',str(base),str(original/'expected.json'),str(target)],check=True,timeout=15)
        result=json.loads(target.read_text(encoding="utf-8"));expected=json.loads((original/'expected.json').read_text(encoding="utf-8"))
        assert result['extents_bits']==['40a00000']*3 and len(result['queries'])==25
        for index,row in enumerate(result['queries']):
            assert row['point_bits']==expected['output']['queries'][index]['nearest_bits']
            assert row['poly_ref'] and row['status']==0x40000000 and row['is_over_poly']
        manifest['cases'][case]={'original_files_sha256':inputs,'expected_sha256':sha(target),'query_count':len(result['queries'])}
    manifest['neighbor_fixture_inputs']=[]
    for x,y,name in [(30,31,'ridge'),(31,31,'ridge_xplus'),(29,31,'ridge_xminus'),(30,32,'ridge_yplus'),(30,30,'ridge_yminus')]:
        adt=fixtures/'authored'/(name+'.adt.test-fixture');terrain=fixtures/'authored'/(name+'.map')
        manifest['neighbor_fixture_inputs'].append({'tile_x':x,'tile_y':y,'adt_fixture':str(adt.relative_to(fixtures)),
            'adt_sha256':sha(adt),'map_fixture':str(terrain.relative_to(fixtures)),'map_sha256':sha(terrain),
            'original_map_filename':f'000{y:02}{x:02}.map','mpq_basename_template':f'{{directory}}_{x}_{y}.adt'})
    manifest['neighbor_tile_union']=[[29,31],[30,30],[30,31],[30,32],[31,31]]
    (output/'manifest.json').write_bytes(encoded(manifest))

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('source',type=Path);p.add_argument('--work',type=Path,default=HERE/'.work/full');p.add_argument('--check',action='store_true');a=p.parse_args()
    target=HERE/'fixtures/scalar-nearest';generated=a.work.resolve()/'scalar-nearest-expected'
    capture(a.source.resolve(),a.work.resolve(),HERE/'fixtures',generated)
    if a.check:
        assert {f.name:f.read_bytes() for f in generated.iterdir()}=={f.name:f.read_bytes() for f in target.iterdir()},'scalar nearest capture differs'
        print('Verified 75 original scalar nearest queries')
    else:
        target.mkdir(exist_ok=True)
        for path in generated.iterdir():shutil.copyfile(path,target/path.name)
        print('Captured 75 original scalar nearest queries')
