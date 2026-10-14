// SPDX-License-Identifier: GPL-3.0-or-later
// Authored adapters and rows; exact CMaNGOS methods are inserted by generator.
// CMaNGOS contributors, upstream GPL-2.0-or-later; without warranty.
#include <algorithm>
#include <array>
#include <cassert>
#include <cfenv>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>
using uint8=uint8_t;using uint32=uint32_t;using int32=int32_t;
#define MAX_REPUTATION_RANK 8
#define MIN_REPUTATION_RANK REP_HATED
#define MAX_SPILLOVER_FACTIONS 4
#define QUEST_REPUTATIONS_COUNT 5
using RepListID=uint32;
// PINNED_DECLS
using FactionStateList=std::map<RepListID,FactionState>;
struct FactionStore {std::map<uint32,FactionEntry> rows;const FactionEntry*LookupEntry(uint32 id)const{auto it=rows.find(id);return it==rows.end()?nullptr:&it->second;}uint32 GetNumRows()const{return rows.empty()?0:rows.rbegin()->first+1;}}sFactionStore;
struct ObjectMgr {std::map<uint32,RepRewardRate> rates;std::map<uint32,RepSpilloverTemplate> spills;const RepRewardRate*GetRepRewardRate(uint32 id)const{auto it=rates.find(id);return it==rates.end()?nullptr:&it->second;}const RepSpilloverTemplate*GetRepSpilloverTemplate(uint32 id)const{auto it=spills.find(id);return it==spills.end()?nullptr:&it->second;}}sObjectMgr;
enum {CONFIG_FLOAT_RATE_REPUTATION_GAIN,CONFIG_FLOAT_RATE_REPUTATION_LOWLEVEL_KILL,CONFIG_FLOAT_RATE_REPUTATION_LOWLEVEL_QUEST,SPELL_AURA_MOD_REPUTATION_GAIN,SPELL_AURA_MOD_FACTION_REPUTATION_GAIN};
struct World {float gain=1,kill=.2f,quest=.2f;float getConfig(int id)const{return id==CONFIG_FLOAT_RATE_REPUTATION_GAIN?gain:id==CONFIG_FLOAT_RATE_REPUTATION_LOWLEVEL_KILL?kill:quest;}}sWorld;
enum {SMSG_SET_FACTION_STANDING,SMSG_INITIALIZE_FACTIONS,SMSG_SET_FACTION_VISIBLE};
struct WorldPacket {int type;std::vector<uint32> values;WorldPacket(int t,int):type(t){}size_t wpos()const{return values.size();}WorldPacket&operator<<(uint32 n){values.push_back(n);return *this;}template<class T>void put(size_t p,T n){values[p]=n;}};
struct Session {bool PlayerLoading()const{return false;}};
struct Player;
struct ReputationMgr {
 Player*m_player;FactionStateList m_factions;explicit ReputationMgr(Player*p):m_player(p){}
 static const int32 PointsInRank[MAX_REPUTATION_RANK];static const int32 Reputation_Cap=42999,Reputation_Bottom=-42000;
 static ReputationRank ReputationToRank(int32);const FactionState*GetState(const FactionEntry*)const;const FactionState*GetState(RepListID)const;
 int32 GetReputation(const FactionEntry*)const;int32 GetBaseReputation(const FactionEntry*)const;uint32 GetDefaultStateFlags(const FactionEntry*)const;ReputationRank GetRank(const FactionEntry*)const;ReputationRank GetBaseRank(const FactionEntry*)const;bool IsAtWar(const FactionEntry*)const;
 void Initialize();bool SetReputation(const FactionEntry*,int32,bool,bool);bool SetOneFactionReputation(const FactionEntry*,int32,bool);
 bool ModifyReputation(const FactionEntry*f,int32 n,bool no=false){return SetReputation(f,n,true,no);}
 void SetVisible(FactionState*);void SetVisible(const FactionEntry*);void SetAtWar(RepListID,bool);void SetAtWar(FactionState*,bool);void SetInactive(RepListID,bool);void SetInactive(FactionState*,bool);
 void SendState(const FactionState*);void SendInitialReputations();void SendVisible(const FactionState*)const;
};
struct Quest {uint32 RewRepFaction[5]={};int32 RewRepValue[5]={};uint32 flags=0;int32 level=1;uint32 GetRewFactionFlags()const{return flags;}int32 GetQuestLevel()const{return level;}};
struct Player {
 ReputationMgr rep{this};bool reentrant=false,triggered=false;uint32 level=1,race=1,klass=128;std::vector<std::string>events;Session session;
 uint32 GetLevel()const{return level;}uint32 getRaceMask()const{return race;}uint32 getClassMask()const{return klass;}Session*GetSession(){return &session;}
 int GetTotalAuraModifier(int)const{return 0;}int GetTotalAuraModifierByMiscValue(int,int)const{return 0;}
 ReputationRank GetReputationRank(uint32 id)const{return rep.GetRank(sFactionStore.LookupEntry(id));}ReputationMgr&GetReputationMgr(){return rep;}
 uint32 GetQuestLevelForPlayer(const Quest*q)const{return q&&q->GetQuestLevel()>0?q->GetQuestLevel():GetLevel();}
 int32 CalculateReputationGain(ReputationSource,int32,int32,uint32,bool=false)const;void RewardReputation(const Quest*);
 std::string snapshot()const{std::ostringstream out;for(const auto&kv:rep.m_factions){const auto&s=kv.second;out<<s.ReputationListID<<':'<<s.Standing<<':'<<s.Flags<<':'<<s.needSend<<':'<<s.needSave<<',';}return out.str();}
 void ReputationChanged(const FactionEntry*f){events.push_back("C:"+std::to_string(f->ID)+"@"+snapshot());if(reentrant&&!triggered&&f->ID==101){triggered=true;level=60;rep.SetReputation(sFactionStore.LookupEntry(101),42000,false,true);}}
 void SendDirectMessage(const WorldPacket&p){std::ostringstream out;if(p.type==SMSG_SET_FACTION_VISIBLE){out<<"V:"<<p.values[0];}else if(p.type==SMSG_SET_FACTION_STANDING){out<<"S:";for(size_t i=1;i<p.values.size();i+=2)out<<p.values[i]<<':'<<int32(p.values[i+1])<<',';}else{return;}events.push_back(out.str());}
};
namespace MaNGOS {namespace XP {
// PINNED_XP
}}
// PINNED_METHODS
float bits(uint32 n){float out;std::memcpy(&out,&n,sizeof(out));return out;}
const std::array<uint32,8> rates={0x00000000,0x3e4ccccd,0x3f000000,0x3f800000,0x3f8ccccd,0x3fc00000,0x40551eb8,0x3f7fffff};
const std::array<int32,12> bases={-42000,-6001,-6000,-3001,-3000,-1,0,2999,3000,8999,21000,41999};
const std::array<int32,9> values={-16777217,-1001,-101,-1,0,1,101,1001,16777217};
void configure(uint32 c){sWorld.gain=bits(rates[(c/11)%8]);sWorld.kill=bits(rates[(c/13)%4]);sWorld.quest=bits(rates[(c/17)%4]);}
void seed(uint32 c){sFactionStore.rows.clear();sObjectMgr.rates.clear();sObjectMgr.spills.clear();for(uint32 i=0;i<4;++i){FactionEntry f{};f.ID=100+i;f.reputationListID=i;f.BaseRepRaceMask[0]=c%3==0?0:2;f.BaseRepRaceMask[1]=1;f.BaseRepClassMask[1]=128;f.BaseRepValue[0]=bases[(c+i)%12];f.BaseRepValue[1]=bases[(c+i+3)%12];f.ReputationFlags[0]=(c/3+i)%64;f.ReputationFlags[1]=(c/5+i)%64;f.team=100;sFactionStore.rows[f.ID]=f;}
 RepSpilloverTemplate spill{};spill.faction[0]=101;spill.faction[1]=101;spill.faction[2]=100;spill.faction[3]=102;spill.faction_rate[0]=spill.faction_rate[1]=.5f;spill.faction_rate[2]=.2f;spill.faction_rate[3]=-.25f;spill.faction_rank[0]=spill.faction_rank[1]=(c/7)%8;spill.faction_rank[2]=spill.faction_rank[3]=7;sObjectMgr.spills[100]=spill;
 if(c%3){sObjectMgr.rates[100]={bits(rates[(c/19)%8]),bits(rates[(c/23)%8]),bits(rates[(c/29)%8])};}
 configure(c);
}
int main(int argc,char**argv){std::fesetround(FE_TONEAREST);bool reentrant=argc>1&&std::string(argv[1])=="--host";if(argc>1&&!reentrant){for(uint32 c=0;c<8192;++c){seed(c);Player p;p.level=1+c%63;uint32 target=(c*7)%66;auto source=ReputationSource(c%3);int32 value=values[(c/9)%9];int32 result=p.CalculateReputationGain(source,value,100,target,c%2);std::cout<<c<<'\t'<<result<<'\n';}return 0;}
 for(uint32 c=0;c<(reentrant?256u:2048u);++c){seed(c);Player p;p.reentrant=reentrant;p.level=1+c%63;p.rep.Initialize();if(c%2)p.rep.SendInitialReputations();p.events.clear();int32 value=values[(c/9)%9];bool increase=p.rep.SetReputation(sFactionStore.LookupEntry(100),value,(c/2)%2,(c/4)%2);p.rep.SetAtWar(RepListID(2),(c/8)%2);p.rep.SetInactive(RepListID(1),(c/16)%2);p.rep.SetVisible(sFactionStore.LookupEntry(103));Quest q;q.level=c%7==0?-1:1+(c*7)%63;q.flags=c%32;q.RewRepFaction[0]=100;q.RewRepFaction[1]=101;q.RewRepFaction[2]=100;q.RewRepFaction[3]=103;q.RewRepValue[0]=value;q.RewRepValue[1]=-value;q.RewRepValue[2]=value/3;q.RewRepValue[3]=0;p.RewardReputation(&q);std::cout<<c<<'\t'<<increase<<'\t'<<p.snapshot()<<'\t';for(const auto&e:p.events)std::cout<<e<<'|';std::cout<<'\t';for(uint32 i=0;i<4;++i){auto*f=sFactionStore.LookupEntry(100+i);std::cout<<p.rep.GetBaseReputation(f)<<':'<<p.rep.GetReputation(f)<<':'<<p.rep.GetRank(f)<<':'<<p.rep.IsAtWar(f)<<',';}std::cout<<'\n';}
}
