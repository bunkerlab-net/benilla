// SPDX-License-Identifier: GPL-3.0-or-later
// Pinned CMaNGOS GPL-2.0-or-later methods inserted by hash-checking generator.
// Only arithmetic plus Map::Add bounds: no resident registration or terrain load.
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <limits>
#include <set>
using uint32 = std::uint32_t;
// PINNED_SOURCE
static float value(uint32 bits) { float v; std::memcpy(&v,&bits,4); return v; }
static uint32 bits(float v) { uint32 b; std::memcpy(&b,&v,4); return b; }
static int axis(float x, uint32& out) {
    if (!std::isfinite(x)) return 2;
    // Guard only the otherwise undefined C++ integer conversion, then invoke
    // the unmodified extracted source function for the actual coordinate.
    double index=(double(x)-CENTER_GRID_CELL_OFFSET)/SIZE_OF_GRID_CELL+CENTER_GRID_CELL_ID+0.5;
    double truncated=std::trunc(index);
    if (!std::isfinite(index)||truncated<std::numeric_limits<int>::min()||truncated>std::numeric_limits<int>::max()) return 2;
    const CellPair p=MaNGOS::ComputeCellPair(x,0.f);
    out=p.x_coord;
    if(out>=TOTAL_NUMBER_OF_CELLS_PER_MAP) return 1;
    return 0;
}
int main() {
    std::set<uint32> inputs={0,0x80000000u,1,0x80000001u,0x007fffffu,0x807fffffu,0x7fc00000u,0x7f800000u,0xff800000u,0x7f7fffffu,0xff7fffffu};
    for(int cell=-2;cell<=1026;++cell) {
        float boundary=float((double(cell)-CENTER_GRID_CELL_ID)*SIZE_OF_GRID_CELL);
        inputs.insert(bits(boundary));
        inputs.insert(bits(std::nextafter(boundary,-std::numeric_limits<float>::infinity())));
        inputs.insert(bits(std::nextafter(boundary,std::numeric_limits<float>::infinity())));
    }
    for(float v: {-17080.f,-17100.f,-17066.666f,-17066.f,17066.f,17066.666f,17100.f,-1000000.f,1000000.f}) inputs.insert(bits(v));
    std::cout<<"# x_bits y_bits status x y\n";
    for(uint32 x:inputs) for(uint32 y:{0u,x^0x80000000u}) {
        uint32 cx=0,cy=0; int status=axis(value(x),cx);
        if(!status) status=axis(value(y),cy);
        if(status) cx=cy=0;
        std::cout<<x<<'\t'<<y<<'\t'<<status<<'\t'<<cx<<'\t'<<cy<<'\n';
    }
}
