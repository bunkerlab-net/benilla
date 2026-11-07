#!/usr/bin/env python3
"""Generate original numeric/timer probes from hash-checked pinned CMaNGOS C++.

Usage: python3 tools/generate_spell_fixture.py /path/to/pinned/source/files
No native rows/assets. Each extracted statement block is unchanged; adapters
explicitly represent only the bounded no-modifier branches, not a whole server.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile

HASHES = {
    "Object.cpp": "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b",
    "Spell.cpp": "ebcabd3073f188541e6a1abcc82a8d0ee6d48d360dc5ab16d9e63b3e87d75201",
    "SpellAuras.cpp": "5e60510d23b789f8f0fb584da8f05a5be675f1cb4024ce997c989cde00105fde",
    "SpellMgr.cpp": "d1a7064febbe3a7657e85931c4bda89a09adfabc89b7cbed23fdbffd6e37c22d",
    "Util.h": "1f53eafa77ab89935c905f331d9ee7e5e2fabf1fca7f812e2e913f19243c5878",
    "Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
}


def body(text, signature):
    start = text.index(signature)
    opening = text.index("{", start)
    depth = 1
    end = opening + 1
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
    effect = body(source["Object.cpp"], "int32 WorldObject::CalculateSpellEffectValue(")
    effect = effect[effect.index("    int32 baseDice ="):effect.index("    // random damage")]
    travel = source["Spell.cpp"]
    start = travel.index("        dist = sqrt(dist);")
    end = travel.index("\n\n        // Calculate minimum incoming time", start)
    travel = travel[start:end]
    crit = body(source["Unit.cpp"], "uint32 Unit::CalculateCritAmount(const Unit *victim")
    chance = body(source["Util.h"], "inline uint32 chance_u(float chance)")
    chance += body(source["Util.h"], "inline bool roll_chance_combat(float chance)")
    aura = body(source["SpellAuras.cpp"], "void Aura::Update(uint32 diff)")
    time_body = body(source["Spell.cpp"], "void Spell::update(uint32 difftime)")
    time_body = time_body[time_body.index("        case SPELL_STATE_CASTING:"):time_body.index("        case SPELL_STATE_CHANNELING:")]
    cost = body(source["Spell.cpp"], "uint32 Spell::CalculatePowerCost(")
    cost = cost[cost.index("    // Base powerCost"):cost.index("    SpellSchools school")]
    cost_end = body(source["Spell.cpp"], "uint32 Spell::CalculatePowerCost(")
    cost_end = cost_end[cost_end.index("    // PCT mod from user auras by school"):cost_end.rindex("}")]
    cast = body(source["SpellMgr.cpp"], "uint32 GetSpellCastTime(")
    cast = cast[cast.index("    int32 castTime ="):cast.index("    // Hunter Ranged spells")]
    harness = pathlib.Path(__file__).with_name("spell_oracle.cpp").read_text()
    for marker, block in {"EFFECT": effect, "TRAVEL": travel, "CRIT": crit, "CHANCE": chance, "AURA": aura, "TIMER": time_body, "COST": cost + cost_end, "CAST": cast, "MISS": body(source["Unit.cpp"], "float Unit::CalculateSpellMissChance("), "MAGIC_RESIST": body(source["Unit.cpp"], "float Unit::CalculateEffectiveMagicResistancePercent(")}.items():
        harness = harness.replace("// PINNED_" + marker, block)
    output = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures/spell_cmangos.tsv"
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror", "-fno-fast-math", "-ffp-contract=off", "-frounding-math", "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all", str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        data = subprocess.check_output([str(root / "oracle")])
        output.write_bytes(data)
        print(len(data.splitlines()), hashlib.sha256(data).hexdigest())


if __name__ == "__main__":
    main()
