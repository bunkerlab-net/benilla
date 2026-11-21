// SPDX-License-Identifier: GPL-3.0-or-later
// Authored clock adapter; injected CMaNGOS methods GPL-2.0-or-later, no warranty.
#include <chrono>
#include <cstdint>
#include <iostream>
using uint32=uint32_t;
using Clock=std::chrono::steady_clock;
Clock::time_point at(uint64_t ms){return Clock::time_point(std::chrono::milliseconds(ms));}
enum class CreatureStaticFlags3 {FOREVER_CORPSE_DURATION};
struct Settings {bool HasFlag(CreatureStaticFlags3)const{return false;}};
struct Map {Clock::time_point now;Clock::time_point GetCurrentClockTime()const{return now;}};
struct Creature {
 Settings m_settings;Clock::time_point m_corpseExpirationTime;uint32 m_corpseAccelerationDecayDelay;Map map;
 bool IsInWorld()const{return true;}Map*GetMap(){return &map;}
 void InspectingLoot();void ReduceCorpseDecayTimer();
};
// PINNED_INSPECT
// PINNED_REDUCE
int main(){
 for(uint64_t death:{1ull,2ull,1999ull,1000000ull,4294967296ull})
 for(uint32 decay:{1u,3000u,60000u,120000u,4000000000u})
 for(uint32 elapsed:{0u,1u,2999u,60000u})
 for(uint32 delay:{1u,3000u,120000u,4000000000u})
 for(bool inspect:{false,true}){
  Creature c;c.m_corpseExpirationTime=at(death+decay);c.m_corpseAccelerationDecayDelay=delay;c.map.now=at(death+elapsed);
  if(inspect)c.InspectingLoot();else c.ReduceCorpseDecayTimer();
  auto result=std::chrono::duration_cast<std::chrono::milliseconds>(c.m_corpseExpirationTime.time_since_epoch()).count();
  std::cout<<death<<'\t'<<decay<<'\t'<<elapsed<<'\t'<<delay<<'\t'<<inspect<<'\t'<<result<<'\n';
 }
}
