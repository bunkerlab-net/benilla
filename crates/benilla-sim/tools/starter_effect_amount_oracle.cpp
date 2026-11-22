// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic adapters for the unchanged pinned WorldObject effect-value method.
#include <array>
#include <cassert>
#include <cfenv>
#include <cstdint>
#include <iostream>
using uint8=uint8_t;using uint32=uint32_t;using int32=int32_t;using SpellEffectIndex=uint32;
constexpr int TYPEID_PLAYER=1,SPELLMOD_ALL_EFFECTS=0,SPELL_ATTR_SCALES_WITH_CREATURE_LEVEL=0;
enum {SPELL_AURA_PERIODIC_DAMAGE,SPELL_AURA_PERIODIC_LEECH,SPELL_AURA_SCHOOL_ABSORB,SPELL_AURA_POWER_BURN_MANA,SPELL_AURA_PERIODIC_MANA_LEECH};
enum {SPELL_EFFECT_SCHOOL_DAMAGE,SPELL_EFFECT_POWER_DRAIN,SPELL_EFFECT_ENVIRONMENTAL_DAMAGE,SPELL_EFFECT_HEALTH_LEECH,SPELL_EFFECT_HEAL,SPELL_EFFECT_SUMMON,SPELL_EFFECT_WEAPON_DAMAGE_NOSCHOOL,SPELL_EFFECT_WEAPON_DAMAGE,SPELL_EFFECT_POWER_BURN,SPELL_EFFECT_NORMALIZED_WEAPON_DMG};
struct SpellEntry {
 uint32 Id=900000140,EffectBaseDice[3]{},maxLevel=0,baseLevel=0,spellLevel=0,EffectApplyAuraName[3]{},Effect[3]{};
 float EffectRealPointsPerLevel[3]{},EffectDicePerLevel[3]{},EffectPointsPerComboPoint[3]{};
 int32 EffectBasePoints[3]{},EffectDieSides[3]{};
 bool HasAttribute(int)const{return false;}
};
struct Unit;struct Player;
struct WorldObject {
 virtual ~WorldObject()=default;
 int GetTypeId()const{return TYPEID_PLAYER;}
 int32 CalculateSpellEffectValue(Unit const*,SpellEntry const*,SpellEffectIndex,int32 const*,bool,bool)const;
};
struct Unit:WorldObject {
 uint32 GetSpellRank(SpellEntry const*)const{return 1;}uint32 GetLevel()const{return 1;}uint32 GetObjectGuid()const{return 0;}
 Player* GetSpellModOwner()const{return nullptr;}bool IsCreature()const{return false;}
};
struct Player:Unit {
 uint8 GetComboPoints()const{return 0;}uint32 GetComboTargetGuid()const{return 0;}
 void ApplySpellMod(uint32,int,float&,bool){assert(false);}
};
struct CreatureInfo {uint32 UnitClass=0;};
struct CreatureClassLvlStats {float BaseDamage=0;};
struct Creature:Unit {const CreatureInfo* GetCreatureInfo()const{assert(false);return nullptr;}};
struct ObjectMgr {const CreatureClassLvlStats* GetCreatureClassLvlStats(uint32,uint32)const{assert(false);return nullptr;}}sObjectMgr;
int32 irand(int32,int32){assert(false);return 0;}bool IsOnlySelfTargeting(SpellEntry const*){return true;}
// PINNED_EFFECT_VALUE
int main(){
 assert(std::fesetround(FE_TONEAREST)==0);
 std::cout<<"# synthetic original constant effect binary32 arithmetic, not game rows\n";
 const std::array<std::array<int32,2>,12> cases{{
  {{-16777217,16777218}},{{-16777219,16777220}},{{-16777216,16777217}},
  {{-16777215,16777216}},{{-2147483647,2147483647}},{{16777217,0}},
  {{16777217,1}},{{-32769,1}},{{32766,1}},{{32767,1}},{{-1,0}},{{0,1}}
 }};
 for(auto values:cases)for(int32 die_sides:{0,1}){
  SpellEntry entry;entry.EffectBasePoints[0]=values[0];entry.EffectBaseDice[0]=uint32(values[1]);entry.EffectDieSides[0]=die_sides;
  const int32 cached=values[0]+values[1];Player player;
  std::cout<<values[0]<<'\t'<<values[1]<<'\t'<<die_sides<<'\t'<<player.CalculateSpellEffectValue(&player,&entry,0,&cached,false,true)<<'\n';
 }
}
