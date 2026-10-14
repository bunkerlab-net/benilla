// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic state adapters. Generator inserts unchanged pinned CMaNGOS methods.
#include <algorithm>
#include <array>
#include <cassert>
#include <cfenv>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <map>
#include <utility>
#include <vector>
using uint8 = uint8_t; using uint16 = uint16_t; using uint32 = uint32_t; using int16 = int16_t; using int32 = int32_t;
enum Stats { STAT_STRENGTH, STAT_AGILITY, STAT_STAMINA, STAT_INTELLECT, STAT_SPIRIT, MAX_STATS };
enum WeaponAttackType { BASE_ATTACK, OFF_ATTACK, RANGED_ATTACK, MAX_ATTACK };
enum WeaponDamageRange { MINDAMAGE, MAXDAMAGE };
enum SpellSchools { SPELL_SCHOOL_NORMAL, SPELL_SCHOOL_HOLY, SPELL_SCHOOL_FIRE, SPELL_SCHOOL_NATURE, SPELL_SCHOOL_FROST, SPELL_SCHOOL_SHADOW, SPELL_SCHOOL_ARCANE, MAX_SPELL_SCHOOL };
using SpellSchoolMask = uint32; using Powers = uint32;
SpellSchoolMask GetSchoolMask(uint32 school) { return 1u << school; }
enum UnitMods { UNIT_MOD_STAT_START = 0, UNIT_MOD_STAT_STRENGTH = 0, UNIT_MOD_STAT_AGILITY, UNIT_MOD_STAT_STAMINA, UNIT_MOD_STAT_INTELLECT, UNIT_MOD_STAT_SPIRIT, UNIT_MOD_HEALTH, UNIT_MOD_MANA, UNIT_MOD_ARMOR, UNIT_MOD_RESISTANCE_HOLY, UNIT_MOD_RESISTANCE_FIRE, UNIT_MOD_RESISTANCE_NATURE, UNIT_MOD_RESISTANCE_FROST, UNIT_MOD_RESISTANCE_SHADOW, UNIT_MOD_RESISTANCE_ARCANE, UNIT_MOD_ATTACK_POWER, UNIT_MOD_ATTACK_POWER_RANGED, UNIT_MOD_DAMAGE_MAINHAND, UNIT_MOD_DAMAGE_OFFHAND, UNIT_MOD_DAMAGE_RANGED, UNIT_MOD_END };
enum UnitModifierType { BASE_VALUE, BASE_PCT, TOTAL_VALUE, TOTAL_PCT };
enum BaseModGroup { CRIT_PERCENTAGE, OFFHAND_CRIT_PERCENTAGE, RANGED_CRIT_PERCENTAGE, SHIELD_BLOCK_VALUE };
enum BaseModType { FLAT_MOD, PCT_MOD };
enum class AttackPowerMod { MELEE_ATTACK_POWER, RANGED_ATTACK_POWER };
enum class AttackPowerModSign { MOD_SIGN_POS, MOD_SIGN_NEG };
enum ShapeshiftForm { FORM_NONE, FORM_CAT, FORM_BEAR, FORM_DIREBEAR };
constexpr uint32 CLASS_WARRIOR=1, CLASS_PALADIN=2, CLASS_HUNTER=3, CLASS_ROGUE=4, CLASS_PRIEST=5, CLASS_SHAMAN=7, CLASS_MAGE=8, CLASS_WARLOCK=9, CLASS_DRUID=11, MAX_CLASSES=12;
constexpr uint32 UNIT_FIELD_ATTACK_POWER=0, UNIT_FIELD_ATTACK_POWER_MODS=1, UNIT_FIELD_ATTACK_POWER_MULTIPLIER=2, UNIT_FIELD_RANGED_ATTACK_POWER=3, UNIT_FIELD_RANGED_ATTACK_POWER_MODS=4, UNIT_FIELD_RANGED_ATTACK_POWER_MULTIPLIER=5, UNIT_FIELD_MINDAMAGE=6, UNIT_FIELD_MAXDAMAGE=7, UNIT_FIELD_MINOFFHANDDAMAGE=8, UNIT_FIELD_MAXOFFHANDDAMAGE=9, UNIT_FIELD_MINRANGEDDAMAGE=10, UNIT_FIELD_MAXRANGEDDAMAGE=11, PLAYER_CRIT_PERCENTAGE=12, PLAYER_RANGED_CRIT_PERCENTAGE=13, PLAYER_DODGE_PERCENTAGE=14, PLAYER_PARRY_PERCENTAGE=15, PLAYER_BLOCK_PERCENTAGE=16, PLAYER_FIELD_MOD_DAMAGE_DONE_POS=20;
constexpr uint32 SPELL_AURA_MOD_BLOCK_PERCENT=1, SPELL_AURA_MOD_PARRY_PERCENT=2, SPELL_AURA_MOD_DODGE_PERCENT=3, SPELL_AURA_MOD_SPELL_CRIT_CHANCE=4, SPELL_AURA_MOD_SPELL_CRIT_CHANCE_SCHOOL=5, SPELL_AURA_MOD_HIT_CHANCE=6, SPELL_AURA_MOD_POWER_REGEN_PERCENT=7, SPELL_AURA_MOD_POWER_REGEN=8, SPELL_AURA_MOD_MANA_REGEN_INTERRUPT=9, SPELL_AURA_MOD_DAMAGE_DONE=10, SPELL_AURA_MOD_SPELL_DAMAGE_OF_STAT_PERCENT=11;
constexpr uint32 POWER_MANA=0, TYPEID_PLAYER=4, SHAPESHIFT_FLAG_AGILITY_ATTACK_BONUS=1, SKILL_UNARMED=162, SKILL_DEFENSE=95;
constexpr uint32 EQUIPMENT_SLOT_MAINHAND=15, EQUIPMENT_SLOT_OFFHAND=16, EQUIPMENT_SLOT_RANGED=17, INVENTORY_SLOT_BAG_END=23, MAX_ITEM_PROTO_STATS=10, BASE_ATTACK_TIME=2000;
constexpr int MAX_POWERS=5, MAX_ITEM_PROTO_DAMAGES=5;
constexpr float BASE_MINDAMAGE=1.f, BASE_MAXDAMAGE=2.f;
constexpr uint32 ITEM_MOD_MANA=0, ITEM_MOD_HEALTH=1, ITEM_MOD_AGILITY=3, ITEM_MOD_STRENGTH=4, ITEM_MOD_INTELLECT=5, ITEM_MOD_SPIRIT=6, ITEM_MOD_STAMINA=7, INVTYPE_2HWEAPON=17, INVTYPE_RANGED=15, INVTYPE_RANGEDRIGHT=26, INVTYPE_THROWN=25, INVTYPE_WEAPON=13, INVTYPE_WEAPONMAINHAND=21, INVTYPE_WEAPONOFFHAND=22, ITEM_SUBCLASS_WEAPON_DAGGER=15;
struct SpellShapeshiftFormEntry { uint32 flags1; };
struct Forms { const SpellShapeshiftFormEntry* LookupEntry(ShapeshiftForm) const { assert(false); return nullptr; } } sSpellShapeshiftFormStore;
struct SpellProto { int32 EquippedItemClass; uint32 EquippedItemInventoryTypeMask; };
struct Modifier { int32 m_amount, m_miscvalue; };
struct Aura { const Modifier* GetModifier() const { assert(false); return nullptr; } const SpellProto* GetSpellProto() const { assert(false); return nullptr; } };
using AuraList = std::vector<Aura*>;
struct ItemPrototype {
    struct Stat { uint32 ItemStatType=0; int32 ItemStatValue=0; } ItemStat[10];
    struct DamageRow { float DamageMin=0, DamageMax=0; uint32 DamageType=0; } Damage[5];
    uint32 Armor=0, Block=0, HolyRes=0, FireRes=0, NatureRes=0, FrostRes=0, ShadowRes=0, ArcaneRes=0, Delay=2750, InventoryType=17, SubClass=6;
    bool weapon=true;
    bool IsWeapon() const { return weapon; } bool IsRangedWeapon() const { return false; }
};
struct Item { ItemPrototype proto; bool broken=false; const ItemPrototype* GetProto() const { return &proto; } uint32 GetSkill() const { return 136; } bool IsFitToSpellRequirements(const SpellProto*) const { assert(false); return false; } };
struct Object {
    uint32 fields[40]{};
    uint32 GetUInt32Value(uint32 i) const { return fields[i]; }
    int32 GetInt32Value(uint32 i) const { return int32(fields[i]); }
    int16 GetInt16Value(uint32 i, size_t half) const { return int16(fields[i] >> (half*16)); }
    float GetFloatValue(uint32 i) const { float v; std::memcpy(&v,&fields[i],4); return v; }
    void SetUInt32Value(uint32 i,uint32 v) { fields[i]=v; }
    void SetInt32Value(uint32 i,int32 v) { fields[i]=uint32(v); }
    void SetInt16Value(uint32 i,size_t half,int32 v) { fields[i]=(fields[i]&~(0xffffu<<(half*16)))|(uint32(uint16(v))<<(half*16)); }
    void SetFloatValue(uint32 i,float v) { std::memcpy(&fields[i],&v,4); }
    void SetStatFloatValue(uint16,float); void SetStatInt32Value(uint16,int32);
};
struct Player;
struct Unit : Object {
    uint32 level=1;
    float create[5]{}, attributes[5]{}, m_auraModifiersGroup[UNIT_MOD_END][4]{};
    float m_attackPowerMod[2][2]{}, m_modAttackBaseDPSPct[3]{1,1,1};
    float m_modCritChance[3]{}, m_modSpellCritChance[7]{}, m_modWeaponHitChance[3]{};
    float m_modDodgeChance=0, m_modParryChance=0, m_modBlockChance=0, m_modManaRegen=0, m_modManaRegenInterrupt=0;
    uint32 attack_time[3]{2000,2000,2000};
    struct WeaponDamageInfo { struct Weapon { struct Damage { float value[2]{1,2}; SpellSchools school=SPELL_SCHOOL_NORMAL; } damage[5]; uint32 lines=1; } weapon[3]; } m_weaponDamageInfo;
    Unit() { for(auto& row:m_auraModifiersGroup) row[BASE_PCT]=row[TOTAL_PCT]=1; }
    uint32 GetLevel() const { return level; } uint32 getClass() const { return CLASS_MAGE; } uint32 GetTypeId() const { return TYPEID_PLAYER; }
    float GetStat(Stats s) const { return attributes[s]; } float GetCreateStat(Stats s) const { return create[s]; }
    void SetStat(Stats s,int32 v) { attributes[s]=float(std::max(v,0)); }
    float GetModifierValue(UnitMods m,UnitModifierType t) const { return m_auraModifiersGroup[m][t]; }
    void SetModifierValue(UnitMods m,UnitModifierType t,float v) { m_auraModifiersGroup[m][t]=v; }
    uint32 GetAttackTime(WeaponAttackType a) const { return attack_time[a]; }
    void SetAttackTime(WeaponAttackType a,uint32 v) { attack_time[a]=v; }
    bool IsAttackSpeedOverridenShapeShift() const { return false; }
    ShapeshiftForm GetShapeshiftForm() const { return FORM_NONE; }
    bool CanUseEquippedWeapon(WeaponAttackType) const { return true; }
    bool hasOffhandWeaponForAttack() const { return false; }
    void SetBaseWeaponDamage(WeaponAttackType a,WeaponDamageRange r,float v,uint8 i=0) { m_weaponDamageInfo.weapon[a].damage[i].value[r]=v; }
    void SetWeaponDamageSchool(WeaponAttackType a,SpellSchools s,uint8 i) { m_weaponDamageInfo.weapon[a].damage[i].school=s; }
    const AuraList& GetAurasByType(uint32) const { static const AuraList empty; return empty; }
    int32 GetTotalAuraModifier(uint32) const { return 0; }
    int32 GetTotalAuraModifierByMiscMask(uint32,uint32) const { return 0; }
    int32 GetTotalAuraModifierByMiscValue(uint32,uint32) const { return 0; }
    float GetTotalAuraMultiplierByMiscValue(uint32,uint32) const { return 1.f; }
    float GetTotalStatValue(Stats) const;
    float GetTotalAttackPowerValue(WeaponAttackType) const;
    float GetBaseWeaponDamage(WeaponAttackType,WeaponDamageRange,uint8=0) const;
    float GetAPMultiplier(WeaponAttackType,bool);
    float OCTRegenMPPerSpirit() const;
    int32 SpellBaseDamageBonusDone(SpellSchoolMask);
};
struct Player : Unit {
    Item item;
    bool has_weapon=true;
    float m_auraBaseMod[4][2]{};
    uint16 defense=4, weapon_skill=4, unarmed_skill=2;
    uint32 GetSkillMaxForLevel() const { return level*5; }
    uint32 GetDefenseSkillValue() const { return defense; }
    uint32 GetWeaponSkillValue(WeaponAttackType a) const { return a==BASE_ATTACK ? (has_weapon&&!item.broken ? weapon_skill : unarmed_skill) : 0; }
    Item* GetWeaponForAttack(WeaponAttackType a,bool=true,bool=true) { return a==BASE_ATTACK&&has_weapon&&!item.broken ? &item : nullptr; }
    bool CanDualWield() const { return false; } bool CanBlock() const { return false; } bool CanParry() const { return false; }
    uint32 GetEnchantmentModifier(WeaponAttackType) const { return 0; }
    std::pair<float,float> GetAmmoDPS() const { return {0.f,0.f}; }
    float GetTotalPercentageModValue(BaseModGroup g) const { return m_auraBaseMod[g][FLAT_MOD]+m_auraBaseMod[g][PCT_MOD]; }
    void SetBaseModValue(BaseModGroup g,BaseModType t,float v) { m_auraBaseMod[g][t]=v; }
    bool CanModifyStats() const { return true; }
    void ApplyStatBuffMod(Stats,float,bool) {}
    void HandleBaseModValue(BaseModGroup,BaseModType,float,bool) { assert(false); }
    void HandleStatModifier(UnitMods m,UnitModifierType t,float value,bool apply) {
        assert(t==BASE_VALUE); m_auraModifiersGroup[m][t]+=apply?value:-value;
        if(m<5) UpdateStats(Stats(m));
    }
    // Non-ancillary callbacks are intentionally outside this oracle's output.
    void UpdateArmor() {} void UpdateMaxHealth() {} void UpdateMaxPower(uint32) {} void UpdateResistances(uint32) {}
    float GetMeleeCritFromAgility() const; float GetDodgeFromAgility(float) const; float GetSpellCritFromIntellect() const;
    void UpdateAttackPowerAndDamage(bool=false); void CalculateMinMaxDamage(WeaponAttackType,bool,float&,float&,uint8=0); void UpdateDamagePhysical(WeaponAttackType);
    void UpdateAllCritPercentages(); void UpdateCritPercentage(WeaponAttackType);
    void UpdateDefenseBonusesMod(); void UpdateBlockPercentage(); void UpdateParryPercentage(); void UpdateDodgePercentage();
    void UpdateAllSpellCritChances(); void UpdateSpellCritChance(uint32);
    void UpdateSpellDamageBonus(); void UpdateManaRegen(); void UpdateWeaponHitChances(WeaponAttackType);
    bool UpdateStats(Stats); bool UpdateAllStats(); void _ApplyItemBonuses(const ItemPrototype*,uint8,bool);
};
// PINNED_BODIES
uint32 bits(float f) { uint32 b; std::memcpy(&b,&f,4); return b; }
void output(Player& p,uint32 mode,uint32 stage) {
    std::vector<uint32> v{p.GetUInt32Value(UNIT_FIELD_ATTACK_POWER),p.GetUInt32Value(UNIT_FIELD_RANGED_ATTACK_POWER),bits(p.GetTotalAttackPowerValue(BASE_ATTACK)),bits(p.GetTotalAttackPowerValue(RANGED_ATTACK))};
    for(uint32 i=UNIT_FIELD_MINDAMAGE;i<=UNIT_FIELD_MAXRANGEDDAMAGE;++i) v.push_back(p.fields[i]);
    for(uint32 i=0;i<3;++i) v.push_back(bits(p.m_auraBaseMod[i][PCT_MOD]));
    for(float f:p.m_modCritChance) v.push_back(bits(f));
    v.push_back(p.fields[PLAYER_CRIT_PERCENTAGE]); v.push_back(p.fields[PLAYER_RANGED_CRIT_PERCENTAGE]);
    for(float f:p.m_modSpellCritChance) v.push_back(bits(f));
    v.push_back(bits(p.m_modDodgeChance));v.push_back(p.fields[PLAYER_DODGE_PERCENTAGE]);
    v.push_back(bits(p.m_modParryChance));v.push_back(p.fields[PLAYER_PARRY_PERCENTAGE]);
    v.push_back(bits(p.m_modBlockChance));v.push_back(p.fields[PLAYER_BLOCK_PERCENTAGE]);
    for(uint32 i=0;i<7;++i) v.push_back(p.fields[PLAYER_FIELD_MOD_DAMAGE_DONE_POS+i]);
    v.push_back(bits(p.m_modManaRegen));v.push_back(bits(p.m_modManaRegenInterrupt));
    for(float f:p.m_modWeaponHitChance) v.push_back(uint32(f));
    std::cout<<p.level<<'\t'<<mode<<'\t'<<stage<<'\t';
    for(size_t i=0;i<v.size();++i) std::cout<<(i?",":"")<<v[i];
    std::cout<<'\n';
}
int main() {
    assert(std::fesetround(FE_TONEAREST)==0);
    std::cout<<"# source-extracted ancillary hook and item-break fixtures; synthetic rows only\n";
    for(uint32 level=1;level<=60;++level) for(uint32 mode=0;mode<=8;++mode) {
        Player p; p.level=level;
        const int stats[5]={17,23,29,31,37};
        for(uint32 i=0;i<5;++i) p.create[i]=float(stats[i]+level*(i==3?2:1));
        ItemPrototype& proto=p.item.proto;
        proto.Damage[0]={4.25f,7.75f,0};
        proto.ItemStat[0]={mode==1||mode==3||mode==8?3u:mode==2?5u:mode==4?4u:mode==5?6u:mode==6?7u:0u,mode==0||mode==7?0:5};
        if(mode==8) { proto.ItemStat[1]={5,7};proto.ItemStat[2]={3,3}; }
        if(mode==7) proto.Delay=0;
        for(const auto& s:proto.ItemStat) if(s.ItemStatValue) {
            uint32 index=s.ItemStatType==3?1:s.ItemStatType==4?0:s.ItemStatType==5?3:s.ItemStatType==6?4:2;
            p.m_auraModifiersGroup[index][BASE_VALUE]+=float(s.ItemStatValue);
        }
        p.SetBaseWeaponDamage(BASE_ATTACK,MINDAMAGE,4.25f);p.SetBaseWeaponDamage(BASE_ATTACK,MAXDAMAGE,7.75f);
        p.SetAttackTime(BASE_ATTACK,proto.Delay?proto.Delay:2000);
        p.UpdateAllStats();output(p,mode,0);
        ItemPrototype armor=proto; armor.weapon=false;
        p._ApplyItemBonuses(mode==3?&armor:&proto,mode==3?4:15,false);
        if(mode!=3) p.item.broken=true;
        output(p,mode,1);
        p.UpdateAllStats();output(p,mode,2);
    }
}
