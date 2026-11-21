// SPDX-License-Identifier: GPL-3.0-or-later
// Authored synthetic host; no game rows or assets. Pinned unchanged player
// initialization/native scale, Object setters/getter, Unit height, DBC declarations,
// formats/constants and Gender are inserted by the generator. Not the original
// translation unit, DBC loader or live object lifecycle.
// Source: CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// Project contributors, GPL-2.0-or-later. Authored host: GPL-3.0-or-later.
#include <cassert>
#include <cfenv>
#include <cfloat>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <limits>
#include <vector>
#include <xmmintrin.h>
#define MANGOS_ASSERT assert
using uint8 = uint8_t;
using uint16 = uint16_t;
using uint32 = uint32_t;
using int32 = int32_t;
static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
static_assert(sizeof(uint32) == 4 && FLT_EVAL_METHOD == 0);
// PINNED_CONSTANTS
// PINNED_GENDER
// PINNED_FORMATS
// PINNED_DISPLAY
// PINNED_MODEL
struct Displays {
    CreatureDisplayInfoEntry male{31, 51, 0, 1.0f};
    CreatureDisplayInfoEntry female{37, 57, 0, 1.0f};
    CreatureDisplayInfoEntry const* LookupEntry(uint32 id) const {
        return id == 31 ? &male : id == 37 ? &female : nullptr;
    }
} sCreatureDisplayInfoStore;
struct Models {
    CreatureModelDataEntry male{51, 0, 1.0f, 11.0f, 3.0f};
    CreatureModelDataEntry female{57, 0, 1.0f, 13.0f, 5.0f};
    CreatureModelDataEntry const* LookupEntry(uint32 id) const {
        return id == 51 ? &male : id == 57 ? &female : nullptr;
    }
} sCreatureModelDataStore;
struct PlayerInfo { uint32 displayId_m = 31, displayId_f = 37; };
struct ObjectManager {
    PlayerInfo info;
    PlayerInfo const* GetPlayerInfo(uint8 race, uint8 player_class) const {
        return race == 1 && player_class == 1 ? &info : nullptr;
    }
} sObjectMgr;
struct Log {
    uint32 errors = 0;
    void outError(char const*, uint32) { ++errors; }
} sLog;
constexpr uint16 OBJECT_FIELD_SCALE_X = 0;
struct Object {
    // Original zero-filled allocator/storage declarations; authored count and
    // constructor model the verified fresh-player pre-InitDisplayIds boundary.
// PINNED_STORAGE
    Object() { _InitValues(); }
    ~Object() { delete[] m_uint32Values; }
    void _InitValues();
    bool m_objectUpdated = false;
    uint16 m_valuesCount = 1;
    bool PrintIndexError(uint16, bool) const { return false; }
    void MarkForClientUpdate() {}
    void SetFloatValue(uint16 index, float value);
    void SetObjectScale(float newScale);
// PINNED_GETTER
};
// PINNED_INIT_VALUES
// PINNED_SET_FLOAT
// PINNED_SET_SCALE
struct Unit : Object {
    uint32 native_display = 0, current_display = 0;
    uint32 GetNativeDisplayId() const { return native_display; }
    uint32 GetDisplayId() const { return current_display; }
    void SetNativeDisplayId(uint32 value) { native_display = value; }
    void SetDisplayId(uint32 value) { current_display = value; }
    float GetCollisionHeight() const;
};
// PINNED_HEIGHT
struct Player : Unit {
    uint8 gender = GENDER_MALE, race = 1;
    uint8 getRace() const { return race; }
    uint8 getClass() const { return 1; }
    uint8 getGender() const { return gender; }
    uint32 GetGUIDLow() const { return 1; }
    void InitDisplayIds();
    float GetNativeScale() const;
};
// PINNED_INIT
// PINNED_NATIVE_SCALE
uint32 bits(float value) { uint32 result; std::memcpy(&result, &value, sizeof result); return result; }
float number(uint32 value) { float result; std::memcpy(&result, &value, sizeof result); return result; }
void emit(uint8 gender, uint32 display_scale, uint32 model_scale, uint32 raw_height) {
    Player player;
    player.gender = gender;
    auto& display = gender == GENDER_MALE ? sCreatureDisplayInfoStore.male : sCreatureDisplayInfoStore.female;
    auto& model = gender == GENDER_MALE ? sCreatureModelDataStore.male : sCreatureModelDataStore.female;
    display.scale = number(display_scale);
    model.Scale = number(model_scale);
    model.CollisionHeight = number(raw_height);
    player.InitDisplayIds();
    assert(player.GetNativeDisplayId() == display.Displayid);
    assert(player.GetDisplayId() == display.Displayid);
    // Current-display data cannot substitute for the native model in height.
    player.SetDisplayId(99);
    float product = player.GetObjectScale() * model.CollisionHeight;
    std::cout << unsigned(gender) << ' ' << display.Displayid << ' ' << model.Id << ' '
              << display_scale << ' ' << model_scale << ' ' << raw_height << ' '
              << bits(player.GetNativeScale()) << ' ' << bits(player.m_floatValues[0]) << ' '
              << bits(player.GetObjectScale()) << ' ' << bits(product) << ' '
              << bits(player.GetCollisionHeight()) << '\n';
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
    for (uint32 stored : {0u, 0x80000000u}) {
        Object object;
        object.m_floatValues[0] = number(stored);
        if (bits(object.GetObjectScale()) != 0x3f800000u) return 2;
    }
    if (std::strcmp(CreatureDisplayInfofmt, "nixifxxxxxxx") != 0
        || std::strcmp(CreatureModelDatafmt, "nixxfxxxxxxxxxff") != 0
        || bits(DEFAULT_COLLISION_HEIGHT) != 0x4002007eu) return 2;
    Player invalid_gender;
    invalid_gender.gender = GENDER_NONE;
    invalid_gender.InitDisplayIds();
    Player invalid_race;
    invalid_race.race = 0;
    invalid_race.InitDisplayIds();
    if (sLog.errors != 2 || invalid_gender.GetNativeDisplayId() || invalid_race.GetNativeDisplayId()) return 2;
    for (uint8 gender : {uint8(GENDER_MALE), uint8(GENDER_FEMALE)}) {
        for (uint32 display : {0u, 0x80000000u, 0x00800000u, 0x80800000u,
                               0x3f000000u, 0x3f7fffffu, 0x3f800000u, 0x3f800001u,
                               0x3fa00000u, 0x40000000u, 0xbf800000u, 0x49742400u}) {
            for (uint32 model : {0u, 0x80000000u, 0x00800000u, 0x3f000000u,
                                 0x3f7fffffu, 0x3f800000u, 0x3f800001u, 0x3fc00000u,
                                 0xbf800000u, 0x49742400u}) {
                for (uint32 height : {0u, 0x80000000u, 0x00800000u, 0x80800000u,
                                      0x3f000000u, 0x4002007du, 0x4002007eu, 0x4002007fu,
                                      0x40400000u, 0xbf800000u, 0x49742400u})
                    emit(gender, display, model, height);
            }
        }
        // Invalid raw inputs enter the oracle but must fail the Rust API boundary.
        for (uint32 invalid : {1u, 0x007fffffu, 0x49742401u, 0x7f7fffffu,
                               0x80000001u, 0x807fffffu, 0xc9742401u, 0xff7fffffu}) {
            emit(gender, invalid, 0x3f800000u, 0x3f800000u);
            emit(gender, 0x3f800000u, invalid, 0x3f800000u);
            emit(gender, 0x3f800000u, 0x3f800000u, invalid);
        }
        // Each stage rounds separately, with adjacent result bounds.
        for (uint32 near : {0x48f423ffu, 0x48f42400u, 0x48f42401u}) {
            emit(gender, 0x40000000u, near, 0x3f000000u);
            emit(gender, 0x3f800000u, 0x40000000u, near);
        }
    }
    uint32 seed = 0x519034afu;
    for (uint32 i = 0; i < 4096; ++i) {
        seed = seed * 1664525u + 1013904223u;
        uint32 display = 0x3e000000u + seed % 0x03000000u;
        seed = seed * 1664525u + 1013904223u;
        uint32 model = 0x3e000000u + seed % 0x03000000u;
        seed = seed * 1664525u + 1013904223u;
        uint32 height = 0x3e000000u + seed % 0x03000000u;
        if (i & 2) height |= 0x80000000u;
        emit(uint8(i & 1), display, model, height);
    }
}
