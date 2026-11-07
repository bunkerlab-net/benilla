// SPDX-License-Identifier: GPL-3.0-or-later
// Original bounded probes; generator inserts unchanged CMaNGOS source blocks.
// CMaNGOS Project contributors; original GPL-2.0-or-later grant applies.
// This authored orchestration is not full Spell::cast/CheckCast execution.
// No native database/client rows, modifiers, item/triggered or health-power paths.
// Columns: prepare rank, release rank, release create mana, available mana,
// prepared cast ms, newly computed cast ms, prepare cost, release cost, launched,
// final mana, direct amount (-1 on failure), impact periodic amount (-1 on
// failure), direct effect draw count, last mana-use count, cooldown count.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cstdint>
#include <iostream>
using int32 = int32_t;
using uint16 = uint16_t;
using uint32 = uint32_t;
using Powers = uint32;
constexpr uint32 POWER_MANA=0, POWER_RAGE=1, POWER_FOCUS=2, POWER_ENERGY=3,
    POWER_HAPPINESS=4, MAX_POWERS=5, POWER_HEALTH=99,
    UNIT_FIELD_POWER_COST_MULTIPLIER=100, SPELL_ATTR_EX2_DONT_BLOCK_MANA_REGEN=1;
constexpr uint32 SPELL_STATE_CASTING=1;
enum SpellCastResult { SPELL_CAST_OK, SPELL_FAILED_NO_POWER, SPELL_FAILED_CASTER_AURASTATE, SPELL_FAILED_UNKNOWN };
struct Log { template<typename... T> void outError(const char*, T...) { assert(false); } } sLog;
struct SpellEntry {
    uint32 Id=900001, baseLevel=1, spellLevel=1, maxLevel=0;
    uint32 manaCost=10, manaCostPerlevel=10, ManaCostPercentage=1, powerType=POWER_MANA;
    uint32 EffectBaseDice[3]{1,1,0};
    int32 EffectBasePoints[3]{9,1,0}, EffectDieSides[3]{6,1,0};
    float EffectRealPointsPerLevel[3]{10.f,2.f,0.f}, EffectDicePerLevel[3]{1.f,0.f,0.f}, EffectPointsPerComboPoint[3]{};
    bool HasAttribute(uint32) const { return false; }
};
struct Unit {
    uint32 rank=5, create_mana=100, mana=100, last_mana_uses=0;
    uint32 GetSpellRank(const SpellEntry*) const { return rank; }
    uint32 GetCreateMana() const { return create_mana; }
    uint32 GetCreateHealth() const { assert(false); return 0; }
    uint32 GetMaxPower(Powers) const { assert(false); return 0; }
    uint32 GetHealth() const { assert(false); return 0; }
    uint32 GetPower(Powers power) const { assert(power == POWER_MANA); return mana; }
    float GetFloatValue(uint16) const { return 0.f; }
    void ModifyHealth(int32) { assert(false); }
    void ModifyPower(Powers power, int32 delta) { assert(power == POWER_MANA); mana += delta; }
    void SetLastManaUse() { ++last_mana_uses; }
    void* GetSpellModOwner() const { return nullptr; }
};
struct Spell;
struct SpellModRAII { SpellModRAII(Spell*, void*, bool, bool) {} };
uint32 GetSpellCastTime(const SpellEntry* spellInfo, Unit* caster, Spell*, bool) {
    struct { int32 CastTime=1500, CastTimePerLevel=-100, MinCastTime=500; } row;
    auto* spellCastTimeEntry=&row;
    int32 spellRank=caster->GetSpellRank(spellInfo);
    // PINNED_CAST
    return castTime>0 ? uint32(castTime) : 0;
}
int32 effect_roll=0;
uint32 draws=0;
int32 irand(int32 lo, int32 hi) { assert(effect_roll >= lo && effect_roll <= hi); ++draws; return effect_roll; }
int32 effect(const SpellEntry* spellProto, const Unit* unitCaster, unsigned effect_index) {
    const int32* effBasePoints=nullptr;
    bool maximum=false;
    // PINNED_EFFECT
    (void)comboDamage;
    return value;
}
bool IsAllowingDeadTarget(const SpellEntry*) { return false; }
bool IsNextMeleeSwingSpell(const SpellEntry*) { return false; }
struct Target { bool IsAlive() const { return true; } };
struct Spell {
    const SpellEntry* m_spellInfo;
    Unit* m_caster;
    Unit* m_trueCaster;
    void* m_CastItem=nullptr;
    bool m_triggeredByAuraSpell=false, m_ignoreCastTime=false, m_IsTriggeredSpell=false;
    int32 m_casttime=0;
    uint32 m_timer=0, m_powerCost=0, m_spellState=0;
    uint32 released=0, failed=0, cooldowns=0;
    int32 direct=-1;
    Target target;
    struct Targets { Target* target; Target* getUnitTarget() { return target; } } m_targets{&target};
    void prepareDataForTriggerSystem() {}
    bool IsAutoRepeat() const { return false; }
    void cancel() { assert(false); }
    void Prepare() {
        // PINNED_PREPARE
    }
    // PINNED_RESET
    static uint32 CalculatePowerCost(const SpellEntry* spellInfo, Unit* caster, Spell*, void*, bool) {
        uint32 school=2;
        // PINNED_COST
    }
    SpellCastResult CheckPower(bool strict);
    void TakePower();
    void cast() {
        // Authored bounded CheckCast(false)/launch orchestration. The unchanged
        // CheckPower below re-evaluates source rank and create mana at this point.
        if (CheckPower(false) != SPELL_CAST_OK) { ++failed; return; }
        ++cooldowns;
        TakePower();
        direct=effect(m_spellInfo,m_caster,0);
        ++released;
    }
    void update(uint32 difftime) {
        switch (m_spellState) {
            // PINNED_TIMER
        }
    }
};
// PINNED_CHECK_POWER
// PINNED_TAKE_POWER
int main() {
    assert(std::fesetround(FE_TONEAREST) == 0);
    for (uint32 prepared_rank : {5U,10U,15U,295U,300U,315U})
    for (uint32 release_rank : {5U,10U,20U,295U,300U,315U})
    for (uint32 create_mana : {100U,250U})
    for (uint32 available_mana : {0U,10U,30U,100U,1000U}) {
        SpellEntry entry;
        Unit unit;
        unit.rank=prepared_rank; unit.mana=10000;
        Spell spell{&entry,&unit,&unit};
        assert(spell.CheckPower(true) == SPELL_CAST_OK);
        uint32 prepared_cost=spell.m_powerCost;
        spell.Prepare();
        uint32 prepared_time=spell.m_timer;
        unit.rank=release_rank;
        unit.create_mana=create_mana;
        unit.mana=available_mana;
        uint32 current_time=GetSpellCastTime(&entry,&unit,&spell,true);
        effect_roll=6 + int32(release_rank/5)-1;
        draws=0;
        spell.update(prepared_time-1);
        assert(spell.m_timer == 1 && spell.released == 0 && spell.failed == 0);
        spell.update(1);
        uint32 direct_draws=draws;
        int32 periodic=-1;
        if (spell.released) {
            unit.rank=25;
            periodic=effect(&entry,&unit,1);
        }
        std::cout << prepared_rank << ' ' << release_rank << ' ' << create_mana << ' ' << available_mana
            << ' ' << prepared_time << ' ' << current_time << ' ' << prepared_cost << ' ' << spell.m_powerCost
            << ' ' << spell.released << ' ' << unit.mana << ' ' << spell.direct << ' ' << periodic
            << ' ' << direct_draws << ' ' << unit.last_mana_uses << ' ' << spell.cooldowns << '\n';
    }
}
