#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compile unchanged pinned loot algorithms on authored distribution outcomes.

CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, LootMgr.cpp:
Roll 374-391; LootItem 397-437; AddItem 930-941; GenerateMoneyLoot 1173-1185;
corpse constructor block 1711-1731; group/ungrouped processors 2397-2477,
2504-2544, 2606-2670. Util.h supplies the exact float/double comparison.
The adapter substitutes only distribution outcomes and shuffle permutations. It
proves neither MT engine words nor std::shuffle implementation parity. No game
rows or assets are included. Source copyright CMaNGOS contributors,
GPL-2.0-or-later, without warranty; authored harness GPL-3.0-or-later.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
from generate_inventory_fixture import body

PINS = {
    "src/game/Loot/LootMgr.cpp": "7ffc34395011773310f944cfd7f1c59c23ae3043d0c8634d7665c0bbb5752cce",
    "src/game/Loot/LootMgr.h": "932f10d264919235639bb78ad726988f79f0ab4f3bfa3c144b885e6e40f17a20",
    "src/shared/Util/Util.h": "1f53eafa77ab89935c905f331d9ee7e5e2fabf1fca7f812e2e913f19243c5878",
    "src/shared/Util/Util.cpp": "f408c8a466a183cabdf96b0f7c18907b8cbb2144d73b0b2e8b81fdf421b43133",
}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = {}
    for path, pin in PINS.items():
        data = (args.upstream / path).read_bytes()
        assert hashlib.sha256(data).hexdigest() == pin, path
        source[path] = data.decode()
    loot = source["src/game/Loot/LootMgr.cpp"]
    pieces = {"METHODS": "\n\n".join(body(loot, signature) for signature in [
        "bool LootStoreItem::Roll(", "LootItem::LootItem(LootStoreItem const&",
        "void Loot::AddItem(LootStoreItem const&", "void Loot::GenerateMoneyLoot(",
        "void LootTemplate::LootGroup::AddEntry(", "LootStoreItem const* LootTemplate::LootGroup::Roll(",
        "void LootTemplate::LootGroup::Process(", "void LootTemplate::AddEntry(",
        "void LootTemplate::Process(",
    ]), "ROLL_CHANCE": body(source["src/shared/Util/Util.h"], "inline bool roll_chance_f(")}
    start = loot.index("            if ((creatureInfo->LootId && FillLoot(")
    pieces["CORPSE_BLOCK"] = body(loot[start:], "            if ((creatureInfo->LootId && FillLoot(")
    assert "#define MAX_NR_LOOT_ITEMS 16" in source["src/game/Loot/LootMgr.h"]
    harness = Path(__file__).with_name("loot_oracle.cpp").read_text()
    for marker, piece in pieces.items():
        token = "// PINNED_" + marker + "\n"
        assert harness.count(token) == 1, marker
        harness = harness.replace(token, piece + "\n")
    with tempfile.TemporaryDirectory() as directory:
        cpp, exe = Path(directory) / "oracle.cpp", Path(directory) / "oracle"
        cpp.write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O2", "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter",
                        "-fno-fast-math", "-ffp-contract=off", "-fsanitize=undefined,float-cast-overflow",
                        "-fno-sanitize-recover=all", str(cpp), "-o", str(exe)], check=True)
        output = subprocess.check_output([str(exe)])
    # Empty row/draw/item vectors use an explicit sentinel, including the final field.
    assert all(len(line.split(b"\t")) == 8 and all(line.split(b"\t"))
               and line == line.rstrip() for line in output.splitlines())
    target = Path(__file__).parents[1] / "tests/fixtures/loot_cmangos.tsv"
    if args.check:
        assert target.read_bytes() == output, "loot fixture changed"
    else:
        target.write_bytes(output)
    print(f"{target.name}: {len(output.splitlines())} source cases, SHA256 {hashlib.sha256(output).hexdigest()}")

if __name__ == "__main__":
    main()
