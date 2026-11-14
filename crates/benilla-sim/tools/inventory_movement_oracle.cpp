// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// CMaNGOS Project contributors; upstream GPL-2.0-or-later, translation GPL-3.0-or-later.
// Authored bounded adapters; PINNED markers receive unchanged CMaNGOS source.
// Synthetic owner1 has only backpack slots23..38. Equipment, equipped bags,
// bank and keyring are absent; carried bags are empty. Player is out-of-world.
// Random properties, enchantments, durations, temporary loot and scripts are
// absent. Unsupported execution dependencies abort rather than simulate success.
// Instance creation seeds the initial state with pinned Item/Bag::Create;
// the movement operation cannot generate an ID or clone an instance.
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
constexpr uint8 NULL_BAG=0,NULL_SLOT=255,INVENTORY_SLOT_BAG_0=255;
// PINNED_PLAYER_SLOTS
// PINNED_EQUIPMENT_SLOTS
// PINNED_INVENTORY_SLOTS
// PINNED_PACK_SLOTS
// PINNED_BANK_ITEM_SLOTS
// PINNED_BANK_BAG_SLOTS
// PINNED_BUYBACK_SLOTS
// PINNED_KEYRING_SLOTS
constexpr uint32 BASE_ATTACK=0,OFF_ATTACK=1;
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
 uint32 GenerateItemLowGuid(){std::abort();}
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
 bool HasGeneratedLoot()const{return false;}bool HasTemporaryLoot()const{return false;}bool IsBindedNotWith(Player const*)const{return false;}virtual bool IsBag()const{return false;}
 void SetBinding(bool n){bound=n;}void SetSlot(uint8 n){slot=n;}void SetContainer(Bag* value){assert(value==nullptr);}void SetOwnerGuid(ObjectGuid owner){SetGuidValue(ITEM_FIELD_OWNER,owner);}
 void SetState(uint32 state,Player*){assert(state==ITEM_CHANGED||state==ITEM_REMOVED);}void AddToWorld(){std::abort();}void RemoveFromWorld(){std::abort();}
 void SendCreateUpdateToPlayer(Player*){std::abort();}void DestroyForPlayer(Player*){std::abort();}
 static int32 GenerateItemRandomPropertyId(uint32);uint32 GetItemRandomPropertyId()const{return 0;}
 void SetItemRandomProperties(uint32){std::abort();}
 InventoryResult CanBeMergedPartlyWith(ItemPrototype const*)const;
 uint32 GetEnchantmentId(EnchantmentSlot)const{return 0;}
 uint32 GetEnchantmentDuration(EnchantmentSlot)const{std::abort();}
 void SendTimeUpdate(Player*){std::abort();}
 virtual bool Create(uint32,uint32,Player const*);
 static Item* CreateItem(uint32,uint32,Player const*,uint32=0);
 Item*CloneItem(uint32,Player const*)const{std::abort();}
 bool IsMainHandOnlyEnchant(EnchantmentSlot)const{std::abort();}
 void ClearEnchantment(EnchantmentSlot){std::abort();}
 void DeleteFromInventoryDB(){std::abort();}void SaveToDB(){std::abort();}
};
struct Bag:Item {
 std::array<Item*,MAX_BAG_SIZE>m_bagslot{};
 bool IsBag()const override{return true;}
 bool Create(uint32,uint32,Player const*)override;
 bool IsEmpty()const;uint32 GetBagSize()const{return GetUInt32Value(CONTAINER_FIELD_NUM_SLOTS);}
 Item*GetItemByPos(uint8)const;void StoreItem(uint8,Item*){std::abort();}void RemoveItem(uint8){std::abort();}
 uint32 GetItemCount(uint32,Item*)const{std::abort();}
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
 uint32 GetItemCount(uint32,bool,Item*)const;
 uint32 GetMaxKeyringClientSize()const{std::abort();}
 ObjectGuid GetObjectGuid()const{return 1;}bool IsInWorld()const{return false;}
 void SetGuidValue(uint32 field,ObjectGuid guid){assert(field>=PLAYER_FIELD_INV_SLOT_HEAD);root_guids.at((field-PLAYER_FIELD_INV_SLOT_HEAD)/2)=guid;}
 std::array<ObjectGuid,39>root_guids{};
 static bool IsBagPos(uint16);
 static bool IsInventoryPos(uint8,uint8);static bool IsEquipmentPos(uint8,uint8);static bool IsBankPos(uint8,uint8);
 static bool IsInventoryPos(uint16 pos){return IsInventoryPos(pos>>8,pos&255);}
 static bool IsEquipmentPos(uint16 pos){return IsEquipmentPos(pos>>8,pos&255);}
 static bool IsBankPos(uint16 pos){return IsBankPos(pos>>8,pos&255);}
 bool IsValidPos(uint8,uint8,bool)const;
 bool alive=true;InventoryResult error=EQUIP_ERR_OK;uint8 error_bag=0;ObjectGuid error_source=0,error_destination=0;
 bool IsAlive()const{return alive;}
 void SendEquipError(InventoryResult code,Item*source,Item*destination,uint8 bag=0){error=code;error_bag=bag;error_source=source?source->GetObjectGuid():0;error_destination=destination?destination->GetObjectGuid():0;}
 InventoryResult CanUnequipItem(uint16,bool)const{std::abort();}
 InventoryResult CanEquipItem(uint8,uint16&,Item*,bool)const{std::abort();}
 InventoryResult CanBankItem(uint8,uint8,ItemPosCountVec&,Item*,bool,uint8&)const{std::abort();}
 void BankItem(ItemPosCountVec const&,Item*,bool){std::abort();}
 void EquipItem(uint16,Item*,bool){std::abort();}
 void SwapItem(uint16,uint16);void RemoveItem(uint8,uint8,bool);
 void AutoUnequipOffhandIfNeed(uint8=0);
 void _ApplyItemMods(Item*,uint8,bool){std::abort();}
 void RemoveItemDependentAurasAndCasts(Item*){std::abort();}
 void UpdateWeaponDependantStats(uint32){std::abort();}
 void SetVisibleItemSlot(uint8,Item*){std::abort();}
 bool IsTwoHandUsed()const{std::cerr<<"unsupported-offhand: IsTwoHandUsed\n";std::abort();}
 void MoveItemFromInventory(uint8,uint8,bool){std::abort();}
 struct Session{std::string GetMangosString(uint32){std::abort();}};
 Session*GetSession(){std::abort();}
 void AddEnchantmentDurations(Item*);void AddItemDurations(Item*);
 void AddEnchantmentDuration(Item*,EnchantmentSlot,uint32){std::abort();}
 void SendNewItem(Item*item,uint32 count,bool received,bool created){notification={uint32(item->guid),count,received,created};}
 void RemoveEnchantmentDurations(Item*){}void RemoveItemDurations(Item*){}
 void ItemAddedQuestCheck(uint32 entry,uint32 count){callback={entry,count,GetItemCount(entry,true,nullptr),sObjectMgr.next};}
 InventoryResult _CanTakeMoreSimilarItems(uint32,uint32,Item*,uint32*)const;
 InventoryResult _CanStoreItem_InInventorySlots(uint8,uint8,ItemPosCountVec&,ItemPrototype const*,uint32&,bool,Item*,uint8,uint8)const;
 InventoryResult _CanStoreItem_InBag(uint8,ItemPosCountVec&,ItemPrototype const*,uint32&,bool,bool,Item*,uint8,uint8,uint8&)const;
 InventoryResult _CanStoreItem_InSpecificSlot(uint8,uint8,ItemPosCountVec&,ItemPrototype const*,uint32&,bool,Item*,uint8&)const;
 InventoryResult _CanStoreItem(uint8,uint8,ItemPosCountVec&,uint32,uint32,uint8&,Item*,bool,uint32*)const;
 // PINNED_CAN_STORE
 Item*StoreItem(ItemPosCountVec const&,Item*,bool);
 Item*_StoreItem(uint16,Item*,uint32,bool,bool);
};
struct Script {bool(*pItemLoot)(Player*,Item*,bool)=nullptr;};
struct ScriptDevAIMgr {
 Script*GetScript(uint32 id){assert(id==0);return nullptr;}
 bool OnItemLoot(Player*,Item*,bool);
}sScriptDevAIMgr;
void RemoveItemsSetItem(Player*,ItemPrototype const*){std::abort();}
struct Database{void BeginTransaction(){std::abort();}void CommitTransaction(){std::abort();}}CharacterDatabase;
constexpr uint32 LANG_NOT_EQUIPPED_ITEM=1,MAIL_STATIONERY_GM=1,MAIL_CHECK_MASK_COPIED=1;
struct MailSender{MailSender(Player*,uint32){std::abort();}};
struct MailDraft{explicit MailDraft(std::string const&){std::abort();}MailDraft&AddItem(Item*){std::abort();}void SendMailTo(Player*,MailSender,uint32){std::abort();}};
struct WorldPacket {
 std::array<uint8,2>input;uint32 cursor=0;
 WorldPacket(uint8 source,uint8 destination):input{source,destination}{}
 WorldPacket&operator>>(uint8&value){value=input.at(cursor++);return *this;}
};
struct WorldSession{Player*_player;void HandleSwapInvItemOpcode(WorldPacket&);};
// PINNED_RANDOM_PROPERTY
// PINNED_ENCHANTMENT_DURATIONS
// PINNED_ITEM_DURATIONS
// PINNED_LOOT_HOOK
// PINNED_POS
// PINNED_MERGE
// PINNED_COUNT
// PINNED_UNIQUE
// PINNED_SLOTS
// PINNED_BAG
// PINNED_SPECIFIC
// PINNED_STORE
// PINNED_CREATE
// PINNED_CREATE_BAG
// PINNED_STORE_ITEM
// PINNED_STORE_ONE
// PINNED_BAG_EMPTY
// PINNED_BAG_ITEM
// PINNED_INVENTORY_POS
// PINNED_EQUIPMENT_POS
// PINNED_BANK_POS
// PINNED_BAG_POS
// PINNED_VALID_POS
// PINNED_REMOVE
// PINNED_OFFHAND
// PINNED_SWAP
// PINNED_HANDLER
// Each fixture row carries its complete synthetic input. Durations/random
// properties/scripts remain zero; their dependent routes abort.
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
 i->SetCount(count);i->SetBinding(bound);i->SetSlot(slot);i->SetGuidValue(ITEM_FIELD_CONTAINED,p.GetObjectGuid());p.m_items[slot]=i;p.root_guids[slot]=i->GetObjectGuid();
}
// TSV: case, alive, source_slot, destination_slot, templates, before,
// result, error_bag, next_id, after, error_source_guid, error_destination_guid. No header, '-' denotes empty state.
// Templates: comma-separated entry:metadata using metadata() above.
// All instance IDs are initial slot+1. No movement path may allocate an ID.
// Ordinary rows cross all 16 scenarios, 4 stack sizes, 5 bonding modes,
// both directions, alive/dead, and initially bound/unbound (when legal).
void configure(uint32 entry,uint32 stack,uint32 bonding,uint32 kind){
 prototypes[entry]={entry,stack,0};auto&row=prototypes[entry];
 row.Bonding=bonding;row.MaxDurability=entry%100+17;
 row.Class=kind?ITEM_CLASS_CONTAINER:15;row.InventoryType=kind?INVTYPE_BAG:INVTYPE_NON_EQUIP;
 row.ContainerSlots=kind?std::array<uint32,4>{0,1,16,36}[kind%4]:0;
 row.StartQuest=entry+10000;row.ItemSet=entry+20000;
 for(uint32 k=0;k<5;++k){auto&s=row.Spells[k];s.SpellId=entry+30000+k;s.SpellTrigger=k;
  s.SpellCharges=std::array<int32,5>{-5,0,3,-1,99}[k];s.SpellPPMRate=float(k)*0.25f;
  s.SpellCooldown=k%2?-1:123+k;s.SpellCategory=91+k;s.SpellCategoryCooldown=k%2?77+k:-1;
 }
}
void emit(uint32 c,Player&p,uint8 source,uint8 destination){
 for(auto&row:prototypes){auto count=p.GetItemCount(row.first,true,nullptr);row.second.MaxCount=c%3==0?0:count+int32(c%2);}
 const auto before=snapshot(p);
 WorldSession session{&p};WorldPacket packet(source,destination);
 session.HandleSwapInvItemOpcode(packet);
 assert(sObjectMgr.next==1000);
 assert((p.callback==std::array<uint32,4>{}));assert((p.notification==std::array<uint32,4>{}));
 for(uint32 slot=0;slot<p.m_items.size();++slot){
  auto*item=p.m_items[slot];assert((item?item->GetObjectGuid():0)==p.root_guids[slot]);
  if(item){assert(item->slot==slot);assert(item->GetGuidValue(ITEM_FIELD_OWNER)==1);assert(item->GetGuidValue(ITEM_FIELD_CONTAINED)==1);}
 }
 std::cout<<c<<'\t'<<p.alive<<'\t'<<unsigned(source)<<'\t'<<unsigned(destination)<<'\t';
 bool first=true;for(auto const&row:prototypes){if(!first)std::cout<<',';first=false;std::cout<<row.first<<':';metadata(row.second);}
 std::cout<<'\t'<<before<<'\t'<<unsigned(p.error)<<'\t'<<unsigned(p.error_bag)<<'\t'<<sObjectMgr.next<<'\t'<<snapshot(p)<<'\t'<<p.error_source<<'\t'<<p.error_destination<<'\n';
}
void cases(){
 uint32 c=0;
 for(uint32 stack:std::array<uint32,4>{1,2,7,255})
 for(uint32 bonding=0;bonding<5;++bonding)
 for(uint32 mode=0;mode<16;++mode)
 for(uint32 reverse=0;reverse<2;++reverse)
 for(uint32 alive=0;alive<2;++alive)
 for(uint32 bound=0;bound<2;++bound){
  reset();Player p;p.alive=alive;uint8 source=reverse?38:23,destination=reverse?23:38;
  configure(70001,stack,bonding,0);configure(70002,stack,(bonding+1)%5,0);configure(70003,1,0,0);
  const auto bind=[&](uint32 entry){auto b=prototypes[entry].Bonding;return bound&&(b==1||b==4);};
  uint32 source_count=1,destination_count=1,destination_entry=70001;
  if(mode==7||mode==9||mode==13||mode==14)source_count=stack;
  if(mode==8||mode==9)destination_count=stack;
  if(mode==10)source_count=std::max(1u,stack-1);
  if(mode==11)destination_count=std::max(1u,stack-1);
  if(mode==12)source_count=std::max(1u,stack/2);
  if(mode==13)destination_count=std::max(1u,stack/2);
  if(mode==5||mode==15)destination_entry=70002;
  if(mode!=0&&mode!=1&&mode!=3)seed_created(p,source,70001,source_count,bind(70001));
  if(mode==1||mode>=5)seed_created(p,destination,destination_entry,destination_count,bind(destination_entry));
  if(mode==2||mode==3)destination=source;
  if(mode==14)for(uint32 s=23;s<39;++s)if(s!=source&&s!=destination)seed_created(p,s,70003,1,false);
  if(mode==15)seed_created(p,31,70001,stack,bind(70001));
  emit(c++,p,source,destination);
 }
 // Bags stay empty and carried. Different capacities and bondings must survive
 // empty moves, item swaps, and bag swaps without equipping or transferring.
 for(uint32 mode=0;mode<8;++mode)
 for(uint32 bonding=0;bonding<5;++bonding)
 for(uint32 reverse=0;reverse<2;++reverse)
 for(uint32 alive=0;alive<2;++alive)
 for(uint32 bound=0;bound<2;++bound){
  reset();Player p;p.alive=alive;uint8 source=reverse?38:23,destination=reverse?23:38;
  configure(70001,1,bonding,mode==7?0:mode%4+1);
  configure(70002,1,(bonding+1)%5,mode==1?0:(mode+1)%4+1);
  const auto bind=[&](uint32 entry){auto b=prototypes[entry].Bonding;return bound&&(b==1||b==4);};
  if(mode!=5)seed_created(p,source,70001,1,bind(70001));
  if(mode!=0&&mode!=6)seed_created(p,destination,mode==3?70001:70002,1,bind(mode==3?70001:70002));
  if(mode==6)destination=source;
  emit(c++,p,source,destination);
 }
 reset();
}
int main(int argc,char**argv){
 if(argc>1){
  Player p;configure(70001,7,0,0);seed_created(p,23,70001,1,false);auto*item=p.m_items[23];
  const std::string probe=argv[1];
  if(probe=="--unsupported-clone")p._StoreItem(uint16(255u<<8|24),item,1,true,false);
  else if(probe=="--unsupported-script"){prototypes[70001].ScriptId=1;p.SwapItem(uint16(255u<<8|23),uint16(255u<<8|24));}
  else if(probe=="--unsupported-duration"){item->SetUInt32Value(ITEM_FIELD_DURATION,1);p.SwapItem(uint16(255u<<8|23),uint16(255u<<8|24));}
  else if(probe=="--unsupported-equipment")p.SwapItem(uint16(255u<<8|23),uint16(255u<<8|4));
  else if(probe=="--unsupported-bank")p.SwapItem(uint16(255u<<8|23),uint16(255u<<8|39));
  else if(probe=="--unsupported-bag")p.SwapItem(uint16(255u<<8|23),uint16(19u<<8));
  else if(probe=="--unsupported-offhand"){
   configure(70002,1,0,0);seed_created(p,24,70002,1,false);seed_created(p,16,70001,1,false);
   p.SwapItem(uint16(255u<<8|23),uint16(255u<<8|24));
  }
  else return 2;
  return 0;
 }
 cases();
}
