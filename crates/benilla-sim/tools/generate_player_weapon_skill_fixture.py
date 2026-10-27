#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Pinned full weapon-skill methods, synthetic operands and canonical log check.

No admission/callback integration proof. This captures unchanged source chance,
strict roll boundaries, UpdateSkill results and UpdateAllCritPercentages calls.
The existing canonical defense log table is read-only; a final source-vs-table
chance divergence fails rather than silently changing canonical arithmetic.
"""
import hashlib
import pathlib
import re
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
HASHES = {
    "Player.cpp": "d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e",
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
    for name, expected in HASHES.items():
        raw = pathlib.Path(sys.argv[1], name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit("Wrong pinned source: " + name)
        sources[name] = raw.decode()
    logs = re.findall(r"0x([0-9a-f]{16})", (ROOT / "src/npc_melee/skill_log.rs").read_text())
    assert len(logs) == 294
    packed = b"".join(int(value, 16).to_bytes(8, "little") for value in logs)
    assert hashlib.sha256(packed).hexdigest() == "88a6d0c0b95514fb14dbc7fd369d857f701f6cd13161d73e0853d13823ff426e"
    methods = "".join(body(sources["Player.cpp"], signature) for signature in (
        "void Player::UpdateCombatSkills(", "void Player::UpdateWeaponSkill(",
        "void Player::UpdateSkill(",
    ))
    values = {
        "PINNED_METHODS": methods,
        "PINNED_ROLL": body(sources["Util.h"], "inline bool roll_chance_f("),
        "CANONICAL_LOG_BITS": "const uint64_t canonical_log_bits[] = {" + ",".join("UINT64_C(0x" + value + ")" for value in logs) + "};\n",
    }
    harness = pathlib.Path(__file__).with_name("player_weapon_skill_oracle.cpp").read_text()
    for key, value in values.items():
        marker = "// " + key + "\n"
        assert harness.count(marker) == 1
        harness = harness.replace(marker, value)
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run([
            "g++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror",
            "-Wno-unused-parameter", "-fno-fast-math", "-ffp-contract=off", "-frounding-math",
            "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all",
            str(root / "oracle.cpp"), "-o", str(root / "oracle"),
        ], check=True)
        data = subprocess.check_output([str(root / "oracle")])
    data = b"\n".join(dict.fromkeys(data.splitlines())) + b"\n"
    path = ROOT / "tests/fixtures/player_weapon_skill_cmangos.tsv"
    path.write_bytes(data)
    print(len(data.splitlines()), hashlib.sha256(data).hexdigest())


if __name__ == "__main__":
    main()
