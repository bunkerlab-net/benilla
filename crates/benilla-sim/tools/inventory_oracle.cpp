// SPDX-License-Identifier: GPL-3.0-or-later
// Authored bounded adapters; PINNED markers receive unchanged CMaNGOS source.
#include <algorithm>
#include <array>
#include <cassert>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <map>
#include <set>
#include <sstream>
#include <string>
#include <vector>
using uint8=uint8_t; using uint16=uint16_t; using uint32=uint32_t; using int32=int32_t;
using ObjectGuid=uint64_t;
// PINNED_RESULTS
constexpr uint8 NULL_BAG=0,NULL_SLOT=255,INVENTORY_SLOT_BAG_0=255,INVENTORY_SLOT_BAG_START=19,INVENTORY_SLOT_BAG_END=23,
INVENTORY_SLOT_ITEM_START=23,INVENTORY_SLOT_ITEM_END=39,KEYRING_SLOT_START=81,KEYRING_SLOT_END=97;
// PINNED_BAG_FAMILY
constexpr uint32 ITEM_CLASS_CONTAINER=1,ITEM_SUBCLASS_CONTAINER=0;
constexpr uint32 BIND_WHEN_PICKED_UP=1,BIND_WHEN_EQUIPPED=2,BIND_QUEST_ITEM=4;
constexpr uint32 HIGHGUID_CONTAINER=1,MAX_BAG_SIZE=36;
// PINNED_INVENTORY_TYPE
// PINNED_ENCHANTMENT_SLOT
constexpr uint32 CONTAINER_FIELD_NUM_SLOTS=16,CONTAINER_FIELD_SLOT_1=17;
constexpr uint32 HIGHGUID_ITEM=0,MAX_ITEM_PROTO_SPELLS=5,DEFAULT_OBJECT_SCALE=1,PLAYER_FIELD_INV_SLOT_HEAD=100;
enum {ITEM_FIELD_OWNER,ITEM_FIELD_CONTAINED,ITEM_FIELD_STACK_COUNT,ITEM_FIELD_MAXDURABILITY,ITEM_FIELD_DURABILITY,
ITEM_FIELD_DURATION,ITEM_FIELD_CREATOR,ITEM_FIELD_GIFTCREATOR,ITEM_FIELD_FLAGS,ITEM_CHANGED,ITEM_REMOVED};
#define DEBUG_LOG(...) ((void)0)
#define MANGOS_ASSERT(x) assert(x)
// PINNED_SPELL_DECL
struct ItemPrototype {
 uint32 ItemId,Stackable;int32 MaxCount;uint32 BagFamily=0,Class=15,SubClass=0,Bonding=0,MaxDurability=0,Duration=0;
 uint32 InventoryType=0,ContainerSlots=0,RandomProperty=0,ScriptId=0,StartQuest=0,ItemSet=0;
 std::array<_Spell,5> Spells{};
 uint32 GetMaxStackSize()const{return Stackable;}
};
std::map<uint32,ItemPrototype> prototypes;
struct ObjectMgr {
 uint32 next=1000;
 static ItemPrototype const* GetItemPrototype(uint32 id){auto it=prototypes.find(id);return it==prototypes.end()?nullptr:&it->second;}
 uint32 GenerateItemLowGuid(){return next++;}
} sObjectMgr;
struct ItemStorage {template<class T>T const*LookupEntry(uint32 id)const{return ObjectMgr::GetItemPrototype(id);}}sItemStorage;
struct ItemRandomPropertiesEntry {uint32 ID;};
struct RandomPropertyStore {ItemRandomPropertiesEntry const*LookupEntry(uint32)const{std::abort();}}sItemRandomPropertiesStore;
uint32 GetItemEnchantMod(uint32){std::abort();}
struct Log {template<class... T>void outErrorDb(T...){std::abort();}}sLog;
class Player;struct Bag;struct Item;
std::set<Item*> allocations;
struct Object {
 ObjectGuid guid=0;
 void _Create(uint32 low,uint32,uint32,uint32){guid=low;}
 ObjectGuid GetObjectGuid()const{return guid;}
};
struct Item:Object {
 Item(){allocations.insert(this);}virtual ~Item(){allocations.erase(this);}
 uint32 entry=0;std::array<uint32,90> fields{};std::array<int32,5> charges{};bool bound=false;uint8 slot=0;
 ItemPrototype const*GetProto()const{return ObjectMgr::GetItemPrototype(entry);}
 uint32 GetEntry()const{return entry;}void SetEntry(uint32 id){entry=id;}
 uint32 GetCount()const{return fields[ITEM_FIELD_STACK_COUNT];}void SetCount(uint32 n){fields[ITEM_FIELD_STACK_COUNT]=n;}
 void SetUInt32Value(uint32 index,uint32 n){fields[index]=n;}uint32 GetUInt32Value(uint32 index)const{return fields[index];}
 void SetGuidValue(uint32 index,ObjectGuid n){fields[index]=uint32(n);}ObjectGuid GetGuidValue(uint32 index)const{return fields[index];}
 void SetSpellCharges(uint8 index,int32 n){charges[index]=n;}void SetObjectScale(uint32){}
 bool HasGeneratedLoot()const{return false;}bool HasTemporaryLoot()const{std::abort();}bool IsBindedNotWith(Player const*)const{std::abort();}virtual bool IsBag()const{return false;}
 void SetBinding(bool n){bound=n;}void SetSlot(uint8 n){slot=n;}void SetContainer(Bag*){}void SetOwnerGuid(ObjectGuid){}
 void SetState(uint32,Player*){}void AddToWorld(){std::abort();}void RemoveFromWorld(){std::abort();}
 void SendCreateUpdateToPlayer(Player*){std::abort();}void DestroyForPlayer(Player*){std::abort();}
 static int32 GenerateItemRandomPropertyId(uint32);uint32 GetItemRandomPropertyId()const{return 0;}
 void SetItemRandomProperties(uint32){std::abort();}
 InventoryResult CanBeMergedPartlyWith(ItemPrototype const*)const;
 uint32 GetEnchantmentId(EnchantmentSlot)const{return 0;}
 uint32 GetEnchantmentDuration(EnchantmentSlot)const{std::abort();}
 void SendTimeUpdate(Player*){std::abort();}
 virtual bool Create(uint32,uint32,Player const*);
 static Item* CreateItem(uint32,uint32,Player const*,uint32=0);
 Item*CloneItem(uint32,Player const*)const;
};
struct Bag:Item {
 std::array<Item*,MAX_BAG_SIZE>m_bagslot{};
 bool IsBag()const override{return true;}
 bool Create(uint32,uint32,Player const*)override;
 bool IsEmpty()const{std::abort();}uint32 GetBagSize()const{std::abort();}void StoreItem(uint8,Item*){std::abort();}
};
void reset(){while(!allocations.empty())delete *allocations.begin();prototypes.clear();sObjectMgr.next=1000;}
// PINNED_NEW_ITEM_OR_BAG
bool ItemCanGoIntoBag(ItemPrototype const*,ItemPrototype const*){std::abort();}
// PINNED_POS_DECL
class Player {public:
 std::array<Item*,39> m_items{};
 std::array<uint32,4> callback{};
 std::vector<Item*>m_itemDuration;
 std::array<uint32,4> notification{};
 Item*GetItemByPos(uint8 bag,uint8 slot)const {if(bag!=255)std::abort();return slot<m_items.size()?m_items[slot]:nullptr;}
 Item*GetItemByPos(uint16 pos)const{return GetItemByPos(pos>>8,pos&255);}
 uint32 GetItemCount(uint32 id,bool,Item*)const{uint32 n=0;for(auto*i:m_items)if(i&&i->GetEntry()==id)n+=i->GetCount();return n;}
 uint32 GetMaxKeyringClientSize()const{std::abort();}
 ObjectGuid GetObjectGuid()const{return 1;}bool IsInWorld()const{return false;}
 void SetGuidValue(uint32,ObjectGuid){}bool IsBagPos(uint16)const{return false;}
 void AddEnchantmentDurations(Item*);void AddItemDurations(Item*);
 void AddEnchantmentDuration(Item*,EnchantmentSlot,uint32){std::abort();}
 void SendNewItem(Item*item,uint32 count,bool received,bool created){notification={uint32(item->guid),count,received,created};}
 void RemoveEnchantmentDurations(Item*){}void RemoveItemDurations(Item*){}
 void ItemAddedQuestCheck(uint32 entry,uint32 count){callback={entry,count,GetItemCount(entry,true,nullptr),sObjectMgr.next};}
 InventoryResult _CanTakeMoreSimilarItems(uint32,uint32,Item*,uint32*)const;
 InventoryResult _CanStoreItem_InInventorySlots(uint8,uint8,ItemPosCountVec&,ItemPrototype const*,uint32&,bool,Item*,uint8,uint8)const;
 InventoryResult _CanStoreItem_InBag(uint8,ItemPosCountVec&,ItemPrototype const*,uint32&,bool,bool,Item*,uint8,uint8,uint8&)const;
 InventoryResult _CanStoreItem_InSpecificSlot(uint8,uint8,ItemPosCountVec&,ItemPrototype const*,uint32&,bool,Item*,uint8&)const{std::abort();}
 InventoryResult _CanStoreItem(uint8,uint8,ItemPosCountVec&,uint32,uint32,uint8&,Item*,bool,uint32*)const;
 // PINNED_CAN_STORE_NEW
 Item*StoreNewItem(ItemPosCountVec const&,uint32,bool,int32=0);
 Item*StoreItem(ItemPosCountVec const&,Item*,bool);
 Item*_StoreItem(uint16,Item*,uint32,bool,bool);
};
struct Script {bool(*pItemLoot)(Player*,Item*,bool)=nullptr;};
struct ScriptDevAIMgr {
 Script*GetScript(uint32 id){assert(id==0);return nullptr;}
 bool OnItemLoot(Player*,Item*,bool);
}sScriptDevAIMgr;
constexpr uint32 LANG_ITEM_CANNOT_CREATE=1;
struct ChatHandler {
 bool sent_error=false;uint32 error_count=0;
 void PSendSysMessage(uint32 message,uint32 entry,uint32 count){assert(message==LANG_ITEM_CANNOT_CREATE&&entry==70001);error_count=count;}
 void SetSentErrorMessage(bool value){sent_error=value;}
 bool GrantItem(Player*,Player*,uint32,int32);
};
// PINNED_RANDOM_PROPERTY
// PINNED_ENCHANTMENT_DURATIONS
// PINNED_ITEM_DURATIONS
// PINNED_LOOT_HOOK
// PINNED_POS
// PINNED_MERGE
// PINNED_UNIQUE
// PINNED_SLOTS
// PINNED_BAG
// PINNED_STORE
// PINNED_CREATE
// PINNED_CREATE_BAG
// PINNED_CREATE_ITEM
// PINNED_CLONE
// PINNED_STORE_NEW
// PINNED_STORE_ITEM
// PINNED_STORE_ONE
// PINNED_GRANT
void seed(Player&p,uint8 slot,uint32 entry,uint32 count){
 auto*i=new Item;i->entry=entry;i->guid=slot+1;i->SetCount(count);i->slot=slot;
 p.m_items[slot]=i;
}
void positions(ItemPosCountVec const&dest){if(dest.empty()){std::cout<<'-';return;}bool first=true;for(auto&d:dest){if(!first)std::cout<<',';first=false;std::cout<<(d.pos&255)<<':'<<unsigned(d.count);}}
void instances(Player const&p){bool first=true;for(uint32 s=23;s<39;++s){auto*i=p.m_items[s];if(!i)continue;if(!first)std::cout<<',';first=false;
 std::cout<<s<<':'<<i->guid<<':'<<i->entry<<':'<<i->GetCount()<<':'<<i->bound<<':'<<i->GetUInt32Value(ITEM_FIELD_DURABILITY)<<':'<<i->GetUInt32Value(ITEM_FIELD_MAXDURABILITY);
 for(auto charge:i->charges)std::cout<<':'<<charge;
}}
// Each fixture row carries its complete synthetic input. Durations/random
// properties/scripts remain zero in grants; their dependent routes abort.
// Instance fields: slot:id:entry:count:bound:durability:max_durability:
// charge0:charge1:charge2:charge3:charge4:is_bag:container_slots:duration.
std::string snapshot(Player const&p){
 std::ostringstream out;bool first=true;
 for(uint32 s=23;s<39;++s){auto*i=p.m_items[s];if(!i)continue;
  if(!first)out<<',';first=false;
  out<<s<<':'<<i->guid<<':'<<i->entry<<':'<<i->GetCount()<<':'<<i->bound<<':'<<i->GetUInt32Value(ITEM_FIELD_DURABILITY)<<':'<<i->GetUInt32Value(ITEM_FIELD_MAXDURABILITY);
  for(auto charge:i->charges)out<<':'<<charge;
  out<<':'<<i->IsBag()<<':'<<i->GetUInt32Value(CONTAINER_FIELD_NUM_SLOTS)<<':'<<i->GetUInt32Value(ITEM_FIELD_DURATION);
  if(i->IsBag())for(uint32 k=0;k<MAX_BAG_SIZE;++k){assert(i->GetGuidValue(CONTAINER_FIELD_SLOT_1+k*2)==0);assert(static_cast<Bag*>(i)->m_bagslot[k]==nullptr);}
 }
 return first?"-":out.str();
}
uint32 float_bits(float value){uint32 result;static_assert(sizeof(result)==sizeof(value));std::memcpy(&result,&value,sizeof(result));return result;}
void metadata(ItemPrototype const&row){
 std::cout<<row.Stackable<<':'<<row.MaxCount<<':'<<row.Class<<':'<<row.InventoryType<<':'<<row.ContainerSlots<<':'<<row.Bonding<<':'<<row.MaxDurability<<':'<<row.BagFamily<<':'<<row.StartQuest<<':'<<row.ItemSet;
 for(auto&s:row.Spells)std::cout<<':'<<s.SpellId<<':'<<s.SpellTrigger<<':'<<s.SpellCharges<<':'<<float_bits(s.SpellPPMRate)<<':'<<s.SpellCooldown<<':'<<s.SpellCategory<<':'<<s.SpellCategoryCooldown;
}
void seed_created(Player&p,uint8 slot,uint32 entry,uint32 count,bool bound){
 auto*i=NewItemOrBag(ObjectMgr::GetItemPrototype(entry));assert(i->Create(slot+1,entry,&p));
 i->SetCount(count);i->SetBinding(bound);i->SetSlot(slot);p.m_items[slot]=i;
}
void notification(std::array<uint32,4>const&values){for(uint32 k=0;k<4;++k){if(k)std::cout<<':';std::cout<<values[k];}}
void grants(){
 // TSV: case, other_target, requested, template, equipped_count, before,
 // handler_success, plan_result, no_space, granted, next_id, quest_callback,
 // destinations, last_slot_id_count, GM_notification, target_notification,
 // error_count, sent_error, after. Empty snapshots/destinations/last use '-'.
 // Template: stack:max_count:class:inventory_type:slots:bonding:durability:
 // bag_family:startquest:itemset, then five (id:trigger:charges:ppm_rate_bits:cooldown:
 // category:category_cooldown) groups. Callback/notifications have four fields.
 for(uint32 c=0;c<512;++c)for(uint32 other=0;other<2;++other){
  reset();Player gm,recipient;Player&p=other?recipient:gm;
  uint32 stack=std::array<uint32,4>{1,2,7,255}[c%4];
  prototypes[70001]={70001,stack,0};prototypes[70002]={70002,1,0};auto&row=prototypes[70001];
  row.Class=std::array<uint32,4>{15,1,11,2}[(c/4)%4];
  row.InventoryType=std::array<uint32,4>{0,18,27,18}[(c/16)%4];
  row.ContainerSlots=std::array<uint32,4>{0,1,16,36}[(c/7)%4];
  row.Bonding=c%5;row.MaxDurability=c*7;
  row.BagFamily=std::array<uint32,4>{0,1,2,8}[(c/9)%4];
  row.StartQuest=c%3==0?0:80001+c;row.ItemSet=c%5==0?0:90001+c;
  for(uint32 k=0;k<5;++k){auto&s=row.Spells[k];s.SpellId=81000+c*5+k;s.SpellTrigger=k;
   s.SpellCharges=std::array<int32,5>{-5,0,3,-1,99}[k];s.SpellPPMRate=float(c%9+k)*0.25f;s.SpellCooldown=k%2?-1:int32(c*2+k);s.SpellCategory=91+k;s.SpellCategoryCooldown=k%2?int32(c+k):-1;}
  uint32 equipped=row.InventoryType!=18&&c%5==0?1:0;
  if(equipped)seed_created(p,4,70001,equipped,false);
  uint32 layout=(c/64)%4;
  for(uint32 slot=23;slot<39;++slot){
   if(layout==0)continue;
   if(layout==1){if(slot<38)seed_created(p,slot,70002,1,false);continue;}
   if(layout==2){uint32 mode=(c+slot)%4;if(mode<2)seed_created(p,slot,70001,1+(c+slot)%stack,(row.Bonding==1||row.Bonding==4)&&slot%2==0);else if(mode==2)seed_created(p,slot,70002,1,false);continue;}
   seed_created(p,slot,slot%2?70001:70002,slot%2?stack:1,(row.Bonding==1||row.Bonding==4)&&slot%2);
  }
  uint32 previous=p.GetItemCount(70001,true,nullptr);
  row.MaxCount=c%3==0?0:previous+(c/3)%10;
  uint32 count=std::array<uint32,8>{1,2,7,255,256,257,4096,10000}[(c/3)%8];
  uint32 no_space=0;ItemPosCountVec dest;
  auto code=p.CanStoreNewItem(NULL_BAG,NULL_SLOT,dest,70001,count,&no_space);
  auto before=snapshot(p);ChatHandler handler;
  bool success=handler.GrantItem(&gm,&p,70001,int32(count));
  uint32 granted=p.GetItemCount(70001,true,nullptr)-previous;
  std::cout<<c<<'\t'<<other<<'\t'<<count<<'\t';metadata(row);
  std::cout<<'\t'<<equipped<<'\t'<<before<<'\t'<<success<<'\t'<<unsigned(code)<<'\t'<<no_space<<'\t'<<granted<<'\t'<<sObjectMgr.next<<'\t';notification(p.callback);
  std::cout<<'\t';positions(dest);std::cout<<'\t';
  if(granted){auto*last=p.GetItemByPos(dest.back().pos);assert(last);std::cout<<unsigned(last->slot)<<':'<<last->guid<<':'<<last->GetCount();}else std::cout<<'-';
  std::cout<<'\t';notification(gm.notification);std::cout<<'\t';notification(p.notification);
  std::cout<<'\t'<<handler.error_count<<'\t'<<handler.sent_error<<'\t'<<snapshot(p)<<'\n';
 }
 reset();
}
void creations(){
 // TSV: case, inventory_type, class, container_slots, requested, created,
 // next_id, instance_or_dash, owner_contained_or_dash. Fixed inputs are
 // stack255/durability123/duration4567/charges[-5,0,3,-1,99]/owner1/next1000.
 uint32 c=0;
 for(uint32 inventory:std::array<uint32,3>{0,18,27})
 for(uint32 item_class:std::array<uint32,3>{1,11,15})
 for(uint32 slots:std::array<uint32,6>{0,1,16,36,37,255})
 for(uint32 count:std::array<uint32,4>{0,1,255,256}){
  reset();Player owner;prototypes[70001]={70001,255,0};auto&row=prototypes[70001];
  row.InventoryType=inventory;row.Class=item_class;row.ContainerSlots=slots;row.MaxDurability=123;row.Duration=4567;
  for(uint32 k=0;k<5;++k)row.Spells[k].SpellCharges=std::array<int32,5>{-5,0,3,-1,99}[k];
  auto*item=Item::CreateItem(70001,count,&owner);
  std::cout<<c++<<'\t'<<inventory<<'\t'<<item_class<<'\t'<<slots<<'\t'<<count<<'\t'<<bool(item)<<'\t'<<sObjectMgr.next<<'\t';
  if(item){owner.m_items[23]=item;item->SetSlot(23);std::cout<<snapshot(owner)<<'\t'<<item->GetGuidValue(ITEM_FIELD_OWNER)<<':'<<item->GetGuidValue(ITEM_FIELD_CONTAINED);}else std::cout<<"-\t-";
  std::cout<<'\n';
 }
 reset();
}
void unsupported(std::string const&mode){
 reset();Player player;prototypes[70001]={70001,1,0};auto&row=prototypes[70001];
 if(mode=="--unsupported-random")row.RandomProperty=1;
 else if(mode=="--unsupported-script")row.ScriptId=1;
 else if(mode=="--unsupported-duration")row.Duration=1;
 else if(mode=="--unsupported-keyring")row.BagFamily=BAG_FAMILY_KEYS;
 else std::exit(2);
 ChatHandler handler;handler.GrantItem(&player,&player,70001,1);
}

int main(int argc,char**argv){
 if(argc==2&&std::string(argv[1]).find("--unsupported-")==0){unsupported(argv[1]);return 0;}
 if(argc==2&&std::string(argv[1])=="--grant"){grants();return 0;}
 if(argc==2&&std::string(argv[1])=="--create"){creations();return 0;}
 bool store=argc==2&&std::string(argv[1])=="--store";
 for(uint32 c=0;c<2048;++c){
  reset();
  uint32 stack=c<1024?1+c%7:std::array<uint32,4>{1,2,7,255}[c%4];int32 maximum=c%4==0?0:10+c%37;Player p;
  if(c%5==0)seed(p,4,70001,1);
  for(uint32 s=23;s<39;++s){uint32 mode=(c*13+s*7)%5;if(mode<2)seed(p,s,70001,1+(c+s)%stack);else if(mode<4)seed(p,s,70002,1);}
  if(maximum>0&&p.GetItemCount(70001,true,nullptr)>uint32(maximum))continue;
  prototypes[70001]={70001,stack,maximum};prototypes[70002]={70002,1,0};
  uint32 count=c<1024?1+c%29:std::array<uint32,7>{1,254,255,256,257,4096,10000}[c%7],no_space=0;uint8 bagSlot=0;ItemPosCountVec dest;
  if(!store&&c>=1024)for(uint32 s=23;s<39;++s)if((c+s)%7==0)dest.emplace_back((255<<8)|s,1);
  auto code=p._CanStoreItem(NULL_BAG,NULL_SLOT,dest,70001,count,bagSlot,nullptr,false,&no_space);
  if(!store){std::cout<<c<<'\t'<<unsigned(code)<<'\t'<<no_space<<'\t';positions(dest);std::cout<<'\n';}
  else if(code==EQUIP_ERR_OK){auto*last=p.StoreNewItem(dest,70001,false);assert(last);
   std::cout<<c<<'\t'<<sObjectMgr.next<<'\t';for(uint32 k=0;k<4;++k){if(k)std::cout<<':';std::cout<<p.callback[k];}
   std::cout<<'\t'<<unsigned(last->slot)<<':'<<last->guid<<':'<<last->GetCount()<<'\t';instances(p);std::cout<<'\n';
  }
 }
 // Empty inventory covers full uint8 stack size, original/cloned identities,
 // source binding, durability and the pre-store callback on split creation.
 for(uint32 c=2048;c<2080;++c){
  reset();Player p;
  uint32 stack=255,count=1+(c-2048)*100;prototypes[70001]={70001,stack,0};
  prototypes[70001].Bonding=(c-2048)%5;prototypes[70001].MaxDurability=(c-2048)*7;
  uint32 no_space=0;uint8 bagSlot=0;ItemPosCountVec dest;
  auto code=p._CanStoreItem(NULL_BAG,NULL_SLOT,dest,70001,count,bagSlot,nullptr,false,&no_space);
  if(!store){std::cout<<c<<'\t'<<unsigned(code)<<'\t'<<no_space<<'\t';positions(dest);std::cout<<'\n';}
  else {assert(code==EQUIP_ERR_OK);auto*last=p.StoreNewItem(dest,70001,false);assert(last);
   std::cout<<c<<'\t'<<sObjectMgr.next<<'\t';for(uint32 k=0;k<4;++k){if(k)std::cout<<':';std::cout<<p.callback[k];}
   std::cout<<'\t'<<unsigned(last->slot)<<':'<<last->guid<<':'<<last->GetCount()<<'\t';instances(p);std::cout<<'\n';
  }
 }
}
