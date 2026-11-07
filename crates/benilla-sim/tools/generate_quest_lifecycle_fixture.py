#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Hash-check pinned CMaNGOS and compile original quest lifecycle method bodies.
No native rows/assets, network or Rust invocation. Generated probes are synthetic.
"""
import hashlib
import pathlib
import subprocess
import sys
import tempfile

HASHES = {
    "Player.h": "acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb",
    "ObjectMgr.cpp": "dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b",
    "Player.cpp": "d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e",
    "QuestDef.h": "a03821312ade7e0e30e620bf37d68e32899d65494e37887c9053eee50ebfa66a",
    "QuestDef.cpp": "e4f868e95421c2be5b90bd046178b6f4413dc5203e945cf5735b9bac26104cfb",
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
        path = pathlib.Path(sys.argv[2]) if name == "ObjectMgr.cpp" else pathlib.Path(sys.argv[1], name)
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise SystemExit("Wrong upstream source: " + name)
        source[name] = data.decode()
    methods = "".join(body(source["Player.cpp"], signature) for signature in [
        "bool Player::CanTakeQuest(", "bool Player::CanAddQuest(", "bool Player::CanCompleteQuest(",
        "bool Player::CanRewardQuest(Quest const* pQuest, bool", "bool Player::CanRewardQuest(Quest const* pQuest, uint32",
        "void Player::AddQuest(", "void Player::CompleteQuest(", "void Player::IncompleteQuest(", "void Player::RewardQuest(",
        "bool Player::SatisfyQuestLevel(", "bool Player::SatisfyQuestClass(", "bool Player::SatisfyQuestRace(",
        "bool Player::SatisfyQuestPreviousQuest(", "bool Player::SatisfyQuestNextChain(", "bool Player::SatisfyQuestPrevChain(",
        "bool Player::SatisfyQuestStatus(", "bool Player::SatisfyQuestLog(",
        "bool Player::IsCurrentQuest(", "void Player::SetQuestStatus(", "uint16 Player::FindQuestSlot(",
        "void Player::AdjustQuestReqItemCount(", "void Player::ItemAddedQuestCheck(",
        "void Player::ItemRemovedQuestCheck(", "void Player::KilledMonsterCredit(", "void Player::MoneyChanged(",
        "void Player::TalkedToCreature(",
    ])
    methods += "".join(body(source["QuestDef.cpp"], signature) for signature in [
        "uint32 Quest::XPValue(", "int32  Quest::GetRewOrReqMoney()", "uint32 Quest::GetRewMoneyMaxLevel()",
    ])
    harness = pathlib.Path(__file__).with_name("quest_lifecycle_oracle.cpp").read_text().replace("// PINNED_METHODS", methods)
    money = body(source["Player.h"], "void ModifyMoney(int32 d)")
    harness = harness.replace("// PINNED_MODIFY_MONEY", money.replace("ModifyMoney(", "ModifyMoneyNative(", 1))
    loader = source["ObjectMgr.cpp"]
    start = loader.index("        if (qinfo->NextQuestInChain)")
    end = loader.index("        if (qinfo->ExclusiveGroup)", start)
    harness = harness.replace("// PINNED_RELATION_LOADER", loader[start:end])
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        root.joinpath("oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-fno-fast-math", "-ffp-contract=off", "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all", str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        result = subprocess.check_output([str(root / "oracle")])
        result = b"\n".join(line.rstrip() for line in result.splitlines()) + b"\n"
        pathlib.Path(__file__).resolve().parents[1].joinpath("tests/fixtures/quest_lifecycle_cmangos.tsv").write_bytes(result)

if __name__ == "__main__":
    main()
