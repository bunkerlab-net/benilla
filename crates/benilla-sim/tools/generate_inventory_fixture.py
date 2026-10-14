#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Exact pinned-source backpack planning and creation oracle, synthetic rows only."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

PINS = {
    "Entities/Player.cpp": "d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e",
    "Entities/Player.h": "acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb",
    "Entities/Item.cpp": "5e401746527531beb521d3eeaa6bc0b55fceaa0d4aef1e780d4f429a545f72e4",
    "Entities/Item.h": "a884fd9b01da289e3fd556979a6070091f4588400c29b84851d5e88e193bf414",
    "Entities/ItemPrototype.h": "64768cb942d73c17ce00fe76f953a3d7ca3e5e617a5f4c798cb5d259bf931756",
    "Globals/ObjectMgr.cpp": "dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b",
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
        "CREATE_ITEM": body(item, "Item* Item::CreateItem("),
        "CLONE": body(item, "Item* Item::CloneItem("),
        "STORE_NEW": body(player, "Item* Player::StoreNewItem("),
        "STORE_ITEM": body(player, "Item* Player::StoreItem("),
        "STORE_ONE": body(player, "Item* Player::_StoreItem("),
    }
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
                   "inventory_store.tsv": subprocess.check_output([str(exe), "--store"])}
    for filename, output in outputs.items():
        target = Path(__file__).parents[1] / "tests/fixtures" / filename
        if args.check:
            assert target.read_bytes() == output, filename + " changed"
        else:
            target.write_bytes(output)
        print(f"{filename}: {len(output.splitlines())} exact source cases, SHA256 {hashlib.sha256(output).hexdigest()}")

if __name__ == "__main__":
    main()
