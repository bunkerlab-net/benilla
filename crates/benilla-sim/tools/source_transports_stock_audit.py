#!/usr/bin/env python3
"""Audit source undefined-path guards against checked retained raw transport inputs."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'scripts'))
from classic_export.catalog_io import Catalog
from classic_export.catalog_dbc import DbcViews


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('catalog',type=Path);p.add_argument('evidence',type=Path);args=p.parse_args()
    with Catalog(args.catalog) as catalog:
        views=DbcViews(catalog)
        def words(name):
            view=views.open(name)
            return [view.words(ordinal) for _,ordinal in sorted(view.by_id.items())]
        maps={w[0]:w[2] for w in words('Map.dbc')}
        path_count=max((w[0] for w in words('TaxiPath.dbc')),default=0)+1
        slots={}
        for w in words('TaxiPathNode.dbc'):
            if w[0]:slots.setdefault(w[1],{})[w[2]]=w
        transports=[]
        for _,row in views.rows('gameobject_template'):
            if int(row['type'])==15:
                transports.append({k:int(row[k]) for k in ['entry','data0','data1','data2']})
        reports=[];inputs=[]
        for raw in transports:
            entry,path,speed,accel=(raw[k] for k in ['entry','data0','data1','data2'])
            violations=[];nodes=[slots.get(path,{}).get(i) for i in range(max(slots.get(path,{0:None}))+1)]
            if path>=path_count:violations.append('rejected_out_of_range_path')
            if any(n is None for n in nodes):violations.append('null_taxi_node')
            if len(nodes)<3:violations.append('orientation_controls_or_empty_frames')
            frames=[];skip=False
            if not violations:
                for i in range(1,len(nodes)-1):
                    if skip:skip=False;continue
                    if nodes[i][7]&1 or nodes[i][3]!=nodes[i+1][3]:
                        if not frames:violations.append('teleport_without_previous_frame')
                        else:frames[-1][1]=True
                        skip=True
                    else:frames.append([i,False])
                if not frames:violations.append('empty_key_frames')
                if path in [293,303] and len(frames)<=12:violations.append('refresh_frame_12')
                used={nodes[i][3] for i,_ in frames}
                if any(m not in maps for m in used):violations.append('map_lookup_null')
                if len(used)>1 and any(maps.get(m) in [1,2,3] for m in used):violations.append('cross_map_instance_assertion')
            else:used=set()
            if speed==0 or accel==0:violations.append('zero_speed_or_acceleration')
            reports.append({'entry':entry,'path':path,'raw_nodes':len(nodes),'frames':len(frames),'maps':sorted(used),'violations':violations})
            if not violations:
                inputs.append({'name':f'raw_stock_{entry}','entry':entry,'path':path,'speed':speed,'accel':accel,'period':None,
                    'nodes':[{'id':n[0],'path':n[1],'index':n[2],'map':n[3],'position':n[4:7],'flags':n[7],'delay':n[8]} for n in nodes]})
        report={'catalog_sha256':catalog.manifest['catalog_sha256'],'templates':len(reports),'rows':reports,
            'guard_reaches':sum(bool(r['violations']) for r in reports),
            'scope':'Checked raw stock data, not effective ObjectMgr or sealed F1 acceptance. Float conversion legality is compared with the pinned C++ oracle and Rust generation.'}
    root=args.evidence.resolve();root.mkdir(parents=True,exist_ok=True)
    (root/'stock-transport-guard-audit.json').write_text(json.dumps(report,indent=2)+'\n')
    (root/'stock-transport-inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
    print(json.dumps(report))

if __name__=='__main__':main()
