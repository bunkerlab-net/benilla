#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compile exact pinned ordinary Creature regeneration and Unit::ModifyHealth methods."""
import argparse
import hashlib
from pathlib import Path
import re
import subprocess
import tempfile
from generate_inventory_fixture import body

HASHES = {
    "src/game/Entities/Creature.cpp": "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
    "src/game/Entities/Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "src/game/Entities/Creature.h": "a92b0e95e5bf7827389fcca0acc0edaba48a1a083efbf5a01a752a4d171664fc",
    "src/game/Entities/Unit.h": "2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1",
    "src/game/Entities/CreatureDefines.h": "f5c824f4197960e9004e6631cff67ad0f391fee58fa97187cad4f264893b4c5b",
}

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream",type=Path)
    parser.add_argument("--check",action="store_true")
    args=parser.parse_args()
    source={}
    for name,digest in HASHES.items():
        data=(args.upstream/name).read_bytes()
        assert hashlib.sha256(data).hexdigest()==digest, name
        source[Path(name).name]=data.decode()
    c,u,ch,uh,cd=[source[n] for n in ["Creature.cpp","Unit.cpp","Creature.h","Unit.h","CreatureDefines.h"]]
    # Constructor is the only assignment outside the two class-specific regen paths.
    assert "m_regenTimer(0)" in u and u.count("m_regenTimer") == 1
    assert c.count("m_regenTimer") == 3 and uh.count("m_regenTimer") == 1
    assert "uint32 addvalue = IsPlayerControlled() ? maxValue * 0.13 : maxValue / 3;" in c
    update=body(c,"void Creature::Update(")
    alive=update[update.index("case ALIVE:"):]
    assert alive.index("Unit::Update(diff)") < alive.index("if (!IsAlive())") < alive.index("RegenerateAll(diff)")
    assert "RegenerateAll" not in update[:update.index("case ALIVE:")]
    unit=body(u,"void Unit::Update(")
    assert unit.index("m_events.Update(diff)") < unit.index("_UpdateSpells(diff)") < unit.index("i_motionMaster.UpdateMotion(diff)")
    assert unit.index("i_motionMaster.UpdateMotion(diff)") < unit.index("AI()->UpdateAI(diff)") < unit.index("GetCombatManager().Update(diff)")
    methods="\n".join([body(c,"void Creature::RegenerateAll("),body(c,"void Creature::RegenerateHealth("),body(u,"int32 Unit::ModifyHealth(")])
    interval=re.search(r"#define REGEN_TIME_FULL_UNIT\s+\d+",uh).group(0)
    health=re.search(r"REGEN_FLAG_HEALTH\s*=\s*0x[0-9A-Fa-f]+",ch).group(0)
    control=re.search(r"UNIT_FLAG_PLAYER_CONTROLLED\s*=\s*0x[0-9A-Fa-f]+",uh).group(0)
    interaction=re.search(r"ALLOW_INTERACTION_WHILE_IN_COMBAT\s*=\s*0x[0-9A-Fa-f]+",cd).group(0)
    constants=f"{interval}\nconstexpr uint32 {health};\nconstexpr uint32 {control};\nconstexpr uint32 UNIT_FIELD_FLAGS=0;\nnamespace CreatureTypeFlags {{constexpr uint32 {interaction};}}"
    harness=Path(__file__).with_name("creature_regeneration_oracle.cpp").read_text()
    for marker,value in [("CONSTANTS",constants),("METHODS",methods),("PREDICATE",body(ch,"bool IsRegeneratingHealth() const")),("CONTROL",body(uh,"bool IsPlayerControlled() const"))]:
        harness=harness.replace("// PINNED_"+marker+"\n",value+"\n")
    with tempfile.TemporaryDirectory() as temp:
        cpp,exe=Path(temp)/"oracle.cpp",Path(temp)/"oracle"
        cpp.write_text(harness)
        subprocess.run(["g++","-std=c++17","-O2","-Wall","-Wextra","-Werror","-fno-fast-math","-ffp-contract=off","-fsanitize=undefined,float-cast-overflow","-fno-sanitize-recover=all",str(cpp),"-o",str(exe)],check=True)
        output=subprocess.check_output([str(exe)])
    target=Path(__file__).parents[1]/"tests/fixtures/creature_regeneration.tsv"
    if args.check:
        assert target.read_bytes()==output,"creature regeneration fixture changed"
    else:
        target.write_bytes(output)
    print(f"{len(output.splitlines())} source-extracted cases; SHA256 {hashlib.sha256(output).hexdigest()}")

if __name__=="__main__":
    main()
