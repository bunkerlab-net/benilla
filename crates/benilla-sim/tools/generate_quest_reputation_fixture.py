#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Pinned quest reputation gates, callbacks and orchestration on synthetic rows.

Extracts unchanged Classic Player methods, including SatisfyQuestReputation and
ReputationChanged. The reputation host below applies authored signed deltas only:
this proves callback placement, not RewardReputation arithmetic/spillover parity.
No native game rows/assets or held content compiler inputs are read.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
from generate_quest_lifecycle_fixture import HASHES, body


def replace_once(text, old, new):
    assert text.count(old) == 1, old
    return text.replace(old, new, 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = {}
    for name, pin in HASHES.items():
        directory = "Globals" if name == "ObjectMgr.cpp" else "Quests" if name.startswith("QuestDef") else "Entities"
        raw = (args.upstream / "src/game" / directory / name).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == pin, name
        source[name] = raw.decode()
    harness = Path(__file__).with_name("quest_lifecycle_oracle.cpp").read_text().split("int main(")[0]
    replacements = [
        ("CONFIG_UINT32_MAX_PLAYER_LEVEL};", "CONFIG_UINT32_MAX_PLAYER_LEVEL,CONFIG_INT32_QUEST_HIGH_LEVEL_HIDE_DIFF};"),
        ("return key==CONFIG_FLOAT_RATE_XP_QUEST?", "return key==CONFIG_INT32_QUEST_HIGH_LEVEL_HIDE_DIFF?-1:key==CONFIG_FLOAT_RATE_XP_QUEST?"),
        ("struct FactionEntry{};", "struct FactionEntry{uint32 ID;};"),
        ("struct FactionStore {FactionEntry* LookupEntry(uint32){assert(false);return nullptr;}}sFactionStore;", "struct FactionStore {std::map<uint32,FactionEntry> rows;FactionEntry* LookupEntry(uint32 id){auto it=rows.find(id);return it==rows.end()?nullptr:&it->second;}}sFactionStore;"),
        ("struct Reputation {void SetVisible(const FactionEntry*){assert(false);}int GetReputation(uint32)const{assert(false);return 0;}};", "struct Reputation {std::map<uint32,int32> standing;void SetVisible(const FactionEntry*f){emit(\"visible:\"+std::to_string(f->ID));}int32 GetReputation(uint32 id)const{return standing.at(id);}int32 GetReputation(const FactionEntry*f)const{return GetReputation(f->ID);}};"),
        ("using PrevQuests=std::vector<int32>;", "uint32 rep_objective=0,rep_min=0,rep_max=0;int32 objective_value=0,min_value=0,max_value=0;uint32 reward_faction[5]={};int32 reward_value[5]={};\n using PrevQuests=std::vector<int32>;"),
        ("uint32 GetRepObjectiveFaction()const{return 0;}int32 GetRepObjectiveValue()const{return 0;}", "uint32 GetRepObjectiveFaction()const{return rep_objective;}int32 GetRepObjectiveValue()const{return objective_value;}uint32 GetRequiredMinRepFaction()const{return rep_min;}int32 GetRequiredMinRepValue()const{return min_value;}uint32 GetRequiredMaxRepFaction()const{return rep_max;}int32 GetRequiredMaxRepValue()const{return max_value;}"),
        ("QuestStatusMap mQuestStatus;", "mutable Reputation reputation;QuestStatusMap mQuestStatus;"),
        ("Reputation&GetReputationMgr()const{static Reputation r;return r;}", "Reputation&GetReputationMgr()const{return reputation;}"),
        ("bool SatisfyQuestReputation(const Quest*,bool)const{return true;}", "bool SatisfyQuestReputation(const Quest*,bool)const;"),
        ("void RewardReputation(const Quest*q){emit(\"rep:\"+std::to_string(q->QuestId));}", "void ReputationChanged(const FactionEntry*);void RewardReputation(const Quest*q){for(int i=0;i<5;++i)if(q->reward_faction[i]){reputation.standing.at(q->reward_faction[i])+=q->reward_value[i];ReputationChanged(sFactionStore.LookupEntry(q->reward_faction[i]));}emit(\"rep:\"+std::to_string(q->QuestId));}"),
        ("bool CanTakeQuest(const Quest*,bool)const;", "bool CanSeeStartQuest(const Quest*)const;bool CanTakeQuest(const Quest*,bool)const;"),
    ]
    for old, new in replacements:
        harness = replace_once(harness, old, new)
    signatures = [
        "bool Player::CanSeeStartQuest(", "bool Player::CanTakeQuest(", "bool Player::CanAddQuest(", "bool Player::CanCompleteQuest(",
        "bool Player::CanRewardQuest(Quest const* pQuest, bool", "bool Player::CanRewardQuest(Quest const* pQuest, uint32",
        "void Player::AddQuest(", "void Player::CompleteQuest(", "void Player::IncompleteQuest(", "void Player::RewardQuest(",
        "bool Player::SatisfyQuestLevel(", "bool Player::SatisfyQuestClass(", "bool Player::SatisfyQuestRace(",
        "bool Player::SatisfyQuestPreviousQuest(", "bool Player::SatisfyQuestNextChain(", "bool Player::SatisfyQuestPrevChain(",
        "bool Player::SatisfyQuestStatus(", "bool Player::SatisfyQuestLog(", "bool Player::SatisfyQuestReputation(",
        "bool Player::IsCurrentQuest(", "void Player::SetQuestStatus(", "uint16 Player::FindQuestSlot(",
        "void Player::AdjustQuestReqItemCount(", "void Player::ItemAddedQuestCheck(", "void Player::ItemRemovedQuestCheck(",
        "void Player::KilledMonsterCredit(", "void Player::MoneyChanged(", "void Player::TalkedToCreature(", "void Player::ReputationChanged(",
    ]
    methods = "".join(body(source["Player.cpp"], sig) for sig in signatures)
    methods += "".join(body(source["QuestDef.cpp"], sig) for sig in ["uint32 Quest::XPValue(", "int32  Quest::GetRewOrReqMoney()", "uint32 Quest::GetRewMoneyMaxLevel()"])
    harness = replace_once(harness, "// PINNED_METHODS", methods)
    harness = replace_once(harness, "// PINNED_MODIFY_MONEY", body(source["Player.h"], "void ModifyMoney(int32 d)").replace("ModifyMoney(", "ModifyMoneyNative(", 1))
    loader = source["ObjectMgr.cpp"]
    start = loader.index("        if (qinfo->NextQuestInChain)")
    harness = replace_once(harness, "// PINNED_RELATION_LOADER", loader[start:loader.index("        if (qinfo->ExclusiveGroup)", start)])
    harness += Path(__file__).with_name("quest_reputation_oracle.cpp").read_text()
    with tempfile.TemporaryDirectory() as temp:
        cpp, exe = Path(temp) / "oracle.cpp", Path(temp) / "oracle"
        cpp.write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O2", "-fno-fast-math", "-ffp-contract=off", "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all", str(cpp), "-o", str(exe)], check=True)
        output = subprocess.check_output([str(exe)])
    target = Path(__file__).parents[1] / "tests/fixtures/quest_reputation_cmangos.tsv"
    if args.check:
        assert target.read_bytes() == output, "quest reputation fixture changed"
    else:
        target.write_bytes(output)
    print(f"{target.name}: {len(output.splitlines())} source probes, SHA256 {hashlib.sha256(output).hexdigest()}")


if __name__ == "__main__":
    main()
