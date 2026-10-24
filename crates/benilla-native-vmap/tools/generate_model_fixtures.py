#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Authored raw model edge cases through the unchanged pinned CMaNGOS assembler.

The oracle compiles complete original TileAssembler.cpp, WorldModel.cpp, BIH.cpp
and required G3D translation units. It calls TileAssembler::convertRawFile, so
expected VMO bytes include the original raw loader, object construction, BIH and
serializer. Only Platform/Define.h integer aliases are supplied by the harness.
The authored input bytes are not game assets or captured install data. This is a
little-endian IEEE binary32 source-format oracle, not an extractor or admission
oracle; it does not invoke a Rust writer or inspect any listfile/compiler patch.
"""
import argparse
import hashlib
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
HERE = Path(__file__).resolve().parent


def array(tag, count, payload):
    return tag + struct.pack('<II', 4 + len(payload), count) + payload


def liquid():
    """One authored 1x2 grid; source chunk sizes intentionally misstate lengths."""
    x_tiles, y_tiles = 1, 2
    heights = [3.0, 4.5, -2.0, 9.0, 0.0, 16.0]
    tiles = bytes([0x02, 0x8F])
    header = struct.pack('<4I3fh2x', x_tiles + 1, y_tiles + 1,
                         x_tiles, y_tiles, -8.0, 2.0, 3.0, -2)
    # The original extractor advertises 8 bytes/vertex but writes float heights.
    advertised_size = 32 + 8 * len(heights) + len(tiles)
    return (b'LIQU' + struct.pack('<I', advertised_size) + header
            + struct.pack('<6f', *heights) + tiles)


def group(identifier, bounds, vertices=(), *, wet=False, flags=0x2000,
          branches=()):
    result = struct.pack('<II6fI', flags, identifier, *bounds, int(wet))
    result += array(b'GRP ', len(branches),
                    b''.join(struct.pack('<I', item) for item in branches))
    result += array(b'INDX', 0, b'')
    result += array(b'VERT', len(vertices),
                    b''.join(struct.pack('<3f', *vertex) for vertex in vertices))
    if wet:
        result += liquid()
    return result


def raw_model(groups, *, vector_count=0, root=5875):
    return b'VMAPs05\0' + struct.pack('<III', vector_count, len(groups), root) + b''.join(groups)


def cases():
    bounds = (-9.0, -4.0, -2.0, 7.0, 8.0, 12.0)
    vertices = [(-1.0, 2.0, 3.0), (4.0, -2.0, 5.0), (0.0, 1.0, 9.0)]
    yield 'zero_groups', raw_model([])
    yield 'one_zero_vertex_group', raw_model([group(17, bounds)])
    yield 'vertices_zero_triangles', raw_model([
        group(23, bounds, vertices, branches=[9, 0, 4])], vector_count=3)
    yield 'zero_vertex_group_with_liquid', raw_model([group(29, bounds, wet=True)])
    yield 'vertices_zero_triangles_with_liquid', raw_model([
        group(31, bounds, vertices, wet=True)], vector_count=3)
    # Input group order deliberately differs from spatial/BIH order. The bounds
    # are authored, ordered low<=high, and cannot be inferred from empty meshes.
    groups = []
    for index, x in enumerate([20.0, -30.0, 4.0, 50.0, -8.0]):
        groups.append(group(100 + index, (x, -2.0, -1.0, x + 2.0, 3.0, 5.0),
                            flags=8 if index % 2 else 0x2000))
    yield 'multiple_empty_groups_authored_order', raw_model(groups)
    yield 'empty_and_triangleless_groups', raw_model([
        group(41, (-20.0, -3.0, 1.0, -10.0, 2.0, 6.0), wet=True),
        group(43, bounds, vertices),
        group(47, (20.0, -1.0, -2.0, 40.0, 4.0, 7.0)),
    ], vector_count=3)


HARNESS = r'''
#include <cfenv>
#include <cstddef>
#include <limits>
#include "TileAssembler.cpp"

int main(int argc, char** argv) {
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
    static_assert(sizeof(int) == 4 && sizeof(short) == 2);
    static_assert(sizeof(G3D::Vector3) == 12 && sizeof(G3D::AABox) == 24);
    static_assert(sizeof(VMAP::WMOLiquidHeader) == 32);
    static_assert(offsetof(VMAP::WMOLiquidHeader, type) == 28);
    if (argc != 3 || std::fesetround(FE_TONEAREST) != 0) return 1;
    VMAP::TileAssembler assembler(argv[1], argv[2]);
    return assembler.convertRawFile("model") ? 0 : 2;
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('Original binary fixture profile requires little-endian')
    source = args.source.resolve()
    revision = subprocess.check_output(
        ['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    if revision != PIN:
        raise SystemExit(f'Wrong source revision: expected {PIN}, got {revision}')
    # Covers compiled units and the original headers included transitively.
    subprocess.run(['git', '-C', str(source), 'diff', '--exit-code', 'HEAD', '--',
                    'src/game/vmap', 'dep/g3dlite'], check=True,
                   stdout=subprocess.DEVNULL)
    rows = [
        '# name\tauthored-raw-hex\toriginal-vmo-hex',
        f'# source-pin\t{PIN}',
        '# scope\toriginal TileAssembler::convertRawFile, raw loader, WorldModel writer and BIH',
        '# raw-LIQU-size\t32+8*height_count+tile_count; payload=32+4*height_count+tile_count',
        '# final-LIQU-size\t20+4*height_count+tile_count; payload=24+4*height_count+tile_count',
    ]
    fixtures = list(cases())
    with tempfile.TemporaryDirectory(prefix='benilla-source-model-') as temporary:
        work = Path(temporary)
        (work / 'Platform').mkdir()
        (work / 'Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
''')
        (work / 'probe.cpp').write_text(HARNESS)
        vmap = source / 'src/game/vmap'
        g3d = source / 'dep/g3dlite'
        command = [
            'g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
            '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections',
            '-DNO_CORE_FUNCS', '-I' + str(work), '-I' + str(vmap), '-I' + str(g3d),
            str(work / 'probe.cpp'), str(vmap / 'WorldModel.cpp'), str(vmap / 'BIH.cpp'),
        ]
        command += [str(g3d / (name + '.cpp')) for name in ['Vector3', 'AABox', 'g3dmath']]
        command += ['-Wl,--gc-sections', '-o', str(work / 'probe')]
        subprocess.run(command, check=True, timeout=60)
        for name, payload in fixtures:
            (work / 'model').write_bytes(payload)
            subprocess.run([str(work / 'probe'), str(work), str(work)],
                           check=True, timeout=10)
            expected = (work / 'model.vmo').read_bytes()
            rows.append('\t'.join([name, payload.hex(), expected.hex()]))
    output = ('\n'.join(rows) + '\n').encode()
    target = HERE / 'fixtures/source-model.tsv'
    if args.check:
        if target.read_bytes() != output:
            raise SystemExit('Original model fixtures differ')
    else:
        target.write_bytes(output)
    print(f'{"Verified" if args.check else "Generated"} {len(fixtures)} original model fixtures; '
          f'sha256={hashlib.sha256(output).hexdigest()}')


if __name__ == '__main__':
    main()
