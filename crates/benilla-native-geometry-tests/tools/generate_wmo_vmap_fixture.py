#!/usr/bin/env python3
"""Generate authored WMO input -> original extractor -> assembler -> query fixtures.

Offline; source must be CMaNGOS Classic at the guarded pin. No install assets.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import pathlib
import subprocess
import sys
import tempfile

from generate_vmap_fixture import G3D, HERE, PIN, VMAP, guarded_sources, method

EXTRACTOR = 'contrib/vmap_extractor/vmapextract/'


def sources(root):
    source = guarded_sources(root)
    for line in (HERE / 'wmo_vmap_source_hashes.sha256').read_text().splitlines():
        expected, name = line.split('  ', 1)
        data = (root / name).read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}; expected CMaNGOS {PIN}')
        source[name] = data.decode('latin1')
    return source


def fragments(source):
    wmo = source[EXTRACTOR + 'wmo.cpp']
    extracted = wmo[:wmo.index('#include')]
    exported = source[EXTRACTOR + 'vmapexport.cpp']
    for prefix in ['const char* szRawVMAPMagic =', 'char szWorkDirWmo[',
                   'bool preciseVectorData =', 'std::unordered_map<std::string, WMODoodadData> WmoDoodads;']:
        extracted += next(line for line in exported.splitlines() if line.startswith(prefix)) + '\n'
    adt = source[EXTRACTOR + 'adtfile.cpp']
    for signature in ['char* GetPlainName(char*', 'void fixnamen(', 'void fixname2(']:
        extracted += method(adt, signature)
    for signature in ['WMORoot::WMORoot(', 'bool WMORoot::open(',
                      'bool WMORoot::ConvertToVMAPRootWmo(', 'WMORoot::~WMORoot(',
                      'WMOGroup::WMOGroup(', 'bool WMOGroup::open(',
                      'int WMOGroup::ConvertToVMAPGroupWmo(',
                      'bool WMOGroup::ShouldSkip(', 'WMOGroup::~WMOGroup(']:
        extracted += method(wmo, signature)
    for signature in ['bool FileExists(', 'bool ExtractSingleWmo(']:
        extracted += method(exported, signature)
    tree = source[VMAP + 'MapTree.cpp']
    extracted += tree[:tree.index('#include')] + 'namespace VMAP {\n'
    for signature in ['class MapRayCallback', 'class AreaInfoCallback', 'class LocationInfoCallback']:
        extracted += method(tree, signature, ';')
    for signature in ['StaticMapTree::StaticMapTree(', 'StaticMapTree::~StaticMapTree(',
                      'bool StaticMapTree::getIntersectionTime(', 'float StaticMapTree::getHeight(',
                      'bool StaticMapTree::getAreaInfo(', 'bool StaticMapTree::GetLocationInfo(']:
        extracted += method(tree, signature)
    extracted += '}\n'
    extracted += '''namespace VMAP { class VMapManager2 { public:
    Vector3 convertPositionToInternalRep(float x, float y, float z) const;
};
'''
    extracted += method(source[VMAP + 'VMapManager2.cpp'], 'Vector3 VMapManager2::convertPositionToInternalRep(')
    extracted += '}\n'
    collision = source[G3D + 'CollisionDetection.cpp']
    aa = collision[:collision.index('#include')]
    aa += '#include "G3D/CollisionDetection.h"\n#include "G3D/AABox.h"\nnamespace G3D {\nVector3 CollisionDetection::ignore;\n'
    for signature in ['float CollisionDetection::collisionTimeForMovingPointFixedAABox(',
                      'bool CollisionDetection::collisionLocationForMovingPointFixedAABox(']:
        aa += method(collision, signature)
    aa += '}\n'
    return extracted, aa


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('Requires little-endian IEEE binary32')
    root = args.source.resolve()
    source = sources(root)
    extracted, collision = fragments(source)
    target = HERE / 'fixtures/wmo_vmap_source'
    with tempfile.TemporaryDirectory(prefix='.wmo-vmap-oracle-', dir=HERE) as temporary:
        work = pathlib.Path(temporary)
        (work / 'stubs/Platform').mkdir(parents=True)
        (work / 'stubs/loadlib').mkdir()
        (work / 'stubs/Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
''')
        (work / 'stubs/loadlib/loadlib.h').write_text('#pragma once\n#include "Platform/Define.h"\n')
        (work / 'oracle.cpp').write_text((HERE / 'wmo_vmap_fixture.cpp').read_text().replace('// PINNED_SOURCE_METHODS', extracted))
        (work / 'collision.cpp').write_text(collision)
        compiled = [root / VMAP / f'{name}.cpp' for name in ['BIH', 'WorldModel', 'ModelInstance', 'TileAssembler']]
        compiled += [root / G3D / f'{name}.cpp' for name in ['Ray', 'Vector3', 'AABox', 'Matrix3', 'g3dmath']]
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                   '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections', '-DNO_CORE_FUNCS',
                   '-I' + str(work / 'stubs'), '-I' + str(root / VMAP), '-I' + str(root / G3D),
                   '-I' + str(root / EXTRACTOR), '-I' + str(root / 'dep/libmpq'), str(work / 'oracle.cpp'), str(work / 'collision.cpp'),
                   *map(str, compiled), '-Wl,--gc-sections', '-o', str(work / 'oracle')]
        subprocess.run(command, check=True)
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(work / 'oracle'), str(output)], check=True)
        products = {path.relative_to(output): path.read_bytes() for path in output.rglob('*') if path.is_file()}
        products[pathlib.Path('source-fragments.cpp.test-fixture')] = (extracted + collision).encode()
        if args.check:
            existing = {path.relative_to(target) for path in target.rglob('*') if path.is_file()}
            if existing != set(products):
                raise SystemExit(f'Fixture file set differs: {existing ^ set(products)}')
        for name, data in products.items():
            destination = target / name
            if args.check:
                if not destination.is_file() or destination.read_bytes() != data:
                    raise SystemExit(f'Fixture differs: {name}')
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
        print(f'{"Verified" if args.check else "Generated"} {len(products)} authored WMO oracle files in {target}')


if __name__ == '__main__':
    main()
