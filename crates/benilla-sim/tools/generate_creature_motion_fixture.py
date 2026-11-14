#!/usr/bin/env python3
"""Verify pinned motion sources, compile source-expression probes, compare/write fixture.
Usage: generate_creature_motion_fixture.py /path/to/pinned/mangos-classic [--check]
This is a source-expression harness, not a complete CMaNGOS build or geometry oracle.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile
HASHES = {
    "src/game/MotionGenerators/TargetedMovementGenerator.cpp": "eaa569e520a7b7a37d3011822e0381d41ab49b9f2dc6ffa1e43d6344bc14ff15",
    "src/game/MotionGenerators/HomeMovementGenerator.cpp": "d225dd76bd02a33fcfdd17dc0d8a2a953f60f99349f59224e20ef678dbb144ac",
    "src/game/MotionGenerators/PathFinder.cpp": "0fc2684275621af341e104d7593c7f66e5baa6251e088548deb5c547625294ef",
    "src/game/Movement/MoveSpline.cpp": "fcd3a74dc4f8b49a9ff11c268a550815fd4a41c755cfc66c27530e4b73397c08",
    "src/game/Movement/MoveSplineInit.cpp": "cba7b5346dba789a118193f14fd0cc3d9d7387b1e10c9171dc2b58468fcc89cb",
    "src/game/Movement/spline.cpp": "7e8340b710694f360e4582c6c509c133b7a360edbfcb6673e76dcfad71e27c70",
    "src/game/Entities/Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "src/game/Entities/Creature.cpp": "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
    "dep/g3dlite/G3D/Vector3.h": "75b8fa263ade1337c4a06d5e6918aa0a7d28bcb107ed405487e62235838125e2",
}
def main():
    source = {}
    for path, digest in HASHES.items():
        data = pathlib.Path(sys.argv[1], path).read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise SystemExit("Wrong pinned source: " + path)
        source[pathlib.Path(path).name] = data.decode()
    checks = {
        "MoveSpline.cpp": ["velocityInv(1000.f / _velocity), time(minimal_duration)", "time += (s.SegLength(i) * velocityInv);", "u = (time_passed - spline.length(point_Idx)) / (float)seg_time;", "uint32 minimal_diff = std::min(ms_time_diff, segment_time_elapsed());"],
        "spline.cpp": ["result = points[index] + (points[index + 1] - points[index]) * u;", "return (points[index] - points[index + 1]).length();"],
        "Vector3.h": ["return ::sqrtf(x*x + y*y + z*z);"],
        "TargetedMovementGenerator.cpp": ["init.SetCombatSlowed(1.f - ((30.f - std::min(owner.GetHealthPercent(), 30.f)) * 1.67) / 100);", "this->i_recheckDistance.Reset(250);"],
        "Creature.cpp": ["return !IsNoWoundedSlowdown() && GetHealthPercent() < 30.f;"],
        "Unit.cpp": ["POSITION_UPDATE_DELAY = 400", "m_lastMoveTime = GetMap()->GetCurrentClockTime();"],
    }
    for path, expressions in checks.items():
        for expression in expressions:
            if expression not in source[path]:
                raise SystemExit("Missing pinned expression: " + expression)
    tool = pathlib.Path(__file__).with_name("creature_motion_oracle.cpp")
    fixture = tool.parents[1] / "tests/fixtures/creature_motion_cmangos.tsv"
    with tempfile.TemporaryDirectory() as temp:
        binary = pathlib.Path(temp, "oracle")
        subprocess.run(["g++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror", "-ffp-contract=off", "-fno-fast-math", "-frounding-math", "-fsanitize=undefined,float-cast-overflow", str(tool), "-o", str(binary)], check=True)
        data = subprocess.check_output([str(binary)])
    if "--check" in sys.argv[2:]:
        if data != fixture.read_bytes():
            raise SystemExit("Motion oracle fixture differs")
    else:
        fixture.write_bytes(data)
    print("Verified", len(data.splitlines()), "motion numeric probes against pinned source expressions")
if __name__ == "__main__":
    main()
