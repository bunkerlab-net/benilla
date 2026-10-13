// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic source-state adapters, not database records. Pinned bodies inserted by generator.
#include <algorithm>
#include <array>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <random>
#include <vector>
using uint8=uint8_t;using uint16=uint16_t;using uint32=uint32_t;using int32=int32_t;
using WeaponAttackType=int;using SpellSchoolMask=uint32;using SpellDmgClass=int;
#define DEBUG_FILTER_LOG(...) (void)0
enum{BASE_ATTACK,OFF_ATTACK,RANGED_ATTACK};
enum{TYPEID_PLAYER,TYPEID_UNIT};
enum{CLASS_WARRIOR=1,CLASS_PALADIN=2,CLASS_HUNTER=3,CLASS_ROGUE=4,CLASS_PRIEST=5,CLASS_SHAMAN=7,CLASS_MAGE=8,CLASS_WARLOCK=9,CLASS_DRUID=11};
enum{STAT_AGILITY,UNIT_FIELD_FLAGS,UNIT_FLAG_PLAYER_CONTROLLED,SPELL_AURA_MOD_ATTACKER_RANGED_HIT_CHANCE,SPELL_AURA_MOD_ATTACKER_MELEE_HIT_CHANCE,SCRIPT_LOCATION_CRIT_CHANCE,CURRENT_MELEE_SPELL,CLASSMASK_WAND_USERS,SPELL_DAMAGE_CLASS_RANGED,SPELL_DAMAGE_CLASS_MELEE,SPELL_DAMAGE_CLASS_MAGIC,SPELL_AURA_MOD_DODGE_PERCENT,PLAYER_DODGE_PERCENTAGE};
enum{UNIT_FIELD_MINDAMAGE,UNIT_FIELD_MAXDAMAGE,UNIT_FIELD_MINRANGEDDAMAGE,UNIT_FIELD_MAXRANGEDDAMAGE,UNIT_FIELD_MINOFFHANDDAMAGE,UNIT_FIELD_MAXOFFHANDDAMAGE};
enum{SPELL_SCHOOL_MASK_NORMAL=1,SPELL_SCHOOL_NORMAL=0,DIRECT_DAMAGE=0,EFFECT_INDEX_0=0,SPELL_PARTIAL_RESIST_PCT_100=0};
// PINNED_ENUMS
// PINNED_DIE
struct SpellEntry{int DmgClass=0;};struct Spell{SpellEntry* m_spellInfo=nullptr;};
bool IsSpellUseWeaponSkill(const SpellEntry*){return true;}bool IsPositiveSpell(const SpellEntry*,const void*,const void*){return false;}bool IsNextMeleeSwingSpell(const SpellEntry*){return false;}
struct Aura{bool isAffectedOnSpell(const SpellEntry*){return false;}void OnCritChanceCalculate(const void*,float&,const SpellEntry*){}};
using SpellPartialResistChanceEntry=std::array<uint32,1>;static std::array<SpellPartialResistChanceEntry,1> SPELL_PARTIAL_RESIST_DISTRIBUTION{};
struct Combat{bool IsInEvadeMode()const{return false;}};
static std::vector<uint32> rolls;static size_t cursor;
uint32 urand(uint32 min,uint32 max){assert(cursor<rolls.size());uint32 v=rolls[cursor++];assert(v>=min&&v<=max);return v;}
struct Unit;using WorldObject=Unit;
struct SubDamageInfo{uint32 damageSchoolMask=1,damage=0,absorb=0;int32 resist=0;};
struct CalcDamageInfo{Unit* attacker=nullptr;Unit* target=nullptr;int attackType=0;uint32 totalDamage=0,cleanDamage=0,absorb=0,blocked_amount=0;uint32 TargetState=0,HitInfo=0,procAttacker=0,procVictim=0,procEx=0;MeleeHitOutcome hitOutCome=MELEE_HIT_EVADE;SubDamageInfo subDamage[1];};
// PINNED_FLAGS
struct Unit{
 int type=TYPEID_UNIT;uint32 level=1,defense=0;int32 armor=0;bool standing=true,casting=false,noCrush=false;float minDamage=0,maxDamage=5,dodge=0,parry=0,block=0;
 struct Weapon{int lines=1;};struct DamageInfo{Weapon weapon[3];}m_weaponDamageInfo;
 Spell* m_currentSpells[1]={nullptr};
 int GetTypeId()const{return type;}bool HasFlag(int,int)const{return type==TYPEID_PLAYER;}
 uint32 GetSkillMaxForLevel(const Unit* =nullptr)const{return level*5;}uint32 GetWeaponSkillValue(int,const Unit*)const{return level*5;}uint32 GetDefenseSkillValue(const Unit* =nullptr)const{return defense;}
 bool IsAlive()const{return true;}bool IsUnit()const{return true;}int32 GetArmor()const{return armor;}uint32 GetLevel()const{return level;}int GetResistancePenetration(int)const{return 0;}
 float GetDodgeChance()const{return dodge;}float GetParryChance()const{return parry;}float GetBlockChance()const{return block;}
 float GetCritChance(int)const{return 5.f;}float GetCritChance(const SpellEntry*,uint32)const{return 5.f;}
 float GetMissChance(int)const{return 5.f;}float GetMissChance(const SpellEntry*,uint32)const{return 5.f;}
 float GetHitChance(int)const{return 0.f;}float GetHitChance(const SpellEntry*,uint32)const{return 0.f;}
 float GetCritTakenChance(uint32,int)const{return 0.f;}int GetTotalAuraModifier(int)const{return 0;}
 bool hasOffhandWeaponForAttack()const{return false;}float CalculateEffectiveMagicResistancePercent(const Unit*,uint32)const{return 0.f;}
 uint32 GetMeleeDamageSchoolMask(bool=true,bool=false)const{return 1;}std::vector<Aura*> GetScriptedLocationAuras(int)const{return {};}
 Combat GetCombatManager()const{return {};}bool IsStandState()const{return standing;}bool CanCrit(int)const{return true;}bool CanReactInCombat()const{return true;}
 bool CanDodgeInCombat(const Unit*)const{return !casting&&dodge>=.005f;}bool CanParryInCombat(const Unit*)const{return false;}bool CanBlockInCombat(const Unit*,uint32)const{return false;}
 bool CanGlanceInCombat(const Unit*)const{return false;}bool CanCrushInCombat(const Unit*)const{return !noCrush;}
 float CalculateEffectiveGlanceChance(const Unit*,int)const{return 0;}
 float GetFloatValue(int field)const{return field==UNIT_FIELD_MINDAMAGE?minDamage:maxDamage;}
 uint32 GetWeaponDamageSchool(int,int)const{return 0;}bool IsImmuneToDamage(uint32)const{return false;}
 uint32 MeleeDamageBonusDone(Unit*,uint32 damage,int,uint32,const void*,int,int,int,bool){return damage;}
 uint32 MeleeDamageBonusTaken(Unit*,uint32 damage,int,uint32,const void*,int,int,int,bool){return damage;}
 uint32 GetShieldBlockValue()const{return 0;}void CalculateGlanceAmount(CalcDamageInfo*)const{}
 void CalculateDamageAbsorbAndResist(Unit*,uint32,int,uint32,uint32* absorb,int32* resist,bool){*absorb=0;*resist=0;}
 uint32 GetCreatureTypeMask()const{return 0;}float GetCritMultiplier(uint32,uint32)const{return 2.f;}float GetCritTakenMultiplier(uint32,int,float)const{return 1.f;}
 float CalculateEffectiveMissChance(const Unit*,int,const SpellEntry* =nullptr)const;
 float CalculateEffectiveDodgeChance(const Unit*,int,const SpellEntry* =nullptr)const;
 float CalculateEffectiveParryChance(const Unit*,int,const SpellEntry* =nullptr)const;
 float CalculateEffectiveBlockChance(const Unit*,int,const SpellEntry* =nullptr)const;
 float CalculateEffectiveCritChance(const Unit*,int,const SpellEntry* =nullptr)const;
 float CalculateEffectiveCrushChance(const Unit*,int)const;
 MeleeHitOutcome RollMeleeOutcomeAgainst(const Unit*,int,uint32)const;
 uint32 CalculateDamage(int,bool,uint8);
 void CalculateMeleeDamage(Unit*,CalcDamageInfo*,int=BASE_ATTACK);
 uint32 CalculateCritAmount(CalcDamageInfo*)const;
 static float CalcArmorReducedDamage(WorldObject*,Unit*,float);
};
struct Player:Unit{uint32 agility=0;Player(){type=TYPEID_PLAYER;}int getClass()const{return CLASS_MAGE;}float GetStat(int)const{return float(agility);}float GetDodgeFromAgility(float)const;void CalculateMinMaxDamage(int,bool,float&,float&,uint8){assert(false);}};
uint32 GetSchoolMask(uint32 n){return 1u<<n;}

using UnitMods=int;
enum{STAT_STRENGTH=50,UNIT_MOD_DAMAGE_MAINHAND,UNIT_MOD_DAMAGE_OFFHAND,UNIT_MOD_DAMAGE_RANGED,UNIT_MOD_ATTACK_POWER,UNIT_MOD_ATTACK_POWER_RANGED,BASE_VALUE,BASE_PCT,TOTAL_VALUE,TOTAL_PCT,MINDAMAGE,MAXDAMAGE,UNIT_FIELD_ATTACK_POWER,UNIT_FIELD_ATTACK_POWER_MODS,UNIT_FIELD_ATTACK_POWER_MULTIPLIER,UNIT_FIELD_RANGED_ATTACK_POWER,UNIT_FIELD_RANGED_ATTACK_POWER_MODS,UNIT_FIELD_RANGED_ATTACK_POWER_MULTIPLIER};
enum class AttackPowerMod{MELEE_ATTACK_POWER,RANGED_ATTACK_POWER};enum class AttackPowerModSign{MOD_SIGN_POS,MOD_SIGN_NEG};
struct Creature:Unit{
 int classId=CLASS_WARRIOR;uint32 strength=0,agility=0,attackPower=0;float createStrength=0,createAgility=0,m_baseAP=0,m_baseRAP=0,m_damageMultiplier=1,weaponMin=0,weaponMax=0,apValue=0;bool unusable=false;
 int m_attackPowerMod[2][2]={{0,0},{0,0}};
 int getClass()const{return classId;}float GetStat(int field)const{return field==STAT_STRENGTH?float(strength):float(agility);}float GetCreateStat(int field)const{return field==STAT_STRENGTH?createStrength:createAgility;}
 void SetModifierValue(int,int,float value){apValue=value;}
 float GetModifierValue(int unit,int slot)const{return slot==BASE_PCT||slot==TOTAL_PCT?1.f:(slot==BASE_VALUE&&(unit==UNIT_MOD_ATTACK_POWER||unit==UNIT_MOD_ATTACK_POWER_RANGED)?apValue:0.f);}
 void SetInt32Value(int,uint32 value){attackPower=value;}void SetInt16Value(int,int,int){}void SetFloatValue(int,float){}
 float GetTotalAttackPowerValue(int)const{return float(attackPower);}float GetBaseWeaponDamage(int,int range)const{return range==MINDAMAGE?weaponMin:weaponMax;}
 float GetConditionalTotalPhysicalDamageModifier(int)const{return unusable?.5f:1.f;}
 void SetStatFloatValue(int field,float value){if(field==UNIT_FIELD_MINDAMAGE)minDamage=value;if(field==UNIT_FIELD_MAXDAMAGE)maxDamage=value;}
 void UpdateAttackPowerAndDamage(bool);void UpdateDamagePhysical(int);
};
// PINNED_METHODS
uint32 bits(float n){uint32 b;std::memcpy(&b,&n,4);return b;}
float from_bits(uint32 b){float n;std::memcpy(&n,&b,4);return n;}
// MT19937 output is specified; explicit rejection sampling avoids modulo bias
// and a platform-specific uniform_int_distribution implementation.
struct CaseRandom {
    std::mt19937 engine{0x72a19e84};
    uint32 operator()(uint32 bound) {
        assert(bound != 0);
        const uint32 threshold = uint32(0u - bound) % bound;
        uint32 value;
        do { value = engine(); } while (value < threshold);
        return value % bound;
    }
};

std::array<uint32, 4> source_chances(Unit& a, Player& p) {
    p.dodge = 3.25f + p.GetDodgeFromAgility(p.GetStat(STAT_AGILITY));
    return {
        chance_u(a.CalculateEffectiveMissChance(&p, BASE_ATTACK)),
        p.standing && !p.casting ? chance_u(p.CalculateEffectiveDodgeChance(&a, BASE_ATTACK)) : 0,
        p.standing ? chance_u(a.CalculateEffectiveCritChance(&p, BASE_ATTACK)) : 10000,
        p.standing && !a.noCrush ? chance_u(a.CalculateEffectiveCrushChance(&p, BASE_ATTACK)) : 0,
    };
}

void emit_melee(Unit& a, Player& p, uint32 damage, uint32 outcome_roll) {
    const auto chances = source_chances(a, p);
    rolls = {damage, outcome_roll};
    cursor = 0;
    CalcDamageInfo result;
    a.CalculateMeleeDamage(&p, &result);
    assert(cursor == 2); // Includes singleton ranges and misses/dodges.
    std::cout << "m " << a.level << ' ' << p.level << ' ' << p.defense << ' '
              << p.agility << ' ' << p.armor << ' ' << p.standing << ' '
              << p.casting << ' ' << a.noCrush << ' ' << bits(a.minDamage) << ' '
              << bits(a.maxDamage) << ' ' << damage << ' ' << outcome_roll;
    for (uint32 chance : chances) std::cout << ' ' << chance;
    std::cout << ' ' << result.hitOutCome << ' ' << result.totalDamage << ' '
              << result.cleanDamage << '\n';
}

void emit_boundaries(Unit& a, Player& p) {
    std::vector<uint32> outcome_rolls{1, 10000};
    uint32 cumulative = 0;
    for (uint32 chance : source_chances(a, p)) {
        cumulative += chance;
        for (uint32 roll : {cumulative, cumulative + 1})
            if (roll >= 1 && roll <= 10000) outcome_rolls.push_back(roll);
    }
    std::sort(outcome_rolls.begin(), outcome_rolls.end());
    outcome_rolls.erase(std::unique(outcome_rolls.begin(), outcome_rolls.end()), outcome_rolls.end());
    float lo = std::min(a.minDamage, a.maxDamage);
    float hi = std::max(a.minDamage, a.maxDamage);
    if (hi == 0) hi = 5;
    std::vector<uint32> damage_rolls{uint32(lo), uint32(hi)};
    if (damage_rolls.front() == damage_rolls.back()) damage_rolls.pop_back();
    for (uint32 damage : damage_rolls)
        for (uint32 roll : outcome_rolls) emit_melee(a, p, damage, roll);
}

int main() {
    assert(std::fesetround(FE_TONEAREST) == 0);
    Player p;
    Unit a;
    for (uint32 level = 1; level <= 60; level++)
        for (uint32 agi : {0u, 1u, 5u, 17u, 30u, 101u, 1000000u}) {
            p.level = level;
            p.agility = agi;
            float value = 3.25f;
            value += p.GetDodgeFromAgility(p.GetStat(STAT_AGILITY));
            std::cout << "d " << level << ' ' << agi << ' ' << bits(value) << '\n';
        }

    CaseRandom draw;
    for (uint32 row = 0; row < 5000; row++) {
        a.level = 1 + draw(63);
        p.level = 1 + draw(60);
        p.defense = draw(400);
        p.agility = draw(80);
        p.armor = int32(draw(10000)) - 100;
        p.standing = draw(5) != 0;
        p.casting = draw(3) == 0;
        a.noCrush = draw(4) == 0;
        a.minDamage = float(draw(1000)) / 16.f;
        a.maxDamage = float(draw(1000)) / 16.f;
        if (row % 101 == 0) a.minDamage = a.maxDamage = 0;
        float lo = std::min(a.minDamage, a.maxDamage);
        float hi = std::max(a.minDamage, a.maxDamage);
        if (hi == 0) hi = 5;
        const uint32 damage = uint32(lo) + draw(uint32(hi) - uint32(lo) + 1);
        const uint32 roll = 1 + draw(10000);
        emit_melee(a, p, damage, roll);
    }

    // Every admitted mage level, exact <= bucket seams, crushing enabled.
    for (uint32 level = 1; level <= 60; level++) {
        a = Unit{};
        p = Player{};
        a.level = std::min(63u, level + 3);
        p.level = level;
        p.defense = level * 5;
        p.agility = 17;
        a.minDamage = 0;
        a.maxDamage = 5;
        emit_boundaries(a, p);
    }

    // Deficit 14/15/16, cap versus current defense, crit/dodge suppression and
    // pushed-off-table outcomes. Both no-crushing states and sit/cast branches.
    for (uint32 defense : {0u, 1u, 284u, 285u, 286u, 300u, 301u, 65535u})
        for (bool no_crush : {false, true})
            for (bool standing : {false, true})
                for (bool casting : {false, true}) {
                    a = Unit{};
                    p = Player{};
                    a.level = p.level = 60;
                    a.noCrush = no_crush;
                    a.minDamage = 2.1f;
                    a.maxDamage = 9.99f;
                    p.defense = defense;
                    p.agility = 17;
                    p.standing = standing;
                    p.casting = casting;
                    emit_boundaries(a, p);
                }

    // Armor zero/minimum/cap, zero and singleton ranges, odd crushing damage,
    // high exact integers and both ends of reversed fractional ranges.
    for (int32 armor : {-1000000, 0, 1000000})
        for (float damage : {0.f, 1.f, 2.f, 3.f, 999999.f, 1000000.f}) {
            a = Unit{};
            p = Player{};
            a.level = 63;
            a.minDamage = a.maxDamage = damage;
            p.level = 60;
            p.defense = 300;
            p.agility = 17;
            p.armor = armor;
            emit_boundaries(a, p);
        }
    a.minDamage = 9.99f;
    a.maxDamage = 2.1f;
    emit_boundaries(a, p);
    p.agility = 1000000;
    emit_boundaries(a, p);

    for (int klass : {CLASS_WARRIOR, CLASS_PALADIN, CLASS_ROGUE, CLASS_MAGE})
        for (uint32 row = 0; row < 250; row++) {
            Creature c;
            c.classId = klass;
            c.strength = 20 + draw(80);
            c.agility = 20 + draw(80);
            c.createStrength = float(draw(80)) / 4.f;
            c.createAgility = float(draw(80)) / 4.f;
            c.m_baseAP = float(draw(1000)) / 16.f;
            c.m_damageMultiplier = float(1 + draw(200)) / 32.f;
            c.weaponMin = float(draw(300)) / 16.f;
            c.weaponMax = float(draw(300)) / 16.f;
            c.unusable = row % 2;
            c.UpdateAttackPowerAndDamage(false);
            std::cout << "w " << klass << ' ' << c.strength << ' ' << c.agility << ' '
                      << bits(c.createStrength) << ' ' << bits(c.createAgility) << ' '
                      << bits(c.m_baseAP) << ' ' << bits(c.m_damageMultiplier) << ' '
                      << bits(c.weaponMin) << ' ' << bits(c.weaponMax) << ' ' << c.unusable
                      << ' ' << c.attackPower << ' ' << bits(c.minDamage) << ' '
                      << bits(c.maxDamage) << '\n';
        }

    for (uint32 maximum : {5u, 10u, 100u, 300u, 65535u})
        for (uint32 room = 1; room < 7 && room <= maximum; room++) {
            double baseChance = 0;
            const double levelCapScale = 1;
            // PINNED_SKILL_QUADRATIC
            baseChance = std::clamp(baseChance, 0.0, 1.0);
            double intellectBonus = 0;
            // PINNED_SKILL_FINAL
            std::cout << "s " << maximum << ' ' << room << ' ' << bits(finalChance) << '\n';
        }
}
