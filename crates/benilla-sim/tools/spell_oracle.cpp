// SPDX-License-Identifier: GPL-3.0-or-later
// Original adapter/probes. Generator inserts unchanged pinned source bodies or
// named contiguous arithmetic blocks. No real database/DBC rows are represented.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
using int32 = int32_t;
using uint8 = uint8_t;
using uint32 = uint32_t;
using uint64 = uint64_t;
using SpellSchoolMask = uint32;
using SpellDmgClass = uint32;
using SpellSchools = uint32;
constexpr uint32 SPELL_SCHOOL_MASK_NORMAL=1, SPELL_SCHOOL_MASK_HOLY=2, TYPEID_UNIT=3;
constexpr uint32 SPELL_ATTR_EX3_ALWAYS_HIT=1,SPELL_ATTR_EX3_NORMAL_RANGED_ATTACK=2,SPELL_DAMAGE_CLASS_MELEE=2,SPELL_DAMAGE_CLASS_RANGED=3,UNIT_FIELD_FLAGS=1,UNIT_FLAG_PLAYER_CONTROLLED=2,SPELL_AURA_MOD_ATTACKER_SPELL_HIT_CHANCE=1;
using Powers = uint32;
using uint16 = uint16_t;
constexpr uint32 POWER_HEALTH=99, POWER_MANA=0, POWER_RAGE=1, POWER_FOCUS=2, POWER_ENERGY=3, POWER_HAPPINESS=4, UNIT_FIELD_POWER_COST_MULTIPLIER=100;
struct Log { template<typename... T> void outError(const char*,T...) {assert(false);} } sLog;
float number(uint32 bits) { float v; std::memcpy(&v, &bits, 4); return v; }
uint32 bits(float v) { uint32 b; std::memcpy(&b, &v, 4); return b; }
struct SpellEntry {
    bool HasAttribute(uint32) const { return false; }
    uint32 EffectBaseDice[3]{};
    float EffectRealPointsPerLevel[3]{}, EffectDicePerLevel[3]{}, EffectPointsPerComboPoint[3]{};
    int32 EffectBasePoints[3]{}, EffectDieSides[3]{};
    uint32 maxLevel{}, baseLevel{}, spellLevel{}, DmgClass{1}, manaCost{}, manaCostPerlevel{}, ManaCostPercentage{}, Id{}, powerType{};
};
uint32 GetSpellSchoolMask(const SpellEntry*) { return 4; }
uint32 GetWeaponAttackType(const SpellEntry*) { return 0; }
struct Unit {
    uint32 rank{}, create_mana{}, level{};
    float miss=0;
    bool HasFlag(uint32,uint32) const { return false; }
    uint32 GetLevelForTarget(const Unit*) const { return level; }
    float GetMissChance(const SpellEntry*,SpellSchoolMask) const { return miss; }
    float CalculateEffectiveMissChance(const Unit*,uint32,const SpellEntry*) const { assert(false);return 0; }
    float GetHitChance(const SpellEntry*,SpellSchoolMask) const { return 0; }
    float GetTotalAuraModifierByMiscMask(uint32,SpellSchoolMask) const { return 0; }
    uint16 GetSkillMaxForLevel(const Unit* = nullptr) const { return uint16(level * 5); }
    uint32 GetTypeId() const { return TYPEID_UNIT; }
    int32 GetResistance(SpellSchools) const { return 0; }
    int32 GetResistancePenetration(SpellSchools) const { return 0; }
    float CalculateEffectiveMagicResistancePercent(const Unit*, SpellSchoolMask, bool) const;
    float CalculateSpellMissChance(const Unit*,SpellSchoolMask,const SpellEntry*) const;
    uint32 GetCreateMana() const { return create_mana; }
    uint32 GetCreateHealth() const { return 100; }
    uint32 GetMaxPower(Powers) const { return 100; }
    float GetFloatValue(uint16) const { return 0; }
    uint32 GetSpellRank(const SpellEntry*) const { return rank; }
    uint32 GetCreatureTypeMask() const { return 1; }
    float GetCritMultiplier(SpellSchoolMask, uint32, const SpellEntry*, bool) const { return 1.5f; }
    float GetCritTakenMultiplier(SpellSchoolMask, SpellDmgClass, float, bool) const { return 1.0f; }
    uint32 CalculateCritAmount(const Unit*, uint32, const SpellEntry*, bool) const;
};
int32 effect_roll = 0;
int32 irand(int32 lo, int32 hi) { assert(effect_roll >= lo && effect_roll <= hi); return effect_roll; }
uint32 combat_roll = 1;
uint32 urand(uint32 lo, uint32 hi) { assert(combat_roll >= lo && combat_roll <= hi); return combat_roll; }
int32 effect(const SpellEntry* spellProto, const Unit* unitCaster) {
    unsigned effect_index = 0;
    const int32* effBasePoints = nullptr;
    bool maximum = false;
    // PINNED_EFFECT
    (void)comboDamage;
    return value;
}
uint64 travel(float dist, float speed) {
    struct { uint64 timeDelay; } targetInfo{};
    // PINNED_TRAVEL
    return targetInfo.timeDelay;
}
// PINNED_CRIT
// PINNED_CHANCE
// PINNED_MISS
// PINNED_MAGIC_RESIST
struct Aura {
    bool m_isPeriodic = true;
    int32 m_periodicTimer = 1000;
    uint32 m_periodicTick = 0;
    struct { uint32 periodictime = 1000; } m_modifier;
    uint32 dealt = 0;
    void PeriodicTick() { ++dealt; }
    void Update(uint32);
};
// PINNED_AURA
struct TimerTarget { bool alive=true; bool IsAlive() const { return alive; } };
struct SpellTimer {
    uint32 m_timer=0;
    bool m_IsTriggeredSpell=false;
    SpellEntry* m_spellInfo=nullptr;
    struct { TimerTarget* target; TimerTarget* getUnitTarget() { return target; } } m_targets;
    uint32 cancelled=0,casted=0;
    bool IsAutoRepeat() const { return false; }
    void cancel() { ++cancelled; }
    void cast() { ++casted; }
    void update(uint32);
};
bool IsAllowingDeadTarget(const SpellEntry*) { return false; }
bool IsNextMeleeSwingSpell(const SpellEntry*) { return false; }
void SpellTimer::update(uint32 difftime) {
    constexpr int SPELL_STATE_CASTING=1;
    switch(SPELL_STATE_CASTING) {
        // PINNED_TIMER
    }
}
uint32 cost(const SpellEntry* spellInfo, Unit* caster) {
    uint32 school=2;
    // PINNED_COST
}
uint32 casttime(int32 base,int32 perlevel,int32 minimum,int32 rank,uint32 baselevel) {
    struct { int32 CastTime,CastTimePerLevel,MinCastTime; } row{base,perlevel,minimum};
    auto* spellCastTimeEntry=&row;
    SpellEntry p; p.baseLevel=baselevel;
    auto* spellInfo=&p;
    int32 spellRank=rank;
    // PINNED_CAST
    return castTime>0 ? uint32(castTime) : 0;
}
int main() {
    std::fesetround(FE_TONEAREST);
    Unit unit;
    for (uint32 rank : {1,4,5,9,10,15,100,105,295,300,315}) {
        unit.rank = rank;
        for (int32 base : {-50,0,1,18,999990}) for (int32 dice : {0,1,6,-3})
        for (uint32 coefficient : {0U, 0x3dcccccdU,0x3f000000U,0xbf000000U})
        for (uint32 maxlevel : {0U,2U,20U}) {
            SpellEntry p;
            p.EffectBaseDice[0] = 1;
            p.EffectBasePoints[0] = base;
            p.EffectDieSides[0] = dice;
            p.EffectRealPointsPerLevel[0] = number(coefficient);
            p.EffectDicePerLevel[0] = 0.1f;
            p.baseLevel = 1; p.spellLevel = 2; p.maxLevel = maxlevel;
            int32 level = int32(rank/5);
            if (level > int32(p.maxLevel) && p.maxLevel) level = int32(p.maxLevel);
            else if (level < int32(p.baseLevel)) level = int32(p.baseLevel);
            level -= int32(p.spellLevel);
            int32 sides = dice + (coefficient ? int32(level * p.EffectDicePerLevel[0]) : 0);
            for (int32 chosen : {std::min(1,sides),std::max(1,sides)}) {
                effect_roll = chosen;
                int32 value = effect(&p, &unit);
                std::cout << "E " << rank << ' ' << base << ' ' << dice << ' ' << coefficient << ' ' << maxlevel << ' ' << ((sides == 0 || sides == 1) ? -9999999 : chosen) << ' ' << value << '\n';
            }
        }
    }
    for (uint32 distance : {0U,1U,0x00800000U,0x3f800000U,0x41c7ffffU,0x41c80000U,0x41c80001U,0x42c80000U,0x447a0000U,0x49742400U}) {
        if (distance == 1) continue; // input domain excludes subnormals
        for (float speed : {1.0f,7.0f,20.0f,28.0f,1000.0f})
            std::cout << "T " << distance << ' ' << bits(speed) << ' ' << travel(number(distance), speed) << '\n';
    }
    // Deterministic original numeric probes around square-root/quotient boundaries.
    uint32 state = 0x519acd31U;
    for (unsigned i=0; i<2048; ++i) {
        state ^= state << 13; state ^= state >> 17; state ^= state << 5;
        float d = float(state % 1000001);
        float s = float(1 + (state >> 20) % 1000);
        std::cout << "T " << bits(d) << ' ' << bits(s) << ' ' << travel(d,s) << '\n';
    }
    for (uint32 input : {0U,0x3ba3d709U,0x3ba3d70aU,0x3ba3d70bU,0x3f000000U,0x40800000U,0x42c7ffffU,0x42c80000U})
    for (uint32 roll : {1U,49U,50U,51U,399U,400U,401U,9999U,10000U}) {
        combat_roll = roll;
        std::cout << "C " << input << ' ' << roll << ' ' << chance_u(number(input)) << ' ' << roll_chance_combat(number(input)) << '\n';
    }
    SpellEntry p;
    for (uint32 damage : {0U,1U,2U,3U,17U,18U,99999U,333333U,666666U})
        std::cout << "D " << damage << ' ' << unit.CalculateCritAmount(&unit, damage, &p, false) << '\n';
    for (uint32 remaining : {1U,500U,1500U}) for (uint32 diff : {0U,1U,499U,500U,1499U,1500U,1501U}) for (bool alive : {false,true}) {
        SpellTimer timer; TimerTarget target; target.alive=alive; timer.m_timer=remaining; timer.m_targets.target=&target;
        timer.update(diff);
        std::cout << "S " << remaining << ' ' << diff << ' ' << alive << ' ' << timer.m_timer << ' ' << timer.cancelled << ' ' << timer.casted << '\n';
    }
    for (uint32 flat : {0U,10U,1000U}) for (uint32 per : {0U,1U,10U}) for (uint32 pct : {0U,1U,10U,100U}) for (uint32 rank : {5U,10U,100U,295U,300U,315U}) {
        SpellEntry s; s.manaCost=flat; s.manaCostPerlevel=per; s.ManaCostPercentage=pct; s.baseLevel=2;
        unit.rank=rank; unit.create_mana=123;
        std::cout << "P " << flat << ' ' << per << ' ' << pct << ' ' << rank << ' ' << cost(&s,&unit) << '\n';
    }
    for (int32 base : {-100,0,1000,3000}) for (int32 per : {-100,0,100}) for (int32 minimum : {0,500}) for (int32 rank : {5,10,100,295,300,315})
        std::cout << "B " << base << ' ' << per << ' ' << minimum << ' ' << rank << ' ' << casttime(base,per,minimum,rank,1) << '\n';
    Unit target;
    for (uint32 casterlevel=1;casterlevel<=60;++casterlevel) for (uint32 targetlevel=1;targetlevel<=63;++targetlevel)
    for (uint32 base : {0U,0x3ba3d709U,0x3ba3d70aU,0x40800000U,0x42c80000U}) {
        unit.level=casterlevel; target.level=targetlevel; target.miss=number(base);
        const auto resistance=target.CalculateEffectiveMagicResistancePercent(&unit,4,false);
        if (base == 0) std::cout << "R " << casterlevel << ' ' << targetlevel << ' ' << bits(resistance) << '\n';
        if (resistance != 0.f) continue;
        std::cout << "M " << casterlevel << ' ' << targetlevel << ' ' << base << ' ' << chance_u(unit.CalculateSpellMissChance(&target,4,&p)) << '\n';
    }
    Aura aura;
    for (uint32 diff : {999U,1U,3500U,0U,0U,500U,1000U}) {
        aura.Update(diff);
        std::cout << "A " << diff << ' ' << aura.m_periodicTimer << ' ' << aura.m_periodicTick << '\n';
    }
}
