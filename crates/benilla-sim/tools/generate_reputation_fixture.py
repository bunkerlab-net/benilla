#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Pinned CMaNGOS reputation bodies, authored rows and strict-float C++ oracle."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
from generate_inventory_fixture import body
PINS = {
    "Reputation/ReputationMgr.cpp": "87094877ab19cfa6e3b297b6d68afeedf5e9f312efd84e09be0293879796f63b",
    "Reputation/ReputationMgr.h": "76df8b8b91e791aff896c8a2fa5ca62395eb5764f0c40b056a987a5996f5f60f",
    "Server/DBCStructure.h": "354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50",
    "Globals/SharedDefines.h": "4832b8e0da45211143acae7c42173fea42732e2acdbf190692725e51b020bfe8",
    "Globals/ObjectMgr.h": "27648698cb0ed86a6354cf737895ccd3752de1847f89ee70b49260954d903229",
    "Entities/Player.cpp": "d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e",
    "Entities/Player.h": "acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb",
    "Tools/Formulas.h": "ca8b55fd022b0c0191255bc457574ccafce5eeb2f99a447466ba75be4717652c",
}
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream",type=Path);parser.add_argument("--check",action="store_true")
    args=parser.parse_args();source={}
    for file,pin in PINS.items():
        raw=(args.upstream/"src/game"/file).read_bytes()
        assert pin and hashlib.sha256(raw).hexdigest()==pin,file
        source[file]=raw.decode()
    cpp=source["Reputation/ReputationMgr.cpp"]
    methods=["ReputationRank ReputationMgr::ReputationToRank(","FactionState const* ReputationMgr::GetState(FactionEntry const*", "FactionState const* ReputationMgr::GetState(RepListID", "int32 ReputationMgr::GetReputation(FactionEntry const*", "int32 ReputationMgr::GetBaseReputation(","bool ReputationMgr::IsAtWar(FactionEntry const*", "ReputationRank ReputationMgr::GetRank(","ReputationRank ReputationMgr::GetBaseRank(", "uint32 ReputationMgr::GetDefaultStateFlags(", "void ReputationMgr::Initialize(","bool ReputationMgr::SetReputation(","bool ReputationMgr::SetOneFactionReputation(","void ReputationMgr::SetVisible(FactionEntry const*", "void ReputationMgr::SetVisible(FactionState*", "void ReputationMgr::SetAtWar(RepListID", "void ReputationMgr::SetAtWar(FactionState*", "void ReputationMgr::SetInactive(RepListID", "void ReputationMgr::SetInactive(FactionState*", "void ReputationMgr::SendState(","void ReputationMgr::SendInitialReputations(", "void ReputationMgr::SendVisible("]
    decls=[("Globals/SharedDefines.h","enum ReputationRank\n"),("Reputation/ReputationMgr.h","enum FactionFlags\n"),("Reputation/ReputationMgr.h","struct FactionState\n"),("Server/DBCStructure.h","struct FactionEntry\n"),("Globals/ObjectMgr.h","struct RepRewardRate\n"),("Globals/ObjectMgr.h","struct RepSpilloverTemplate\n"),("Entities/Player.h","enum ReputationSource\n")]
    constants=cpp[cpp.index("const int32 ReputationMgr::PointsInRank"):cpp.index("ReputationRank ReputationMgr::ReputationToRank")]
    pieces={"DECLS":"\n".join(body(source[f],s)+";" for f,s in decls),"METHODS":constants+"\n"+"\n".join(body(cpp,s) for s in methods)+"\n"+body(source["Entities/Player.cpp"],"int32 Player::CalculateReputationGain(")+"\n"+body(source["Entities/Player.cpp"],"void Player::RewardReputation(Quest const*"),"XP":"\n".join(body(source["Tools/Formulas.h"],s) for s in ["inline bool IsTrivialLevelDifference(","inline uint32 GetQuestGreenRange("])}
    reward=body(source["Entities/Player.cpp"],"void Player::RewardQuest(")
    assert reward.index("RewardReputation(pQuest);")<reward.index("SetQuestSlot(log_slot, 0);")<reward.index("GiveXP(xp, nullptr);")
    harness=Path(__file__).with_name("reputation_oracle.cpp").read_text()
    for marker,piece in pieces.items():
        token="// PINNED_"+marker+"\n";assert harness.count(token)==1;harness=harness.replace(token,piece+"\n")
    with tempfile.TemporaryDirectory() as temp:
        cpp,exe=Path(temp)/"oracle.cpp",Path(temp)/"oracle";cpp.write_text(harness)
        subprocess.run(["g++","-std=c++17","-O2","-Wall","-Wextra","-Werror","-fno-fast-math","-ffp-contract=off","-fsanitize=undefined,float-cast-overflow","-fno-sanitize-recover=all",str(cpp),"-o",str(exe)],check=True)
        outputs={"reputation.tsv":subprocess.check_output([str(exe)]),"reputation_gain.tsv":subprocess.check_output([str(exe),"--gain"]),"reputation_host.tsv":subprocess.check_output([str(exe),"--host"])}
    for name,output in outputs.items():
        path=Path(__file__).parents[1]/"tests/fixtures"/name
        if args.check: assert path.read_bytes()==output,name
        else:path.write_bytes(output)
        print(f"{name}: {len(output.splitlines())} source cases, SHA256 {hashlib.sha256(output).hexdigest()}")
if __name__=="__main__":main()
