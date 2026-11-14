#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Hash-pinned GridDefines source arithmetic; no game data/terrain or registration."""
import argparse, hashlib, pathlib, re, subprocess, tempfile
from generate_player_derived_fixture import body

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('upstream',type=pathlib.Path)
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    raw=(args.upstream/'GridDefines.h').read_bytes()
    assert hashlib.sha256(raw).hexdigest()=='5f5413657f84c5f5964f2a69d342874d8d1f7f5ed2bab969e1d78b0650141d82'
    mapraw=(args.upstream/'Map.cpp').read_bytes()
    assert hashlib.sha256(mapraw).hexdigest()=='b9ada3277b8f60dd93f6ceba7e85c1dffa129385fbb4d9043fd93771a9704d12'
    assert b'if (p.x_coord >= TOTAL_NUMBER_OF_CELLS_PER_MAP || p.y_coord >= TOTAL_NUMBER_OF_CELLS_PER_MAP)' in mapraw
    source=raw.decode()
    names=['MAX_NUMBER_OF_GRIDS','SIZE_OF_GRIDS','MAX_NUMBER_OF_CELLS','SIZE_OF_GRID_CELL','CENTER_GRID_CELL_ID','CENTER_GRID_CELL_OFFSET','TOTAL_NUMBER_OF_CELLS_PER_MAP']
    macros='\n'.join(re.search(r'^#define '+n+r'\s+.*$',source,re.M).group() for n in names)
    # Retain original CoordPair (including unsigned fields and constructor).
    start=source.index('template<const unsigned int LIMIT>')
    end=source.index('typedef CoordPair<MAX_NUMBER_OF_GRIDS>')
    pair=source[start:end]
    compute=body(source,'inline RET_TYPE Compute(')
    cell=body(source,'inline CellPair ComputeCellPair(')
    extracted=macros+'\n'+pair+'\ntypedef CoordPair<TOTAL_NUMBER_OF_CELLS_PER_MAP> CellPair;\nnamespace MaNGOS {\ntemplate<class RET_TYPE, int CENTER_VAL>\n'+compute+'\n'+cell+'\n}\n'
    harness=pathlib.Path(__file__).with_name('resident_map_oracle.cpp').read_text().replace('// PINNED_SOURCE',extracted)
    with tempfile.TemporaryDirectory() as directory:
        tmp=pathlib.Path(directory);(tmp/'oracle.cpp').write_text(harness)
        subprocess.run(['g++','-std=c++17','-O0','-Wall','-Wextra','-Werror','-fno-fast-math','-ffp-contract=off','-frounding-math','-fsanitize=undefined,float-cast-overflow',str(tmp/'oracle.cpp'),'-o',str(tmp/'oracle')],check=True)
        data=subprocess.check_output([str(tmp/'oracle')])
    target=pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/resident_map_cmangos.tsv'
    if args.check: assert target.read_bytes()==data
    else: target.write_bytes(data)
    print('rows',len(data.splitlines())-1,'sha256',hashlib.sha256(data).hexdigest())
if __name__=='__main__':main()
