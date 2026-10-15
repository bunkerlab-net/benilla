// SPDX-License-Identifier: GPL-3.0-or-later
// Authored scalar callback observer. Inserted CMaNGOS methods remain unchanged.
#include <array>
#include <bit>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <limits>
#include <string>
#include <vector>
#ifdef ENABLE_PLAYERBOTS
#error This observer requires the original non-PLAYERBOTS source branch
#endif
using uint8 = std::uint8_t;
using uint16 = std::uint16_t;
using uint32 = std::uint32_t;
// PINNED_DECLARATIONS

static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
static_assert(std::endian::native == std::endian::little);
static_assert(NAV_GROUND == 1 && NAV_GROUND_STEEP == 2 && NAV_WATER == 4 && NAV_MAGMA_SLIME == 8);
static_assert(MAP_ALL_LIQUIDS == 15 && (MAP_LIQUID_TYPE_WATER | MAP_LIQUID_TYPE_OCEAN) == 10);
static_assert(LIQUID_MAP_NO_WATER == 0 && LIQUID_MAP_ABOVE_WATER == 1 &&
              LIQUID_MAP_WATER_WALK == 2 && LIQUID_MAP_IN_WATER == 4 && LIQUID_MAP_UNDER_WATER == 8);
[[noreturn]] static void fail(const char* reason) { std::fprintf(stderr, "%s\n", reason); std::exit(2); }
struct Input {
    uint32 walk, swim, height, grid;
    std::array<uint32, 3> position;
    std::array<uint32, 3> status, type;
};
struct Observation {
    std::string trace;
    std::vector<uint32> masks;
    unsigned include_calls = 0, exclude_calls = 0, read_calls = 0, height_calls = 0, grid_calls = 0;
    uint16 initial_include = 0;
};
static Input input;
static Observation observation;
static void position(float x, float y, float z) {
    if (std::bit_cast<uint32>(x) != input.position[0] ||
        std::bit_cast<uint32>(y) != input.position[1] ||
        std::bit_cast<uint32>(z) != input.position[2]) fail("liquid position differs from actual owner");
}
struct VMapManager {
    bool isHeightCalcEnabled() const {
        observation.trace += 'H'; ++observation.height_calls;
        return input.height;
    }
};
struct GridMap {};
struct TerrainInfo {
    VMapManager manager;
    VMapManager* m_vmgr = &manager;
    GridMap grid;
    GridMap* GetGrid(float x, float y) {
        position(x, y, std::bit_cast<float>(input.position[2]));
        observation.trace += 'G'; ++observation.grid_calls;
        return input.grid ? &grid : nullptr;
    }
    GridMapLiquidStatus getLiquidStatus(float x, float y, float z, uint8 mask, GridMapLiquidData* data) const {
        position(x, y, z);
        if (!data || observation.masks.size() >= input.status.size()) fail("unexpected liquid callback");
        if (mask != MAP_ALL_LIQUIDS && mask != (MAP_LIQUID_TYPE_WATER | MAP_LIQUID_TYPE_OCEAN))
            fail("unexpected liquid mask");
        const auto index = observation.masks.size();
        observation.masks.push_back(mask); observation.trace += mask == MAP_ALL_LIQUIDS ? 'A' : 'O';
        *data = {input.type[index], 0, 23.0f, -7.0f};
        return static_cast<GridMapLiquidStatus>(input.status[index]);
    }
    bool CanCheckLiquidLevel(float x, float y) const;
    bool IsInWater(float x, float y, float z, GridMapLiquidData* data = nullptr) const;
    bool IsUnderWater(float x, float y, float z, float* pWaterZ = nullptr) const;
};
struct Unit {
    TerrainInfo terrain;
    TypeID GetTypeId() const { observation.trace += 'T'; return TYPEID_UNIT; }
    TerrainInfo* GetTerrain() const { return const_cast<TerrainInfo*>(&terrain); }
    float GetPositionX() const { return std::bit_cast<float>(input.position[0]); }
    float GetPositionY() const { return std::bit_cast<float>(input.position[1]); }
    float GetPositionZ() const { return std::bit_cast<float>(input.position[2]); }
    bool IsInWater() const;
    bool IsUnderwater() const;
};
struct Creature : Unit {
    // Explicit authored scalar predicate callbacks, not a source capability loader.
    bool CanWalk() const { observation.trace += 'W'; return input.walk; }
    bool CanSwim() const { observation.trace += 'S'; return input.swim; }
};
struct QueryFilter {
    uint16 include = 0xbeef, exclude = 0xface;
    void setIncludeFlags(uint16 flags) {
        observation.trace += 'I';
        if (!observation.include_calls++) observation.initial_include = flags;
        include = flags;
    }
    void setExcludeFlags(uint16 flags) { observation.trace += 'E'; ++observation.exclude_calls; exclude = flags; }
    uint16 getIncludeFlags() const { observation.trace += 'R'; ++observation.read_calls; return include; }
};
struct PathFinder {
    Unit* m_sourceUnit;
    QueryFilter m_filter;
    void createFilter();
    void updateFilter();
    NavTerrainFlag getNavTerrain(float x, float y, float z) const;
};
// PINNED_METHODS

int main() {
    std::array<uint32, 13> raw;
    while (const auto count = std::fread(raw.data(), 1, sizeof(raw), stdin)) {
        if (count != sizeof(raw)) fail("partial input record");
        for (unsigned i = 0; i < 4; ++i) if (raw[i] > 1) fail("nonboolean scalar callback");
        for (unsigned i = 4; i < 7; ++i)
            if (!std::isfinite(std::bit_cast<float>(raw[i]))) fail("nonfinite owner position");
        for (unsigned i = 7; i < 13; i += 2)
            if (raw[i] > 15 || raw[i + 1] > 63) fail("liquid scalar outside observation domain");
        input = {raw[0], raw[1], raw[2], raw[3], {raw[4], raw[5], raw[6]},
                 {raw[7], raw[9], raw[11]}, {raw[8], raw[10], raw[12]}};
        observation = {};
        Creature creature;
        PathFinder path{&creature, {}};
        path.createFilter();
        std::printf("%u\t%u\t%u\t%u\t%u\t%u\t%u\t%u\t%zu\t",
                    path.m_filter.include, path.m_filter.exclude, observation.initial_include,
                    observation.include_calls, observation.exclude_calls, observation.read_calls,
                    observation.height_calls, observation.grid_calls, observation.masks.size());
        if (observation.masks.empty()) std::printf("-");
        for (unsigned i = 0; i < observation.masks.size(); ++i)
            std::printf("%s%u", i ? "," : "", observation.masks[i]);
        std::printf("\t%s\n", observation.trace.c_str());
    }
    if (std::ferror(stdin) || !std::feof(stdin)) fail("input read failure");
}
