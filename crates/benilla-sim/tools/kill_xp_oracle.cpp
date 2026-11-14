// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic adapters; generator inserts unchanged hash-pinned CMaNGOS bodies.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>
using uint8 = uint8_t; using uint32 = uint32_t; using uint64 = uint64_t;
uint32 bits(float f) { uint32 x; std::memcpy(&x, &f, 4); return x; }
float f32(uint32 x) { float f; std::memcpy(&f, &x, 4); return f; }
enum UIntConfig { CONFIG_UINT32_MAX_PLAYER_LEVEL };
enum FloatConfig { CONFIG_FLOAT_RATE_XP_KILL, CONFIG_FLOAT_RATE_REST_INGAME, CONFIG_FLOAT_RATE_REST_OFFLINE_IN_TAVERN_OR_CITY, CONFIG_FLOAT_RATE_REST_OFFLINE_IN_WILDERNESS };
struct World { uint32 cap=9; float rates[4]{1,1,1,1}; uint32 getConfig(UIntConfig) const { return cap; } float getConfig(FloatConfig key) const { return rates[key]; } } sWorld;
constexpr uint32 PLAYER_XP=0, PLAYER_NEXT_LEVEL_XP=1, PLAYER_REST_STATE_EXPERIENCE=2, PLAYER_BYTES_2=3, SMSG_LOG_XPGAIN=4;
constexpr uint8 REST_STATE_RESTED=1, REST_STATE_NORMAL=2;
#define DETAIL_LOG(...) ((void)0)
struct ObjectGuid { uint64 value{}; };
struct WorldPacket {
    std::vector<uint8> bytes;
    WorldPacket(uint32 op, uint32) { assert(op==SMSG_LOG_XPGAIN); }
    WorldPacket& operator<<(ObjectGuid v) { for (int i=0;i<8;i++) bytes.push_back(uint8(v.value>>(8*i))); return *this; }
    WorldPacket& operator<<(uint32 v) { for (int i=0;i<4;i++) bytes.push_back(uint8(v>>(8*i))); return *this; }
    WorldPacket& operator<<(uint8 v) { bytes.push_back(v); return *this; }
    WorldPacket& operator<<(float v) { return *this << bits(v); }
};
std::vector<std::string> trace;
std::string joined() { std::string out; for (const auto& v:trace) { if(!out.empty())out+=';'; out+=v; } return out.empty()?"-":out; }
struct Unit {
    uint32 level=1, health=100, others=0;
    uint32 GetLevel() const { return level; }
    uint32 GetMaxHealth() const { return health; }
    bool IsCreature() const { return true; }
    uint32 GetDamageDoneByOthers() const { return others; }
    uint32 GetModifierXpBasedOnDamageReceived(uint32);
    ObjectGuid GetObjectGuid() const { return {0xF130000001000001ULL}; }
};
struct Map { bool IsNoRaid() const { return true; } } map;
struct Creature : Unit {
    bool no_xp=false, critter=false;
    struct Info { float ExperienceMultiplier=1; } info;
    bool IsTotem() const { return false; } bool IsPet() const { return false; }
    bool IsNoXp() const { return no_xp; } bool IsCritter() const { return critter; }
    bool IsElite() const { return false; }
    Map* GetMap() const { return &map; } Info* GetCreatureInfo() { return &info; }
};
// PINNED_DAMAGE
namespace MaNGOS { namespace XP {
// PINNED_TRIVIAL
// PINNED_ZERO
// PINNED_BASE
// PINNED_GAIN
} }
struct Player;
struct Session { const Player* owner{}; void SendPacket(const WorldPacket&) const; };
struct Player : Unit {
    bool alive=true;
    uint32 xp=0, next=137, experience=0;
    float m_rest_bonus=0;
    uint8 rest_state=REST_STATE_NORMAL;
    Session session{this};
    bool IsAlive() const { return alive; }
    uint32 GetUInt32Value(uint32 key) const { return key==PLAYER_XP?xp:key==PLAYER_NEXT_LEVEL_XP?next:experience; }
    void record(const std::string& tag) const { trace.push_back(tag+","+std::to_string(level)+","+std::to_string(xp)+","+std::to_string(next)+","+std::to_string(bits(m_rest_bonus))+","+std::to_string(rest_state)+","+std::to_string(experience)); }
    void SetUInt32Value(uint32 key,uint32 value) { if(key==PLAYER_XP){xp=value;record("xp");}else{assert(key==PLAYER_REST_STATE_EXPERIENCE);experience=value;record("rest_xp");} }
    void SetByteValue(uint32 key,uint8 offset,uint8 value) { assert(key==PLAYER_BYTES_2&&offset==3);rest_state=value;record("rest_byte"); }
    float GetRestBonus() const { return m_rest_bonus; }
    Session const* GetSession() const { return &session; }
    uint32 GetXPRestBonus(uint32); void SetRestBonus(float); void SendLogXPGain(uint32,Unit*,uint32,float) const; void GiveXP(uint32,Creature*,float=1.f);
    float ComputeRest(time_t,bool=false,bool=false) const;
    // The existing progression oracle separately extracts the complete GiveLevel.
    // This trace adapter tests GiveXP's callback position and unchanged old XP.
    void GiveLevel(uint32 value) { record("level");level=value;next=level<sWorld.cap?100+37*level:0; }
};
void Session::SendPacket(const WorldPacket& p) const {
    owner->record("packet");
    std::ostringstream stream; stream << std::hex << std::setfill('0');
    for(uint8 value:p.bytes) stream<<std::setw(2)<<unsigned(value);
    trace.push_back(stream.str());
}
// PINNED_REST_BONUS
// PINNED_SET_REST
// PINNED_LOG
// PINNED_GIVE_XP
// PINNED_COMPUTE_REST
void xp_case(uint32 level,bool alive,uint32 old_xp,uint32 amount,uint32 rest_bits,uint8 state,uint32 cap) {
    sWorld.cap=cap; Player p; p.level=level;p.alive=alive;p.xp=old_xp;p.next=level<cap?100+37*level:0;p.m_rest_bonus=f32(rest_bits);p.rest_state=state;p.experience=uint32(p.m_rest_bonus);
    trace.clear();Creature victim;p.GiveXP(amount,&victim);
    std::cout<<"X\t"<<level<<'\t'<<alive<<'\t'<<old_xp<<'\t'<<amount<<'\t'<<rest_bits<<'\t'<<unsigned(state)<<'\t'<<cap<<'\t'<<p.level<<'\t'<<p.xp<<'\t'<<p.next<<'\t'<<bits(p.m_rest_bonus)<<'\t'<<unsigned(p.rest_state)<<'\t'<<p.experience<<'\t'<<joined()<<'\n';
}
int main() {
    std::fesetround(FE_TONEAREST);
    std::cout<<"# G player mob mult_bits rate_bits health others no_xp critter base_bits gain; X level alive old_xp amount rest_bits state cap -> level xp next rest_bits state experience trace; R seconds next interval rates[3] result_bits; S input_bits old_state level cap next result_bits state experience trace\n";
    for(uint32 pl=1;pl<=63;pl++)for(uint32 mob=1;mob<=63;mob++) {
        Unit p;p.level=pl;Creature c;c.level=mob;
        std::cout<<"G\t"<<pl<<'\t'<<mob<<'\t'<<bits(1.f)<<'\t'<<bits(1.f)<<"\t100\t0\t0\t0\t"<<bits(MaNGOS::XP::BaseGain(pl,mob))<<'\t'<<MaNGOS::XP::Gain(&p,&c)<<'\n';
    }
    for(uint32 pl=1;pl<=9;pl++)for(uint32 mob=1;mob<=12;mob++)for(float mult:{0.f,.1f,.5f,1.f,1.17f,2.5f,1000.f})for(float rate:{0.f,.33f,1.f,1.05f,2.5f})for(uint32 damage:{0u,1u,33u,99u,100u,101u}) {
        Unit p;p.level=pl;Creature c;c.level=mob;c.others=damage;c.info.ExperienceMultiplier=mult;sWorld.rates[0]=rate;
        std::cout<<"G\t"<<pl<<'\t'<<mob<<'\t'<<bits(mult)<<'\t'<<bits(rate)<<"\t100\t"<<damage<<"\t0\t0\t"<<bits(MaNGOS::XP::BaseGain(pl,mob))<<'\t'<<MaNGOS::XP::Gain(&p,&c)<<'\n';
    }
    sWorld.rates[0]=1;
    for(uint32 level=1;level<=9;level++)for(bool alive:{false,true})for(uint32 amount:{0u,1u,49u,50u,137u,700u,4000u,0xFFFFFFF0u})for(float rest:{0.f,.5f,1.f,1.5f,9.5f,10.f,10.5f,50.75f,1000.f})for(uint8 state:{REST_STATE_RESTED,REST_STATE_NORMAL})xp_case(level,alive,23,amount,bits(rest),state,9);
    xp_case(1,true,0xFFFFFFF0u,24,bits(18.75f),REST_STATE_RESTED,9);
    xp_case(1,true,0,0xFFFFFFF0u,bits(80.f),REST_STATE_RESTED,9);
    for(uint64 seconds:{0ULL,1ULL,9ULL,10ULL,28800ULL,86400ULL,16777217ULL,2147483647ULL})for(uint32 next:{0u,1u,137u,1000u,16777217u,0xFFFFFFFFu})for(uint32 interval=0;interval<3;interval++)for(float rate:{0.f,.33f,1.f,2.5f}) {
        Player p;p.next=next;sWorld.rates[1]=rate;sWorld.rates[2]=rate;sWorld.rates[3]=rate;
        std::cout<<"R\t"<<seconds<<'\t'<<next<<'\t'<<interval<<'\t'<<bits(rate)<<'\t'<<bits(rate)<<'\t'<<bits(rate)<<'\t'<<bits(p.ComputeRest(time_t(seconds),interval!=0,interval==1))<<'\n';
    }
    for(float value:{-100.f,-.5f,0.f,.5f,1.f,1.5f,9.5f,10.f,10.5f,50.75f,1000.f})for(uint8 state:{REST_STATE_RESTED,REST_STATE_NORMAL})for(uint32 level:{1u,9u})for(uint32 next:{0u,1u,137u,16777217u}) {
        sWorld.cap=9;Player p;p.level=level;p.next=next;p.rest_state=state;trace.clear();p.SetRestBonus(value);
        std::cout<<"S\t"<<bits(value)<<'\t'<<unsigned(state)<<'\t'<<level<<"\t9\t"<<next<<'\t'<<bits(p.m_rest_bonus)<<'\t'<<unsigned(p.rest_state)<<'\t'<<p.experience<<'\t'<<joined()<<'\n';
    }
}
