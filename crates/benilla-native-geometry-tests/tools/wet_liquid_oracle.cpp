// SPDX-License-Identifier: GPL-3.0-or-later
// Authored inputs/storage for exact pinned source bodies, never a liquid algorithm.
// Source commit: cmangos/mangos-classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Every LiquidType lookup searches the explicit authored rows below. A missing ID
// really is absent; matching IDs are >=21. All area lookup paths abort because
// area/zone override selection is outside this fixture's admitted domain.
#include <array>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <initializer_list>
#include <limits>
using uint8 = uint8_t;
using uint16 = uint16_t;
using uint32 = uint32_t;
using int32 = int32_t;
// PINNED_GRID_CONSTANTS
// PINNED_GRIDMAP_DEFINES
// PINNED_DBC_STRUCTS
static const std::array<LiquidTypeEntry, 4> LIQUID_ROWS = {{{21, 23, 3, 0},
    {22, 29, 1, 0}, {23, 35, 0, 0}, {24, 41, 2, 0}}};
struct LiquidStore {
    const LiquidTypeEntry* LookupEntry(uint32 id) const {
        for (const auto& row : LIQUID_ROWS) if (row.Id == id) return &row;
        return nullptr;
    }
} sLiquidTypeStore;
struct UnavailableAreaStore {
    const AreaTableEntry* LookupEntry(uint32) const { std::abort(); }
} sAreaStore;
const AreaTableEntry* GetAreaEntryByAreaID(uint32) { std::abort(); }
struct GridMap {
    // PINNED_GRIDMAP_FIELDS
    uint16 getArea(float, float) const { std::abort(); }
    GridMapLiquidStatus getLiquidStatus(float, float, float, uint8, GridMapLiquidData*, float);
};
// PINNED_UPSTREAM_METHODS
static uint32 bits(float value) { uint32 out; std::memcpy(&out, &value, 4); return out; }
static float fromBits(uint32 value) { float out; std::memcpy(&out, &value, 4); return out; }

// Entries/flags are row-major 16*16 cell tables, with independent cycles indexed
// by i=row*16+column. Liquid height maps are row-major height*width arrays.
static constexpr uint16 ENTRY_CYCLE[] = {21, 22, 23, 24, 77, 0, 20};
static constexpr uint8 FLAG_CYCLE[] = {0, 1, 2, 4, 8, 16, 24, 32, 63};
struct Profile {
    uint16 entry = 77;
    uint8 flags = MAP_LIQUID_TYPE_WATER;
    uint8 offX = 11, offY = 19, width = 7, height = 5;
    float level = 10.f;
    bool typeTable = false, heightMap = false, zeroFlags = false;
    std::array<uint16, 256> entries{};
    std::array<uint8, 256> types{};
    std::array<float, 128 * 128> levels{};
};
static Profile profile(int id) {
    Profile p;
    switch (id) {
        case 0: break;                                     // Global, missing row, flat.
        case 1: p.heightMap = true; break;                 // Global, missing row, map.
        case 2:                                           // Both authored cell tables, flat.
        case 6:                                           // Present but zero flags table.
            p.offX = p.offY = 0; p.width = p.height = 128;
            p.entry = 78; p.flags = id == 2 ? 32 : 0;
            p.typeTable = true; p.zeroFlags = id == 6;
            break;
        case 3:                                           // Tables plus nonsquare map spanning cells.
            p.typeTable = p.heightMap = true;
            p.offX = 5; p.offY = 7; p.width = 13; p.height = 11;
            break;
        case 4: p.entry = 21; p.flags = 63; break;         // Resolve row, retain only deep.
        case 5: p.entry = 20; p.flags = 63; break;         // Absent <21 row, retain all flags.
        case 7:                                           // Early dry, even with matched ID.
        case 8:                                           // Deep-only, absent row.
        case 9:                                           // Zero level, exact delta boundaries.
            p.offX = p.offY = 0; p.width = p.height = 128;
            p.entry = id == 7 ? 21 : 77;
            p.flags = id == 7 ? 0 : (id == 8 ? MAP_LIQUID_TYPE_DEEP_WATER : MAP_LIQUID_TYPE_WATER);
            if (id == 9) p.level = 0.f;
            break;
        default: std::abort();
    }
    for (size_t i = 0; i < p.entries.size(); ++i) {
        p.entries[i] = ENTRY_CYCLE[i % 7];
        p.types[i] = p.zeroFlags ? 0 : FLAG_CYCLE[i % 9];
    }
    for (int x = 0; x < p.height; ++x) for (int y = 0; y < p.width; ++y)
        p.levels[x * p.width + y] = 10.f + float(x) * .5f + float(y) * .25f;
    return p;
}
static GridMap grid(Profile& p, float ground) {
    GridMap g{};
    g.m_gridHeight = ground;
    g.m_gridGetHeight = &GridMap::getHeightFromFlat;
    g.m_liquidGlobalEntry = p.entry;
    g.m_liquidGlobalFlags = p.flags;
    g.m_liquid_offX = p.offX; g.m_liquid_offY = p.offY;
    g.m_liquid_width = p.width; g.m_liquid_height = p.height;
    g.m_liquidLevel = p.level;
    g.m_liquidEntry = p.typeTable ? p.entries.data() : nullptr;
    g.m_liquidFlags = p.typeTable ? p.types.data() : nullptr;
    g.m_liquid_map = p.heightMap ? p.levels.data() : nullptr;
    return g;
}
// All queries use tile (32,32); world coordinates are printed as exact bits.
static float world(float local) { return (32.f - (32.f + local / 128.f)) * SIZE_OF_GRIDS; }
static void sample(int id, Profile& p, float x, float y, float z, float collisionHeight,
                   uint8 requested, float ground, bool wantsData = true) {
    GridMap g = grid(p, ground);
    GridMapLiquidData data{0xdecafbad, 0xdeadbeef, fromBits(0x4a123456), fromBits(0xca654321)};
    auto status = g.getLiquidStatus(x, y, z, requested, wantsData ? &data : nullptr, collisionHeight);
    std::printf("%d\t%08x\t%08x\t%08x\t%08x\t%u\t%08x\t%d\t%u\t%u\t%u\t%08x\t%08x\n",
        id, bits(x), bits(y), bits(z), bits(collisionHeight), unsigned(requested), bits(ground),
        int(wantsData), unsigned(status), data.entry, data.type_flags, bits(data.level), bits(data.depth_level));
}
static std::array<float, 3> adjacent(float value) {
    return {std::nextafter(value, -INFINITY), value, std::nextafter(value, INFINITY)};
}
int main() {
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559, "binary32 required");
    if (std::fesetround(FE_TONEAREST)) return 1;
    std::puts("# Exact complete GridMap::getLiquidStatus; pinned cmangos 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f");
    std::puts("# Authored numeric inputs only; no client assets. Ground uses source flat-height dispatch/body.");
    std::puts("# Scope excludes area/zone overrides, TerrainInfo, VMAP liquid selection and gameplay admission.");
    std::puts("# DBC rows (Id,LiquidId,Type,SpellId): 21,23,3,0;22,29,1,0;23,35,0,0;24,41,2,0");
    std::puts("# Missing lookup IDs 0,20,77,78 are genuinely absent from that table; every area lookup aborts.");
    std::puts("# Cell entries[i] = [21,22,23,24,77,0,20][i%7]; flags[i] = [0,1,2,4,8,16,24,32,63][i%9]");
    std::puts("# Cell tables are row-major 16*16; profile 6 replaces flags[i] with zero.");
    std::puts("# Height-map levels[row*width+col] = binary32(10 + binary32(row*0.5) + binary32(col*0.25)).");
    std::puts("# Profile columns: id global_entry global_flags offX offY width height flat_level_bits type_table height_map zero_flags");
    for (int id = 0; id < 10; ++id) {
        auto p = profile(id);
        std::printf("# profile\t%d\t%u\t%u\t%u\t%u\t%u\t%u\t%08x\t%d\t%d\t%d\n",
            id, unsigned(p.entry), unsigned(p.flags), unsigned(p.offX), unsigned(p.offY),
            unsigned(p.width), unsigned(p.height), bits(p.level), int(p.typeTable), int(p.heightMap), int(p.zeroFlags));
    }
    std::puts("# All queries select tile 32,32. All *_bits are hexadecimal binary32; other columns are decimal.");
    std::puts("# Output data starts entry=3735928559,type_flags=3737844653,level_bits=4a123456,depth_bits=ca654321.");
    std::puts("# NO_WATER preserves every data sentinel; wants_data=0 also preserves them for nonzero status.");
    std::puts("# Ground -200000 is the source INVALID_HEIGHT_VALUE supplied as a scalar, not a hole-interpolation proof.");
    std::puts("# profile\tx_bits\ty_bits\tz_bits\tcollision_height_bits\trequired_mask\tground_bits\twants_data\tstatus\tentry\ttype_flags\tlevel_bits\tdepth_bits");

    for (int id = 0; id < 10; ++id) {
        auto p = profile(id);
        // Every cell is sampled; all 63 combinations of the independent cycles
        // receive every basic/deep/WMO mask. Other profiles need no repeated dry grid.
        if (id == 2 || id == 6) {
            for (int x = 0; x < 16; ++x) for (int y = 0; y < 16; ++y)
                for (uint8 mask : {0, 1, 2, 4, 8, 15, 16, 32, 63})
                    if (mask == 0 || x * 16 + y < 63)
                        sample(id, p, world(float(x * 8 + 4)), world(float(y * 8 + 4)), 9.f, 2.f, mask, -100.f);
        } else {
            for (uint8 mask : {0, 1, 2, 4, 8, 15, 16, 32, 63})
                sample(id, p, world(p.offY + .25f), world(p.offX + .25f), 9.f, 2.f, mask, -100.f);
        }
        // Read every element of each authored height map.
        if (p.heightMap)
            for (int x = 0; x < p.height; ++x) for (int y = 0; y < p.width; ++y)
                sample(id, p, world(p.offY + float(x) + .25f), world(p.offX + float(y) + .25f), 9.f, 2.f, 0, -100.f);
        // Interior plus each side of the nonsquare restricted rectangle.
        if (p.width < 128)
            for (float x : {p.offY - .75f, p.offY + .25f, p.offY + p.height * .5f + .25f,
                            p.offY + p.height - .75f, p.offY + p.height + .25f})
                for (float y : {p.offX - .75f, p.offX + .25f, p.offX + p.width * .5f + .25f,
                                p.offX + p.width - .75f, p.offX + p.width + .25f})
                    for (float z : {7.f, 9.f, 10.f, 10.5f, 11.f, 14.f})
                        sample(id, p, world(x), world(y), z, 2.f, 0, -100.f);
        // Adjacent world floats at rectangle and cell boundaries, without assuming
        // one world ULP survives source coordinate conversion into a different cell.
        for (float edge : {8.f, 11.f, 18.f, 19.f, 24.f})
            for (float coordinate : adjacent(world(edge))) {
                sample(id, p, coordinate, world(14.25f), 9.f, 2.f, 0, -100.f);
                sample(id, p, world(21.25f), coordinate, 9.f, 2.f, 0, -100.f);
            }
        // liquid < ground and z < ground-2: equality and its two immediate neighbors.
        for (float ground : adjacent(10.f))
            sample(id, p, world(21.25f), world(14.25f), 10.f, 2.f, 0, ground);
        for (float z : adjacent(5.f))
            sample(id, p, world(21.25f), world(14.25f), z, 2.f, 0, 7.f);
        for (float z : {9.f, 10.f, 11.f})
            sample(id, p, world(21.25f), world(14.25f), z, 2.f, 0, -100.f, false);
    }
    // Level zero avoids subtraction cancellation at delta=collisionHeight,0,-1.
    auto p = profile(9);
    for (float collision : {0.f, 2.f, 2.03128f, 16.f})
        for (float center : {-collision, 0.f, 1.f})
            for (float z : adjacent(center))
                sample(9, p, world(21.25f), world(14.25f), z, collision, 15, -100.f);
    for (float ground : adjacent(0.f))
        sample(9, p, world(21.25f), world(14.25f), 0.f, 2.f, 0, ground);
    for (float z : adjacent(-102.f))
        sample(9, p, world(21.25f), world(14.25f), z, 2.f, 0, -100.f);
    sample(9, p, world(21.25f), world(14.25f), 0.f, 2.f, 0, INVALID_HEIGHT_VALUE);
}
