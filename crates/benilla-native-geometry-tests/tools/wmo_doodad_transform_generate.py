#!/usr/bin/env python3
"""Guarded one-candidate original WMO doodad transform/serialization oracle."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import os
import pathlib
import platform
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
from generate_vmap_fixture import G3D, HERE, PIN, method

EXTRACTOR = 'contrib/vmap_extractor/vmapextract/'
TARGET = HERE / 'fixtures/wmo_doodad_transform_source'


def sources(root):
    result = {}
    for line in (HERE / 'wmo_doodad_transform_source_hashes.sha256').read_text().splitlines():
        expected, name = line.split('  ', 1)
        data = (root / name).read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}; expected CMaNGOS {PIN}')
        result[name] = data.decode('latin1')
    return result


def fragments(source):
    model = source[EXTRACTOR + 'model.cpp']
    result = model[:model.index('#include')]
    exported = source[EXTRACTOR + 'vmapexport.cpp']
    for prefix in ['char szWorkDirWmo[', 'std::map<std::pair<uint32, uint16>, uint32> uniqueObjectIds;']:
        result += next(line for line in exported.splitlines() if line.startswith(prefix)) + '\n'
    result += method(exported, 'uint32 GenerateUniqueObjectId(')
    for signature in ['char* GetPlainName(char*', 'void fixnamen(', 'void fixname2(']:
        result += method(source[EXTRACTOR + 'adtfile.cpp'], signature)
    result += method(source[EXTRACTOR + 'wmo.cpp'], 'WMOInstance::WMOInstance(')
    result += method(model, 'void Doodad::ExtractSet(')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true', help='Trap undefined behavior and float-to-integer overflow')
    args = parser.parse_args()
    if sys.byteorder != 'little' or platform.machine() != 'x86_64':
        raise SystemExit('Requires the documented Linux x86-64 little-endian IEEE binary32 profile')
    root = args.source.resolve()
    source = sources(root)  # Guards precede all compilation/execution.
    extracted = fragments(source)
    with tempfile.TemporaryDirectory(prefix='.wmo-doodad-transform-', dir=HERE) as temporary:
        work = pathlib.Path(temporary)
        (work / 'stubs/Platform').mkdir(parents=True)
        (work / 'stubs/loadlib').mkdir()
        (work / 'stubs/Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
''')
        (work / 'stubs/loadlib/loadlib.h').write_text('#pragma once\n#include "Platform/Define.h"\n')
        (work / 'oracle.cpp').write_text((HERE / 'wmo_doodad_transform_oracle.cpp').read_text().replace('// PINNED_SOURCE_METHODS', extracted))
        compiled = [root / G3D / f'{name}.cpp' for name in ['Quat', 'Matrix3', 'Vector3', 'g3dmath']]
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                   '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections',
                   '-I' + str(work / 'stubs'), '-I' + str(root / G3D),
                   '-I' + str(root / EXTRACTOR), '-I' + str(root / 'dep/libmpq'),
                   str(work / 'oracle.cpp'), *map(str, compiled), '-Wl,--gc-sections', '-o', str(work / 'oracle')]
        if args.sanitize:
            command[1:1] = ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
        subprocess.run(command, check=True)
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(work / 'oracle'), str(output)], check=True)
        products = {path.relative_to(output): path.read_bytes() for path in output.rglob('*') if path.is_file()}
        products[pathlib.Path('source-fragments.cpp.test-fixture')] = extracted.encode()
        provenance = ['# item\tsha256-or-value', 'pin\t' + PIN,
                      'profile\tLinux-x86_64-little-endian-IEEE754-round-nearest-C++17-O0-no-fast-math-no-fp-contract',
                      'allocator\toriginal-GenerateUniqueObjectId-cleared-per-case; no-global-order-claim',
                      'compiled\tQuat.cpp,Matrix3.cpp,Vector3.cpp,g3dmath.cpp unchanged',
                      'compiler\t' + subprocess.run(['g++', '--version'], check=True, text=True, capture_output=True).stdout.splitlines()[0],
                      'libc\t' + os.confstr('CS_GNU_LIBC_VERSION')]
        for name, data in sorted(source.items()):
            provenance.append(name + '\t' + hashlib.sha256(data.encode('latin1')).hexdigest())
        provenance.append('extracted_source\t' + hashlib.sha256(extracted.encode()).hexdigest())
        products[pathlib.Path('provenance.tsv')] = ('\n'.join(provenance) + '\n').encode()
        if args.check:
            existing = {path.relative_to(TARGET) for path in TARGET.rglob('*') if path.is_file()}
            if existing != set(products):
                raise SystemExit(f'Fixture file set differs: {existing ^ set(products)}')
        for name, data in products.items():
            destination = TARGET / name
            if args.check:
                if not destination.is_file() or destination.read_bytes() != data:
                    raise SystemExit(f'Fixture differs: {name}')
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
        print(f'{"Verified" if args.check else "Generated"} {len(products)} authored one-candidate WMO doodad oracle files')


if __name__ == '__main__':
    main()
