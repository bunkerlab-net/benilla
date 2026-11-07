// SPDX-License-Identifier: GPL-3.0-or-later
// Authored WDBC/MAPS inputs only; selection and loading execute pinned source.
#include <array>
#include <list>
#include <utility>
#include "Database/DBCStore.h"
// PINNED_GRID_CONSTANTS
// PINNED_DBC_FORMATS
// PINNED_DBC_STRUCTS
// PINNED_DBC_INDEX_TYPES
static DBCStorage<AreaTableEntry> sAreaStore(AreaTableEntryfmt);
static DBCStorage<MapEntry> sMapStore(MapEntryfmt);
static DBCStorage<WMOAreaTableEntry> sWMOAreaTableStore(WMOAreaTableEntryfmt);
static AreaIDByAreaFlag sAreaIDByAreaFlag;
static AreaFlagByMapID sAreaFlagByMapID;
static WMOAreaInfoByTripple sWMOAreaInfoByTripple;
// PINNED_DBC_METHODS
// Only logging and unrelated lazy-load ownership infrastructure are substituted.
#undef DEBUG_FILTER_LOG
#define DEBUG_FILTER_LOG(...) do {} while (0)
struct OracleLog {
    template<class... Args> void outError(const char* format, Args... args) {
        std::fprintf(stderr, format, args...);
    }
} sLog;
// PINNED_GRID_CLASS
struct TerrainManager {
    static void GetZoneAndAreaIdByAreaFlag(uint32&, uint32&, uint16, uint32);
};
struct TerrainInfo {
    uint32 m_mapId;
    VMAP::VMapManager2* m_vmgr;
    GridMap* m_GridMaps[MAX_NUMBER_OF_GRIDS][MAX_NUMBER_OF_GRIDS]{};
    bool m_GridMapsLoadAttempted[MAX_NUMBER_OF_GRIDS][MAX_NUMBER_OF_GRIDS]{};
    uint32 GetMapId() const { return m_mapId; }
    GridMap* GetGrid(float, float, bool = false);
    GridMap* LoadMapAndVMap(uint32, uint32, bool) {
        throw std::runtime_error("all area-oracle terrain grids must already be loaded");
    }
    bool GetAreaInfo(float, float, float, uint32&, int32&, int32&, int32&) const;
    uint16 GetAreaFlag(float, float, float, bool* = nullptr, int32* = nullptr) const;
    void GetZoneAndAreaId(uint32&, uint32&, float, float, float, int32* = nullptr) const;
};
// PINNED_TERRAIN_METHODS

static const char AREA_STRINGS[] = "\0authored-area\0authored-map\0authored-wmo\0";
static constexpr unsigned STORE_COUNT = 6, PROFILE_COUNT = 20;
static constexpr std::array<uint32, 6> AREA_KEYS[] = {
    // Intentionally unsorted physical records; source indices are numeric IDs.
    // ID, map, parent, exploreFlag, flags, liquid override.
    {220, 8, 600, 1001, 0x400, 33}, {205, 0, 0, 1000, 0x800, 34},
    {120, 0, 121, 9002, 0x10, 0}, {110, 0, 600, 9001, 0x20, 21},
    {600, 9, 601, 9600, 0x40, 23}, {3, 7, 0, 9100, 0x80, 24},
    {200, 0, 8888, 1000, 0x100, 25}, {140, 0, 600, 70000, 0x200, 26},
    {141, 0, 601, 4464, 0x400, 27}, {150, 0, 0, 0, 0x800, 28},
    {151, 0, 0, 65536, 0x1000, 29}, {130, 7, 0, 9003, 0x2000, 30},
    {160, 0, 0, 9100, 0x4000, 31}, {5, 0, 600, 9100, 0x8000, 32},
    {210, 7, 0, 1001, 0x10000, 35}, {190, 7, 0, 1000, 0x20000, 36},
    {230, 0, 0, 0, 0x40000, 37}, {240, 0, 0, 65535, 0x80000, 38},
    {500, 7, 600, 9500, 0x100000, 0}, {601, 0, 0, 9601, 0x200000, 41},
    {121, 0, 120, 9121, 0x400000, 42},
};
static std::array<uint32,25> areaWords(const std::array<uint32,6>& key) {
    std::array<uint32,25> words{};
    for (size_t i = 0; i < words.size(); ++i) words[i] = 101 + uint32(i) * 7;
    for (size_t i = 0; i < 5; ++i) words[i] = key[i];
    words[10] = uint32(-17);
    for (size_t i = 11; i <= 18; ++i) words[i] = 1;
    words[20] = 3; words[24] = key[5];
    return words;
}
static std::array<uint32,42> mapWords(uint32 id, uint32 linked) {
    std::array<uint32,42> words{};
    for (size_t i = 0; i < words.size(); ++i) words[i] = 701 + uint32(i) * 11;
    words[0] = id; words[1] = 14; words[2] = 0; words[3] = 0;
    for (size_t i = 4; i <= 11; ++i) words[i] = 14;
    for (size_t i = 20; i <= 27; ++i) words[i] = 14;
    for (size_t i = 29; i <= 36; ++i) words[i] = 14;
    words[19] = linked; words[41] = bits(1.f);
    return words;
}
static std::array<uint32,20> wmoWords(uint32 id, uint32 root, uint32 adt,
                                      uint32 group, uint32 area) {
    std::array<uint32,20> words{};
    for (size_t i = 0; i < words.size(); ++i) words[i] = 1701 + uint32(i) * 13;
    words[0] = id; words[1] = root; words[2] = adt; words[3] = group;
    words[9] = 0x80000000u | id; words[10] = area;
    for (size_t i = 11; i <= 18; ++i) words[i] = 27;
    return words;
}
template<size_t N> static void writeAreaDbc(const std::filesystem::path& path,
                                           const std::vector<std::array<uint32,N>>& rows) {
    FILE* file = std::fopen(path.c_str(), "wb");
    require(file, "open authored complete WDBC");
    writeBytes(file, "WDBC", 4);
    for (uint32 word : {uint32(rows.size()), uint32(N), uint32(N * 4), uint32(sizeof(AREA_STRINGS))})
        writeValue(file, word);
    for (const auto& row : rows) writeBytes(file, row.data(), row.size() * 4);
    writeBytes(file, AREA_STRINGS, sizeof(AREA_STRINGS));
    require(std::fclose(file) == 0, "close authored WDBC");
}
static void writeAreaStores(const std::filesystem::path& root, unsigned variant) {
    const auto dir = root / "terrain-area" / ("store" + std::to_string(variant));
    std::filesystem::create_directories(dir);
    std::vector<std::array<uint32,25>> areas;
    for (const auto& key : AREA_KEYS) areas.push_back(areaWords(key));
    if (variant == 1 || variant == 4) areas.insert(areas.begin() + 7, areaWords({0,0,601,9200,0x100,45}));
    writeAreaDbc(dir / "AreaTable.dbc.test-fixture", areas);
    std::vector<std::array<uint32,42>> maps{mapWords(8, 500), mapWords(7, 0)};
    if (variant != 2) maps.push_back(mapWords(0, variant == 3 ? 999 : variant >= 4 ? 0 : 500));
    writeAreaDbc(dir / "Map.dbc.test-fixture", maps);
    std::vector<std::array<uint32,20>> wmos{
        wmoWords(80,0xf2222222,33025,0xe2222222,120),
        wmoWords(10,0xf2222222,33025,0xe2222222,110),
        wmoWords(90,0xf2222222,33025,0xe2222222,0),
        wmoWords(70,0xf2222222,33025,0xe2222222,999),
        wmoWords(40,0xf2222222,33025,0xe2222222,130),
        wmoWords(101,0xf1111111,33025,0xe1111111,140),
        wmoWords(102,0xf3333333,33027,0xe3333333,150),
        wmoWords(103,0xf1111111,33028,0xe1111111,130),
        wmoWords(104,0xf4444444,33029,0xe4444443,999),
        wmoWords(105,0xf4444444,33029,0xffffffff,110),
        wmoWords(106,0xf4444444,33031,0xe4444443,0),
        wmoWords(107,0xf4444444,33031,0xffffffff,110),
        wmoWords(108,0xf1111111,33032,0xffffffff,110),
        wmoWords(109,0xf1111111,33035,0xe1111111,160),
    };
    for (uint32 i = 0; i < 4; ++i) wmos.push_back(wmoWords(120 + i,0xf4444444,33030,0xe4444440 + i,151));
    // Same root/group with a different ADT must not match the mixed source tuple.
    wmos.push_back(wmoWords(140,0xf2222222,33026,0xe2222222,160));
    writeAreaDbc(dir / "WMOAreaTable.dbc.test-fixture", wmos);
}
static void loadAreaStores(const std::filesystem::path& root, unsigned variant) {
    sWMOAreaInfoByTripple.clear(); sAreaIDByAreaFlag.clear(); sAreaFlagByMapID.clear();
    sAreaStore.Clear(); sMapStore.Clear(); sWMOAreaTableStore.Clear();
    const auto dir = root / "terrain-area" / ("store" + std::to_string(variant));
    require(sAreaStore.Load((dir / "AreaTable.dbc.test-fixture").c_str()), "original AreaTable WDBC loader");
    require(sMapStore.Load((dir / "Map.dbc.test-fixture").c_str()), "original Map WDBC loader");
    require(sWMOAreaTableStore.Load((dir / "WMOAreaTable.dbc.test-fixture").c_str()), "original WMOArea WDBC loader");
    require(sAreaStore.GetFieldCount() == 25 && sMapStore.GetFieldCount() == 42 && sWMOAreaTableStore.GetFieldCount() == 20,
            "complete source WDBC field counts");
    require(sizeof(AreaTableEntry) == DBCFileLoader::GetFormatRecordSize(AreaTableEntryfmt) &&
            sizeof(MapEntry) == DBCFileLoader::GetFormatRecordSize(MapEntryfmt) &&
            sizeof(WMOAreaTableEntry) == DBCFileLoader::GetFormatRecordSize(WMOAreaTableEntryfmt), "source packed DBC ABI");
    // PINNED_BUILD_DBC_INDICES
}
struct AreaProfile {
    uint16 area = 1000;
    float height = 0;
    unsigned encoding = 0;
    bool holes = false, cells = false, missingHeight = false, slope = false;
};
static AreaProfile areaProfile(unsigned id) {
    AreaProfile p;
    switch (id) {
        case 0: break;
        case 1: p.height = 6; break;
        case 2: p.height = std::nextafter(6.f,-INFINITY); break;
        case 3: p.height = std::nextafter(6.f,INFINITY); break;
        case 4: p.height = 9; break;
        case 5: p.height = std::nextafter(9.f,-INFINITY); break;
        case 6: p.height = std::nextafter(9.f,INFINITY); break;
        case 7: p.height = INVALID_HEIGHT_VALUE; break;
        case 8: p.height = 7; p.encoding = 1; p.holes = true; break;
        case 9: p.height = 7; p.encoding = 1; break;
        case 10: p.height = 7; p.holes = true; break;
        case 11: p.area = 0; break;
        case 12: p.area = 1001; break;
        case 13: p.area = 1002; break;
        case 14: p.area = 65535; break;
        case 15: p.cells = true; break;
        case 16: p.missingHeight = true; break;
        case 17: p.height = 7; p.encoding = 2; p.holes = true; break;
        case 18: p.height = 7; p.encoding = 3; p.holes = true; break;
        case 19: p.height = 0; p.encoding = 1; p.slope = true; p.cells = true; break;
        default: throw std::runtime_error("unknown area MAPS profile");
    }
    return p;
}
static uint32 areaFourcc(const char* chars) { uint32 result; std::memcpy(&result, chars, 4); return result; }
static void writeAreaMap(const std::filesystem::path& path, unsigned id) {
    const auto p = areaProfile(id);
    const uint32 areaSize = p.cells ? 520 : 8;
    const uint32 heightSize = p.missingHeight ? 0 : 16 + (p.encoding ? (129 * 129 + 128 * 128) * (p.encoding == 1 ? 4 : p.encoding == 2 ? 1 : 2) : 0);
    GridMapFileHeader head{areaFourcc("MAPS"),areaFourcc("z1.4"),40,areaSize,
        p.missingHeight ? 0 : 40 + areaSize,heightSize,0,0,40 + areaSize + heightSize,512};
    GridMapAreaHeader area{areaFourcc("AREA"),uint16(p.cells ? 0 : MAP_AREA_NO_AREA),p.area};
    GridMapHeightHeader height{areaFourcc("MHGT"),uint32(p.encoding == 0 ? MAP_HEIGHT_NO_HEIGHT :
        p.encoding == 2 ? MAP_HEIGHT_AS_INT8 : p.encoding == 3 ? MAP_HEIGHT_AS_INT16 : 0),
        p.slope ? -1.5f : p.height,p.slope ? 6.5f : p.encoding == 0 ? p.height : p.height + 255};
    FILE* file = std::fopen(path.c_str(), "wb"); require(file, "open authored MAPS");
    writeValue(file,head); writeValue(file,area);
    static constexpr uint16 cells[] = {1000,1001,1002,0,65535,9100,4464,9002};
    if (p.cells) for (unsigned i = 0; i < 256; ++i) writeValue(file,cells[i % 8]);
    if (!p.missingHeight) {
        writeValue(file,height);
        for (unsigned side : {129u,128u}) for (unsigned x = 0; x < side; ++x) for (unsigned y = 0; y < side; ++y) {
            if (!p.encoding) continue;
            if (p.encoding == 1) {
                const float value = p.slope ? (float((x + 3 * y) % 17) - 3.f) * .5f : p.height;
                writeValue(file,value);
            } else if (p.encoding == 2) writeValue(file,uint8(0));
            else writeValue(file,uint16(0));
        }
    }
    for (unsigned i = 0; i < 256; ++i) writeValue(file,uint16(p.holes ? 0xffff : 0));
    require(std::fclose(file) == 0, "close authored MAPS");
}
static void writeResolvedArea(std::ostream& out, const AreaTableEntry* row) {
    out << '\t' << unsigned(row != nullptr);
    if (row) out << '\t' << row->ID << '\t' << row->mapid << '\t' << row->zone
                 << '\t' << row->exploreFlag << '\t' << row->LiquidTypeOverride;
    else out << "\t0\t0\t0\t0\t0";
}
static size_t terrainAreaCount = 0, terrainRejected = 0;
static void terrainAreaQueries(const std::filesystem::path& root, VMAP::StaticMapTree& tree,
                               const std::string& dataset) {
    const auto dir = root / "terrain-area";
    if (dataset == "scopedarea/forward") {
        std::filesystem::create_directories(dir);
        for (unsigned store = 0; store < STORE_COUNT; ++store) writeAreaStores(root,store);
        for (unsigned profile = 0; profile < PROFILE_COUNT; ++profile)
            writeAreaMap(dir / (std::to_string(profile) + ".map.test-fixture"),profile);
    }
    std::ofstream out(root / "terrain_area.tsv", dataset == "scopedarea/forward" ? std::ios::out : std::ios::app);
    if (dataset == "scopedarea/forward") out <<
        "# Exact pinned TerrainInfo area closure; source 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f\n"
        "# Original MAPS/WDBC/VMap loaders reread authored products; existing native fixture bytes verified unchanged\n"
        "# Floats binary32 hexadecimal; flags and IDs decimal uint32 patterns; all grids present, no source-nullgrid cases\n"
        "# dataset\tstore\tprofile\tcase\tmap_id\tx\ty\tz\traw_hit\traw_floor\traw_flags\traw_adt\traw_root\traw_group\tarea_info_hit\tterrain_height\tterrain_area\tarea_flag\tarea_present\tarea_id\tarea_map\tarea_zone\tarea_explore\tarea_liquid\tparent_present\tparent_id\tparent_map\tparent_zone\tparent_explore\tparent_liquid\ttile_x\ttile_y\n";
    VMAP::VMapManager2 reconstructed; reconstructed.iInstanceMapTrees[0] = &tree;
    VMAP::VMapManager2 manager;
    VMAP::StaticMapTree loaded(0,(root / dataset).string());
    require(loaded.InitMap("000.vmtree",&manager),"original StaticMapTree::InitMap authored bytes");
    require(loaded.LoadMapTile(32,32,&manager),"original StaticMapTree::LoadMapTile authored bytes");
    require(loaded.iNTreeValues == tree.iNTreeValues && loaded.iLoadedSpawns.size() == tree.iNTreeValues,
            "all authored VMap placements reread from native bytes");
    manager.iInstanceMapTrees[0] = &loaded;
    auto sample = [&](unsigned store, unsigned profile, const std::string& name, const Vector3& world, GridMap& grid) {
        const int gx = int(32 - world.x / SIZE_OF_GRIDS), gy = int(32 - world.y / SIZE_OF_GRIDS);
        require(gx >= 0 && gx < 64 && gy >= 0 && gy < 64,"authored coordinates bounded for original GetGrid");
        TerrainInfo terrain{}; terrain.m_mapId = 0; terrain.m_vmgr = &manager; terrain.m_GridMaps[gx][gy] = &grid;
        AreaObservation raw{false,world.z,INITIAL_FLAGS,INITIAL_ADT,INITIAL_ROOT,INITIAL_GROUP};
        raw.hit = manager.getAreaInfo(0,world.x,world.y,raw.z,raw.flags,raw.adt,raw.root,raw.group);
        AreaObservation constructed{false,world.z,INITIAL_FLAGS,INITIAL_ADT,INITIAL_ROOT,INITIAL_GROUP};
        constructed.hit = reconstructed.getAreaInfo(0,world.x,world.y,constructed.z,constructed.flags,
                                                  constructed.adt,constructed.root,constructed.group);
        require(sameArea(raw,constructed),"native VMap reload equals source writer geometry bit-for-bit");
        uint32 flags = INITIAL_FLAGS; int32 adt = INITIAL_ADT, rootId = INITIAL_ROOT, group = INITIAL_GROUP;
        const bool accepted = terrain.GetAreaInfo(world.x,world.y,world.z,flags,adt,rootId,group);
        require(flags == raw.flags && adt == raw.adt && rootId == raw.root && group == raw.group,
                "TerrainInfo rejection retains source raw metadata arguments");
        const uint16 flag = terrain.GetAreaFlag(world.x,world.y,world.z);
        const auto* area = GetAreaEntryByAreaFlagAndMap(flag,0);
        const auto* parent = area && area->zone ? GetAreaEntryByAreaID(area->zone) : nullptr;
        uint32 zoneId = 0xffffffffu, areaId = 0xffffffffu;
        terrain.GetZoneAndAreaId(zoneId, areaId, world.x, world.y, world.z);
        require(areaId == (area ? area->ID : 0u) &&
                zoneId == (area ? (area->zone ? area->zone : area->ID) : 0u),
                "original GetZoneAndAreaId uses selected row directly, including zero and missing");
        if (store == 0 && profile <= 10 && name == "internal_0_z1088421888") {
            const bool expected[] = {true,true,true,false,true,false,true,true,true,false,false};
            require(raw.hit && accepted == expected[profile],"strict terrain occlusion boundaries and hole dispatch");
        }
        if (profile == 0 && name == "internal_0_z1088421888") {
            if (dataset == "scopedarea/forward") {
                require(raw.adt == 33025 && uint32(raw.root) == 0xf2222222 && bits(raw.z) == bits(6.f),
                        "raw floor and ADT differ from last successful instance metadata");
                require(area && area->ID == ((store == 1 || store == 4) ? 0u : 120u),
                        "last eligible WMO ID wins including present AreaTable zero");
            } else require(flag == 4464 && area && area->ID == 141,"u16 narrowing precedes final area lookup");
        }
        if (profile == 0 && name == "internal_300_z1088421888")
            require(area && area->ID == 5,"first same-map area flag by ID, not WMO area or physical order");
        if (profile == 12 && name == "internal_15_z1088421888")
            require(area && area->ID == 220,"last wrong-map area flag by ID, before linked-zone fallback");
        if ((profile == 11 || profile == 13) && name == "internal_15_z1088421888") {
            require((area != nullptr) == (store == 0 || store == 1 || store == 4),"linked-zone missing, zero and absent map cases");
            if (area) require(area->ID == (store == 4 ? 0u : 500u),"zero flag and unmatched flag use Map linked zone");
        }
        if (store == 0 && profile == 0 && (name == "internal_60_z1088421888" ||
            name == "internal_200_z1088421888" || name == "scaled_zero_area"))
            require(raw.hit && flag == 1000,"missing/zero area and group-minus-one never bypass exact triple lookup");

        out << dataset << '\t' << store << '\t' << profile << '\t' << name << "\t0";
        for (float value : {world.x,world.y,world.z}) putFloat(out,value);
        out << '\t' << unsigned(raw.hit); putFloat(out,raw.z);
        out << '\t' << raw.flags << '\t' << uint32(raw.adt) << '\t' << uint32(raw.root) << '\t' << uint32(raw.group)
            << '\t' << unsigned(accepted);
        putFloat(out,grid.getHeight(world.x,world.y));
        out << '\t' << grid.getArea(world.x,world.y) << '\t' << flag;
        writeResolvedArea(out,area); writeResolvedArea(out,parent);
        out << '\t' << gx << '\t' << gy << '\n';
        ++terrainAreaCount; terrainRejected += raw.hit && !accepted;
    };
    for (unsigned store = 0; store < STORE_COUNT; ++store) {
        loadAreaStores(root,store);
        for (unsigned profile = 0; profile < PROFILE_COUNT; ++profile) {
            GridMap grid;
            const auto path = dir / (std::to_string(profile) + ".map.test-fixture");
            require(grid.loadData(path.c_str()),"original GridMap::loadData authored bytes"); grid.SetFullyLoaded();
            for (float x : {0.f,30.f,60.f,150.f,200.f,230.f,260.f,300.f,15.f})
                for (float z : {5.899f,5.9f,5.901f,6.f,std::nextafter(7.f,-INFINITY),7.f,std::nextafter(7.f,INFINITY),9.f,11.f})
                    sample(store,profile,"internal_" + std::to_string(int(x)) + "_z" + std::to_string(bits(z)),worldPoint({x,0,z}),grid);
            sample(store,profile,"rotated_u16_zero",worldPoint(place(loaded.iTreeValues[5],{0,0,7})),grid);
            sample(store,profile,"scaled_zero_area",worldPoint(place(loaded.iTreeValues[6],{0,0,7})),grid);
            sample(store,profile,"negative_zero",worldPoint({0,0,-0.f}),grid);
            if (profile == 15 || profile == 19) {
                // Every AREA cell, and both neighboring world floats at each internal edge.
                for (int x = 0; x < 16; ++x) for (int y = 0; y < 16; ++y)
                    sample(store,profile,"cell_" + std::to_string(x) + "_" + std::to_string(y),
                           {(32.f - (float(x) + .5f) / 16.f) * SIZE_OF_GRIDS,
                            (32.f - (float(y) + .5f) / 16.f) * SIZE_OF_GRIDS,7.f},grid);
                for (int edge = 1; edge < 16; ++edge) for (int axis = 0; axis < 2; ++axis) {
                    const float value = (32.f - float(edge) / 16.f) * SIZE_OF_GRIDS;
                    for (float coordinate : {std::nextafter(value,-INFINITY),value,std::nextafter(value,INFINITY)}) {
                        Vector3 world((32.f - .5f / 16.f) * SIZE_OF_GRIDS,(32.f - .5f / 16.f) * SIZE_OF_GRIDS,7.f);
                        world[axis] = coordinate;
                        sample(store,profile,"edge_" + std::to_string(edge) + "_" + std::to_string(axis) + "_" + std::to_string(bits(coordinate)),world,grid);
                    }
                }
            }
        }
    }
    out.flush(); require(bool(out),"write source TerrainInfo area expectations");
    if (dataset == "scopedarea/reverse")
        std::printf("TerrainInfo area oracle: %zu queries, %zu raw hits rejected by original terrain occlusion\n",terrainAreaCount,terrainRejected);
}
