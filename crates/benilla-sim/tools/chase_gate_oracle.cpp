// SPDX-License-Identifier: GPL-3.0-or-later
// Authored synthetic host for unchanged extracted CMaNGOS Classic expressions,
// functions and declarations; not the original method/translation unit, real
// geometry providers, movement lifecycle, PathFinder or routing admission.
// Source: 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, CMaNGOS contributors,
// GPL-2.0-or-later. Authored host: GPL-3.0-or-later. No game data or assets.
#include <array>
#include <cassert>
#include <cfenv>
#include <cfloat>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <limits>
#include <string>
#include <xmmintrin.h>
using uint8 = uint8_t;
using uint32 = uint32_t;
static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
static_assert(sizeof(uint32) == 4 && FLT_EVAL_METHOD == 0);
uint32 bits(float value) { uint32 result; std::memcpy(&result, &value, sizeof result); return result; }
float number(uint32 value) { float result; std::memcpy(&result, &value, sizeof result); return result; }
// PINNED_DISTANCE_ENUM
// PINNED_LIQUID_ENUM
// PINNED_LIQUID_DATA
// PINNED_LIQUID_MASKS
// PINNED_ENVIRONMENT_ENUM
struct Position { float x, y, z; };
std::string trace;
struct TerrainInfo {
    Position expected{};
    GridMapLiquidStatus status = LIQUID_MAP_NO_WATER;
    bool coverage = true;
    char role = 'O';
    bool CanCheckLiquidLevel(float x, float y) const {
        trace += role == 'O' ? 'o' : 't';
        assert(bits(x) == bits(expected.x) && bits(y) == bits(expected.y));
        return coverage;
    }
// PINNED_WATER_DECL
// PINNED_LIQUID_DECL
};
GridMapLiquidStatus TerrainInfo::getLiquidStatus(float x, float y, float z, uint8 mask,
                                                GridMapLiquidData* data, float height) const {
    trace += role;
    assert(data && mask == MAP_ALL_LIQUIDS && mask == 15 && bits(height) == 0x4002007eu);
    assert(bits(x) == bits(expected.x) && bits(y) == bits(expected.y) && bits(z) == bits(expected.z));
    return status;
}
struct Map {
    bool clear = true;
    mutable unsigned calls = 0;
    mutable std::array<uint32, 7> ray{};
    bool IsInLineOfSight(float x, float y, float z, float ox, float oy, float oz, bool ignoreM2) const {
        trace += 'L';
        ++calls;
        ray = {bits(x), bits(y), bits(z), bits(ox), bits(oy), bits(oz), uint32(ignoreM2)};
        return clear;
    }
};
struct WorldObject {
    Position position{};
    float reach = 0.f;
    float height = 0.f;
    Map* map = nullptr;
    Position GetPosition(void const*) const { return position; }
    void GetPosition(float& x, float& y, float& z) const { x = position.x; y = position.y; z = position.z; }
    float GetPositionX() const { return position.x; }
    float GetPositionY() const { return position.y; }
    float GetPositionZ() const { return position.z; }
    void const* GetTransport() const { std::abort(); }
    float GetCombatReach() const { return reach; }
    float GetCollisionHeight() const { return height; }
    // These branches are outside DIST_CALC_NONE and must never be executed.
    float GetObjectBoundingRadius() const { std::abort(); }
    float GetCombinedCombatReach(bool) const { std::abort(); }
    Map* GetMap() const { return map; }
// PINNED_DISTANCE_DECL
// PINNED_WITHIN_DECL
// PINNED_LOS_DECL
};
struct Unit : WorldObject {
    TerrainInfo* terrain = nullptr;
    TerrainInfo* GetTerrain() const { return terrain; }
    virtual bool IsInWater() const;
};
struct LoggedFlags {
    uint32 value = 0;
    uint32 operator&(EnvironmentFlags mask) const { trace += 'P'; return value & uint32(mask); }
};
struct Player : Unit {
    LoggedFlags m_environmentFlags;
// PINNED_PLAYER_WATER
};
// PINNED_DISTANCE
// PINNED_WITHIN
// PINNED_LOS
// PINNED_UNIT_WATER
// PINNED_TERRAIN_WATER
bool gate(Unit& owner, Unit* i_target, float x, float y, float z) {
// PINNED_CHASE_CONDITION
        return true;
    return false;
}
bool distance_part(Unit& owner, float x, float y, float z) {
    return
// PINNED_DISTANCE_EXPRESSION
    ;
}
bool vertical_part(Unit& owner, float z) {
    return
// PINNED_VERTICAL_EXPRESSION
    ;
}
struct Case {
    Position owner{0.f, 0.f, 0.f};
    Position goal{10.f, 0.f, 0.f};
    float reach = 1.5f, owner_height = 3.f, target_height = 2.f;
    // Deliberately irrelevant controls: source coordinate overload has no radius
    // or target-reach read; target feet affect only NPC target water, never LOS.
    float owner_radius = 100.f, target_reach = 90.f;
    Position target{500.f, -600.f, 700.f};
    bool los = true;
    uint8 owner_status = 0;
    bool player = true;
    uint8 target_status = 0;
    uint32 target_flags = 0;
    bool owner_coverage = true, target_coverage = true;
};
void position_out(Position p) { std::cout << bits(p.x) << ' ' << bits(p.y) << ' ' << bits(p.z) << ' '; }
void emit(Case const& c) {
    Map map;
    map.clear = c.los;
    TerrainInfo owner_terrain, target_terrain;
    owner_terrain.expected = c.owner;
    owner_terrain.status = GridMapLiquidStatus(c.owner_status);
    owner_terrain.coverage = c.owner_coverage;
    target_terrain.expected = c.target;
    target_terrain.status = GridMapLiquidStatus(c.target_status);
    target_terrain.coverage = c.target_coverage;
    target_terrain.role = 'T';
    Unit owner, npc_target;
    Player player_target;
    owner.position = c.owner; owner.reach = c.reach; owner.height = c.owner_height;
    owner.terrain = &owner_terrain; owner.map = &map;
    Unit* target = c.player ? &player_target : &npc_target;
    target->position = c.target; target->reach = c.target_reach; target->height = c.target_height;
    target->terrain = &target_terrain; target->map = &map;
    player_target.m_environmentFlags.value = c.target_flags;
    const bool distance = distance_part(owner, c.goal.x, c.goal.y, c.goal.z);
    const bool vertical = vertical_part(owner, c.goal.z);
    trace.clear();
    const bool result = gate(owner, target, c.goal.x, c.goal.y, c.goal.z);
    position_out(c.owner); position_out(c.goal);
    std::cout << bits(c.reach) << ' ' << bits(c.owner_height) << ' ' << bits(c.target_height) << ' '
              << bits(c.owner_radius) << ' ' << bits(c.target_reach) << ' ';
    position_out(c.target);
    std::cout << c.los << ' ' << unsigned(c.owner_status) << ' ' << c.player << ' '
              << unsigned(c.target_status) << ' ' << c.target_flags << ' '
              << c.owner_coverage << ' ' << c.target_coverage << " | "
              << distance << ' ' << vertical << ' ' << result << ' ' << map.calls;
    for (uint32 value : map.ray) std::cout << ' ' << value;
    std::cout << " | " << (trace.empty() ? "-" : trace) << '\n';
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
    volatile float normal = number(0x00800000), subnormal = number(1), half = .5f, one = 1.f;
    if (bits(normal * half) != 0x00400000 || bits(subnormal * one) != 1) return 2;
    emit(Case{});
    for (float reach : {0.f, .25f, 1.5f, 10.f, 100.f}) {
        for (int offset : {-1, 0, 1}) {
            for (unsigned sign : {0u, 0x80000000u}) {
                Case c;
                c.reach = reach;
                c.goal.x = number((bits(200.f + reach) + offset) | sign);
                emit(c);
                c.goal.z = number(0x409fffffu); emit(c);
                c.goal.z = 5.f; emit(c);
            }
        }
    }
    for (unsigned sign : {0u, 0x80000000u}) {
        for (uint32 z : {0u, 1u, 0x007fffffu, 0x00800000u, 0x409fffffu, 0x40a00000u, 0x40a00001u}) {
            Case c;
            c.goal.z = number(z | sign); emit(c);
            c.owner.z = c.goal.z; c.goal.z = 0.f; emit(c);
        }
    }
    // Each short-circuit edge, all source statuses and every cached flag byte.
    for (unsigned status = 0; status < 16; ++status) {
        Case c;
        c.owner_status = status; emit(c);
        c.player = false; c.target_status = status; c.owner_status = 0; emit(c);
        c.los = false; emit(c);
        c.los = true; c.goal.z = 5.f; emit(c);
        c.goal.x = 500.f; emit(c);
    }
    for (unsigned flags = 0; flags < 256; ++flags) {
        Case c;
        c.target_flags = flags;
        // A wet raw target terrain result must not replace Player's cached bit.
        c.target_status = 8; emit(c);
    }
    for (bool owner_coverage : {false, true}) {
        for (bool target_coverage : {false, true}) {
            Case c;
            c.player = false;
            c.owner_status = c.target_status = 8;
            c.owner_coverage = owner_coverage; c.target_coverage = target_coverage;
            emit(c);
        }
    }
    for (uint32 height : {0u, 0x80000000u, 1u, 0x80000001u, 0x00800000u, 0x80800000u,
                          0x3f7fffffu, 0x3f800000u, 0x3f800001u, 0xbf800000u, 0x49742400u}) {
        Case c;
        c.owner.z = c.goal.z = 1.f;
        c.owner_height = number(height); c.target_height = number(height); emit(c);
        c.owner.z = c.goal.z = 0.f; emit(c);
    }
    // Controls that distinguish the coordinate overload and goal eye endpoint.
    for (float radius : {0.f, 1.f, 500.f}) {
        for (float target_reach : {0.f, 1.f, 500.f}) {
            Case c;
            c.goal.x = 201.f; c.owner_radius = radius; c.target_reach = target_reach;
            c.target = {-900.f, 800.f, -700.f}; emit(c);
        }
    }
    // These two boundary vectors distinguish a fused first multiply/add and a
    // right-associated sum, respectively, from the unchanged source's order.
    for (auto v : {std::array<uint32, 4>{1118325860u, 1129227023u, 1083398687u, 1102662419u},
                   std::array<uint32, 4>{1128846626u, 1120493892u, 1083034757u, 1103470976u}}) {
        Case c;
        c.goal = {number(v[0]), number(v[1]), number(v[2])};
        c.reach = number(v[3]); emit(c);
    }
    for (uint32 reach : {0u, 0x80000000u, 1u, 0x007fffffu, 0x00800000u, 0x7f7fffffu}) {
        Case c;
        c.goal.x = 300.f; c.reach = number(reach); emit(c);
    }
    for (float sign : {-1.f, 1.f}) {
        Case c;
        c.owner = {sign * 32760.f, sign * 32760.f, sign * 32760.f};
        c.goal = {sign * 32759.f, sign * 32759.f, sign * 32759.f};
        emit(c);
    }
    uint32 seed = 0xd3e1a9c7u;
    auto next = [&seed]() { seed = seed * 1664525u + 1013904223u; return seed; };
    for (unsigned i = 0; i < 2048; ++i) {
        Case c;
        c.owner = {float(int(next() % 32000) - 16000), float(int(next() % 32000) - 16000),
                   float(int(next() % 1000) - 500)};
        c.goal = {c.owner.x + float(int(next() % 45000) - 22500) / 100.f,
                  c.owner.y + float(int(next() % 40000) - 20000) / 100.f,
                  c.owner.z + float(int(next() % 1200) - 600) / 100.f};
        c.reach = number(0x3d000000u + next() % 0x05800000u);
        c.owner_height = number(0x3c000000u + next() % 0x06000000u);
        c.target_height = number(0x3c000000u + next() % 0x06000000u);
        c.player = (next() & 8) != 0;
        c.los = (next() % 5) != 0;
        c.owner_status = (next() % 8) == 0 ? 1 : 0;
        c.target_status = (next() % 8) == 0 ? 2 : 0;
        c.target_flags = next() % 4;
        emit(c);
    }
}
