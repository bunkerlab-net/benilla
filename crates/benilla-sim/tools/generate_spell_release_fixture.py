#!/usr/bin/env python3
"""Regenerate bounded prepare/release probes from exact pinned CMaNGOS source.

Accepts either the pinned checkout root or a directory of flattened source files.
Source statement blocks remain unchanged; the phase orchestration is authored,
not an execution of the full Spell::cast/CheckCast engine. No native content rows.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile

from generate_spell_fixture import HASHES, body

RELEASE_HASHES = {name: HASHES[name] for name in ("Spell.cpp", "SpellMgr.cpp", "Object.cpp")}
RELEASE_HASHES["Spell.h"] = "62ee5ad8a5d5c3dab9a75923daadcf01b59c86ae8bce82752db3b9d3db694dee"


def main():
    root = pathlib.Path(sys.argv[1])
    source = {}
    for name, digest in RELEASE_HASHES.items():
        files = list(root.rglob(name))
        if len(files) != 1:
            raise SystemExit("Expected one pinned source file: " + name)
        data = files[0].read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise SystemExit("Wrong upstream revision: " + name)
        source[name] = data.decode()
    effect = body(source["Object.cpp"], "int32 WorldObject::CalculateSpellEffectValue(")
    effect = effect[effect.index("    int32 baseDice ="):effect.index("    // random damage")]
    cost = body(source["Spell.cpp"], "uint32 Spell::CalculatePowerCost(")
    cost = (cost[cost.index("    // Base powerCost"):cost.index("    SpellSchools school")]
            + cost[cost.index("    // PCT mod from user auras by school"):cost.rindex("}")])
    cast = body(source["SpellMgr.cpp"], "uint32 GetSpellCastTime(")
    cast = cast[cast.index("    int32 castTime ="):cast.index("    // Hunter Ranged spells")]
    prepare = body(source["Spell.cpp"], "void Spell::Prepare()")
    prepare = prepare[prepare.index("    m_spellState ="):prepare.index("    ReSetTimer();") + len("    ReSetTimer();")]
    timer = body(source["Spell.cpp"], "void Spell::update(uint32 difftime)")
    timer = timer[timer.index("        case SPELL_STATE_CASTING:"):timer.index("        case SPELL_STATE_CHANNELING:")]
    blocks = {
        "EFFECT": effect,
        "COST": cost,
        "CAST": cast,
        "PREPARE": prepare,
        "RESET": body(source["Spell.h"], "void ReSetTimer()"),
        "TIMER": timer,
        "CHECK_POWER": body(source["Spell.cpp"], "SpellCastResult Spell::CheckPower(bool strict)"),
        "TAKE_POWER": body(source["Spell.cpp"], "void Spell::TakePower()"),
    }
    harness = pathlib.Path(__file__).with_name("spell_release_oracle.cpp").read_text()
    for marker, block in blocks.items():
        token = "// PINNED_" + marker
        if harness.count(token) != 1:
            raise SystemExit("Missing or duplicate extraction marker: " + marker)
        harness = harness.replace(token, block)
    output = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures/spell_release_cmangos.tsv"
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run([
            "g++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror",
            "-fno-fast-math", "-ffp-contract=off", "-frounding-math",
            "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all",
            str(root / "oracle.cpp"), "-o", str(root / "oracle"),
        ], check=True)
        data = subprocess.check_output([str(root / "oracle")])
        output.write_bytes(data)
        print(len(data.splitlines()), hashlib.sha256(data).hexdigest())


if __name__ == "__main__":
    main()
