// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic adapters. Generator inserts unchanged pinned methods.
#include <algorithm>
#include <cstdint>
#include <functional>
#include <iostream>
#include <map>
#include <memory>
#include <unordered_map>
#include <tuple>
#include <sstream>
#include <vector>
using uint8=uint8_t;
using uint16=uint16_t;using uint32=uint32_t;using int32=int32_t;
#define DEBUG_LOG(...) do{}while(false)
constexpr uint32 SKILL_CATEGORY_LANGUAGES=10,SKILL_CATEGORY_WEAPON=6,SKILL_CATEGORY_ARMOR=8,SKILL_CATEGORY_CLASS=7,SKILL_CATEGORY_SECONDARY=9,SKILL_CATEGORY_PROFESSION=11,SKILL_CATEGORY_ATTRIBUTES=5,SKILL_CATEGORY_GENERIC=12;
constexpr uint32 SKILL_FIST_WEAPONS=473,SKILL_POISONS=40,SKILL_LOCKPICKING=633,SKILL_FISHING=356,SKILL_COOKING=185,SKILL_FIRST_AID=129,SKILL_RIDING=762,SKILL_FLAG_MAXIMIZED=0x10,MAX_SKILL_STEP=16;
constexpr uint32 SKILL_FLAG_DISPLAY_AS_MONO=0x400,PLAYER_MAX_SKILLS=127,SKILL_UNCHANGED=0,SKILL_DELETED=1;
uint32 PLAYER_SKILL_INDEX(uint32 i){return i*3;}uint32 PLAYER_SKILL_VALUE_INDEX(uint32 i){return i*3+1;}uint32 PLAYER_SKILL_BONUS_INDEX(uint32 i){return i*3+2;}
uint32 MAKE_PAIR32(uint16 lo,uint16 hi){return uint32(lo)|(uint32(hi)<<16);}uint32 MAKE_SKILL_VALUE(uint16 lo,uint16 hi){return MAKE_PAIR32(lo,hi);}
uint16 SKILL_VALUE(uint32 n){return uint16(n);}
struct Field {uint32 value;uint16 GetUInt16()const{return uint16(value);}uint32 GetUInt32()const{return value;}bool GetBool()const{return value!=0;}};
struct TalentSpellPos{};const TalentSpellPos* GetTalentSpellPos(uint32){return nullptr;}
struct QueryResult {Field fields[3];Field* Fetch(){return fields;}bool NextRow(){return false;}};
struct Logger {template<class...T>void outError(const char*,T...){} }sLog;
struct Database {template<class...T>void PExecute(const char*,T...){} }CharacterDatabase;
struct SkillStatusData {uint32 pos,uState;SkillStatusData(uint32 p,uint32 s):pos(p),uState(s){}};using SkillStatusMap=std::unordered_map<uint32,SkillStatusData>;
enum SkillRangeType {SKILL_RANGE_LANGUAGE,SKILL_RANGE_LEVEL,SKILL_RANGE_MONO,SKILL_RANGE_RANK,SKILL_RANGE_NONE};
struct SkillLineEntry {uint32 id;int32 categoryId;};
struct SkillRaceClassInfoEntry {uint32 skillId,raceMask,classMask,flags,skillTierId;};
struct SkillTiersEntry {uint32 id,skillValue[16]{},maxSkillValue[16]{};};
template<class T>struct Store {std::map<uint32,T> rows;const T* LookupEntry(uint32 id)const{auto it=rows.find(id);return it==rows.end()?nullptr:&it->second;}};
Store<SkillLineEntry>sSkillLineStore;Store<SkillTiersEntry>sSkillTiersStore;
struct InitialSkill {uint16 SkillId,Step;};struct PlayerInfo {std::vector<InitialSkill> skill;std::vector<uint32> spell;};
struct ObjectMgr {PlayerInfo info;const PlayerInfo* GetPlayerInfo(uint32,uint32)const{return &info;}}sObjectMgr;
using InfoMap=std::multimap<uint32,const SkillRaceClassInfoEntry*>;
struct SpellMgr {InfoMap rows;auto GetSkillRaceClassInfoMapBounds(uint32 id)const{return rows.equal_range(id);}}sSpellMgr;
// PINNED_PRIMARY_PROFESSION
// PINNED_IS_PROFESSION
// PINNED_IS_RIDING
// PINNED_SKILL_RANGE
struct LiveSkill {uint16 value=0,max=0,step=0;};
struct Player {
 uint32 weapon_proficiency=0,armor_proficiency=0;
 bool IsPlayer()const{return true;}
 uint32 GetWeaponProficiency()const{return weapon_proficiency;}uint32 GetArmorProficiency()const{return armor_proficiency;}
 void AddWeaponProficiency(uint32 mask){weapon_proficiency|=mask;}void AddArmorProficiency(uint32 mask){armor_proficiency|=mask;}
 void SendProficiency(uint32,uint32){}
 std::map<uint16,LiveSkill> skills;SkillStatusMap mSkillStatus;std::map<uint32,uint32> fields;
 std::ostringstream phases;
 uint32 GetGUIDLow()const{return 0;}
 uint32 GetUInt32Value(uint32 index)const{return fields.at(index);}
 bool IsInWorld()const{return false;}
 bool addSpell(uint32 id,bool active,bool learning,bool dependent,bool disabled){phases<<"A:"<<id<<":"<<active<<":"<<learning<<":"<<dependent<<":"<<disabled<<",";return true;}
 void learnSpell(uint32,bool){}
 void UpdateSkillTrainedSpells(uint16 id,uint16 value){phases<<"T:"<<id<<":"<<value<<",";}
 void _LoadSpells(std::unique_ptr<QueryResult>);void learnDefaultSpells();
 void SetUInt32Value(uint32 index,uint32 value){fields[index]=value;if(index%3==1){uint32 tag=fields[index-1];uint16 id=uint16(tag);if(id)skills[id]={uint16(value),uint16(value>>16),uint16(tag>>16)};}}
 void _LoadSkills(std::unique_ptr<QueryResult>);
 uint32 getRace()const{return 1;}uint32 getClass()const{return 8;}uint32 GetLevel()const{return 1;}uint32 getRaceMask()const{return 1;}uint32 getClassMask()const{return 128;}
 uint16 GetSkillMaxForLevel()const{return 5;}bool HasSkill(uint16 id)const{return skills.count(id);}
 uint16 GetSkillValuePure(uint16 id)const{auto it=skills.find(id);return it==skills.end()?0:it->second.value;}
 void SetSkill(uint16 id,uint16 value,uint16 max,uint16 step=0){if(value)skills[id]={value,max,step};else skills.erase(id);}
 const SkillRaceClassInfoEntry* GetSkillInfo(uint16,std::function<bool(const SkillRaceClassInfoEntry&)> = nullptr)const;
 void LearnDefaultSkills();void SetSkillStep(uint16,uint16);
};
// PINNED_LOAD_SPELLS
// PINNED_DEFAULT_SPELLS
// PINNED_LOAD_SKILLS
// PINNED_GET_INFO
// PINNED_DEFAULT_SKILLS
// PINNED_SET_STEP
constexpr uint32 ITEM_CLASS_WEAPON=2,ITEM_CLASS_ARMOR=4;
using SpellEffectIndex=uint32;
struct SpellEntry {int32 EquippedItemClass;uint32 EquippedItemSubClassMask;};
struct Spell {Player* m_caster;const SpellEntry* m_spellInfo;void EffectProficiency(SpellEffectIndex);};
// PINNED_PROFICIENCY
int main(){std::cout<<"# synthetic source skills, not game rows\n";
 const uint32 id=60001;
 for(int cat:{5,6,7,8,9,10,11,12})for(uint32 flags:{0u,16u})for(uint32 tier:{0u,1u})for(uint16 step:{0,1,2}){
 sSkillLineStore.rows={{id,{id,cat}}};SkillTiersEntry tiers{};tiers.id=1;for(int n=0;n<16;++n){tiers.skillValue[n]=42;tiers.maxSkillValue[n]=uint32((n+1)*75);}sSkillTiersStore.rows={{1,tiers}};
 SkillRaceClassInfoEntry info{id,1,128,flags,tier};sSpellMgr.rows.clear();sSpellMgr.rows.emplace(id,&info);sObjectMgr.info.skill={{uint16(id),step}};
 Player p;p.LearnDefaultSkills();auto it=p.skills.find(id);LiveSkill out=it==p.skills.end()?LiveSkill{}:it->second;
 std::cout<<"D\t"<<cat<<'\t'<<flags<<'\t'<<tier<<'\t'<<step<<'\t'<<out.value<<'\t'<<out.max<<'\t'<<out.step<<'\n';
 }
 for(uint32 flags:{0u,16u})for(uint16 old:{0,1})for(uint16 step:{1,2,16}){
 sSkillLineStore.rows={{id,{id,11}}};SkillTiersEntry tiers{};tiers.id=1;for(int n=0;n<16;++n){tiers.skillValue[n]=42;tiers.maxSkillValue[n]=uint32((n+1)*75);}sSkillTiersStore.rows={{1,tiers}};
 SkillRaceClassInfoEntry info{id,1,128,flags,1};sSpellMgr.rows.clear();sSpellMgr.rows.emplace(id,&info);Player p;if(old)p.skills[id]={old,150,1};p.SetSkillStep(id,step);auto out=p.skills.at(id);
 std::cout<<"S\t"<<flags<<'\t'<<old<<'\t'<<step<<'\t'<<out.value<<'\t'<<out.max<<'\t'<<out.step<<'\n';
 }
 for(int cat:{6,7,9,10,11,12})for(uint32 flags:{0u,16u,1040u})for(uint32 tier:{0u,1u})for(uint16 value:{1,3,75})for(uint16 max:{1,5,75,150})for(uint16 step:{0,1,2}){
  sSkillLineStore.rows={{id,{id,cat}}};SkillTiersEntry tiers{};tiers.id=1;for(int n=0;n<16;++n){tiers.skillValue[n]=42;tiers.maxSkillValue[n]=uint32((n+1)*75);}sSkillTiersStore.rows={{1,tiers}};
  SkillRaceClassInfoEntry info{id,1,128,flags,tier};sSpellMgr.rows.clear();sSpellMgr.rows.emplace(id,&info);
  Player p;auto q=std::make_unique<QueryResult>();q->fields[0].value=uint16(id);q->fields[1].value=value;q->fields[2].value=max;p._LoadSkills(std::move(q));if(step)p.SetSkillStep(uint16(id),step);auto out=p.skills.at(id);
  std::cout<<"L\t"<<cat<<'\t'<<flags<<'\t'<<tier<<'\t'<<value<<'\t'<<max<<'\t'<<step<<'\t'<<out.value<<'\t'<<out.max<<'\t'<<out.step<<'\n';
 }

 sSkillLineStore.rows={{id,{id,6}}};sSpellMgr.rows.clear();sObjectMgr.info.spell={900000111};
 Player fresh;auto q=std::make_unique<QueryResult>();q->fields[0].value=id;q->fields[1].value=1;q->fields[2].value=5;
 fresh._LoadSkills(std::move(q));fresh._LoadSpells(nullptr);fresh.learnDefaultSpells();
 std::cout<<"P\t"<<fresh.phases.str()<<'\n';

 for(int32 item_class:{2,4})for(uint32 first:{0u,1u,3u,8u})for(uint32 second:{0u,1u,2u,3u,9u}){
  Player p;SpellEntry entry{item_class,first};Spell cast{&p,&entry};cast.EffectProficiency(0);
  entry.EquippedItemSubClassMask=second;cast.EffectProficiency(0);
  std::cout<<"F\t"<<item_class<<'\t'<<first<<'\t'<<second<<'\t'<<p.weapon_proficiency<<'\t'<<p.armor_proficiency<<'\n';
 }

}
