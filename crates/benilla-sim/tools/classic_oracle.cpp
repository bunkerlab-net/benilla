// SPDX-License-Identifier: GPL-3.0-or-later
// Original test harness. The generator injects verbatim, hash-verified upstream bodies.
// No copied world rows: all parameters below are arithmetic test values.
#include <algorithm>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <string>
using uint32 = uint32_t;
using int32 = int32_t;
enum Powers { POWER_MANA };
enum UnitMods { UNIT_MOD_HEALTH, UNIT_MOD_POWER_START };
enum Modifiers { BASE_VALUE, BASE_PCT, TOTAL_VALUE, TOTAL_PCT };
enum Stats { STAT_STAMINA, STAT_INTELLECT };
constexpr int SPELL_SCHOOL_NORMAL = 0;
struct WorldObject {
    uint32 level = 1;
    bool IsUnit() const { return true; }
    uint32 GetLevel() const { return level; }
};
struct Unit : WorldObject {
    int32 armor = 0, penetration = 0, resolved_stat = 0;
    uint32 create = 0, result = 0;
    float mods[4] = {0, 1, 0, 1};
    int32 GetArmor() const { return armor; }
    int32 GetResistancePenetration(int) const { return penetration; }
    int32 GetStat(Stats) const { return resolved_stat; }
    uint32 GetCreateHealth() const { return create; }
    uint32 GetCreatePowers(Powers) const { return create; }
    float GetModifierValue(UnitMods, Modifiers slot) const { return mods[slot]; }
    void SetMaxHealth(uint32 value) { result = value; }
    void SetMaxPower(Powers, uint32 value) { result = value; }
    static float GetHealthBonusFromStamina(float stamina);
    virtual float GetHealthBonusFromStamina() const;
    static float GetManaBonusFromIntellect(float intellect);
    float GetManaBonusFromIntellect() const;
    void UpdateMaxHealth();
    void UpdateMaxPower(Powers power);
    static float CalcArmorReducedDamage(WorldObject*, Unit*, const float);
};
struct Creature : Unit {
    float create_stamina = 0, m_healthMultiplier = 1;
    float GetCreateStat(Stats) const { return create_stamina; }
    float GetHealthBonusFromStamina() const override;
    void UpdateMaxHealth();
};
// PINNED_UPSTREAM_METHODS
uint32 bits(float value) { uint32 out; std::memcpy(&out, &value, sizeof(out)); return out; }
void armor_row(uint32 level, int32 armor, int32 penetration, uint32 damage) {
    Unit attacker, victim;
    attacker.level = level; attacker.penetration = penetration; victim.armor = armor;
    float out = Unit::CalcArmorReducedDamage(&attacker, &victim, float(damage));
    std::cout << level << '\t' << armor << '\t' << penetration << '\t' << damage
              << '\t' << bits(out) << '\t' << uint32(out) << '\n';
}
void stat_row(uint32 kind, uint32 create, int32 stat, float initial, float multiplier,
              float base, float base_pct, float total, float total_pct) {
    Creature creature;
    creature.create = create; creature.resolved_stat = stat;
    creature.create_stamina = initial; creature.m_healthMultiplier = multiplier;
    creature.mods[0] = base; creature.mods[1] = base_pct;
    creature.mods[2] = total; creature.mods[3] = total_pct;
    if (kind == 0) { // A real Unit object avoids dispatching the creature-only stamina override.
        Unit player; player.create = create; player.resolved_stat = stat;
        std::copy(std::begin(creature.mods), std::end(creature.mods), std::begin(player.mods));
        player.UpdateMaxHealth(); creature.result = player.result;
    } else if (kind == 1) creature.UpdateMaxPower(POWER_MANA);
    else creature.UpdateMaxHealth();
    std::cout << kind << '\t' << create << '\t' << stat << '\t' << bits(initial)
              << '\t' << bits(multiplier) << '\t' << bits(base) << '\t' << bits(base_pct)
              << '\t' << bits(total) << '\t' << bits(total_pct) << '\t' << creature.result << '\n';
}
int main(int argc, char** argv) {
    if (argc != 2 || std::fesetround(FE_TONEAREST) != 0) return 2;
    std::string mode(argv[1]);
    if (mode == "armor") {
        std::cout << "# level armor penetration damage remaining_f32_bits melee_u32\n";
        // Every level; cap-1/cap/cap+1; integer truncation and binary32 integer boundaries.
        for (uint32 level = 1; level <= 63; ++level) {
            int32 k = 85 * level + 400;
            for (int32 armor : {-1, 0, 1, k-1, k, k+1, 3*k-1, 3*k, 3*k+1})
                for (uint32 damage : {0u, 1u, 3u, 4u, 5u, 7u, 100u, 16777217u})
                    armor_row(level, armor, 0, damage);
        }
        for (uint32 level : {1u, 60u, 63u})
            for (int32 armor : {-1000000000, -1, 0, 1, 16777215, 16777216, 16777217, 1000000000})
                for (int32 penetration : {-1000000000, -1, 0, 1, 1000000000})
                    for (uint32 damage : {0u, 1u, 100u, 16777215u, 16777216u, 16777217u, 1000000000u})
                        armor_row(level, armor, penetration, damage);
    } else if (mode == "stats") {
        std::cout << "# kind create stat initial_f32 multiplier_f32 base_f32 base_pct_f32 total_f32 total_pct_f32 result\n";
        for (uint32 kind = 0; kind < 3; ++kind)
            for (uint32 create : {0u, 1u, 50u, 16777215u, 16777216u, 16777217u})
                for (int32 stat : {0, 1, 19, 20, 21, 100, 1000000})
                    for (float pct : {0.0f, 0.5f, 1.0f, 1.25f})
                        stat_row(kind, create, stat, float(stat), 1, 0, 1, 0.5f, pct);
        for (uint32 kind = 0; kind < 3; ++kind)
            for (int32 stat : {0, 1, 19, 20, 21, 22, 100})
                for (float fraction : {0.0f, 0.25f, 0.5f, 0.75f})
                    for (float pct : {0.0f, 0.1f, 0.5f, 1.0f, 1.1f, 2.0f})
                        stat_row(kind, 100, stat, float(stat) + fraction, pct, -10, 1.1f, 2.5f, 0.9f);
        stat_row(0, 0, -1000000, 0, 1, 0, 1, 0, 1);
        stat_row(0, 0, 0, 0, 1, -1000000000, 1000, 0, 1000);
        stat_row(1, 0, -1000000, 0, 1, 0, 1, 0, 1);
    } else if (mode == "diff") {
        std::cout << "# level armor damage rational_u32 upstream_u32; exhaustive level1..10 armor0..2000 damage0..64 penetration0\n";
        // Independent algebraic comparator; not generated by nor linked to Rust.
        for (uint32 level = 1; level <= 10; ++level)
            for (int32 armor = 0; armor <= 2000; ++armor)
                for (uint32 damage = 0; damage <= 64; ++damage) {
                    Unit attacker, victim;
                    attacker.level = level; victim.armor = armor;
                    uint32 native = uint32(Unit::CalcArmorReducedDamage(&attacker, &victim, float(damage)));
                    uint64_t k = 85 * level + 400;
                    uint64_t rational = uint64_t(armor) >= 3*k ? damage/4 : damage*k/(armor+k);
                    rational = std::max(uint64_t(1), rational);
                    if (native != rational)
                        std::cout << level << '\t' << armor << '\t' << damage << '\t' << rational << '\t' << native << '\n';
                }
    } else return 2;
}
