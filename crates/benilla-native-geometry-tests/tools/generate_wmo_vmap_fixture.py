#!/usr/bin/env python3
"""Generate authored WMO input -> original extractor -> assembler -> query fixtures.

Offline; source must be CMaNGOS Classic at the guarded pin. No install assets.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import pathlib
import platform
import subprocess
import sys
import tempfile

from generate_vmap_fixture import G3D, HERE, PIN, VMAP, guarded_sources, method

EXTRACTOR = 'contrib/vmap_extractor/vmapextract/'
CAPTURED = HERE / 'fixtures/captured_wmo_source'


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


def captured_fragments(source):
    """Additional unchanged placement methods; keep the older fixture untouched."""
    exported = source[EXTRACTOR + 'vmapexport.cpp']
    result = next(line for line in exported.splitlines()
                  if line.startswith('std::map<std::pair<uint32, uint16>, uint32> uniqueObjectIds;')) + '\n'
    result += method(exported, 'uint32 GenerateUniqueObjectId(')
    result += method(source[EXTRACTOR + 'wmo.cpp'], 'WMOInstance::WMOInstance(')
    return result


def captured_metadata(source, extracted, products):
    products[pathlib.Path('source-fragments.cpp.test-fixture')] = extracted.encode()
    lines = ['# item\tsha256-or-value', 'pin\t' + PIN,
             'profile\tLinux-x86_64-little-endian-IEEE754-round-nearest-C++17-O0-no-fast-math-no-fp-contract',
             'acquisition\tclosed authored root/group/MODF map; no archive, install, Rust or production bytes',
             'extraction\toriginal ExtractSingleWmo, WMORoot, WMOGroup, WMOInstance and GenerateUniqueObjectId',
             'allocator\toriginal empty allocator; client 0x89abcdef and doodad 0 produce ID 1',
             'placement\tmap 0; dir_bin tile 30,31; Hull.wmo; internal position 16600,16100,4; scale 1',
             'raw-offset-8\toriginal accumulated collision triangle count 1, not vertex count 3']
    for name, data in sorted(source.items()):
        lines.append(name + '\t' + hashlib.sha256(data.encode('latin1')).hexdigest())
    for name, data in sorted(products.items()):
        lines.append(str(name) + '\t' + hashlib.sha256(data).hexdigest())
    products[pathlib.Path('provenance.tsv')] = ('\n'.join(lines) + '\n').encode()
    products[pathlib.Path('README.md')] = f"""# Captured dry WMO source oracle

SPDX-License-Identifier: GPL-3.0-or-later

These are entirely authored numeric inputs and unchanged original CMaNGOS
outputs, with no game-install assets. The guarded source pin is `{PIN}`.
Regenerate using `python3 crates/benilla-native-geometry-tests/tools/generate_wmo_vmap_fixture.py /path/to/cmangos-source`;
add `--check` to re-execute and compare every byte. The existing six-group oracle
is generated and checked separately without changing its fixture bytes.

- `hull.root.test-fixture`: authored version-17 root, one group, root ID 5875,
  no doodads or liquid, bounds [-1,-2,-3] through [4,5,6]
- `hull_000.group.test-fixture`: authored group ID 23, flags 0x2000, vertices
  [-1,-2,-3], [4,0,6], [0,5,1], indices [0,1,2], one collision MOPY entry,
  one MOBA record with word 8 equal to 1, matching bounds
- `hull.modf.test-fixture`: full 64-byte authored MODF including name index 0,
  client ID 0x89abcdef, position [16100,4,16600], zero rotation, bounds
  [16098,1,16599] through [16105,10,16604], flags/doodad-set 0, name-set 11,
  scale field 1024; the original ADT caller's four-byte name-index read is
  represented by seeking to offset 4 before the original WMOInstance constructor
- `hull.raw.vmap`: actual original ExtractSingleWmo output; offset 8 is its
  collision triangle count 1, not an authored replacement or vertex count 3
- `dir_bin`: actual original WMOInstance output for map 0, tile 30,31;
  GenerateUniqueObjectId starts with an empty allocator and allocates ID 1;
  original internal position [16600,16100,4], scale 1 and plain `Hull.wmo`
- `source-fragments.cpp.test-fixture`: exact hash-guarded original methods
  compiled by the harness, including the placement constructor and allocator
- `provenance.tsv`: pin, execution profile, original-source and captured-byte hashes

The only input substitution is the closed authored MPQFile acquisition map.
The no-doodad guard fails if model extraction is unexpectedly attempted. No
source behavior, ID allocator, model filename, placement, or raw output is patched.
`generate_map_products.py` first verifies this capture by re-executing the original
source, then supplies its raw and directory bytes to unchanged original
TileAssembler and public VMapManager2 queries. Its `captured_wmo` row in
`map-products.tsv` contains all three assembled products; the corresponding 66
`map-queries.tsv` rows cover loading, coordinates, heights, LOS and object hits.
The public load uses tile 31,30 because the original tile filename helper
transposes the dir_bin tile coordinates. Only the natural plain model lookup is
used for this capture; the older shortened-name fixtures are independent.
""".encode()


def publish(target, products, check):
    if check:
        existing = {path.relative_to(target) for path in target.rglob('*') if path.is_file()}
        if existing != set(products):
            raise SystemExit(f'Fixture file set differs in {target}: {existing ^ set(products)}')
    for name, data in products.items():
        destination = target / name
        if check:
            if not destination.is_file() or destination.read_bytes() != data:
                raise SystemExit(f'Fixture differs: {destination}')
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
    print(f'{"Verified" if check else "Generated"} {len(products)} authored WMO oracle files in {target}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if sys.platform != 'linux' or sys.byteorder != 'little' or platform.machine() != 'x86_64':
        raise SystemExit('Requires Linux x86-64 little-endian IEEE binary32')
    root = args.source.resolve()
    source = sources(root)  # Hash guards precede every subprocess.
    revision = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True, timeout=10).strip()
    if revision != PIN:
        raise SystemExit(f'Wrong source revision: expected {PIN}, got {revision}')
    extracted, collision = fragments(source)
    captured = captured_fragments(source)
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
        (work / 'oracle.cpp').write_text((HERE / 'wmo_vmap_fixture.cpp').read_text().replace('// PINNED_SOURCE_METHODS', extracted).replace('// CAPTURED_SOURCE_METHODS', captured))
        (work / 'collision.cpp').write_text(collision)
        compiled = [root / VMAP / f'{name}.cpp' for name in ['BIH', 'WorldModel', 'ModelInstance', 'TileAssembler']]
        compiled += [root / G3D / f'{name}.cpp' for name in ['Ray', 'Vector3', 'AABox', 'Matrix3', 'g3dmath']]
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                   '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections', '-DNO_CORE_FUNCS',
                   '-I' + str(work / 'stubs'), '-I' + str(root / VMAP), '-I' + str(root / G3D),
                   '-I' + str(root / EXTRACTOR), '-I' + str(root / 'dep/libmpq'), str(work / 'oracle.cpp'), str(work / 'collision.cpp'),
                   *map(str, compiled), '-Wl,--gc-sections', '-o', str(work / 'oracle')]
        subprocess.run(command, check=True, timeout=120)
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(work / 'oracle'), str(output)], check=True, timeout=30)
        products = {path.relative_to(output): path.read_bytes() for path in output.rglob('*') if path.is_file()}
        products[pathlib.Path('source-fragments.cpp.test-fixture')] = (extracted + collision).encode()
        publish(target, products, args.check)
        output = work / 'captured'
        output.mkdir()
        subprocess.run([str(work / 'oracle'), str(output), 'captured'], check=True, timeout=30)
        products = {path.relative_to(output): path.read_bytes() for path in output.rglob('*') if path.is_file()}
        captured_metadata(source, extracted + captured + collision, products)
        publish(CAPTURED, products, args.check)


if __name__ == '__main__':
    main()
