// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic adapters and probes; pinned CMaNGOS bodies inserted verbatim.
#include <algorithm>
#include <cassert>
#include <cmath>
#include <cfenv>
#include <cstdint>
#include <ctime>
#include <iostream>
#include <map>
#include <string>
#include <vector>
using uint8=uint8_t;using uint16=uint16_t;using uint32=uint32_t;using int32=int32_t;using ObjectGuid=uint64_t;
#define MANGOS_ASSERT assert
#define DEBUG_LOG(...) (void)0
#define MAX_QUEST_LOG_SIZE 20
#define QUEST_OBJECTIVES_COUNT 4
#define QUEST_ITEM_OBJECTIVES_COUNT 4
#define MAX_EFFECT_INDEX 3
#define IN_MILLISECONDS 1000
#define MAX_MONEY_AMOUNT 0x7ffffffeu
static std::vector<std::string> events;
void emit(std::string s){events.push_back(s);}
enum QuestStatus {QUEST_STATUS_NONE=0,QUEST_STATUS_COMPLETE=1,QUEST_STATUS_INCOMPLETE=3};
enum QuestSpecialFlags {QUEST_SPECIAL_FLAG_DELIVER=8,QUEST_SPECIAL_FLAG_SPEAKTO=16,QUEST_SPECIAL_FLAG_KILL_OR_CAST=32,QUEST_SPECIAL_FLAG_EXPLORATION_OR_EVENT=2,QUEST_SPECIAL_FLAG_TIMED=64};
enum {QUEST_FLAGS_AUTO_REWARDED=0x400,QUEST_FLAGS_NO_MONEY_FROM_XP=0x100,QUEST_STATE_COMPLETE=1,QUEST_NEW=0,QUEST_CHANGED=1};
enum {TYPEID_UNIT=1,TYPEID_ITEM=2,TYPEID_CONTAINER=3,TYPEID_GAMEOBJECT=4,TYPEID_PLAYER=5,TYPEMASK_ITEM=2};
enum {INVALIDREASON_DONT_HAVE_REQ=1,INVALIDREASON_QUEST_FAILED_WRONG_RACE=2,INVALIDREASON_QUEST_ALREADY_ON=3,SMSG_QUESTLOG_FULL=4};
enum {EQUIP_ERR_OK=0,EQUIP_ERR_ITEM_NOT_FOUND=1,NULL_BAG=0,NULL_SLOT=0,MAIL_CHECK_MASK_HAS_BODY=0,TRIGGERED_OLD_TRIGGERED=0};
enum {SCRIPT_TYPE_QUEST_START,SCRIPT_TYPE_QUEST_END,SPELL_EFFECT_LEARN_SPELL,SPELL_EFFECT_CREATE_ITEM,TARGET_UNIT,TARGET_UNIT_FRIEND};
enum {CONFIG_FLOAT_RATE_XP_QUEST,CONFIG_FLOAT_RATE_DROP_MONEY,CONFIG_UINT32_MAX_PLAYER_LEVEL};
struct Player;struct Quest;
struct Object {virtual ~Object()=default;virtual int GetTypeId()const{return TYPEID_UNIT;}uint32 GetEntry()const{return 900;}bool isType(int)const{return false;}};
struct Unit:Object {void CastSpell(Player*,const void*,int){assert(false);}};
struct Creature:Unit{};struct GameObject:Object{};
using InventoryResult=int;using ItemPosCountVec=std::vector<int>;
struct Item:Object {int GetBagSlot()const{return 0;}int GetSlot()const{return 0;}static int GenerateItemRandomPropertyId(uint32){assert(false);return 0;}};
struct FactionEntry{};
struct FactionStore {FactionEntry* LookupEntry(uint32){assert(false);return nullptr;}}sFactionStore;
struct Reputation {void SetVisible(const FactionEntry*){assert(false);}int GetReputation(uint32)const{assert(false);return 0;}};
struct SpellEntry {int Effect[3]={},EffectImplicitTargetA[3]={};};
struct SpellTemplate {template<class T>const T* LookupEntry(uint32){assert(false);return nullptr;}}sSpellTemplate;
struct SpellArea {void ApplyOrRemoveSpellIfCan(Player*,uint32,uint32,bool)const{assert(false);}};
using SpellAreaForAreaMap=std::multimap<uint32,const SpellArea*>;
using SpellAreaForAreaMapBounds=std::pair<SpellAreaForAreaMap::const_iterator,SpellAreaForAreaMap::const_iterator>;
struct SpellMgr {SpellAreaForAreaMap empty;SpellAreaForAreaMapBounds GetSpellAreaForAreaMapBounds(uint32){return {empty.end(),empty.end()};}}sSpellMgr;
struct Map {enum {SCRIPT_EXEC_PARAM_UNIQUE_BY_SOURCE};void ScriptsStart(int,uint32,Object*,Player*,int){assert(false);}};
struct ScriptMgr {void OnQuestAccept(Player*,Object*,const Quest*);bool OnQuestRewarded(Player*,Object*,const Quest*);}sScriptDevAIMgr;
struct World {float xp=1,money=1;uint32 maxlevel=60;float getConfig(int key){return key==CONFIG_FLOAT_RATE_XP_QUEST?xp:key==CONFIG_FLOAT_RATE_DROP_MONEY?money:float(maxlevel);}}sWorld;
struct Quest {
 uint32 QuestId=1,QuestLevel=1,RewMoneyMaxLevel=60,MinLevel=1,MaxLevel=60,RequiredClasses=0,RequiredRaces=0;
 int32 RewOrReqMoney=7,PrevQuestId=0,NextQuestId=0;uint32 NextQuestInChain=0,BreadcrumbForQuestId=0;uint16 flags=0,special=48;bool active=true;
 uint32 ReqItemId[4]={},ReqItemCount[4]={},ReqSpell[4]={},ReqCreatureOrGOCount[4]={2,0,0,0};int32 ReqCreatureOrGOId[4]={100,0,0,0};
 uint32 RewChoiceItemId[6]={},RewChoiceItemCount[6]={},RewItemId[4]={},RewItemCount[4]={};
 using PrevQuests=std::vector<int32>;PrevQuests prevQuests;std::vector<uint32>prevChainQuests;
 uint32 GetQuestId()const{return QuestId;}uint32 GetQuestLevel()const{return QuestLevel;}uint32 GetMinLevel()const{return MinLevel;}uint32 GetMaxLevel()const{return MaxLevel;}
 uint32 GetRequiredClasses()const{return RequiredClasses;}uint32 GetRequiredRaces()const{return RequiredRaces;}
 bool HasQuestFlag(int f)const{return flags&f;}bool HasSpecialFlag(QuestSpecialFlags f)const{return special&f;}
 bool IsAutoComplete()const{return false;}bool IsActive()const{return active;}bool IsWeekly()const{return false;}bool IsRepeatable()const{return false;}bool IsAllowedInRaid()const{return false;}
 uint32 GetRepObjectiveFaction()const{return 0;}int32 GetRepObjectiveValue()const{return 0;}uint32 GetLimitTime()const{return 0;}
 int32 GetPrevQuestId()const{return PrevQuestId;}int32 GetNextQuestId()const{return NextQuestId;}int32 GetExclusiveGroup()const{return 0;}uint32 GetNextQuestInChain()const{return NextQuestInChain;}
 uint32 GetQuestStartScript()const{return 0;}uint32 GetQuestCompleteScript()const{return 0;}uint32 GetSrcItemId()const{return 0;}
 uint32 GetRewChoiceItemsCount()const{return 0;}uint32 GetRewItemsCount()const{return 0;}uint32 GetRewMailTemplateId()const{return 0;}uint32 GetRewMailDelaySecs()const{return 0;}
 uint32 GetRewSpellCast()const{return 0;}uint32 GetRewSpell()const{return 0;}
 uint32 XPValue(Player*)const;int32 GetRewOrReqMoney()const;uint32 GetRewMoneyMaxLevel()const;
};
using ExclusiveQuestGroupsMap=std::multimap<int32,uint32>;using ExclusiveQuestGroupsMapBounds=std::pair<ExclusiveQuestGroupsMap::const_iterator,ExclusiveQuestGroupsMap::const_iterator>;
struct Logger {template<class... T>void outErrorDb(T...){assert(false);}}sLog;
using QuestMap=std::map<uint32,Quest*>;
struct ObjectMgr {std::map<uint32,Quest> rows;ExclusiveQuestGroupsMap empty;const Quest*GetQuestTemplate(uint32 id)const{auto it=rows.find(id);return it==rows.end()?nullptr:&it->second;}ExclusiveQuestGroupsMapBounds GetExclusiveQuestGroupsMapBounds(int32){assert(false);return {empty.end(),empty.end()};}}sObjectMgr;
struct QuestStatusData {QuestStatus m_status=QUEST_STATUS_NONE;bool m_rewarded=false,m_explored=false;uint32 m_timer=0,m_itemcount[4]={},m_creatureOrGOcount[4]={};int uState=QUEST_NEW;};
using QuestStatusMap=std::map<uint32,QuestStatusData>;
struct WorldPacket {WorldPacket(int,int){}};
struct Session {void SendPacket(WorldPacket&){}};
struct Group {bool IsRaidGroup(){assert(false);return false;}};
struct Player:Unit {
 QuestStatusMap mQuestStatus;uint32 log[20]={};uint32 level=1,race=1,klass=8,money=0,xp=0;bool split=false,trace_views=false;std::map<uint32,uint32>carried,bank;
 int GetTypeId()const override{return TYPEID_PLAYER;}uint32 GetLevel()const{return level;}uint32 getClassMask()const{return 1u<<(klass-1);}uint32 getRaceMask()const{return 1u<<(race-1);}
 uint32 GetMoney()const{return money;}uint32 GetItemCount(uint32 id,bool include_bank=false)const{auto a=carried.find(id),b=bank.find(id);return (a==carried.end()?0:a->second)+(include_bank&&b!=bank.end()?b->second:0);}
 QuestStatus GetQuestStatus(uint32 id)const{auto p=mQuestStatus.find(id);return p==mQuestStatus.end()?QUEST_STATUS_NONE:p->second.m_status;}bool GetQuestRewardStatus(uint32 id)const{auto p=mQuestStatus.find(id);return p!=mQuestStatus.end()&&p->second.m_rewarded;}
 const QuestStatusMap&getQuestStatusMap()const{return mQuestStatus;}QuestStatusMap&getQuestStatusMap(){return mQuestStatus;}
 Group*GetGroup(){return nullptr;}Reputation&GetReputationMgr()const{static Reputation r;return r;}Map*GetMap(){static Map m;return &m;}Session*GetSession()const{static Session s;return &s;}
 void SendCanTakeQuestResponse(uint32)const{}void SendEquipError(int,const void*,const void*,int=0,uint32=0)const{}
 bool SatisfyQuestSkill(const Quest*,bool)const{return true;}bool SatisfyQuestCondition(const Quest*,bool)const{return true;}bool SatisfyQuestReputation(const Quest*,bool)const{return true;}
 bool SatisfyQuestTimed(const Quest*,bool)const{return true;}bool SatisfyQuestExclusiveGroup(const Quest*,bool)const{return true;}bool SatisfyQuestBreadcrumbQuest(const Quest*,bool)const{return true;}bool SatisfyQuestDependentBreadcrumbQuests(const Quest*,bool)const{return true;}bool SatisfyQuestWeek(const Quest*)const{return true;}
 bool CanGiveQuestSourceItemIfNeed(const Quest*)const{return true;}void GiveQuestSourceItemIfNeed(const Quest*){}
 uint32 GetQuestSlotQuestId(uint16 i)const{return log[i];}void SetQuestSlot(uint16 slot,uint32 id,uint32=0){log[slot]=id;emit("log:"+std::to_string(slot)+":"+std::to_string(id));}
 void SetQuestSlotState(uint16 slot,int){emit("bit:"+std::to_string(slot)+":1");}void RemoveQuestSlotState(uint16 slot,int){emit("bit:"+std::to_string(slot)+":0");}
 void UpdateForQuestWorldObjects(){std::string view="world";if(trace_views){view+="@";for(uint32 id:log)view+=std::to_string(id)+"/";view+="@";for(const auto&kv:mQuestStatus){const auto&q=kv.second;view+=std::to_string(kv.first)+":"+std::to_string(q.m_status)+":"+std::to_string(q.m_rewarded);for(uint32 n:q.m_itemcount)view+=":"+std::to_string(n);for(uint32 n:q.m_creatureOrGOcount)view+=":"+std::to_string(n);view+=";";}}emit(view);}void AddTimedQuest(uint32){assert(false);}void RemoveTimedQuest(uint32 id){emit("timed:"+std::to_string(id));}
 void DestroyItem(int,int,bool){assert(false);}void DestroyItemCount(uint32 id,uint32 count,bool){while(count){uint32 step=split?1:count;assert(carried[id]>=step);ItemRemovedQuestCheck(id,step);carried[id]-=step;emit("destroy:"+std::to_string(id)+":"+std::to_string(step));count-=step;}}
 int CanStoreNewItem(int,int,ItemPosCountVec&,uint32,uint32)const{assert(false);return 1;}Item*StoreNewItem(ItemPosCountVec&,uint32,bool,int){assert(false);return nullptr;}void SendNewItem(Item*,uint32,bool,bool,bool,bool){assert(false);}
 void RewardReputation(const Quest*q){emit("rep:"+std::to_string(q->QuestId));}void GiveXP(uint32 amount,void*){xp+=amount;emit("xp:"+std::to_string(amount));}
 void SetMoney(uint32 v){money=v;MoneyChanged(money);}
 void ModifyMoney(int32 d){emit("money:"+std::to_string(d));ModifyMoneyNative(d);}
 // PINNED_MODIFY_MONEY
 void SetWeeklyQuestStatus(uint32){assert(false);}void SendQuestReward(const Quest*q,uint32 amount){emit("announce:"+std::to_string(q->QuestId)+":"+std::to_string(amount));}
 void GetZoneAndAreaId(uint32&zone,uint32&area){zone=area=0;}
 void SendQuestUpdateAddCreatureOrGo(const Quest*q,ObjectGuid guid,int i,uint32 count){emit("kill:"+std::to_string(q->QuestId)+":"+std::to_string(guid)+":"+std::to_string(i)+":"+std::to_string(count));}
 void SendQuestUpdateAddItem(const Quest*q,int i,uint32 previous,uint32 added){emit("item:"+std::to_string(q->QuestId)+":"+std::to_string(i)+":"+std::to_string(previous)+":"+std::to_string(added));}
 bool CanTakeQuest(const Quest*,bool)const;bool CanAddQuest(const Quest*,bool)const;bool CanCompleteQuest(uint32)const;bool CanRewardQuest(const Quest*,bool)const;bool CanRewardQuest(const Quest*,uint32,bool)const;
 void AddQuest(const Quest*,Object*);void CompleteQuest(uint32);void IncompleteQuest(uint32);void RewardQuest(const Quest*,uint32,Object*,bool);
 bool SatisfyQuestLevel(const Quest*,bool)const;bool SatisfyQuestClass(const Quest*,bool)const;bool SatisfyQuestRace(const Quest*,bool)const;bool SatisfyQuestPreviousQuest(const Quest*,bool)const;bool SatisfyQuestNextChain(const Quest*,bool)const;bool SatisfyQuestPrevChain(const Quest*,bool)const;bool SatisfyQuestStatus(const Quest*,bool)const;bool SatisfyQuestLog(bool)const;
 bool IsCurrentQuest(uint32,uint8=0)const;void SetQuestStatus(uint32,QuestStatus);uint16 FindQuestSlot(uint32)const;void AdjustQuestReqItemCount(const Quest*,QuestStatusData&);void ItemAddedQuestCheck(uint32,uint32);void ItemRemovedQuestCheck(uint32,uint32);void KilledMonsterCredit(uint32,ObjectGuid);void TalkedToCreature(uint32,ObjectGuid);void MoneyChanged(uint32);
};
void ScriptMgr::OnQuestAccept(Player*,Object*g,const Quest*q){emit("accept:"+std::to_string(q->QuestId)+":"+std::to_string(g->GetEntry()));}
bool ScriptMgr::OnQuestRewarded(Player*,Object*g,const Quest*q){emit("reward:"+std::to_string(q->QuestId)+":"+std::to_string(g->GetEntry()));return false;}
struct MailDraft {MailDraft(uint32){assert(false);}void SendMailTo(Player*,Object*,int,uint32){assert(false);}};
void load_relations(){QuestMap mQuestTemplates;for(auto&kv:sObjectMgr.rows)mQuestTemplates.emplace(kv.first,&kv.second);for(auto&kv:mQuestTemplates){Quest*qinfo=kv.second;
// PINNED_RELATION_LOADER
}}
// PINNED_METHODS
void output(const std::string&name,const Player&p,uint32 id){const auto&q=p.mQuestStatus.at(id);std::cout<<name<<" | "<<q.m_status<<' '<<q.m_rewarded<<' '<<q.m_itemcount[0]<<' '<<q.m_creatureOrGOcount[0]<<' '<<p.log[0]<<' '<<p.money<<' '<<p.xp<<" | ";for(size_t i=0;i<events.size();++i){if(i)std::cout<<',';std::cout<<events[i];}std::cout<<'\n';events.clear();}
int main(){std::fesetround(FE_TONEAREST);Creature giver;
 for(int kind=0;kind<3;++kind)for(uint32 stock:{0u,1u,2u,3u})for(uint32 bank:{0u,2u})for(int32 price:{-3,0,7})for(uint32 startmoney:{0u,3u,MAX_MONEY_AMOUNT-1})for(uint32 level:{1u,60u})for(bool talk:{false,true}){
  sObjectMgr.rows.clear();Quest q;q.RewOrReqMoney=price;q.special=kind==0?48:kind==1?8:56;if(kind!=0){q.ReqItemId[0]=200;q.ReqItemCount[0]=2;}if(kind==1){q.ReqCreatureOrGOId[0]=0;q.ReqCreatureOrGOCount[0]=0;}sObjectMgr.rows[1]=q;Player p;p.carried[200]=stock;p.bank[200]=bank;p.money=startmoney;p.level=level;
  std::string name=std::to_string(kind)+" "+std::to_string(stock)+" "+std::to_string(bank)+" "+std::to_string(price)+" "+std::to_string(startmoney)+" "+std::to_string(level)+" "+std::to_string(talk);
  assert(p.CanTakeQuest(&q,false)&&p.CanAddQuest(&q,false));events.clear();p.AddQuest(&q,&giver);if(p.CanCompleteQuest(1))p.CompleteQuest(1);output("a "+name,p,1);
  p.KilledMonsterCredit(100,500);output("k "+name,p,1);if(talk)p.TalkedToCreature(100,501);else p.KilledMonsterCredit(100,501);output("l "+name,p,1);
  p.carried[200]+=2;p.ItemAddedQuestCheck(200,2);output("i "+name,p,1);
  bool can=p.CanRewardQuest(&q,0,false);if(can)p.RewardQuest(&q,0,&giver,true);output(std::string(can?"r ":"n ")+name,p,1);
 }
 // Inbound loader-derived relations and source eligibility/order probes.
 for(int relation=0;relation<5;++relation)for(int history=0;history<4;++history)for(int deny=0;deny<6;++deny){
  sObjectMgr.rows.clear();Quest one;one.QuestId=1;Quest two;two.QuestId=2;
  if(relation==0)two.PrevQuestId=1;if(relation==1)two.PrevQuestId=-1;if(relation==2)one.NextQuestId=2;if(relation==3)one.NextQuestId=-2;if(relation==4)one.NextQuestInChain=2;
  if(deny==1)two.RequiredClasses=1;if(deny==2)two.RequiredRaces=2;if(deny==3)two.MinLevel=2;if(deny==4)two.active=false;
  sObjectMgr.rows.emplace(1,one);sObjectMgr.rows.emplace(2,two);load_relations();Player p;
  if(history){auto&st=p.mQuestStatus[1];st.m_status=history==1?QUEST_STATUS_INCOMPLETE:QUEST_STATUS_COMPLETE;st.m_rewarded=history==3;}
  if(deny==5)p.mQuestStatus[2].m_status=QUEST_STATUS_INCOMPLETE;
  auto*q=sObjectMgr.GetQuestTemplate(2);std::cout<<"g "<<relation<<' '<<history<<' '<<deny<<" | "<<p.CanTakeQuest(q,false)<<" | ";
  for(auto prev:q->prevQuests)std::cout<<prev<<',';std::cout<<" | ";for(auto prev:q->prevChainQuests)std::cout<<prev<<',';std::cout<<'\n';
 }

 // Shared collection items: native removal may raise counters above requirements.
 for(bool reward:{false,true})for(bool split:{false,true})for(uint32 stock:{3u,4u,5u,6u}){
  sObjectMgr.rows.clear();Player p;p.carried[200]=stock;p.split=split;p.trace_views=true;
  for(uint32 id:{1u,2u}){Quest q;q.QuestId=id;q.special=8;q.ReqItemId[0]=200;q.ReqItemCount[0]=id==1?3:2;q.ReqCreatureOrGOId[0]=0;q.ReqCreatureOrGOCount[0]=0;q.RewOrReqMoney=0;sObjectMgr.rows[id]=q;p.AddQuest(&q,&giver);if(p.CanCompleteQuest(id))p.CompleteQuest(id);}
  events.clear();if(reward){auto*q=sObjectMgr.GetQuestTemplate(1);assert(p.CanRewardQuest(q,0,false));p.RewardQuest(q,0,&giver,true);}else p.DestroyItemCount(200,3,true);
  for(uint32 id:{1u,2u})output("d "+std::to_string(reward)+" "+std::to_string(split)+" "+std::to_string(stock)+" "+std::to_string(id)+" "+std::to_string(p.carried[200]),p,id);
 }

}
