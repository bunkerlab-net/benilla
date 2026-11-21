#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Original readMapSpawns container control flow on authored numeric records.

ModelSpawn parsing is deliberately a two-word stub. This establishes duplicate
and container ordering behavior, not the original binary parser or geometry.
"""
import hashlib
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest

SOURCE_SHA = 'd2d3f583fef504bf3b8b675d533464125d0da13596cd9a4022e34e1fce8acf95'

@unittest.skipUnless(os.environ.get('CMANGOS_SOURCE'), 'set CMANGOS_SOURCE for original-source comparison')
class OriginalMapSpawnIndex(unittest.TestCase):
    def test_original_first_wins_and_tile_multimap(self):
        raw = (Path(os.environ['CMANGOS_SOURCE'])/'src/game/vmap/TileAssembler.cpp').read_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(), SOURCE_SHA)
        source = raw.decode()
        start = source.index('    bool TileAssembler::readMapSpawns()')
        end = source.index('    bool TileAssembler::calculateTransformedBound', start)
        function = source[start:end]
        harness = r'''
#include <map>
#include <string>
#include <cstdint>
#include <cstdio>
using uint32=uint32_t;
using std::pair;
struct ModelSpawn {
 uint32 ID=0, payload=0;
 static bool readFromFile(FILE* f, ModelSpawn& s) {
  return fread(&s.ID,4,1,f)==1 && fread(&s.payload,4,1,f)==1;
 }
};
struct StaticMapTree { static uint32 packTileID(uint32 x,uint32 y){return x<<16|y;} };
struct MapSpawns {
 std::map<uint32,ModelSpawn> UniqueEntries;
 std::multimap<uint32,uint32> TileEntries;
};
using MapData=std::map<uint32,MapSpawns*>;
struct TileAssembler { std::string iSrcDir; MapData mapData; bool readMapSpawns(); };
''' + function + r'''
int main(int argc,char**argv){
 if(argc!=2)return 1;
 TileAssembler a; a.iSrcDir=argv[1];
 if(!a.readMapSpawns())return 2;
 for(auto& m:a.mapData){
  for(auto& u:m.second->UniqueEntries)printf("U %u %u %u\n",m.first,u.first,u.second.payload);
  for(auto& t:m.second->TileEntries)printf("T %u %u %u\n",m.first,t.first,t.second);
  delete m.second;
 }
}
'''
        rows = [(7,1,0,9,11),(7,0,0,3,33),(7,0,0,9,22),
                (7,0,0,3,44),(7,65,65,1,55),(7,1,0,2,66)]
        with tempfile.TemporaryDirectory(prefix='benilla-map-spawn-index-') as directory:
            root=Path(directory)
            (root/'dir_bin').write_bytes(b''.join(struct.pack('<5I',*row) for row in rows))
            (root/'probe.cpp').write_text(harness)
            subprocess.run(['g++','-std=c++17','-Wall','-Wextra','-Werror',str(root/'probe.cpp'),'-o',str(root/'probe')],check=True,timeout=30)
            result=subprocess.run([str(root/'probe'),str(root)],check=True,capture_output=True,text=True,timeout=10)
        records=[line for line in result.stdout.splitlines() if line.startswith(('U ','T '))]
        self.assertEqual(records, ['U 7 1 55','U 7 2 66','U 7 3 33','U 7 9 11',
            'T 7 0 3','T 7 0 9','T 7 0 3','T 7 65536 9','T 7 65536 2','T 7 4259905 1'])

if __name__ == '__main__':
    unittest.main()
