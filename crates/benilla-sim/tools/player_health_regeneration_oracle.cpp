// SPDX-License-Identifier: GPL-3.0-or-later
// Authored adapters around unchanged hash-verified CMaNGOS methods inserted by generator.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <vector>
using uint32=uint32_t; using uint8=uint8_t; using int32=int32_t;
enum { CLASS_DRUID=11,CLASS_HUNTER=3,CLASS_MAGE=8,CLASS_PALADIN=2,CLASS_PRIEST=5,CLASS_ROGUE=4,CLASS_SHAMAN=7,CLASS_WARLOCK=9,CLASS_WARRIOR=1,STAT_SPIRIT=4,
CONFIG_FLOAT_RATE_HEALTH=0,SPELL_AURA_MOD_REGEN_DURING_COMBAT=116,SPELL_AURA_MOD_HEALTH_REGEN_PERCENT=88,SPELL_AURA_MOD_HEALTH_REGEN_IN_COMBAT=161 };
struct Modifier {int32 m_amount=0;};
struct Aura { const Modifier* GetModifier()const { assert(false);return nullptr;} };
using AuraList=std::vector<Aura*>;
struct World {float rate=1;float getConfig(int)const{return rate;}} sWorld;
struct Unit {
    uint32 health=0,maximum=0,spirit=0;
    uint32 GetHealth()const{return health;} uint32 GetMaxHealth()const{return maximum;}
    void SetHealth(uint32 value){health=std::min(value,maximum);}
    float GetStat(int)const{return float(spirit);} uint8 getClass()const{return CLASS_MAGE;}
    int32 ModifyHealth(int32); float OCTRegenHPPerSpirit()const;
};
struct Player:Unit {
    bool combat=false; int32 requested=0;
    bool IsInCombat()const{return combat;} bool HasAuraType(int)const{return false;}
    bool IsStandState()const{return true;}
    const AuraList& GetAurasByType(int)const{static const AuraList empty;return empty;}
    int32 GetTotalAuraModifier(int)const{return 0;}
    int32 ModifyHealth(int32 value){requested=value;return Unit::ModifyHealth(value);}
    void RegenerateHealth(uint32);
};
// PINNED_BODIES
float number(uint32 bits){float f;std::memcpy(&f,&bits,4);return f;}
int main(){
    assert(std::fesetround(FE_TONEAREST)==0);
    std::cout<<"# health max spirit diff ratebits combat after requested gained; standing mage, empty health-aura lists, no scheduling/death coverage\n";
    for(uint32 spirit:{0u,1u,8u,9u,37u,99u,100u,16777215u,16777216u,16777217u,4294967295u})
    for(uint32 diff:{0u,1u,999u,1000u,1999u,2000u,2001u,16777215u,16777216u,16777217u,4294967295u})
    for(uint32 rate:{0u,0x80000000u,0x3a83126fu,0x3effffffu,0x3f000000u,0x3f000001u,0x3f7fffffu,0x3f800000u,0x3f800001u})
    for(uint32 mode:{0u,1u,2u}) {
        Player p;p.spirit=spirit;p.health=mode==2?999u:100u;p.maximum=mode==2?1000u:2000000000u;p.combat=mode==1;sWorld.rate=number(rate);
        // Do not execute source undefined float->int or signed-add cases. Rust
        // separately verifies explicit errors instead of manufacturing source output.
        if(!p.combat){
            float add=p.OCTRegenHPPerSpirit()*sWorld.rate;add+=0;
            float request=add*float(diff)/1000;
            if(!std::isfinite(request)||double(request)>=2147483648.0||double(request)+p.health>2147483647.0)continue;
        }
        uint32 before=p.health;p.RegenerateHealth(diff);
        std::cout<<before<<'\t'<<p.maximum<<'\t'<<spirit<<'\t'<<diff<<'\t'<<rate<<'\t'<<p.combat<<'\t'<<p.health<<'\t'<<p.requested<<'\t'<<p.health-before<<'\n';
    }
    // Full health returns before reading rate/spirit or converting any health to int32.
    Player full;full.health=full.maximum=4294967295u;full.spirit=4294967295u;sWorld.rate=1000000;
    full.RegenerateHealth(4294967295u);
    std::cout<<full.health<<'\t'<<full.maximum<<'\t'<<full.spirit<<'\t'<<4294967295u<<'\t'<<0x49742400u<<"\t0\t"<<full.health<<"\t0\t0\n";
}
