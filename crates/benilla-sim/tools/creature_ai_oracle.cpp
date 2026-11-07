// SPDX-License-Identifier: GPL-3.0-or-later
// Original bounded adapters; exact upstream bodies inserted by hash-checking generator.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <list>
using uint32 = uint32_t; using int32 = int32_t; using uint8 = uint8_t;
#define MANGOS_ASSERT assert
constexpr float BASE_MELEERANGE_OFFSET = 1.33f, ATTACK_DISTANCE = 5.f, MELEE_LEEWAY = 2.66f, M_PI_F = 3.14159265358979323846f;
constexpr int CONFIG_FLOAT_RATE_CREATURE_AGGRO = 1, SPELL_AURA_MOD_DETECT_RANGE = 1, SPELL_AURA_MOD_DETECTED_RANGE = 2;
constexpr int TYPEID_PLAYER = 1, TYPEID_UNIT = 2, BASE_ATTACK = 0, OFF_ATTACK = 1;
constexpr uint32 ATTACK_DISPLAY_DELAY = 200;
struct World { float rate = 1; float getConfig(int) const { return rate; } } sWorld;
enum DistanceCalculation { DIST_CALC_NONE, DIST_CALC_BOUNDING_RADIUS, DIST_CALC_COMBAT_REACH, DIST_CALC_COMBAT_REACH_WITH_MELEE };
struct WorldObject {
 float x=0,y=0,z=0,reach=1.5f,radius=0.5f;
 float GetPositionX() const {return x;} float GetPositionY() const {return y;} float GetPositionZ() const {return z;}
 float GetCombatReach() const {return reach;} float GetObjectBoundingRadius() const {return radius;}
 float GetCombinedCombatReach(WorldObject const*, bool=false, float=0) const;
 float GetCombinedCombatReach(bool=false, float=0) const;
 float GetDistance2d(float, float, DistanceCalculation=DIST_CALC_BOUNDING_RADIUS) const;
};
struct Player;
struct Unit : WorldObject {
 uint32 level=1, timer=0, interval=2000, swings=0; float detection=18;
 bool moving=false,walking=false,casting=false,initiate=true,inrange=true,inarc=true,alive=true,attack=true;
 uint32 m_extraAttacks=0; Unit* victim=nullptr;
 uint32 GetLevelForTarget(Unit const*) const{return level;}
 float GetDetectionRange() const{return detection;}
 float GetTotalAuraModifier(int) const{return 0;}
 bool IsPlayerControlled() const{return false;} bool IsCreature() const{return true;}
 bool IsClientControlled() const{return false;} Player const* GetClientControlling() const{return nullptr;}
 bool IsMoving() const{return moving;} bool IsWalking() const{return walking;}
 int GetTypeId() const{return TYPEID_UNIT;} bool CanInitiateAttack() const{return initiate;}
 Unit* GetVictim() const{return victim;} bool IsNonMeleeSpellCasted(bool) const{return casting;}
 void DoExtraAttacks(Unit*) {} bool isAttackReady(int type) const{return type==BASE_ATTACK&&timer==0;}
 bool hasOffhandWeaponForAttack() const{return false;} uint32 getAttackTimer(int) const{return timer;}
 void setAttackTimer(int type,uint32 value){if(type==BASE_ATTACK)timer=value;}
 void resetAttackTimer(int){timer=interval;} void AttackerStateUpdate(Unit*,int){swings++;}
 bool HasInArc(Unit*,float) const{return inarc;} bool IsAlive() const{return alive;}
 bool CanAttackInCombat(Unit*,bool,bool) const{return attack;}
 float GetAttackDistance(Unit const*) const;
 bool CanReachWithMeleeAttack(Unit const*,float=0) const;
 bool UpdateMeleeAttackingState();
 bool IsIgnoringRangedTargets() const {return false;} bool IsPlayer() const{return true;}
};
using Creature = Unit;
struct Player : Unit { uint8 LastSwingErrorMsg()const{return 0;} void SendAttackSwingNotInRange(){} void SendAttackSwingDeadTarget(){} void SendAttackSwingBadFacingAttack(){} void SendAttackSwingCantAttack(){} void SwingErrorMsg(uint8){} };
struct HostileReference { Unit* target; float threat; Unit* getTarget()const{return target;} int GetTauntState()const{return 0;} int GetHostileState()const{return 0;} float getThreat()const{return threat;} };
using ThreatList=std::list<HostileReference*>;
struct ThreatContainer {ThreatList iThreatList; HostileReference* selectNextVictim(Unit*,HostileReference*);};
// PINNED_ENUM
// PINNED_METHODS
uint32 bits(float x){uint32 b;std::memcpy(&b,&x,4);return b;}
float number(uint32 b){float x;std::memcpy(&x,&b,4);return x;}
int main(){
 std::fesetround(FE_TONEAREST);
 // Every output row is synthetic arithmetic/branch data, never a game-data row.
 Unit a,b;
 for(uint32 ca=1;ca<=63;ca++)for(uint32 pa=1;pa<=63;pa++)for(float d:{0.f,18.f,20.25f})for(float r:{0.f,0.65f,1.f,1.5f}){
  a.level=ca;b.level=pa;a.detection=d;sWorld.rate=r;
  std::cout<<"a "<<ca<<' '<<pa<<' '<<bits(d)<<' '<<bits(r)<<' '<<bits(a.GetAttackDistance(&b))<<'\n';
 }
 for(float ar:{0.f,1.5f,3.2f,10.f})for(float br:{0.f,1.5f,4.5f}){
  a.reach=ar;b.reach=br;
  std::cout<<"r "<<bits(ar)<<' '<<bits(br)<<' '<<bits(a.GetCombinedCombatReach(&b,true,0.f))<<'\n';
 }
 // Cancellation, sqrt round-to-nearest and bounding-radius subtraction; global sqrt(float) is binary64.
 uint32 seed=0x7cd81922;
 for(uint32 i=0;i<4096;i++){
  seed=seed*1664525u+1013904223u;a.x=float(int32(seed%1000000)-500000)/32.f;
  seed=seed*1664525u+1013904223u;a.y=float(int32(seed%1000000)-500000)/32.f;
  seed=seed*1664525u+1013904223u;float x=float(int32(seed%1000000)-500000)/32.f;
  seed=seed*1664525u+1013904223u;float y=float(int32(seed%1000000)-500000)/32.f;
  a.radius=float(i%100)/16.f;
  std::cout<<"d "<<bits(a.x)<<' '<<bits(a.y)<<' '<<bits(x)<<' '<<bits(y)<<' '<<bits(a.radius)<<' '<<bits(a.GetDistance2d(x,y))<<'\n';
 }
 a=Unit{};b=Unit{};
 for(uint32 mask=0;mask<128;mask++)for(uint32 timer:{0u,1u,100u,2000u}){
  a.victim=mask&1?&b:nullptr;a.casting=mask&2;a.initiate=mask&4;
  a.x=0;b.x=mask&8?1.f:10.f;a.inarc=mask&16;b.alive=mask&32;a.attack=mask&64;
  a.timer=timer;a.swings=0;bool error=a.UpdateMeleeAttackingState();
  std::cout<<"s "<<mask<<' '<<timer<<' '<<a.timer<<' '<<a.swings<<' '<<error<<'\n';
 }
 a=Unit{};b=Unit{};Unit challenger;
 for(float current:{0.f,1.f,10.f,100.f,9999999.f})for(float multiplier:{1.f,1.1f,1.10001f,1.3f,1.30001f})for(bool melee:{false,true}){
  float amount=current*multiplier; b.x=1;challenger.x=melee?1.f:10.f;
  HostileReference c{&b,current},v{&challenger,amount};ThreatContainer container;
  container.iThreatList=amount>current?ThreatList{&v,&c}:ThreatList{&c,&v};
  bool switched=container.selectNextVictim(&a,&c)==&v;
  std::cout<<"t "<<bits(current)<<' '<<bits(amount)<<' '<<melee<<' '<<switched<<'\n';
 }
 a=Unit{};b=Unit{};
 for(uint32 distance:{0u,0x409fffffu,0x40a00000u,0x40a00001u,0x40f51eb8u,0x40f51eb9u})for(uint32 movement=0;movement<16;movement++){
  b.x=number(distance);b.z=100;
  a.moving=movement&1;a.walking=movement&2;b.moving=movement&4;b.walking=movement&8;
  std::cout<<"m "<<distance<<' '<<movement<<' '<<a.CanReachWithMeleeAttack(&b)<<'\n';
 }
}
