#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate/verify exact-source swimming allowed-Z scalar observations offline."""
import argparse
import hashlib
import pathlib
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
import ground_height_generate as base

HERE = pathlib.Path(__file__).resolve().parent
EXTRA = HERE / 'swimming_allowed_z_source_hashes.sha256'
PACKAGE = HERE / 'swimming_allowed_z_source_fragments.test-fixture'
FIXTURE = HERE / 'fixtures/swimming-allowed-z.test-fixture'


def build(root):
    source = base.guarded_sources(root)
    for line in EXTRA.read_text().splitlines():
        expected, name = line.split('  ', 1)
        raw = (root / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}')
        source[name] = raw.decode('latin1')
    constants = '\n'.join(base.exact_line(source['src/game/Maps/GridDefines.h'], rf'^#define {name}\s+.*?$') for name in ('INVALID_HEIGHT', 'INVALID_HEIGHT_VALUE', 'DEFAULT_HEIGHT_SEARCH', 'DEFAULT_WATER_SEARCH'))
    # VMAP_INVALID_HEIGHT_VALUE is the same source sentinel, from the VMap interface.
    constants += '\n' + base.exact_line(source['src/game/vmap/IVMapManager.h'], r'^#define VMAP_INVALID_HEIGHT_VALUE\s+.*?$') + '\n'
    defs = source['src/game/Maps/GridMapDefines.h']
    liquid_types = base.exact_method(defs, 'enum GridMapLiquidStatus\n', ';')
    liquid_types += '\n'.join(base.exact_line(defs, rf'^#define {name}\s+.*?$') for name in ('MAP_LIQUID_TYPE_WATER', 'MAP_LIQUID_TYPE_MAGMA', 'MAP_LIQUID_TYPE_OCEAN', 'MAP_LIQUID_TYPE_SLIME', 'MAP_ALL_LIQUIDS')) + '\n'
    liquid_types += base.exact_method(defs, 'struct GridMapLiquidData\n', ';')
    declaration = base.exact_line(source['src/game/Maps/GridMap.h'], r'^        (GridMapLiquidStatus getLiquidStatus\(.*? const;)\s*$')
    methods = ''.join(base.exact_method(source[name], signature) for name, signature in (
        ('src/game/Maps/Map.cpp', 'float Map::GetHeight('),
        ('src/game/Entities/Unit.cpp', 'void Unit::UpdateAllowedPositionZ('),
        ('src/game/Maps/GridMap.cpp', 'float TerrainInfo::GetWaterOrGroundLevel(')))
    pieces = {'PINNED_CONSTANTS': constants, 'PINNED_LIQUID_TYPES': liquid_types, 'PINNED_LIQUID_DECLARATION': declaration, 'PINNED_METHODS': methods}
    notices = '\n'.join((source[name][:source[name].index('*/') + 2] if '*/' in source[name] else source[name][:source[name].index('#ifndef')]) for name in ('src/game/Maps/Map.cpp', 'src/game/Entities/Unit.cpp', 'src/game/Maps/GridMap.cpp', 'src/game/Maps/GridMap.h', 'src/game/Maps/GridMapDefines.h', 'src/game/vmap/IVMapManager.h'))
    harness = (HERE / 'swimming_allowed_z_oracle.cpp').read_text()
    package = '// Verbatim source fragments, CMaNGOS Classic ' + base.PIN + '\n' + notices + '\n'
    for name, text in pieces.items():
        marker = '// ' + name
        if harness.count(marker) != 1:
            raise SystemExit('Missing/duplicate marker: ' + marker)
        harness = harness.replace(marker, text)
        package += '\n// ' + name + '\n' + text
    return notices + '\n' + harness, package


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true')
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('Little-endian binary32 required')
    harness, package = build(args.source.resolve())
    if args.check and PACKAGE.read_text() != package:
        raise SystemExit('Verbatim swimming allowed-Z source package differs')
    with tempfile.TemporaryDirectory(prefix='swimming-allowed-z-oracle-') as temporary:
        work = pathlib.Path(temporary)
        (work / 'oracle.cpp').write_text(harness)
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off', str(work / 'oracle.cpp'), '-o', str(work / 'oracle')]
        if args.sanitize:
            command += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
        base.run(command)
        result = base.run([str(work / 'oracle')], stdout=subprocess.PIPE).stdout
    if args.check:
        if FIXTURE.read_bytes() != result:
            raise SystemExit('Swimming allowed-Z fixture mismatch')
    else:
        PACKAGE.write_text(package)
        FIXTURE.write_bytes(result)
    count = sum(not line.startswith(b'#') for line in result.splitlines())
    print(f'{"Verified" if args.check else "Generated"} {count} swimming allowed-Z rows; sha256={hashlib.sha256(result).hexdigest()}')


if __name__ == '__main__':
    main()
