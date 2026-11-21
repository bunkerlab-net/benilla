// SPDX-License-Identifier: GPL-3.0-or-later
// Authored observations around complete verbatim source bodies, not product queries.
#include <algorithm>
#include <array>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
using uint8 = uint8_t;
using uint32 = uint32_t;
// PINNED_CONSTANTS
// PINNED_LIQUID_TYPES
struct TerrainInfo {
    float ground, level;
    GridMapLiquidStatus status;
    bool can_check;
    mutable unsigned static_count = 0, liquid_count = 0;
    mutable float search = 0.f, liquid_z = 0.f, classification_height = 0.f;
    mutable uint8 mask = 0;
    float GetHeightStatic(float x, float y, float z, bool use_vmap, float range) const {
        if (++static_count != 1 || !use_vmap || x != 3.f || y != 7.f) std::abort();
        search = range;
        return ground;
    }
    bool CanCheckLiquidLevel(float x, float y) const {
        if (x != 3.f || y != 7.f) std::abort();
        return can_check;
    }
    // PINNED_LIQUID_DECLARATION
    float GetWaterOrGroundLevel(float, float, float, float&, bool, float) const;
};
GridMapLiquidStatus TerrainInfo::getLiquidStatus(float x, float y, float z, uint8 required, GridMapLiquidData* data, float height) const {
    if (++liquid_count != 1 || x != 3.f || y != 7.f) std::abort();
    liquid_z = z; mask = required; classification_height = height;
    data->level = level;
    data->depth_level = 987.f; // GetWaterOrGroundLevel must compare against groundZ.
    return status;
}
struct Dynamic {
    mutable unsigned count = 0;
    mutable float z = 0.f, range = 0.f;
    float getHeight(float x, float y, float start, float distance) const {
        if (++count != 1 || x != 3.f || y != 7.f) std::abort();
        z = start; range = distance;
        return -std::numeric_limits<float>::infinity();
    }
};
struct Map {
    TerrainInfo* m_TerrainData;
    Dynamic m_dyn_tree;
    TerrainInfo* GetTerrain() const { return m_TerrainData; }
    float GetHeight(float, float, float, bool = false) const;
};
static constexpr int SPELL_AURA_WATER_WALK = 0;
struct Unit {
    Map* map;
    bool water_walk;
    float collision_height;
    Map* GetMap() const { return map; }
    bool CanFly() const { return false; }
    bool CanSwim() const { return true; }
    bool HasAuraType(int type) const { if (type != SPELL_AURA_WATER_WALK) std::abort(); return water_walk; }
    float GetCollisionHeight() const { return collision_height; }
    void UpdateAllowedPositionZ(float, float, float&, Map* = nullptr) const;
};
// PINNED_METHODS
static uint32 bits(float value) { uint32 out; std::memcpy(&out, &value, 4); return out; }
static float value(uint32 raw) { float out; std::memcpy(&out, &raw, 4); return out; }
static unsigned id = 0;
static void sample(float z, float ground, float level, unsigned status, float height, bool water_walk, bool can_check) {
    TerrainInfo terrain{ground, level, static_cast<GridMapLiquidStatus>(status), can_check};
    Map map{&terrain, {}};
    Unit unit{&map, water_walk, height};
    float allowed = z;
    unit.UpdateAllowedPositionZ(3.f, 7.f, allowed);
    if (terrain.static_count != 1 || map.m_dyn_tree.count != 1 || terrain.search != 50.f) std::abort();
    if (terrain.liquid_count != unsigned(can_check)) std::abort();
    // Independent invocation observes the intermediate maxZ from the complete source method.
    TerrainInfo max_terrain{ground, level, static_cast<GridMapLiquidStatus>(status), can_check};
    float ground_copy = ground;
    float max_z = max_terrain.GetWaterOrGroundLevel(3.f, 7.f, z, ground_copy, !water_walk, height);
    std::printf("%u\t%08x\t%08x\t%08x\t%u\t%08x\t%u\t%u\t%08x\t%08x\t%08x\t%u\t%08x\t%u\t%08x\t%08x\t%08x\n",
        id++, bits(z), bits(ground), bits(level), status, bits(height), unsigned(water_walk), unsigned(can_check),
        bits(terrain.search), bits(map.m_dyn_tree.z), bits(map.m_dyn_tree.range), terrain.liquid_count,
        bits(terrain.liquid_z), unsigned(terrain.mask), bits(terrain.classification_height), bits(max_z), bits(allowed));
}
int main() {
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559, "binary32 required");
    if (std::fesetround(FE_TONEAREST)) return 1;
    std::puts("# Exact swimming-capable Unit::UpdateAllowedPositionZ, Map::GetHeight and TerrainInfo::GetWaterOrGroundLevel; cmangos 8ec338a1");
    std::puts("# Static/liquid/dynamic callback observations only. Wet, water-walk and disabled checking are scalar controls, not provider admission.");
    std::puts("# id z ground level status selected_height water_walk can_check search dynamic_z dynamic_range liquid_count liquid_z mask classification_height max_z allowed_z");
    for (float ground : {-200000.f, -100000.f, std::nextafter(-100000.f, INFINITY), -10.f, -0.f, 0.f, 5.f})
        for (float z : {-100000.f, -10.f, -0.f, 0.f, 2.f, 10.f})
            for (float height : {0.5f, 2.03128f, 3.75f})
                for (bool can_check : {false, true})
                    sample(z, ground, 99.f, 0, height, false, can_check);
    for (unsigned status : {1u, 2u, 4u, 8u, 15u})
        for (float height : {0.5f, 2.03128f, 3.75f})
            for (float level : {std::nextafter(height, -INFINITY), height, std::nextafter(height, INFINITY), 10.f})
                for (float z : {-1.f, -0.f, 0.f, 2.f, 10.f})
                    for (bool walk : {false, true})
                        sample(z, 0.f, level, status, height, walk, true);
    for (float ground : {-0.f, 0.f})
        for (float z : {-0.f, 0.f})
            sample(z, ground, 0.f, 0, 3.75f, false, true);
    for (float ground : {value(0xc7c35001), -100000.f, value(0xc7c34fff)})
        for (float z : {std::nextafter(ground, -INFINITY), ground, std::nextafter(ground, INFINITY)})
            sample(z, ground, 999.f, 0, 3.75f, false, true);
}
