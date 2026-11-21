#!/usr/bin/env python3
"""Build the pinned original-source reader for private captured location inputs."""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import sys

sys.dont_write_bytecode = True
import generate_vmap_fixture as base


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    source = args.source.resolve()
    work = args.output.resolve()
    checked = base.guarded_sources(source)
    collision, _ = base.source_fragments(checked)
    work.mkdir(parents=True)
    (work/'stubs/Platform').mkdir(parents=True)
    (work/'stubs/Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
''')
    (work/'collision.cpp').write_text(collision)
    chunk = base.method(checked[base.VMAP+'TileAssembler.cpp'], 'bool readChunk(')
    (work/'read_chunk.cpp').write_text('#include <cstdio>\n#include <cstring>\n#include "Platform/Define.h"\nnamespace VMAP {\n'+chunk+'}\n')
    sources = [source/base.VMAP/f'{name}.cpp' for name in ['BIH','WorldModel','ModelInstance']]
    sources += [source/base.G3D/f'{name}.cpp' for name in ['Ray','Vector3','AABox','Matrix3','g3dmath']]
    command = ['g++','-std=c++17','-O0','-fno-fast-math','-ffp-contract=off','-fno-strict-aliasing',
               '-ffunction-sections','-fdata-sections','-DNO_CORE_FUNCS',
               '-I'+str(work/'stubs'),'-I'+str(source/base.VMAP),'-I'+str(source/base.G3D),
               str(Path(__file__).with_name('captured_location_oracle.cpp')),
               str(work/'collision.cpp'),str(work/'read_chunk.cpp'),*map(str,sources),
               '-Wl,--gc-sections','-o',str(work/'oracle')]
    print(shlex.join(command), flush=True)
    subprocess.run(command, check=True)
    (work/'build.json').write_text(json.dumps({'source_pin':base.PIN,'command':command,
        'source_hashes':{name:hashlib.sha256(text.encode('latin1')).hexdigest() for name,text in checked.items()},
        'oracle_sha256':hashlib.sha256((work/'oracle').read_bytes()).hexdigest()},indent=2)+'\n')


if __name__ == '__main__':
    main()
