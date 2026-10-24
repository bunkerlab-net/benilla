#!/usr/bin/env python3
"""Compile exact pinned CMaNGOS relation methods against a bounded ordinary-NPC harness.

Usage: python3 tools/generate_target_fixture.py /path/to/pinned/source/files
No network/DB/assets required. Files must match the recorded source hashes.
The generated numeric fixtures are synthetic, not native faction/world rows.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile

HASHES = {
    "Relations.cpp": "06e453c80b33cddf1606004055a5d1407a46007b20fef73c5dc917156256f77b",
    "Unit.h": "2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1",
    "SpellDefines.h": "07c3fd11a40c993ab8feeb718d21a7a2ae9b5c1700efd2e03ac53e76c6f938bf",
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
    # Extract constants and full function bodies verbatim; no Rust implementation used.
    constants = body(source["Unit.h"], "enum UnitFlags") + ";\n"
    constants += body(source["SpellDefines.h"], "enum SpellAttributesEx2") + ";\n"
    constants += body(source["SpellDefines.h"], "enum SpellAttributesEx3") + ";\n"
    comparator = body(source["Relations.cpp"], "static inline ReputationRank GetFactionReaction(FactionTemplateEntry const* thisTemplate, FactionTemplateEntry const* otherTemplate)")
    methods = "".join(body(source["Relations.cpp"], signature) for signature in [
        "bool Unit::CanAttack(const Unit* unit) const",
        "bool Unit::CanAttackNow(const Unit* unit) const",
        "bool Unit::CanAssist(const Unit* unit, bool /*ignoreFlags*/) const",
        "bool Unit::CanAttackServerside(Unit const* unit, bool ignoreFlagsSource, bool ignoreFlagsTarget) const",
        "bool Unit::CanAttackInCombat(Unit const* target, bool ignoreFlagsSource, bool ignoreFlagsTarget) const",
        "bool Unit::CanAttackSpell(Unit const* target, SpellEntry const* spellInfo, bool isAOE) const",
    ])
    harness = pathlib.Path(__file__).with_name("target_oracle.cpp").read_text()
    harness = harness.replace("// PINNED_CONSTANTS", constants).replace("// PINNED_COMPARATOR", comparator).replace("// PINNED_METHODS", methods)
    output = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures"
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror", str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        for mode in ["factions", "targets"]:
            data = subprocess.check_output([str(root / "oracle"), mode])
            # Digest every ordered vector to avoid committing a bulky numeric fixture.
            rows = len(data.splitlines())
            digest = hashlib.sha256(data).hexdigest()
            output.joinpath(f"{mode}_cmangos.sha256").write_text(f"{rows} {digest}\n")
            print(mode, rows, digest)


if __name__ == "__main__":
    main()
