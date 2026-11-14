// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic adapter/probes. Generator inserts hash-checked CMaNGOS
// methods and unchanged named update blocks. No native content or stat rows.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <map>
#include <vector>
using uint8=uint8_t;
using uint16=uint16_t;
using uint32=uint32_t;
using int32=int32_t;
using Powers=uint32;
using SpellSchoolMask=uint32;
constexpr uint32 MAX_CLASSES=12, CLASS_DRUID=11,CLASS_HUNTER=3,CLASS_MAGE=8,CLASS_PALADIN=2,CLASS_PRIEST=5,CLASS_SHAMAN=7,CLASS_WARLOCK=9;
constexpr uint32 STAT_SPIRIT=0,STAT_INTELLECT=1,POWER_MANA=0,POWER_RAGE=1,POWER_ENERGY=2,POWER_FOCUS=3,POWER_HAPPINESS=4,POWER_HEALTH=5;
constexpr uint32 SPELL_AURA_MOD_SPELL_CRIT_CHANCE=0,SPELL_AURA_MOD_SPELL_CRIT_CHANCE_SCHOOL=1,SPELL_AURA_MOD_POWER_REGEN_PERCENT=2,SPELL_AURA_MOD_POWER_REGEN=3,SPELL_AURA_MOD_MANA_REGEN_INTERRUPT=4;
constexpr uint32 CONFIG_FLOAT_RATE_POWER_MANA=0,CONFIG_FLOAT_RATE_POWER_RAGE_LOSS=1,CONFIG_FLOAT_RATE_POWER_ENERGY=2;
constexpr uint32 SKILL_DELETED=1,REGEN_TIME_FULL=2000,TYPEID_UNIT=0;
float number(uint32 bits) {float value;std::memcpy(&value,&bits,4);return value;}
uint32 bits(float value) {uint32 result;std::memcpy(&result,&value,4);return result;}
uint32 PLAYER_SKILL_VALUE_INDEX(uint32 pos) {return pos*2;}
uint32 PLAYER_SKILL_BONUS_INDEX(uint32 pos) {return pos*2+1;}
uint16 SKILL_VALUE(uint32 value) {return uint16(value);}
uint16 SKILL_MAX(uint32 value) {return uint16(value>>16);}
int16_t SKILL_PERM_BONUS(uint32 value) {return int16_t(value>>16);}
int16_t SKILL_TEMP_BONUS(uint32 value) {return int16_t(value);}
struct SkillStatusData {uint32 pos{},uState{};};
using SkillStatusMap=std::map<uint16,SkillStatusData>;
struct SkillLineAbilityEntry {uint32 id,skillId,spellId;};
using SkillLineAbilityMap=std::multimap<uint32,const SkillLineAbilityEntry*>;
using SkillLineAbilityMapBounds=std::pair<SkillLineAbilityMap::const_iterator,SkillLineAbilityMap::const_iterator>;
struct SpellManager {
 SkillLineAbilityMap mappings;
 SkillLineAbilityMapBounds GetSkillLineAbilityMapBoundsBySpellId(uint32 id) const {return mappings.equal_range(id);}
} sSpellMgr;
struct SpellEntry {uint32 Id,maxLevel;};
struct World {float rate=1;float getConfig(uint32) const {return rate;}} sWorld;
struct Modifier {int32 m_miscvalue{},m_amount{};};
struct Aura {Modifier modifier;const Modifier* GetModifier() const {return &modifier;}};
using AuraList=std::vector<Aura*>;
struct Unit {
 uint32 spirit=0,intellect=0,level=1,type=1;
 bool totem=false;
 uint32 m_lastManaUseTimer=0;
 float GetStat(uint32 stat) const {return float(stat==STAT_SPIRIT?spirit:intellect);}
 uint8 getClass() const {return CLASS_MAGE;}
 uint32 GetLevel() const {return level;}
 uint32 GetTypeId() const {return type;}
 float OCTRegenMPPerSpirit() const;
 float GetMissChance(SpellSchoolMask) const;
 void SetLastManaUse() {m_lastManaUseTimer=5000;}
 bool IsUnderLastManaUseEffect() const {return m_lastManaUseTimer!=0;}
 void update_timer(uint32 diff) {
 // PINNED_MANA_TIMER
 }
};
struct Creature:Unit {bool IsTotem() const {return totem;}};
struct Player:Unit {
 uint32 mana=0,maximum=1000000,m_regenTimer=0;
 bool alive=true;
 int32 mp5=0,interrupt_percent=0,crit_all=0,crit_school=0;
 float multiplier=1,m_modManaRegen=0,m_modManaRegenInterrupt=0,m_modSpellCritChance[7]{};
 SkillStatusMap mSkillStatus;
 std::map<uint32,uint32> fields;
 uint32 GetUInt32Value(uint32 field) const {return fields.at(field);}
 uint16 GetSkill(uint16,bool,bool,bool=false) const;
 uint16 GetSkillValue(uint16 id) const {return GetSkill(id,true,true);}
 uint32 GetSpellRank(const SpellEntry*) const;
 float GetSpellCritFromIntellect() const;
 void UpdateSpellCritChance(uint32);
 void UpdateManaRegen();
 uint32 GetPower(uint32 power) const {assert(power==POWER_MANA);return mana;}
 uint32 GetMaxPower(uint32 power) const {assert(power==POWER_MANA);return maximum;}
 void SetPower(uint32 power,uint32 value) {assert(power==POWER_MANA);mana=value;}
 int32 GetTotalAuraModifier(uint32 kind) const {return kind==SPELL_AURA_MOD_MANA_REGEN_INTERRUPT?interrupt_percent:crit_all;}
 int32 GetTotalAuraModifierByMiscMask(uint32,uint32) const {return crit_school;}
 int32 GetTotalAuraModifierByMiscValue(uint32,uint32) const {return mp5;}
 float GetTotalAuraMultiplierByMiscValue(uint32,uint32) const {return multiplier;}
 const AuraList& GetAurasByType(uint32) const {static AuraList list;return list;}
 void Regenerate(Powers,uint32);
 uint32 scheduled=0;
 void RegenerateAll(uint32 diff) {scheduled=diff;Regenerate(POWER_MANA,diff);m_regenTimer-=diff;}
 bool IsAlive() const {return alive;}
 void frame(uint32 diff,bool spent) {
  // The event/aura phase can set last mana use before Unit decrements it.
  if(spent) SetLastManaUse();
  update_timer(diff);
  scheduled=0;
  // PINNED_SCHEDULE
 }
};
// PINNED_GET_SKILL
// PINNED_RANK
// PINNED_CRIT
// PINNED_UPDATE_CRIT
// PINNED_SPIRIT
// PINNED_UPDATE_REGEN
// PINNED_REGEN
// PINNED_BASE_MISS
int main() {
 assert(std::fesetround(FE_TONEAREST)==0);
 Player p;
 // All source level boundaries; adjacent integer stats and the accepted maximum.
 for(uint32 level:{1u,2u,19u,20u,59u,60u}) for(uint32 stat:{0u,1u,2u,19u,20u,21u,255u,256u,257u,65535u,999999u,1000000u}) {
  p.level=level;p.intellect=stat;p.UpdateSpellCritChance(2);
  std::cout<<"C "<<level<<' '<<stat<<' '<<bits(p.m_modSpellCritChance[2])<<'\n';
 }
 for(uint32 spirit:{0u,1u,2u,3u,4u,19u,20u,21u,255u,256u,257u,65535u,999999u,1000000u}) {
  p.spirit=spirit;
  std::cout<<"S "<<spirit<<' '<<bits(p.OCTRegenMPPerSpirit())<<'\n';
  for(float multiplier:{0.0f,0.5f,1.0f,1.25f}) for(int32 mp5:{0,1,17}) for(int32 percent:{0,1,15,30,100,101}) {
   p.multiplier=multiplier;p.mp5=mp5;p.interrupt_percent=percent;p.UpdateManaRegen();
   std::cout<<"U "<<spirit<<' '<<bits(multiplier)<<' '<<mp5<<' '<<percent<<' '<<bits(p.m_modManaRegen)<<' '<<bits(p.m_modManaRegenInterrupt)<<'\n';
  }
 }
 // Cast boundaries include >2^24 and the uint32 upper edge; integer diff/1000 differs.
 for(uint32 diff:{0u,1u,99u,100u,999u,1000u,1999u,2000u,2099u,2100u,4999u,5000u,16776999u,16777999u,33554999u,1000000999u,4294966999u,4294967295u}) {
  std::cout<<"D "<<diff<<' '<<uint32(float(diff)/1000)<<'\n';
  for(uint32 ratebits:{0u,0x3f000000u,0x3f800000u,0x3f800001u}) for(bool recent:{false,true}) {
   p.mana=7;p.m_modManaRegen=0.125f;p.m_modManaRegenInterrupt=0.01f;p.m_lastManaUseTimer=recent?1:0;sWorld.rate=number(ratebits);
   p.Regenerate(POWER_MANA,diff);
   std::cout<<"R "<<diff<<' '<<ratebits<<' '<<recent<<' '<<p.mana<<'\n';
  }
 }
 // Current skill includes signed bonuses, absent/deleted state and uint16 narrowing.
 SkillLineAbilityEntry first{2,7,900001},later{9,8,900001};
 for(uint32 mapping:{0u,1u,2u}) for(uint32 state:{0u,1u,2u}) for(uint32 base:{0u,1u,17u,100u,65535u}) for(int32 bonus:{-32768,-20,-1,0,1,32767}) for(uint32 cap:{0u,1u,20u}) {
  sSpellMgr.mappings.clear();
  if(mapping) sSpellMgr.mappings.insert({first.spellId,&first});
  if(mapping==2) sSpellMgr.mappings.insert({later.spellId,&later});
  p.mSkillStatus.clear();p.fields.clear();
  if(state) {p.mSkillStatus[7]={0,state==2?SKILL_DELETED:0};p.fields[0]=base;p.fields[1]=uint32(uint16(bonus))<<16|uint16(bonus);}
  p.mSkillStatus[8]={1,0};p.fields[2]=999;p.fields[3]=0;
  SpellEntry spell{900001,cap};
  std::cout<<"K "<<mapping<<' '<<state<<' '<<base<<' '<<bonus<<' '<<cap<<' '<<p.GetSpellRank(&spell)<<'\n';
 }
 Creature creature;
 for(uint32 type:{0u,1u}) for(bool totem:{false,true}) {creature.type=type;creature.totem=totem;std::cout<<"B "<<type<<' '<<totem<<' '<<bits(creature.GetMissChance(0))<<'\n';}
 // Source scheduling as a sequence; resets model source constructor counters.
 p.mana=100;p.maximum=1000;p.spirit=20;p.multiplier=1;p.mp5=5;p.interrupt_percent=0;p.UpdateManaRegen();sWorld.rate=1;p.m_regenTimer=0;p.m_lastManaUseTimer=0;
 uint32 index=0;
 for(uint32 diff:{1999u,1u,2101u,500u,1499u,2000u,2000u,4999u,1u,5099u,1u,1900u}) {
  bool spent=index==2||index==7||index==9;
  p.alive=index!=3;
  if(spent) p.mana-=10;
  p.frame(diff,spent);
  std::cout<<"F "<<diff<<' '<<spent<<' '<<p.alive<<' '<<p.mana<<' '<<p.m_regenTimer<<' '<<p.m_lastManaUseTimer<<' '<<p.scheduled<<'\n';
  ++index;
 }
}
