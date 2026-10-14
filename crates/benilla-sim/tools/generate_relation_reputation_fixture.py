#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Exact pinned relationship/reputation bodies over authored, asset-free inputs."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
from generate_target_fixture import body

PINS = {
    "Entities/Relations.cpp": "06e453c80b33cddf1606004055a5d1407a46007b20fef73c5dc917156256f77b",
    "Entities/Unit.h": "2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1",
    "Entities/Creature.cpp": "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
    "Reputation/ReputationMgr.cpp": "87094877ab19cfa6e3b297b6d68afeedf5e9f312efd84e09be0293879796f63b",
    "Reputation/ReputationMgr.h": "76df8b8b91e791aff896c8a2fa5ca62395eb5764f0c40b056a987a5996f5f60f",
    "Server/DBCStructure.h": "354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50",
    "Globals/SharedDefines.h": "4832b8e0da45211143acae7c42173fea42732e2acdbf190692725e51b020bfe8",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = {}
    for file, pin in PINS.items():
        data = (args.upstream / "src/game" / file).read_bytes()
        if hashlib.sha256(data).hexdigest() != pin:
            raise SystemExit("Wrong upstream revision: " + file)
        source[file] = data.decode()
    declarations = [
        ("Globals/SharedDefines.h", "enum ReputationRank\n"),
        ("Entities/Unit.h", "enum UnitFlags\n"),
        ("Entities/Unit.h", "enum UnitState\n"),
        ("Reputation/ReputationMgr.h", "enum FactionFlags\n"),
        ("Reputation/ReputationMgr.h", "struct FactionState\n"),
        ("Server/DBCStructure.h", "struct FactionEntry\n"),
    ]
    relations = [
        "static ReputationRank GetFactionReaction(FactionTemplateEntry const* thisTemplate, Unit const* unit)",
        "ReputationRank Unit::GetReactionTo(Unit const* unit) const",
        "bool Unit::IsEnemy(Unit const* unit) const",
        "bool Unit::IsFriend(Unit const* unit) const",
        "bool Unit::CanAttack(const Unit* unit) const",
        "bool Unit::CanAttackNow(const Unit* unit) const",
        "bool Unit::CanAttackOnSight(Unit const* target) const",
        "bool Unit::CanAttackServerside(Unit const* unit, bool ignoreFlagsSource, bool ignoreFlagsTarget) const",
        "bool Unit::CanAttackInCombat(Unit const* target, bool ignoreFlagsSource, bool ignoreFlagsTarget) const",
    ]
    reputation = [
        "ReputationRank ReputationMgr::ReputationToRank(",
        "FactionState const* ReputationMgr::GetState(FactionEntry const*",
        "FactionState const* ReputationMgr::GetState(RepListID",
        "int32 ReputationMgr::GetReputation(FactionEntry const*",
        "int32 ReputationMgr::GetBaseReputation(",
        "bool ReputationMgr::IsAtWar(FactionEntry const*",
        "ReputationRank ReputationMgr::GetRank(",
        "uint32 ReputationMgr::GetDefaultStateFlags(",
        "void ReputationMgr::Initialize(",
    ]
    rep = source["Reputation/ReputationMgr.cpp"]
    constants = rep[rep.index("const int32 ReputationMgr::PointsInRank"):rep.index("ReputationRank ReputationMgr::ReputationToRank")]
    pieces = {
        "DECLARATIONS": "\n".join(body(source[file], signature) + ";" for file, signature in declarations),
        "COMPARATOR": body(source["Entities/Relations.cpp"], "static inline ReputationRank GetFactionReaction(FactionTemplateEntry const* thisTemplate, FactionTemplateEntry const* otherTemplate)"),
        "COMBAT_ENTRY": body(source["Entities/Unit.h"], "bool CanEnterCombat() const"),
        "METHODS": constants + "\n".join(body(rep, signature) for signature in reputation)
        + "\n".join(body(source["Entities/Relations.cpp"], signature) for signature in relations)
        + body(source["Entities/Creature.cpp"], "bool Creature::CanInitiateAttack() const"),
    }
    harness = Path(__file__).with_name("relation_reputation_oracle.cpp").read_text()
    for marker, piece in pieces.items():
        token = "// PINNED_" + marker + "\n"
        assert harness.count(token) == 1, marker
        harness = harness.replace(token, piece + "\n")
    with tempfile.TemporaryDirectory() as temp:
        cpp, exe = Path(temp) / "oracle.cpp", Path(temp) / "oracle"
        cpp.write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O2", "-Wall", "-Wextra", "-Werror", "-fsanitize=undefined", "-fno-sanitize-recover=all", str(cpp), "-o", str(exe)], check=True)
        for mode in ["relations", "lifecycle"]:
            data = subprocess.check_output([str(exe), mode])
            count, digest = len(data.splitlines()), hashlib.sha256(data).hexdigest()
            result = f"{count} {digest}\n".encode()
            path = Path(__file__).parents[1] / "tests/fixtures" / f"{mode}_reputation_cmangos.sha256"
            if args.check:
                assert path.read_bytes() == result, mode
            else:
                path.write_bytes(result)
            print(mode, count, digest)


if __name__ == "__main__":
    main()
