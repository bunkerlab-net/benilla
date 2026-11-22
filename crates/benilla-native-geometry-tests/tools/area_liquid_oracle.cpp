// SPDX-License-Identifier: GPL-3.0-or-later
// Authored inputs and complete stores for unchanged pinned source methods.
// Source: cmangos/mangos-classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// No liquid/area selection algorithm is implemented by this harness.
#include <array>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <initializer_list>
#include <limits>
#include <vector>
using uint8 = uint8_t;
using uint16 = uint16_t;
using uint32 = uint32_t;
using int32 = int32_t;
// PINNED_GRID_CONSTANTS
// PINNED_GRIDMAP_DEFINES
// PINNED_DBC_FORMATS
// PINNED_DBC_STRUCTS

using LiquidWords = std::array<uint32, 4>;
using AreaWords = std::array<uint32, 25>;
static constexpr LiquidWords LIQUID_WORDS[] = {
    {0, 701, 3, 31}, {1, 703, 0, 37}, {2, 709, 1, 41}, {3, 719, 2, 43},
    {20, 727, 3, 47}, {21, 733, 1, 53}, {22, 739, 2, 59},
    {23, 743, 3, 61}, {24, 751, 0, 67}, {65535, 757, 1, 71},
};
// ID, mapid, zone, exploreFlag, LiquidTypeOverride. Other raw WDBC fields
// are deliberately nonzero and emitted below; the real source structs are used.
static constexpr std::array<uint32, 5> AREA_KEYS[] = {
    {0, 999, 0, 42, 24}, {100, 42, 200, 9000, 22}, {101, 1, 200, 100, 0},
    {102, 0, 0, 101, 0}, {103, 7, 999, 102, 0}, {104, 42, 200, 103, 77},
    {105, 1, 201, 104, 0}, {106, 7, 202, 105, 0}, {107, 0, 108, 106, 0},
    {108, 1, 107, 107, 0}, {109, 42, 109, 108, 0}, {110, 7, 0, 109, 20},
    {112, 1, 0, 110, 65535}, {114, 42, 70000, 112, 0}, {115, 7, 200, 65535, 70000},
    {200, 7, 0, 9200, 23}, {201, 42, 0, 9201, 0}, {202, 1, 200, 9202, 0},
    {500, 1, 0, 300, 24}, {9000, 42, 0, 100, 24}, {70000, 7, 0, 9700, 24},
};
static char NAMES[] = "\0a\0b\0c\0d\0e\0f\0g\0h";
static AreaWords areaWords(const std::array<uint32, 5>& keys) {
    AreaWords words{};
    for (size_t i = 0; i < words.size(); ++i) words[i] = 1000 + uint32(i) * 7;
    for (size_t i = 0; i < 4; ++i) words[i] = keys[i];
    words[10] = uint32(-17);
    for (size_t i = 11; i <= 18; ++i) words[i] = 1 + uint32(i - 11) * 2;
    words[20] = 3;
    words[24] = keys[4];
    return words;
}
struct LiquidStore {
    std::vector<LiquidTypeEntry> rows;
    const LiquidTypeEntry* LookupEntry(uint32 id) const {
        for (const auto& row : rows) if (row.Id == id) return &row;
        return nullptr;
    }
} sLiquidTypeStore;
struct AreaStore {
    std::vector<AreaTableEntry> rows;
    const AreaTableEntry* LookupEntry(uint32 id) const {
        for (const auto& row : rows) if (row.ID == id) return &row;
        return nullptr;
    }
} sAreaStore;
// PINNED_AREA_ID_LOOKUP
static void stores(int variant) {
    sLiquidTypeStore.rows.clear();
    sAreaStore.rows.clear();
    for (const auto& w : LIQUID_WORDS) {
        if (w[0] == 0 && !(variant & 1)) continue;
        sLiquidTypeStore.rows.push_back({w[0], w[1], w[2], w[3]});
    }
    for (const auto& keys : AREA_KEYS) {
        if (keys[0] == 0 && !(variant & 2)) continue;
        const auto w = areaWords(keys);
        AreaTableEntry row{};
        row.ID = w[0]; row.mapid = w[1]; row.zone = w[2]; row.exploreFlag = w[3];
        row.flags = w[4]; row.area_level = int32(w[10]); row.team = w[20];
        row.LiquidTypeOverride = w[24];
        for (size_t i = 0; i < 8; ++i) row.area_name[i] = NAMES + w[11 + i];
        sAreaStore.rows.push_back(row);
    }
}
struct GridMap {
    // PINNED_GRIDMAP_FIELDS
    GridMapLiquidStatus getLiquidStatus(float, float, float, uint8, GridMapLiquidData*, float);
};
// PINNED_UPSTREAM_METHODS
static uint32 bits(float value) { uint32 out; std::memcpy(&out, &value, 4); return out; }
static float fromBits(uint32 value) { float out; std::memcpy(&out, &value, 4); return out; }

static constexpr uint16 AREA_CYCLE[] = {
    100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 112,
    114, 115, 0, 300, 65535, 200, 201, 202, 500, 9000, 107, 109,
};
static constexpr uint16 ENTRY_CYCLE[] = {0, 1, 2, 3, 20, 21, 22, 77};
static constexpr uint8 FLAG_CYCLE[] = {0, 1, 16, 32, 63};
static constexpr uint16 GLOBAL_AREAS[] = {
    100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 112, 114, 115, 0, 300, 65535,
};
struct Profile {
    int store = 0;
    uint16 area = 100, entry = 1;
    uint8 flags = 63;
    bool areaTable = false, typeTable = false;
    std::array<uint16, 256> areas{}, entries{};
    std::array<uint8, 256> flagsTable{};
};
static Profile profile(int id) {
    Profile p;
    if (id < 17) p.area = GLOBAL_AREAS[id];
    else switch (id) {
        case 17: p.store = 1; p.area = 102; break; // No zone; zero LiquidType row exists.
        case 18: p.store = 1; p.area = 103; break; // Missing parent still looks up zero.
        case 19: p.store = 1; p.area = 105; break; // Parent's zero override is looked up.
        case 20: p.store = 1; p.area = 106; break; // Grandparent is not consulted.
        case 21: p.store = 1; p.area = 107; break; // Cycles never recurse.
        case 22: p.store = 2; p.area = 0; break;   // Raw AREA zero is a real present ID.
        case 23: p.store = 3; p.area = 0; break;
        case 24: p.entry = 21; break;            // Entry 21 does not consult AREA.
        case 25: p.entry = 0; break;             // Missing initial row preserves raw flags.
        case 26: p.store = 1; p.entry = 0; break;// Present initial zero does consult AREA.
        case 27: p.entry = 20; p.flags = 0; break;// Global zero flags exits before any lookup.
        case 28: p.entry = 2; p.flags = 16; break;// Deep survives, base kind is overridden.
        case 29: p.entry = 3; p.flags = 32; break;// WMO flag is discarded after row resolution.
        case 30: p.store = 1; p.area = 104; break;// Missing nonzero override never uses zero.
        case 31: p.areaTable = true; break;
        case 32: p.store = 3; p.areaTable = p.typeTable = true; break;
        case 33: p.area = 300; p.entry = 77; break;
        case 34: p.store = 1; p.area = 0; break;  // Missing AREA zero must not look up liquid zero.
        default: std::abort();
    }
    for (size_t i = 0; i < p.areas.size(); ++i) {
        p.areas[i] = AREA_CYCLE[i % 24];
        p.entries[i] = ENTRY_CYCLE[(i / 24) % 8];
        p.flagsTable[i] = FLAG_CYCLE[i % 5];
    }
    return p;
}
static GridMap grid(Profile& p, float ground) {
    GridMap g{};
    g.m_gridHeight = ground;
    g.m_gridGetHeight = &GridMap::getHeightFromFlat;
    g.m_gridArea = p.area;
    g.m_area_map = p.areaTable ? p.areas.data() : nullptr;
    g.m_liquidGlobalEntry = p.entry;
    g.m_liquidGlobalFlags = p.flags;
    g.m_liquid_offX = g.m_liquid_offY = 0;
    g.m_liquid_width = g.m_liquid_height = 128;
    g.m_liquidLevel = 0.f;
    g.m_liquidEntry = p.typeTable ? p.entries.data() : nullptr;
    g.m_liquidFlags = p.typeTable ? p.flagsTable.data() : nullptr;
    g.m_liquid_map = nullptr;
    return g;
}
static float world(float local) { return (32.f - (32.f + local / 128.f)) * SIZE_OF_GRIDS; }
// Boundary positions are authored in the AREA grid directly, independently of
// MLIQ's MAP_RESOLUTION conversion. Different tiles expose cancellation changes.
static float areaWorld(int tile, float cell) {
    return (32.f - (float(tile) + cell / 16.f)) * SIZE_OF_GRIDS;
}
static void sample(int id, Profile& p, float x, float y, float z = -1.f,
                   float collision = 2.f, uint8 mask = 0, float ground = -100.f,
                   bool wantsData = true, int tileX = 32, int tileY = 32) {
    stores(p.store);
    GridMap g = grid(p, ground);
    GridMapLiquidData data{0xdecafbad, 0xdeadbeef, fromBits(0x4a123456), fromBits(0xca654321)};
    auto status = g.getLiquidStatus(x, y, z, mask, wantsData ? &data : nullptr, collision);
    std::printf("%d\t%08x\t%08x\t%08x\t%08x\t%u\t%08x\t%d\t%u\t%u\t%u\t%08x\t%08x\t%u\t%d\t%d\n",
        id, bits(x), bits(y), bits(z), bits(collision), unsigned(mask), bits(ground),
        int(wantsData), unsigned(status), data.entry, data.type_flags, bits(data.level),
        bits(data.depth_level), unsigned(g.getArea(x, y)), tileX, tileY);
}
static std::array<float, 3> adjacent(float value) {
    return {std::nextafter(value, -INFINITY), value, std::nextafter(value, INFINITY)};
}
int main() {
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559, "binary32 required");
    static_assert(sizeof(AreaTableEntryfmt) == 26 && sizeof(LiquidTypefmt) == 5, "source formats");
    if (std::fesetround(FE_TONEAREST)) return 1;
    std::puts("# Exact complete GridMap::getLiquidStatus/getArea/GetAreaEntryByAreaID; cmangos 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f");
    std::puts("# Authored complete WDBC stores only; no game assets. Ground uses source flat-height dispatch/body.");
    std::puts("# Scope excludes TerrainInfo/VMAP liquid selection, terrain interpolation, admission and movement.");
    std::printf("# LiquidType format: %s; AreaTable format: %s\n", LiquidTypefmt, AreaTableEntryfmt);
    std::puts("# Stores 0..3: bit0 adds LiquidType ID 0; bit1 adds AreaTable ID 0; other absent IDs really are absent.");
    std::puts("# DBC strings are hex 0061006200630064006500660067006800; every row below is every raw WDBC word.");
    std::puts("# liquid_row columns: store Id LiquidId Type SpellId; area_row columns: store fields[0..24].");
    for (int variant = 0; variant < 4; ++variant) {
        for (const auto& words : LIQUID_WORDS) {
            if (words[0] == 0 && !(variant & 1)) continue;
            std::printf("# liquid_row\t%d", variant);
            for (auto word : words) std::printf("\t%u", word);
            std::puts("");
        }
        for (const auto& keys : AREA_KEYS) {
            if (keys[0] == 0 && !(variant & 2)) continue;
            std::printf("# area_row\t%d", variant);
            for (auto word : areaWords(keys)) std::printf("\t%u", word);
            std::puts("");
        }
    }
    std::puts("# AREA cells[i]=[100,101,102,103,104,105,106,107,108,109,110,112,114,115,0,300,65535,200,201,202,500,9000,107,109][i%24]");
    std::puts("# MLIQ entries[i]=[0,1,2,3,20,21,22,77][(i/24)%8]; flags[i]=[0,1,16,32,63][i%5]");
    std::puts("# Both cell tables are row-major 16*16. MLIQ is flat level +0, offX=offY=0, width=height=128.");
    std::puts("# Profile columns: id store global_area global_entry global_flags area_table type_table");
    for (int id = 0; id < 35; ++id) {
        const auto p = profile(id);
        std::printf("# profile\t%d\t%d\t%u\t%u\t%u\t%d\t%d\n", id, p.store,
            unsigned(p.area), unsigned(p.entry), unsigned(p.flags), int(p.areaTable), int(p.typeTable));
    }
    std::puts("# Queries select tiles 32,32 and 31,33. *_bits are hexadecimal binary32; other columns are decimal.");
    std::puts("# Cell edges use (32-(tile+cell/16))*SIZE_OF_GRIDS, adjacent world floats and cell +/- 1/1024.");
    std::puts("# Output starts entry=3735928559,type_flags=3737844653,level_bits=4a123456,depth_bits=ca654321.");
    std::puts("# NO_WATER preserves all sentinels; wants_data=0 preserves sentinels even with nonzero status.");
    std::puts("# profile\tx_bits\ty_bits\tz_bits\tcollision_bits\tmask\tground_bits\twants_data\tstatus\tentry\ttype_flags\tlevel_bits\tdepth_bits\traw_area\ttile_x\ttile_y");
    for (int id = 0; id < 35; ++id) {
        auto p = profile(id);
        if (!p.areaTable) {
            for (uint8 mask : {0, 1, 2, 4, 8, 15, 16, 32, 63})
                sample(id, p, world(4.f), world(4.f), -1.f, 2.f, mask);
        } else {
            // Read every AREA cell. Selected rows span every defined area category
            // and mask; no all-dimensions cross product is needed.
            for (int i = 0; i < 256; ++i)
                sample(id, p, world(float(i / 16 * 8 + 4)), world(float(i % 16 * 8 + 4)));
            for (int i = 0; i < 24; ++i)
                for (uint8 mask : {1, 2, 4, 8, 15, 16, 32, 63})
                    sample(id, p, world(float(i / 16 * 8 + 4)), world(float(i % 16 * 8 + 4)), -1.f, 2.f, mask);
            // Every internal AREA edge, both axes and two tiles. Adjacent world
            // ULPs may stay in one cell; the outer probes also reach both sides.
            for (const auto tile : {std::array<int, 2>{32, 32}, std::array<int, 2>{31, 33}})
                for (int edge = 1; edge < 16; ++edge)
                    for (int axis = 0; axis < 2; ++axis) {
                        const float center = areaWorld(tile[axis], float(edge));
                        for (float coordinate : {
                                areaWorld(tile[axis], float(edge) - 1.f / 1024.f),
                                std::nextafter(center, -INFINITY), center,
                                std::nextafter(center, INFINITY),
                                areaWorld(tile[axis], float(edge) + 1.f / 1024.f)}) {
                            const float x = axis == 0 ? coordinate : areaWorld(tile[0], 2.5f);
                            const float y = axis == 1 ? coordinate : areaWorld(tile[1], 1.5f);
                            sample(id, p, x, y, -1.f, 2.f, 0, -100.f, true, tile[0], tile[1]);
                        }
                    }
        }
    }
    // A direct override, a parent fallback, and a zero-row override each pass
    // the status/ground/null-output boundaries after their type has resolved.
    for (int id : {0, 1, 17}) {
        auto p = profile(id);
        for (float collision : {0.f, 2.f, 2.03128f, 16.f})
            for (float center : {-collision, 0.f, 1.f})
                for (float z : adjacent(center))
                    sample(id, p, world(4.f), world(4.f), z, collision);
        for (float ground : adjacent(0.f))
            sample(id, p, world(4.f), world(4.f), 0.f, 2.f, 0, ground);
        for (float z : adjacent(-102.f))
            sample(id, p, world(4.f), world(4.f), z);
        for (float z : {-1.f, 0.f, 1.f})
            sample(id, p, world(4.f), world(4.f), z, 2.f, 0, -100.f, false);
        sample(id, p, world(4.f), world(4.f), 0.f, 2.f, 0, INVALID_HEIGHT_VALUE);
    }
}
