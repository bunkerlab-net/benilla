#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run pinned, unchanged player melee arithmetic over synthetic resolved operands.

This is not a runtime admission or DealMeleeDamage/callback oracle. The harness
supplies audited no-aura, one-physical-line mage/NPC state and explicit eligibility
flags. Every numeric method and the ordered die are extracted unchanged.
"""
import hashlib
import pathlib
import re
import subprocess
import sys
import tempfile

HASHES = {
    "Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "Unit.h": "2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1",
    "Util.h": "1f53eafa77ab89935c905f331d9ee7e5e2fabf1fca7f812e2e913f19243c5878",
}


def body(source, signature):
    start = source.index(signature)
    end = source.index("{", start) + 1
    depth = 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end] + "\n"


def main():
    sources = {}
    for name, digest in HASHES.items():
        raw = pathlib.Path(sys.argv[1], name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != digest:
            raise SystemExit("Wrong upstream revision: " + name)
        sources[name] = raw.decode()
    unit, header, util = (sources[n] for n in ("Unit.cpp", "Unit.h", "Util.h"))
    methods = "".join(body(unit, signature) for signature in (
        "float Unit::CalculateEffectiveMissChance(",
        "float Unit::CalculateEffectiveDodgeChance(",
        "float Unit::CalculateEffectiveParryChance(",
        "float Unit::CalculateEffectiveBlockChance(",
        "float Unit::CalculateEffectiveCritChance(",
        "float Unit::CalculateEffectiveCrushChance(",
        "float Unit::CalculateEffectiveGlanceChance(",
        "MeleeHitOutcome Unit::RollMeleeOutcomeAgainst(",
        "uint32 Unit::CalculateDamage(",
        "void Unit::CalculateMeleeDamage(",
        "uint32 Unit::CalculateCritAmount(CalcDamageInfo*",
        "uint32 Unit::CalculateGlanceAmount(",
        "float Unit::CalcArmorReducedDamage(",
    ))
    enums = (body(header, "enum MeleeHitOutcome") + ";\n"
             + body(header, "enum UnitCombatDieSide")
             + ";\n#define NUM_UNIT_COMBAT_DIE_SIDES (UNIT_COMBAT_DIE_HIT + 1)\n")
    die = (body(util, "inline uint32 chance_u(")
           + body(util, "template<class Side, Side Default, uint8 Sides>") + ";\n")
    names = sorted(set(re.findall(r"\b(?:HITINFO|VICTIMSTATE|PROC_FLAG|PROC_EX)_[A-Z0-9_]+", methods)))
    # Symbolic constants are used only for source branches/assignments, not
    # emitted as protocol proof. Use independent bit flags to preserve |/&.
    flags = ""
    for prefix in ("HITINFO_", "VICTIMSTATE_", "PROC_FLAG_", "PROC_EX_"):
        group = [name for name in names if name.startswith(prefix)]
        assert len(group) < 32
        flags += "enum : uint32 {" + ",".join(f"{name}=UINT32_C(1)<<{i}" for i, name in enumerate(group)) + "};\n"
    harness = pathlib.Path(__file__).with_name("player_melee_oracle.cpp").read_text()
    for key, value in {"ENUMS": enums, "DIE": die, "FLAGS": flags, "METHODS": methods}.items():
        marker = "// PINNED_" + key + "\n"
        assert harness.count(marker) == 1
        harness = harness.replace(marker, value)
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run([
            "g++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror",
            "-Wno-unused-parameter", "-Wno-unused-variable", "-Wno-unused-but-set-variable",
            "-fno-fast-math", "-ffp-contract=off", "-frounding-math",
            "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all",
            str(root / "oracle.cpp"), "-o", str(root / "oracle"),
        ], check=True)
        data = subprocess.check_output([str(root / "oracle")])
    # Both endpoints intentionally exercise the source, but identical complete
    # transcripts need only one stored row (singleton/no-third-draw cases).
    data = b"\n".join(dict.fromkeys(data.splitlines())) + b"\n"
    path = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures/player_melee_cmangos.tsv"
    path.write_bytes(data)
    print(len(data.splitlines()), hashlib.sha256(data).hexdigest())


if __name__ == "__main__":
    main()
