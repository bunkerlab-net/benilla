// SPDX-License-Identifier: GPL-3.0-or-later
// Authored inputs/host; injected CMaNGOS methods GPL-2.0-or-later, no warranty.
#include <cstdint>
#include <iostream>
using uint32=uint32_t; using int32=int32_t;
// PINNED_CONSTANTS
struct Info {uint32 RegenerateStats, flags; bool HasFlag(uint32 f)const{return (flags&f)!=0;}};
struct CombatManager {bool evade;bool IsEvadeRegen()const{return evade;}};
struct Unit {
 uint32 health, maximum, m_regenTimer, unit_flags;
 uint32 GetHealth()const{return health;} uint32 GetMaxHealth()const{return maximum;}
 void SetHealth(uint32 value){health=value;}
 bool HasFlag(uint32, uint32 flag)const{return (unit_flags&flag)!=0;}
 // PINNED_CONTROL
 int32 ModifyHealth(int32);
};
struct Creature:Unit {
 Info info; CombatManager manager; bool combat; uint32 power_calls=0;
 const Info*GetCreatureInfo()const{return &info;}
 bool IsInCombat()const{return combat;}
 CombatManager&GetCombatManager(){return manager;}
 // PINNED_PREDICATE
 void RegenerateAll(uint32); void RegenerateHealth();
 void RegeneratePower(float seconds){if(seconds!=5.f) __builtin_trap(); ++power_calls;}
};
// PINNED_METHODS
int main(){
 for(uint32 maximum:{1u,2u,3u,10u,100u,1000u,1000000u})
 for(uint32 health:{1u,maximum-maximum/3,maximum})
 for(uint32 elapsed:{0u,4999u,5000u,9999u,4294967295u})
 for(uint32 diff:{0u,1u,5000u,10001u})
 for(uint32 flags:{0u,1u,2u,15u})
 for(uint32 type:{0u,0x40000u})
 for(uint32 mode:{0u,1u,2u,3u}) {
  Creature c; c.health=health;c.maximum=maximum;c.m_regenTimer=elapsed;c.unit_flags=0;
  c.info={flags,type};c.combat=(mode&1)!=0;c.manager.evade=(mode&2)!=0;
  c.RegenerateAll(diff);
  std::cout<<maximum<<'\t'<<health<<'\t'<<elapsed<<'\t'<<diff<<'\t'<<flags<<'\t'<<type<<'\t'<<mode<<'\t'<<c.health<<'\t'<<c.m_regenTimer<<'\t'<<c.power_calls<<'\n';
 }
}
