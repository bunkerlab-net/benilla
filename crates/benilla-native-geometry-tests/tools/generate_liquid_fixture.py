#!/usr/bin/env python3
"""Offline exact-source dry GridMap/coordinate oracle; no assets or native server."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import pathlib
import re
import subprocess
import tempfile
from generate_terrain_fixture import method, SOURCE_SHA256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gridmap_cpp", type=pathlib.Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    raw = args.gridmap_cpp.read_bytes()
    if hashlib.sha256(raw).hexdigest() != SOURCE_SHA256:
        raise SystemExit("Wrong pinned GridMap.cpp hash")
    header = args.gridmap_cpp.with_name("GridMapDefines.h").read_bytes()
    if hashlib.sha256(header).hexdigest() != "3a901f22b6d65edae74a93b5cd2be793ebc5c3884eb2c0aa06a37265ef53e54e":
        raise SystemExit("Wrong pinned GridMapDefines.h hash")
    grid_header = args.gridmap_cpp.with_name("GridDefines.h").read_bytes()
    if hashlib.sha256(grid_header).hexdigest() != "5f5413657f84c5f5964f2a69d342874d8d1f7f5ed2bab969e1d78b0650141d82":
        raise SystemExit("Wrong pinned GridDefines.h hash")
    constants = re.findall(r"^#define (?:SIZE_OF_GRIDS|MAP_RESOLUTION)\s+.*?$", grid_header.decode(), flags=re.M)
    if len(constants) != 2:
        raise SystemExit("Missing source grid constants")
    source = raw.decode()
    select = method(source, "GridMap* TerrainInfo::GetGrid(")
    lines = re.findall(r"^    int g[xy] = .*?$", select, flags=re.M)
    if len(lines) != 2:
        raise SystemExit("Missing source grid selectors")
    here = pathlib.Path(__file__).resolve().parent
    harness = (here / "liquid_oracle.cpp").read_text()
    harness = harness.replace("// PINNED_GRIDMAP_DEFINES", header.decode())
    harness = harness.replace("// PINNED_GRID_CONSTANTS", "\n".join(constants))
    harness = harness.replace("// PINNED_UPSTREAM_METHOD", method(source, "GridMapLiquidStatus GridMap::getLiquidStatus("))
    harness = harness.replace("// PINNED_GRID_SELECTION", "\n".join(lines))
    harness = source[:source.index("#include")] + harness
    target = here / "fixtures" / "dry-liquid.tsv"
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-fno-fast-math", "-ffp-contract=off",
                        str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        result = subprocess.check_output([str(root / "oracle")])
    if args.check:
        if target.read_bytes() != result:
            raise SystemExit("Dry liquid oracle mismatch")
    else:
        target.write_bytes(result)
    print(f"{target}: {'verified' if args.check else 'generated'}")


if __name__ == "__main__":
    main()
