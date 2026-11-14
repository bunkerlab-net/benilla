// SPDX-License-Identifier: GPL-3.0-or-later
// Authored minimal typed host for unchanged pinned source bodies and blocks.
// Liquid/status inputs are authored observations, not a geometry implementation.
#include <cstdint>
#include <cstring>
#include <iostream>
#include <limits>
#include <sstream>
#include <stdexcept>
#include <string>
using uint8 = std::uint8_t;
using uint32 = std::uint32_t;
#include "LiquidDefinitions.inc"
#include "InhabitType.inc"
#include "MovementFlags.inc"
enum ExtraFlags {
#include "WalkInWater.inc"
};
enum UnitFlags {
#include "UnitSwimming.inc"
};

static float number(uint32 bits) {
    float value;
    std::memcpy(&value, &bits, sizeof(value));
    return value;
}
static uint32 bits(float value) {
    uint32 result;
    std::memcpy(&result, &value, sizeof(result));
    return result;
}
struct GridMap {};
struct VMapManager {
    bool enabled;
    mutable uint32 checks = 0;
    bool isHeightCalcEnabled() const { ++checks; return enabled; }
};
struct TerrainInfo {
    VMapManager* m_vmgr;
    bool grid_present;
    uint32 status;
    float level, depth;
    mutable uint32 grid_calls = 0, liquid_calls = 0, mask = 0, classification = 0;
    GridMap grid;
    TerrainInfo(VMapManager* vm, bool present, uint32 s, float l, float d)
        : m_vmgr(vm), grid_present(present), status(s), level(l), depth(d) {}
    GridMap* GetGrid(float, float) { ++grid_calls; return grid_present ? &grid : nullptr; }
    bool CanCheckLiquidLevel(float x, float y) const;
#include "IsSwimmableDeclaration.inc"
#include "LiquidStatusDeclaration.inc"
};
#include "CanCheckLiquidLevel.inc"
#include "IsSwimmable.inc"
GridMapLiquidStatus TerrainInfo::getLiquidStatus(float x, float y, float z,
                                                uint8 requested, GridMapLiquidData* data,
                                                float collisionHeight) const {
    if (x != 7.f || y != -11.f || z != 19.f || !data)
        throw std::runtime_error("source query did not forward exact respawn coordinates/data");
    ++liquid_calls;
    mask = requested;
    classification = bits(collisionHeight);
    if (status) *data = GridMapLiquidData{MAP_LIQUID_TYPE_WATER, 1, level, depth};
    return static_cast<GridMapLiquidStatus>(status);
}
struct MovementInfo {
    uint32 moveFlags;
#include "MovementAccessors.inc"
};
struct CreatureInfo { uint32 InhabitType, ExtraFlags; };
struct Position { float x = 7.f, y = -11.f, z = 19.f; };
struct Map {
    TerrainInfo* terrain;
    TerrainInfo* GetTerrain() { return terrain; }
};
struct Creature {
    MovementInfo m_movementInfo;
    Position m_respawnPos;
    CreatureInfo info;
    Map map;
    float height;
    Map* GetMap() { return &map; }
    float GetCollisionHeight() { return height; }
    void init() {
        const CreatureInfo* cinfo = &info;
#include "InitSwimming.inc"
    }
    uint32 unit_flags(uint32 unitFlags) {
#include "ProjectUnitFlags.inc"
        return unitFlags;
    }
};

int main() {
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
    std::string line;
    while (std::getline(std::cin, line)) {
        std::istringstream input(line);
        char op;
        input >> op;
        uint32 vmap, grid, status, level, depth, radius, external;
        uint32 habitat = 0, extra = 0, prior = 0, unit = 0;
        if (op == 'I') input >> habitat >> extra >> prior >> unit;
        input >> vmap >> grid >> status >> level >> depth >> radius >> external;
        std::string trailing;
        if (input.fail() || (input >> trailing) || vmap > 1 || grid > 1 || status > 15
            || external > 1 || (op != 'S' && op != 'I') || (op == 'I' && (habitat < 1 || habitat > 7)))
            throw std::runtime_error("invalid authored oracle input");
        VMapManager manager{vmap != 0};
        TerrainInfo terrain{&manager, grid != 0, status, number(level), number(depth)};
        uint32 result = 0, projected = 0;
        GridMapLiquidData output{123, 456, number(0x42f60000), number(0xc2f60000)};
        if (op == 'S') {
            result = terrain.IsSwimmable(7.f, -11.f, 19.f, number(radius), external ? &output : nullptr);
        } else {
            Creature creature{{prior}, {}, {habitat, extra}, {&terrain}, number(radius)};
            creature.init();
            result = creature.m_movementInfo.GetMovementFlags();
            projected = creature.unit_flags(unit);
        }
        std::cout << line << ' ' << result << ' ' << projected << ' '
                  << manager.checks << ' ' << terrain.grid_calls << ' '
                  << terrain.liquid_calls << ' ' << terrain.mask << ' '
                  << terrain.classification << ' ' << bits(output.level) << ' '
                  << bits(output.depth_level) << '\n';
    }
}
