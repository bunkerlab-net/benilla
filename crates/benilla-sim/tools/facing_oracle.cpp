// SPDX-License-Identifier: GPL-3.0-or-later
// Authored harness. The generator inserts unchanged CMaNGOS source bodies.

#include <cmath>
#include <math.h>
#include <cfloat>
#include <type_traits>
#include <cfenv>
#include <cfloat>
#include <cstdint>
#include <cstring>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <limits>


static float from_bits(uint32_t u) { float f; std::memcpy(&f, &u, 4); return f; }
static uint32_t bits(float f) { uint32_t u; std::memcpy(&u, &f, 4); return u; }
// This substitution only binds std::atan2(float,float) to the pinned source.
// fmod is the source's ordinary float overload, used solely in this oracle.
namespace oracle_math {
static float atan2(float y, float x) { return ::atan2f(y, x); }
static float fmod(float x, float y) { return std::fmod(x, y); }
}
struct Log { void outError(const char*, const char*) { std::abort(); } } sLog;
PI_DEFINITION
#define std oracle_math
struct MapManager { MAP_BODY };
#undef std
struct WorldObject {
    float x, y;
    float GetPositionX() const { return x; }
    float GetPositionY() const { return y; }
    std::string GetGuidStr() const { return "oracle"; }
    static float GetAngleAt(float, float, float, float);
    float GetAngleAt(float, float, const WorldObject*) const;
    bool HasInArcAt(float, float, float, const WorldObject*, float) const;
};
#define std oracle_math
FUNCTIONS
#undef std
static uint32_t next(uint32_t& s) { s ^= s << 13; s ^= s >> 17; s ^= s << 5; return s; }
int main() {
    static_assert(sizeof(float) == 4 && sizeof(double) == 8);
    static_assert(FLT_EVAL_METHOD == 0, "source profile forbids excess precision");
    static_assert(std::is_same_v<decltype(std::atan2(1.f, 1.f)), float>);
    static_assert(std::is_same_v<decltype(atan2(1.f, 1.f)), float>);
    static_assert(FLT_EVAL_METHOD == 0);
    static_assert(std::numeric_limits<float>::is_iec559);
    static_assert(std::numeric_limits<double>::is_iec559);
    if (std::fesetround(FE_TONEAREST) != 0) return 2;
    if (bits(M_PI_F) != 0x40490fdb || bits(2.0f * M_PI_F) != 0x40c90fdb) return 2;
    char kind[16];
    while (std::scanf("%15s", kind) == 1) {
        uint32_t a[7] = {};
        unsigned count = std::strcmp(kind,"atan2") == 0 ? 2 :
            std::strcmp(kind,"normalize") == 0 ? 1 :
            std::strcmp(kind,"bearing") == 0 ? 4 : 7;
        for (unsigned i = 0; i < count; ++i)
            if (std::scanf("%x", &a[i]) != 1) return 4;
        uint32_t answer;
        if (std::strcmp(kind,"atan2") == 0)
            answer = bits(::atan2f(from_bits(a[0]), from_bits(a[1])));
        else if (std::strcmp(kind,"normalize") == 0)
            answer = bits(MapManager::NormalizeOrientation(from_bits(a[0])));
        else if (std::strcmp(kind,"bearing") == 0)
            answer = bits(WorldObject::GetAngleAt(from_bits(a[0]), from_bits(a[1]),
                                                 from_bits(a[2]), from_bits(a[3])));
        else {
            WorldObject origin{from_bits(a[0]), from_bits(a[1])};
            WorldObject target{from_bits(a[3]), from_bits(a[4])};
            answer = origin.HasInArcAt(origin.x, origin.y, from_bits(a[2]),
                                      a[6] ? &origin : &target, from_bits(a[5]));
        }
        std::printf("%08x\n", answer);
    }
}
