#!/usr/bin/env python3
"""Build a fixture from the pinned upstream file, never from the Rust port.
Usage: python3 tools/generate_xp_fixture.py /path/to/cmangos/src/game/Tools/Formulas.h
Requires g++; writes tests/fixtures/xp_cmangos.tsv relative to this crate.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile

source = pathlib.Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(source).hexdigest() == "ca8b55fd022b0c0191255bc457574ccafce5eeb2f99a447466ba75be4717652c", "Wrong upstream revision"
text = source.decode()
start = text.index("        inline bool IsTrivialLevelDifference")
end = text.index("        inline uint32 Gain(", start)
# Compile the original helper bodies verbatim, excluding the world-bound Gain.
harness = r'''
#include <cstdint>
#include <cmath>
#include <cfenv>
#include <cstring>
#include <iostream>
using uint32 = uint32_t;
'''+text[start:end]+r'''
int main() {
    std::fesetround(FE_TONEAREST);
    std::cout << "# unit mob trivial green_range gray zero color base_f32_bits rounded_f32\n";
    for (uint32 unit=1;unit<=63;++unit) for(uint32 mob=1;mob<=63;++mob) {
        float gain=BaseGain(unit,mob); uint32 bits; std::memcpy(&bits,&gain,4);
        std::cout << unit << '\t' << mob << '\t' << IsTrivialLevelDifference(unit,mob)
            << '\t' << GetQuestGreenRange(unit) << '\t' << GetGrayLevel(unit)
            << '\t' << GetZeroDifference(unit) << '\t' << GetColorCode(unit,mob)
            << '\t' << bits << '\t' << uint32(std::nearbyint(gain)) << '\n';
    }
}
'''
with tempfile.TemporaryDirectory() as temporary:
    root = pathlib.Path(temporary)
    (root / 'oracle.cpp').write_text(harness)
    subprocess.run(['g++', '-std=c++17', '-O0', '-fno-fast-math', str(root / 'oracle.cpp'), '-o', str(root / 'oracle')], check=True)
    result = subprocess.check_output([str(root / 'oracle')])
pathlib.Path(__file__).resolve().parents[1].joinpath('tests/fixtures/xp_cmangos.tsv').write_bytes(result)
