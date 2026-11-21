// SPDX-License-Identifier: GPL-3.0-or-later
// Authored callback-observation harness for verbatim pinned Map and Unit bodies.
// Static replies are SCRIPTED: this is NOT a loaded TerrainInfo/product oracle.
// Dynamic mode is either SCRIPTED or the actual empty RegularGrid vertical path.
#include <algorithm>
#include <array>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <unordered_map>
using uint32 = uint32_t;
#include "G3D/BoundsTrait.h"
#include "RegularGrid.h"
// PINNED_HEIGHT_CONSTANTS

// These objects can never be reached: every source RegularGrid node is null.
// Abort instead of inventing model-intersection behavior if that invariant drifts.
struct GameObjectModel {
    bool intersectRay(const G3D::Ray&, float&, bool, bool) const { std::abort(); }
};
struct UnreachableNode {
    template<class Callback>
    void intersectRay(const G3D::Ray&, Callback&, float&, bool) { std::abort(); }
};
using EmptySourceGrid = RegularGrid2D<GameObjectModel, UnreachableNode>;
struct DynamicMapTree {
    mutable EmptySourceGrid impl;
    float getHeight(float, float, float, float) const;
};
// PINNED_DYNAMIC_METHODS

struct StaticCall {
    unsigned count = 0;
    float x = 0.f, y = 0.f, z = 0.f, search = 0.f;
    bool use_vmap = false;
};
struct DynamicCall {
    unsigned count = 0;
    float x = 0.f, y = 0.f, z = 0.f, range = 0.f, reply = 0.f;
};
struct TerrainInfo {
    float reply;
    mutable StaticCall call{};
    float GetHeightStatic(float x, float y, float z, bool use_vmap, float search) const {
        if (call.count) std::abort();
        call = {1, x, y, z, search, use_vmap};
        return reply;
    }
    float GetWaterOrGroundLevel(float, float, float, float&, bool, float) const {
        std::abort(); // Swimming is outside this ground-only Unit experiment.
    }
};
struct ObservedDynamic {
    bool empty;
    float scripted_reply;
    DynamicMapTree tree{};
    mutable DynamicCall call{};
    float getHeight(float x, float y, float z, float range) const {
        if (call.count) std::abort();
        float reply = empty ? tree.getHeight(x, y, z, range) : scripted_reply;
        call = {1, x, y, z, range, reply};
        return reply;
    }
};
struct Map {
    TerrainInfo* m_TerrainData;
    ObservedDynamic& m_dyn_tree;
    TerrainInfo* GetTerrain() const { return m_TerrainData; }
    float GetHeight(float, float, float, bool = false) const;
};
static constexpr int SPELL_AURA_WATER_WALK = 0; // Unreachable; no aura interpretation.
struct Unit {
    Map* owner;
    Map* GetMap() const { return owner; }
    bool CanFly() const { return false; }
    bool CanSwim() const { return false; }
    bool HasAuraType(int) const { std::abort(); }
    float GetCollisionHeight() const { std::abort(); }
    void UpdateAllowedPositionZ(float, float, float&, Map* = nullptr) const;
};
// PINNED_MAP_UNIT_METHODS

static uint32 bits(float value) { uint32 out; std::memcpy(&out, &value, 4); return out; }
static float value(uint32 raw) { float out; std::memcpy(&out, &raw, 4); return out; }
static std::array<float, 3> adjacent(float input) {
    return {std::nextafter(input, -INFINITY), input, std::nextafter(input, INFINITY)};
}
static size_t case_number = 0;
static bool nonfinite_run = false;
static void sample(const char* label, const char* op, float z, float static_reply,
                   float dynamic_reply, bool swim = false, bool empty = false,
                   float x = 3.f, float y = 7.f, bool null_at_map = false) {
    bool finite = std::isfinite(x) && std::isfinite(y) && std::isfinite(z)
               && std::isfinite(static_reply) && (empty || std::isfinite(dynamic_reply));
    if (finite == nonfinite_run) return;
    bool bounded = std::abs(x) <= 100000.f && std::abs(y) <= 100000.f && std::abs(z) <= 100000.f;
    const char* domain = finite ? (bounded ? "finite" : "source_only_finite") : "source_only_nonfinite";
    TerrainInfo terrain{static_reply};
    ObservedDynamic dynamic{empty, dynamic_reply};
    Map map{&terrain, dynamic};
    TerrainInfo other_terrain{12345.f};
    ObservedDynamic other_dynamic{false, 54321.f};
    Map other_map{&other_terrain, other_dynamic};
    float result;
    if (std::strcmp(op, "map") == 0) {
        result = map.GetHeight(x, y, z, swim);
    } else {
        if (swim) std::abort();
        Unit unit{&map};
        result = z;
        unit.UpdateAllowedPositionZ(x, y, result, null_at_map ? nullptr : &other_map);
    }
    if (other_terrain.call.count || other_dynamic.call.count) std::abort();
    const auto& a = terrain.call;
    const auto& b = dynamic.call;
    if (a.count != 1 || b.count != 1 || !a.use_vmap) std::abort();
    if (empty && bits(b.reply) != 0xff800000u) std::abort();
    std::printf("%zu\t%s\t%s\t%s\t%s\t%08x\t%08x\t%08x\t%d\t%08x\t%08x\t%u\t%08x\t%08x\t%08x\t%d\t%08x\t%u\t%08x\t%08x\t%08x\t%08x\t%08x\n",
                case_number++, label, op, domain, empty ? "empty" : "scripted",
                bits(x), bits(y), bits(z), int(swim), bits(static_reply), bits(b.reply),
                a.count, bits(a.x), bits(a.y), bits(a.z), int(a.use_vmap), bits(a.search),
                b.count, bits(b.x), bits(b.y), bits(b.z), bits(b.range), bits(result));
}
static void both(const char* label, float z, float static_reply, float dynamic_reply,
                 bool empty = false, float x = 3.f, float y = 7.f) {
    sample(label, "map", z, static_reply, dynamic_reply, false, empty, x, y);
    sample(label, "ground", z, static_reply, dynamic_reply, false, empty, x, y);
}
int main(int argc, char** argv) {
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559, "binary32 required");
    if (std::fesetround(FE_TONEAREST)) return 1;
    if (argc != 2 || (std::strcmp(argv[1], "finite") && std::strcmp(argv[1], "nonfinite"))) return 2;
    nonfinite_run = std::strcmp(argv[1], "nonfinite") == 0;
    std::puts("# Verbatim Map::GetHeight and Unit::UpdateAllowedPositionZ; cmangos 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f");
    std::puts("# Static replies are authored callbacks, NOT loaded terrain/VMap products or real-world geometry.");
    std::puts("# Dynamic scripted replies isolate source operand order; empty runs actual source RegularGrid/G3D vertical empty-node traversal.");
    std::puts("# Ground op fixes CanFly=false and CanSwim=false. Non-null unrelated atMap is ignored by source; explicit null controls also run.");
    std::puts("# Finite callback observations are not public-provider admission claims. Source-only nonfinite rows are in a separate fixture.");
    std::puts("# empty dynamic replies are -infinity, including in finite-domain rows; no nonfinite input is admitted by that fact.");
    std::puts("# All *_bits are hexadecimal binary32; case, swim, counts and static_use_vmap are decimal.");
    std::puts("# result_bits is Map height for map op; final input/output Z for ground op. Static-height validity is strict > -100000.");
    std::puts("# case\tlabel\top\tdomain\tdynamic_mode\tx_bits\ty_bits\tz_bits\tswim\tstatic_reply_bits\tdynamic_reply_bits\tstatic_call_count\tstatic_x_bits\tstatic_y_bits\tstatic_z_bits\tstatic_use_vmap\tstatic_search_bits\tdynamic_call_count\tdynamic_x_bits\tdynamic_y_bits\tdynamic_z_bits\tdynamic_range_bits\tresult_bits");

    for (float height : {-200000.f, -100001.f, -100000.f, -99999.f, -10.f, -0.f, 0.f, 5.f, 100.f}) {
        for (float z : {-100000.f, -10.f, -0.f, 0.f, 2.f, 100.f, 100000.f}) {
            both("empty_grid_height_and_ground", z, height, 0.f, true);
            for (float dynamic : {-200000.f, -100000.f, -10.f, -0.f, 0.f, 5.f, 100.f})
                both("scripted_operand_order", z, height, dynamic);
        }
    }
    // Query arguments are generated even for invalid static sentinel heights.
    for (float height : adjacent(INVALID_HEIGHT))
        for (float z : adjacent(height)) {
            both("invalid_threshold_adjacent", z, height, 0.f, true);
            both("invalid_threshold_selected_dynamic", z, -200000.f, height);
        }
    // std::max must return static (the first operand) on equal signed zero.
    // Unit's two strict comparisons must retain the INPUT z bits on equality.
    for (float height : {-0.f, 0.f})
        for (float z : {-0.f, 0.f})
            for (float dynamic : {-0.f, 0.f})
                both("signed_zero_equal_operands_and_z", z, height, dynamic);
    for (float height : {-10.f, -0.f, 0.f, 10.f})
        for (float z : adjacent(height))
            for (float dynamic : adjacent(height))
                both("adjacent_selection_and_clamp", z, height, dynamic);
    // First add to the larger selected value, then subtract staticHeight.
    for (float pivot : {-33554432.f, -16777216.f, -100000.f, -2.f, -0.f, 0.f, 2.f, 100000.f, 16777216.f, 33554432.f})
        for (float height : adjacent(pivot))
            for (float z : adjacent(pivot))
                both("binary32_ordered_dynamic_arguments", z, height, 0.f, true);
    // Authored finite coordinates where (2 + z) - static differs by one ULP
    // from the forbidden reassociation 2 + (z - static), across the 4096 edge.
    for (float height : adjacent(value(0x449cfd56u)))
        for (float z : adjacent(value(0x457feb55u)))
            both("bounded_nonassociative_dynamic_arguments", z, height, 0.f, true);
    for (float height : {-200000.f, -100000.f, 0.f, 10.f, 100.f})
        for (float z : {-2.f, 0.f, 10.f, 100.f})
            for (bool swim : {false, true})
                sample("map_swim_static_search", "map", z, height, 0.f, swim, true);
    // Same exact source null-atMap initialization; no water/flying branch.
    for (float height : {-200000.f, -100000.f, 0.f, 10.f})
        sample("ground_null_at_map", "ground", 5.f, height, 0.f, false, true, 3.f, 7.f, true);
    // Actual source cell computation at empty-grid edges; finite casts only.
    for (float x : {-100000.f, -17066.666f, -0.f, 0.f, 17066.666f, 100000.f})
        for (float y : {-100000.f, -0.f, 0.f, 100000.f})
            both("empty_grid_coordinates", 5.f, 3.f, 0.f, true, x, y);

    if (!nonfinite_run) return 0;

    // Observations only. The production prerequisite rejects nonfinite input.
    const float nan = value(0x7fc12345u), negative_nan = value(0xffc54321u);
    for (float static_reply : {nan, negative_nan, -INFINITY, INFINITY, -200000.f, -0.f, 0.f, 10.f})
        for (float dynamic_reply : {nan, negative_nan, -INFINITY, INFINITY, -0.f, 0.f, 10.f})
            for (float z : {-0.f, 0.f, 5.f, nan, -INFINITY, INFINITY})
                both("nonfinite_operand_observation", z, static_reply, dynamic_reply);
    for (float static_reply : {nan, negative_nan, -INFINITY, INFINITY, 0.f})
        for (float z : {nan, -INFINITY, INFINITY, 0.f})
            both("nonfinite_empty_grid_observation", z, static_reply, 0.f, true);
    for (float x : {nan, -INFINITY, INFINITY})
        sample("nonfinite_coordinate_script_only", "map", 5.f, 3.f, 4.f, false, false, x, 7.f);
}
