#!/usr/bin/env python3
"""Generate exact bounded VMap height expectations against pinned source, offline.

Usage: scopedheight_generate.py /path/to/mangos-classic [--check]
Existing authored products are regenerated only in a temporary directory and compared.
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
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('This fixture profile requires little-endian IEEE binary32')
    root = args.source.resolve()
    source = base.guarded_sources(root)
    for line in (HERE / 'scopedheight_source_hashes.sha256').read_text().splitlines():
        expected, name = line.split('  ', 1)
        data = (root / name).read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}')
        source[name] = data.decode('latin1')
    collision, methods = base.source_fragments(source)
    manager = source[base.VMAP + 'VMapManager2.cpp']
    header = source[base.VMAP + 'IVMapManager.h']
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
'''
    manager_declaration += base.method(header, 'bool isHeightCalcEnabled()')
    manager_declaration += '};'
    if methods.count(declaration) != 1:
        raise SystemExit('Source harness manager declaration drift')
    methods = methods.replace(declaration, manager_declaration)
    methods += '\nnamespace VMAP {\n'
    methods += base.method(source[base.VMAP + 'MapTree.cpp'], 'float StaticMapTree::getHeight(')
    methods += base.method(manager, 'InstanceTreeMap::const_iterator VMapManager2::GetMapTree(')
    methods += base.method(manager, 'float VMapManager2::getHeight(')
    methods += '}\n'
    target = HERE / 'fixtures/vmap_source'
    with tempfile.TemporaryDirectory(prefix='.scopedheight-oracle-', dir=HERE) as temporary:
        work = pathlib.Path(temporary)
        (work / 'stubs/Platform').mkdir(parents=True)
        (work / 'stubs/Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
''')
        (work / 'collision.cpp').write_text(collision)
        authored = (HERE / 'vmap_fixture.cpp').read_text()
        authored = authored.replace('// PINNED_SOURCE_METHODS', '#include "IVMapManager.h"\n' + methods)
        (work / 'authored_vmap.cpp').write_text(authored)
        sources = [root / base.VMAP / f'{name}.cpp' for name in ['BIH', 'WorldModel', 'ModelInstance']]
        sources += [root / base.G3D / f'{name}.cpp' for name in ['Ray', 'Vector3', 'AABox', 'Matrix3', 'g3dmath']]
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                   '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections', '-DNO_CORE_FUNCS',
                   '-I' + str(work / 'stubs'), '-I' + str(root / base.VMAP), '-I' + str(root / base.G3D),
                   '-I' + str(work), str(HERE / 'scopedheight_fixture.cpp'), str(work / 'collision.cpp'),
                   *map(str, sources), '-Wl,--gc-sections', '-o', str(work / 'oracle')]
        subprocess.run(command, check=True)
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(work / 'oracle'), str(output)], check=True)
        # Height expectations must refer to the exact existing byte-loaded fixtures.
        for product in output.iterdir():
            if product.name != 'scopedheight.tsv' and product.read_bytes() != (target / product.name).read_bytes():
                raise SystemExit(f'Existing authored product differs: {product.name}')
        actual = (output / 'scopedheight.tsv').read_bytes()
        destination = target / 'scopedheight.tsv'
        if args.check:
            if actual != destination.read_bytes():
                raise SystemExit('Fixture differs: scopedheight.tsv')
        else:
            destination.write_bytes(actual)
        print(f'{"Verified" if args.check else "Generated"} source-exact scopedheight.tsv')


if __name__ == '__main__':
    main()
