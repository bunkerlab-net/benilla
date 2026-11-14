// SPDX-License-Identifier: GPL-3.0-or-later
// Authored selection/scheduling harness for the COMPLETE pinned GetHeightStatic.
// cmangos/mangos-classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// The VMap callback is explicitly SCRIPTED, not geometry or a real ray traversal.
// GetGrid supplies an authored flat GridMap or nullptr, not a real tile loader.
// Actual loaded map bytes and VMap ray results are proved separately.
#include <array>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <initializer_list>
#include <limits>
using uint32 = uint32_t;
// PINNED_HEIGHT_CONSTANTS
struct GridMap {
    // PINNED_GRIDMAP_FIELDS
};
struct Call { float z = 0.f, range = 0.f; };
struct ScriptedVMap {
    std::array<float, 4> replies{};
    std::array<Call, 4> calls{};
    size_t count = 0;
    bool isHeightCalcEnabled() const { return true; }
    float getHeight(uint32 map, float x, float y, float z, float range) {
        if (map != 123 || x != 3.f || y != 7.f || count == calls.size()) std::abort();
        calls[count] = {z, range};
        return replies[count++];
    }
};
struct TerrainInfo {
    GridMap grid{};
    bool mapPresent = true;
    ScriptedVMap* m_vmgr;
    uint32 GetMapId() const { return 123; }
    GridMap* GetGrid(float x, float y) {
        if (x != 3.f || y != 7.f) std::abort();
        return mapPresent ? &grid : nullptr;
    }
    float GetHeightStatic(float, float, float, bool, float) const;
};
// PINNED_UPSTREAM_METHODS
static uint32 bits(float value) { uint32 out; std::memcpy(&out, &value, 4); return out; }
static size_t caseNumber = 0;
static constexpr float MISS = VMAP_INVALID_HEIGHT_VALUE;
static void sample(const char* label, float map, float z, float search,
                   std::initializer_list<float> replies = {}, bool mapPresent = true) {
    ScriptedVMap vmap{};
    vmap.replies.fill(MISS);
    size_t i = 0;
    for (float reply : replies) {
        if (i == vmap.replies.size()) std::abort();
        vmap.replies[i++] = reply;
    }
    TerrainInfo terrain{};
    terrain.grid.m_gridHeight = map;
    terrain.grid.m_gridGetHeight = &GridMap::getHeightFromFlat;
    terrain.mapPresent = mapPresent;
    terrain.m_vmgr = &vmap;
    float result = terrain.GetHeightStatic(3.f, 7.f, z, true, search);
    std::printf("%zu\t%s\t%d\t%08x\t%08x\t%08x", caseNumber++, label,
                int(mapPresent), bits(map), bits(z), bits(search));
    for (float reply : vmap.replies) std::printf("\t%08x", bits(reply));
    std::printf("\t%zu", vmap.count);
    for (const auto& call : vmap.calls) std::printf("\t%08x\t%08x", bits(call.z), bits(call.range));
    std::printf("\t%08x\n", bits(result));
}
static std::array<float, 3> adjacent(float value) {
    return {std::nextafter(value, -INFINITY), value, std::nextafter(value, INFINITY)};
}
int main() {
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559, "binary32 required");
    if (std::fesetround(FE_TONEAREST)) return 1;
    std::puts("# Complete TerrainInfo::GetHeightStatic; cmangos 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f");
    std::puts("# Scope: selection and query schedule ONLY. VMap responses are ordered authored scripts, NOT geometry.");
    std::puts("# Fixed useVmaps=true, isHeightCalcEnabled=true; GetGrid returns source flat-height dispatch or nullptr.");
    std::puts("# No liquid, map loading/coordinates, real VMap ray traversal, support, or movement claim.");
    std::puts("# All *_bits are hexadecimal binary32; case/map_present/call_count are decimal. Unused calls are +0,+0.");
    std::puts("# Scripts contain four replies; only call_count entries are consumed. Other replies intentionally remain unused.");
    std::puts("# Selector-only vectors include negative ranges, sentinel heights and unclamped ranges beyond public query bounds.");
    std::puts("# Map absence supplies source VMAP_INVALID_HEIGHT_VALUE; provided map_height_bits is ignored when map_present=0.");
    std::puts("# case\tlabel\tmap_present\tmap_height_bits\tz_bits\tsearch_bits\treply0_bits\treply1_bits\treply2_bits\treply3_bits\tcall_count\tcall0_z_bits\tcall0_range_bits\tcall1_z_bits\tcall1_range_bits\tcall2_z_bits\tcall2_range_bits\tcall3_z_bits\tcall3_range_bits\tresult_height_bits");

    // A hit stops at each possible stage; later replies must not be queried.
    for (float search : {DEFAULT_HEIGHT_SEARCH, DEFAULT_WATER_SEARCH}) {
        sample("initial_hit", 0.f, 20.f, search, {5.f, 100.f, 200.f, 300.f});
        sample("fallback_hit", 0.f, 20.f, search, {MISS, 5.f, 200.f, 300.f});
        sample("upward_hit", 100.f, 0.f, search, {MISS, MISS, 20.f, 300.f});
        sample("near_terrain_hit", 100.f, 0.f, search, {MISS, MISS, MISS, 101.f});
        sample("near_terrain_without_upward", 20.f, 0.f, search, {MISS, MISS, 19.f, 300.f});
        sample("all_four_miss", 100.f, 0.f, search);
        sample("all_three_miss", 20.f, 0.f, search);
        sample("all_two_miss", 0.f, 20.f, search);
        sample("missing_map_hit", 17.f, 20.f, search, {5.f}, false);
        sample("missing_map_fallback", 17.f, 20.f, search, {MISS, 5.f}, false);
        sample("missing_map_miss", 17.f, 20.f, search, {}, false);
    }
    // Compare z2-map against search exactly, including source binary32 cancellation.
    for (float search : {0.f, DEFAULT_HEIGHT_SEARCH, DEFAULT_WATER_SEARCH, 10000.f})
        for (float z : adjacent(search - 2.f)) {
            sample("extension_boundary", 0.f, z, search, {5.f});
            sample("extension_boundary_fallback", 0.f, z, search);
        }
    for (float map : adjacent(-48.f))
        sample("extension_map_boundary", map, 0.f, DEFAULT_WATER_SEARCH, {0.f});
    for (float z : {48.f, 49.f, 100.f, 10000.f, 100000.f})
        sample("extended_unclamped_range", 0.f, z, DEFAULT_WATER_SEARCH);
    for (float map : adjacent(INVALID_HEIGHT))
        for (float z : {-100000.f, 0.f}) {
            sample("extension_valid_map_threshold", map, z, DEFAULT_HEIGHT_SEARCH, {1.f});
            sample("invalid_map_preserved_on_miss", map, z, DEFAULT_HEIGHT_SEARCH);
        }

    // The upward branch uses strict distance >30 and precedes the near-map query.
    for (float map : adjacent(30.f))
        for (float search : {DEFAULT_HEIGHT_SEARCH, DEFAULT_WATER_SEARCH}) {
            sample("upward_distance_boundary", map, -2.f, search);
            sample("upward_distance_boundary_hit", map, -2.f, search, {MISS, MISS, 29.f, 31.f});
        }
    for (float z : adjacent(-2.f)) {
        sample("upward_z_cancellation", 30.f, z, DEFAULT_WATER_SEARCH);
        sample("upward_z_cancellation_hit", 30.f, z, DEFAULT_WATER_SEARCH, {MISS, MISS, 29.f, 31.f});
    }
    for (float map : adjacent(10.f)) {
        sample("near_terrain_map_boundary", map, 8.f, DEFAULT_WATER_SEARCH);
        sample("near_terrain_map_boundary_hit", map, 8.f, DEFAULT_WATER_SEARCH, {MISS, MISS, 9.f});
    }
    for (float z : adjacent(8.f)) {
        sample("near_terrain_z_boundary", 10.f, z, DEFAULT_WATER_SEARCH);
        sample("near_terrain_z_boundary_hit", 10.f, z, DEFAULT_WATER_SEARCH, {MISS, MISS, 9.f});
    }
    for (float map : {MISS, INVALID_HEIGHT, -99999.f, 0.f, 10.f, 100000.f})
        sample("near_terrain_search_origin", map, map - 40.f, DEFAULT_WATER_SEARCH);

    // Selection is z<map OR vmap>map, with both comparisons strict.
    for (float map : {0.f, 10.f, -10.f, 1000.125f})
        for (float z : adjacent(map))
            for (float hit : adjacent(map))
                sample("selection_strict_boundaries", map, z, DEFAULT_HEIGHT_SEARCH, {hit});
    for (float map : {MISS, INVALID_HEIGHT, -99999.f, -100.f, 0.f, 100.f})
        for (float z : {-200.f, 0.f, 200.f}) {
            sample("selection_relative_surfaces", map, z, DEFAULT_WATER_SEARCH, {-50.f});
            sample("selection_relative_surfaces", map, z, DEFAULT_WATER_SEARCH, {50.f});
        }

    // Invalid return values use <= -100000, independently at every query stage.
    for (float miss : {MISS, std::nextafter(INVALID_HEIGHT, -INFINITY), INVALID_HEIGHT}) {
        sample("invalid_initial_then_hit", 100.f, 0.f, DEFAULT_WATER_SEARCH, {miss, 20.f});
        sample("invalid_fallback_then_hit", 100.f, 0.f, DEFAULT_WATER_SEARCH, {miss, miss, 20.f});
        sample("invalid_upward_then_hit", 100.f, 0.f, DEFAULT_WATER_SEARCH, {miss, miss, miss, 101.f});
        sample("invalid_all_stages", 100.f, 0.f, DEFAULT_WATER_SEARCH, {miss, miss, miss, miss});
    }
    const float firstValid = std::nextafter(INVALID_HEIGHT, INFINITY);
    for (float map : {MISS, INVALID_HEIGHT, firstValid, 0.f}) {
        sample("smallest_valid_initial", map, -500.f, DEFAULT_HEIGHT_SEARCH, {firstValid});
        sample("smallest_valid_fallback", map, -500.f, DEFAULT_HEIGHT_SEARCH, {MISS, firstValid});
    }
    sample("smallest_valid_upward", 100.f, 0.f, DEFAULT_WATER_SEARCH, {MISS, MISS, firstValid});
    sample("smallest_valid_near", 100.f, 0.f, DEFAULT_WATER_SEARCH, {MISS, MISS, MISS, firstValid});

    // No source clamp or abs is applied to the caller range; negating -0 yields +0.
    for (float search : {-50.f, -10.f, -0.f, 0.f, 1.f, 10000.f, 10001.f}) {
        sample("caller_range_preserved_upward", 100.f, 0.f, search);
        sample("caller_range_preserved_missing_map", 10.f, 0.f, search, {}, false);
    }
    for (float search : {-50.f, -10.f, -1.f})
        for (float map : {1.f, 2.f, 10.f}) {
            sample("negative_range_extension", map, 0.f, search, {1.f});
            sample("negative_range_extension_then_fallback", map, 0.f, search);
        }
    for (float map : {-0.f, 0.f})
        for (float z : {-0.f, 0.f})
            for (float hit : {-0.f, 0.f})
                sample("signed_zero_selection", map, z, DEFAULT_HEIGHT_SEARCH, {hit});
    for (float map : {-0.f, 0.f})
        sample("signed_zero_map_on_miss", map, 0.f, DEFAULT_HEIGHT_SEARCH);
    for (float z : {-100000.f, -16777216.f, 16777216.f, 100000.f})
        sample("source_float_addition_order", 100.f, z, DEFAULT_WATER_SEARCH);
}
