#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent original-source navigation captures, without Benilla production code.

Run with pinned source checkout; --check compares deterministic checked-in fixtures.
Mutable build/run scratch stays beneath --work. Peak RSS applies to the standalone
original process and includes original source collection, generation and queries.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import struct
import subprocess
import sys
import time
import zlib
from build_original import build, PIN
from authored_ridge import generate as generate_ridge, OFFSETS

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
TERRAIN=ROOT/'crates/benilla-native-terrain/tools/fixtures'
VMAP=ROOT/'crates/benilla-native-vmap-source/tools/fixtures/map-products.tsv'

def sha(data):return hashlib.sha256(data).hexdigest()
def json_bytes(value):return (json.dumps(value,indent=2,sort_keys=True)+'\n').encode()

def mesh_info(raw):
    result={};offset=0
    for name in ('solid','liquid'):
        nv,nt=struct.unpack_from('<II',raw,offset);offset+=8
        vertices=raw[offset:offset+12*nv];offset+=12*nv
        triangles=raw[offset:offset+12*nt];offset+=12*nt
        value={'vertex_count':nv,'triangle_count':nt,'vertices_sha256':sha(vertices),'triangles_sha256':sha(triangles)}
        if name=='liquid':value['types_sha256']=sha(raw[offset:offset+nt]);offset+=nt
        result[name]=value
    assert offset==len(raw)
    return {'bytes':len(raw),'sha256':sha(raw),'arrays':result}

def request(base,result,budget=256*1024*1024,tile_x=30,tile_y=31):
    # Protocol encoding only. Every algorithmic input comes from original capture.
    output=b'BNAVIN01'+struct.pack('<4IQ',0,tile_x,tile_y,1,budget)
    output+=(base/'mmaps/000.mmap').read_bytes()
    for key in ('bounds_min_bits','bounds_max_bits'):
        output+=b''.join(struct.pack('<I',int(v,16)) for v in result[key])
    for name,fmt in [('borderSize','I'),('walkableSlopeAngle','f'),('walkableHeight','I'),('walkableClimb','I'),('walkableRadius','I'),('maxEdgeLen','I'),('maxSimplificationError','f'),('minRegionArea','I'),('mergeRegionArea','I'),('detailSampleDist','f'),('detailSampleMaxError','f'),('liquidFlagMergeThreshold','f')]:
        value=result['config'][name];output+=struct.pack('<'+fmt,int(value) if fmt=='I' else value)
    return output+(base/'source-mesh.bin').read_bytes()

def verify_liquid_writer(ridge):
    # Replay the retained input through the already-built unchanged ConvertADT.
    # Do not reauthor it or replace its independently captured map expectation.
    name='liquid_full_types'
    shutil.copyfile(TERRAIN/(name+'.adt.test-fixture'),ridge/(name+'.adt'))
    subprocess.run([str(ridge/'writer-build/oracle'),str(ridge),name],check=True,timeout=20)
    assert (ridge/(name+'.map')).read_bytes()==(TERRAIN/(name+'.map')).read_bytes(),'original liquid ConvertADT differs'

def check_generation(case,result):
    output=result['output']
    if case=='ridge_only':
        assert not output['emitted'] and output['merged_polygons']==625
    elif case=='liquid_full_types_wmo':
        # Measured source output is entirely wet. Ground-only misses are evidence,
        # not a reason to relax the filter or manufacture a successful corridor.
        assert output['emitted'] and output['merged_polygons']==998,case
        assert output['areas']=={'8':228,'9':770},case
        assert output['query_filter']=={'include_flags':1,'exclude_flags':0},case
        assert len(output['queries'])==25 and not output['paths'],case
        assert all(q['status']==0x40000000 and q['poly_ref']==0 for q in output['queries']),case
    else:
        assert output['emitted'],case
        assert any(len(path['corridor'])>1 and path['corridor'][-1]==path['end_ref'] for path in output['paths']),case

def worker(args):
    # Separate Python process keeps RUSAGE_CHILDREN from prior compiler/runs.
    base=Path(args.worker[0]);command=args.worker[1:];started=time.monotonic()
    with (base/'run.log').open('w') as log:
        run=subprocess.run(command,check=False,stdout=log,stderr=subprocess.STDOUT,timeout=120)
    measured={'wall_seconds':round(time.monotonic()-started,6),'max_rss_kib':resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,'exit_code':run.returncode}
    (base/'resource.json').write_bytes(json_bytes(measured))
    if run.returncode:raise SystemExit(run.returncode)

def capture(source,work,output,verify_inputs=True):
    output.mkdir(parents=True,exist_ok=True);work.mkdir(parents=True,exist_ok=True)
    exe=build(source,work/'build'); ridge=work/'ridge-input';generate_ridge(source,ridge)
    verify_liquid_writer(ridge)
    vmaps=None
    for line in VMAP.read_text(encoding="utf-8").splitlines():
        if line.startswith('captured_wmo\t'):
            fields=line.split('\t');vmaps=json.loads(fields[3]);captured_records=bytes.fromhex(fields[1]);captured_models=json.loads(fields[2])
    assert vmaps is not None and set(vmaps)=={'000.vmtree','000_30_31.vmtile','Hull.wmo.vmo'}
    if verify_inputs:
        terrain_manifest=json.loads((TERRAIN/'manifest.json').read_text(encoding="utf-8"))
        for case in terrain_manifest['cases']:
            assert sha((TERRAIN/(case['name']+'.map')).read_bytes())==case['map_sha256']
            assert sha((TERRAIN/(case['name']+'.adt.test-fixture')).read_bytes())==case['adt_sha256']
        raw_dir=work/'captured-raw';assembled=work/'captured-vmaps'
        raw_dir.mkdir(exist_ok=True);assembled.mkdir(exist_ok=True)
        (raw_dir/'dir_bin').write_bytes(captured_records)
        assert set(captured_models)=={'Hull.wmo'}
        for name,data in captured_models.items():(raw_dir/name).write_bytes(bytes.fromhex(data))
        subprocess.run([str(exe),'assemble',str(raw_dir),str(assembled)],check=True,stdout=subprocess.DEVNULL,timeout=15)
        actual={p.name:p.read_bytes().hex() for p in assembled.iterdir()}
        assert actual==vmaps,'original captured WMO reassembly differs'

    manifest={'schema':1,'source_pin':PIN,'map_id':0,'tile_x':30,'tile_y':31,'cases':{},'fixture_sources':{'terrain_manifest_sha256':sha((TERRAIN/'manifest.json').read_bytes()),'vmap_products_sha256':sha(VMAP.read_bytes())}}
    cases=[('ridge_wmo','ridge',False,True,True),('ridge_neighbors_wmo','ridge',True,True,True),('ridge_xminus_wmo','ridge',('xminus',),True,True),('ridge_only','ridge',False,False,True),('liquid_full_types_wmo','liquid_full_types',False,True,True)]
    for name in ['flat','varied','nonflat_holes','liquid_water_flat','liquid_full_types','liquid_flag_bytes','liquid_nonsymmetric','liquid_outer_edges','liquid_history61_initial']:
        cases.append((name,name,False,False,False))
    cases += [('varied_neighbors','varied',True,False,False),('nonflat_holes_neighbors','nonflat_holes',True,False,False)]
    for case,terrain,neighbors,collision,generate in cases:
        base=work/case
        if base.exists():shutil.rmtree(base)
        for folder in ['maps','vmaps','mmaps']: (base/folder).mkdir(parents=True,exist_ok=True)
        terrain_path=(ridge if terrain=='ridge' else TERRAIN)/(terrain+'.map')
        map_files={'0003130.map':terrain_path}
        if neighbors:
            for filename,suffix in [('0003131.map','xplus'),('0003129.map','xminus'),('0003230.map','yplus'),('0003030.map','yminus')]:
                if neighbors is not True and suffix not in neighbors:continue
                map_files[filename]=ridge/('ridge_'+suffix+'.map') if terrain=='ridge' else terrain_path
        for filename,path in map_files.items():shutil.copyfile(path,base/'maps'/filename)
        if collision:
            for filename,content in vmaps.items():(base/'vmaps'/filename).write_bytes(bytes.fromhex(content))
        command=[str(exe),str(base),'0','30','31',str(source/'contrib/extractor_scripts/config.json'),str(source/'contrib/extractor_scripts/offmesh.txt')]
        env=os.environ.copy()
        env.pop('BENILLA_ORACLE_QUERY_DETAILS',None)
        env.pop('BENILLA_ORACLE_COLLECT_ONLY',None)
        if case=='liquid_full_types_wmo':env['BENILLA_ORACLE_QUERY_DETAILS']='1'
        if not generate:env['BENILLA_ORACLE_COLLECT_ONLY']='1'
        subprocess.run([sys.executable,str(HERE/'run_oracle.py'),'--worker',str(base),*command],check=True,env=env,timeout=130)
        result=json.loads((base/'source-result.json').read_text(encoding="utf-8"));target=output/case;target.mkdir()
        entry={'terrain':terrain,'neighbors':neighbors,'collision':collision,'generated':generate,'maps':{n:sha(p.read_bytes()) for n,p in map_files.items()},'expected':result}
        if case=='liquid_full_types_wmo':
            entry['input_provenance']={
                'adt_fixture':str((TERRAIN/(terrain+'.adt.test-fixture')).relative_to(ROOT)),
                'adt_sha256':sha((TERRAIN/(terrain+'.adt.test-fixture')).read_bytes()),
                'map_fixture':str(terrain_path.relative_to(ROOT)),
                'map_sha256':sha(terrain_path.read_bytes()),
                'convert_adt_replay':'byte-identical to retained original map',
                'writer_provenance_sha256':sha((ridge/'writer-provenance.json').read_bytes()),
                'vmap_case':'captured_wmo',
                'vmap_products_sha256':{name:sha(bytes.fromhex(data)) for name,data in vmaps.items()},
                'source_hashes_sha256':sha((HERE/'source/source-hashes.json').read_bytes()),
                'original_tile':{'map_id':0,'x':30,'y':31},
                'production_tile_id':{'map_id':0,'x':31,'y':30},
            }
        elif case=='ridge_xminus_wmo':
            entry['input_provenance']={
                'scope':'Selected tile generation with one cardinal terrain strip; no neighboring MMTile or two-tile traversal',
                'terrain_inputs':[
                    {'adt_fixture':'authored/'+name+'.adt.test-fixture',
                     'adt_sha256':sha((ridge/(name+'.adt')).read_bytes()),
                     'map_fixture':'authored/'+name+'.map',
                     'map_sha256':sha((ridge/(name+'.map')).read_bytes()),
                     'original_map_filename':f'000{y:02}{x:02}.map',
                     'mpq_basename_template':f'{{directory}}_{x}_{y}.adt',
                     'original_tile':{'map_id':0,'x':x,'y':y},
                     'production_tile_id':{'map_id':0,'x':y,'y':x}}
                    for name,x,y in [('ridge',30,31),('ridge_xminus',29,31)]
                ],
                'absent_original_map_filenames':['0003131.map','0003230.map','0003030.map'],
                'convert_adt_replay':'Both authored ADTs generated through unchanged original ConvertADT',
                'writer_provenance_sha256':sha((ridge/'writer-provenance.json').read_bytes()),
                'vmap_case':'captured_wmo',
                'vmap_products_sha256':{name:sha(bytes.fromhex(data)) for name,data in vmaps.items()},
                'source_hashes_sha256':sha((HERE/'source/source-hashes.json').read_bytes()),
            }
        for name in ['source-terrain.bin','source-combined.bin','source-mesh.bin']:
            raw=(base/name).read_bytes();entry[name]=mesh_info(raw);(target/(name+'.zlib')).write_bytes(zlib.compress(raw,9))
        (target/'expected.json').write_bytes(json_bytes(result))
        if generate:
            entry['products']={}
            for path in sorted((base/'mmaps').iterdir()):
                data=path.read_bytes();(target/path.name).write_bytes(data);entry['products'][path.name]={'bytes':len(data),'sha256':sha(data)}
            raw=request(base,result);(target/'helper-request.bin.zlib').write_bytes(zlib.compress(raw,9));entry['helper_request']={'bytes':len(raw),'sha256':sha(raw)}
            check_generation(case,result)
        manifest['cases'][case]=entry
        print(case,json.dumps({k:result[k] for k in ['solid_vertices','solid_triangles','liquid_vertices','liquid_triangles']}),flush=True)
    authored=output/'authored';authored.mkdir()
    for case in OFFSETS:
        for extension in ['adt','map']:
            name=case+'.'+extension;shutil.copyfile(ridge/name,authored/(name+'.test-fixture' if extension=='adt' else name))
    shutil.copyfile(ridge/'writer-provenance.json',authored/'writer-provenance.json')
    (output/'manifest.json').write_bytes(json_bytes(manifest))
    resources={case:json.loads((work/case/'resource.json').read_text(encoding="utf-8")) for case,*_ in cases}
    (work/'resources.json').write_bytes(json_bytes(resources))
    from authored_models import capture as capture_models
    capture_models(source,work,output/'models')
    from authored_quantized import capture as capture_quantized
    capture_quantized(source,work,output/'quantized')
    config_output=output/'configs';config_output.mkdir()
    config_base=work/'ridge_wmo'
    subprocess.run([str(exe),'configs',str(config_base),str(source/'contrib/extractor_scripts/config.json')],check=True,stdout=subprocess.DEVNULL,timeout=15)
    shutil.copyfile(config_base/'config-result.json',config_output/'source-config-expected.json')
    authored={'1':{'walkableClimb':7.9,'minRegionArea':9,'123':{'walkableRadius':3.9,'maxSimplificationError':1.25},'ignored_object':{'x':1}}}
    (config_output/'authored-config.json').write_bytes(json_bytes(authored))
    subprocess.run([str(exe),'configs',str(config_base),str(config_output/'authored-config.json')],check=True,stdout=subprocess.DEVNULL,timeout=15)
    shutil.copyfile(config_base/'config-result.json',config_output/'authored-config-expected.json')
    from capture_slab_predicates import capture as capture_slabs
    capture_slabs(source,work/'slab-predicates',output/'slab-predicates')
    from capture_two_tile_seam import capture as capture_two_tile
    capture_two_tile(source,work,output/'two-tile-seam')
    from capture_scalar_nearest import capture as capture_scalar
    capture_scalar(source,work,output,output/'scalar-nearest')

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('source',type=Path,nargs='?');parser.add_argument('--work',type=Path,default=HERE/'.work/full');parser.add_argument('--check',action='store_true');parser.add_argument('--skip-input-regeneration',action='store_true');parser.add_argument('--worker',nargs=argparse.REMAINDER)
    args=parser.parse_args()
    if args.worker:return worker(args)
    if args.source is None:parser.error('source required')
    source=args.source.resolve();work=args.work.resolve();generated=work/'fixtures'
    if generated.exists():shutil.rmtree(generated)
    capture(source,work,generated,not args.skip_input_regeneration)
    expected=HERE/'fixtures'
    if args.check:
        old={str(p.relative_to(expected)):p.read_bytes() for p in expected.rglob('*') if p.is_file()}
        new={str(p.relative_to(generated)):p.read_bytes() for p in generated.rglob('*') if p.is_file()}
        if old!=new:raise SystemExit('Original source fixture comparison differs: '+str([n for n in set(old)|set(new) if old.get(n)!=new.get(n)]))
        print('Verified all deterministic source fixtures')
    else:
        if expected.exists():shutil.rmtree(expected)
        shutil.copytree(generated,expected);print('Generated source fixtures in',expected)
if __name__=='__main__':main()
