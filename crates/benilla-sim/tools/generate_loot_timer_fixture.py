#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compile exact CMaNGOS 8ec338a1 Creature.cpp:2805-2829 corpse timer methods.
Copyright CMaNGOS contributors, GPL-2.0-or-later, without warranty.
Harness and authored clock/expiration inputs are GPL-3.0-or-later.
"""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile
from generate_inventory_fixture import body

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    raw = (args.upstream / "src/game/Entities/Creature.cpp").read_bytes()
    assert hashlib.sha256(raw).hexdigest() == "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302"
    source = raw.decode()
    harness = Path(__file__).with_name("loot_timer_oracle.cpp").read_text()
    for marker, signature in [("INSPECT", "void Creature::InspectingLoot("), ("REDUCE", "void Creature::ReduceCorpseDecayTimer(")]:
        token = "// PINNED_" + marker + "\n"
        assert harness.count(token) == 1
        harness = harness.replace(token, body(source, signature) + "\n")
    with tempfile.TemporaryDirectory() as temp:
        cpp, exe = Path(temp)/"oracle.cpp", Path(temp)/"oracle"
        cpp.write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O2", "-Wall", "-Wextra", "-Werror", "-fsanitize=undefined", "-fno-sanitize-recover=all", str(cpp), "-o", str(exe)], check=True)
        output = subprocess.check_output([str(exe)])
    target = Path(__file__).parents[1]/"tests/fixtures/loot_timer.tsv"
    if args.check:
        assert target.read_bytes() == output, "corpse timer fixture changed"
    else:
        target.write_bytes(output)
    print(f"{target.name}: {len(output.splitlines())} exact source cases, SHA256 {hashlib.sha256(output).hexdigest()}")

if __name__ == "__main__":
    main()
