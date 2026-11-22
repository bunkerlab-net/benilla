#!/usr/bin/env python3
"""Execute pinned complete GridMap liquid/AREA methods over authored DBC stores.

Usage: python3 tools/generate_area_liquid_fixture.py /path/to/GridMap.cpp [--check]
No game assets, network access, or Rust implementation is used by this oracle.
The source's getLiquidStatus, getArea, GetAreaEntryByAreaID, flat-height body and
dispatch run unchanged. The harness supplies complete synthetic stores, not an
alternative implementation of any lookup/override/status decision.
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
    "GridMapDefines.h": "3a901f22b6d65edae74a93b5cd2be793ebc5c3884eb2c0aa06a37265ef53e54e",
    "GridDefines.h": "5f5413657f84c5f5964f2a69d342874d8d1f7f5ed2bab969e1d78b0650141d82",
    "GridMap.h": "accbf9ecc6f436b952d3d38ee2e506fe3d331a7367c048745334160705760c4d",
    "DBCfmt.h": "8232159a33cd63fe473457f19ae9343aaf2c81a0e9a0c3ba0c3294e1637358e4",
    "DBCStructure.h": "354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50",
    "DBCStores.cpp": "5f1bef967c9ad0ecc8779f679e6e734f068994979917323d507eb95663895109",
}


def checked(path):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != SOURCE_HASHES[path.name]:
        raise SystemExit(f"Wrong pinned {path.name} hash")
    return raw.decode()


def single_line(source, expression):
    matches = re.findall(expression, source, flags=re.M)
    if len(matches) != 1:
        raise SystemExit(f"Expected exactly one source declaration: {expression}")
    return matches[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gridmap_cpp", type=pathlib.Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source = checked(args.gridmap_cpp)
    directory = args.gridmap_cpp.parent
    definitions = checked(directory / "GridMapDefines.h")
    grid = checked(directory / "GridDefines.h")
    header = checked(directory / "GridMap.h")
    dbc = checked(directory.parent / "Server" / "DBCStructure.h")
    dbc_format = checked(directory.parent / "Server" / "DBCfmt.h")
    stores = checked(directory.parent / "Server" / "DBCStores.cpp")
    formats = [single_line(dbc_format, rf'^const char {name}\[\] = ".*?";$')
               for name in ("LiquidTypefmt", "AreaTableEntryfmt")]
    if formats != ['const char LiquidTypefmt[] = "niii";',
                   'const char AreaTableEntryfmt[] = "niiiixxxxxissssssssxixxxi";']:
        raise SystemExit("Wrong source LiquidType/AreaTable formats")
    constants = "\n".join(single_line(grid, rf"^#define {name}\s+.*?$") for name in
                          ("SIZE_OF_GRIDS", "MAP_RESOLUTION", "INVALID_HEIGHT_VALUE"))
    liquid_fields = header.split("        // Liquid data\n", 1)[1].split("        // For fast check", 1)[0]
    area_fields = header.split("        // Area data\n", 1)[1].split("        // Height level data", 1)[0]
    height_fields = "\n".join(single_line(header, expression) for expression in (
        r"^        float m_gridHeight;.*?$",
        r"^        typedef float\(GridMap::\*pGetHeightPtr\).*?$",
        r"^        pGetHeightPtr m_gridGetHeight;.*?$",
        r"^        float getHeightFromFlat\(.*?$",
        r"^        inline float getHeight\(.*?$",
        r"^        uint16 getArea\(.*?$",
    ))
    replacements = {
        "// PINNED_GRID_CONSTANTS": constants,
        "// PINNED_GRIDMAP_DEFINES": definitions,
        "// PINNED_DBC_FORMATS": "\n".join(formats),
        "// PINNED_DBC_STRUCTS": "\n".join(method(dbc, f"struct {name}\n").rstrip() + ";" for name in
                                               ("LiquidTypeEntry", "AreaTableEntry")),
        "// PINNED_AREA_ID_LOOKUP": method(stores, "AreaTableEntry const* GetAreaEntryByAreaID("),
        "// PINNED_GRIDMAP_FIELDS": area_fields + liquid_fields + height_fields,
        "// PINNED_UPSTREAM_METHODS": "\n".join(method(source, signature) for signature in (
            "float GridMap::getHeightFromFlat(",
            "uint16 GridMap::getArea(",
            "GridMapLiquidStatus GridMap::getLiquidStatus(",
        )),
    }
    here = pathlib.Path(__file__).resolve().parent
    harness = (here / "area_liquid_oracle.cpp").read_text()
    for marker, replacement in replacements.items():
        if harness.count(marker) != 1:
            raise SystemExit(f"Expected exactly one harness marker: {marker}")
        harness = harness.replace(marker, replacement)
    # Preserve each original grant alongside the extracted declarations/bodies.
    harness = (source[:source.index("#include")] + dbc[:dbc.index("#ifndef")]
               + stores[:stores.index("#include")] + harness)
    target = here / "fixtures" / "area-liquid.tsv"
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-fno-fast-math", "-ffp-contract=off",
                        "-Wall", "-Wextra", "-Werror", str(root / "oracle.cpp"),
                        "-o", str(root / "oracle")], check=True)
        result = subprocess.check_output([str(root / "oracle")])
    if args.check:
        if target.read_bytes() != result:
            raise SystemExit("Area liquid oracle mismatch")
    else:
        target.write_bytes(result)
    print(f"{target}: {'verified' if args.check else 'generated'}")


if __name__ == "__main__":
    main()
