// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic adapters only. The generator inserts unchanged pinned source bodies.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <limits>
#include <map>
#include <string>
#include <vector>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
using uint16 = uint16_t;
using uint32 = uint32_t;
using int32 = int32_t;
using WeaponAttackType = int;
constexpr uint16 SKILL_DEFENSE = 95;
constexpr int STAT_INTELLECT = 0, CONFIG_UINT32_SKILL_GAIN_DEFENSE = 0;
constexpr uint32 SKILL_NEW = 1, SKILL_CHANGED = 2, SKILL_DELETED = 3;
struct SkillStatusData { uint32 pos = 0, uState = 0; };
using SkillStatusMap = std::map<uint16, SkillStatusData>;
uint32 PLAYER_SKILL_VALUE_INDEX(uint32) { return 0; }
uint16 SKILL_VALUE(uint32 value) { return uint16(value); }
uint16 SKILL_MAX(uint32 value) { return uint16(value >> 16); }
uint32 MAKE_SKILL_VALUE(uint32 value, uint32 maximum) { return value | (maximum << 16); }
struct World {
    uint16 gain = 1;
    uint32 getConfig(int) const { return gain; }
} sWorld;
static double roll = 0;
static uint32 draws = 0, captured_chance = 0;
template<class Bits, class Value> Bits bits(Value value) {
    static_assert(sizeof(Bits) == sizeof(Value));
    Bits result;
    std::memcpy(&result, &value, sizeof(result));
    return result;
}
double rand_chance() { ++draws; return roll; }
namespace source_rng {
// PINNED_ROLL
}
bool roll_chance_f(float chance) {
    captured_chance = bits<uint32>(chance);
    return source_rng::roll_chance_f(chance);
}
struct Player {
    uint32 harness_level = 1;
    uint16 skill_value = 1, maximum = 5;
    SkillStatusMap mSkillStatus{{SKILL_DEFENSE, {}}};
    uint16 GetSkillValuePure(uint16) const { return skill_value; }
    uint16 GetSkillMaxPure(uint16) const { return maximum; }
    uint16 GetWeaponSkillIdForAttack(int) const { assert(false); return 0; }
    uint32 GetLevel() const { return harness_level; }
    float GetStat(int) const { assert(false); return 0; }
    void UpdateWeaponSkill(int) { assert(false); }
    void UpdateDefenseBonusesMod() {}
    uint32 GetUInt32Value(uint32) const { return MAKE_SKILL_VALUE(skill_value, maximum); }
    void SetUInt32Value(uint32, uint32 packed) { skill_value = SKILL_VALUE(packed); }
    void UpdateCombatSkills(uint32, WeaponAttackType, bool);
    void UpdateDefense(uint32);
    void UpdateSkill(uint16, uint16);
};
// PINNED_METHODS
// CANONICAL_LOG_BITS

float canonical_chance(uint32 level, uint16 maximum, uint16 value) {
    const int32 room = int32(maximum - value);
    double base;
    if (room >= 7) {
        const double gap = 0.22 * (1.0 + (12.0 / level));
        const double log = bits<double>(canonical_log_bits[room - 7]);
        base = (7.0 / maximum) + gap * log;
    } else {
        base = (room * room) / (7.0 * maximum);
    }
    return std::clamp(base, 0.0, 1.0) * 100.0f;
}
void byte(uint32 n) { std::cout.put(char(n & 255)); }
void word(uint32 n) { for (int i = 0; i < 4; ++i) byte(n >> (8 * i)); }
uint32 boundary_mask(float chance) {
    uint32 result = 0;
    const double exact = chance;
    const double adjacent[] = {std::nextafter(exact, 0.0), exact,
                              std::nextafter(exact, std::numeric_limits<double>::infinity())};
    for (uint32 i = 0; i < 3; ++i) {
        roll = adjacent[i];
        // Invalid semantic draws are tagged 2; they never reach the source RNG.
        uint32 outcome = roll >= 100.0 ? 2 : uint32(source_rng::roll_chance_f(chance));
        result |= outcome << (2 * i);
    }
    return result;
}
int main(int argc, char** argv) {
#ifdef _WIN32
    assert(_setmode(_fileno(stdout), _O_BINARY) != -1);
#endif
    assert(std::fesetround(FE_TONEAREST) == 0);
    static_assert(std::numeric_limits<double>::is_iec559 && sizeof(double) == 8);
    static_assert(std::numeric_limits<float>::is_iec559 && sizeof(float) == 4);
    assert(argc == 2);
    const std::string mode = argv[1];
    if (mode == "logs") {
        for (int room = 7; room <= 300; ++room)
            std::cout << room << ' ' << bits<uint64_t>(std::log((room + 5.0) / 12.0)) << '\n';
    } else if (mode == "chances") {
        for (uint32 level = 1; level <= 60; ++level)
            for (uint16 maximum = 1; maximum <= 300; ++maximum)
                for (uint16 value = 0; value < maximum; ++value) {
                    Player player; player.harness_level = level; player.maximum = maximum; player.skill_value = value;
                    roll = std::nextafter(100.0, 0.0); draws = 0;
                    player.UpdateCombatSkills(0, 0, true);
                    assert(draws == 1);
                    const float chance = bits<float>(captured_chance);
                    const uint32 canonical = bits<uint32>(canonical_chance(level, maximum, value));
                    if (captured_chance != canonical) {
                        std::cerr << "Final f32 divergence: level=" << level << " maximum="
                                  << maximum << " value=" << value << " native_bits="
                                  << captured_chance << " canonical_bits=" << canonical << '\n';
                        return 1;
                    }
                    word(captured_chance);
                    byte(boundary_mask(chance));
                }
    } else if (mode == "edges") {
        std::cout << "# level maximum value gain roll_bits chance_bits draws after\n";
        for (uint32 level : {1u, 2u, 12u, 59u, 60u})
            for (uint16 maximum : {0, 1, 5, 7, 8, 10, 100, 300, 65535}) {
                std::vector<uint16> values{0, maximum};
                if (maximum < 65535) values.push_back(maximum + 1);
                for (uint16 room : {1, 6, 7, 8})
                    if (room <= maximum) values.push_back(maximum - room);
                std::sort(values.begin(), values.end());
                values.erase(std::unique(values.begin(), values.end()), values.end());
                for (uint16 value : values) {
                    if (maximum > 300 && value < maximum && maximum - value >= 7) continue;
                    Player probe; probe.harness_level = level; probe.maximum = maximum; probe.skill_value = value;
                    roll = 0; draws = 0; captured_chance = 0;
                    probe.UpdateCombatSkills(0, 0, true);
                    const float chance = bits<float>(captured_chance);
                    std::vector<double> rolls{0, std::nextafter(100.0, 0.0)};
                    if (draws) {
                        rolls.push_back(std::nextafter(double(chance), 0.0));
                        if (chance < 100) {
                            rolls.push_back(chance);
                            rolls.push_back(std::nextafter(double(chance), 100.0));
                        }
                    }
                    for (uint16 gain : {0, 1, 65535})
                        for (double value_roll : rolls) {
                            Player player; player.harness_level = level; player.maximum = maximum; player.skill_value = value;
                            sWorld.gain = gain; roll = value_roll; draws = 0; captured_chance = 0;
                            player.UpdateCombatSkills(0, 0, true);
                            std::cout << level << ' ' << maximum << ' ' << value << ' ' << gain
                                      << ' ' << bits<uint64_t>(roll) << ' ' << captured_chance
                                      << ' ' << draws << ' ' << player.skill_value << '\n';
                        }
                }
            }
    } else {
        return 1;
    }
}
