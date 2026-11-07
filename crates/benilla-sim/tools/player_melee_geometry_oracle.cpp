// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic operand adapters; unchanged source methods/constants inserted.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <vector>
using uint32=uint32_t;
#define MANGOS_ASSERT assert
constexpr int BASE_ATTACK=0,STAT_STRENGTH=0;
// PINNED_CONSTANTS
struct WorldObject {
 float x=0,y=0,z=0,reach=0;
 float GetPositionX()const{return x;}float GetPositionY()const{return y;}float GetPositionZ()const{return z;}
 float GetCombatReach()const{return reach;}
 float GetCombinedCombatReach(WorldObject const*,bool,float)const;
 float GetCombinedCombatReach(bool,float)const;
};
struct Unit:WorldObject {
 bool moving=false,walking=false;uint32 remaining=0,interval=1;
 bool IsMoving()const{return moving;}bool IsWalking()const{return walking;}
 bool IsPlayerControlled()const{return true;}
 uint32 GetAttackTime(int)const{return interval;}
 void setAttackTimer(int,uint32 value){remaining=value;}
 bool CanReachWithMeleeAttack(Unit const*,float=0)const;
 virtual uint32 GetShieldBlockValue()const{return 0;}
};
struct Creature:Unit {
 uint32 level=1;float strength=0;
 uint32 GetLevel()const{return level;}float GetStat(int)const{return strength;}
 // PINNED_SHIELD
};
// PINNED_METHODS
void parry(Unit* victim){
 float basetime=float(victim->remaining);
 // PINNED_PARRY
}
uint32 bits(float value){uint32 out;std::memcpy(&out,&value,4);return out;}
void reach(Unit&a,Unit&b){
 std::cout<<"r "<<bits(a.x)<<' '<<bits(a.y)<<' '<<bits(a.z)<<' '<<bits(b.x)<<' '<<bits(b.y)<<' '<<bits(b.z)<<' '<<bits(a.reach)<<' '<<bits(b.reach)<<' '<<a.moving<<' '<<a.walking<<' '<<b.moving<<' '<<b.walking<<' '<<a.CanReachWithMeleeAttack(&b)<<'\n';
}
int main(){
 assert(std::fesetround(FE_TONEAREST)==0);
 for(uint32 mask=0;mask<16;mask++)for(float ar:{0.f,1.5f,4.f}){
 Unit a,b;a.reach=ar;b.reach=1.5f;a.moving=mask&1;a.walking=mask&2;b.moving=mask&4;b.walking=mask&8;
 const float threshold=a.GetCombinedCombatReach(&b,true,0)+(a.moving&&!a.walking&&b.moving&&!b.walking?MELEE_LEEWAY:0.f);
 for(float value:{std::nextafter(threshold,0.f),threshold,std::nextafter(threshold,1000.f),7.663f}){
 b.x=value;reach(a,b);b.x=0;b.z=value;reach(a,b);b.z=0;
 }
 b.x=3;b.y=4;reach(a,b);
 }
 uint32 seed=0x71543aac;
 for(uint32 row=0;row<128;row++){
 Unit a,b;auto draw=[&](){seed=seed*1664525u+1013904223u;return float(int(seed%2000)-1000)/16.f;};
 a.x=draw();a.y=draw();a.z=draw();b.x=draw();b.y=draw();b.z=draw();a.reach=float(row%10);b.reach=float(row%7);a.moving=b.moving=true;reach(a,b);
 }
 for(uint32 interval:{1u,3u,7u,999u,1000u,1999u,2000u,2001u,999999u,1000000u}){
 const uint32 twenty=uint32(float(interval)*.2f),sixty=uint32(3.f*(float(interval)*.2f));
 std::vector<uint32> timers{0,1,twenty,twenty+1,sixty,sixty+1,interval,1000000};if(twenty)timers.push_back(twenty-1);if(sixty)timers.push_back(sixty-1);
 std::sort(timers.begin(),timers.end());timers.erase(std::unique(timers.begin(),timers.end()),timers.end());
 for(uint32 timer:timers){Unit victim;victim.remaining=timer;victim.interval=interval;parry(&victim);std::cout<<"p "<<timer<<' '<<interval<<' '<<victim.remaining<<'\n';}
 }
 for(uint32 level=1;level<=63;level++)for(float strength:{0.f,1.f,19.999f,20.f,20.001f,123.75f,1000000.f}){
 Creature victim;victim.level=level;victim.strength=strength;std::cout<<"b "<<level<<' '<<bits(strength)<<' '<<victim.GetShieldBlockValue()<<'\n';
 }
}
