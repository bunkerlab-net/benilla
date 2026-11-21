// SPDX-License-Identifier: GPL-3.0-or-later
// Original authored adapters; the generator inserts complete unchanged functions
// and DBC row declarations from CMaNGOS, GPL-2.0-or-later; see upstream AUTHORS.
// PINNED_LICENSE
// No client DBC bytes or loaded-store evidence. The trigger predicate, spell rank,
// spell modifiers, spell flags, trade state, and speed getters are authored adapters,
// NOT their CMaNGOS implementations. A true trigger predicate proves only this
// function's early exit; it does not prove old-triggered semantics.
// Excluded: abs(INT32_MIN); rank above INT32_MAX; final unsigned cast expression
// above INT32_MAX; signed ranged/modifier overflow; non-finite/out-of-int32 float
// conversions; null spellInfo for cast-time; nonnull spell with null caster.
// Unsigned intermediate wrap from source uint32 baseLevel is defined and retained.
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <limits>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>
#include <type_traits>
using uint32=uint32_t; using int32=int32_t;
static_assert(sizeof(int)==4 && sizeof(uint32)==4 && sizeof(int32)==4);
static_assert(std::numeric_limits<float>::is_iec559 && sizeof(float)==4);
std::ostringstream events;
// PINNED_CAST_ROW
// PINNED_DURATION_ROW
static_assert(std::is_same_v<decltype(SpellCastTimesEntry::CastTime),int32>);
static_assert(std::is_same_v<decltype(SpellDurationEntry::Duration),int32[3]>);
template<class Row> struct Store {
 std::map<uint32,Row> rows; const char* label;
 const Row* LookupEntry(uint32 index) const {
  auto found=rows.find(index);events<<label<<"-lookup:"<<index<<':';
  if(found==rows.end()){events<<"Missing;";return nullptr;}
  events<<"Found("<<found->second.ID<<");";return &found->second;
 }
};
Store<SpellCastTimesEntry> sSpellCastTimesStore{{},"cast"};
Store<SpellDurationEntry> sSpellDurationStore{{},"duration"};
enum SpellAttributes:uint32 {SPELL_ATTR_USES_RANGED_SLOT=0x2,SPELL_ATTR_IS_ABILITY=0x10,SPELL_ATTR_IS_TRADESKILL=0x20};
enum SpellAttributesEx3:uint32 {SPELL_ATTR_EX3_IGNORE_CASTER_MODIFIERS=0x20000000};
constexpr uint32 SPELLMOD_CASTING_TIME=10,TRADE_SLOT_NONTRADED=6,RANGED_ATTACK=2,UNIT_MOD_CAST_SPEED=0x8b+0x06;
struct SpellEntry {
 uint32 Id=700001,DurationIndex=0,CastingTimeIndex=0,baseLevel=0,Attributes=0,AttributesEx3=0;
 bool HasAttribute(SpellAttributes value)const{return Attributes&value;}
 bool HasAttribute(SpellAttributesEx3 value)const{return AttributesEx3&value;}
};
struct Item{};
struct TradeData {
 TradeData* partner=nullptr;Item* item=nullptr;
 TradeData* GetTraderData(){events<<"trade-partner-adapter;";if(!partner)throw std::runtime_error("missing authored trade partner");return partner;}
 Item* GetItem(uint32 slot){if(slot!=TRADE_SLOT_NONTRADED)throw std::runtime_error("trade slot drift");events<<"trade-item-adapter:"<<slot<<';';return item;}
};
struct Player;
struct WorldObject {
 Player* mod_owner=nullptr;
 virtual ~WorldObject()=default;
 virtual bool IsPlayer()const{return false;}
 virtual bool IsUnit()const{return false;}
 Player* GetSpellModOwner(){events<<"mod-owner-adapter:"<<(mod_owner?"present":"none")<<';';return mod_owner;}
};
struct Unit:WorldObject {
 uint32 rank=0;float cast_speed=1.;std::array<float,3> m_modAttackSpeedPct{1.,1.,1.};
 bool IsUnit()const override{return true;}
 uint32 GetSpellRank(const SpellEntry*)const{events<<"rank-adapter:"<<rank<<';';return rank;}
 float GetFloatValue(uint32 field)const{if(field!=UNIT_MOD_CAST_SPEED)throw std::runtime_error("cast field drift");events<<"cast-speed-adapter:"<<cast_speed<<';';return cast_speed;}
};
struct Player:Unit {
 TradeData* trade=nullptr;int32 modifier_delta=0;
 bool IsPlayer()const override{return true;}
 TradeData* GetTradeData(){events<<"trade-adapter:"<<(trade?"present":"none")<<';';return trade;}
 void ApplySpellMod(uint32 id,uint32 op,int32& value,bool consume){
  if(op!=SPELLMOD_CASTING_TIME)throw std::runtime_error("modifier operation drift");
  auto after=int64_t(value)+modifier_delta;
  if(after<std::numeric_limits<int32>::min()||after>std::numeric_limits<int32>::max())throw std::runtime_error("excluded modifier overflow");
  events<<"spellmod-adapter:"<<id<<':'<<op<<':'<<consume<<':'<<value<<"->"<<after<<';';value=int32(after);
 }
};
struct Targets {Item* item=nullptr;Item* getItemTarget(){events<<"target-item-adapter;";return item;}};
struct Spell {
 bool redundant_cast_time=false,auto_repeat=false,ranged=false;Targets m_targets;
 bool IsTriggeredSpellWithRedundantCastTime()const{events<<"redundant-trigger-adapter:"<<redundant_cast_time<<';';return redundant_cast_time;}
 bool IsAutoRepeat()const{events<<"auto-repeat-adapter:"<<auto_repeat<<';';return auto_repeat;}
 bool IsRangedSpell()const{events<<"ranged-spell-adapter:"<<ranged<<';';return ranged;}
};
// PINNED_DURATION
// PINNED_MAX_DURATION
// PINNED_CAST_TIME
struct Input {
 const char* name;const char* category;bool present;uint32 query,row_id;int32 base,per,bound;uint32 rank=0,base_level=0;
};
void reset(){events.str("");events.clear();sSpellCastTimesStore.rows.clear();sSpellDurationStore.rows.clear();}
void output(const Input& in,char kind,int64_t result,int64_t maximum=0){
 std::cout<<in.name<<'\t'<<kind<<'\t'<<in.category<<'\t'<<in.present<<'\t'<<in.query<<'\t'<<in.row_id<<'\t'
          <<in.base<<'\t'<<in.per<<'\t'<<in.bound<<'\t'<<in.rank<<'\t'<<in.base_level<<'\t'<<result<<'\t'<<maximum<<'\t'<<events.str()<<'\n';
}
void duration(const Input& in,bool null_info=false){
 reset();
 if(in.base==std::numeric_limits<int32>::min()||in.bound==std::numeric_limits<int32>::min())throw std::runtime_error("excluded abs(INT32_MIN)");
 if(in.present)sSpellDurationStore.rows.emplace(in.row_id,SpellDurationEntry{in.row_id,{in.base,in.per,in.bound}});
 SpellEntry info;info.DurationIndex=in.query;
 int32 base=GetSpellDuration(null_info?nullptr:&info),maximum=GetSpellMaxDuration(null_info?nullptr:&info);
 output(in,'D',base,maximum);
}
struct Context {
 // Authored endpoints, not production Unit/Player/Spell state.
 enum Caster {UNIT,PLAYER,WORLD,NONE};Caster caster=UNIT;
 bool spell=true,redundant=false,auto_repeat=false,ranged=false,consume=true;
 uint32 attrs=0,attrs3=0,id=700001;
 bool trade=false,trade_item=false,target_matches=false,mod_owner=false;
 int32 delta=0;float cast_speed=1.,ranged_speed=1.;
};
void cast(const Input& in,const Context& context={}){
 reset();
 if(in.rank>uint32(std::numeric_limits<int32>::max()))throw std::runtime_error("excluded rank conversion");
 if(context.spell&&context.caster==Context::NONE)throw std::runtime_error("excluded null caster with spell");
 if(in.present)sSpellCastTimesStore.rows.emplace(in.row_id,SpellCastTimesEntry{in.row_id,in.base,in.per,in.bound});
 SpellEntry info;info.CastingTimeIndex=in.query;info.baseLevel=in.base_level;info.Attributes=context.attrs;info.AttributesEx3=context.attrs3;info.Id=context.id;
 Unit unit;Player player,mod_owner;WorldObject world;Spell spell;TradeData trade,partner;Item item,other;
 unit.rank=player.rank=in.rank;unit.cast_speed=player.cast_speed=context.cast_speed;
 unit.m_modAttackSpeedPct[RANGED_ATTACK]=player.m_modAttackSpeedPct[RANGED_ATTACK]=context.ranged_speed;
 mod_owner.modifier_delta=context.delta;
 for(WorldObject* object:{static_cast<WorldObject*>(&unit),static_cast<WorldObject*>(&player),&world})object->mod_owner=context.mod_owner?&mod_owner:nullptr;
 trade.partner=&partner;partner.item=context.trade_item?&item:nullptr;player.trade=context.trade?&trade:nullptr;
 spell.redundant_cast_time=context.redundant;spell.auto_repeat=context.auto_repeat;spell.ranged=context.ranged;
 spell.m_targets.item=context.target_matches?&item:&other;
 WorldObject* caster=context.caster==Context::UNIT?&unit:context.caster==Context::PLAYER?&player:context.caster==Context::WORLD?&world:nullptr;
 // Check only domains used by authored cases, outside the unchanged source body.
 // The source's unsigned mixed expression can wrap; only its final conversion is bounded.
 uint32 rank=(caster&&caster->IsUnit())?in.rank:0;
 uint32 raw=uint32(in.base)+uint32(in.per)*(uint32(int32(rank)/5)-in.base_level);
 if(in.present&&in.query==in.row_id){
  if(raw>uint32(std::numeric_limits<int32>::max()))throw std::runtime_error("excluded unsigned-to-int32 conversion");
  int64_t before_speed=std::max(int32(raw),in.bound);
  if(context.attrs&SPELL_ATTR_USES_RANGED_SLOT)if(!context.spell||!context.auto_repeat)before_speed+=500;
  if(before_speed>std::numeric_limits<int32>::max())throw std::runtime_error("excluded ranged overflow");
  if(context.spell&&context.mod_owner)before_speed+=context.delta;
  if(before_speed<std::numeric_limits<int32>::min()||before_speed>std::numeric_limits<int32>::max())throw std::runtime_error("excluded modifier overflow");
  float factor=1.;bool applies_speed=false;
  if(context.spell&&caster&&caster->IsUnit()){
   if(!(context.attrs&(SPELL_ATTR_IS_ABILITY|SPELL_ATTR_IS_TRADESKILL))&&!(context.attrs3&SPELL_ATTR_EX3_IGNORE_CASTER_MODIFIERS)){factor=context.cast_speed;applies_speed=true;}
   else if(context.ranged&&!context.auto_repeat){factor=context.ranged_speed;applies_speed=true;}
  }
  if(applies_speed){
   float scaled=float(before_speed)*factor;
   if(!std::isfinite(scaled)||double(scaled)<std::numeric_limits<int32>::min()||double(scaled)>std::numeric_limits<int32>::max())throw std::runtime_error("excluded float-to-int32 conversion");
  }
 }
 output(in,'C',GetSpellCastTime(&info,caster,context.spell?&spell:nullptr,context.consume));
}
template<class Call> void expect_excluded(const char* message,Call call){
 try{call();}catch(const std::runtime_error& error){if(error.what()==std::string(message))return;throw;}
 throw std::runtime_error("missing authored arithmetic-domain exclusion");
}
void check_exclusions(){
 Input in{"not_a_fixture","source_only",true,1,1,0,0,0};Context c;
 in.base=std::numeric_limits<int32>::min();expect_excluded("excluded abs(INT32_MIN)",[&]{duration(in);});
 in.base=0;in.bound=std::numeric_limits<int32>::min();expect_excluded("excluded abs(INT32_MIN)",[&]{duration(in);});in.bound=0;
 in.rank=0x80000000;expect_excluded("excluded rank conversion",[&]{cast(in,c);});in.rank=0;
 c.caster=Context::NONE;expect_excluded("excluded null caster with spell",[&]{cast(in,c);});c={};
 in.base=-1;expect_excluded("excluded unsigned-to-int32 conversion",[&]{cast(in,c);});
 in.base=2147483500;c.attrs=SPELL_ATTR_USES_RANGED_SLOT;expect_excluded("excluded ranged overflow",[&]{cast(in,c);});
 in.base=1000;c={};c.mod_owner=true;c.delta=2147483647;expect_excluded("excluded modifier overflow",[&]{cast(in,c);});
 c={};c.cast_speed=std::numeric_limits<float>::infinity();expect_excluded("excluded float-to-int32 conversion",[&]{cast(in,c);});
 c.cast_speed=3000000.f;expect_excluded("excluded float-to-int32 conversion",[&]{cast(in,c);});
 c.cast_speed=-3000000.f;expect_excluded("excluded float-to-int32 conversion",[&]{cast(in,c);});
 c={};in.base=2147483647;expect_excluded("excluded float-to-int32 conversion",[&]{cast(in,c);});
 std::cout<<"11 arithmetic/pointer exclusion probes passed\n";
}
int main(int argc,char** argv){
 if(argc==2&&std::string(argv[1])=="--check-exclusions"){check_exclusions();return 0;}
 if(argc!=1)throw std::runtime_error("unexpected oracle argument");
 std::cout<<"# authored unchanged-source numeric vectors; no client DBC or loaded-store observation\n"
          <<"# categories describe existing neutral reducer scope; source_only context is named in each case\n"
          <<"# trigger/rank/trade/modifier/speed endpoints are adapters; no old-triggered semantic claim\n"
          <<"# case\tkind\tcategory\tpresent\tquery_index\trow_id\tbase\tper_level\tbound\trank\tbase_level\tsource_result\tsource_max_result\tordered_events\n";
 for(const auto& in:{
  Input{"duration_missing_zero","admitted",false,0,0,0,0,0},
  Input{"duration_missing_21","admitted",false,21,21,0,0,0},
  Input{"duration_found_zero","admitted",true,0,0,0,0,0},
  Input{"duration_found_zero_21","admitted",true,21,21,0,0,0},
  Input{"duration_permanent","admitted",true,21,21,-1,0,-1},
  Input{"duration_permanent_high_index","admitted",true,0xffffffff,0xffffffff,-1,0,-1},
  Input{"duration_positive","rejected",true,21,21,250,0,500},
  Input{"duration_absolute","rejected",true,21,21,-250,0,-500},
  Input{"duration_absolute_near_limit","rejected",true,21,21,-2147483647,0,2147483647},
  Input{"duration_base_permanent_max_zero","rejected",true,21,21,-1,0,0},
  Input{"duration_base_zero_max_permanent","rejected",true,21,21,0,0,-1},
  Input{"duration_ignored_per_level","rejected",true,21,21,0,7,0},
  Input{"duration_ignored_minimum_per_level","rejected",true,21,21,-1,(-2147483647-1),-1},
  Input{"duration_found_wrong_index","rejected",true,21,22,-1,0,-1},
  Input{"duration_missing_wrong_index","rejected",false,21,22,0,0,0},
 })duration(in);
 duration({"source_only_duration_null_info","source_only",true,21,21,-1,0,-1},true);
 for(const auto& in:{
  Input{"cast_missing_zero","admitted",false,0,0,0,0,0},
  Input{"cast_missing_one","admitted",false,1,1,0,0,0},
  Input{"cast_found_zero","admitted",true,0,0,0,0,0},
  Input{"cast_found_zero_one","admitted",true,1,1,0,0,0},
  Input{"cast_zero_nonzero_base_level","admitted",true,1,1,0,0,0,0,60},
  Input{"cast_zero_high_rank_index","admitted",true,0xffffffff,0xffffffff,0,0,0,2147483647,0xffffffff},
  Input{"cast_positive","rejected",true,1,1,1000,0,0},
  Input{"cast_per_level_zero_result","rejected",true,1,1,0,100,0},
  Input{"cast_rank_four","rejected",true,1,1,1000,100,0,4,0},
  Input{"cast_rank_five","rejected",true,1,1,1000,100,0,5,0},
  Input{"cast_rank_nine","rejected",true,1,1,1000,100,0,9,0},
  Input{"cast_rank_ten","rejected",true,1,1,1000,100,0,10,0},
  Input{"cast_below_base_level","rejected",true,1,1,1000,100,0,0,2},
  Input{"cast_negative_per_level","rejected",true,1,1,1000,-100,0,10,0},
  Input{"cast_negative_base_positive_scaled","rejected",true,1,1,-100,100,0,10,0},
  Input{"cast_minimum_clamp","rejected",true,1,1,100,0,500},
  Input{"cast_minimum_above_scaled","rejected",true,1,1,1000,-100,900,10,0},
  Input{"cast_negative_minimum","rejected",true,1,1,0,0,-1},
  Input{"cast_found_wrong_index","rejected",true,1,2,0,0,0},
  Input{"cast_missing_wrong_index","rejected",false,1,2,0,0,0},
 })cast(in);
 Input source{"source_only_redundant_trigger_true","source_only",true,1,1,1000,0,0};Context c;
 c.redundant=true;cast(source,c);
 source.name="source_only_redundant_trigger_false";c.redundant=false;cast(source,c);
 source.name="source_only_trade_matching_item";c.caster=Context::PLAYER;c.trade=true;c.trade_item=true;c.target_matches=true;cast(source,c);
 source.name="source_only_trade_different_item";c.target_matches=false;cast(source,c);
 source.name="source_only_trade_empty_slot";c.trade_item=false;cast(source,c);
 source.name="source_only_trade_missing";c.trade=false;cast(source,c);
 source.name="source_only_missing_before_rank_modifiers";source.present=false;c.mod_owner=true;c.delta=100;c.cast_speed=0.5f;c.attrs=SPELL_ATTR_USES_RANGED_SLOT;cast(source,c);source.present=true;
 source.name="source_only_no_spell_unit";c={};c.spell=false;source.rank=10;source.per=100;cast(source,c);
 source.name="source_only_no_spell_world_rank_zero";c.caster=Context::WORLD;cast(source,c);
 source.name="source_only_no_spell_null_caster";c.caster=Context::NONE;cast(source,c);source.rank=0;source.per=0;
 source.name="source_only_no_spell_maximum_cast";source.base=2147483647;cast(source,c);source.base=1000;
 source.name="source_only_ranged_plus_500";c={};c.attrs=SPELL_ATTR_USES_RANGED_SLOT;c.ranged=true;cast(source,c);
 source.name="source_only_auto_repeat_no_500";c.auto_repeat=true;cast(source,c);
 source.name="source_only_ranged_without_spell";c.spell=false;cast(source,c);
 source.name="source_only_spellmod_before_speed_consume";c={};c.mod_owner=true;c.delta=100;c.cast_speed=0.5f;cast(source,c);
 source.name="source_only_spellmod_no_consume";c.consume=false;cast(source,c);
 source.name="source_only_spellmod_negative_final_clamp";c.delta=-2000;cast(source,c);
 source.name="source_only_speed_truncation";c={};c.cast_speed=0.5f;source.base=1001;cast(source,c);source.base=1000;
 source.name="source_only_ability_skips_cast_speed";c.attrs=SPELL_ATTR_IS_ABILITY;cast(source,c);
 source.name="source_only_tradeskill_skips_cast_speed";c.attrs=SPELL_ATTR_IS_TRADESKILL;cast(source,c);
 source.name="source_only_ignore_skips_cast_speed";c.attrs=0;c.attrs3=SPELL_ATTR_EX3_IGNORE_CASTER_MODIFIERS;cast(source,c);
 source.name="source_only_ignore_does_not_skip_spellmod";c.mod_owner=true;c.delta=100;cast(source,c);
 source.name="source_only_ranged_ability_attack_speed";c={};c.attrs=SPELL_ATTR_USES_RANGED_SLOT|SPELL_ATTR_IS_ABILITY;c.ranged=true;c.ranged_speed=0.5f;cast(source,c);
 source.name="source_only_ranged_ability_auto_repeat";c.auto_repeat=true;cast(source,c);
 source.name="source_only_world_mod_owner_without_speed";c={};c.caster=Context::WORLD;c.mod_owner=true;c.delta=100;c.cast_speed=0.5f;cast(source,c);
 source.name="source_only_19968_after_modifiers";c={};c.id=19968;c.mod_owner=true;c.delta=100;c.cast_speed=0.5f;cast(source,c);
}
