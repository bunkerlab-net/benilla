#!/usr/bin/env python3
"""Generate authored loaded-byte TerrainInfo area closure from pinned source.

Usage: terrain_area_generate.py /path/to/mangos-classic [--check] [--sanitize]
Existing base geometry, dry-location and wet-liquid products are reconstructed
and compared unchanged.
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

HERE = pathlib.Path(__file__).resolve().parent


def exact_line(source, pattern):
    matches = re.findall(pattern, source, flags=re.M)
    if len(matches) != 1:
        raise SystemExit(f'Expected one source declaration: {pattern}')
    return matches[0]


def terrain_source(root):
    checked = {}
    for entry in (HERE / 'terrain_area_source_hashes.sha256').read_text().splitlines():
        expected, name = entry.split('  ', 1)
        raw = (root / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}')
        checked[pathlib.Path(name).name] = raw.decode('latin1')
    grid, header, stores = [checked[n] for n in ('GridMap.cpp', 'GridMap.h', 'DBCStores.cpp')]
    constants = '\n'.join(exact_line(checked['GridDefines.h'], rf'^#define {name}\s+.*?$')
        for name in ('SIZE_OF_GRIDS', 'MAP_RESOLUTION', 'MAX_NUMBER_OF_GRIDS', 'INVALID_HEIGHT_VALUE'))
    constants += '\n' + grid[grid.index('char const* MAP_MAGIC'):grid.index('GridMap::GridMap()')]
    grid_class = base.method(header, 'class GridMap\n', ';')
    # Access is unchanged: original public loading/query API is enough.
    grid_methods = '\n'.join(base.method(grid, signature) for signature in (
        'GridMap::GridMap()', 'GridMap::~GridMap()', 'bool GridMap::loadData(',
        'void GridMap::unloadData(', 'bool GridMap::loadAreaData(',
        'bool GridMap::loadHeightData(', 'bool GridMap::loadHolesData(',
        'bool GridMap::loadGridMapLiquidData(', 'float GridMap::getHeightFromFlat(',
        'float GridMap::getHeightFromFloat(', 'float GridMap::getHeightFromUint16(',
        'float GridMap::getHeightFromUint8(', 'bool GridMap::isHole(', 'uint16 GridMap::getArea(',
        'GridMap* TerrainInfo::GetGrid(', 'inline bool IsOutdoorWMO(',
        'bool TerrainInfo::GetAreaInfo(', 'uint16 TerrainInfo::GetAreaFlag('))
    dbc_formats = '\n'.join(exact_line(checked['DBCfmt.h'], rf'^const char {name}\[\] = ".*?";$')
        for name in ('AreaTableEntryfmt', 'MapEntryfmt', 'WMOAreaTableEntryfmt'))
    dbc_structs = base.method(checked['DBCEnums.h'], 'enum MapTypes', ';')
    dbc_structs += '\n#pragma pack(push,1)\n'
    dbc_structs += '\n'.join(base.method(checked['DBCStructure.h'], f'struct {name}\n', ';')
        for name in ('AreaTableEntry', 'MapEntry', 'WMOAreaTableEntry'))
    dbc_structs += '\n#pragma pack(pop)\n'
    dbc_methods = '\n'.join(base.method(stores, signature) for signature in (
        'std::vector<WMOAreaTableEntry const*>& GetWMOAreaTableEntriesByTripple(',
        'AreaTableEntry const* GetAreaEntryByAreaID(',
        'AreaTableEntry const* GetAreaEntryByAreaFlagAndMap(', 'uint32 GetAreaFlagByMapId('))
    area_index = stores[stores.index('    // must be after sAreaStore loading'):stores.index('    LoadDBC(availableDbcLocales, bar, bad_dbc_files, sAreaTriggerStore')]
    wmo_index = base.method(stores, 'for (uint32 i = 0; i < sWMOAreaTableStore.GetNumRows(); ++i)')
    replacements = {
        '// PINNED_GRID_CONSTANTS': constants,
        '// PINNED_GRID_CLASS': grid_class,
        '// PINNED_TERRAIN_METHODS': grid_methods,
        '// PINNED_DBC_FORMATS': dbc_formats,
        '// PINNED_DBC_STRUCTS': dbc_structs,
        '// PINNED_DBC_INDEX_TYPES': stores[stores.index('typedef std::map<uint32, uint32> AreaIDByAreaFlag;'):stores.index('DBCStorage <AreaTableEntry> sAreaStore')],
        '// PINNED_DBC_METHODS': dbc_methods,
        '// PINNED_BUILD_DBC_INDICES': area_index + '\n' + wmo_index,
    }
    text = (HERE / 'terrain_area_oracle.cpp').read_text()
    for marker, value in replacements.items():
        if text.count(marker) != 1:
            raise SystemExit(f'Expected one harness marker: {marker}')
        text = text.replace(marker, value)
    return grid[:grid.index('#include')] + text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true',
                        help='Trap undefined float-to-integer casts in the original source')
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('This fixture profile requires little-endian IEEE binary32')
    root = args.source.resolve()
    source = base.guarded_sources(root)
    for line in (HERE / 'scopedlocation_source_hashes.sha256').read_text().splitlines():
        expected, name = line.split('  ', 1)
        data = (root / name).read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}')
        source[name] = data.decode('latin1')
    collision, methods = base.source_fragments(source)
    manager = source[base.VMAP + 'VMapManager2.cpp']
    declaration = '''namespace VMAP { class VMapManager2 { public:
    Vector3 convertPositionToInternalRep(float x, float y, float z) const;
};'''
    manager_declaration = '''namespace VMAP {
typedef std::unordered_map<uint32, StaticMapTree*> InstanceTreeMap;
class VMapManager2 { public:
    InstanceTreeMap iInstanceMapTrees;
    std::unordered_map<std::string, std::unique_ptr<WorldModel>> oracleModels;
    WorldModel* acquireModelInstance(const std::string& base, const std::string& name) {
        auto found = oracleModels.find(name);
        if (found != oracleModels.end()) return found->second.get();
        auto model = std::make_unique<WorldModel>();
        if (!model->readFile(base + name + ".vmo")) return nullptr;
        auto* result = model.get();
        oracleModels.emplace(name, std::move(model));
        return result;
    }
    bool iEnableHeightCalc = true;
    Vector3 convertPositionToInternalRep(float x, float y, float z) const;
    InstanceTreeMap::const_iterator GetMapTree(uint32 mapId) const;
    float getHeight(unsigned int mapId, float x, float y, float z, float maxSearchDist);
    bool getAreaInfo(unsigned int mapId, float x, float y, float& z, uint32& flags,
                     int32& adtId, int32& rootId, int32& groupId) const;
    bool GetLiquidLevel(uint32 mapId, float x, float y, float z, uint8 mask,
                        float& level, float& floor, uint32& type) const;
'''
    manager_declaration += base.method(source[base.VMAP + 'IVMapManager.h'], 'bool isHeightCalcEnabled()')
    manager_declaration += '};'
    if methods.count(declaration) != 1:
        raise SystemExit('Source harness manager declaration drift')
    methods = methods.replace(declaration, manager_declaration)
    methods += '\nnamespace VMAP {\n'
    map_source = source[base.VMAP + 'MapTree.cpp']
    methods += base.method(map_source, 'class LocationInfoCallback', ';')
    methods += base.method(map_source, 'class AreaInfoCallback', ';')
    for signature in ['bool StaticMapTree::getAreaInfo(', 'bool StaticMapTree::GetLocationInfo(', 'float StaticMapTree::getHeight(']:
        methods += base.method(map_source, signature)
    for signature in ['InstanceTreeMap::const_iterator VMapManager2::GetMapTree(',
                      'float VMapManager2::getHeight(', 'uint8 GetLiquidMask(',
                      'bool VMapManager2::GetLiquidLevel(', 'bool VMapManager2::getAreaInfo(']:
        methods += base.method(manager, signature)
    for signature in ['std::string StaticMapTree::getTileFileName(', 'bool StaticMapTree::InitMap(', 'bool StaticMapTree::LoadMapTile(']:
        methods += base.method(map_source, signature)
    methods += base.method(source[base.VMAP + 'TileAssembler.cpp'], 'bool readChunk(')
    methods += '}\n'
    terrain = terrain_source(root)
    target = HERE / 'fixtures'
    with tempfile.TemporaryDirectory(prefix='.terrain-area-oracle-', dir=HERE) as temporary:
        work = pathlib.Path(temporary)
        (work / 'stubs/Platform').mkdir(parents=True)
        (work / 'stubs/Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
#define MANGOS_ENDIAN 0
#define MANGOS_BIG_ENDIAN 1
''')
        (work / 'collision.cpp').write_text(collision)
        authored = (HERE / 'vmap_fixture.cpp').read_text()
        authored = authored.replace('// PINNED_SOURCE_METHODS',
                                    '#include "IVMapManager.h"\n#include "Maps/GridMapDefines.h"\n' + methods)
        authored += '''
static void recordArea(VMAP::StaticMapTree& tree, const std::string& dataset,
                       const std::string& name, const Vector3& world);
'''
        (work / 'authored_vmap.cpp').write_text(authored)
        location = (HERE / 'scopedlocation_fixture.cpp').read_text()
        location = location.replace('int main(int argc, char** argv)', 'int authored_location_main(int argc, char** argv)')
        needle = '    void world(const std::string& name, const Vector3& world, uint8 mask = 0) {'
        if location.count(needle) != 1:
            raise SystemExit('Authored location query hook drift')
        location = location.replace(needle, needle + '\n        recordArea(*tree, dataset, name, world);')
        (work / 'authored_location.cpp').write_text(location)
        liquid = (HERE / 'scopedliquid_fixture.cpp').read_text()
        liquid = liquid.replace('int main(int argc, char** argv)', 'int authored_liquid_main(int argc, char** argv)')
        needle = '    void world(const std::string& name, const Vector3& world, uint8 mask = 0, bool evaluateInstance = true) {'
        if liquid.count(needle) != 1:
            raise SystemExit('Authored liquid query hook drift')
        liquid = liquid.replace(needle, needle + '\n        recordArea(tree, "scopedliquid", name, world);')
        (work / 'authored_liquid.cpp').write_text(liquid)
        scoped = (HERE / 'scopedarea_fixture.cpp').read_text()
        marker = 'int main(int argc, char** argv) {'
        scoped = scoped.replace(marker,
            'static void terrainAreaQueries(const std::filesystem::path&, VMAP::StaticMapTree&, const std::string&);\n' + marker)
        marker = '            serializeTree(directory, tree, spawns);'
        if scoped.count(marker) != 1:
            raise SystemExit('Existing authored area hook drift')
        scoped = scoped.replace(marker, marker + '\n            terrainAreaQueries(root, tree, dataset);')
        (work / 'oracle.cpp').write_text(scoped + '\n' + terrain)
        sources = [root / base.VMAP / f'{name}.cpp' for name in ['BIH', 'WorldModel', 'ModelInstance']]
        sources += [root / base.G3D / f'{name}.cpp' for name in ['Ray', 'Vector3', 'AABox', 'Matrix3', 'g3dmath']]
        sources += [root / 'src/shared/Database/DBCFileLoader.cpp']
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                   '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections', '-DNO_CORE_FUNCS',
                   '-I' + str(work / 'stubs'), '-I' + str(root / base.VMAP), '-I' + str(root / base.G3D),
                   '-I' + str(root / 'src/game'), '-I' + str(root / 'src/shared'), '-I' + str(work),
                   str(work / 'oracle.cpp'), str(work / 'collision.cpp'),
                   *map(str, sources), '-Wl,--gc-sections', '-o', str(work / 'oracle')]
        if args.sanitize:
            command[1:1] = ['-fsanitize=float-cast-overflow', '-fno-sanitize-recover=float-cast-overflow']
        subprocess.run(command, check=True)
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(work / 'oracle'), str(output)], check=True)
        products = []
        unchanged = 0
        for product in sorted(output.rglob('*')):
            if not product.is_file():
                continue
            relative = product.relative_to(output)
            if relative.parts[0] not in ['terrain-area', 'terrain_area.tsv']:
                if product.read_bytes() != (target / 'vmap_source' / relative).read_bytes():
                    raise SystemExit(f'Existing authored product differs: {relative}')
                unchanged += 1
                continue
            products.append(relative)
            destination = target / relative
            if args.check:
                if not destination.is_file() or product.read_bytes() != destination.read_bytes():
                    raise SystemExit(f'Fixture differs: {relative}')
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(product.read_bytes())
        if args.check:
            expected = {'terrain_area.tsv'} | {str(path.relative_to(target)) for path in (target / 'terrain-area').rglob('*') if path.is_file()}
            actual = {str(path) for path in products}
            if actual != expected:
                raise SystemExit('Unexpected terrain-area authored products')
        print(f'Verified {unchanged} existing source products byte-identical')
        print(f'{"Verified" if args.check else "Generated"} {len(products)} source-exact TerrainInfo area products')


if __name__ == '__main__':
    main()
