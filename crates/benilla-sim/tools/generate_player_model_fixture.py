#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Hash-check pinned complete sources; execute unchanged player initialization/height.

The authored storage/lookup host is not a live Player or the native DBC loader.
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
HASHES = {
    "src/game/Entities/Player.cpp": "d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e",
    "src/game/Entities/Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "src/game/Entities/Object.cpp": "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b",
    "src/game/Entities/Object.h": "259cdb1a061258d6a8c3e977c743c8c83848c763353b504ca3e617580f58c128",
    "src/game/Entities/ObjectDefines.h": "8a5433945cf1a8016afa7ce9618cd0b2140721961acf05c44dee2b47cbff3491",
    "src/game/Server/DBCStructure.h": "354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50",
    "src/game/Server/DBCfmt.h": "8232159a33cd63fe473457f19ae9343aaf2c81a0e9a0c3ba0c3294e1637358e4",
    "src/game/Globals/SharedDefines.h": "4832b8e0da45211143acae7c42173fea42732e2acdbf190692725e51b020bfe8",
}


def source(root):
    result = {}
    for name, expected in HASHES.items():
        data = (root / name).read_bytes().replace(b"\r\n", b"\n")
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit("pinned source hash mismatch: " + name)
        result[Path(name).name] = data.decode("utf-8")
    return result


def between(text, start, end):
    first = text.index(start)
    return text[first:text.index(end, first)]


def constant(text, name):
    value = re.search(r"^static constexpr float const " + name + r"\s*=\s*[^;]+;", text, re.MULTILINE)
    if value is None:
        raise SystemExit("missing source constant: " + name)
    return value.group()


def format_declaration(text, name):
    value = re.search(r'^const char ' + name + r'\[\] = "[^"]+";', text, re.MULTILINE)
    if value is None:
        raise SystemExit("missing source format: " + name)
    return value.group()


def fragments(s):
    return {
        "INIT": between(s["Player.cpp"], "void Player::InitDisplayIds()", "// Return true is the bought item"),
        "NATIVE_SCALE": between(s["Player.cpp"], "float Player::GetNativeScale() const", "void Player::RewardRage"),
        "HEIGHT": between(s["Unit.cpp"], "float Unit::GetCollisionHeight() const", "float Unit::GetCollisionWidth() const"),
        "GETTER": between(s["Object.h"], "        float GetObjectScale() const", "        virtual float GetNativeScale()"),
        "INIT_VALUES": between(s["Object.cpp"], "void Object::_InitValues()", "void Object::_Create("),
        "STORAGE": between(s["Object.h"], "        union\n", "        uint16 m_valuesCount;"),
        "SET_SCALE": between(s["Object.cpp"], "void Object::SetObjectScale(float newScale)", "void Object::SendForcedObjectUpdate()"),
        "SET_FLOAT": between(s["Object.cpp"], "void Object::SetFloatValue(uint16 index, float value)", "void Object::SetByteValue"),
        "DISPLAY": between(s["DBCStructure.h"], "struct CreatureDisplayInfoEntry\n", "struct CreatureDisplayInfoExtraEntry\n"),
        "MODEL": between(s["DBCStructure.h"], "struct CreatureModelDataEntry\n", "#define MAX_CREATURE_SPELL_DATA_SLOT"),
        "GENDER": between(s["SharedDefines.h"], "enum Gender\n", "#define MAX_GENDER"),
        "CONSTANTS": "\n".join(constant(s["ObjectDefines.h"], n) for n in ["DEFAULT_OBJECT_SCALE", "DEFAULT_COLLISION_HEIGHT"]),
        "FORMATS": "\n".join(format_declaration(s["DBCfmt.h"], n) for n in ["CreatureDisplayInfofmt", "CreatureModelDatafmt"]),
    }


def harness(sources):
    text = Path(__file__).with_name("player_model_oracle.cpp").read_text()
    for name, fragment in fragments(sources).items():
        marker = "// PINNED_" + name + "\n"
        if text.count(marker) != 1:
            raise SystemExit("missing or repeated harness insertion marker: " + marker.strip())
        text = text.replace(marker, fragment + "\n")
    return text


def record_fixture(target, output, check):
    if check:
        if target.read_bytes() != output:
            raise SystemExit("player-model fixture differs from source oracle")
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
    target = Path(__file__).parents[1] / "tests/fixtures/player_model_cmangos.test-fixture"
    record_fixture(target, output, args.check)
    print(f"{len(output.splitlines())} source-extracted cases; SHA256 {hashlib.sha256(output).hexdigest()}")


if __name__ == "__main__":
    main()
