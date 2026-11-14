// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic source-state adapters. No game rows/assets. Pinned methods inserted verbatim.
#include <algorithm>
#include <cassert>
#include <cstdint>
#include <ctime>
#include <iostream>
#include <map>
#include <string>
#include <vector>
using uint8=uint8_t; using uint32=uint32_t; using uint64=uint64_t;
using DamageEffectType=int; using CurrentSpellTypes=int;
#define DEBUG_FILTER_LOG(...) (void)0
#define DEBUG_LOG(...) (void)0
static std::vector<std::string> events;
void emit(const std::string& v) { events.push_back(v); }
enum DeathState { ALIVE=0, JUST_DIED=1, CORPSE=2, JUST_ALIVED=3 };
struct LoggedState {
 DeathState value=ALIVE;
 operator DeathState() const {return value;}
 LoggedState& operator=(DeathState s) {value=s;emit("state:"+std::to_string(s));return *this;}
};
struct LoggedTimer {
 uint32 value=0;
 operator uint32() const {return value;}
 LoggedTimer& operator=(uint32 v){value=v;emit("timer:"+std::to_string(v));return *this;}
 void operator-=(uint32 v){*this=value-v;}
};
struct LoggedExpire {
 time_t value=0;
 operator time_t()const{return value;}
 LoggedExpire& operator=(time_t v){value=v;emit("expire:"+std::to_string(v));return *this;}
};
static time_t clock_now=1000;
time_t test_time(void*) {return clock_now;}
#define time test_time
#define MINUTE 60
#define IN_MILLISECONDS 1000
#define DEATH_EXPIRE_STEP (5*MINUTE)
#define MAX_DEATH_COUNT 3
static const uint32 corpseReclaimDelay[MAX_DEATH_COUNT]={30,60,120};
enum {TYPEID_PLAYER,TYPEID_UNIT,CLASS_PRIEST=5,CLASS_MAGE=8};
enum {INSTAKILL=9,SPELL_AURA_DUMMY=10,PROC_FLAG_KILL,PROC_FLAG_NONE,PROC_EX_NONE,PLAYER_SELF_RES_SPELL,SMSG_PARTYKILLLOG,SMSG_DURABILITY_DAMAGE_DEATH,TRIGGERED_OLD_TRIGGERED,DUEL_INTERRUPTED,PET_SAVE_REAGENTS,PET_SAVE_AS_CURRENT,GROUP_UPDATE_FLAG_STATUS,UNIT_FIELD_FLAGS,UNIT_FLAG_SKINNABLE,CONFIG_UINT32_CREATURE_RESPAWN_AGGRO_DELAY,AURA_STATE_HEALTHLESS_20_PERCENT,UNIT_DYNAMIC_FLAGS,UNIT_DYNFLAG_NONE,PLAYER_FIELD_BYTES,PLAYER_FIELD_BYTE_RELEASE_TIMER,PLAYER_EXTRA_PVP_DEATH,CONFIG_BOOL_DEATH_CORPSE_RECLAIM_DELAY_PVP,CONFIG_BOOL_DEATH_CORPSE_RECLAIM_DELAY_PVE,AURA_REMOVE_BY_DEATH};
enum {SPELL_ATTR_EX3_NO_DURABILITY_LOSS=0x20};
enum {CURRENT_GENERIC_SPELL,CURRENT_AUTOREPEAT_SPELL,CURRENT_CHANNELED_SPELL};
enum {SPELL_STATE_CREATED,SPELL_STATE_TARGETING,SPELL_STATE_CASTING,SPELL_STATE_DELAYED,SPELL_STATE_TRAVELING,SPELL_STATE_LANDING,SPELL_STATE_CHANNELING,SPELL_STATE_FINISHED};
struct Unit; struct Player; struct Group;
struct SpellEntry {uint32 Id=1,SpellIconID=0,attributesEx3=0; bool HasAttribute(int bit)const{return attributesEx3&bit;}};
struct WorldPacket {int opcode; WorldPacket(int op,int):opcode(op){} template<class T> WorldPacket& operator<<(T){return *this;}};
struct Session {void SendPacket(WorldPacket& packet){assert(packet.opcode==SMSG_DURABILITY_DAMAGE_DEATH);emit("durability_message");}};
struct Aura {SpellEntry spell;const SpellEntry* GetSpellProto(){return &spell;}};
using AuraList=std::vector<Aura*>;
struct Holder {uint64 id;uint32 attributes,attributesEx3; bool IsPassive()const{return attributes&0x40;} bool IsDeathPersistent()const{return attributesEx3&0x100000;}};
using SpellAuraHolderMap=std::multimap<uint32,Holder*>;
struct UnitAI {void KilledUnit(Unit*){emit("killer_killed");} void OnSpellInterrupt(const SpellEntry*){assert(false);}};
struct FormationData {void Remove(Player*){assert(false);}};
struct FormationSlot {FormationData* GetFormationData(){assert(false);return nullptr;}};
struct Motion {void Clear(bool,bool){emit("clear_motion");}bool MoveFall(){assert(false);return false;}void MoveIdle(){emit("idle");}};
struct Combat {void StopEvade(){emit("stop_evade");}};
struct Map {bool instance=false;bool Instanceable()const{return instance;}};
static Map current_map;
struct MapStore {Map* LookupEntry(uint32){return &current_map;}} sMapStore;
struct World {bool pve=true;bool getConfig(int config){if(config==CONFIG_BOOL_DEATH_CORPSE_RECLAIM_DELAY_PVE)return pve;assert(false);return false;}} sWorld;
struct InstanceData {void OnPlayerDeath(Player*){assert(false);}};
struct BattleGround {void HandleKillPlayer(Player*,Player*){assert(false);}};
struct OutdoorPvP {void HandlePlayerKill(Player*,Player*){assert(false);}};
struct OutdoorMgr {OutdoorPvP* GetScript(uint32){assert(false);return nullptr;}} sOutdoorPvPMgr;
struct Spell {
 SpellEntry spell;SpellEntry* m_spellInfo=&spell;int state=SPELL_STATE_CASTING;uint64 id=0;int slot=0;bool triggered=false,allow=false;
 int getState()const{return state;}bool CanBeInterrupted()const{return state<=SPELL_STATE_DELAYED||state==SPELL_STATE_CHANNELING;}
 void cancel(){emit("interrupt:"+std::to_string(slot)+":"+std::to_string(id));state=SPELL_STATE_FINISHED;}
 void SetReferencedFromCurrent(bool){}
};
struct Unit {
 bool player=false;uint64 id=0;uint32 health=50,mana=31;LoggedState m_deathState;SpellAuraHolderMap m_spellAuraHolders;Motion i_motionMaster;Spell* m_currentSpells[3]={nullptr,nullptr,nullptr};
 virtual ~Unit()=default;
 bool IsAlive()const{return m_deathState==ALIVE;}bool IsPlayer()const{return player;}int getClass()const{return CLASS_MAGE;}int GetTypeId()const{return player?TYPEID_PLAYER:TYPEID_UNIT;}
 Player* GetBeneficiaryPlayer(){return nullptr;}Unit* GetOwner(){return nullptr;}uint64 GetObjectGuid()const{return id;}
 const AuraList& GetAurasByType(int){static AuraList a;return a;}uint32 GetUInt32Value(int){return 0;}
 virtual void SetUInt32Value(int field,uint32 value){assert(field==UNIT_DYNAMIC_FLAGS&&value==UNIT_DYNFLAG_NONE);emit("dynamic_zero");}
 void CastSpell(void*,int,int,void*,Aura*){assert(false);}void SetHealth(uint32 h){health=h;emit("health_zero");}
 UnitAI* AI(){static UnitAI ai;return player?nullptr:&ai;}void PetOwnerKilledUnit(Unit*){emit("pet_owner_killed");}
 FormationSlot* GetFormationSlot(){return nullptr;}uint32 GetCachedZoneId(){return 0;}
 void CombatStop(){emit("combat_stop");}void CombatStopWithPets(bool){assert(false);}
 void BreakCharmOutgoing(){emit("charm_out");}void BreakCharmIncoming(){emit("charm_in");}void RemoveMiniPet(){emit("minipet");}void UnsummonAllTotems(){emit("totems");}
 void StopMoving(bool){emit("stop_move");}bool CanFly(){return false;}Combat GetCombatManager(){return {};}
 void ModifyAuraState(int state,bool enabled){assert(state==AURA_STATE_HEALTHLESS_20_PERCENT&&!enabled);emit("health20_clear");}
 void ClearAllReactives(){emit("reactives");}void ClearDiminishings(){emit("diminishings");}void ClearComboPointHolders(){emit("combo_holders");}
 void RemoveFlag(int,int){assert(false);}void ScheduleAINotify(int,bool){assert(false);}
 void RemoveSpellAuraHolder(Holder* holder,int mode){assert(mode==AURA_REMOVE_BY_DEATH);for(auto it=m_spellAuraHolders.begin();it!=m_spellAuraHolders.end();++it)if(it->second==holder){emit("aura:"+std::to_string(holder->id)+":"+std::to_string(it->first));m_spellAuraHolders.erase(it);return;}assert(false);}
 static void Kill(Unit*,Unit*,int,const SpellEntry*,bool,bool);
 virtual void SetDeathState(DeathState);
 void RemoveAllAurasOnDeath();void InterruptSpell(CurrentSpellTypes,bool);void InterruptNonMeleeSpells(bool,uint32=0);
};
struct Pet:Unit{};
struct Duel {Player* opponent=nullptr;};
struct Player:Unit {
 Duel* duel=nullptr;LoggedTimer m_deathTimer;LoggedExpire m_deathExpireTime;uint32 m_ExtraFlags=0;Session session;
 Player(){player=true;id=7;}
 Group* GetGroup(){return nullptr;}void SendDirectMessage(WorldPacket&) {assert(false);}bool InBattleGround(){return false;}
 void SetPvPDeath(bool pvp){assert(!pvp);emit("pvp_false");}void DurabilityLossAll(float percent,bool inventory){assert(percent==.10f&&!inventory);emit("durability_10");}
 Session* GetSession(){return &session;}void DuelComplete(int){assert(false);}BattleGround* GetBattleGround(){return nullptr;}
 void SetDrunkValue(int){emit("drunk_zero");}void ClearComboPoints(){emit("combo_clear");}void clearResurrectRequestData(){emit("res_request");}
 Pet* GetPet(){return nullptr;}void RemovePet(int mode){assert(mode==PET_SAVE_REAGENTS);emit("remove_pet");}void RemoveGuardians(){emit("guardians");}
 uint32 GetResurrectionSpellId(){return 0;}InstanceData* GetInstanceData(){return nullptr;}void SetGroupUpdateFlag(int){assert(false);}
 void SendAutoRepeatCancel(){emit("auto_cancel");}void SendMoveRoot(bool b){assert(b);emit("root");}uint32 GetMapId(){return 0;}Map* GetMap(){return &current_map;}
 void ApplyModByteFlag(int,int,int,bool b){emit("release_flag:"+std::to_string(b));}void UpdateObjectVisibility(){emit("visibility");}
 void RewardSinglePlayerAtKill(Unit*){assert(false);}
 void SetDeathState(DeathState)override;void KillPlayer();void UpdateCorpseReclaimDelay();uint32 getCorpseReclaimDelayHelper(time_t,time_t,bool)const;uint32 GetCorpseReclaimDelay(bool)const;
 void BuildPlayerRepop(){emit("build_repop");}void RepopAtGraveyard(){emit("repop_graveyard");}
 void update_death(uint32 diff){
 // PINNED_JUST_DIED
 // PINNED_TIMER
 }
};
struct Creature:Unit{Player* GetOriginalLootRecipient(){assert(false);return nullptr;}Group* GetGroupLootRecipient(){assert(false);return nullptr;}};
struct Group {void BroadcastPacketInRange(Unit*,WorldPacket&,bool,int){assert(false);}int GetMemberGroup(uint64){assert(false);return 0;}void RewardGroupAtKill(Unit*,Player*){assert(false);}};
struct ProcSystemArguments {ProcSystemArguments(Unit*,Unit*,int,int,int,int,int){}};
void ProcDamageAndSpell(const ProcSystemArguments&){emit("kill_proc");}
void JustKilledCreature(Unit*,Creature*,Player*){assert(false);}
// PINNED_METHODS
void print_events(){for(size_t i=0;i<events.size();++i){if(i)std::cout<<',';std::cout<<events[i];}std::cout<<'\n';}
void kill_case(int state,int traits,bool durability,int spellAttrs){
 Player p;Unit killer;killer.id=9;std::vector<Holder> holders={{11,0,0},{12,0x40,0},{13,0,0x100000},{14,0x40,0x100000}};
 for(size_t i=0;i<holders.size();++i)p.m_spellAuraHolders.emplace(100+uint32(i),&holders[i]);
 Spell generic,automatic,channel;Spell* spells[]={&generic,&automatic,&channel};
 for(int slot=0;slot<3;++slot){spells[slot]->id=21+slot;spells[slot]->slot=slot;spells[slot]->state=state;spells[slot]->triggered=traits&1;spells[slot]->allow=traits&2;p.m_currentSpells[slot]=spells[slot];}
 SpellEntry spell;spell.attributesEx3=uint32(spellAttrs);
 events.clear();Unit::Kill(&killer,&p,0,spellAttrs<0?nullptr:&spell,durability,false);
 std::cout<<"k "<<state<<' '<<traits<<' '<<durability<<' '<<spellAttrs<<" | "<<p.health<<' '<<p.mana<<' '<<int(p.m_deathState.value)<<" | ";print_events();
 events.clear();current_map.instance=false;sWorld.pve=true;clock_now=1000;p.update_death(17);
 std::cout<<"c "<<state<<' '<<traits<<' '<<durability<<' '<<spellAttrs<<" | "<<p.m_deathTimer.value<<' '<<p.m_deathExpireTime.value<<' '<<p.GetCorpseReclaimDelay(false)<<" | ";print_events();
}
void timer_case(bool instance,bool enabled,time_t prior,uint32 diff){
 Player p;current_map.instance=instance;sWorld.pve=enabled;p.m_deathState.value=JUST_DIED;p.m_deathExpireTime.value=prior;clock_now=1000;
 events.clear();p.update_death(diff);
 std::cout<<"t "<<instance<<' '<<enabled<<' '<<prior<<' '<<diff<<" | "<<p.m_deathTimer.value<<' '<<p.m_deathExpireTime.value<<' '<<p.GetCorpseReclaimDelay(false)<<" | ";print_events();
}
int main(){
 for(int state=0;state<8;++state)for(int traits=0;traits<4;++traits)for(bool durability:{false,true})for(int attrs:{-1,0,0x20})kill_case(state,traits,durability,attrs);
 for(bool instance:{false,true})for(bool enabled:{false,true})for(time_t prior:{0,999,1000,1001,1299,1300,1301,1599,1600,1899,1900})for(uint32 diff:{0u,1u,17u,359999u,360000u,360001u,0xffffffffu})timer_case(instance,enabled,prior,diff);
}
