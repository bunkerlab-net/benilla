// SPDX-License-Identifier: GPL-3.0-or-later
// Authored inputs/storage only. Exact pinned composite/grid/height methods are
// injected below; loaded original VMap queries execute real authored geometry.
#include <array>
#include <algorithm>
// PINNED_GRID_CONSTANTS
// PINNED_DBC_FORMATS
// PINNED_DBC_STRUCTS

static constexpr std::array<uint32, 4> LIQUID_WORDS[] = {
    {0, 701, 1, 31}, {1, 703, 3, 37}, {20, 709, 0, 41},
    {21, 719, 3, 43}, {22, 727, 2, 47}, {23, 733, 0, 53},
};
// ID, map, parent area, explore flag, liquid override. Missing IDs are absent
// from this complete authored store, rather than supplied by query callbacks.
static constexpr std::array<uint32, 5> AREA_KEYS[] = {
    {100, 0, 200, 9000, 22}, {101, 0, 200, 9001, 0},
    {102, 0, 0, 9002, 0}, {103, 0, 999, 9003, 0},
    {104, 0, 200, 9004, 77}, {105, 0, 201, 9005, 0},
    {106, 0, 107, 9006, 0}, {107, 0, 106, 9007, 0},
    {200, 0, 0, 9200, 23}, {201, 0, 200, 9201, 0},
};
static char DBC_STRINGS[] = "\0authored\0";
static std::array<uint32, 25> areaWords(const std::array<uint32, 5>& key) {
    std::array<uint32, 25> words{};
    for (size_t i = 0; i < words.size(); ++i) words[i] = 1000 + uint32(i) * 7;
    for (size_t i = 0; i < 4; ++i) words[i] = key[i];
    words[10] = uint32(-17);
    for (size_t i = 11; i <= 18; ++i) words[i] = 1;
    words[20] = 3;
    words[24] = key[4];
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
static size_t unsupportedWetCalls = 0;
static const AreaTableEntry* GetAreaEntryByAreaFlagAndMap(uint32, uint32) {
    ++unsupportedWetCalls;
    throw std::runtime_error("wet WMO area resolution is outside this oracle");
}
struct GridMap {
    // PINNED_GRID_FIELDS
    bool m_fullyLoaded = true;
    bool IsFullyLoaded() const { return m_fullyLoaded; }
    GridMapLiquidStatus getLiquidStatus(float, float, float, uint8, GridMapLiquidData*, float);
};
struct TerrainInfo {
    VMAP::VMapManager2* m_vmgr;
    GridMap* m_GridMaps[MAX_NUMBER_OF_GRIDS][MAX_NUMBER_OF_GRIDS]{};
    bool m_GridMapsLoadAttempted[MAX_NUMBER_OF_GRIDS][MAX_NUMBER_OF_GRIDS]{};
    uint32 GetMapId() const { return 0; }
    GridMap* GetGrid(float, float, bool = false);
    GridMap* LoadMapAndVMap(uint32, uint32, bool) {
        throw std::runtime_error("all queried terrain grids must already be loaded");
    }
    uint16 GetAreaFlag(float, float, float) const {
        ++unsupportedWetCalls;
        throw std::runtime_error("wet WMO area flag is outside this oracle");
    }
    float GetHeightStatic(float, float, float, bool = true, float = DEFAULT_HEIGHT_SEARCH) const;
    GridMapLiquidStatus getLiquidStatus(float, float, float, uint8, GridMapLiquidData*, float) const;
};
// PINNED_TERRAIN_METHODS

static float fromBits(uint32 word) { float value; std::memcpy(&value, &word, 4); return value; }
static constexpr unsigned PROFILE_COUNT = 29;
struct Profile {
    uint16 area = 100, entry = 21;
    uint8 flags = MAP_LIQUID_TYPE_WATER, offX = 0, offY = 0, width = 128, height = 128;
    float ground = 0.f, level = 5.f;
    bool floatHeight = false, holes = false, absentLiquid = false, liquidMap = false;
    bool cellTypes = false, cellAreas = false;
    std::array<float, 129 * 129> v9{};
    std::array<float, 128 * 128> v8{}, levels{};
    std::array<uint16, 256> entries{}, areas{};
    std::array<uint8, 256> types{};
};
static Profile profile(unsigned id) {
    Profile p;
    switch (id) {
        case 0: break; // location floor 4, static floor 6, raw water 5 survives.
        case 1: p.level = 4.f; break;
        case 2: p.level = std::nextafter(4.f, -INFINITY); break;
        case 3: p.level = std::nextafter(4.f, INFINITY); break;
        case 4: p.ground = 6.f; break; // raw water below raw terrain.
        case 5: p.ground = 5.f; break; // raw water equal to raw terrain remains valid.
        case 6: p.ground = 7.f; p.level = 8.f; break;
        case 7: p.ground = std::nextafter(7.f, INFINITY); p.level = 8.f; break;
        case 8: p.ground = -2.f; p.level = 8.f; break;
        case 9: p.flags = 0; break;
        case 10: p.entry = 77; p.flags = 32; break;
        case 11: p.entry = 1; p.flags = 63; break; // direct override ->22.
        case 12: p.entry = 1; p.flags = 63; p.area = 101; break; // parent ->23.
        case 13: p.entry = 1; p.flags = 16; p.area = 102; break; // override zero ->0.
        case 14: p.offX = 1; p.offY = 2; p.width = 3; p.height = 2; break;
        case 15: p.floatHeight = p.holes = true; p.ground = 2.f; break;
        case 16: p.floatHeight = true; p.ground = 2.f; break;
        case 17: p.holes = true; p.ground = 2.f; break; // flat source ignores holes.
        case 18: p.level = 0.f; break;
        case 19: p.ground = INVALID_HEIGHT_VALUE; break;
        case 20: p.entry = 0; p.flags = 63; p.area = 999; break; // real ID0, absent AREA.
        case 21: p.entry = 1; p.flags = 63; p.area = 103; break; // missing parent ->0.
        case 22: p.entry = 1; p.flags = 63; p.area = 104; break; // absent nonzero override.
        case 23: p.entry = 1; p.flags = 63; p.area = 105; break; // parent0, no grandparent.
        case 24: p.entry = 1; p.flags = 63; p.area = 106; break; // cycle does not recurse.
        case 25: p.absentLiquid = true; p.entry = 0; p.flags = 0; break;
        case 26:
            p.liquidMap = true; p.offX = 0; p.offY = 0; p.width = 4; p.height = 3;
            break;
        case 27: p.cellTypes = p.cellAreas = true; p.entry = 77; p.flags = 0; break;
        case 28: p.level = 8.f; break;
        default: throw std::runtime_error("unknown authored terrain profile");
    }
    p.v9.fill(p.ground); p.v8.fill(p.ground);
    for (int x = 0; x < p.height; ++x) for (int y = 0; y < p.width; ++y)
        p.levels[x * p.width + y] = p.level + float(x) * .5f + float(y) * .25f;
    static constexpr uint16 entries[] = {1, 0, 21, 77};
    static constexpr uint16 areas[] = {100, 101, 102, 999, 104, 105, 106};
    static constexpr uint8 flags[] = {0, 1, 16, 32, 63};
    for (size_t i = 0; i < 256; ++i) {
        p.entries[i] = entries[i % 4];
        p.areas[i] = areas[i % 7];
        p.types[i] = flags[i % 5];
    }
    return p;
}
static GridMap grid(Profile& p) {
    GridMap g{};
    g.m_gridArea = p.area;
    g.m_area_map = p.cellAreas ? p.areas.data() : nullptr;
    g.m_gridHeight = p.ground;
    g.m_gridGetHeight = p.floatHeight ? &GridMap::getHeightFromFloat : &GridMap::getHeightFromFlat;
    g.m_V9 = p.floatHeight ? p.v9.data() : nullptr;
    g.m_V8 = p.floatHeight ? p.v8.data() : nullptr;
    for (auto& row : g.m_holes) for (auto& word : row) word = p.holes ? 0xffff : 0;
    g.m_liquidGlobalEntry = p.entry; g.m_liquidGlobalFlags = p.flags;
    g.m_liquid_offX = p.offX; g.m_liquid_offY = p.offY;
    g.m_liquid_width = p.width; g.m_liquid_height = p.height;
    g.m_liquidLevel = p.level;
    g.m_liquidEntry = p.cellTypes ? p.entries.data() : nullptr;
    g.m_liquidFlags = p.cellTypes ? p.types.data() : nullptr;
    g.m_liquid_map = p.liquidMap ? p.levels.data() : nullptr;
    return g;
}
static uint32 fourcc(const char* value) { uint32 word; std::memcpy(&word, value, 4); return word; }
static void serializeMap(const std::filesystem::path& path, const Profile& p, const GridMap& g) {
    static_assert(sizeof(GridMapFileHeader) == 40 && sizeof(GridMapAreaHeader) == 8 &&
                  sizeof(GridMapHeightHeader) == 16 && sizeof(GridMapLiquidHeader) == 16);
    const uint32 areaSize = p.cellAreas ? 520 : 8;
    const uint32 heightSize = p.floatHeight ? 132116 : 16;
    const uint32 liquidSize = p.absentLiquid ? 0 : 16 + (p.cellTypes ? 768 : 0) +
                             (p.liquidMap ? uint32(p.width) * p.height * 4 : 0);
    const uint32 liquidStart = 40 + areaSize + heightSize;
    GridMapFileHeader header{fourcc("MAPS"), fourcc("z1.4"), 40, areaSize, 40 + areaSize,
        heightSize, p.absentLiquid ? 0 : liquidStart, liquidSize, liquidStart + liquidSize, 512};
    GridMapAreaHeader areaHeader{fourcc("AREA"), uint16(p.cellAreas ? 0 : MAP_AREA_NO_AREA), p.area};
    GridMapHeightHeader heightHeader{fourcc("MHGT"), uint32(p.floatHeight ? 0 : MAP_HEIGHT_NO_HEIGHT), p.ground, p.ground};
    GridMapLiquidHeader liquidHeader{fourcc("MLIQ"),
        uint8((p.cellTypes ? 0 : MAP_LIQUID_NO_TYPE) | (p.liquidMap ? 0 : MAP_LIQUID_NO_HEIGHT)),
        p.flags, p.entry, p.offX, p.offY, p.width, p.height, p.level};
    FILE* file = std::fopen(path.c_str(), "wb");
    require(file, "open authored map");
    writeValue(file, header); writeValue(file, areaHeader);
    if (p.cellAreas) writeBytes(file, p.areas.data(), 512);
    writeValue(file, heightHeader);
    if (p.floatHeight) {
        writeBytes(file, p.v9.data(), p.v9.size() * 4);
        writeBytes(file, p.v8.data(), p.v8.size() * 4);
    }
    if (!p.absentLiquid) {
        writeValue(file, liquidHeader);
        if (p.cellTypes) { writeBytes(file, p.entries.data(), 512); writeBytes(file, p.types.data(), 256); }
        if (p.liquidMap) writeBytes(file, p.levels.data(), size_t(p.width) * p.height * 4);
    }
    writeBytes(file, g.m_holes, 512);
    require(std::fclose(file) == 0, "close authored map");
}
template<size_t N> static void serializeDbc(const std::filesystem::path& path,
                                           const std::vector<std::array<uint32, N>>& rows) {
    FILE* file = std::fopen(path.c_str(), "wb");
    require(file, "open authored DBC");
    writeBytes(file, "WDBC", 4);
    for (uint32 word : {uint32(rows.size()), uint32(N), uint32(N * 4), uint32(sizeof(DBC_STRINGS))}) writeValue(file, word);
    for (const auto& row : rows) writeBytes(file, row.data(), N * 4);
    writeBytes(file, DBC_STRINGS, sizeof(DBC_STRINGS));
    require(std::fclose(file) == 0, "close authored DBC");
}
static void setupStores(const std::filesystem::path& root) {
    std::vector<std::array<uint32, 4>> liquids;
    std::vector<std::array<uint32, 25>> areas;
    for (const auto& words : LIQUID_WORDS) {
        liquids.push_back(words);
        sLiquidTypeStore.rows.push_back({words[0], words[1], words[2], words[3]});
    }
    for (const auto& keys : AREA_KEYS) {
        const auto words = areaWords(keys);
        areas.push_back(words);
        AreaTableEntry row{};
        row.ID = words[0]; row.mapid = words[1]; row.zone = words[2]; row.exploreFlag = words[3];
        row.flags = words[4]; row.area_level = int32(words[10]); row.team = words[20];
        row.LiquidTypeOverride = words[24];
        for (size_t i = 0; i < 8; ++i) row.area_name[i] = DBC_STRINGS + words[11 + i];
        sAreaStore.rows.push_back(row);
    }
    serializeDbc(root / "LiquidType.dbc.test-fixture", liquids);
    serializeDbc(root / "AreaTable.dbc.test-fixture", areas);
}
static GridMapLiquidData sentinel() { return {0xdecafbad, 0xdeadbeef, fromBits(0x4a123456), fromBits(0xca654321)}; }
static bool sameData(const GridMapLiquidData& a, const GridMapLiquidData& b) {
    return a.entry == b.entry && a.type_flags == b.type_flags && bits(a.level) == bits(b.level)
        && bits(a.depth_level) == bits(b.depth_level);
}
static void terrainLiquidQueries(const std::filesystem::path& output, VMAP::StaticMapTree& tree) {
    const auto root = output / "terrain-liquid";
    std::filesystem::create_directory(root);
    setupStores(root);
    VMAP::VMapManager2 manager;
    manager.iInstanceMapTrees[0] = &tree;
    std::ofstream out(output / "terrain-liquid.tsv");
    out << "# Full exact-source TerrainInfo::getLiquidStatus/GetHeightStatic/GetGrid + GridMap liquid/float/flat/AREA; CMaNGOS "
           "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
        << "# Original loaded scopedlocation VMap geometry; actual GetLocationInfo/GetLiquidLevel floor side effects; all WMO models dry\n"
        << "# Authored raw MAPS/WDBC products in terrain-liquid/; profile ID selects ID.map.test-fixture at map0/tile0,0; complete DBC stores\n"
        << "# LiquidType format: " << LiquidTypefmt << "; AreaTable format: " << AreaTableEntryfmt << '\n'
        << "# Every float column is hexadecimal binary32; other numbers decimal. Output untouched sentinels: entry=3735928559 flags=3737844653 level=4a123456 depth=ca654321\n"
        << "# No wet-WMO or admission claim; unimplemented wet-only area helpers throw if called; no unloaded-grid equivalence cases\n"
        << "# profile\tcase\tx\ty\tz\tcollision\tmask\twants_data\tstatus\tentry\ttype_flags\tlevel\tdepth\tlocation_hit\tlocation_floor\tstatic_height\teffective_floor\traw_status\traw_entry\traw_flags\traw_level\traw_depth\traw_height\ttile_x\ttile_y\n";
    size_t count = 0, retained = 0, filtered = 0, overwritten = 0, noLocation = 0;
    bool statuses[16]{};
    for (unsigned id = 0; id < PROFILE_COUNT; ++id) {
        auto p = profile(id);
        auto g = grid(p);
        serializeMap(root / (std::to_string(id) + ".map.test-fixture"), p, g);
        TerrainInfo terrain{&manager};
        terrain.m_GridMaps[0][0] = &g;
        auto sample = [&](const std::string& name, Vector3 internal, float collision = 2.f,
                          uint8 mask = 0, bool wantsData = true) {
            const auto world = manager.convertPositionToInternalRep(internal.x, internal.y, internal.z);
            internal = manager.convertPositionToInternalRep(world.x, world.y, world.z);
            const int gx = int(32 - world.x / SIZE_OF_GRIDS), gy = int(32 - world.y / SIZE_OF_GRIDS);
            require(gx == 0 && gy == 0, "all authored queries address loaded tile0,0");
            VMAP::LocationInfo location;
            const bool hit = tree.GetLocationInfo(internal, location);
            const float selected = terrain.GetHeightStatic(world.x, world.y, world.z, true, DEFAULT_WATER_SEARCH);
            float floor = selected, liquid = INVALID_HEIGHT_VALUE;
            uint32 type = 0;
            require(!manager.GetLiquidLevel(0, world.x, world.y, world.z, mask, liquid, floor, type), "no wet source models");
            require(bits(liquid) == bits(INVALID_HEIGHT_VALUE), "dry source leaves liquid level unchanged");
            require(type == 0 && bits(floor) == bits(hit ? location.ground_Z : selected), "dry source actual floor overwrite");
            auto raw = sentinel(), result = sentinel();
            const auto rawStatus = g.getLiquidStatus(world.x, world.y, world.z, mask, &raw, collision);
            const auto status = terrain.getLiquidStatus(world.x, world.y, world.z, mask, wantsData ? &result : nullptr, collision);
            require(wantsData && status != LIQUID_MAP_NO_WATER ? sameData(result, raw) : sameData(result, sentinel()),
                    "composite preserves raw GridMap data or caller sentinels");
            if (id == 0 && name == "floor4_static6_water5") {
                require(hit && bits(location.ground_Z) == bits(4.f) && bits(selected) == bits(6.f), "real point4/static6 witness");
                require(status == LIQUID_MAP_WATER_WALK && bits(result.level) == bits(5.f) && bits(result.depth_level) == bits(0.f),
                        "water5 survives original floor side effect preserving raw depth0");
            }
            out << std::dec << id << '\t' << name;
            auto hex = [&](float value) { out << '\t' << std::hex << std::setw(8) << std::setfill('0') << bits(value); };
            for (float value : {world.x, world.y, world.z, collision}) hex(value);
            out << std::dec << '\t' << unsigned(mask) << '\t' << unsigned(wantsData) << '\t' << unsigned(status)
                << '\t' << result.entry << '\t' << result.type_flags;
            hex(result.level); hex(result.depth_level);
            out << std::dec << '\t' << unsigned(hit);
            hex(location.ground_Z); hex(selected); hex(floor);
            out << std::dec << '\t' << unsigned(rawStatus) << '\t' << raw.entry << '\t' << raw.type_flags;
            hex(raw.level); hex(raw.depth_level); hex(g.getHeight(world.x, world.y));
            out << std::dec << '\t' << gx << '\t' << gy << '\n';
            ++count; statuses[status] = true;
            retained += status != LIQUID_MAP_NO_WATER;
            filtered += rawStatus != LIQUID_MAP_NO_WATER && status == LIQUID_MAP_NO_WATER;
            overwritten += hit && bits(floor) != bits(selected);
            noLocation += !hit;
        };
        sample("floor4_static6_water5", {0, 0, 5});
        for (float z : {-2.f, 0.f, 2.f, 3.9f, 3.95f, 4.f, 5.f, 5.9f, 6.f, 7.f, 8.f, 11.f, 60.f})
            for (uint8 mask : {0, 1, 2, 4, 8, 15, 16, 32, 63})
                sample("status_mask_" + std::to_string(bits(z)) + "_" + std::to_string(mask), {0, 0, z}, 2.f, mask);
        for (float collision : {0.f, 2.f, 2.03128f})
            for (float center : {p.level - collision, p.level, p.level + 1.f})
                for (float z : {std::nextafter(center, -INFINITY), center, std::nextafter(center, INFINITY)})
                    sample("threshold_" + std::to_string(bits(collision)) + "_" + std::to_string(bits(z)), {0, 0, z}, collision);
        for (float z : {3.f, 5.f, 11.f}) sample("null_data_" + std::to_string(bits(z)), {0, 0, z}, 2.f, 0, false);
        if (id == 0 || id == 1 || id == 15 || id == 18 || id == 19 || id == 28)
            for (const Vector3 xy : {Vector3(3.9f, 3.9f, 0), Vector3(4.01f, 0, 0), Vector3(15, 0, 0),
                                    Vector3(30, 0, 0), Vector3(70, 20, 0), Vector3(130, 5, 0), Vector3(50, 10, 0)})
                for (float z : {-.5f, 0.f, 3.95f, 5.f, 7.f, 8.f, 11.f, 60.f})
                    sample("geometry_" + std::to_string(bits(xy.x)) + "_" + std::to_string(bits(xy.y)) + "_" + std::to_string(bits(z)), {xy.x, xy.y, z});
        if (id == 14 || id == 26 || id == 27)
            for (float x : {0.f, 4.f, 4.2f, 8.2f, 8.4f, 12.4f, 12.6f, 16.8f, 35.f, 70.f, 100.f, 200.f, 400.f, 530.f})
                for (float y : {0.f, 4.2f, 8.4f, 12.6f, 16.8f, 35.f, 70.f, 200.f, 400.f, 530.f})
                    sample("cells_" + std::to_string(bits(x)) + "_" + std::to_string(bits(y)), {x, y, 5});
    }
    require(unsupportedWetCalls == 0, "unsupported wet-only helpers never called");
    for (unsigned status : {0, 1, 2, 4, 8}) require(statuses[status], "all source liquid statuses represented");
    require(retained && filtered && overwritten && noLocation, "composite branch witnesses present");
    out.flush(); require(bool(out), "write composite fixture");
    std::printf("Exact pinned TerrainInfo dry-VMap liquid: %zu rows; %zu retained, %zu filtered, %zu actual floor overwrites, %zu no-location; zero wet calls\n",
                count, retained, filtered, overwritten, noLocation);
}
int main(int argc, char** argv) { return authored_location_main(argc, argv); }
