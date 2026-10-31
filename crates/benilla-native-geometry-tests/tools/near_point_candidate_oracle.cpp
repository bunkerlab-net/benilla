// SPDX-License-Identifier: GPL-3.0-or-later
// Authored fixture plumbing. Markers receive verbatim, hash-checked upstream code.
#include <cfenv>
#include <cfloat>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <limits>

#ifdef __FAST_MATH__
#error This oracle requires strict IEEE arithmetic
#endif
static_assert(FLT_EVAL_METHOD == 0, "No excess floating-point precision");
static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559 &&
              FLT_RADIX == 2 && FLT_MANT_DIG == 24 && FLT_MAX_EXP == 128);
static_assert(sizeof(double) == 8 && std::numeric_limits<double>::is_iec559 &&
              DBL_MANT_DIG == 53 && DBL_MAX_EXP == 1024);
extern "C" float source_sin(float);
extern "C" float source_cos(float);

static uint32_t bits(float x) { uint32_t u; std::memcpy(&u, &x, 4); return u; }
static float value(uint32_t u) { float x; std::memcpy(&x, &u, 4); return x; }

// PINNED_MAP_MACROS
namespace MaNGOS {
// PINNED_NORMALIZE_MAP_COORD
}
struct WorldObject {
    static void GetNearPoint2dAt(const float, const float, float&, float&, float, float);
};
// Only symbol binding changes: arithmetic and source bodies stay byte-for-byte.
#define sinf source_sin
#define cosf source_cos
// PINNED_GET_NEAR_POINT
#undef sinf
#undef cosf

static bool finite(uint32_t word) { return (word & 0x7f800000u) != 0x7f800000u; }
static bool positive(uint32_t word) { return !(word >> 31) || word == 0x80000000u; }
static bool signed_angle(uint32_t word) { return finite(word) && (word & 0x7fffffffu) <= 0x40c90fdbu; }
static bool fallback_angle(uint32_t word) {
    const uint32_t limit = word >> 31 ? 0x40490fdbu : 0x4116cbe4u;
    return finite(word) && (word & 0x7fffffffu) <= limit;
}
static bool coordinate(uint32_t word) { return finite(word) && (word & 0x7fffffffu) <= 0x46fff000u; }
static bool output(uint32_t word) {
    uint8_t bytes[4] = {uint8_t(word), uint8_t(word >> 8), uint8_t(word >> 16), uint8_t(word >> 24)};
    return std::fwrite(bytes, 1, 4, stdout) == 4;
}
int main(int argc, char** argv) {
    if (argc != 2) return 1;
    const bool fallback = std::strcmp(argv[1], "fallback-trig") == 0 ||
                          std::strcmp(argv[1], "fallback-candidate") == 0;
    const bool trig = std::strcmp(argv[1], "trig") == 0 ||
                      std::strcmp(argv[1], "fallback-trig") == 0;
    if (!trig && !fallback && std::strcmp(argv[1], "candidate") != 0) return 1;
    if (std::fesetround(FE_TONEAREST) != 0 || std::fegetround() != FE_TONEAREST) return 2;
    volatile float subnormal = value(1), one = 1.0f;
    if (bits(subnormal * one) != 1 || bits(float(MAP_HALFSIZE - 0.5)) != 0x46855455u) return 2;
    const size_t size = trig ? 4 : 16;
    uint8_t bytes[16];
    for (;;) {
        const size_t got = std::fread(bytes, 1, size, stdin);
        if (got == 0) return std::ferror(stdin) ? 5 : 0;
        if (got != size) return 6;
        uint32_t words[4] = {};
        for (size_t i = 0; i < size / 4; ++i)
            words[i] = uint32_t(bytes[i * 4]) | uint32_t(bytes[i * 4 + 1]) << 8 |
                       uint32_t(bytes[i * 4 + 2]) << 16 | uint32_t(bytes[i * 4 + 3]) << 24;
        if (trig) {
            if (!(fallback ? fallback_angle(words[0]) : signed_angle(words[0]))) return 3;
            if (!output(words[0]) || !output(bits(source_sin(value(words[0])))) ||
                !output(bits(source_cos(value(words[0]))))) return 5;
        } else {
            if (!coordinate(words[0]) || !coordinate(words[1]) || !finite(words[2]) ||
                !positive(words[2]) || !(fallback ? fallback_angle(words[3]) :
                signed_angle(words[3]) && positive(words[3]))) return 4;
            float x, y;
            WorldObject::GetNearPoint2dAt(value(words[0]), value(words[1]), x, y, value(words[2]), value(words[3]));
            for (uint32_t word : words) if (!output(word)) return 5;
            if (!output(bits(x)) || !output(bits(y))) return 5;
        }
    }
}
