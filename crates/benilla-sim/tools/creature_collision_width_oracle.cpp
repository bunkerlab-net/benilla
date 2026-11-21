// SPDX-License-Identifier: GPL-3.0-or-later
// Authored synthetic host; no game rows or assets. Unchanged pinned CMaNGOS
// Unit::GetCollisionWidth and its default constant are inserted by the generator.
// Source: CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// Project contributors, GPL-2.0-or-later. This host is GPL-3.0-or-later.
#include <cassert>
#include <cfenv>
#include <cfloat>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <limits>
#include <xmmintrin.h>
#define MANGOS_ASSERT assert
using uint32 = uint32_t;
static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
static_assert(sizeof(uint32) == 4 && FLT_EVAL_METHOD == 0);
struct CreatureDisplayInfoEntry { uint32 ModelId; };
struct CreatureModelDataEntry { float CollisionWidth; };
struct Displays {
    CreatureDisplayInfoEntry row{51};
    CreatureDisplayInfoEntry const* LookupEntry(uint32 id) const { return id == 31 ? &row : nullptr; }
} sCreatureDisplayInfoStore;
struct Models {
    CreatureModelDataEntry row{0};
    CreatureModelDataEntry const* LookupEntry(uint32 id) const { return id == 51 ? &row : nullptr; }
} sCreatureModelDataStore;
struct Unit {
    float scale;
    float GetObjectScale() const { return scale; }
    uint32 GetNativeDisplayId() const { return 31; }
    float GetCollisionWidth() const;
};
// PINNED_CONSTANT
// PINNED_METHOD
uint32 bits(float value) { uint32 result; std::memcpy(&result, &value, sizeof result); return result; }
float number(uint32 value) { float result; std::memcpy(&result, &value, sizeof result); return result; }
void emit(uint32 scale, uint32 width) {
    Unit unit{number(scale)};
    sCreatureModelDataStore.row.CollisionWidth = number(width);
    // The raw product lets Rust identify explicit out-of-domain underflow/overflow
    // cases separately from the source's zero-only default branch.
    float product = unit.scale * sCreatureModelDataStore.row.CollisionWidth;
    std::cout << scale << ' ' << width << ' ' << bits(product) << ' '
              << bits(unit.GetCollisionWidth()) << '\n';
}
int main(int argc, char** argv) {
    if (std::fesetround(FE_TONEAREST) != 0) return 2;
    if (argc == 2) {
        if (std::strcmp(argv[1], "--reject-ftz") == 0) _mm_setcsr(_mm_getcsr() | 0x8000);
        else if (std::strcmp(argv[1], "--reject-daz") == 0) _mm_setcsr(_mm_getcsr() | 0x0040);
        else if (std::strcmp(argv[1], "--reject-round") == 0) {
            if (std::fesetround(FE_DOWNWARD) != 0) return 3;
        } else return 3;
    } else if (argc != 1) return 3;
    if (std::fegetround() != FE_TONEAREST || (_mm_getcsr() & (0x8040 | 0x6000)) != 0) return 2;
    volatile float smallest_normal = number(0x00800000);
    volatile float smallest_subnormal = number(1);
    volatile float half = 0.5f;
    volatile float one = 1.0f;
    if (bits(smallest_normal * half) != 0x00400000 || bits(smallest_subnormal * one) != 1) return 2;
    for (uint32 scale : {0x00800000u, 0x2edbe6ffu, 0x38d1b717u, 0x3dcccccdu,
                         0x3f000000u, 0x3f7fffffu, 0x3f800000u, 0x3f800001u,
                         0x3fa00000u, 0x3fe00000u, 0x40400000u, 0x41fa0000u,
                         0x49742400u}) {
        for (uint32 width : {0u, 0x80000000u, 0x00800000u, 0x80800000u,
                             0x38d1b717u, 0x3dcccccdu, 0x3f000000u,
                             0x3f027bb2u, 0x3f027bb3u, 0x3f027bb4u,
                             0x3f7fffffu, 0x3f800000u, 0x3f800001u,
                             0x3fa00000u, 0x3fe00000u, 0xbf800000u,
                             0xbf000000u, 0x49742400u, 0xc9742400u})
            emit(scale, width);
    }
    uint32 seed = 0x519034afu;
    for (uint32 i = 0; i < 2048; ++i) {
        seed = seed * 1664525u + 1013904223u;
        uint32 scale = 0x3c000000u + seed % 0x06000000u;
        seed = seed * 1664525u + 1013904223u;
        uint32 width = 0x3c000000u + seed % 0x06000000u;
        if (i & 1) width |= 0x80000000u;
        emit(scale, width);
    }
}
