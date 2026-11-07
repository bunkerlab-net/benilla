// SPDX-License-Identifier: GPL-3.0-or-later
// Extracted methods: CMaNGOS Project, GPL-2.0-or-later; see upstream AUTHORS.
// Original authored adapters; generator inserts complete unchanged CMaNGOS bodies.
// addSpell/learnSpell/removeSpell below are endpoint recorders, NOT Player methods.
// The optional callback aura installation is authored reentrancy, not addSpell closure.
#include <algorithm>
#include <array>
#include <cstdint>
#include <functional>
#include <iostream>
#include <map>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <vector>
using uint8=uint8_t; using uint16=uint16_t; using uint32=uint32_t; using int32=int32_t;
constexpr uint32 PLAYER_MAX_SKILLS=127;
// PINNED_STATUS
constexpr uint16 SKILL_DEFENSE=95;
constexpr uint32 SPELL_AURA_MOD_SKILL=30, SPELL_AURA_MOD_SKILL_TALENT=98, SPELL_EFFECT_SKILL=118;
// Authored compact field storage: preserves source low/high word packing.
uint32 PLAYER_SKILL_INDEX(uint32 p){return p*3;}
uint32 PLAYER_SKILL_VALUE_INDEX(uint32 p){return p*3+1;}
uint32 PLAYER_SKILL_BONUS_INDEX(uint32 p){return p*3+2;}
uint32 MAKE_PAIR32(uint16 a,uint16 b){return uint32(a)|(uint32(b)<<16);}
uint32 MAKE_SKILL_VALUE(uint16 a,uint16 b){return MAKE_PAIR32(a,b);}
struct SkillStatusData {uint32 pos,uState; SkillStatusData(uint32 p,uint32 s):pos(p),uState(s){}};
using SkillStatusMap=std::unordered_map<uint32,SkillStatusData>;
struct SkillLineEntry{};
struct SkillStore {std::set<uint32> ids;const SkillLineEntry* LookupEntry(uint32 id)const{static SkillLineEntry row;return ids.count(id)?&row:nullptr;}}sSkillLineStore;
struct Logger {template<class...T> void outError(const char*,T...){throw std::runtime_error("unexpected source error");}}sLog;
struct SkillLineAbilityEntry {uint32 spellId=0,racemask=0,classmask=0,learnOnGetSkill=1,req_skill_value=0;};
struct SpellLearnSkillNode {uint16 skill=0;uint32 effect=SPELL_EFFECT_SKILL;uint16 step=0;};
using SkillLineAbilityMap=std::multimap<uint32,const SkillLineAbilityEntry*>;
using SkillLineAbilityMapBounds=std::pair<SkillLineAbilityMap::const_iterator,SkillLineAbilityMap::const_iterator>;
struct SpellMgr {
 SkillLineAbilityMap abilities;std::map<uint32,SpellLearnSkillNode> training;
 SkillLineAbilityMapBounds GetSkillLineAbilityMapBoundsBySkillId(uint32 id)const{return abilities.equal_range(id);}
 const SpellLearnSkillNode* GetSpellLearnSkill(uint32 id)const{auto i=training.find(id);return i==training.end()?nullptr:&i->second;}
}sSpellMgr;
struct Modifier {int32 m_miscvalue;};
struct Aura {Modifier modifier;std::function<void(bool)> callback;Modifier* GetModifier(){return &modifier;}void ApplyModifier(bool apply){callback(apply);}};
struct Player {
 SkillStatusMap mSkillStatus;std::array<uint32,PLAYER_MAX_SKILLS*3> fields{};
 std::map<uint32,std::vector<Aura*>> auras;std::set<uint32> known;
 std::ostringstream events;bool in_world=false;uint16 watched=60001;std::function<void(uint32)> authored_callback;
 uint32 getRaceMask()const{return 1;}uint32 getClassMask()const{return 128;}
 bool IsInWorld()const{return in_world;}bool HasSpell(uint32 id)const{return known.count(id);}
 uint32 GetUInt32Value(uint32 i)const{return fields.at(i);}
 void SetUInt32Value(uint32 i,uint32 v){fields.at(i)=v;events<<"field:"<<i<<'='<<v<<';';}
 uint16 GetSkillStep(uint16 id)const{auto i=mSkillStatus.find(id);return i==mSkillStatus.end()||i->second.uState==SKILL_DELETED?0:uint16(fields.at(i->second.pos*3)>>16);}
 const std::vector<Aura*>& GetAurasByType(uint32 type){return auras[type];}
 void UpdateDefenseBonusesMod(){events<<"defense;";}
 void endpoint(const char* name,uint32 id){
  auto i=mSkillStatus.find(watched);
  events<<name<<':'<<id<<":snapshot=";
  if(i==mSkillStatus.end())events<<"missing";
  else {auto p=i->second.pos*3;events<<fields[p]<<','<<fields[p+1]<<','<<fields[p+2]<<','<<i->second.uState;}
  events<<';';known.insert(id);if(authored_callback)authored_callback(id);
 }
 bool addSpell(uint32 id,bool active,bool learning,bool dependent,bool disabled){
  if(!active||!learning||!dependent||disabled)throw std::runtime_error("unexpected addSpell flags");
  endpoint("add-endpoint",id);return true;
 }
 void learnSpell(uint32 id,bool dependent){if(!dependent)throw std::runtime_error("unexpected learnSpell flags");endpoint("learn-endpoint",id);}
 void removeSpell(uint32 id){events<<"remove-endpoint:"<<id<<';';known.erase(id);}
 void removeSpell(uint32 id,bool a,bool b,bool dependent){if(a||b||!dependent)throw std::runtime_error("unexpected remove flags");events<<"remove-dependent-endpoint:"<<id<<';';known.erase(id);}
 void SetSkill(SkillStatusMap::iterator,uint16,uint16,uint16=0);
 void SetSkill(uint16,uint16,uint16,uint16=0);
 void UpdateSkillTrainedSpells(uint16,uint16);
};
// PINNED_SET_EXISTING
// PINNED_SET_ID
// PINNED_TRAIN
void output(const std::string& name,Player& p){
 std::cout<<name<<'\t'<<p.events.str()<<'\t';
 // Sort only the output snapshot, never source training traversal.
 std::map<uint32,SkillStatusData> sorted(p.mSkillStatus.begin(),p.mSkillStatus.end());
 for(auto& pair:sorted){auto v=pair.second;auto pos=v.pos*3;std::cout<<pair.first<<':'<<v.pos<<':'<<v.uState<<':'<<p.fields[pos]<<':'<<p.fields[pos+1]<<':'<<p.fields[pos+2]<<';';}
 std::cout<<'\t';for(auto id:p.known)std::cout<<id<<',';std::cout<<'\n';
}
void reset(){sSpellMgr.abilities.clear();sSpellMgr.training.clear();sSkillLineStore.ids={60001,SKILL_DEFENSE};}
void seed(Player& p,uint16 id,uint16 value,uint16 step,uint32 status=0){p.watched=id;p.mSkillStatus.emplace(id,SkillStatusData(0,status));p.fields[0]=MAKE_PAIR32(id,step);p.fields[1]=MAKE_SKILL_VALUE(value,75);p.fields[2]=MAKE_PAIR32(2,3);}
int main(){
 std::cout<<"# authored source-callback mechanism vectors; no client DBC or Human order\n"
          <<"# add/learn/remove endpoints are adapters; optional aura callback is authored, not recursive addSpell proof\n"
          <<"# case\tordered_events\tfinal_skills(id:position:status:tag:value:bonus)\tendpoint_known\n";
 for(bool existing:{false,true})for(bool defense:{false,true})for(bool callback:{false,true}){
  reset();Player p;uint16 id=defense?SKILL_DEFENSE:60001;p.watched=id;
  if(existing)seed(p,id,1,2);
  SkillLineAbilityEntry row{70001,0,0,1,0};sSpellMgr.abilities.emplace(id,&row);
  Aura temp{{id},[&](bool apply){if(!apply)throw std::runtime_error("unexpected aura removal");p.events<<"aura-temp;";auto pos=p.mSkillStatus.at(id).pos*3+2;p.fields[pos]+=2;}};
  Aura perm{{id},[&](bool apply){if(!apply)throw std::runtime_error("unexpected aura removal");p.events<<"aura-perm;";auto pos=p.mSkillStatus.at(id).pos*3+2;p.fields[pos]+=uint32(5)<<16;}};
  p.auras[SPELL_AURA_MOD_SKILL]={&temp};
  if(callback)p.authored_callback=[&](uint32){p.events<<"authored-install-perm;";p.auras[SPELL_AURA_MOD_SKILL_TALENT]={&perm};perm.ApplyModifier(true);};
  else p.auras[SPELL_AURA_MOD_SKILL_TALENT]={&perm};
  p.SetSkill(id,3,75,4);
  output(std::string(existing?"existing":"new")+(defense?"_defense":"_ordinary")+(callback?"_callback_aura":"_retained_aura"),p);
 }
 for(uint32 method:{0u,1u})for(uint16 value:{2,3,4})for(bool world:{false,true}){
  reset();Player p;p.in_world=world;seed(p,60001,value,2);
  SkillLineAbilityEntry wildcard{70001,0,0,method,3},race_miss{70002,2,0,1,0},class_miss{70003,0,1,1,0},matched{70004,1,128,1,3};
  for(auto* row:{&wildcard,&race_miss,&class_miss,&matched})sSpellMgr.abilities.emplace(60001,row);
  sSpellMgr.training.emplace(70001,SpellLearnSkillNode{60001,SPELL_EFFECT_SKILL,1});
  p.UpdateSkillTrainedSpells(60001,value);
  output("training_m"+std::to_string(method)+"_v"+std::to_string(value)+(world?"_world":"_offline"),p);
 }
 for(uint16 step:{1,2,3})for(bool known:{false,true}){
  reset();Player p;seed(p,60001,3,2);if(known)p.known.insert(70001);
  SkillLineAbilityEntry row{70001,0,0,0,0};sSpellMgr.abilities.emplace(60001,&row);sSpellMgr.training.emplace(70001,SpellLearnSkillNode{60001,SPELL_EFFECT_SKILL,step});
  p.UpdateSkillTrainedSpells(60001,3);output("repair_step"+std::to_string(step)+(known?"_known":"_missing"),p);
 }
 for(bool reverse:{false,true}){
  reset();Player p;seed(p,60001,3,2);SkillLineAbilityEntry a{70001,0,0,1,0},b{70002,0,0,1,0};
  // Authored loaded-map insertion order; no claim about genuine Human rows.
  sSpellMgr.abilities.emplace(60001,reverse?&b:&a);sSpellMgr.abilities.emplace(60001,reverse?&a:&b);
  p.UpdateSkillTrainedSpells(60001,3);output(reverse?"rows_reverse":"rows_forward",p);
 }
 for(uint32 status:{0u,uint32(SKILL_NEW)}){
  reset();Player p;seed(p,60001,3,2,status);p.known={70001};SkillLineAbilityEntry row{70001,2,1,0,99};sSpellMgr.abilities.emplace(60001,&row);
  p.SetSkill(uint16(60001),0,0);output(status==SKILL_NEW?"remove_new_outside_starter":"remove_saved_outside_starter",p);
 }
 for(uint32 threshold:{0u,1u,2u})for(bool reverse:{false,true}){
  reset();Player starter;
  SkillLineAbilityEntry tempRow{70001,1,128,1,threshold},raceMiss{70002,2,128,1,0},classMiss{70003,1,1,1,0},permRow{70004,1,128,1,threshold};
  std::vector<SkillLineAbilityEntry*> rows{&tempRow,&raceMiss,&classMiss,&permRow};if(reverse)std::reverse(rows.begin(),rows.end());
  for(auto row:rows)sSpellMgr.abilities.emplace(60001,row);
  Aura temp{{60001},[&](bool apply){if(!apply)throw std::runtime_error("unexpected aura removal");starter.events<<"aura-temp;";starter.fields[2]+=2;}};
  Aura perm{{60001},[&](bool apply){if(!apply)throw std::runtime_error("unexpected aura removal");starter.events<<"aura-perm;";starter.fields[2]+=uint32(5)<<16;}};
  starter.authored_callback=[&](uint32 spell){
   if(spell==70001){starter.events<<"authored-install-temp;";starter.auras[SPELL_AURA_MOD_SKILL]={&temp};temp.ApplyModifier(true);}
   else if(spell==70004){starter.events<<"authored-install-perm;";starter.auras[SPELL_AURA_MOD_SKILL_TALENT]={&perm};perm.ApplyModifier(true);}
   else throw std::runtime_error("unexpected starter callback spell");
  };
  starter.SetSkill(uint16(60001),1,5);
  output("starter_threshold"+std::to_string(threshold)+(reverse?"_reverse":"_forward"),starter);
 }
 reset();Player p;p.SetSkill(uint16(0),1,1);p.SetSkill(p.mSkillStatus.end(),1,1);output("invalid_id_and_end_noop",p);
}
