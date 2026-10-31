// SPDX-License-Identifier: GPL-3.0-or-later
// Authored host; injected CMaNGOS methods GPL-2.0-or-later, no warranty.
#include <cstdint>
#include <cstdlib>
#include <iostream>
using uint32=uint32_t;using int32=int32_t;
using DamageEffectType=int;struct SpellEntry{};struct Unit;
uint32 callbacks=0;
struct UnitAI {void DamageDeal(Unit*,uint32&,int,const SpellEntry*){callbacks=callbacks*10+1;} void DamageTaken(Unit*,uint32&,int,const SpellEntry*){callbacks=callbacks*10+2;}};
struct Manager {bool evade;bool IsInEvadeMode()const{return evade;}};
struct Unit {
 Manager manager;
 bool IsAlive()const{return true;}bool IsTaxiFlying()const{return false;}
 Manager&GetCombatManager(){return manager;}
 UnitAI ai;UnitAI*AI(){return &ai;}
 static void DealDamageMods(Unit*,Unit*,uint32&,uint32*,DamageEffectType,SpellEntry const*);
};
// PINNED_MODS
int main(){
 for(uint32 damage:{0u,1u,17u,1000000u})
 for(uint32 prior_absorb:{0u,3u,1000000u})
 for(int32 resist:{-1000000,-5,0,5,1000000})
 for(bool evade:{false,true}){
  callbacks=0;Unit target,dealer;target.manager.evade=evade;
  uint32 pdamage=damage,absorb=prior_absorb;
  Unit::DealDamageMods(&dealer,&target,pdamage,&absorb,0,nullptr);
  // PINNED_MITIGATION
  std::cout<<damage<<'\t'<<prior_absorb<<'\t'<<resist<<'\t'<<evade<<'\t'<<pdamage<<'\t'<<absorb<<'\t'<<callbacks<<'\n';
 }
}
