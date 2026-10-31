// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic adapter. Generator inserts unchanged pinned source methods.
#include <algorithm>
#include <array>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <string>
#include <vector>
struct Logger {void outError(const char*){assert(false);}} sLog;
using uint8=uint8_t;using uint32=uint32_t;using int32=int32_t;
enum Stats {STAT_STRENGTH,STAT_AGILITY,STAT_STAMINA,STAT_INTELLECT,STAT_SPIRIT,MAX_STATS};
enum SpellSchools {SPELL_SCHOOL_NORMAL,SPELL_SCHOOL_HOLY,SPELL_SCHOOL_FIRE,SPELL_SCHOOL_NATURE,SPELL_SCHOOL_FROST,SPELL_SCHOOL_SHADOW,SPELL_SCHOOL_ARCANE,MAX_SPELL_SCHOOL};
enum Powers {POWER_MANA,MAX_POWERS};
enum UnitMods {UNIT_MOD_STAT_START=0,UNIT_MOD_STAT_STRENGTH=0,UNIT_MOD_STAT_AGILITY,UNIT_MOD_STAT_STAMINA,UNIT_MOD_STAT_INTELLECT,UNIT_MOD_STAT_SPIRIT,UNIT_MOD_HEALTH,UNIT_MOD_POWER_START,UNIT_MOD_MANA=UNIT_MOD_POWER_START,UNIT_MOD_RESISTANCE_START,UNIT_MOD_ARMOR=UNIT_MOD_RESISTANCE_START,UNIT_MOD_RESISTANCE_HOLY,UNIT_MOD_RESISTANCE_FIRE,UNIT_MOD_RESISTANCE_NATURE,UNIT_MOD_RESISTANCE_FROST,UNIT_MOD_RESISTANCE_SHADOW,UNIT_MOD_RESISTANCE_ARCANE,UNIT_MOD_END};
enum UnitModifierType {BASE_VALUE,BASE_PCT,TOTAL_VALUE,TOTAL_PCT,MODIFIER_TYPE_END};
constexpr int BASE_ATTACK=0,OFF_ATTACK=1,RANGED_ATTACK=2,SPELL_AURA_MOD_RESISTANCE_OF_STAT_PERCENT=0,SPELL_SCHOOL_MASK_NORMAL=1;
struct Unit;
constexpr uint32 TYPEID_PLAYER=1,SPELL_ATTR_IS_ABILITY=0x10,MAX_ITEM_PROTO_STATS=10,INVENTORY_SLOT_BAG_END=23,SHIELD_BLOCK_VALUE=0,FLAT_MOD=0;
constexpr uint32 ITEM_MOD_MANA=0,ITEM_MOD_HEALTH=1,ITEM_MOD_AGILITY=3,ITEM_MOD_STRENGTH=4,ITEM_MOD_INTELLECT=5,ITEM_MOD_SPIRIT=6,ITEM_MOD_STAMINA=7;
struct ItemStat {uint32 ItemStatType=0;int32 ItemStatValue=0;};
struct ItemPrototype {struct ItemStat ItemStat[10];uint32 Armor=0,Block=0,HolyRes=0,FireRes=0,NatureRes=0,FrostRes=0,ShadowRes=0,ArcaneRes=0;};
struct Modifier {int32 m_miscvalue{},m_amount{};};
struct SpellEntry {bool HasAttribute(uint32)const{return false;}};
struct Aura {Modifier m_modifier;Unit* target=nullptr;Modifier* GetModifier(){return &m_modifier;}Unit* GetTarget(){return target;}const SpellEntry* GetSpellProto()const{static SpellEntry s;return &s;}void HandleModTotalPercentStat(bool,bool);};
struct Unit {
 float m_auraModifiersGroup[UNIT_MOD_END][4]{};int32 create[5]{},stats[5]{};uint32 create_health=0,create_mana=0,health=0,max_health=0,mana=0,max_mana=0;int32 resist[7]{};bool modify=false;std::string trace;
 Unit(){for(auto& m:m_auraModifiersGroup){m[BASE_PCT]=m[TOTAL_PCT]=1.f;}}
 float GetCreateStat(Stats s)const{return float(create[s]);}float GetStat(Stats s)const{return float(stats[s]);}
 void SetStat(Stats s,int32 v){stats[s]=std::max(0,v);}uint32 GetCreateHealth()const{return create_health;}uint32 GetCreatePowers(Powers)const{return create_mana;}
 uint32 GetHealth()const{return health;}uint32 GetMaxHealth()const{return max_health;}uint32 GetPower(Powers)const{return mana;}uint32 GetMaxPower(Powers)const{return max_mana;}
 int32 GetCreateResistance(SpellSchools)const{return 0;}void SetArmor(int32 v){resist[0]=v;}void SetResistance(SpellSchools s,int32 v){resist[s]=v;}
 void SetHealth(uint32 v){health=std::min(v,max_health);}void SetPower(Powers,uint32 v){mana=std::min(v,max_mana);}void SetMaxHealth(uint32 v){max_health=v;if(health>v)health=v;}void SetMaxPower(Powers,uint32 v){max_mana=v;if(mana>v)mana=v;}
 float GetModifierValue(UnitMods m,UnitModifierType k)const{return k==TOTAL_PCT&&m_auraModifiersGroup[m][k]<=0.f?0.f:m_auraModifiersGroup[m][k];}
 uint32 GetTypeId()const{return TYPEID_PLAYER;}
 virtual void RefreshStats()=0;
 void HandleStatModifier(UnitMods m,UnitModifierType k,float v,bool apply){Mod(m,k,v,apply);if(modify)RefreshStats();}
 float GetTotalStatValue(Stats)const;float GetTotalResistanceValue(SpellSchools)const;
 static float GetHealthBonusFromStamina(float);float GetHealthBonusFromStamina()const;static float GetManaBonusFromIntellect(float);float GetManaBonusFromIntellect()const;
 void UpdateMaxHealth();void UpdateMaxPower(Powers);
 void Mod(UnitMods unitMod,UnitModifierType modifierType,float amount,bool apply=true){
 // PINNED_MODIFIER
 }
};
struct Player:Unit {
 std::vector<std::array<uint32,4>> steps;
 void RefreshStats()override{UpdateAllStats();steps.push_back({health,max_health,mana,max_mana});}
 int32 input_stats[5]{},flat=0,pct=0,item_hp=0,item_mana=0,item_armor=0;int32 mode=0;
 const std::vector<Aura*>& GetAurasByType(int)const{static std::vector<Aura*> empty;return empty;}
 void UpdateArmor();bool UpdateAllStats();void _ApplyItemBonuses(const ItemPrototype*,uint8,bool);
 void ApplyStatBuffMod(Stats,float,bool){} void ApplyStatPercentBuffMod(Stats,float,bool){} void HandleBaseModValue(uint32,uint32,float,bool){}
 void UpdateAttackPowerAndDamage(bool ranged=false){trace+=ranged?'r':'a';}
 void UpdateAllCritPercentages(){trace+='c';}void UpdateAllSpellCritChances(){trace+='s';}void UpdateDefenseBonusesMod(){trace+='d';}void UpdateSpellDamageBonus(){trace+='b';}void UpdateManaRegen(){trace+='m';}
 void UpdateWeaponHitChances(int attack){trace+=char('0'+attack);}void UpdateResistances(uint32 school){if(school==0)UpdateArmor();else SetResistance(SpellSchools(school),int32(GetTotalResistanceValue(SpellSchools(school))));}
 Powers GetPowerType()const{return POWER_MANA;}
 void InitStatsForLevel(){trace+='I';for(int i=0;i<5;++i)create[i]=stats[i]=input_stats[i];max_health=health=create_health;max_mana=mana=create_mana;SetArmor(create[1]*2);}
 void InitTaxiNodes(){}void InitTalentForLevel(){}void InitPrimaryProfessions(){}
 void LearnDefaultSkills(){trace+='K';}
 void learnDefaultSpells(){trace+='L';Aura a;a.target=this;a.m_modifier={mode,pct};a.HandleModTotalPercentStat(true,true);}
 void create_character(){
 // PINNED_CREATE_PREFIX
 trace+='E';ItemPrototype item;const uint32 kinds[5]={4,3,7,5,6};item.ItemStat[0]={kinds[mode],flat};item.ItemStat[1]={1,item_hp};item.ItemStat[2]={0,item_mana};item.Armor=uint32(item_armor);_ApplyItemBonuses(&item,4,true);
 }
};
// PINNED_AURA_TOTAL_STAT
// PINNED_ITEM_BONUSES
// PINNED_TOTAL_STAT
// PINNED_TOTAL_RESIST
// PINNED_HEALTH_BONUS
// PINNED_HEALTH_GETTER
// PINNED_MANA_BONUS
// PINNED_MANA_GETTER
// PINNED_MAX_HEALTH
// PINNED_MAX_MANA
// PINNED_ARMOR
// PINNED_UPDATE_ALL
int main(int argc,char**){assert(std::fesetround(FE_TONEAREST)==0);
 if(argc>1){std::cout<<"# synthetic item break/repair, original item-bonus block\n";
  for(int base:{21,30,127})for(int stam:{1,10})for(int flat_hp:{-50,-1,0,7})for(int intellect:{1,5})for(int flat_mana:{-10,0,7}){
   Player p;p.create_health=41;p.create_mana=59;for(int& s:p.input_stats)s=base;p.create_character();
   ItemPrototype item;item.ItemStat[0]={7,stam};item.ItemStat[1]={1,flat_hp};item.ItemStat[2]={5,intellect};item.ItemStat[3]={0,flat_mana};item.Armor=13;item.NatureRes=5;
   p._ApplyItemBonuses(&item,4,true);p.modify=true;p.UpdateAllStats();p.SetHealth(p.max_health);p.SetPower(POWER_MANA,p.max_mana/2);p.steps.clear();
   std::cout<<base<<'\t'<<stam<<'\t'<<flat_hp<<'\t'<<intellect<<'\t'<<flat_mana<<'\t'<<p.health<<'\t'<<p.max_health<<'\t'<<p.mana<<'\t'<<p.max_mana;
   p._ApplyItemBonuses(&item,4,false);std::cout<<'\t'<<p.health<<'\t'<<p.max_health<<'\t'<<p.mana<<'\t'<<p.max_mana<<'\t'<<p.resist[0]<<'\t'<<p.resist[3]<<'\t';
   for(auto step:p.steps){for(auto v:step)std::cout<<v<<',';std::cout<<';';}std::cout<<'\t';
   p.steps.clear();p._ApplyItemBonuses(&item,4,true);std::cout<<p.health<<'\t'<<p.max_health<<'\t'<<p.mana<<'\t'<<p.max_mana<<'\n';
  }return 0;
 }
 std::cout<<"# synthetic source extraction, not game rows\n";
 for(int mode=0;mode<5;++mode)for(int base:{1,19,20,21,31,127,255})for(int pct:{-100,-7,0,5,17,100})for(int flat:{0,1,7,31}){
 Player p;p.mode=mode;p.flat=flat;p.pct=pct;p.item_hp=7;p.item_mana=11;p.item_armor=13;p.create_health=37;p.create_mana=53;for(int& s:p.input_stats)s=base;
 p.create_character();uint32 saved_h=p.health,saved_m=p.mana;int32 before=p.stats[mode];uint32 before_h=p.max_health,before_m=p.max_mana;assert(!p.modify);
 p.modify=true;p.trace+='U';p.UpdateAllStats();p.SetHealth(saved_h);p.SetPower(POWER_MANA,saved_m);p.trace+='R';
 std::cout<<mode<<'\t'<<base<<'\t'<<pct<<'\t'<<flat<<'\t'<<before<<'\t'<<before_h<<'\t'<<before_m;
 for(auto s:p.stats)std::cout<<'\t'<<s;std::cout<<'\t'<<p.health<<'\t'<<p.max_health<<'\t'<<p.mana<<'\t'<<p.max_mana<<'\t'<<p.resist[0]<<'\t'<<p.trace<<'\n';
 }
}
