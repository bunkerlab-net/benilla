#!/usr/bin/env python3
"""Generate complete wet TerrainInfo liquid closure through original readers.

Usage: terrain_wet_liquid_generate.py /path/to/mangos-classic [--check] [--sanitize]
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
import terrain_area_generate as area

HERE = pathlib.Path(__file__).resolve().parent


def exact_line(source, pattern):
    matches = re.findall(pattern, source, flags=re.M)
    if len(matches) != 1:
        raise SystemExit(f'Expected one source declaration: {pattern}')
    return matches[0]


def terrain_source(root):
    # Reuse the already hash-guarded original area/load/index closure unchanged.
    text = area.terrain_source(root)
    grid = (root / 'src/game/Maps/GridMap.cpp').read_text(encoding='latin1')
    defines = (root / 'src/game/Maps/GridDefines.h').read_text(encoding='latin1')
    structure = (root / 'src/game/Server/DBCStructure.h').read_text(encoding='latin1')
    formats = (root / 'src/game/Server/DBCfmt.h').read_text(encoding='latin1')
    extra = '\n'.join(area.exact_line(defines, rf'^#define {name}\s+.*?$')
                      for name in ('INVALID_HEIGHT', 'DEFAULT_HEIGHT_SEARCH', 'DEFAULT_WATER_SEARCH'))
    extra += '\n' + area.exact_line(formats, r'^const char LiquidTypefmt\[\] = ".*?";$')
    extra += '\n#pragma pack(push,1)\n' + base.method(structure, 'struct LiquidTypeEntry\n', ';')
    extra += '\n#pragma pack(pop)\nstatic DBCStorage<LiquidTypeEntry> sLiquidTypeStore(LiquidTypefmt);\n'
    text = text.replace('struct TerrainInfo {', extra + '\nstruct TerrainInfo {', 1)
    text = text.replace('    uint32 m_mapId;', '''    float GetHeightStatic(float, float, float, bool = true, float = DEFAULT_HEIGHT_SEARCH) const;
    GridMapLiquidStatus getLiquidStatus(float, float, float, uint8, GridMapLiquidData*, float) const;
    uint32 m_mapId;''', 1)
    text += '\n' + '\n'.join(base.method(grid, signature) for signature in (
        'GridMapLiquidStatus GridMap::getLiquidStatus(', 'float TerrainInfo::GetHeightStatic(',
        'GridMapLiquidStatus TerrainInfo::getLiquidStatus('))
    stores = (root / 'src/game/Server/DBCStores.cpp').read_text(encoding='latin1')
    indices = stores[stores.index('    // must be after sAreaStore loading'):stores.index('    LoadDBC(availableDbcLocales, bar, bad_dbc_files, sAreaTriggerStore')]
    indices += '\n' + base.method(stores, 'for (uint32 i = 0; i < sWMOAreaTableStore.GetNumRows(); ++i)')
    authored = (HERE / 'terrain_wet_liquid_oracle.cpp').read_text()
    if authored.count('// PINNED_WET_BUILD_DBC_INDICES') != 1:
        raise SystemExit('Wet source index marker drift')
    return text + '\n' + authored.replace('// PINNED_WET_BUILD_DBC_INDICES', indices)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true',
                        help='Trap undefined shifts and float-to-integer casts in the original source')
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
    with tempfile.TemporaryDirectory(prefix='.terrain-wet-liquid-oracle-', dir=HERE) as temporary:
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
        scoped = scoped.replace('int main(int argc, char** argv)', 'int authored_area_main(int argc, char** argv)')
        (work / 'oracle.cpp').write_text(scoped + '\n' + terrain)
        sources = [root / base.VMAP / f'{name}.cpp' for name in ['BIH', 'WorldModel', 'ModelInstance']]
        sources += [root / base.G3D / f'{name}.cpp' for name in ['Ray', 'Vector3', 'AABox', 'Matrix3', 'g3dmath']]
        sources += [root / 'src/shared/Database/DBCFileLoader.cpp']
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                   '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections', '-DNO_CORE_FUNCS',
                   '-I' + str(work / 'stubs'), '-I' + str(root / base.VMAP), '-I' + str(root / base.G3D),
                   '-I' + str(root / 'src/game'), '-I' + str(root / 'src/shared'), '-I' + str(work),
                   str(work / 'oracle.cpp'), str(work / 'collision.cpp'),
                   *map(str, sources), '-Wl,--gc-sections', '-o', str(work / 'oracle.test-fixture')]
        if args.sanitize:
            command[1:1] = ['-fsanitize=shift,float-cast-overflow', '-fno-sanitize-recover=shift,float-cast-overflow']
        subprocess.run(command, check=True)
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(work / 'oracle.test-fixture'), str(output)], check=True)
        products = []
        unchanged = 0
        for product in sorted(output.rglob('*')):
            if not product.is_file():
                continue
            relative = product.relative_to(output)
            if relative.parts[0] not in ['terrain-wet-liquid', 'terrain-wet-liquid.tsv', 'terrain-wet-vmap']:
                previous = target / relative if relative.parts[0] in ['terrain-area', 'terrain_area.tsv'] else target / 'vmap_source' / relative
                if product.read_bytes() != previous.read_bytes():
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
            expected = {'terrain-wet-liquid.tsv'} | {str(path.relative_to(target)) for directory in ['terrain-wet-liquid', 'terrain-wet-vmap'] for path in (target / directory).rglob('*') if path.is_file()}
            actual = {str(path) for path in products}
            if actual != expected:
                raise SystemExit('Unexpected terrain-wet-liquid authored products')
        if unchanged != 93 or len(products) != 111:
            raise SystemExit(f'Unexpected product counts: old={unchanged}, new={len(products)}')
        print(f'Verified {unchanged} existing source products byte-identical')
        print(f'{"Verified" if args.check else "Generated"} {len(products)} source-exact wet TerrainInfo liquid products')


if __name__ == '__main__':
    main()
