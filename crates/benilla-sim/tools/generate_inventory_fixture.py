#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Exact pinned-source backpack and positive .additem oracle, synthetic rows only."""
import argparse
import hashlib
from pathlib import Path
import resource
import signal
import subprocess
import tempfile

PINS = {
    "Entities/Player.cpp": "d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e",
    "Entities/Player.h": "acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb",
    "Entities/Item.cpp": "5e401746527531beb521d3eeaa6bc0b55fceaa0d4aef1e780d4f429a545f72e4",
    "Entities/Item.h": "a884fd9b01da289e3fd556979a6070091f4588400c29b84851d5e88e193bf414",
    "Entities/ItemPrototype.h": "64768cb942d73c17ce00fe76f953a3d7ca3e5e617a5f4c798cb5d259bf931756",
    "Globals/ObjectMgr.cpp": "dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b",
    "Entities/Bag.cpp": "d25b146210b4d13e880be6972cdc574fd72e91eeab5580870f4ee583fcaf6e24",
    "Entities/Bag.h": "936351212bfa52621d36eb3d1f38e494fc0b4ea3a3e7740fb344c3fb252a6c15",
    "Chat/Level3.cpp": "2dc12faad03e5b027c4e41566aa7b88f4109c75a8111e14bcecee35633d09bf6",
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

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path, help="Pinned mangos-classic checkout root")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = {}
    for path, pin in PINS.items():
        data = (args.upstream / "src/game" / path).read_bytes()
        assert hashlib.sha256(data).hexdigest() == pin, path
        source[path] = data.decode()
    player, item = source["Entities/Player.cpp"], source["Entities/Item.cpp"]
    pieces = {
        "SPELL_DECL": body(source["Entities/ItemPrototype.h"], "struct _Spell\n") + ";",
        "BAG_FAMILY": body(source["Entities/ItemPrototype.h"], "enum BagFamily\n") + ";",
        "INVENTORY_TYPE": body(source["Entities/ItemPrototype.h"], "enum InventoryType\n") + ";",
        "ENCHANTMENT_SLOT": body(source["Entities/Item.h"], "enum EnchantmentSlot\n") + ";",
        "RESULTS": body(source["Entities/Item.h"], "enum InventoryResult\n") + ";",
        "POS_DECL": body(source["Entities/Player.h"], "struct ItemPosCount\n")
            + ";\ntypedef std::vector<ItemPosCount> ItemPosCountVec;",
        "POS": body(player, "bool ItemPosCount::isContainedIn("),
        "MERGE": body(item, "InventoryResult Item::CanBeMergedPartlyWith("),
        "UNIQUE": body(player, "InventoryResult Player::_CanTakeMoreSimilarItems("),
        "SLOTS": body(player, "InventoryResult Player::_CanStoreItem_InInventorySlots("),
        "BAG": body(player, "InventoryResult Player::_CanStoreItem_InBag("),
        "STORE": body(player, "InventoryResult Player::_CanStoreItem("),
        "CREATE": body(item, "bool Item::Create("),
        "CREATE_BAG": body(source["Entities/Bag.cpp"], "bool Bag::Create("),
        "NEW_ITEM_OR_BAG": body(source["Entities/Bag.h"], "inline Item* NewItemOrBag("),
        "CAN_STORE_NEW": body(source["Entities/Player.h"], "InventoryResult CanStoreNewItem("),
        "RANDOM_PROPERTY": body(item, "int32 Item::GenerateItemRandomPropertyId("),
        "CREATE_ITEM": body(item, "Item* Item::CreateItem("),
        "CLONE": body(item, "Item* Item::CloneItem("),
        "STORE_NEW": body(player, "Item* Player::StoreNewItem("),
        "STORE_ITEM": body(player, "Item* Player::StoreItem("),
        "STORE_ONE": body(player, "Item* Player::_StoreItem("),
        "ENCHANTMENT_DURATIONS": body(player, "void Player::AddEnchantmentDurations("),
        "ITEM_DURATIONS": body(player, "void Player::AddItemDurations("),
        "LOOT_HOOK": body(source["AI/ScriptDevAI/ScriptDevAIMgr.cpp"], "bool ScriptDevAIMgr::OnItemLoot("),
    }
    # The parser, database-name lookup, and negative-count removal branch are
    # outside this oracle. The complete positive branch is inserted unchanged.
    add_item = body(source["Chat/Level3.cpp"], "bool ChatHandler::HandleAddItemCommand(")
    positive = add_item[add_item.index("    // Adding items\n"):]
    pieces["GRANT"] = (
        "bool ChatHandler::GrantItem(Player* pl, Player* plTarget, uint32 itemId, int32 count)\n{\n"
        + positive
    )
    assert "#define MAX_BAG_SIZE 36" in source["Entities/Bag.h"]
    assert "uint8 count;" in pieces["POS_DECL"]
    loader = source["Globals/ObjectMgr.cpp"]
    assert "else if (proto->Stackable > 255)" in loader
    assert "const_cast<ItemPrototype*>(proto)->Stackable = 255;" in loader
    harness = Path(__file__).with_name("inventory_oracle.cpp").read_text()
    for marker, piece in pieces.items():
        token = "// PINNED_" + marker + "\n"
        assert harness.count(token) == 1, marker
        harness = harness.replace(token, piece + "\n")
    with tempfile.TemporaryDirectory() as directory:
        cpp, exe = Path(directory) / "oracle.cpp", Path(directory) / "oracle"
        cpp.write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O2", "-Wall", "-Wextra", "-Werror",
                        "-Wno-misleading-indentation", "-fsanitize=undefined",
                        "-fno-sanitize-recover=all", str(cpp), "-o", str(exe)], check=True)
        outputs = {"inventory.tsv": subprocess.check_output([str(exe)]),
                   "inventory_store.tsv": subprocess.check_output([str(exe), "--store"]),
                   "inventory_grant.tsv": subprocess.check_output([str(exe), "--grant"]),
                   "inventory_create.tsv": subprocess.check_output([str(exe), "--create"])}
        # Unsupported dependent behavior must reach an aborting boundary, rather
        # than silently produce an invented successful no-op oracle result.
        def no_core_dump():
            resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        for dependency in ("random", "script", "duration", "keyring"):
            probe = subprocess.run([str(exe), "--unsupported-" + dependency],
                                   capture_output=True, preexec_fn=no_core_dump)
            assert probe.returncode == -signal.SIGABRT, dependency + " did not fail closed"
    for filename, output in outputs.items():
        target = Path(__file__).parents[1] / "tests/fixtures" / filename
        if args.check:
            assert target.read_bytes() == output, filename + " changed"
        else:
            target.write_bytes(output)
        print(f"{filename}: {len(output.splitlines())} exact source cases, SHA256 {hashlib.sha256(output).hexdigest()}")

if __name__ == "__main__":
    main()
