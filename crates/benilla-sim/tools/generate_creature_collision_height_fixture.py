#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Hash-check pinned sources; execute unchanged Unit::GetCollisionHeight on authored rows.

The authored host is not the original translation unit, DBC loader or live Unit.
"""
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
    "src/game/Entities/Object.h": "259cdb1a061258d6a8c3e977c743c8c83848c763353b504ca3e617580f58c128",
    "src/game/Server/DBCStructure.h": "354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50",
    "src/game/Server/DBCfmt.h": "8232159a33cd63fe473457f19ae9343aaf2c81a0e9a0c3ba0c3294e1637358e4",
}
REVISION = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"


def source(root):
    result = {}
    for name, expected in HASHES.items():
        data = (root / name).read_bytes().replace(b"\r\n", b"\n")
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit("pinned source hash mismatch: " + name)
        result[Path(name).name] = data.decode("utf-8")
    return result


def extracted_method(text):
    start = text.index("float Unit::GetCollisionHeight() const")
    end = text.index("\nfloat Unit::GetCollisionWidth() const", start)
    return text[start:end]


def extracted_getter(text):
    start = text.index("        float GetObjectScale() const")
    end = text.index("        virtual float GetNativeScale()", start)
    return text[start:end]


def extracted_model(text):
    start = text.index("struct CreatureModelDataEntry\n")
    end = text.index("\n#define MAX_CREATURE_SPELL_DATA_SLOT", start)
    return text[start:end]


def constant(text, name):
    constant = re.search(
        r"^static constexpr float const " + name + r"\s*=\s*[^;]+;",
        text, re.MULTILINE,
    )
    if constant is None:
        raise SystemExit("missing source constant: " + name)
    return constant.group()


def extracted_format(text):
    declaration = re.search(r'^const char CreatureModelDatafmt\[\] = "[^"]+";', text, re.MULTILINE)
    if declaration is None:
        raise SystemExit("missing source model-data format")
    return declaration.group()


def harness(sources):
    constants = "\n".join(constant(sources["ObjectDefines.h"], name) for name in [
        "DEFAULT_COLLISION_HEIGHT", "DEFAULT_OBJECT_SCALE",
    ])
    text = Path(__file__).with_name("creature_collision_height_oracle.cpp").read_text()
    for marker, fragment in {
        "// PINNED_CONSTANTS": constants,
        "// PINNED_METHOD": extracted_method(sources["Unit.cpp"]),
        "// PINNED_GETTER": extracted_getter(sources["Object.h"]),
        "// PINNED_MODEL": extracted_model(sources["DBCStructure.h"]),
        "// PINNED_FORMAT": extracted_format(sources["DBCfmt.h"]),
    }.items():
        if text.count(marker) != 1:
            raise SystemExit("missing or repeated harness insertion marker: " + marker)
        text = text.replace(marker, fragment)
    return text


def record_fixture(target, output, check):
    if check:
        if target.read_bytes() != output:
            raise SystemExit("collision-height fixture differs from source oracle")
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
    print("Authored host/Rust translation: GPL-3.0-or-later; not the original translation unit", flush=True)
    for name, digest in HASHES.items():
        print("LF-normalized full-source SHA256 " + digest + " " + name, flush=True)
    for name, fragment in [
        ("Unit::GetCollisionHeight", extracted_method(sources["Unit.cpp"])),
        ("Object::GetObjectScale", extracted_getter(sources["Object.h"])),
        ("CreatureModelDataEntry", extracted_model(sources["DBCStructure.h"])),
    ]:
        print("Unchanged extraction SHA256 " + hashlib.sha256(fragment.encode()).hexdigest() + " " + name, flush=True)
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
    target = Path(__file__).parents[1] / "tests/fixtures/creature_collision_height_cmangos.test-fixture"
    record_fixture(target, output, args.check)
    print(f"{len(output.splitlines())} source-extracted cases; SHA256 {hashlib.sha256(output).hexdigest()}")


if __name__ == "__main__":
    main()
