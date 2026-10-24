#!/usr/bin/env python3
"""Offline source-exact glibc 2.41 float-trig oracle for the VMap Euler profile.

Pass a directory containing the five pinned flt-32 files; --check never rewrites fixtures.
Only declarations/include plumbing are supplied. All math function bodies and coefficient
arrays come verbatim from the hash-checked originals, including the generic reducer.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import pathlib
import struct
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
PIN = '74f59e9271cbb4071671e5a474e7d4f1622b186f'
MAX_DEGREES_BITS = 0x43B40000
GRID_STEP = 4093


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    source = {}
    for line in (HERE / 'vmap_glibc_source_hashes.sha256').read_text().splitlines():
        expected, name = line.split('  ', 1)
        data = (args.source / name).read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit(f'Wrong glibc {PIN} source hash: {name}')
        source[name] = data.decode()
    prelude = '''#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cmath>
#include <cfenv>
#include <limits>
#define TOINT_INTRINSICS 0
#define attribute_hidden
#undef __glibc_unlikely
#define __glibc_unlikely(x) (x)
#undef __glibc_likely
#define __glibc_likely(x) (x)
#define math_force_eval(x) do { volatile float v = (x); (void)v; } while (0)
#define SINF source_sin
#define COSF source_cos
static uint32_t asuint(float x) { uint32_t b; std::memcpy(&b, &x, 4); return b; }
static float __math_invalidf(float x) { return x - x; }
'''
    body = prelude
    for name in ['sincosf_poly.h', 's_sincosf.h', 's_sincosf_data.c', 's_sinf.c', 's_cosf.c']:
        body += '\n'.join(line for line in source[name].splitlines()
                          if not line.startswith('#include')) + '\n'
    body += '''int main() {
    static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559);
    static_assert(sizeof(double) == 8 && std::numeric_limits<double>::is_iec559);
    if (std::fesetround(FE_TONEAREST) != 0) return 2;
    uint8_t bytes[4];
    while (std::fread(bytes, 1, 4, stdin) == 4) {
        uint32_t word = uint32_t(bytes[0]) | uint32_t(bytes[1]) << 8 |
                        uint32_t(bytes[2]) << 16 | uint32_t(bytes[3]) << 24;
        float angle; std::memcpy(&angle, &word, 4);
        if (!std::isfinite(angle) || std::abs(angle) > 360.f) return 3;
        const float radians = (0x1.921fb54442d18p1 * double(angle)) / 180.0;
        for (uint32_t output : {word, asuint(source_sin(radians)), asuint(source_cos(radians))}) {
            uint8_t out[4] = {uint8_t(output), uint8_t(output >> 8),
                             uint8_t(output >> 16), uint8_t(output >> 24)};
            if (std::fwrite(out, 1, 4, stdout) != 4) return 4;
        }
    }
    return std::ferror(stdin) ? 5 : 0;
}
'''
    body = '#include <initializer_list>\n' + body
    # Both signs around zero/subnormals, all admitted quadrant boundaries, the
    # abstop12 small-angle switches, and the concrete 13-degree platform seam.
    centers = [0, 1, 0x007FFFFF, 0x00800000]
    centers += [bits(v) for v in [9, 13, 37, 45, 90, 135, 180, 225, 270, 315, 360]]
    centers += [bits(v * 180.0 / float.fromhex('0x1.921fb54442d18p1'))
                for v in [float.fromhex('0x1p-12'),
                          struct.unpack('<f', struct.pack('<I', 0x3F400000))[0]]]
    edges = sorted({(center + delta) | sign for center in centers for delta in range(-3, 4)
                    if 0 <= center + delta <= MAX_DEGREES_BITS
                    for sign in [0, 0x80000000]})
    grid = [word | sign for word in range(0, MAX_DEGREES_BITS + 1, GRID_STEP)
            for sign in [0, 0x80000000]]
    grid += [bits(step / 1024.0) | sign for step in range(360 * 1024 + 1)
             for sign in [0, 0x80000000]]
    with tempfile.TemporaryDirectory(prefix='vmap-trig-oracle-') as tmp:
        work = pathlib.Path(tmp)
        (work / 'oracle.cpp').write_text(body)
        subprocess.run(['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                        str(work / 'oracle.cpp'), '-o', str(work / 'oracle')], check=True)
        def run(words):
            return subprocess.check_output([str(work / 'oracle')],
                                           input=b''.join(struct.pack('<I', w) for w in words))
        output = run(edges)
        cases = '# angle_degrees_bits\tsine_bits\tcosine_bits; exact glibc 2.41 source\n'
        cases += ''.join('%08x\t%08x\t%08x\n' % row for row in struct.iter_unpack('<III', output))
        digest = hashlib.sha256(run(grid)).hexdigest() + '\n'
    target = HERE / 'fixtures/vmap_source'
    for name, content in [('trig-cases.tsv', cases), ('trig-grid.sha256', digest)]:
        path = target / name
        if args.check:
            if path.read_text() != content:
                raise SystemExit(f'Fixture differs: {name}')
        else:
            path.write_text(content)
    print(f'{"Verified" if args.check else "Generated"} {len(edges)} edge cases and '
          f'{len(grid)} signed angle samples; SHA-256 {digest.strip()}')


if __name__ == '__main__':
    main()
