#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Execute the unchanged pinned Chase condition and helpers in an authored host.

This does not compile the complete original DispatchSplineToPosition method,
translation unit, real map/terrain providers, PathFinder, or movement lifecycle.
"""
import argparse
import hashlib
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile

REVISION = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
MANIFEST = Path(__file__).with_name("chase_gate_source_hashes.sha256")
HASHES = {line.split()[1]: line.split()[0] for line in MANIFEST.read_text().splitlines()
          if line and not line.startswith("#")}


def source(root):
    result = {}
    for name, expected in HASHES.items():
        data = (root / name).read_bytes().replace(b"\r\n", b"\n")
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit("pinned source hash mismatch: " + name)
        result[Path(name).name] = data.decode("utf-8")
    return result


def block(text, signature):
    """Copy an unchanged brace-delimited block from the hash-verified source."""
    start = text.index(signature)
    opening = text.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        if end == len(text):
            raise ValueError("unterminated source block: " + signature)
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    if end < len(text) and text[end] == ";":
        end += 1
    return text[start:end]


def declaration(text, signature):
    matches = [line for line in text.splitlines() if signature in line]
    if len(matches) != 1:
        raise ValueError("missing or ambiguous source declaration: " + signature)
    return matches[0]


def gate_condition(text):
    signature = "bool ChaseMovementGenerator::DispatchSplineToPosition("
    method = block(text, signature)
    matches = [line for line in method.splitlines()
               if line.strip().startswith("if (owner.IsWithinDist3d(x, y, z, 200.f) &&")]
    if len(matches) != 1:
        raise ValueError("missing or ambiguous source Chase condition")
    return matches[0]


def fragments(sources):
    condition = gate_condition(sources["TargetedMovementGenerator.cpp"])
    # Only observe the original numeric subexpressions, without recreating them.
    parts = condition.strip().removeprefix("if (").removesuffix(")").split(" && ")
    if len(parts) != 5:
        raise ValueError("unexpected source Chase condition shape")
    liquid = sources["GridMapDefines.h"]
    constants = re.search(r"^#define MAP_LIQUID_TYPE_NO_WATER[^\n]*\n.*?^#define MAP_ALL_LIQUIDS[^\n]*",
                          liquid, re.MULTILINE | re.DOTALL)
    if constants is None:
        raise ValueError("missing source liquid masks")
    return {
        "DISTANCE_ENUM": block(sources["Object.h"], "enum DistanceCalculation\n"),
        "LIQUID_ENUM": block(liquid, "enum GridMapLiquidStatus\n"),
        "LIQUID_DATA": block(liquid, "struct GridMapLiquidData\n"),
        "LIQUID_MASKS": constants.group(),
        "ENVIRONMENT_ENUM": block(sources["Player.h"], "enum EnvironmentFlags\n"),
        "DISTANCE_DECL": declaration(sources["Object.h"], "float GetDistance(float x, float y, float z,"),
        "WITHIN_DECL": declaration(sources["Object.h"], "bool IsWithinDist3d(float x, float y, float z,"),
        "LOS_DECL": declaration(sources["Object.h"], "bool IsWithinLOS(float ox, float oy, float oz,"),
        "WATER_DECL": declaration(sources["GridMap.h"], "bool IsInWater(float x, float y, float z,"),
        "LIQUID_DECL": declaration(sources["GridMap.h"], "float collisionHeight = 2.03128f) const;"),
        "PLAYER_WATER": declaration(sources["Player.h"], "bool IsInWater() const override"),
        "DISTANCE": block(sources["Object.cpp"], "float WorldObject::GetDistance(float x, float y, float z,"),
        "WITHIN": block(sources["Object.cpp"], "bool WorldObject::IsWithinDist3d("),
        "LOS": block(sources["Object.cpp"], "bool WorldObject::IsWithinLOS(float ox,"),
        "UNIT_WATER": block(sources["Unit.cpp"], "bool Unit::IsInWater() const"),
        "TERRAIN_WATER": block(sources["GridMap.cpp"], "bool TerrainInfo::IsInWater("),
        "CHASE_CONDITION": condition,
        "DISTANCE_EXPRESSION": parts[0],
        "VERTICAL_EXPRESSION": parts[1],
    }


def harness(sources):
    text = Path(__file__).with_name("chase_gate_oracle.cpp").read_text()
    for name, fragment in fragments(sources).items():
        marker = "// PINNED_" + name
        if text.count(marker + "\n") != 1:
            raise ValueError("missing or repeated harness insertion marker: " + marker)
        text = text.replace(marker + "\n", fragment + "\n")
    if "// PINNED_" in text:
        raise ValueError("unexpanded source marker")
    return text


def record_fixture(target, output, check):
    if check:
        if target.read_bytes() != output:
            raise SystemExit("Chase gate fixture differs from source oracle")
    else:
        target.write_bytes(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--sanitize", action="store_true")
    args = parser.parse_args()
    sources = source(args.upstream)
    text = harness(sources)
    compiler = shutil.which("g++")
    if compiler is None:
        raise SystemExit("missing required g++ compiler")
    version = subprocess.check_output([compiler, "-dumpfullversion"], text=True).strip()
    target = subprocess.check_output([compiler, "-dumpmachine"], text=True).strip()
    if version != "14.2.0" or target != "x86_64-linux-gnu":
        raise SystemExit("unsupported audited compiler profile: " + version + " " + target)
    print(f"Compiler {compiler}; version {version}; target {target}", flush=True)
    print("Source: CMaNGOS Classic " + REVISION + "; Project contributors; GPL-2.0-or-later", flush=True)
    print("Authored host/Rust translation: GPL-3.0-or-later; not the original method/translation unit", flush=True)
    for name, digest in HASHES.items():
        print("LF-normalized full-source SHA256 " + digest + " " + name, flush=True)
    for name, fragment in fragments(sources).items():
        print("Unchanged extraction SHA256 " + hashlib.sha256(fragment.encode()).hexdigest() + " " + name, flush=True)
    print("Profile: IEEE binary32, FLT_EVAL_METHOD=0, FE_TONEAREST, FTZ=DAZ=0, gradual underflow", flush=True)
    print("Generated harness SHA256 " + hashlib.sha256(text.encode()).hexdigest(), flush=True)
    with tempfile.TemporaryDirectory() as temp:
        cpp, exe = Path(temp) / "oracle.cpp", Path(temp) / "oracle"
        cpp.write_text(text)
        command = [compiler, "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror",
                   "-march=x86-64", "-mno-fma", "-fno-fast-math", "-ffp-contract=off", "-frounding-math"]
        if args.sanitize:
            command += ["-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all"]
        command += [str(cpp), "-o", str(exe)]
        print("Compiler invocation: " + shlex.join(command), flush=True)
        subprocess.run(command, check=True)
        output = subprocess.check_output([str(exe)])
        for control in ["--reject-ftz", "--reject-daz", "--reject-round"]:
            negative = subprocess.run([str(exe), control], capture_output=True)
            if negative.returncode != 2 or negative.stdout:
                raise SystemExit("floating-environment rejection failed: " + control)
        print("Rejected FTZ, DAZ and non-nearest controls before source execution", flush=True)
    fixture = Path(__file__).parents[1] / "tests/fixtures/chase_gate_cmangos.test-fixture"
    record_fixture(fixture, output, args.check)
    print(f"{len(output.splitlines())} source-extracted cases; SHA256 {hashlib.sha256(output).hexdigest()}")


if __name__ == "__main__":
    main()
