#!/usr/bin/env python3
"""Exact-source TerrainInfo height-selection oracle, not a VMap geometry oracle.

Usage: python3 tools/generate_static_height_fixture.py /path/to/pinned/GridMap.cpp [--check]
Compile the complete hash-checked GetHeightStatic body with its source constants
and original flat GridMap height dispatch. The harness deliberately supplies
ordered scripted VMap replies and records every queried z/range. This isolates
selection and query scheduling; actual VMap ray geometry has a separate oracle.
No game assets, network, native server, or Rust compiler is used.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import pathlib
import re
import subprocess
import tempfile

from generate_terrain_fixture import method

SOURCE_HASHES = {
    "GridMap.cpp": "52c6d22fbe9f275196a44bf4f9b39da527071e3c81f7460a4ac03a3c9107b85d",
    "GridDefines.h": "5f5413657f84c5f5964f2a69d342874d8d1f7f5ed2bab969e1d78b0650141d82",
    "GridMap.h": "accbf9ecc6f436b952d3d38ee2e506fe3d331a7367c048745334160705760c4d",
    "IVMapManager.h": "5226e3265f3fed67140a908e52d4d8005028602fdbe2192adc9e9838fa778336",
}


def checked(path):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != SOURCE_HASHES[path.name]:
        raise SystemExit(f"Wrong pinned {path.name} hash")
    return raw.decode()


def declaration(source, expression):
    matches = re.findall(expression, source, flags=re.M)
    if len(matches) != 1:
        raise SystemExit(f"Expected one source declaration: {expression}")
    return matches[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gridmap_cpp", type=pathlib.Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = checked(args.gridmap_cpp)
    directory = args.gridmap_cpp.parent
    definitions = checked(directory / "GridDefines.h")
    header = checked(directory / "GridMap.h")
    vmap = checked(directory.parent / "vmap" / "IVMapManager.h")
    constants = "\n".join(declaration(definitions, rf"^#define {name}\s+.*?$") for name in (
        "INVALID_HEIGHT", "INVALID_HEIGHT_VALUE", "DEFAULT_HEIGHT_SEARCH", "DEFAULT_WATER_SEARCH",
    ))
    constants += "\n" + declaration(vmap, r"^#define VMAP_INVALID_HEIGHT_VALUE\s+.*?$")
    fields = "\n".join(declaration(header, expression) for expression in (
        r"^        float m_gridHeight;.*?$",
        r"^        typedef float\(GridMap::\*pGetHeightPtr\).*?$",
        r"^        pGetHeightPtr m_gridGetHeight;.*?$",
        r"^        float getHeightFromFlat\(.*?$",
        r"^        inline float getHeight\(.*?$",
    ))
    bodies = "\n".join(method(source, signature) for signature in (
        "float GridMap::getHeightFromFlat(",
        "float TerrainInfo::GetHeightStatic(",
    ))
    replacements = {
        "// PINNED_HEIGHT_CONSTANTS": constants,
        "// PINNED_GRIDMAP_FIELDS": fields,
        "// PINNED_UPSTREAM_METHODS": bodies,
    }
    here = pathlib.Path(__file__).resolve().parent
    harness = (here / "static_height_oracle.cpp").read_text()
    for marker, replacement in replacements.items():
        if harness.count(marker) != 1:
            raise SystemExit(f"Expected one harness marker: {marker}")
        harness = harness.replace(marker, replacement)
    harness = source[:source.index("#include")] + harness
    target = here / "fixtures" / "static-height.tsv"
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-fno-fast-math", "-ffp-contract=off",
                        "-Wall", "-Wextra", "-Werror", str(root / "oracle.cpp"),
                        "-o", str(root / "oracle")], check=True)
        result = subprocess.check_output([str(root / "oracle")])
    if args.check:
        if target.read_bytes() != result:
            raise SystemExit("Static height selection oracle mismatch")
    else:
        target.write_bytes(result)
    print(f"{target}: {'verified' if args.check else 'generated'}")


if __name__ == "__main__":
    main()
