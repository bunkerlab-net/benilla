#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compile unchanged pinned loot algorithms on authored distribution outcomes.

CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, LootMgr.cpp:
Roll 374-391; LootItem 397-437; AddItem 930-941; GenerateMoneyLoot 1173-1185;
corpse constructor block 1711-1731; group/ungrouped processors 2397-2477,
2504-2544, 2606-2670. Util.h supplies the exact float/double comparison.
The adapter owns authored reference tables and substitutes only lookups, distribution
outcomes and shuffle permutations. Reference expansion is the extracted Process body. It
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

ORDINARY_SHA256 = "6c20dc8be219729e772c1e0bef001cb73d999467650a39b3487f04c7121fe429"
ORDINARY_CASES = 1536
REFERENCE_CASES = 1024


def extract_harness(upstream):
    """Validate every source pin, then insert complete unchanged source bodies."""
    source = {}
    for path, pin in PINS.items():
        data = (upstream / path).read_bytes()
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
    assert "// PINNED_" not in harness
    # Both reference branches must remain extracted source, not authored algorithms.
    assert "LootTemplates_Reference.GetLootFor(-Entrie.mincountOrRef)" in pieces["METHODS"]
    assert "Referenced->Process(loot, lootOwner, rate, lsData.get());" in pieces["METHODS"]
    assert "lRef->Process(loot, lootOwner, rate, nullptr);" in pieces["METHODS"]
    return harness


def validate_output(output, fields, cases):
    lines = output.splitlines()
    assert len(lines) == cases, "unexpected source case count"
    # Empty row/draw/item vectors use an explicit sentinel, including the final field.
    assert all(len(line.split(b"\t")) == fields and all(line.split(b"\t"))
               and line == line.rstrip() for line in lines)


def validate_references(output):
    validate_output(output, 9, REFERENCE_CASES)
    for sample, line in enumerate(output.splitlines()):
        columns = line.split(b"\t")
        assert int(columns[0]) == ORDINARY_CASES + sample
        assert len(columns[3].split(b":")) == 4
        rows = [row.split(b":") for row in columns[4].split(b";")]
        assert any(int(row[3]) < 0 for row in rows)
        refs = dict(part.split(b"=", 1) for part in columns[5].split(b"|"))
        for row in rows:
            if int(row[3]) < 0:
                assert row[2] == b"0" and str(-int(row[3])).encode() in refs
        for values in refs.values():
            assert all(int(row.split(b":")[3]) > 0 for row in values.split(b";"))
        draws = [] if columns[6] == b"-" else columns[6].split(b";")
        assert len(draws) <= 4096, "reference case exceeds runtime draw ceiling"
        if sample % 16 == 5:
            # Sixteen guaranteed root items consume the first sixteen count draws.
            # Source must still execute the later reference and root/group draws.
            assert len(columns[8].split(b";")) == 16
            assert len(draws) > 16
            assert any(draw.startswith(b"c:") for draw in draws[16:])
            assert any(draw.startswith(b"s:") for draw in draws[16:])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--verify-source-only", action="store_true",
                        help="Check pins/extraction and the frozen ordinary fixture without compiling")
    args = parser.parse_args()
    harness = extract_harness(args.upstream)
    fixtures = Path(__file__).parents[1] / "tests/fixtures"
    ordinary = (fixtures / "loot_cmangos.tsv").read_bytes()
    assert hashlib.sha256(ordinary).hexdigest() == ORDINARY_SHA256, "frozen ordinary fixture changed"
    validate_output(ordinary, 8, ORDINARY_CASES)
    if args.verify_source_only:
        print(f"source pins/extraction verified; frozen {ORDINARY_CASES} cases: {ORDINARY_SHA256}")
        return
    with tempfile.TemporaryDirectory() as directory:
        cpp, exe = Path(directory) / "oracle.cpp", Path(directory) / "oracle"
        cpp.write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O2", "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter",
                        "-fno-fast-math", "-ffp-contract=off", "-fsanitize=undefined,float-cast-overflow",
                        "-fno-sanitize-recover=all", str(cpp), "-o", str(exe)], check=True)
        old_output = subprocess.check_output([str(exe)])
        output = subprocess.check_output([str(exe), "--references"])
    # Always compare the old vectors and never rewrite them, even without --check.
    assert old_output == ordinary, "ordinary source cases changed"
    validate_references(output)
    target = fixtures / "loot_reference_cmangos.tsv"
    if args.check:
        assert target.read_bytes() == output, "reference loot fixture changed"
    else:
        target.write_bytes(output)
    print(f"loot_cmangos.tsv: {ORDINARY_CASES} unchanged source cases, SHA256 {ORDINARY_SHA256}")
    print(f"{target.name}: {REFERENCE_CASES} source cases, SHA256 {hashlib.sha256(output).hexdigest()}")

if __name__ == "__main__":
    main()
