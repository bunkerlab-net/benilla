#!/usr/bin/env python3
"""Guarded authored WMO liquid extraction and original assembly, offline."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import difflib
import hashlib
import pathlib
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
import generate_wmo_vmap_fixture as dry
from generate_vmap_fixture import G3D, HERE, PIN, VMAP, method

POLICY = 'WMO_LIQUID_HEADER_ZERO_PADDING_V1'
ALLOCATION = '            hlq = new WMOLiquidHeader;\n'
INITIALIZED = ALLOCATION + '''            // WMO_LIQUID_HEADER_ZERO_PADDING_V1: only ABI tail padding.
            reinterpret_cast<unsigned char*>(hlq)[30] = 0;
            reinterpret_cast<unsigned char*>(hlq)[31] = 0;
'''


def sources(root):
    result = dry.sources(root)
    for line in (HERE / 'wmo_liquid_source_hashes.sha256').read_text().splitlines():
        expected, name = line.split('  ', 1)
        data = (root / name).read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}; expected CMaNGOS {PIN}')
        result[name] = data.decode('latin1')
    return result


def fragments(source):
    original = source[dry.EXTRACTOR + 'wmo.cpp']
    if original.count(ALLOCATION) != 1:
        raise SystemExit('Liquid header allocation drift')
    patched = original.replace(ALLOCATION, INITIALIZED)
    patched_sources = dict(source)
    patched_sources[dry.EXTRACTOR + 'wmo.cpp'] = patched
    extracted, collision = dry.fragments(patched_sources)
    declaration = '''namespace VMAP { class VMapManager2 { public:
    Vector3 convertPositionToInternalRep(float x, float y, float z) const;
};'''
    replacement = '''namespace VMAP {
typedef std::unordered_map<uint32, StaticMapTree*> InstanceTreeMap;
class VMapManager2 { public:
    InstanceTreeMap iInstanceMapTrees;
    Vector3 convertPositionToInternalRep(float x, float y, float z) const;
    bool GetLiquidLevel(uint32 mapId, float x, float y, float z, uint8 mask,
                        float& level, float& floor, uint32& type) const;
};'''
    if extracted.count(declaration) != 1:
        raise SystemExit('Manager declaration drift')
    extracted = extracted.replace(declaration, replacement)
    extracted += '\nnamespace VMAP {\n'
    for signature in ['uint8 GetLiquidMask(', 'bool VMapManager2::GetLiquidLevel(']:
        extracted += method(source[VMAP + 'VMapManager2.cpp'], signature)
    extracted += '}\n'
    patch = ''.join(difflib.unified_diff(original.splitlines(keepends=True), patched.splitlines(keepends=True),
        fromfile='a/' + dry.EXTRACTOR + 'wmo.cpp', tofile='b/' + dry.EXTRACTOR + 'wmo.cpp'))
    return extracted, collision, patch, patched


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true', help='Trap undefined original floating-point integer conversions')
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('Requires little-endian IEEE binary32')
    root = args.source.resolve()
    source = sources(root)
    extracted, collision, patch, patched = fragments(source)
    target = HERE / 'fixtures/wmo_liquid_source'
    with tempfile.TemporaryDirectory(prefix='.wmo-liquid-oracle-', dir=HERE) as temporary:
        work = pathlib.Path(temporary)
        (work / 'stubs/Platform').mkdir(parents=True)
        (work / 'stubs/loadlib').mkdir()
        (work / 'stubs/Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
''')
        (work / 'stubs/loadlib/loadlib.h').write_text('#pragma once\n#include "Platform/Define.h"\n')
        (work / 'oracle.cpp').write_text((HERE / 'wmo_liquid_fixture.cpp').read_text().replace('// PINNED_SOURCE_METHODS', extracted))
        (work / 'collision.cpp').write_text(collision)
        # Include the complete unchanged assembler translation unit; append only
        # read-only ABI inspection of its otherwise TU-local header declaration.
        (work / 'assembler.cpp').write_text('#include "' + str(root / VMAP / 'TileAssembler.cpp') + '"\n' + '''
#include <cstddef>
extern "C" void assemblerLiquidLayout(size_t* out) {
    using H = VMAP::WMOLiquidHeader;
    const size_t values[] = {sizeof(H), alignof(H), offsetof(H,xverts), offsetof(H,yverts),
        offsetof(H,xtiles), offsetof(H,ytiles), offsetof(H,pos_x), offsetof(H,pos_y),
        offsetof(H,pos_z), offsetof(H,type), sizeof(H::type)};
    for (size_t i=0;i<11;++i) out[i]=values[i];
}
''')
        compiled = [root / VMAP / f'{name}.cpp' for name in ['BIH', 'WorldModel', 'ModelInstance']]
        compiled += [root / G3D / f'{name}.cpp' for name in ['Ray', 'Vector3', 'AABox', 'Matrix3', 'g3dmath']]
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                   '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections', '-DNO_CORE_FUNCS',
                   '-I' + str(work / 'stubs'), '-I' + str(root / VMAP), '-I' + str(root / G3D),
                   '-I' + str(root / dry.EXTRACTOR), '-I' + str(root / 'dep/libmpq'),
                   '-I' + str(root / 'src/game'), str(work / 'oracle.cpp'), str(work / 'collision.cpp'),
                   str(work / 'assembler.cpp'), *map(str, compiled), '-Wl,--gc-sections', '-o', str(work / 'oracle')]
        if args.sanitize:
            command[1:1] = ['-fsanitize=float-cast-overflow', '-fno-sanitize-recover=float-cast-overflow']
        subprocess.run(command, check=True)
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(work / 'oracle'), str(output)], check=True)
        products = {path.relative_to(output): path.read_bytes() for path in output.rglob('*') if path.is_file()}
        products[pathlib.Path('source-fragments.cpp.test-fixture')] = (extracted + collision).encode()
        products[pathlib.Path('padding-v1.patch.test-fixture')] = patch.encode()
        products[pathlib.Path('provenance.tsv')] = ('# item\tsha256-or-value\n'
            + 'pin\t' + PIN + '\npolicy\t' + POLICY + '\n'
            + 'original_wmo.cpp\t' + hashlib.sha256(source[dry.EXTRACTOR + 'wmo.cpp'].encode('latin1')).hexdigest() + '\n'
            + 'padding_patch\t' + hashlib.sha256(patch.encode()).hexdigest() + '\n'
            + 'patched_wmo.cpp\t' + hashlib.sha256(patched.encode('latin1')).hexdigest() + '\n').encode()
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
        print(f'{"Verified" if args.check else "Generated"} {len(products)} authored liquid oracle files')


if __name__ == '__main__':
    main()
