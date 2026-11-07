#!/usr/bin/env python3
"""Hash-checked original effect-value arithmetic on synthetic constant self casts."""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile
from generate_starter_init_fixture import body


def main():
    data = pathlib.Path(sys.argv[1], "Object.cpp").read_bytes()
    assert hashlib.sha256(data).hexdigest() == "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b"
    original = body(data.decode(), "int32 WorldObject::CalculateSpellEffectValue(")
    harness = pathlib.Path(__file__).with_name("starter_effect_amount_oracle.cpp").read_text()
    marker = "// PINNED_EFFECT_VALUE\n"
    assert harness.count(marker) == 1
    harness = harness.replace(marker, original)
    with tempfile.TemporaryDirectory() as directory:
        root = pathlib.Path(directory)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run([
            "g++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror",
            "-fno-fast-math", "-ffp-contract=off", "-frounding-math",
            "-fsanitize=undefined,float-cast-overflow", str(root / "oracle.cpp"),
            "-o", str(root / "oracle"),
        ], check=True)
        output = subprocess.check_output([str(root / "oracle")])
    destination = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures/starter_effect_amount_cmangos.tsv"
    destination.write_bytes(output)
    print(len(output.splitlines()), hashlib.sha256(output).hexdigest())


if __name__ == "__main__":
    main()
