// SPDX-License-Identifier: GPL-3.0-or-later
// Authored adapters/probes. Generator inserts unchanged pinned CMaNGOS bodies.
// Stubs specify the explicit zero-resistance/no-modifier ordinary-NPC domain.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <vector>
using uint8=uint8_t; using uint16=uint16_t;using uint32=uint32_t;using int32=int32_t;
using SpellSchoolMask=uint32;using SpellDmgClass=uint32;using SpellSchools=uint32;
constexpr uint32 SPELL_ATTR_EX2_CANT_CRIT=0x20000000,SPELL_ATTR_EX3_ALWAYS_HIT=1,SPELL_ATTR_EX3_NORMAL_RANGED_ATTACK=2;
constexpr uint32 SPELL_DAMAGE_CLASS_MAGIC=1,SPELL_DAMAGE_CLASS_MELEE=2,SPELL_DAMAGE_CLASS_RANGED=3,UNIT_FIELD_FLAGS=1,UNIT_FLAG_PLAYER_CONTROLLED=8;
constexpr uint32 SPELL_AURA_MOD_ATTACKER_SPELL_HIT_CHANCE=1,SCRIPT_LOCATION_CRIT_CHANCE=2;
struct SpellEntry {uint32 attributes=0,DmgClass=1;bool HasAttribute(uint32 flag)const{return attributes&flag;}};
struct Script {bool isAffectedOnSpell(const SpellEntry*)const{assert(false);return false;} void OnCritChanceCalculate(const void*,float&,const SpellEntry*)const{assert(false);}};
struct Unit {
 uint32 level=1;bool player=false;float crit=0;
 bool HasFlag(uint32,uint32)const{return player;}
 uint32 GetLevelForTarget(const Unit*)const{return level;}
 float GetMissChance(const SpellEntry*,SpellSchoolMask)const{return 4.0f;}
 float GetHitChance(const SpellEntry*,SpellSchoolMask)const{return 0.0f;}
 int32 GetTotalAuraModifierByMiscMask(uint32,SpellSchoolMask)const{return 0;}
 float CalculateEffectiveMissChance(const Unit*,uint32,const SpellEntry*)const{assert(false);return 0;}
 float CalculateEffectiveCritChance(const Unit*,uint32,const SpellEntry*)const{assert(false);return 0;}
 float GetCritChance(const SpellEntry*,SpellSchoolMask)const{return crit;}
 float GetCritTakenChance(SpellSchoolMask,SpellDmgClass)const{return 0;}
 std::vector<const Script*> GetScriptedLocationAuras(uint32)const{return {};}
 float CalculateSpellMissChance(const Unit*,SpellSchoolMask,const SpellEntry*)const;
 float CalculateSpellCritChance(const Unit*,SpellSchoolMask,const SpellEntry*)const;
};
uint32 GetWeaponAttackType(const SpellEntry*){assert(false);return 0;}
bool IsPositiveSpell(const SpellEntry*,const Unit*,const Unit*){return false;}
// PINNED_MISS
// PINNED_CRIT
// PINNED_CHANCE
float number(uint32 bits){float v;std::memcpy(&v,&bits,4);return v;}
int main(){
 std::fesetround(FE_TONEAREST);Unit caster,target;caster.player=true;
 for(uint32 c:{1,2,3,4,20})for(uint32 t:{1,2,3,4,20})for(uint32 crit:{0U,0x3ba3d709U,0x3ba3d70aU,0x3fd9999aU,0x40a00000U,0x42c80000U,0x42f00000U})for(bool disabled:{false,true}){
  caster.level=c;target.level=t;caster.crit=number(crit);SpellEntry s;s.attributes=disabled?SPELL_ATTR_EX2_CANT_CRIT:0;
  std::cout<<c<<' '<<t<<' '<<crit<<' '<<disabled<<' '<<chance_u(caster.CalculateSpellMissChance(&target,4,&s))<<' '<<chance_u(caster.CalculateSpellCritChance(&target,4,&s))<<'\n';
 }
}
