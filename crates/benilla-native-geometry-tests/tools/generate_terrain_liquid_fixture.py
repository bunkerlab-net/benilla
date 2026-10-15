#!/usr/bin/env python3
"""Run pinned composite dry-VMap TerrainInfo liquid over authored native geometry.

Usage: generate_terrain_liquid_fixture.py /path/to/mangos-classic [--check]
No actual assets, native input compiler, network, Rust, or scripted query replies.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import pathlib
import re
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
import generate_vmap_fixture as base
import generate_area_liquid_fixture as area

HERE = pathlib.Path(__file__).resolve().parent


def line(source, pattern):
    matches = re.findall(pattern, source, flags=re.M)
    if len(matches) != 1:
        raise SystemExit(f'Expected exactly one pinned declaration: {pattern}')
    return matches[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('This fixture requires little-endian IEEE binary32')
    root = args.source.resolve()
    source = base.guarded_sources(root)
    for entry in (HERE / 'scopedlocation_source_hashes.sha256').read_text().splitlines():
        expected, name = entry.split('  ', 1)
        raw = (root / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}')
        source[name] = raw.decode('latin1')
    checked = {}
    for name in area.SOURCE_HASHES:
        directory = 'Maps' if name.startswith('Grid') else 'Server'
        checked[name] = area.checked(root / 'src/game' / directory / name)
    grid = checked['GridMap.cpp']
    header = checked['GridMap.h']
    constants = '\n'.join(line(checked['GridDefines.h'], rf'^#define {name}\s+.*?$') for name in (
        'SIZE_OF_GRIDS', 'MAP_RESOLUTION', 'MAX_NUMBER_OF_GRIDS', 'INVALID_HEIGHT',
        'INVALID_HEIGHT_VALUE', 'DEFAULT_HEIGHT_SEARCH', 'DEFAULT_WATER_SEARCH'))
    constants += '\n' + '\n'.join(line(grid, rf'^static uint16 {name}\[4\].*?$') for name in ('holetab_h', 'holetab_v'))
    fields = header.split('        // Area data\n', 1)[1].split('        // For fast check', 1)[0]
    fields += '\n' + '\n'.join(line(header, expression) for expression in (
        r'^        uint16 m_holes\[16\]\[16\];$',
        r'^        typedef float\(GridMap::\*pGetHeightPtr\).*?$',
        r'^        pGetHeightPtr m_gridGetHeight;.*?$',
        r'^        float getHeightFromFlat\(.*?$',
        r'^        float getHeightFromFloat\(.*?$',
        r'^        bool isHole\(.*?$',
        r'^        inline float getHeight\(.*?$',
        r'^        uint16 getArea\(.*?$',
    ))
    methods = '\n'.join(base.method(grid, signature) for signature in (
        'float GridMap::getHeightFromFlat(', 'bool GridMap::isHole(',
        'float GridMap::getHeightFromFloat(', 'uint16 GridMap::getArea(',
        'GridMapLiquidStatus GridMap::getLiquidStatus(', 'GridMap* TerrainInfo::GetGrid(',
        'float TerrainInfo::GetHeightStatic(', 'GridMapLiquidStatus TerrainInfo::getLiquidStatus('))
    formats = '\n'.join(line(checked['DBCfmt.h'], rf'^const char {name}\[\] = ".*?";$')
                        for name in ('LiquidTypefmt', 'AreaTableEntryfmt'))
    replacements = {
        '// PINNED_GRID_CONSTANTS': constants,
        '// PINNED_GRID_FIELDS': fields,
        '// PINNED_DBC_FORMATS': formats,
        '// PINNED_DBC_STRUCTS': '\n'.join(base.method(checked['DBCStructure.h'], f'struct {name}\n', ';')
                                         for name in ('LiquidTypeEntry', 'AreaTableEntry')),
        '// PINNED_AREA_ID_LOOKUP': base.method(checked['DBCStores.cpp'], 'AreaTableEntry const* GetAreaEntryByAreaID('),
        '// PINNED_TERRAIN_METHODS': methods,
    }
    terrain = (HERE / 'terrain_liquid_oracle.cpp').read_text()
    for marker, value in replacements.items():
        if terrain.count(marker) != 1:
            raise SystemExit(f'Unexpected harness marker: {marker}')
        terrain = terrain.replace(marker, value)
    terrain = grid[:grid.index('#include')] + terrain

    collision, vmap_methods = base.source_fragments(source)
    old_manager = '''namespace VMAP { class VMapManager2 { public:
    Vector3 convertPositionToInternalRep(float x, float y, float z) const;
};'''
    manager = '''namespace VMAP {
typedef std::unordered_map<uint32, StaticMapTree*> InstanceTreeMap;
class VMapManager2 { public:
    InstanceTreeMap iInstanceMapTrees;
    bool iEnableHeightCalc = true;
    Vector3 convertPositionToInternalRep(float x, float y, float z) const;
    InstanceTreeMap::const_iterator GetMapTree(uint32 mapId) const;
    float getHeight(unsigned int mapId, float x, float y, float z, float maxSearchDist);
    bool GetLiquidLevel(uint32 mapId, float x, float y, float z, uint8 mask,
                        float& level, float& floor, uint32& type) const;
'''
    manager += base.method(source[base.VMAP + 'IVMapManager.h'], 'bool isHeightCalcEnabled()') + '};'
    if vmap_methods.count(old_manager) != 1:
        raise SystemExit('Unexpected source manager declaration')
    vmap_methods = vmap_methods.replace(old_manager, manager)
    vmap_methods += '\nnamespace VMAP {\n'
    map_source = source[base.VMAP + 'MapTree.cpp']
    vmap_methods += base.method(map_source, 'class LocationInfoCallback', ';')
    for signature in ('bool StaticMapTree::GetLocationInfo(', 'float StaticMapTree::getHeight('):
        vmap_methods += base.method(map_source, signature)
    for signature in ('InstanceTreeMap::const_iterator VMapManager2::GetMapTree(',
                      'float VMapManager2::getHeight(', 'uint8 GetLiquidMask(',
                      'bool VMapManager2::GetLiquidLevel('):
        vmap_methods += base.method(source[base.VMAP + 'VMapManager2.cpp'], signature)
    vmap_methods += '}\n'
    target = HERE / 'fixtures'
    with tempfile.TemporaryDirectory(prefix='.terrain-liquid-oracle-', dir=HERE) as temporary:
        work = pathlib.Path(temporary)
        (work / 'stubs/Platform').mkdir(parents=True)
        (work / 'stubs/Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
''')
        (work / 'collision.cpp').write_text(collision)
        authored = (HERE / 'vmap_fixture.cpp').read_text().replace('// PINNED_SOURCE_METHODS',
            '#include "IVMapManager.h"\n#include "Maps/GridMapDefines.h"\n' + vmap_methods)
        (work / 'authored_vmap.cpp').write_text(authored)
        scoped = (HERE / 'scopedlocation_fixture.cpp').read_text()
        scoped = scoped.replace('int main(int argc, char** argv) {',
            'static void terrainLiquidQueries(const std::filesystem::path&, VMAP::StaticMapTree&);\n'
            'int authored_location_main(int argc, char** argv) {')
        scoped = scoped.replace('        oracle.finish();', '        terrainLiquidQueries(root, volumeTree);\n        oracle.finish();')
        (work / 'oracle.cpp').write_text(scoped + '\n' + terrain)
        sources = [root / base.VMAP / f'{name}.cpp' for name in ('BIH', 'WorldModel', 'ModelInstance')]
        sources += [root / base.G3D / f'{name}.cpp' for name in ('Ray', 'Vector3', 'AABox', 'Matrix3', 'g3dmath')]
        subprocess.run(['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
            '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections', '-DNO_CORE_FUNCS',
            '-I' + str(work / 'stubs'), '-I' + str(root / base.VMAP), '-I' + str(root / base.G3D),
            '-I' + str(root / 'src/game'), '-I' + str(work), str(work / 'oracle.cpp'),
            str(work / 'collision.cpp'), *map(str, sources), '-Wl,--gc-sections', '-o', str(work / 'oracle')], check=True)
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(work / 'oracle'), str(output)], check=True)
        count = 0
        actual = set()
        for product in sorted(output.rglob('*')):
            if not product.is_file():
                continue
            relative = product.relative_to(output)
            if relative.parts[0] not in ('terrain-liquid', 'terrain-liquid.tsv'):
                if product.read_bytes() != (target / 'vmap_source' / relative).read_bytes():
                    raise SystemExit(f'Existing source geometry/fixture differs: {relative}')
                continue
            actual.add(str(relative))
            destination = target / relative
            if args.check:
                if not destination.is_file() or product.read_bytes() != destination.read_bytes():
                    raise SystemExit(f'Terrain liquid fixture differs: {relative}')
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(product.read_bytes())
            count += 1
        if args.check:
            expected = {'terrain-liquid.tsv'} | {str(path.relative_to(target)) for path in
                       (target / 'terrain-liquid').iterdir() if path.is_file()}
            if expected != actual:
                raise SystemExit('Unexpected terrain-liquid authored products')
        print(f'{"Verified" if args.check else "Generated"} {count} exact-source terrain-liquid files')


if __name__ == '__main__':
    main()
