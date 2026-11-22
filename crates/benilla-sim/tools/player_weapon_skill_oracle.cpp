// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic adapters only. The generator inserts unchanged pinned source bodies.
#include <algorithm>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <limits>
#include <map>
#include <string>
#include <vector>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
using uint16 = uint16_t;
using uint32 = uint32_t;
using int32 = int32_t;
using WeaponAttackType = int;
constexpr uint16 SKILL_DEFENSE = 95;
constexpr int STAT_INTELLECT = 0, CONFIG_UINT32_SKILL_GAIN_DEFENSE = 0, CONFIG_UINT32_SKILL_GAIN_WEAPON = 1, BASE_ATTACK = 0, ITEM_SUBCLASS_WEAPON_FISHING_POLE = 20, SKILL_UNARMED = 162;
constexpr uint32 SKILL_NEW = 1, SKILL_CHANGED = 2, SKILL_DELETED = 3;
struct SkillStatusData { uint32 pos = 0, uState = 0; };
using SkillStatusMap = std::map<uint16, SkillStatusData>;
uint32 PLAYER_SKILL_VALUE_INDEX(uint32) { return 0; }
uint16 SKILL_VALUE(uint32 value) { return uint16(value); }
uint16 SKILL_MAX(uint32 value) { return uint16(value >> 16); }
uint32 MAKE_SKILL_VALUE(uint32 value, uint32 maximum) { return value | (maximum << 16); }
struct World {
    uint16 gain = 1;
    uint32 getConfig(int) const { return gain; }
} sWorld;
static double roll = 0;
static uint32 draws = 0, captured_chance = 0;
template<class Bits, class Value> Bits bits(Value value) {
    static_assert(sizeof(Bits) == sizeof(Value));
    Bits result;
    std::memcpy(&result, &value, sizeof(result));
    return result;
}
double rand_chance() { ++draws; return roll; }
namespace source_rng {
// PINNED_ROLL
}
bool roll_chance_f(float chance) {
    captured_chance = bits<uint32>(chance);
    return source_rng::roll_chance_f(chance);
}
struct Unit { bool IsPlayerControlled()const{return false;}bool IsNoWeaponSkillGain()const{return false;} };
struct ItemPrototype { int SubClass=10; };
struct Item { ItemPrototype proto;const ItemPrototype* GetProto()const{return &proto;}uint16 GetSkill()const{return 43;} };
struct Player {
    uint32 harness_level = 1, intellect = 0, crit_updates = 0;
    Unit victim;Item weapon;
    uint16 skill_value = 1, maximum = 5;
    SkillStatusMap mSkillStatus{{43, {}}};
    uint16 GetSkillValuePure(uint16) const { return skill_value; }
    uint16 GetSkillMaxPure(uint16) const { return maximum; }
    uint16 GetWeaponSkillIdForAttack(int) const { return 43; }
    uint32 GetLevel() const { return harness_level; }
    float GetStat(int) const { return float(intellect); }
    void UpdateWeaponSkill(int);
    Unit* GetVictim(){return &victim;}
    bool IsNoWeaponShapeShift()const{return false;}
    Item* GetWeaponForAttack(int,bool,bool){return &weapon;}
    void UpdateAllCritPercentages(){++crit_updates;}
    void UpdateDefenseBonusesMod() {}
    uint32 GetUInt32Value(uint32) const { return MAKE_SKILL_VALUE(skill_value, maximum); }
    void SetUInt32Value(uint32, uint32 packed) { skill_value = SKILL_VALUE(packed); }
    void UpdateCombatSkills(uint32, WeaponAttackType, bool);
    void UpdateDefense(uint32){assert(false);}
    void UpdateSkill(uint16, uint16);
};
// PINNED_METHODS
// CANONICAL_LOG_BITS
float canonical_chance(uint32 level,uint32 intellect,uint16 maximum,uint16 value){
 const int32 room=int32(maximum-value);double base;
 if(room>=7){const double gap=0.22*(1.0+(12.0/level));const double log=bits<double>(canonical_log_bits[room-7]);base=(7.0/maximum)+gap*log;}
 else{base=(room*room)/(7.0*maximum);}
 base=std::clamp(base,0.0,1.0);
 const double bonus=std::clamp(0.10*(double(intellect)/750)*(60.0/double(level)),0.0,0.10);
 return std::clamp(base+bonus,0.0,1.0)*100.0f;
}
void emit(uint32 level,uint32 intellect,uint16 maximum,uint16 value,uint16 gain){
 Player probe;probe.harness_level=level;probe.intellect=intellect;probe.maximum=maximum;probe.skill_value=value;
 sWorld.gain=gain;roll=0;draws=0;captured_chance=0;probe.UpdateCombatSkills(0,BASE_ATTACK,false);
 const uint32 expected_draws=draws,expected_chance=captured_chance;
 if(draws){assert(captured_chance==bits<uint32>(canonical_chance(level,intellect,maximum,value)));}
 const double exact=bits<float>(captured_chance);
 std::vector<double> edges{0,std::nextafter(100.0,0.0)};
 if(draws)for(double n:{std::nextafter(exact,0.0),exact,std::nextafter(exact,std::numeric_limits<double>::infinity())})if(n>=0&&n<100)edges.push_back(n);
 std::sort(edges.begin(),edges.end());edges.erase(std::unique(edges.begin(),edges.end()),edges.end());
 for(double edge:edges){
 Player player;player.harness_level=level;player.intellect=intellect;player.maximum=maximum;player.skill_value=value;
 roll=edge;draws=0;captured_chance=0;player.UpdateCombatSkills(0,BASE_ATTACK,false);
 assert(draws==expected_draws&&captured_chance==expected_chance);
 std::cout<<level<<' '<<intellect<<' '<<maximum<<' '<<value<<' '<<gain<<' '<<bits<uint64_t>(edge)<<' '<<captured_chance<<' '<<draws<<' '<<player.skill_value<<' '<<player.crit_updates<<'\n';
 }
}
int main(){
 assert(std::fesetround(FE_TONEAREST)==0);
 for(uint32 level=1;level<=60;level++)for(uint32 intellect:{0u,1u,17u,749u,750u,1000000u})for(uint16 room:{1,6,7,8,50}){
 const uint16 maximum=300;emit(level,intellect,maximum,maximum-room,1);
 }
 for(uint32 level:{1u,60u})for(uint32 intellect:{0u,750u})for(uint16 maximum:{0,5,300,65535})for(uint16 gain:{0,1,65535}){
 emit(level,intellect,maximum,maximum,gain);
 if(maximum){emit(level,intellect,maximum,maximum-1,gain);if(maximum<=300)emit(level,intellect,maximum,0,gain);}
 }
}
