#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Hash-pinned player range, mainhand parry block and creature shield oracle.

The parry numeric block is extracted unchanged from DealMeleeDamage; this is not
an oracle for the surrounding outcome/offhand/no-parry-haste callback gates.
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
    "Object.cpp": "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b",
    "ObjectDefines.h": "8a5433945cf1a8016afa7ce9618cd0b2140721961acf05c44dee2b47cbff3491",
    "Creature.h": "a92b0e95e5bf7827389fcca0acc0edaba48a1a083efbf5a01a752a4d171664fc",
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
            raise SystemExit("Wrong pinned source: " + name)
        sources[name] = raw.decode()
    constants = re.search(r"#define BASE_MELEERANGE_OFFSET[^\n]+", sources["Unit.h"]).group() + "\n"
    for name in ("ATTACK_DISTANCE", "MELEE_LEEWAY"):
        constants += re.search(r"static constexpr float const " + name + r"\s*=[^;]+;", sources["ObjectDefines.h"]).group() + "\n"
    methods = body(sources["Unit.cpp"], "bool Unit::CanReachWithMeleeAttack(")
    for signature in ("float WorldObject::GetCombinedCombatReach(WorldObject const*", "float WorldObject::GetCombinedCombatReach(bool"):
        methods += body(sources["Object.cpp"], signature)
    source = sources["Unit.cpp"]
    start = source.index("                float percent20 = victim->GetAttackTime(BASE_ATTACK) * 0.20f;", source.index("void Unit::DealMeleeDamage("))
    end = source.index("\n            }\n        }\n    }", start)
    parry = source[start:end] + "\n"
    harness = pathlib.Path(__file__).with_name("player_melee_geometry_oracle.cpp").read_text()
    for key, value in {"CONSTANTS": constants, "METHODS": methods, "PARRY": parry, "SHIELD": body(sources["Creature.h"], "uint32 GetShieldBlockValue() const override")}.items():
        marker = "// PINNED_" + key + "\n"
        assert harness.count(marker) == 1
        harness = harness.replace(marker, value)
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter", "-fno-fast-math", "-ffp-contract=off", "-frounding-math", "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all", str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        data = subprocess.check_output([str(root / "oracle")])
    path = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures/player_melee_geometry_cmangos.tsv"
    path.write_bytes(data)
    print(len(data.splitlines()), hashlib.sha256(data).hexdigest())


if __name__ == "__main__":
    main()
