#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
# CMaNGOS Project contributors; upstream GPL-2.0-or-later, translation GPL-3.0-or-later.
"""Exact pinned-source SwapItem/packet-handler oracle; backpack-only synthetic rows."""
import argparse
import hashlib
from pathlib import Path
import resource
import signal
import subprocess
import tempfile

COMMIT = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
PINS = {
    "Entities/Player.cpp": "d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e",
    "Entities/Player.h": "acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb",
    "Entities/Item.cpp": "5e401746527531beb521d3eeaa6bc0b55fceaa0d4aef1e780d4f429a545f72e4",
    "Entities/Item.h": "a884fd9b01da289e3fd556979a6070091f4588400c29b84851d5e88e193bf414",
    "Entities/ItemPrototype.h": "64768cb942d73c17ce00fe76f953a3d7ca3e5e617a5f4c798cb5d259bf931756",
    "Entities/Bag.cpp": "d25b146210b4d13e880be6972cdc574fd72e91eeab5580870f4ee583fcaf6e24",
    "Entities/Bag.h": "936351212bfa52621d36eb3d1f38e494fc0b4ea3a3e7740fb344c3fb252a6c15",
    "Entities/ItemHandler.cpp": "833cd58e5cba491d72f11b12d5a48f7c0fa36b0310adbccc04442101b3b6edd7",
    "AI/ScriptDevAI/ScriptDevAIMgr.cpp": "ad6a4113dc1f31aa7059a1167b1409ffd1cfc8c8ebf2b2f73907b8598deb3592",
}


def body(source, signature):
    assert source.count(signature) == 1, signature
    start = source.index(signature)
    end = source.index("{", start) + 1
    depth = 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def assemble(upstream):
    source = {}
    for path, pin in PINS.items():
        raw = (upstream / "src/game" / path).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == pin, path
        source[path] = raw.decode()
    player, item = source["Entities/Player.cpp"], source["Entities/Item.cpp"]
    pieces = {}
    for marker, signature in {
        "SPELL_DECL": "struct _Spell\n", "BAG_FAMILY": "enum BagFamily\n",
        "INVENTORY_TYPE": "enum InventoryType\n",
    }.items():
        pieces[marker] = body(source["Entities/ItemPrototype.h"], signature) + ";"
    for marker, signature in {
        "PLAYER_SLOTS": "enum PlayerSlots", "EQUIPMENT_SLOTS": "enum EquipmentSlots",
        "INVENTORY_SLOTS": "enum InventorySlots", "PACK_SLOTS": "enum InventoryPackSlots",
        "BANK_ITEM_SLOTS": "enum BankItemSlots", "BANK_BAG_SLOTS": "enum BankBagSlots",
        "BUYBACK_SLOTS": "enum BuyBackSlots", "KEYRING_SLOTS": "enum KeyRingSlots",
    }.items():
        pieces[marker] = body(source["Entities/Player.h"], signature) + ";"
    pieces.update({
        "ENCHANTMENT_SLOT": body(source["Entities/Item.h"], "enum EnchantmentSlot\n") + ";",
        "RESULTS": body(source["Entities/Item.h"], "enum InventoryResult\n") + ";",
        "POS_DECL": body(source["Entities/Player.h"], "struct ItemPosCount\n")
            + ";\ntypedef std::vector<ItemPosCount> ItemPosCountVec;",
        "CAN_STORE": body(source["Entities/Player.h"], "InventoryResult CanStoreItem("),
        "NEW_ITEM_OR_BAG": body(source["Entities/Bag.h"], "inline Item* NewItemOrBag("),
        "HANDLER": body(source["Entities/ItemHandler.cpp"], "void WorldSession::HandleSwapInvItemOpcode("),
        "MERGE": body(item, "InventoryResult Item::CanBeMergedPartlyWith("),
        "CREATE": body(item, "bool Item::Create("),
        "RANDOM_PROPERTY": body(item, "int32 Item::GenerateItemRandomPropertyId("),
        "CREATE_BAG": body(source["Entities/Bag.cpp"], "bool Bag::Create("),
        "BAG_EMPTY": body(source["Entities/Bag.cpp"], "bool Bag::IsEmpty("),
        "BAG_ITEM": body(source["Entities/Bag.cpp"], "Item* Bag::GetItemByPos("),
        "LOOT_HOOK": body(source["AI/ScriptDevAI/ScriptDevAIMgr.cpp"], "bool ScriptDevAIMgr::OnItemLoot("),
    })
    for marker, signature in {
        "POS": "bool ItemPosCount::isContainedIn(",
        "COUNT": "uint32 Player::GetItemCount(",
        "UNIQUE": "InventoryResult Player::_CanTakeMoreSimilarItems(",
        "SLOTS": "InventoryResult Player::_CanStoreItem_InInventorySlots(",
        "BAG": "InventoryResult Player::_CanStoreItem_InBag(",
        "SPECIFIC": "InventoryResult Player::_CanStoreItem_InSpecificSlot(",
        "STORE": "InventoryResult Player::_CanStoreItem(",
        "STORE_ITEM": "Item* Player::StoreItem(",
        "STORE_ONE": "Item* Player::_StoreItem(",
        "ENCHANTMENT_DURATIONS": "void Player::AddEnchantmentDurations(",
        "ITEM_DURATIONS": "void Player::AddItemDurations(",
        "INVENTORY_POS": "bool Player::IsInventoryPos(",
        "EQUIPMENT_POS": "bool Player::IsEquipmentPos(",
        "BANK_POS": "bool Player::IsBankPos(",
        "BAG_POS": "bool Player::IsBagPos(",
        "VALID_POS": "bool Player::IsValidPos(",
        "REMOVE": "void Player::RemoveItem(",
        "OFFHAND": "void Player::AutoUnequipOffhandIfNeed(",
        "SWAP": "void Player::SwapItem(",
    }.items():
        pieces[marker] = body(player, signature)
    assert "#define MAX_BAG_SIZE 36" in source["Entities/Bag.h"]
    assert "uint8 count;" in pieces["POS_DECL"]
    assert "EQUIP_ERR_YOU_ARE_DEAD                       = 38" in pieces["RESULTS"]
    harness = Path(__file__).with_name("inventory_movement_oracle.cpp").read_text()
    for marker, piece in pieces.items():
        token = "// PINNED_" + marker + "\n"
        assert harness.count(token) == 1, marker
        harness = harness.replace(token, piece + "\n")
    assert "// PINNED_" not in harness
    return harness


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path, help="Pinned mangos-classic checkout root")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    harness = assemble(args.upstream)
    with tempfile.TemporaryDirectory(prefix="inventory-movement-") as directory:
        cpp, exe = Path(directory) / "oracle.cpp", Path(directory) / "oracle"
        cpp.write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O2", "-Wall", "-Wextra", "-Werror",
                        "-Wno-misleading-indentation", "-fsanitize=undefined",
                        "-fno-sanitize-recover=all", str(cpp), "-o", str(exe)], check=True)
        output = subprocess.check_output([str(exe)])
        def no_core_dump():
            resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        for dependency in ("clone", "script", "duration", "equipment", "bank", "bag", "offhand"):
            probe = subprocess.run([str(exe), "--unsupported-" + dependency],
                                   capture_output=True, preexec_fn=no_core_dump)
            assert probe.returncode == -signal.SIGABRT, dependency + " did not fail closed"
            if dependency == "offhand":
                assert b"unsupported-offhand: IsTwoHandUsed\n" in probe.stderr
    target = Path(__file__).parents[1] / "tests/fixtures/inventory_movement.tsv"
    if args.check:
        assert target.read_bytes() == output, "inventory_movement.tsv changed"
    else:
        target.write_bytes(output)
    print(f"inventory_movement.tsv: {len(output.splitlines())} exact source cases, SHA256 {hashlib.sha256(output).hexdigest()}")


if __name__ == "__main__":
    main()
