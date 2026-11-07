#!/usr/bin/env python3
"""Generate authored height vectors using exact hash-checked pinned GridMap bodies.

Usage: python3 tools/generate_terrain_fixture.py /path/to/pinned/GridMap.cpp
No game assets, network access, native server, or new library dependencies.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile

SOURCE_SHA256 = "52c6d22fbe9f275196a44bf4f9b39da527071e3c81f7460a4ac03a3c9107b85d"


def method(source, signature):
    start = source.index(signature)
    opening = source.index("{", start)
    depth, end = 1, opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end] + "\n"


def main():
    data = pathlib.Path(sys.argv[1]).read_bytes()
    if hashlib.sha256(data).hexdigest() != SOURCE_SHA256:
        raise SystemExit("Wrong pinned GridMap.cpp hash")
    source = data.decode()
    methods = "\n".join(method(source, signature) for signature in [
        "float GridMap::getHeightFromFlat(",
        "bool GridMap::isHole(",
        "float GridMap::getHeightFromFloat(",
    ])
    here = pathlib.Path(__file__).resolve().parent
    harness = (here / "terrain_oracle.cpp").read_text().replace("// PINNED_UPSTREAM_METHODS", methods)
    # Preserve the source grant with the extracted functions in the temporary translation.
    harness = source[:source.index("#include")] + harness
    target = here.parent.parent / "benilla-app/src/net/p2p/world/native_geometry/products/terrain-oracle.tsv"
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-fno-fast-math", "-ffp-contract=off",
                        str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        target.write_bytes(subprocess.check_output([str(root / "oracle")]))
    print(target)


if __name__ == "__main__":
    main()
