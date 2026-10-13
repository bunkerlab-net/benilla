// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic adapters. Hash-checked unchanged CMaNGOS bodies inserted by generator.
#include <algorithm>
#include <array>
#include <cassert>
#include <cfenv>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <map>
#include <string>
#include <vector>
using uint8=uint8_t;using uint16=uint16_t;using uint32=uint32_t;using int32=int32_t;
using UnitMoveType=int;
enum{MOVE_WALK,MOVE_RUN,MOVE_RUN_BACK,MOVE_SWIM,MOVE_SWIM_BACK,MOVE_TURN_RATE,MAX_MOVE_TYPE};
// PINNED_BASE_SPEEDS
using Stats=int;using Powers=int;using SpellSchools=int;using UnitMods=int;using WeaponAttackType=int;
constexpr uint32 USE_DEFAULT_DATABASE_LEVEL=0,MAX_CREATURE_MODEL=4;
enum{STAT_STRENGTH,STAT_AGILITY,STAT_STAMINA,STAT_INTELLECT,STAT_SPIRIT,MAX_STATS};
enum{CLASS_WARRIOR=1,CLASS_PALADIN=2,CLASS_ROGUE=4,CLASS_MAGE=8};
enum{POWER_MANA,POWER_RAGE,POWER_FOCUS,POWER_ENERGY,POWER_HAPPINESS,MAX_POWERS};
constexpr uint32 POWER_FOCUS_DEFAULT=100,POWER_HAPPINESS_DEFAULT=1000000;
enum{SPELL_SCHOOL_NORMAL,SPELL_SCHOOL_HOLY,SPELL_SCHOOL_FIRE,SPELL_SCHOOL_NATURE,SPELL_SCHOOL_FROST,SPELL_SCHOOL_SHADOW,SPELL_SCHOOL_ARCANE,MAX_SPELL_SCHOOL};
enum{BASE_ATTACK,OFF_ATTACK,RANGED_ATTACK};
enum{BASE_VALUE,BASE_PCT,TOTAL_VALUE,TOTAL_PCT};
enum{UNIT_MOD_STAT_START=0,UNIT_MOD_HEALTH=5,UNIT_MOD_POWER_START=6,UNIT_MOD_MANA=6,UNIT_MOD_ARMOR=12,UNIT_MOD_RESISTANCE_START=12,UNIT_MOD_ATTACK_POWER=19,UNIT_MOD_ATTACK_POWER_RANGED=20,UNIT_MOD_DAMAGE_MAINHAND=21,UNIT_MOD_DAMAGE_OFFHAND=22,UNIT_MOD_DAMAGE_RANGED=23};
enum{MINDAMAGE,MAXDAMAGE};
enum{UNIT_FIELD_ATTACK_POWER,UNIT_FIELD_ATTACK_POWER_MODS,UNIT_FIELD_ATTACK_POWER_MULTIPLIER,UNIT_FIELD_RANGED_ATTACK_POWER,UNIT_FIELD_RANGED_ATTACK_POWER_MODS,UNIT_FIELD_RANGED_ATTACK_POWER_MULTIPLIER,UNIT_FIELD_MINDAMAGE,UNIT_FIELD_MAXDAMAGE,UNIT_FIELD_MINOFFHANDDAMAGE,UNIT_FIELD_MAXOFFHANDDAMAGE,UNIT_FIELD_MINRANGEDDAMAGE,UNIT_FIELD_MAXRANGEDDAMAGE};
enum class AttackPowerMod{MELEE_ATTACK_POWER,RANGED_ATTACK_POWER};enum class AttackPowerModSign{MOD_SIGN_POS,MOD_SIGN_NEG};
struct Log{template<class...T>void outError(T...){assert(false);}template<class...T>void outErrorDb(T...){assert(false);}}sLog;
struct World{float getConfig(int)const{return 1;}}sWorld;
enum{CONFIG_FLOAT_RATE_POWER_MANA,CONFIG_FLOAT_RATE_POWER_ENERGY,CONFIG_FLOAT_RATE_POWER_FOCUS,SPELL_AURA_MOD_POWER_REGEN,SPELL_AURA_MOD_POWER_REGEN_PERCENT,UNIT_FIELD_FLAGS,UNIT_FLAG_PLAYER_CONTROLLED,CONFIG_FLOAT_GHOST_RUN_SPEED_BG,CONFIG_FLOAT_GHOST_RUN_SPEED_WORLD,SPELL_AURA_MOD_INCREASE_MOUNTED_SPEED,SPELL_AURA_MOD_MOUNTED_SPEED_ALWAYS,SPELL_AURA_MOD_MOUNTED_SPEED_NOT_STACK,SPELL_AURA_MOD_INCREASE_SPEED,SPELL_AURA_MOD_SPEED_ALWAYS,SPELL_AURA_MOD_SPEED_NOT_STACK,SPELL_AURA_MOD_INCREASE_SWIM_SPEED,SPELL_AURA_USE_NORMAL_MOVEMENT_SPEED,SPELL_AURA_MOD_DECREASE_SPEED,TYPEID_PLAYER,CORPSE};
struct Modifier{int m_miscvalue=0,m_amount=0;};struct Aura{Modifier modifier;const Modifier*GetModifier()const{return &modifier;}};using AuraList=std::vector<Aura*>;
constexpr uint32 REGEN_TIME_FULL_UNIT=5000;
struct CombatManager{bool IsEvadeRegen()const{return false;}};
struct CreatureInfo{
 uint32 Entry=99,MinLevel=1,MaxLevel=20,UnitClass=1,Rank=0,MinLevelHealth=61,MaxLevelHealth=603,MinLevelMana=97,MaxLevelMana=331,Armor=17,MeleeAttackPower=60,RangedAttackPower=70,MeleeBaseAttackTime=2300;
 float HealthMultiplier=1,PowerMultiplier=1,ArmorMultiplier=1,DamageMultiplier=1,DamageVariance=.35f,StrengthMultiplier=1,AgilityMultiplier=1,StaminaMultiplier=1,IntellectMultiplier=1,SpiritMultiplier=1,MinMeleeDmg=1.75f,MaxMeleeDmg=5.5f,MinRangedDmg=2,MaxRangedDmg=4,DamageMultiplierOLD=1,DamageVarianceOLD=1,SpeedWalk=0,SpeedRun=0;
 uint32 DisplayId[4]={31,32,33,0},DisplayIdProbability[4]={2,3,1,0};
};
struct CreatureClassLvlStats{uint32 BaseHealth=73,BaseMana=137,BaseArmor=59,Strength=19,Agility=23,Stamina=21,Intellect=22,Spirit=17;float BaseDamage=7.125f,BaseDamageOLD=6.f,BaseMeleeAttackPower=100.f,BaseRangedAttackPower=70.25f;};
struct CreatureModelInfo{uint32 modelid=0,modelid_other_gender=0;float bounding_radius=.5f,combat_reach=1.5f,SpeedWalk=1,SpeedRun=1;uint8 gender=0;};
uint32 urand(uint32,uint32);
struct CreatureSpawnTemplate{uint32 modelId=0;};struct CreatureData{CreatureSpawnTemplate* spawnTemplate=nullptr;uint32 spawntimesecsmin=0,spawntimesecsmax=0;
// PINNED_RESPAWN_DRAW
};
struct WorldObject{uint32 dbGuid=0,lowGuid=0;uint32 GetDbGuid()const{return dbGuid;}uint32 GetGUIDLow()const{return lowGuid;}bool IsUsingNewSpawningSystem()const;};
struct SpawnResult:WorldObject{uint32 m_respawnDelay=0,m_corpseDelay=0;void resolve(const CreatureData*data){
// PINNED_TIMING
}};struct GameEventCreatureData{uint32 modelid=0;};
static std::vector<uint32> rolls;static size_t cursor;
uint32 urand(uint32 lo,uint32 hi){assert(cursor<rolls.size());uint32 r=rolls[cursor++];assert(r>=lo&&r<=hi);return r;}
int32 irand(int32 lo,int32 hi){return int32(urand(uint32(lo),uint32(hi)));}
struct ObjectMgr{
 CreatureClassLvlStats cls;std::map<uint32,CreatureModelInfo>models;
 const CreatureClassLvlStats*GetCreatureClassLvlStats(uint32,uint32)const{return &cls;}
 const CreatureModelInfo*GetCreatureModelInfo(uint32 id)const{auto p=models.find(id);return p==models.end()?nullptr:&p->second;}
 const CreatureModelInfo*GetCreatureModelRandomGender(uint32)const;
 static const CreatureInfo*GetCreatureTemplate(uint32){return nullptr;}
}sObjectMgr;
struct Unit{
 float m_auraModifiersGroup[24][4]{};float create[5]{};int32 attrs[5]{};uint32 createHealth=0,createMana=0,health=0,maxHealth=0,powers[5]{},maxPowers[5]{},ap[2]{};float weapon[3][2]{},damage[3][2]{};int32 armor=0,resists[7]{};
 float m_speed_rate[MAX_MOVE_TYPE]={1,1,1,1,1,1};float m_baseSpeedWalk=1,m_baseSpeedRun=1;float GetSpeed(UnitMoveType)const;void UpdateSpeed(UnitMoveType,bool,float=1.f);void SetSpeedRate(UnitMoveType,float,bool);
 bool IsMounted()const{return false;}int32 GetMaxPositiveAuraModifier(int)const{return 0;}int32 GetMaxNegativeAuraModifier(int)const{return 0;}float GetTotalAuraMultiplier(int)const{return 1.f;}int GetTypeId()const{return -1;}int GetDeathState()const{return -1;}bool IsCreature()const{return true;}
 int m_attackPowerMod[2][2]{};uint32 level=1;int powerType=POWER_RAGE;
 Unit(){for(auto&m:m_auraModifiersGroup){m[BASE_PCT]=1;m[TOTAL_PCT]=1;}m_auraModifiersGroup[UNIT_MOD_DAMAGE_OFFHAND][TOTAL_PCT]=.5f;}
 virtual ~Unit()=default;
 float GetStat(int i)const{return float(attrs[i]);}float GetCreateStat(int i)const{return create[i];}void SetCreateStat(int i,float v){create[i]=v;}void SetStat(int i,int32 v){attrs[i]=v;}
 float GetTotalStatValue(int)const;float GetTotalResistanceValue(int)const;static float GetHealthBonusFromStamina(float);virtual float GetHealthBonusFromStamina()const;static float GetManaBonusFromIntellect(float);float GetManaBonusFromIntellect()const;void UpdateMaxPower(int);
 uint32 GetCreateHealth()const{return createHealth;}void SetCreateHealth(uint32 n){createHealth=n;}void SetCreateMana(uint32 n){createMana=n;}
 uint32 GetCreatePowers(int i)const{return i==POWER_MANA?createMana:0;}uint32 GetMaxHealth()const{return maxHealth;}void SetMaxHealth(uint32 v){maxHealth=v;health=std::min(health,v);}void SetHealth(uint32 v){health=std::min(maxHealth,v);}uint32 GetHealth()const{return health;}
 void SetMaxPower(int i,uint32 v){maxPowers[i]=v;powers[i]=std::min(powers[i],v);}void SetPower(int i,uint32 v){powers[i]=std::min(v,maxPowers[i]);}uint32 GetMaxPower(int i)const{return maxPowers[i];}
 uint32 m_regenTimer=0;uint32 GetPower(int i)const{return powers[i];}void ModifyPower(int i,int32 value){assert(value>=0);SetPower(i,GetPower(i)+uint32(value));}void ModifyHealth(uint32 value){SetHealth(health+value);}
 bool IsPlayerControlled()const{return false;}bool HasFlag(int,int)const{return false;}bool GetMasterGuid()const{return false;}bool IsInCombat()const{return false;}bool IsUnderLastManaUseEffect()const{return false;}float OCTRegenMPPerSpirit()const{return 0;}const AuraList&GetAurasByType(int)const{static AuraList empty;return empty;}CombatManager GetCombatManager()const{return {};}
 int GetPowerType()const{return powerType;}uint32 GetLevel()const{return level;}void SetLevel(uint32 v){level=v;}
 void SetModifierValue(int m,int i,float v){m_auraModifiersGroup[m][i]=v;}float GetModifierValue(int m,int i)const{return m_auraModifiersGroup[m][i];}
 void SetBaseWeaponDamage(int a,int b,float v){weapon[a][b]=v;}float GetBaseWeaponDamage(int a,int b)const{return weapon[a][b];}
 float GetCreateResistance(int)const{return 0;}void SetResistance(int i,int32 v){resists[i]=v;}int32 GetArmor()const{return armor;}void SetArmor(int32 n){armor=n;}
 void SetInt32Value(int i,uint32 v){ap[i==UNIT_FIELD_RANGED_ATTACK_POWER]=v;}void SetInt16Value(int,int,int){}void SetFloatValue(int,float){}
 float GetTotalAttackPowerValue(int a)const{return float(ap[a==RANGED_ATTACK]);}
 void SetStatFloatValue(int i,float v){if(i>=UNIT_FIELD_MINDAMAGE)damage[(i-UNIT_FIELD_MINDAMAGE)/2][(i-UNIT_FIELD_MINDAMAGE)%2]=std::max(0.f,v);}
};
struct Player:Unit{bool InBattleGround()const{return false;}};
struct Creature:Unit{
 CreatureInfo info;float m_baseAP=0,m_baseRAP=0,m_healthMultiplier=1,m_damageMultiplier=1,healthRate=1,damageRate=1;float m_modelRunSpeed=0;CreatureInfo* m_creatureInfo=&info;const CreatureInfo*m_mountInfo=nullptr;
 const CreatureInfo*GetCreatureInfo()const{return &info;}bool IsPet()const{return false;}bool healthRegen=true,powerRegen=true;bool IsRegeneratingPower()const{return powerRegen;}bool IsRegeneratingHealth()const{return healthRegen;}void RegenerateAll(uint32);void RegenerateHealth();void RegeneratePower(float);int getClass()const{return int(info.UnitClass);}std::string GetGuidStr()const{return "synthetic";}float _GetDamageMod(uint32)const{return damageRate;}float _GetHealthMod(uint32)const{return healthRate;}
 void SelectLevel(uint32=0);bool UpdateStats(int);bool UpdateAllStats();void UpdateArmor();void UpdateResistances(uint32);void UpdateAttackPowerAndDamage(bool=false);void UpdateDamagePhysical(int);void UpdateMaxHealth();float GetHealthBonusFromStamina()const override;
 float GetConditionalTotalPhysicalDamageModifier(int)const{return 1;}
 uint32 ChooseDisplayId(const CreatureInfo*,const CreatureData* =nullptr,const GameEventCreatureData* =nullptr);
 void SetBaseWalkSpeed(float);void SetBaseRunSpeed(float,bool=false);
};
// PINNED_METHODS
// PINNED_SPEED_RATE
void load(CreatureClassLvlStats&cCLS){
// PINNED_LOADER
}
uint32 bits(float n){uint32 v;std::memcpy(&v,&n,4);return v;}
void emit(Creature&c,uint32 selected){
 CreatureClassLvlStats raw=sObjectMgr.cls;
 const auto&t=c.info;
 std::cout<<"s "<<t.UnitClass<<' '<<selected<<' '<<bits(t.HealthMultiplier)<<' '<<bits(t.PowerMultiplier)<<' '<<bits(t.ArmorMultiplier)<<' '<<bits(t.DamageMultiplier)<<' '<<bits(t.DamageVariance)<<' '<<bits(c.healthRate)<<' '<<bits(c.damageRate);
 for(float x:{t.StrengthMultiplier,t.AgilityMultiplier,t.StaminaMultiplier,t.IntellectMultiplier,t.SpiritMultiplier})std::cout<<' '<<bits(x);
 std::cout<<' '<<raw.BaseHealth<<' '<<raw.BaseMana<<' '<<raw.BaseArmor<<' '<<bits(raw.BaseMeleeAttackPower)<<' '<<bits(raw.BaseRangedAttackPower)<<' '<<bits(raw.BaseDamage);
 for(uint32 n:{raw.Strength,raw.Agility,raw.Stamina,raw.Intellect,raw.Spirit})std::cout<<' '<<n;
 load(sObjectMgr.cls);rolls={selected};cursor=0;c.powerType=t.UnitClass==1?POWER_RAGE:t.UnitClass==4?POWER_ENERGY:POWER_MANA;c.SelectLevel();assert(cursor==1);
 c.UpdateAllStats(); // UpdateEntry repeats this after SelectLevel, relevant to rogue AP.
 for(float n:c.create)std::cout<<' '<<bits(n);
 for(int32 n:c.attrs)std::cout<<' '<<n;
 std::cout<<' '<<c.createHealth<<' '<<c.createMana<<' '<<bits(c.m_auraModifiersGroup[UNIT_MOD_ARMOR][BASE_VALUE])<<' '<<c.armor<<' '<<bits(c.m_baseAP)<<' '<<bits(sObjectMgr.cls.BaseRangedAttackPower)<<' '<<bits(c.m_damageMultiplier)<<' '<<bits(c.weapon[BASE_ATTACK][MINDAMAGE])<<' '<<bits(c.weapon[BASE_ATTACK][MAXDAMAGE])<<' '<<c.ap[0]<<' '<<bits(c.damage[BASE_ATTACK][MINDAMAGE])<<' '<<bits(c.damage[BASE_ATTACK][MAXDAMAGE])<<' '<<c.maxHealth<<' '<<c.maxPowers[POWER_MANA]<<'\n';
}
int main(){
 std::fesetround(FE_TONEAREST);
 for(uint32 cls:{1u,2u,4u,8u})for(uint32 level:{1u,2u,7u,19u,20u})for(float multiplier:{0.f,.3f,.5f,1.f,1.25f,2.f})for(float rate:{0.f,.65f,1.f,1.5f})for(bool legacy:{false,true}){
  Creature c;sObjectMgr.cls=CreatureClassLvlStats{};c.info.UnitClass=cls;c.healthRate=rate;c.damageRate=1.3f;c.info.HealthMultiplier=multiplier;c.info.PowerMultiplier=multiplier;c.info.ArmorMultiplier=legacy?-1.f:multiplier;c.info.DamageMultiplier=.75f;
  c.info.StrengthMultiplier=1.25f;c.info.AgilityMultiplier=.65f;c.info.StaminaMultiplier=1.025f;c.info.IntellectMultiplier=1.275f;c.info.SpiritMultiplier=-1.f;
  sObjectMgr.cls.BaseHealth+=level*17;sObjectMgr.cls.BaseMana+=level*7;sObjectMgr.cls.BaseArmor+=level*5;
  emit(c,level);
 }
 {
  Creature c;c.info.UnitClass=8;c.info.AgilityMultiplier=1.025f;sObjectMgr.cls=CreatureClassLvlStats{};sObjectMgr.cls.BaseRangedAttackPower=0.f;emit(c,1);
 }
 // Getter/load clamps and low/high intellect; synthetic values, no DB rows.
 for(uint32 intellect:{0u,1u,19u,20u,21u,255u})for(uint32 mana:{0u,1u,100u,10000u})for(uint32 armor:{0u,1u,11u,900u}){
  Creature c;c.info.UnitClass=8;sObjectMgr.cls=CreatureClassLvlStats{};sObjectMgr.cls.Intellect=intellect;sObjectMgr.cls.BaseMana=mana;sObjectMgr.cls.BaseArmor=armor;emit(c,1);
 }
 for(uint32 hp:{1u,2u,3u,101u})for(uint32 mana:{0u,1u,2u,3u,101u,1000000u})for(uint32 elapsed:{0u,4999u,5000u,9000u})for(uint32 diff:{0u,1u,5000u,10000u})for(uint32 mask=0;mask<4;mask++){
  Creature c;c.powerType=POWER_MANA;c.healthRegen=mask&1;c.powerRegen=mask&2;c.maxHealth=hp;c.health=1;c.maxPowers[POWER_MANA]=mana;c.powers[POWER_MANA]=mana/2;c.m_regenTimer=elapsed;c.RegenerateAll(diff);
  std::cout<<"r "<<hp<<' '<<mana<<' '<<elapsed<<' '<<diff<<' '<<mask<<' '<<c.health<<' '<<c.powers[POWER_MANA]<<' '<<c.m_regenTimer<<'\n';
 }
 for(uint32 minimum:{0u,1u,9u,100u})for(uint32 rawMaximum:{0u,5u,11u,100u})for(uint32 entryDelay:{0u,1u,60u,3600u})for(bool dynamic:{false,true})for(bool upper:{false,true}){
  CreatureData data;data.spawntimesecsmin=minimum;data.spawntimesecsmax=std::max(minimum,rawMaximum);uint32 draw=upper?data.spawntimesecsmax:minimum;
  SpawnResult result;result.dbGuid=91;result.lowGuid=dynamic?92:91;result.m_corpseDelay=entryDelay;rolls={draw};cursor=0;result.resolve(&data);assert(cursor==1);
  std::cout<<"t "<<minimum<<' '<<rawMaximum<<' '<<entryDelay<<' '<<result.lowGuid<<' '<<draw<<' '<<result.m_respawnDelay<<' '<<result.m_corpseDelay<<'\n';
 }
 // Unchanged ChooseDisplayId/GetCreatureModelRandomGender consumption.
 sObjectMgr.models={{31,{31,34,.5f,1.5f,1,1,0}},{32,{32,0,.5f,1.5f,1,1,0}},{33,{33,0,.5f,1.5f,1,1,0}},{34,{34,31,.5f,1.5f,1,1,1}}};
 for(uint32 roll=0;roll<6;roll++)for(uint32 gender=0;gender<2;gender++){
  Creature c;rolls={roll,gender};cursor=0;uint32 base=c.ChooseDisplayId(&c.info);auto m=sObjectMgr.GetCreatureModelRandomGender(base);std::cout<<"m "<<roll<<' '<<gender<<' '<<base<<' '<<m->modelid<<' '<<cursor<<'\n';
 }
 for(float tw:{0.f,.001f,.009f,.01f,.011f,.7f,1.3f})for(float tr:{0.f,.001f,.009f,.01f,.011f,.8f,1.4f})for(float mw:{.001f,.01f,.65f,1.f,1.5f})for(float mr:{.001f,.01f,.6f,1.f,2.f}){
  Creature c;c.info.SpeedWalk=tw;c.info.SpeedRun=tr;c.m_modelRunSpeed=mr;c.SetBaseWalkSpeed(mw);c.SetBaseRunSpeed(mr,false);
  std::cout<<"v "<<bits(tw)<<' '<<bits(tr)<<' '<<bits(mw)<<' '<<bits(mr)<<' '<<bits(c.GetSpeed(MOVE_WALK))<<' '<<bits(c.GetSpeed(MOVE_RUN))<<'\n';
 }
}
