#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Pinned source reward-item + real inventory integration on authored quest rows.

CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f Player.cpp:
CanRewardQuest overloads 12378-12455; RewardQuest 12611-12766; StoreNewItem
9914-9980 and storage helpers are extracted by signature. GPL-2.0-or-later
source, copyright CMaNGOS contributors, without warranty. Authored harness is
GPL-3.0-or-later. No game rows/assets. DestroyItemCount's backpack-only adapter
is explicitly authored; quest callbacks and inventory creation/planning are
unchanged source. ItemAddedQuestCheck dispatches through a virtual adapter to
the exact quest method before actual StoreItem mutations.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
from generate_inventory_fixture import PINS, body
from generate_quest_lifecycle_fixture import HASHES

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream",type=Path)
    parser.add_argument("--check",action="store_true")
    args=parser.parse_args()
    source={}
    pins=dict(PINS)
    for name,pin in HASHES.items():
        path="Globals/"+name if name=="ObjectMgr.cpp" else "Quests/"+name if name.startswith("QuestDef") else "Entities/"+name
        pins[path]=pin
    for path,pin in pins.items():
        raw=(args.upstream/"src/game"/path).read_bytes()
        assert hashlib.sha256(raw).hexdigest()==pin,path
        source[path]=raw.decode()
    player,item=source["Entities/Player.cpp"],source["Entities/Item.cpp"]
    pieces={
        "RESULTS":body(source["Entities/Item.h"],"enum InventoryResult\n")+";",
        "POS_DECL":body(source["Entities/Player.h"],"struct ItemPosCount\n")+";\ntypedef std::vector<ItemPosCount> ItemPosCountVec;",
        "POS":body(player,"bool ItemPosCount::isContainedIn("),
        "MERGE":body(item,"InventoryResult Item::CanBeMergedPartlyWith("),
        "UNIQUE":body(player,"InventoryResult Player::_CanTakeMoreSimilarItems("),
        "SLOTS":body(player,"InventoryResult Player::_CanStoreItem_InInventorySlots("),
        "BAG":body(player,"InventoryResult Player::_CanStoreItem_InBag("),
        "STORE":body(player,"InventoryResult Player::_CanStoreItem("),
        "CREATE":body(item,"bool Item::Create("),
        "CREATE_ITEM":body(item,"Item* Item::CreateItem("),
        "CLONE":body(item,"Item* Item::CloneItem("),
        "STORE_NEW":body(player,"Item* Player::StoreNewItem("),
        "STORE_ITEM":body(player,"Item* Player::StoreItem("),
        "STORE_ONE":body(player,"Item* Player::_StoreItem("),
    }
    inventory=Path(__file__).with_name("inventory_oracle.cpp").read_text().split("int main(")[0]
    inventory=inventory.replace("using uint8=","namespace storage {\nusing uint8=",1)
    inventory=inventory.replace("void ItemAddedQuestCheck(","virtual void ItemAddedQuestCheck(",1)
    inventory=inventory.replace("uint32 GetEntry()const{return entry;}","int GetBagSlot()const{return 255;}int GetSlot()const{return slot;}uint32 GetEntry()const{return entry;}")
    for marker,piece in pieces.items():
        token="// PINNED_"+marker+"\n"
        assert inventory.count(token)==1,marker
        inventory=inventory.replace(token,piece+"\n")
    inventory+="\n}\n#undef DEBUG_LOG\n#undef MANGOS_ASSERT\n"
    harness=Path(__file__).with_name("quest_reward_items_oracle.cpp").read_text().replace("// PINNED_INVENTORY_HARNESS",inventory)
    signatures=[
        "bool Player::CanTakeQuest(","bool Player::CanAddQuest(","bool Player::CanCompleteQuest(",
        "bool Player::CanRewardQuest(Quest const* pQuest, bool","bool Player::CanRewardQuest(Quest const* pQuest, uint32",
        "void Player::AddQuest(","void Player::CompleteQuest(","void Player::IncompleteQuest(","void Player::RewardQuest(",
        "bool Player::SatisfyQuestLevel(","bool Player::SatisfyQuestClass(","bool Player::SatisfyQuestRace(",
        "bool Player::SatisfyQuestPreviousQuest(","bool Player::SatisfyQuestNextChain(","bool Player::SatisfyQuestPrevChain(",
        "bool Player::SatisfyQuestStatus(","bool Player::SatisfyQuestLog(","bool Player::IsCurrentQuest(","void Player::SetQuestStatus(",
        "uint16 Player::FindQuestSlot(","void Player::AdjustQuestReqItemCount(","void Player::ItemAddedQuestCheck(",
        "void Player::ItemRemovedQuestCheck(","void Player::KilledMonsterCredit(","void Player::MoneyChanged(","void Player::TalkedToCreature(",
    ]
    methods="\n".join(body(player,sig) for sig in signatures)
    methods+="\n"+"\n".join(body(source["Quests/QuestDef.cpp"],sig) for sig in ["uint32 Quest::XPValue(","int32  Quest::GetRewOrReqMoney()","uint32 Quest::GetRewMoneyMaxLevel()"])
    harness=harness.replace("// PINNED_METHODS",methods)
    harness=harness.replace("// PINNED_MODIFY_MONEY",body(source["Entities/Player.h"],"void ModifyMoney(int32 d)").replace("ModifyMoney(","ModifyMoneyNative(",1))
    loader=source["Globals/ObjectMgr.cpp"]
    harness=harness.replace("// PINNED_RELATION_LOADER",loader[loader.index("        if (qinfo->NextQuestInChain)"):loader.index("        if (qinfo->ExclusiveGroup)",loader.index("        if (qinfo->NextQuestInChain)"))])
    with tempfile.TemporaryDirectory() as tmp:
        cpp,exe=Path(tmp)/"oracle.cpp",Path(tmp)/"oracle"
        cpp.write_text(harness)
        subprocess.run(["g++","-std=c++17","-O2","-fno-fast-math","-ffp-contract=off","-fsanitize=undefined,float-cast-overflow","-fno-sanitize-recover=all",str(cpp),"-o",str(exe)],check=True)
        output=subprocess.check_output([str(exe)])
    target=Path(__file__).parents[1]/"tests/fixtures/quest_reward_items_cmangos.tsv"
    if args.check:
        assert target.read_bytes()==output,"quest reward item fixture changed"
    else:
        target.write_bytes(output)
    print(f"{target.name}: {len(output.splitlines())} exact source cases, SHA256 {hashlib.sha256(output).hexdigest()}")

if __name__=="__main__":main()
