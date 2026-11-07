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
static std::vector<uint32> rolls;static size_t cursor;static std::vector<std::pair<uint32,uint32>> requests;
uint32 urand(uint32 min,uint32 max){assert(cursor<rolls.size());requests.emplace_back(min,max);if(cursor==2)rolls[cursor]=rolls[cursor]==UINT32_MAX?max:min;uint32 v=rolls[cursor++];assert(v>=min&&v<=max);return v;}
struct Unit;using WorldObject=Unit;
struct SubDamageInfo{uint32 damageSchoolMask=1,damage=0,absorb=0;int32 resist=0;};
struct CalcDamageInfo{Unit* attacker=nullptr;Unit* target=nullptr;int attackType=0;uint32 totalDamage=0,cleanDamage=0,absorb=0,blocked_amount=0;uint32 TargetState=0,HitInfo=0,procAttacker=0,procVictim=0,procEx=0;MeleeHitOutcome hitOutCome=MELEE_HIT_EVADE;SubDamageInfo subDamage[1];};
// PINNED_FLAGS
struct Unit{
 int type=TYPEID_UNIT;uint32 level=1,defense=0;int32 armor=0;bool standing=true,casting=false,noCrush=false;float minDamage=0,maxDamage=5,dodge=5,parry=5,block=5,crit=5,hit=0;uint32 weaponSkill=5,shieldBlock=0;bool react=true,allowDodge=true,allowParry=true,allowBlock=true,front=true;
 struct Weapon{int lines=1;};struct DamageInfo{Weapon weapon[3];}m_weaponDamageInfo;
 Spell* m_currentSpells[1]={nullptr};
 int GetTypeId()const{return type;}bool HasFlag(int,int)const{return type==TYPEID_PLAYER;}
 uint32 GetSkillMaxForLevel(const Unit* =nullptr)const{return level*5;}uint32 GetWeaponSkillValue(int,const Unit*)const{return weaponSkill;}uint32 GetDefenseSkillValue(const Unit* =nullptr)const{return level*5;}
 bool IsAlive()const{return true;}bool IsUnit()const{return true;}int32 GetArmor()const{return armor;}uint32 GetLevel()const{return level;}int GetResistancePenetration(int)const{return 0;}
 float GetDodgeChance()const{return dodge;}float GetParryChance()const{return parry;}float GetBlockChance()const{return block;}
 float GetCritChance(int)const{return crit;}float GetCritChance(const SpellEntry*,uint32)const{return 5.f;}
 float GetMissChance(int)const{return 5.f;}float GetMissChance(const SpellEntry*,uint32)const{return 5.f;}
 float GetHitChance(int)const{return hit;}float GetHitChance(const SpellEntry*,uint32)const{return 0.f;}
 float GetCritTakenChance(uint32,int)const{return 0.f;}int GetTotalAuraModifier(int)const{return 0;}
 bool hasOffhandWeaponForAttack()const{return false;}float CalculateEffectiveMagicResistancePercent(const Unit*,uint32)const{return 0.f;}
 uint32 GetMeleeDamageSchoolMask(bool=true,bool=false)const{return 1;}std::vector<Aura*> GetScriptedLocationAuras(int)const{return {};}
 Combat GetCombatManager()const{return {};}bool IsStandState()const{return standing;}bool CanCrit(int)const{return true;}bool CanReactInCombat()const{return react;}
 // These exact bounded eligibility branches supply resolved ordinary NPC flags.
 bool CanDodgeInCombat(const Unit*)const{return allowDodge&&react&&!casting&&dodge>=.005f;}
 bool CanParryInCombat(const Unit*)const{return allowParry&&react&&!casting&&front&&parry>=.005f;}
 bool CanBlockInCombat(const Unit*,uint32 mask)const{return mask==1&&allowBlock&&react&&!casting&&front&&block>=.005f;}
 bool CanGlanceInCombat(const Unit* victim)const{return type==TYPEID_PLAYER&&victim->type==TYPEID_UNIT&&victim->level>10;}bool CanCrushInCombat(const Unit*)const{return false;}
 float CalculateEffectiveGlanceChance(const Unit*,int)const;uint32 GetLevelForTarget(const Unit*)const{return level;}int getClassMask()const{return CLASSMASK_WAND_USERS;}
 float GetFloatValue(int field)const{return field==UNIT_FIELD_MINDAMAGE?minDamage:maxDamage;}
 uint32 GetWeaponDamageSchool(int,int)const{return 0;}bool IsImmuneToDamage(uint32)const{return false;}
 uint32 MeleeDamageBonusDone(Unit*,uint32 damage,int,uint32,const void*,int,int,int,bool){return damage;}
 uint32 MeleeDamageBonusTaken(Unit*,uint32 damage,int,uint32,const void*,int,int,int,bool){return damage;}
 uint32 GetShieldBlockValue()const{return shieldBlock;}uint32 CalculateGlanceAmount(CalcDamageInfo*)const;
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
struct Player:Unit{uint32 agility=0;Player(){type=TYPEID_PLAYER;}int getClass()const{return CLASS_MAGE;}float GetStat(int)const{return float(agility);}void CalculateMinMaxDamage(int,bool,float&,float&,uint8){assert(false);}};
uint32 GetSchoolMask(uint32 n){return 1u<<n;}

// PINNED_METHODS
uint32 bits(float n){uint32 b;std::memcpy(&b,&n,4);return b;}
struct CaseRandom {
 std::mt19937 engine{0x72a19e84};
 uint32 operator()(uint32 bound){assert(bound);const uint32 threshold=uint32(0u-bound)%bound;uint32 value;do{value=engine();}while(value<threshold);return value%bound;}
};
std::array<uint32,6> chances(Player& a, Unit& d){
 return {chance_u(a.CalculateEffectiveMissChance(&d,BASE_ATTACK)),
 d.CanDodgeInCombat(&a)?chance_u(d.CalculateEffectiveDodgeChance(&a,BASE_ATTACK)):0,
 d.CanParryInCombat(&a)?chance_u(d.CalculateEffectiveParryChance(&a,BASE_ATTACK)):0,
 d.CanBlockInCombat(&a,1)?chance_u(d.CalculateEffectiveBlockChance(&a,BASE_ATTACK)):0,
 a.CanGlanceInCombat(&d)?chance_u(a.CalculateEffectiveGlanceChance(&d,BASE_ATTACK)):0,
 chance_u(a.CalculateEffectiveCritChance(&d,BASE_ATTACK))};
}
void emit(Player& a,Unit& d,uint32 damage,uint32 outcome,bool high){
 const auto c=chances(a,d);
 // Discover the conditional third draw from the actual source invocation,
 // never from a copied glance-bound formula. A provisional zero is replaced
 // by the requested low/high endpoint inside urand below.
 rolls={damage,outcome,high?UINT32_MAX:0};cursor=0;requests.clear();
 CalcDamageInfo result;a.CalculateMeleeDamage(&d,&result);
 assert(cursor==2||cursor==3);
 std::cout<<"m "<<a.level<<' '<<a.weaponSkill<<' '<<bits(a.minDamage)<<' '<<bits(a.maxDamage)<<' '<<bits(a.crit)<<' '<<bits(a.hit)
 <<' '<<d.level<<' '<<d.armor<<' '<<d.react<<' '<<d.casting<<' '<<d.allowDodge<<' '<<d.allowParry<<' '<<d.allowBlock<<' '<<d.front<<' '<<d.shieldBlock
 <<' '<<damage<<' '<<outcome<<' '<<(cursor==3?int64_t(rolls[2]):-1);
 for(auto value:c)std::cout<<' '<<value;
 std::cout<<' '<<result.hitOutCome<<' '<<result.totalDamage<<' '<<result.cleanDamage<<' '<<result.blocked_amount<<' '<<result.subDamage[0].damage
 <<' '<<cursor;
 for(size_t i=0;i<3;i++)std::cout<<' '<<(i<cursor?int64_t(requests[i].first):-1)<<' '<<(i<cursor?int64_t(requests[i].second):-1);
 std::cout<<'\n';
}
void boundaries(Player&a,Unit&d){
 std::vector<uint32> seams{1,10000};uint32 total=0;
 for(auto chance:chances(a,d)){total+=chance;for(auto roll:{total,total+1})if(roll>=1&&roll<=10000)seams.push_back(roll);}
 std::sort(seams.begin(),seams.end());seams.erase(std::unique(seams.begin(),seams.end()),seams.end());
 float low=std::min(a.minDamage,a.maxDamage),high=std::max(a.minDamage,a.maxDamage);if(high==0)high=5;
 for(uint32 damage:{uint32(low),uint32(high)})for(auto roll:seams)for(bool endpoint:{false,true})emit(a,d,damage,roll,endpoint);
}
int main(){
 assert(std::fesetround(FE_TONEAREST)==0);Player a;Unit d;CaseRandom draw;
 // Every player level; boundary skill differences around each source branch,
 // level10/11 glancing boundary, front/cast/reactivity and explicit flags.
 for(uint32 level=1;level<=60;level++)for(int difference:{-11,-1,0,1,10,11,20,21}){
 if(difference!=0&&level!=1&&level!=10&&level!=11&&level!=30&&level!=60)continue;
 a=Player{};d=Unit{};a.level=level;d.level=std::min(63u,level+2);a.weaponSkill=uint32(std::max(0,int(d.level*5)-difference));a.minDamage=2.1f;a.maxDamage=9.99f;a.crit=5.25f;d.armor=100;d.shieldBlock=3;boundaries(a,d);
 }
 // The minimum miss clamp observes the mutated skill difference: 20/21,
 // not the initial 10/11 split. Near-zero base crit returns before modifiers.
 for(uint32 skill:{279u,280u,289u,290u})for(float crit:{0.0049f,0.005f}){
 a=Player{};d=Unit{};a.level=60;a.weaponSkill=skill;a.minDamage=2;a.maxDamage=3;a.hit=100;a.crit=crit;d.level=60;d.shieldBlock=1;boundaries(a,d);
 }
 for(uint32 mask:{0u,1u,3u,5u,9u,17u,33u,61u,63u,65u,125u,127u}){
 a=Player{};d=Unit{};a.level=20;a.weaponSkill=100;a.minDamage=3;a.maxDamage=3;d.level=23;d.armor=20;d.shieldBlock=100;
 d.react=mask&1;d.casting=mask&2;d.allowDodge=mask&4;d.allowParry=mask&8;d.allowBlock=mask&16;d.front=mask&32;a.crit=(mask&64)?0.f:5.f;boundaries(a,d);
 }
 for(uint32 row=0;row<300;row++){
 a=Player{};d=Unit{};a.level=1+draw(60);a.weaponSkill=draw(401);a.minDamage=float(draw(16000))/16.f;a.maxDamage=float(draw(16000))/16.f;a.crit=float(draw(10001))/100.f;a.hit=float(draw(4001))/100.f-20.f;
 d.level=1+draw(63);d.armor=int32(draw(100000))-100;d.react=draw(4)!=0;d.casting=draw(5)==0;d.allowDodge=draw(2);d.allowParry=draw(2);d.allowBlock=draw(2);d.front=draw(2);d.shieldBlock=draw(1000);
 float low=std::min(a.minDamage,a.maxDamage),high=std::max(a.minDamage,a.maxDamage);if(high==0)high=5;
 emit(a,d,uint32(low)+draw(uint32(high)-uint32(low)+1),1+draw(10000),draw(2));
 }
 for(uint32 level:{60u})for(int32 armor:{-1000000,0,1000000})for(float damage:{0.f,1.f,2.f,3.f,999999.f,1000000.f}){
 a=Player{};d=Unit{};a.level=60;a.weaponSkill=300;a.minDamage=a.maxDamage=damage;d.level=level;d.armor=armor;d.shieldBlock=2;boundaries(a,d);
 }
}
