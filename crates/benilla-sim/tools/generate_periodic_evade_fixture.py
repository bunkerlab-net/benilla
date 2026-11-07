#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Exact source damage-mod/periodic-log accounting, with authored values only."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
from generate_inventory_fixture import body

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream",type=Path)
    parser.add_argument("--check",action="store_true")
    args=parser.parse_args()
    unit=(args.upstream/"src/game/Entities/Unit.cpp").read_bytes()
    aura=(args.upstream/"src/game/Spells/SpellAuras.cpp").read_bytes()
    assert hashlib.sha256(unit).hexdigest()=="03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0"
    assert hashlib.sha256(aura).hexdigest()=="5e60510d23b789f8f0fb584da8f05a5be675f1cb4024ce997c989cde00105fde"
    unit=unit.decode();aura=aura.decode()
    start=aura.index("            const uint32 bonus =",aura.index("case SPELL_AURA_PERIODIC_DAMAGE:",aura.index("void Aura::PeriodicTick")))
    end=aura.index("            SpellPeriodicAuraLogInfo",start)
    assert aura.rfind("Unit::DealDamageMods",0,start)>aura.rfind("CalculateDamageAbsorbAndResist",0,start)
    deal=body(unit,"uint32 Unit::DealDamage(")
    assert deal.index("if (!damage)") < deal.index("SetLootRecipient") < deal.index("HandleDamageDealt")
    damage=body(unit,"void Unit::HandleDamageDealt(")
    assert damage.index("ModifyHealth") < damage.index("AddThreat")
    harness=Path(__file__).with_name("periodic_evade_oracle.cpp").read_text().replace("// PINNED_MODS\n",body(unit,"void Unit::DealDamageMods(")+"\n").replace("// PINNED_MITIGATION\n",aura[start:end])
    with tempfile.TemporaryDirectory() as temp:
        cpp,exe=Path(temp)/"oracle.cpp",Path(temp)/"oracle"
        cpp.write_text(harness)
        subprocess.run(["g++","-std=c++17","-O2","-Wall","-Wextra","-Werror","-fsanitize=undefined","-fno-sanitize-recover=all",str(cpp),"-o",str(exe)],check=True)
        output=subprocess.check_output([str(exe)])
    target=Path(__file__).parents[1]/"tests/fixtures/periodic_evade.tsv"
    if args.check: assert target.read_bytes()==output
    else: target.write_bytes(output)
    print(f"{len(output.splitlines())} source-extracted cases; SHA256 {hashlib.sha256(output).hexdigest()}")
if __name__=="__main__":main()
