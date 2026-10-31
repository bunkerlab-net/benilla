#!/usr/bin/env python3
"""Generate authored wet liquid expectations from pinned source, offline.

Usage: scopedliquid_generate.py /path/to/mangos-classic [--check] [--sanitize]
Existing ray/height and dry-location products are reconstructed and compared unchanged.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import pathlib
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
import generate_vmap_fixture as base

HERE = pathlib.Path(__file__).resolve().parent


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
    bool iEnableHeightCalc = true;
    Vector3 convertPositionToInternalRep(float x, float y, float z) const;
    InstanceTreeMap::const_iterator GetMapTree(uint32 mapId) const;
    float getHeight(unsigned int mapId, float x, float y, float z, float maxSearchDist);
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
    for signature in ['bool StaticMapTree::GetLocationInfo(', 'float StaticMapTree::getHeight(']:
        methods += base.method(map_source, signature)
    for signature in ['InstanceTreeMap::const_iterator VMapManager2::GetMapTree(',
                      'float VMapManager2::getHeight(', 'uint8 GetLiquidMask(',
                      'bool VMapManager2::GetLiquidLevel(']:
        methods += base.method(manager, signature)
    methods += '}\n'
    target = HERE / 'fixtures/vmap_source'
    with tempfile.TemporaryDirectory(prefix='.scopedliquid-oracle-', dir=HERE) as temporary:
        work = pathlib.Path(temporary)
        (work / 'stubs/Platform').mkdir(parents=True)
        (work / 'stubs/Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
''')
        (work / 'collision.cpp').write_text(collision)
        authored = (HERE / 'vmap_fixture.cpp').read_text()
        authored = authored.replace('// PINNED_SOURCE_METHODS',
                                    '#include "IVMapManager.h"\n#include "Maps/GridMapDefines.h"\n' + methods)
        (work / 'authored_vmap.cpp').write_text(authored)
        location = (HERE / 'scopedlocation_fixture.cpp').read_text()
        location = location.replace('int main(int argc, char** argv)', 'int authored_location_main(int argc, char** argv)')
        (work / 'authored_location.cpp').write_text(location)
        sources = [root / base.VMAP / f'{name}.cpp' for name in ['BIH', 'WorldModel', 'ModelInstance']]
        sources += [root / base.G3D / f'{name}.cpp' for name in ['Ray', 'Vector3', 'AABox', 'Matrix3', 'g3dmath']]
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                   '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections', '-DNO_CORE_FUNCS',
                   '-I' + str(work / 'stubs'), '-I' + str(root / base.VMAP), '-I' + str(root / base.G3D),
                   '-I' + str(root / 'src/game'), '-I' + str(work),
                   str(HERE / 'scopedliquid_fixture.cpp'), str(work / 'collision.cpp'),
                   *map(str, sources), '-Wl,--gc-sections', '-o', str(work / 'oracle')]
        if args.sanitize:
            command[1:1] = ['-fsanitize=float-cast-overflow', '-fno-sanitize-recover=float-cast-overflow']
        subprocess.run(command, check=True)
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(work / 'oracle'), str(output)], check=True)
        products = []
        for product in sorted(output.rglob('*')):
            if not product.is_file():
                continue
            relative = product.relative_to(output)
            if relative.parts[0] not in ['scopedliquid', 'scopedliquid.tsv', 'scopedliquid_direct.tsv', 'scopedliquid_sizes.tsv']:
                if product.read_bytes() != (target / relative).read_bytes():
                    raise SystemExit(f'Existing authored product differs: {relative}')
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
            expected = {str(path.relative_to(target)) for path in (target / 'scopedliquid').iterdir() if path.is_file()}
            actual = {str(path) for path in products if path.parts[0] == 'scopedliquid'}
            if actual != expected:
                raise SystemExit('Unexpected scopedliquid authored products')
        print(f'{"Verified" if args.check else "Generated"} {len(products)} source-exact scopedliquid products')


if __name__ == '__main__':
    main()
