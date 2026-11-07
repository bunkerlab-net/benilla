#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Hash-check pinned sources; execute unchanged Unit::GetCollisionWidth on authored rows."""
import argparse
import hashlib
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile

HASHES = {
    "src/game/Entities/Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "src/game/Entities/ObjectDefines.h": "8a5433945cf1a8016afa7ce9618cd0b2140721961acf05c44dee2b47cbff3491",
}


def source(root):
    result = {}
    for name, expected in HASHES.items():
        data = (root / name).read_bytes().replace(b"\r\n", b"\n")
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit("pinned source hash mismatch: " + name)
        result[Path(name).name] = data.decode("utf-8")
    return result


def extracted_method(text):
    start = text.index("float Unit::GetCollisionWidth() const")
    end = text.index("\nbool Unit::HasOverrideScript", start)
    return text[start:end]


def harness(sources):
    constant = re.search(
        r"^static constexpr float const DEFAULT_COLLISION_WIDTH\s*=\s*[^;]+;",
        sources["ObjectDefines.h"], re.MULTILINE,
    )
    if constant is None:
        raise SystemExit("missing source collision-width constant")
    text = Path(__file__).with_name("creature_collision_width_oracle.cpp").read_text()
    return text.replace("// PINNED_CONSTANT", constant.group()).replace(
        "// PINNED_METHOD", extracted_method(sources["Unit.cpp"])
    )


def record_fixture(target, output, check):
    if check:
        if target.read_bytes() != output:
            raise SystemExit("collision-width fixture differs from source oracle")
    else:
        target.write_bytes(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=Path)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--sanitize", action="store_true")
    args = parser.parse_args()
    text = harness(source(args.upstream))
    compiler = shutil.which("g++")
    if compiler is None:
        raise SystemExit("missing required g++ compiler")
    version = subprocess.check_output([compiler, "-dumpfullversion"], text=True).strip()
    target = subprocess.check_output([compiler, "-dumpmachine"], text=True).strip()
    if version != "14.2.0" or target != "x86_64-linux-gnu":
        raise SystemExit("unsupported audited compiler profile: " + version + " " + target)
    print(f"Compiler {compiler}; version {version}; target {target}", flush=True)
    print("Profile: IEEE binary32, FLT_EVAL_METHOD=0, FE_TONEAREST, FTZ=DAZ=0, gradual underflow", flush=True)
    print("Generated harness SHA256 " + hashlib.sha256(text.encode()).hexdigest(), flush=True)
    with tempfile.TemporaryDirectory() as temp:
        cpp, exe = Path(temp) / "oracle.cpp", Path(temp) / "oracle"
        cpp.write_text(text)
        command = [
            compiler, "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror",
            "-march=x86-64", "-mno-fma",
            "-fno-fast-math", "-ffp-contract=off", "-frounding-math",
        ]
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
    target = Path(__file__).parents[1] / "tests/fixtures/creature_collision_width_cmangos.test-fixture"
    record_fixture(target, output, args.check)
    print(f"{len(output.splitlines())} source-extracted cases; SHA256 {hashlib.sha256(output).hexdigest()}")


if __name__ == "__main__":
    main()
