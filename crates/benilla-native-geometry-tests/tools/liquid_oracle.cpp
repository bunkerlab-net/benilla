// SPDX-License-Identifier: GPL-3.0-or-later
// Authored harness for the complete pinned GridMap::getLiquidStatus dry branch.
// Non-dry dependencies abort: no fabricated DBC/height answers are accepted.
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <initializer_list>
using uint8 = uint8_t;
using uint16 = uint16_t;
using uint32 = uint32_t;
// PINNED_GRID_CONSTANTS
// PINNED_GRIDMAP_DEFINES
struct LiquidTypeEntry { uint32 Id, Type; };
struct AreaTableEntry { uint32 LiquidTypeOverride, zone; };
template<class T> struct UnavailableStore { const T* LookupEntry(uint32) const { std::abort(); } };
UnavailableStore<LiquidTypeEntry> sLiquidTypeStore;
UnavailableStore<AreaTableEntry> sAreaStore;
const AreaTableEntry* GetAreaEntryByAreaID(uint32) { std::abort(); }
struct GridMap {
    uint16 m_liquidGlobalEntry=0;
    uint8 m_liquidGlobalFlags=0, m_liquid_offX=0, m_liquid_offY=0, m_liquid_width=0, m_liquid_height=0;
    float m_liquidLevel=0;
    uint16* m_liquidEntry=nullptr;
    uint8* m_liquidFlags=nullptr;
    float* m_liquid_map=nullptr;
    uint16 getArea(float,float) { std::abort(); }
    float getHeight(float,float) { std::abort(); }
    GridMapLiquidStatus getLiquidStatus(float,float,float,uint8,GridMapLiquidData*,float);
};
// PINNED_UPSTREAM_METHOD
static uint32 bits(float f) { uint32 b; std::memcpy(&b,&f,4); return b; }
static void sample(float x,float y,float z,float collisionHeight) {
    // PINNED_GRID_SELECTION
    GridMap grid;
    GridMapLiquidData data{};
    auto status=grid.getLiquidStatus(x,y,z,MAP_ALL_LIQUIDS,&data,collisionHeight);
    std::printf("%08x\t%08x\t%08x\t%08x\t%d\t%d\t%d\n",bits(x),bits(y),bits(z),bits(collisionHeight),int(status),gx,gy);
}
int main() {
    std::fesetround(FE_TONEAREST);
    std::puts("# x_bits y_bits z_bits collision_height_bits source_status grid_x grid_y");
    for(int tile=0;tile<64;tile++) {
        float center=(32.f-(float(tile)+.5f))*SIZE_OF_GRIDS;
        for(float z : {-1000.f,-0.f,0.f,7.f,1000.f})
            for(float height : {0.f,2.03128f,16.f}) sample(center,800.f,z,height);
    }
    for(int tile=1;tile<64;tile++) {
        float edge=(32.f-float(tile))*SIZE_OF_GRIDS;
        for(float x : {std::nextafter(edge,-INFINITY),edge,std::nextafter(edge,INFINITY)}) {
            sample(x,800.f,7.f,2.03128f);
            sample(0.f,x,7.f,2.03128f);
        }
    }
}
