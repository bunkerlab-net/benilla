// SPDX-License-Identifier: GPL-3.0-or-later
// Authored synthetic host; no game rows or assets. Unchanged pinned CMaNGOS
// Unit::GetCollisionHeight, Object::GetObjectScale, model declaration, DBC format
// and default constants are inserted unchanged by the generator. This is not the
// original translation unit, DBC loader, complete Unit or lifecycle implementation.
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
// PINNED_CONSTANTS
// PINNED_FORMAT
// PINNED_MODEL
struct CreatureDisplayInfoEntry { uint32 ModelId; };
struct Displays {
    CreatureDisplayInfoEntry row{51};
    CreatureDisplayInfoEntry const* LookupEntry(uint32 id) const { return id == 31 ? &row : nullptr; }
} sCreatureDisplayInfoStore;
struct Models {
    CreatureModelDataEntry row{51, 0, 7.0f, 11.0f, 0};
    CreatureModelDataEntry const* LookupEntry(uint32 id) const { return id == 51 ? &row : nullptr; }
} sCreatureModelDataStore;
constexpr uint32 OBJECT_FIELD_SCALE_X = 0;
struct Object {
    float m_floatValues[1]{};
// PINNED_GETTER
};
struct Unit : Object {
    uint32 GetNativeDisplayId() const { return 31; }
    uint32 GetDisplayId() const { return 99; }
    float GetCollisionHeight() const;
};
// PINNED_METHOD
uint32 bits(float value) { uint32 result; std::memcpy(&result, &value, sizeof result); return result; }
float number(uint32 value) { float result; std::memcpy(&result, &value, sizeof result); return result; }
void emit(uint32 scale, uint32 height) {
    Unit unit;
    unit.m_floatValues[OBJECT_FIELD_SCALE_X] = number(scale);
    sCreatureModelDataStore.row.CollisionHeight = number(height);
    // All scales are bounded positive effective initial scales. Raw heights also
    // include invalid SpellFloat inputs to distinguish refusal before arithmetic
    // from admitted-input products outside the supported result domain.
    float product = unit.GetObjectScale() * sCreatureModelDataStore.row.CollisionHeight;
    std::cout << scale << ' ' << height << ' ' << bits(product) << ' '
              << bits(unit.GetCollisionHeight()) << '\n';
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
    // The native getter repairs both stored zeros. The Plan receipt cannot retain
    // stored zero scale, so these controls are deliberately outside the fixture.
    for (uint32 stored : {0u, 0x80000000u}) {
        Unit unit;
        unit.m_floatValues[OBJECT_FIELD_SCALE_X] = number(stored);
        if (bits(unit.GetObjectScale()) != 0x3f800000u) return 2;
    }
    if (std::strcmp(CreatureModelDatafmt, "nixxfxxxxxxxxxff") != 0
        || std::strlen(CreatureModelDatafmt) != 16 || CreatureModelDatafmt[15] != 'f') return 2;
    if (bits(DEFAULT_COLLISION_HEIGHT) != 0x4002007eu) return 2;
    for (uint32 scale : {0x00800000u, 0x2edbe6ffu, 0x38d1b717u, 0x3dcccccdu,
                         0x3f000000u, 0x3f7fffffu, 0x3f800000u, 0x3f800001u,
                         0x3fa00000u, 0x3fe00000u, 0x40400000u, 0x41fa0000u,
                         0x49742400u}) {
        for (uint32 height : {0u, 0x80000000u, 0x00800000u, 0x80800000u,
                             1u, 0x80000001u, 0x007fffffu, 0x807fffffu,
                             0x38d1b717u, 0x3dcccccdu, 0x3f000000u,
                             0x4002007du, 0x4002007eu, 0x4002007fu,
                             0x3f7fffffu, 0x3f800000u, 0x3f800001u,
                             0x3fa00000u, 0x3fe00000u, 0xbf800000u,
                             0xbf000000u, 0x497423ffu, 0x49742400u,
                             0x49742401u, 0xc9742400u, 0x7f7fffffu, 0xff7fffffu})
            emit(scale, height);
    }
    // Adjacent representable values around the one-million result bound.
    for (uint32 height : {0x48f423ffu, 0x48f42400u, 0x48f42401u,
                         0xc8f423ffu, 0xc8f42400u, 0xc8f42401u})
        emit(0x40000000u, height);
    // Positive normal inputs yielding exact, rounded or flushed-to-zero underflow.
    for (uint32 height : {0x00800000u, 0x00800001u, 0x3f000000u})
        emit(0x00800000u, height);
    uint32 seed = 0x519034afu;
    for (uint32 i = 0; i < 2048; ++i) {
        seed = seed * 1664525u + 1013904223u;
        uint32 scale = 0x3c000000u + seed % 0x06000000u;
        seed = seed * 1664525u + 1013904223u;
        uint32 height = 0x3c000000u + seed % 0x06000000u;
        if (i & 1) height |= 0x80000000u;
        emit(scale, height);
    }
}
