// SPDX-License-Identifier: GPL-3.0-or-later
// Authored bounded adapters; PINNED markers receive unchanged CMaNGOS source.
#include <algorithm>
#include <array>
#include <cassert>
#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <map>
#include <memory>
#include <string>
#include <vector>
using uint8=uint8_t; using uint16=uint16_t; using uint32=uint32_t; using int32=int32_t;
using ObjectGuid=uint64_t;
// PINNED_RESULTS
constexpr uint8 NULL_BAG=0,NULL_SLOT=255,INVENTORY_SLOT_BAG_0=255,INVENTORY_SLOT_BAG_START=19,INVENTORY_SLOT_BAG_END=23,
INVENTORY_SLOT_ITEM_START=23,INVENTORY_SLOT_ITEM_END=39,KEYRING_SLOT_START=81,KEYRING_SLOT_END=97;
constexpr uint32 BAG_FAMILY_KEYS=256,ITEM_CLASS_CONTAINER=1,ITEM_SUBCLASS_CONTAINER=0;
constexpr uint32 BIND_WHEN_PICKED_UP=1,BIND_WHEN_EQUIPPED=2,BIND_QUEST_ITEM=4;
constexpr uint32 HIGHGUID_ITEM=0,MAX_ITEM_PROTO_SPELLS=5,DEFAULT_OBJECT_SCALE=1,PLAYER_FIELD_INV_SLOT_HEAD=100;
enum {ITEM_FIELD_OWNER,ITEM_FIELD_CONTAINED,ITEM_FIELD_STACK_COUNT,ITEM_FIELD_MAXDURABILITY,ITEM_FIELD_DURABILITY,
ITEM_FIELD_DURATION,ITEM_FIELD_CREATOR,ITEM_FIELD_GIFTCREATOR,ITEM_FIELD_FLAGS,ITEM_CHANGED,ITEM_REMOVED};
#define DEBUG_LOG(...) ((void)0)
#define MANGOS_ASSERT(x) assert(x)
struct ItemPrototype {
 uint32 ItemId,Stackable;int32 MaxCount;uint32 BagFamily=0,Class=15,SubClass=0,Bonding=0,MaxDurability=0,Duration=0;
 struct Spell {int32 SpellCharges=0;};std::array<Spell,5> Spells{};
 uint32 GetMaxStackSize()const{return Stackable;}
};
std::map<uint32,ItemPrototype> prototypes;
struct ObjectMgr {
 uint32 next=1000;
 static ItemPrototype const* GetItemPrototype(uint32 id){auto it=prototypes.find(id);return it==prototypes.end()?nullptr:&it->second;}
 uint32 GenerateItemLowGuid(){return next++;}
} sObjectMgr;
class Player;struct Bag;
struct Object {
 ObjectGuid guid=0;
 void _Create(uint32 low,uint32,uint32,uint32){guid=low;}
 ObjectGuid GetObjectGuid()const{return guid;}
};
struct Item:Object {
 uint32 entry=0;std::array<uint32,16> fields{};std::array<int32,5> charges{};bool bound=false;uint8 slot=0;
 ItemPrototype const*GetProto()const{return ObjectMgr::GetItemPrototype(entry);}
 uint32 GetEntry()const{return entry;}void SetEntry(uint32 id){entry=id;}
 uint32 GetCount()const{return fields[ITEM_FIELD_STACK_COUNT];}void SetCount(uint32 n){fields[ITEM_FIELD_STACK_COUNT]=n;}
 void SetUInt32Value(uint32 index,uint32 n){fields[index]=n;}uint32 GetUInt32Value(uint32 index)const{return fields[index];}
 void SetGuidValue(uint32 index,ObjectGuid n){fields[index]=uint32(n);}ObjectGuid GetGuidValue(uint32 index)const{return fields[index];}
 void SetSpellCharges(uint8 index,int32 n){charges[index]=n;}void SetObjectScale(uint32){}
 bool HasGeneratedLoot()const{return false;}bool HasTemporaryLoot()const{std::abort();}bool IsBindedNotWith(Player const*)const{std::abort();}bool IsBag()const{std::abort();}
 void SetBinding(bool n){bound=n;}void SetSlot(uint8 n){slot=n;}void SetContainer(Bag*){}void SetOwnerGuid(ObjectGuid){}
 void SetState(uint32,Player*){}void AddToWorld(){std::abort();}void RemoveFromWorld(){std::abort();}
 void SendCreateUpdateToPlayer(Player*){std::abort();}void DestroyForPlayer(Player*){std::abort();}
 static uint32 GenerateItemRandomPropertyId(uint32){return 0;}uint32 GetItemRandomPropertyId()const{return 0;}
 void SetItemRandomProperties(uint32){std::abort();}
 InventoryResult CanBeMergedPartlyWith(ItemPrototype const*)const;
 bool Create(uint32,uint32,Player const*);
 static Item* CreateItem(uint32,uint32,Player const*,uint32=0);
 Item*CloneItem(uint32,Player const*)const;
};
struct Bag:Item {bool IsEmpty()const{std::abort();}uint32 GetBagSize()const{std::abort();}void StoreItem(uint8,Item*){std::abort();}};
std::vector<std::unique_ptr<Item>> allocation;
Item*NewItemOrBag(ItemPrototype const*){allocation.push_back(std::make_unique<Item>());return allocation.back().get();}
bool ItemCanGoIntoBag(ItemPrototype const*,ItemPrototype const*){std::abort();}
// PINNED_POS_DECL
class Player {public:
 std::array<Item*,39> m_items{};
 std::array<uint32,4> callback{};
 Item*GetItemByPos(uint8 bag,uint8 slot)const {if(bag!=255)std::abort();return slot<m_items.size()?m_items[slot]:nullptr;}
 uint32 GetItemCount(uint32 id,bool,Item*)const{uint32 n=0;for(auto*i:m_items)if(i&&i->GetEntry()==id)n+=i->GetCount();return n;}
 uint32 GetMaxKeyringClientSize()const{std::abort();}
 ObjectGuid GetObjectGuid()const{return 1;}bool IsInWorld()const{return false;}
 void SetGuidValue(uint32,ObjectGuid){}bool IsBagPos(uint16)const{return false;}
 void AddEnchantmentDurations(Item*){}void AddItemDurations(Item*){}void RemoveEnchantmentDurations(Item*){}void RemoveItemDurations(Item*){}
 void ItemAddedQuestCheck(uint32 entry,uint32 count){callback={entry,count,GetItemCount(entry,true,nullptr),sObjectMgr.next};}
 InventoryResult _CanTakeMoreSimilarItems(uint32,uint32,Item*,uint32*)const;
 InventoryResult _CanStoreItem_InInventorySlots(uint8,uint8,ItemPosCountVec&,ItemPrototype const*,uint32&,bool,Item*,uint8,uint8)const;
 InventoryResult _CanStoreItem_InBag(uint8,ItemPosCountVec&,ItemPrototype const*,uint32&,bool,bool,Item*,uint8,uint8,uint8&)const;
 InventoryResult _CanStoreItem_InSpecificSlot(uint8,uint8,ItemPosCountVec&,ItemPrototype const*,uint32&,bool,Item*,uint8&)const{std::abort();}
 InventoryResult _CanStoreItem(uint8,uint8,ItemPosCountVec&,uint32,uint32,uint8&,Item*,bool,uint32*)const;
 Item*StoreNewItem(ItemPosCountVec const&,uint32,bool,int32=0);
 Item*StoreItem(ItemPosCountVec const&,Item*,bool);
 Item*_StoreItem(uint16,Item*,uint32,bool,bool);
};
struct ScriptManager{void OnItemLoot(Player*,Item*,bool){}}sScriptDevAIMgr;
// PINNED_POS
// PINNED_MERGE
// PINNED_UNIQUE
// PINNED_SLOTS
// PINNED_BAG
// PINNED_STORE
// PINNED_CREATE
// PINNED_CREATE_ITEM
// PINNED_CLONE
// PINNED_STORE_NEW
// PINNED_STORE_ITEM
// PINNED_STORE_ONE
void seed(Player&p,uint8 slot,uint32 entry,uint32 count){
 auto*i=NewItemOrBag(nullptr);i->entry=entry;i->guid=slot+1;i->SetCount(count);i->slot=slot;
 p.m_items[slot]=i;
}
void positions(ItemPosCountVec const&dest){if(dest.empty()){std::cout<<'-';return;}bool first=true;for(auto&d:dest){if(!first)std::cout<<',';first=false;std::cout<<(d.pos&255)<<':'<<unsigned(d.count);}}
void instances(Player const&p){bool first=true;for(uint32 s=23;s<39;++s){auto*i=p.m_items[s];if(!i)continue;if(!first)std::cout<<',';first=false;
 std::cout<<s<<':'<<i->guid<<':'<<i->entry<<':'<<i->GetCount()<<':'<<i->bound<<':'<<i->GetUInt32Value(ITEM_FIELD_DURABILITY)<<':'<<i->GetUInt32Value(ITEM_FIELD_MAXDURABILITY);
 for(auto charge:i->charges)std::cout<<':'<<charge;
}}
int main(int argc,char**argv){
 bool store=argc==2&&std::string(argv[1])=="--store";
 for(uint32 c=0;c<2048;++c){
  allocation.clear();prototypes.clear();sObjectMgr.next=1000;
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
  allocation.clear();prototypes.clear();sObjectMgr.next=1000;Player p;
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
