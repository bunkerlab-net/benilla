// SPDX-License-Identifier: GPL-3.0-or-later
// Authored scalar callbacks around unchanged, pinned CMaNGOS source bodies.
// Height/LOS replies are scripted scalars, never loaded-world terrain evidence.
#include <algorithm>
#include <array>
#include <cassert>
#include <cfenv>
#include <cfloat>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <string>
#include <type_traits>
#include <vector>
#include <xmmintrin.h>
#include "Maps/ObjectPosSelector.h"
#define MANGOS_ASSERT(x) assert(x)
// PINNED_CONSTANTS
// PINNED_MAP_MACROS
namespace MaNGOS {
// PINNED_NORMALIZE_MAP_COORD
}
static_assert(FLT_EVAL_METHOD == 0 && sizeof(float) == 4 && sizeof(double) == 8);
static_assert(FLT_RADIX == 2 && FLT_MANT_DIG == 24 && DBL_MANT_DIG == 53);
static_assert(std::numeric_limits<float>::is_iec559 && std::numeric_limits<double>::is_iec559);
static_assert(std::is_same_v<decltype(atan(1.0f)), double>);
#ifdef __FAST_MATH__
#error Strict IEEE arithmetic is required
#endif
static uint32_t bits(float x) { uint32_t u; std::memcpy(&u, &x, 4); return u; }
static float value(uint32_t u) { float x; std::memcpy(&x, &u, 4); return x; }
struct Reply { uint32_t ground, static_los, dynamic_los; };
struct Height {
    int xy_index = -1;
    uint32_t angle = 0;
    std::array<float, 3> query{};
    Reply reply{};
    uint32_t adjusted_z = 0;
    bool finalized = false, static_called = false, dynamic_called = false;
    std::array<float, 6> los{};
};
struct State {
    // pos XYZ, distance, searcher radius, target radius, angle, target XYZ,
    // collision height, detect; script length is a separate input word.
    std::array<uint32_t, 12> input{};
    std::vector<Reply> replies;
    std::vector<Height> heights;
    float* z = nullptr;
    unsigned xy_calls = 0, sin_calls = 0, static_calls = 0, dynamic_calls = 0;
    unsigned config_calls = 0, radius_calls = 0;
    uint32_t angle = 0;
    bool pending_xy = false;
    std::string trace;
    float f(size_t n) const { return value(input[n]); }
    void finish_height() {
        if (!heights.empty() && !heights.back().finalized) {
            heights.back().adjusted_z = bits(*z);
            heights.back().finalized = true;
        }
    }
};
static State* current;
extern "C" float source_sin(float);
extern "C" float source_cos(float);
// These bindings observe arguments and forward to the unchanged pinned glibc
// body. They never calculate an expected angle or candidate coordinate.
__attribute__((noinline)) static float traced_cos(float angle) {
    current->finish_height();
    if (current->pending_xy || current->xy_calls >= 629) std::abort();
    current->pending_xy = true;
    current->angle = bits(angle);
    ++current->xy_calls;
    current->trace += 'I';
    return source_cos(angle);
}
__attribute__((noinline)) static float traced_sin(float angle) {
    if (!current->pending_xy || bits(angle) != current->angle ||
        current->sin_calls + 1 != current->xy_calls) std::abort();
    ++current->sin_calls;
    return source_sin(angle);
}
enum { CONFIG_BOOL_DETECT_POS_COLLISION, SPELL_AURA_WATER_WALK };
struct World {
    bool getConfig(int key) const {
        if (key != CONFIG_BOOL_DETECT_POS_COLLISION) std::abort();
        ++current->config_calls;
        current->trace += 'C';
        return current->input[11] != 0;
    }
} sWorld;
namespace VMAP {
struct Manager {
    bool isInLineOfSight(unsigned id, float x, float y, float z,
                        float ox, float oy, float oz, bool ignore_m2) const {
        if (id != 17 || ignore_m2 || current->heights.empty()) std::abort();
        auto& height = current->heights.back();
        if (height.static_called) std::abort();
        height.static_called = true;
        height.los = {x, y, z, ox, oy, oz};
        ++current->static_calls;
        current->trace += 'S';
        return height.reply.static_los != 0;
    }
};
struct VMapFactory {
    static Manager* createOrGetVMapManager() { static Manager manager; return &manager; }
};
}
struct DynamicTree {
    bool isInLineOfSight(float x, float y, float z, float ox, float oy, float oz,
                        bool ignore_m2) const {
        auto& height = current->heights.back();
        if (!height.static_called || !height.reply.static_los || height.dynamic_called || ignore_m2)
            std::abort();
        const std::array<float, 6> arguments{x, y, z, ox, oy, oz};
        for (size_t i = 0; i != 6; ++i)
            if (bits(arguments[i]) != bits(height.los[i])) std::abort();
        height.dynamic_called = true;
        ++current->dynamic_calls;
        current->trace += 'D';
        return height.reply.dynamic_los != 0;
    }
};
struct Terrain {
    float GetWaterOrGroundLevel(float, float, float, float&, bool, float) const { std::abort(); }
};
struct Map {
    DynamicTree m_dyn_tree;
    unsigned GetId() const { return 17; }
    Terrain* GetTerrain() const { std::abort(); }
    float GetHeight(float x, float y, float z, bool swim = false) const {
        current->finish_height();
        if (swim || current->heights.size() >= 630) std::abort();
        Height height;
        if (current->pending_xy) {
            height.xy_index = static_cast<int>(current->xy_calls) - 1;
            height.angle = current->angle;
        }
        current->pending_xy = false;
        height.query = {x, y, z};
        height.reply = current->replies[std::min(current->heights.size(), current->replies.size() - 1)];
        current->heights.push_back(height);
        current->trace += 'H';
        return value(height.reply.ground);
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
        x = current->f(7); y = current->f(8); z = current->f(9);
    }
    float GetCollisionHeight() const { return current->f(10); }
    float GetObjectBoundingRadius() const {
        ++current->radius_calls;
        current->trace += 'R';
        return current->f(5);
    }
    __attribute__((noinline)) static void GetNearPoint2dAt(float, float, float&, float&, float, float);
    virtual void UpdateAllowedPositionZ(float, float, float&, Map* = nullptr) const { std::abort(); }
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
// PINNED_COMPLETE_SELECTOR_CPP
#define cosf traced_cos
#define sinf traced_sin
// PINNED_GET_NEAR_POINT_2D
#undef sinf
#undef cosf
// PINNED_MAP_UNIT_OBJECT_METHODS
static bool read_word(uint32_t& word, bool first = false) {
    unsigned char bytes[4];
    size_t count = std::fread(bytes, 1, sizeof bytes, stdin);
    if (!count && first) { if (std::ferror(stdin)) std::exit(5); return false; }
    if (count != sizeof bytes) std::exit(6);
    word = uint32_t(bytes[0]) | uint32_t(bytes[1]) << 8 |
           uint32_t(bytes[2]) << 16 | uint32_t(bytes[3]) << 24;
    return true;
}
static bool read_record(State& s) {
    if (!read_word(s.input[0], true)) return false;
    for (size_t i = 1; i != s.input.size(); ++i) read_word(s.input[i]);
    uint32_t length = 0;
    read_word(length);
    if (s.input[11] > 1 || !length || length > 630) std::exit(4);
    for (size_t i = 0; i != 11; ++i) if (!std::isfinite(s.f(i))) std::exit(3);
    for (size_t i : {3u, 4u, 5u, 10u}) if (s.f(i) < 0) std::exit(3);
    if (s.f(6) < 0 || (s.input[6] & 0x7fffffffu) > 0x40c90fdbu) std::exit(3);
    for (size_t i : {0u, 1u}) if ((s.input[i] & 0x7fffffffu) > 0x46fff000u) std::exit(3);
    for (uint32_t i = 0; i != length; ++i) {
        Reply reply{};
        read_word(reply.ground); read_word(reply.static_los); read_word(reply.dynamic_los);
        if (!std::isfinite(value(reply.ground))) std::exit(3);
        if (reply.static_los > 1 || reply.dynamic_los > 1) std::exit(4);
        s.replies.push_back(reply);
    }
    return true;
}
static void word(uint32_t u) { std::printf("\t%08x", u); }
int main() {
    if (std::fesetround(FE_TONEAREST) || std::fegetround() != FE_TONEAREST ||
        (_mm_getcsr() & (0x8040 | 0x6000))) return 2;
    for (;;) {
        State state;
        if (!read_record(state)) return 0;
        current = &state;
        Map map;
        WorldObject target(&map);
        Unit searcher(&map);
        float x = -123.f, y = 456.f, z = -789.f;
        state.z = &z;
        target.GetNearPointAt(state.f(0), state.f(1), state.f(2), &searcher, x, y, z,
                              state.f(4), state.f(3), state.f(6), false);
        state.finish_height();
        if (state.pending_xy || state.xy_calls != state.sin_calls || state.config_calls != 1 ||
            state.heights.empty()) std::abort();
        const char* outcome = !state.input[11] ? "collision-disabled" :
            state.heights.size() > state.xy_calls ? "exhausted" :
            state.xy_calls == 1 ? "primary-accepted" : "fallback-accepted";
        std::printf("S\t%s", outcome);
        for (float f : {x, y, z}) word(bits(f));
        std::printf("\t%u\t%zu\t%u\t%u\t%u\t%u\t%s\n", state.xy_calls, state.heights.size(),
                    state.static_calls, state.dynamic_calls, state.config_calls, state.radius_calls,
                    state.trace.c_str());
        for (size_t i = 0; i != state.heights.size(); ++i) {
            const Height& h = state.heights[i];
            std::printf("H\t%zu\t%d", i, h.xy_index);
            word(h.angle);
            for (float f : h.query) word(bits(f));
            word(h.reply.ground); word(h.adjusted_z);
            std::printf("\t%d\t%d\t%u\t%u", int(h.static_called), int(h.dynamic_called),
                        h.reply.static_los, h.reply.dynamic_los);
            for (float f : h.los) word(bits(f));
            std::printf("\t0\t0\n"); // verified swim=false, ignoreM2Model=false
        }
    }
}
