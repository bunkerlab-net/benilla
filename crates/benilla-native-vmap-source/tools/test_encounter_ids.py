#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the pinned original GenerateUniqueObjectId on authored numeric keys only."""
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SOURCE_SHA = 'b22db3c3459c524d56bb16bfe9b73c7c90b0553ed30c1bed7a53e710bd1df811'
BODY = '''uint32 GenerateUniqueObjectId(uint32 clientId, uint16 clientDoodadId)
{
    return uniqueObjectIds.emplace(std::make_pair(clientId, clientDoodadId), uniqueObjectIds.size() + 1).first->second;
}'''

@unittest.skipUnless(os.environ.get('CMANGOS_SOURCE'), 'set CMANGOS_SOURCE for pinned original-source comparison')
class OriginalEncounterIds(unittest.TestCase):
    def test_original_function_duplicates_and_unsigned_wrap(self):
        path = Path(os.environ['CMANGOS_SOURCE']) / 'contrib/vmap_extractor/vmapextract/vmapexport.cpp'
        raw = path.read_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(), SOURCE_SHA)
        source = raw.decode()
        self.assertIn(BODY, source)
        # Paste the exact source span, not a separately implemented oracle.
        start = source.index(BODY)
        function = source[start:start + len(BODY)]
        harness = r'''
#include <map>
#include <cstdint>
#include <cstdio>
using uint32 = uint32_t;
using uint16 = uint16_t;
std::map<std::pair<uint32, uint16>, uint32> uniqueObjectIds;
''' + function + r'''
int main() {
    const std::pair<uint32,uint16> keys[] = {{42,0},{42,1},{7,0},{42,0},{42,2},{7,1},{42,1},{UINT32_MAX,UINT16_MAX}};
    for (auto k:keys) printf("%u ",GenerateUniqueObjectId(k.first,k.second));
    printf("\n%zu\n",uniqueObjectIds.size());
    uniqueObjectIds.clear();
    if (GenerateUniqueObjectId(42,0)!=1) return 1;
    uint16 ordinal=0;
    for (uint32 expected=2;expected<=65536;++expected) {
        ++ordinal;
        if (GenerateUniqueObjectId(42,ordinal)!=expected) return 2;
    }
    ++ordinal;
    printf("%u %u %zu\n",ordinal,GenerateUniqueObjectId(42,ordinal),uniqueObjectIds.size());
}
'''
        with tempfile.TemporaryDirectory(prefix='benilla-encounter-ids-') as directory:
            root = Path(directory)
            (root/'probe.cpp').write_text(harness)
            subprocess.run(['g++','-std=c++17','-Wall','-Wextra','-Werror',str(root/'probe.cpp'),'-o',str(root/'probe')],check=True,timeout=30)
            result = subprocess.run([str(root/'probe')],check=True,capture_output=True,text=True,timeout=10)
        self.assertEqual(result.stdout,'1 2 3 1 4 5 2 6 \n6\n0 1 65536\n')

if __name__ == '__main__':
    unittest.main()
