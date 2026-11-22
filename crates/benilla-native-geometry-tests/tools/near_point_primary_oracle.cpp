// SPDX-License-Identifier: GPL-3.0-or-later
// Authored observations around complete, verbatim, hash-checked source methods.
// Initial XY, GetHeight and static/dynamic LOS return SCRIPTED scalars.
#include <array>
#include <cfenv>
#include <cfloat>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <string>
#include "ObjectPosSelector.h"

#ifdef __FAST_MATH__
#error Strict IEEE arithmetic is required
#endif
static_assert(FLT_EVAL_METHOD == 0 && sizeof(float) == 4 && FLT_RADIX == 2 &&
              FLT_MANT_DIG == 24 && std::numeric_limits<float>::is_iec559);
static_assert(sizeof(double) == 8 && DBL_MANT_DIG == 53);
// PINNED_CONSTANTS

static uint32_t bits(float x) { uint32_t u; std::memcpy(&u, &x, 4); return u; }
static float value(uint32_t u) { float x; std::memcpy(&x, &u, 4); return x; }
struct FallbackRequired {};
struct State {
    // x,y,z,distance,searcher radius,target radius,angle,candidate x/y,height,
    // actual target x/y/z,collision height; then detect/static LOS/dynamic LOS.
    std::array<uint32_t, 17> input{};
    unsigned xy_calls = 0, ground_calls = 0, static_calls = 0, dynamic_calls = 0;
    unsigned config_calls = 0, radius_calls = 0, fallback_calls = 0;
    std::array<float, 3> ground{};
    std::array<float, 6> los{};
    bool swim = false, ignore_m2 = false;
    std::string trace;
    float f(size_t n) const { return value(input[n]); }
};
static State* current;
enum { CONFIG_BOOL_DETECT_POS_COLLISION, SPELL_AURA_WATER_WALK };
struct World {
    bool getConfig(int key) const {
        if (key != CONFIG_BOOL_DETECT_POS_COLLISION) std::abort();
        ++current->config_calls;
        current->trace += 'C';
        return current->input[14] != 0;
    }
} sWorld;

namespace VMAP {
struct Manager {
    bool isInLineOfSight(unsigned id, float x, float y, float z,
                        float ox, float oy, float oz, bool ignore_m2) const {
        if (id != 17 || current->static_calls++) std::abort();
        current->trace += 'S';
        current->los = {x, y, z, ox, oy, oz};
        current->ignore_m2 = ignore_m2;
        return current->input[15] != 0;
    }
};
struct VMapFactory {
    static Manager* createOrGetVMapManager() { static Manager manager; return &manager; }
};
}
struct DynamicTree {
    bool isInLineOfSight(float x, float y, float z, float ox, float oy, float oz,
                        bool ignore_m2) const {
        if (current->dynamic_calls++ || current->static_calls != 1) std::abort();
        const std::array<float, 6> args{x, y, z, ox, oy, oz};
        for (size_t n = 0; n != args.size(); ++n)
            if (bits(args[n]) != bits(current->los[n])) std::abort();
        if (ignore_m2 != current->ignore_m2) std::abort();
        current->trace += 'D';
        return current->input[16] != 0;
    }
};
struct Terrain {
    float GetWaterOrGroundLevel(float, float, float, float&, bool, float) const {
        std::abort(); // Swimming is outside this experiment.
    }
};
struct Map {
    DynamicTree m_dyn_tree;
    unsigned GetId() const { return 17; }
    Terrain* GetTerrain() const { std::abort(); }
    float GetHeight(float x, float y, float z, bool swim = false) const {
        if (current->ground_calls++) std::abort();
        current->trace += 'H';
        current->ground = {x, y, z};
        current->swim = swim;
        return current->f(9);
    }
    bool IsInLineOfSight(float, float, float, float, float, float, bool) const;
};
struct WorldObject {
    Map* owner;
    explicit WorldObject(Map* map) : owner(map) {}
    virtual ~WorldObject() = default;
    Map* GetMap() const { return owner; }
    void GetPosition(float& x, float& y, float& z) const {
        current->trace += 'P';
        x = current->f(10); y = current->f(11); z = current->f(12);
    }
    float GetCollisionHeight() const { return current->f(13); }
    float GetObjectBoundingRadius() const {
        ++current->radius_calls;
        current->trace += 'R';
        return current->f(5);
    }
    static void GetNearPoint2dAt(float x, float y, float& ox, float& oy,
                               float distance, float angle) {
        if (current->xy_calls++ || bits(x) != current->input[0] ||
            bits(y) != current->input[1] || bits(distance) != current->input[3] ||
            bits(angle) != current->input[6]) std::abort();
        current->trace += 'I';
        ox = current->f(7); oy = current->f(8);
    }
    virtual void UpdateAllowedPositionZ(float, float, float&, Map* = nullptr) const {
        std::abort(); // All experiments supply the ground-only Unit searcher.
    }
    void UpdateGroundPositionZ(float, float, float&) const { std::abort(); }
    // PINNED_LOS_DECLARATION
    void GetNearPointAt(float, float, float, WorldObject const*, float&, float&, float&,
                        float, float, float, bool) const;
};
struct Unit : WorldObject {
    explicit Unit(Map* map) : WorldObject(map) {}
    bool CanFly() const { return false; }
    bool CanSwim() const { return false; }
    bool HasAuraType(int) const { std::abort(); }
    void UpdateAllowedPositionZ(float, float, float&, Map* = nullptr) const override;
};

// PINNED_SELECTOR_METHODS
// This authored sentinel is the boundary, not an implementation of fallback.
void ObjectPosSelector::InitializeAngle() {
    if (!m_UsedAreaLists[USED_POS_PLUS].empty() ||
        !m_UsedAreaLists[USED_POS_MINUS].empty()) std::abort();
    ++current->fallback_calls;
    current->trace += 'F';
    throw FallbackRequired{};
}
bool ObjectPosSelector::NextAngle(float&) { std::abort(); }
bool ObjectPosSelector::NextUsedAngle(float&) { std::abort(); }
// PINNED_MAP_UNIT_OBJECT_METHODS

static void word(uint32_t u) { std::printf("\t%08x", u); }
static bool read_record(State& s) {
    unsigned char bytes[68];
    size_t count = std::fread(bytes, 1, sizeof bytes, stdin);
    if (!count) {
        if (std::ferror(stdin)) std::exit(5);
        return false;
    }
    if (count != sizeof bytes) std::exit(6);
    for (size_t i = 0; i != s.input.size(); ++i)
        s.input[i] = uint32_t(bytes[4 * i]) | uint32_t(bytes[4 * i + 1]) << 8 |
                     uint32_t(bytes[4 * i + 2]) << 16 | uint32_t(bytes[4 * i + 3]) << 24;
    for (size_t i = 0; i != 14; ++i)
        if (!std::isfinite(s.f(i))) std::exit(3);
    for (size_t i = 14; i != 17; ++i)
        if (s.input[i] > 1) std::exit(4);
    for (size_t i : {3u, 4u, 5u, 13u})
        if (s.f(i) < 0) std::exit(3);
    return true;
}
int main() {
    if (std::fesetround(FE_TONEAREST) || std::fegetround() != FE_TONEAREST) return 2;
    volatile float tiny = value(1), one = 1.0f;
    if (bits(tiny * one) != 1) return 2;
    for (;;) {
        State state;
        if (!read_record(state)) return 0;
        current = &state;
        Map map;
        WorldObject target(&map);
        Unit searcher(&map);
        // Deliberately unrelated pre-call Z proves the complete source resets it.
        float x = -123.f, y = 456.f, z = -789.f;
        const char* outcome = state.input[14] ? "primary-accepted" : "collision-disabled";
        try {
            target.GetNearPointAt(state.f(0), state.f(1), state.f(2), &searcher,
                                  x, y, z, state.f(4), state.f(3), state.f(6), false);
        } catch (const FallbackRequired&) {
            outcome = "fallback-required";
        }
        if (state.xy_calls != 1 || state.config_calls != 1 || state.ground_calls != 1 ||
            state.fallback_calls > 1 || state.swim || state.ignore_m2) std::abort();
        std::printf("%s", outcome);
        for (float v : {x, y, z}) word(bits(v));
        std::printf("\t%u\t%u", state.xy_calls, state.ground_calls);
        for (float v : state.ground) word(bits(v));
        std::printf("\t%d\t%u\t%u", int(state.swim), state.static_calls, state.dynamic_calls);
        for (float v : state.los) word(bits(v));
        std::printf("\t%d\t%u\t%u\t%u\t%s\n", int(state.ignore_m2), state.config_calls,
                    state.radius_calls, state.fallback_calls, state.trace.c_str());
    }
}
