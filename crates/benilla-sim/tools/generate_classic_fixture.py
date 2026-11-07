#!/usr/bin/env python3
"""Compile exact pinned CMaNGOS bodies, not the Rust port, to produce oracle vectors.

Usage: python3 tools/generate_classic_fixture.py /path/to/upstream/Entities
Requires g++ and the exact StatSystem.cpp and Unit.cpp named in PROVENANCE.md.
No network, database, game files or runtime credentials are used.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile

HASHES = {
    "StatSystem.cpp": "91aa0eb47c31aea9d97b64a97c1bdc350b4ea7e7fd30f1ce885a40b69112f1bb",
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
    methods = "".join(body(source["StatSystem.cpp"], signature) for signature in [
        "float Unit::GetHealthBonusFromStamina(float stamina)",
        "float Unit::GetHealthBonusFromStamina() const",
        "float Unit::GetManaBonusFromIntellect(float intellect)",
        "float Unit::GetManaBonusFromIntellect() const",
        "void Unit::UpdateMaxHealth()",
        "void Unit::UpdateMaxPower(Powers power)",
        "void Creature::UpdateMaxHealth()",
        "float Creature::GetHealthBonusFromStamina() const",
    ])
    methods += body(source["Unit.cpp"], "float Unit::CalcArmorReducedDamage(")
    harness = pathlib.Path(__file__).with_name("classic_oracle.cpp").read_text().replace("// PINNED_UPSTREAM_METHODS", methods)
    target = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures"
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-fno-fast-math", "-ffp-contract=off",
                        str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        for mode, filename in [("armor", "armor_cmangos.tsv"), ("stats", "stats_cmangos.tsv"),
                               ("diff", "armor_rounding_differences.tsv")]:
            target.joinpath(filename).write_bytes(subprocess.check_output([str(root / "oracle"), mode]))

if __name__ == "__main__":
    main()
