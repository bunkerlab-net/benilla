// SPDX-License-Identifier: GPL-3.0-or-later
// Authored distribution-outcome adapters. PINNED markers receive unchanged
// CMaNGOS bodies, copyright CMaNGOS contributors, GPL-2.0-or-later, no warranty.
// Macro substitutes only shuffle outcomes; no MT/std::shuffle word parity claim.
#include <algorithm>
#include <array>
#include <cassert>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <map>
#include <memory>
#include <sstream>
#include <string>
#include <vector>
using uint8=uint8_t;using uint16=uint16_t;using uint32=uint32_t;using int32=int32_t;
constexpr uint32 MAX_NR_LOOT_ITEMS=16,ITEM_FLAG_MULTI_DROP=0x800;
enum {LOOTITEM_TYPE_QUEST,LOOTITEM_TYPE_CONDITIONNAL,LOOTITEM_TYPE_NORMAL,
 CONFIG_FLOAT_RATE_DROP_ITEM_REFERENCED=7,CONFIG_FLOAT_RATE_DROP_ITEM_QUEST=8,
 CONFIG_FLOAT_RATE_DROP_MONEY=9,CONFIG_BOOL_CORPSE_EMPTY_LOOT_SHOW=10,
 UNIT_DYNAMIC_FLAGS,UNIT_DYNFLAG_LOOTABLE,CREATURE_LOOT_STATUS_LOOTED};
constexpr std::array<int,7>qualityToRate={0,1,2,3,4,5,6};
struct World {std::array<float,11>rates{};float getConfig(uint32 i)const{return rates.at(i);}}sWorld;
template<class T,class B>B bits(T f){static_assert(sizeof(T)==sizeof(B));B n;std::memcpy(&n,&f,sizeof n);return n;}
std::vector<std::string> draws;uint32 case_id=0,draw=0;
uint32 urand(uint32 low,uint32 high){assert(low<=high);auto v=low+((case_id*1664525u+draw++*1013904223u)%(uint64_t(high)-low+1));draws.push_back("u:"+std::to_string(low)+":"+std::to_string(high)+":"+std::to_string(v));return v;}
double rand_chance(){const std::array<double,10>v={0.,0.0000001,0.000001,12.5,25.,49.999999999,50.,75.,99.99999999,99.99999999999999};double r=v[(case_id+draw++*7)%v.size()];draws.push_back("c:"+std::to_string(bits<double,uint64_t>(r)));return r;}
float rand_chance_f(){return float(rand_chance());}
// PINNED_ROLL_CHANCE
struct Generator{}generator;Generator*GetRandomGenerator(){return &generator;}
// The function name is substituted only at source shuffle helper callsites.
namespace loot_outcomes {template<class It>void loot_outcome_shuffle(It begin,It end,Generator&){
 std::string record="s:";for(auto it=begin;it!=end;++it){if(it!=begin)record+=",";record+=std::to_string((*it)->itemIndex);}record+=":";
 auto n=end-begin;if(n){std::rotate(begin,begin+(case_id+draw)%n,end);if((case_id+draw)%2)std::reverse(begin,end);}++draw;
 for(auto it=begin;it!=end;++it){if(it!=begin)record+=",";record+=std::to_string((*it)->itemIndex);}draws.push_back(record);
}}
// Redirect the qualified helper without adding any declarations to namespace std.
// Every other standard-library symbol is the real std member through this alias.
namespace loot_source_std {using namespace std;using loot_outcomes::loot_outcome_shuffle;}
using loot_outcomes::loot_outcome_shuffle;
#define std loot_source_std
#define shuffle loot_outcome_shuffle
struct Logger {template<class...A>void outDebug(A...){std::abort();}template<class...A>void outError(A...){std::abort();}}sLog;
struct ItemPrototype {uint32 Quality=0,DisplayInfoID=0,Flags=0;};std::map<uint32,ItemPrototype>prototypes;
struct ObjectMgr {static ItemPrototype const*GetItemPrototype(uint32 id){return &prototypes.at(id);}};
struct Item {static int32 GenerateItemRandomPropertyId(uint32){return 0;}};
struct LootStoreItem {uint32 itemid=0;float chance=0;uint8 group=0;int32 mincountOrRef=1;uint32 maxcount=1;uint16 conditionId=0;bool needs_quest=false;uint32 itemIndex=0;bool Roll(bool)const;};
struct LootItem {uint32 lootItemType=0;ItemPrototype const*itemProto=nullptr;bool freeForAll=false,isUnderThreshold=false,isBlocked=false,currentLooterPass=false,isReleased=false;uint32 displayID=0,itemId=0,conditionId=0,lootSlot=0,count=0,randomSuffix=0;int32 randomPropertyId=0;LootItem(LootStoreItem const&,uint32,uint32);};
struct Player{};
struct LootStats {struct GroupStats {void IncItemCount(uint32,std::pair<int32,uint32>){std::abort();}};GroupStats*GetStatsForLootId(int32){std::abort();}};
struct LootStatsData {LootStats*stats;int32 groupIdOrItemId;LootStatsData(int32,LootStats*){std::abort();}};
struct LootTemplate;
struct LootStore {LootTemplate const*GetLootFor(int32)const{std::abort();}}LootTemplates_Reference, LootTemplates_Creature;
struct CreatureInfo {uint32 LootId=0,MinLootGold=0,MaxLootGold=0;};
struct Creature {void SetFlag(uint32,uint32){}void SetLootStatus(uint32){}};
struct Loot {
 std::vector<LootItem*>m_lootItems;uint32 m_maxSlot=0,m_threshold=2,m_gold=0;bool m_haveItemOverThreshold=false,m_isFakeLoot=false;LootTemplate const*tab=nullptr;
 ~Loot(){for(auto*p:m_lootItems)delete p;}
 bool IsItemAlreadyIn(uint32 id)const{for(auto*p:m_lootItems)if(p->itemId==id)return true;return false;}
 void AddItem(LootStoreItem const&);void GenerateMoneyLoot(uint32,uint32);
 bool FillLoot(uint32,LootStore const&,Player*,bool);
 bool IsLootedForAll()const{return m_gold==0&&m_lootItems.empty();}
 void ForceLootAnimationClientUpdate(){}
 void corpse(CreatureInfo const*creatureInfo,Creature*creature,Player*player){do{
// PINNED_CORPSE_BLOCK
 }while(false);}
};
struct LootTemplate {
 struct LootGroup {std::vector<LootStoreItem>ExplicitlyChanced,EqualChanced;void AddEntry(LootStoreItem const&);LootStoreItem const*Roll(Loot const&,Player const*)const;void Process(Loot&,Player const*,bool,LootStatsData* =nullptr)const;};
 std::vector<LootStoreItem>Entries;std::vector<LootGroup>Groups;void AddEntry(LootStoreItem const&);void Process(Loot&,Player const*,bool,LootStatsData* =nullptr)const;
 static bool PlayerOrGroupFulfilsCondition(Loot const&,Player const*,uint16){std::abort();}
};
bool Loot::FillLoot(uint32,LootStore const&,Player*owner,bool){if(!tab)return false;tab->Process(*this,owner,true);return true;}
// PINNED_METHODS
int main(){
 for(case_id=0;case_id<1536;++case_id){
  prototypes.clear();draws.clear();draw=0;LootTemplate tab;Loot loot;Creature creature;CreatureInfo info;Player owner;std::vector<LootStoreItem>rows;
  info.LootId=case_id%17==0?0:1;
  uint32 row_count=case_id<1024?1+case_id%23:1+case_id%40;
  for(uint32 i=0;i<row_count&&info.LootId;++i){
   const std::array<float,9>chances={0.000001f,0.1f,12.5f,25.f,50.f,75.f,99.99999f,100.f,150.f};
   LootStoreItem row;row.itemid=70001+(i+case_id)%7;row.itemIndex=i;row.group=case_id%6==0?0:(i*3+case_id)%8;row.chance=chances[(i+case_id)%chances.size()];
   if(row.group&&(i+case_id)%3==0)row.chance=0;
   if(case_id>=1024)row.chance=row.group?0:100;
   row.mincountOrRef=1+(i+case_id)%5;row.maxcount=row.mincountOrRef+(i*7+case_id)%17;
   prototypes[row.itemid]={uint32((row.itemid-70001)%7),90000+row.itemid,case_id%2?0u:0x800u};rows.push_back(row);tab.AddEntry(row);
  }
  const std::array<float,7>rates={0.f,0.1f,0.5f,1.f,1.5f,3.f,1000.f};
  for(uint32 i=0;i<7;++i)sWorld.rates[i]=rates[(case_id+i)%rates.size()];
  sWorld.rates[CONFIG_FLOAT_RATE_DROP_MONEY]=std::array<float,5>{0.f,0.1f,1.f,1.25f,2.f}[case_id%5];
  sWorld.rates[CONFIG_BOOL_CORPSE_EMPTY_LOOT_SHOW]=case_id%2;
  switch((case_id/3)%8){case 0:info.MinLootGold=info.MaxLootGold=0;break;case 1:info.MinLootGold=10;info.MaxLootGold=3;break;case 2:info.MinLootGold=info.MaxLootGold=77;break;case 3:info.MinLootGold=100;info.MaxLootGold=32799;break;case 4:info.MinLootGold=100;info.MaxLootGold=32800;break;case 5:info.MinLootGold=255;info.MaxLootGold=65536;break;case 6:info.MinLootGold=16777217;info.MaxLootGold=16777219;break;default:info.MinLootGold=100000;info.MaxLootGold=300000;break;}
  loot.tab=info.LootId?&tab:nullptr;loot.corpse(&info,&creature,&owner);
  std::cout<<case_id<<'\t'<<info.LootId<<'\t'<<info.MinLootGold<<':'<<info.MaxLootGold<<'\t';
  for(uint32 i=0;i<7;++i){if(i)std::cout<<',';std::cout<<bits<float,uint32>(sWorld.rates[i]);}
  std::cout<<':'<<bits<float,uint32>(sWorld.rates[CONFIG_FLOAT_RATE_DROP_MONEY])<<':'<<bool(sWorld.rates[CONFIG_BOOL_CORPSE_EMPTY_LOOT_SHOW])<<'\t';
  if(rows.empty())std::cout<<'-';
  for(uint32 i=0;i<rows.size();++i){if(i)std::cout<<';';auto&r=rows[i];std::cout<<r.itemid<<':'<<bits<float,uint32>(r.chance)<<':'<<unsigned(r.group)<<':'<<r.mincountOrRef<<':'<<r.maxcount;}
  std::cout<<'\t';if(draws.empty())std::cout<<'-';for(uint32 i=0;i<draws.size();++i){if(i)std::cout<<';';std::cout<<draws[i];}
  std::cout<<'\t'<<loot.m_gold<<':'<<loot.m_isFakeLoot<<'\t';
  if(loot.m_lootItems.empty())std::cout<<'-';
  for(uint32 i=0;i<loot.m_lootItems.size();++i){if(i)std::cout<<';';auto*r=loot.m_lootItems[i];std::cout<<r->lootSlot<<':'<<r->itemId<<':'<<r->count<<':'<<r->displayID<<':'<<r->randomPropertyId;}std::cout<<'\n';
 }
}
