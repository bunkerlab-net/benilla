#!/usr/bin/env python3
"""Compile exact pinned CMaNGOS quest bodies and an unchanged RewardQuest fragment.

Usage: python3 tools/generate_quest_fixture.py /path/to/pinned/sources
Directory must contain QuestDef.cpp, QuestDef.h and Player.cpp. No network,
world rows, client assets, database or runtime credentials are used.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile

HASHES = {
    "QuestDef.cpp": "e4f868e95421c2be5b90bd046178b6f4413dc5203e945cf5735b9bac26104cfb",
    "QuestDef.h": "a03821312ade7e0e30e620bf37d68e32899d65494e37887c9053eee50ebfa66a",
    "Player.cpp": "d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e",
}


def body(text, signature):
    start = text.index(signature)
    opening = text.index("{", start)
    depth, end = 1, opening + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end] + "\n"


def main():
    source = {}
    for name, digest in HASHES.items():
        data = pathlib.Path(sys.argv[1], name).read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise SystemExit("Wrong upstream revision: " + name)
        source[name] = data.decode()
    methods = "".join(body(source["QuestDef.cpp"], signature) for signature in [
        "uint32 Quest::XPValue(", "int32  Quest::GetRewOrReqMoney()",
        "uint32 Quest::GetRewMoneyMaxLevel()",
    ])
    methods += "".join(body(source["Player.cpp"], signature) for signature in [
        "bool Player::SatisfyQuestLevel(", "bool Player::SatisfyQuestClass(",
        "bool Player::SatisfyQuestRace(",
    ])
    reward = body(source["Player.cpp"], "void Player::RewardQuest(")
    fragment = reward[reward.index("    uint32 xp ="):reward.index("    // Send reward mail")]
    harness = pathlib.Path(__file__).with_name("quest_oracle.cpp").read_text()
    harness = harness.replace("// PINNED_QUEST_METHODS", methods)
    harness = harness.replace("// PINNED_REWARD_FRAGMENT", fragment)
    target = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures"
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        root.joinpath("oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-fno-fast-math", "-ffp-contract=off",
                        "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all",
                        str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        for mode in ["xp", "money", "reward", "gates"]:
            target.joinpath(f"quest_{mode}_cmangos.tsv").write_bytes(
                subprocess.check_output([str(root / "oracle"), mode]))


if __name__ == "__main__":
    main()
