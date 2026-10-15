#!/usr/bin/env python3
"""Offline initial-candidate oracle: untouched CMaNGOS bodies and glibc float trig.

Usage: near_point_candidate_generate.py MANGOS_ROOT GLIBC_FLT32 [--check] [--sanitize]
Only synthetic numeric inputs; no Rust/native producer, game assets, or network.
The input-rejection policy bounds this experiment; it is not CMaNGOS gameplay.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import pathlib
import re
import shlex
import struct
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
FIXTURES = HERE / 'fixtures/near_point_candidate'
PACKAGE = HERE / 'near_point_candidate_source_fragments.cpp.txt'
PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
GLIBC_PIN = '74f59e9271cbb4071671e5a474e7d4f1622b186f'
SIGN = 0x80000000
TAU = 0x40C90FDB
COORD = 0x46FFF000
CLAMP = 0x46855455
GRID_STEP = 4093
DYADIC_DENOMINATOR = 16384
DYADIC_STEPS = 102943  # floor(binary32 TAU * 16384)
SEED = 0x4E504341
CANDIDATE_SAMPLES = 32768
GLIBC_NAMES = ('sincosf_poly.h', 's_sincosf.h', 's_sincosf_data.c', 's_sinf.c', 's_cosf.c')


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def value(word):
    return struct.unpack('<f', struct.pack('<I', word))[0]


def guarded_sources(mangos, glibc):
    result = {}
    for manifest, root, pin in (
        ('near_point_candidate_source_hashes.sha256', mangos, PIN),
        ('vmap_glibc_source_hashes.sha256', glibc, GLIBC_PIN),
    ):
        for line in (HERE / manifest).read_text().splitlines():
            expected, name = line.split('  ', 1)
            raw = (root / name).read_bytes()
            if hashlib.sha256(raw).hexdigest() != expected:
                raise SystemExit(f'Wrong pinned source hash: {name}; expected {pin}')
            result[name] = raw.decode('latin1')
    return result


def exact_method(source, signature):
    if source.count(signature) != 1:
        raise SystemExit(f'Expected one source method: {signature}')
    start = source.index(signature)
    opening = source.index('{', start)
    depth = 0
    for end in range(opening, len(source)):
        depth += (source[end] == '{') - (source[end] == '}')
        if depth == 0:
            return source[start:end + 1]
    raise SystemExit(f'Unclosed source method: {signature}')


def exact_line(source, expression):
    found = re.findall(expression, source, flags=re.M)
    if len(found) != 1:
        raise SystemExit(f'Expected one source declaration: {expression}')
    return found[0]


def source_fragments(source):
    grid_name, object_name = 'src/game/Maps/GridDefines.h', 'src/game/Entities/Object.cpp'
    grid, obj = source[grid_name], source[object_name]
    macros = '\n'.join(exact_line(grid, rf'^#define {name}\s+.*?$') for name in (
        'MAX_NUMBER_OF_GRIDS', 'SIZE_OF_GRIDS', 'MAP_SIZE', 'MAP_HALFSIZE'))
    normalize = exact_method(grid, '    inline void NormalizeMapCoord(float& c)')
    candidate = exact_method(obj, 'void WorldObject::GetNearPoint2dAt(')
    for text, signature, line in ((grid, '    inline void NormalizeMapCoord(float& c)', 169),
                                  (obj, 'void WorldObject::GetNearPoint2dAt(', 2297)):
        if text[:text.index(signature)].count('\n') + 1 != line:
            raise SystemExit(f'Pinned source location drift: {signature}')
    package = f'// Verbatim source package: CMaNGOS {PIN}; glibc {GLIBC_PIN}.\n'
    package += '// Test-only extraction. Authored bindings and I/O are in near_point_candidate_oracle.cpp.\n'
    for name, fragment in ((grid_name, macros + '\n' + normalize), (object_name, candidate)):
        notice = source[name][:source[name].index('*/') + 2]
        package += f'\n// BEGIN {name}\n{notice}\n{fragment}\n// END {name}\n'
    for name in GLIBC_NAMES:
        package += f'\n// BEGIN COMPLETE glibc/sysdeps/ieee754/flt-32/{name}\n{source[name]}'
        package += f'\n// END COMPLETE {name}\n'
    return macros, normalize, candidate, package


def build_harness(source):
    macros, normalize, candidate, package = source_fragments(source)
    text = (HERE / 'near_point_candidate_oracle.cpp').read_text()
    for marker, fragment in (
        ('// PINNED_MAP_MACROS', macros),
        ('// PINNED_NORMALIZE_MAP_COORD', normalize),
        ('// PINNED_GET_NEAR_POINT', candidate),
    ):
        if text.count(marker) != 1:
            raise SystemExit(f'Expected one harness marker: {marker}')
        text = text.replace(marker, fragment)
    notices = '\n'.join(source[name][:source[name].index('*/') + 2] for name in (
        'src/game/Maps/GridDefines.h', 'src/game/Entities/Object.cpp'))
    return notices + '\n' + text, package


def run(command, **kwargs):
    print('+ ' + shlex.join(map(str, command)), file=sys.stderr, flush=True)
    return subprocess.run(command, check=True, **kwargs)


def compile_oracle(source, work, sanitize, receipts=None):
    harness, _ = build_harness(source)
    (work / 'oracle.cpp').write_text(harness, encoding='latin1')
    for name in GLIBC_NAMES:
        # Complete originals, including includes, comments and aliases, unchanged.
        (work / name).write_text(source[name], encoding='latin1')
    (work / 'math_config.h').write_text('''#ifndef ORACLE_MATH_CONFIG_H
#define ORACLE_MATH_CONFIG_H
#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include <float.h>
#if FLT_EVAL_METHOD != 0 || FLT_RADIX != 2 || FLT_MANT_DIG != 24 || DBL_MANT_DIG != 53
#error Unsupported floating-point evaluation profile
#endif
#ifdef __FAST_MATH__
#error Fast math is forbidden
#endif
#define TOINT_INTRINSICS 0
#define attribute_hidden
#undef __glibc_unlikely
#undef __glibc_likely
#define __glibc_unlikely(x) (x)
#define __glibc_likely(x) (x)
static inline uint32_t asuint(float x) { uint32_t u; memcpy(&u, &x, 4); return u; }
/* Nonfinite values are rejected before the original glibc body is called. */
static inline float __math_invalidf(float x) { (void)x; abort(); }
#endif
''')
    (work / 'math-barriers.h').write_text(
        '#define math_force_eval(x) do { volatile float v = (x); (void)v; } while (0)\n')
    (work / 'libm-alias-float.h').write_text(
        '#define libm_alias_float(a,b) /* SINF/COSF explicitly bind both symbols. */\n')
    flags = ['-O0', '-fno-fast-math', '-ffp-contract=off', '-frounding-math',
             '-fno-builtin', '-fno-strict-aliasing', '-I' + str(work)]
    if sanitize:
        flags += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
    objects = []
    for name in ('s_sincosf_data.c', 's_sinf.c', 's_cosf.c'):
        target = work / (name + '.o')
        run(['gcc', '-std=c11', *flags, '-DSINF=source_sin', '-DCOSF=source_cos',
             '-c', str(work / name), '-o', str(target)])
        objects.append(str(target))
    binary = work / 'oracle'
    run(['g++', '-std=c++17', *flags, str(work / 'oracle.cpp'), *objects, '-lm', '-o', str(binary)])
    defined = run(['nm', '-C', '--defined-only', str(binary)], stdout=subprocess.PIPE).stdout.decode()
    undefined = run(['nm', '-C', '-u', str(binary)], stdout=subprocess.PIPE).stdout.decode()
    assembly = run(['objdump', '-d', '-C', str(binary)], stdout=subprocess.PIPE).stdout.decode()
    verify_symbols(defined, undefined, assembly)
    if receipts:
        receipts.mkdir(parents=True, exist_ok=True)
        (receipts / 'symbols.txt').write_text(defined + '\nUNDEFINED\n' + undefined)
        (receipts / 'oracle.disassembly.txt').write_text(assembly)
        (receipts / 'oracle.cpp').write_text(harness, encoding='latin1')
        (receipts / 'compiler-version.txt').write_bytes(
            run(['gcc', '--version'], stdout=subprocess.PIPE).stdout +
            run(['g++', '--version'], stdout=subprocess.PIPE).stdout)
    return binary


def verify_symbols(defined, undefined, assembly):
    for symbol in ('source_sin', 'source_cos'):
        if not re.search(r'\bT ' + symbol + r'$', defined, re.M):
            raise SystemExit(f'Missing explicitly bound original symbol: {symbol}')
    if re.search(r'\b(?:sin|sinf|sincos|sincosf|cos|cosf|__sinf|__cosf)(?:@|\b)', undefined):
        raise SystemExit('Host trigonometric fallback is forbidden')
    match = re.search(r'<WorldObject::GetNearPoint2dAt\([^\n]+>:\n(.*?)(?=\n\n)', assembly, re.S)
    if not match:
        raise SystemExit('Missing source candidate disassembly')
    code = match.group(1)
    for symbol in ('source_sin', 'source_cos'):
        if not re.search(r'call[^\n]*<' + symbol + '>', code):
            raise SystemExit(f'Candidate call is not bound to {symbol}')
    if code.count('mulss') != 2 or code.count('addss') != 2 or re.search(r'\b[v]?fmadd', code):
        raise SystemExit('Expected two separate binary32 multiplies and adds (x86-64 oracle)')


def trig_edges():
    centers = [0, 1, 0x007FFFFF, 0x00800000, 0x34000000, 0x39800000, 0x3F400000]
    centers += [bits(n * float.fromhex('0x1.921fb54442d18p1') / 4) for n in range(1, 9)]
    centers += [bits(v) for v in (1e-20, 1e-10, 0.1, 1.0, 3.0, 6.0)]
    return sorted({(center + delta) | sign for center in centers for delta in range(-3, 4)
                   if 0 <= center + delta <= TAU for sign in (0, SIGN)})


def trig_grid():
    grid = [word | sign for word in range(0, TAU + 1, GRID_STEP) for sign in (0, SIGN)]
    grid += [bits(step / DYADIC_DENOMINATOR) | sign
             for step in range(DYADIC_STEPS + 1) for sign in (0, SIGN)]
    return grid + [TAU, TAU | SIGN]


def candidate_inputs(trig_rows):
    rows = []
    def add(group, x, y, distance, angle):
        rows.append((group, (x, y, distance, angle)))
    for x in (0, SIGN):
        for y in (0, SIGN):
            for distance in (0, SIGN, 1, 0x007FFFFF, 0x00800000, bits(1), 0x7F7FFFFF):
                for angle in (0, SIGN, 1, 0x007FFFFF, 0x00800000, 0x39800000, TAU):
                    add('signed_zero_subnormal_tiny', x, y, distance, angle)
    positive = [(a, s, c) for a, s, c in trig_rows if a <= TAU]
    for angle, sine, cosine in positive:
        for x, y in ((0, 0), (bits(-1), bits(1)), (COORD, COORD | SIGN)):
            for distance in (bits(1), bits(17), bits(10000), 0x7F7FFFFF):
                add('trig_boundaries', x, y, distance, angle)
        for distance in (bits(1.0000001192092896), bits(3.25), bits(12345)):
            # These only choose cancellation inputs. Expected results are always
            # emitted by the independently compiled original source body.
            x = bits(-value(distance) * value(cosine))
            y = bits(-value(distance) * value(sine))
            if (x & ~SIGN) <= COORD and (y & ~SIGN) <= COORD:
                add('cancellation_and_fma', x, y, distance, angle)
    for sign in (0, SIGN):
        for x in range(CLAMP - 3, CLAMP + 4):
            for distance in (0, SIGN, 1, bits(0.00048828125), bits(0.0009765625),
                             bits(0.001953125), bits(0.00390625), bits(1), 0x7F7FFFFF):
                for angle in (0, 0x3FC90FDB, 0x40490FDB, 0x4096CBE4, TAU):
                    add('clamp_neighbors', x | sign, x | (sign ^ SIGN), distance, angle)
    for sign in (0, SIGN):
        for x in (CLAMP + 1, bits(20000), bits(30000), COORD - 1, COORD):
            for distance in (0, SIGN, x, bits(10000), bits(40000)):
                for angle in (0, 0x3FC90FDB, 0x40490FDB, 0x4096CBE4, TAU):
                    add('outside_clamp_input_not_preclamped', x | sign, x | (sign ^ SIGN), distance, angle)
    # The current-pose facing source fixture bearing_12_2 emits minimum subnormal
    # 00000001 for (+minsub,-minsub) -> (1.0,0). Keep that direct-radian input.
    add('current_bearing_minimum_subnormal', 0, 0, 0x7F7FFFFF, 1)
    return rows


def candidate_grid():
    state = SEED
    def next_word():
        nonlocal state
        state ^= (state << 13) & 0xFFFFFFFF
        state ^= state >> 17
        state ^= (state << 5) & 0xFFFFFFFF
        return state
    rows = []
    for _ in range(CANDIDATE_SAMPLES):
        x, y, distance, angle = (next_word() for _ in range(4))
        rows.append(((x & SIGN) | ((x & ~SIGN) % (COORD + 1)),
                     (y & SIGN) | ((y & ~SIGN) % (COORD + 1)),
                     (distance & ~SIGN) % 0x7F800000, (angle & ~SIGN) % (TAU + 1)))
    return rows


def rejected_inputs():
    nonfinite = [0x7F800000, 0xFF800000, 0x7FC00000, 0x7F800001, 0xFFC01234]
    trig = nonfinite + [TAU + 1, (TAU + 1) | SIGN, 0x7F7FFFFF, 0xFF7FFFFF]
    candidate = []
    for field in range(4):
        invalid = list(nonfinite)
        if field < 2:
            invalid += [COORD + 1, (COORD + 1) | SIGN, 0x7F7FFFFF, 0xFF7FFFFF]
        elif field == 2:
            invalid += [SIGN | 1, SIGN | 0x007FFFFF, bits(-1), 0xFF7FFFFF]
        else:
            invalid += [SIGN | 1, bits(-1), TAU + 1, TAU | SIGN, 0x7F7FFFFF]
        for word in invalid:
            row = [0, 0, bits(1), 0]
            row[field] = word
            candidate.append(tuple(row))
    return trig, candidate


def encode(rows):
    return b''.join(struct.pack('<' + 'I' * len(row), *row) for row in rows)


def observe(binary, mode, rows, width):
    data = run([str(binary), mode], input=encode(rows), stdout=subprocess.PIPE).stdout
    if len(data) != len(rows) * width * 4:
        raise SystemExit(f'Wrong oracle record count: {mode}')
    observed = list(struct.iter_unpack('<' + 'I' * width, data))
    if any(tuple(row) != result[:len(row)] for row, result in zip(rows, observed, strict=True)):
        raise SystemExit(f'Oracle input echo differs: {mode}')
    return observed, data


def format_rows(header, rows):
    return '# ' + header + '\n' + ''.join('\t'.join(f'{word:08x}' for word in row) + '\n' for row in rows)


def fixture_contents(binary):
    trig_rows, _ = observe(binary, 'trig', [(w,) for w in trig_edges()], 3)
    _, trig_sample = observe(binary, 'trig', [(w,) for w in trig_grid()], 3)
    cases = candidate_inputs(trig_rows)
    candidate_rows, _ = observe(binary, 'candidate', [row for _, row in cases], 6)
    _, candidate_sample = observe(binary, 'candidate', candidate_grid(), 6)
    candidate_text = '# target_x_bits\ttarget_y_bits\tdistance_bits\tangle_radians_bits\tx_bits\ty_bits\n'
    previous = None
    for (group, _), row in zip(cases, candidate_rows, strict=True):
        if previous != group:
            candidate_text += '# ' + group + '\n'
            previous = group
        candidate_text += '\t'.join(f'{word:08x}' for word in row) + '\n'
    trig_bad, candidate_bad = rejected_inputs()
    for mode, rows, status in (('trig', [(w,) for w in trig_bad], 3), ('candidate', candidate_bad, 4)):
        for row in rows:
            result = subprocess.run([str(binary), mode], input=encode([row]), capture_output=True)
            if result.returncode != status or result.stdout or result.stderr:
                raise SystemExit(f'Rejection failed: {mode} {row}: {result}')
        partial = subprocess.run([str(binary), mode], input=b'\x00', capture_output=True)
        if partial.returncode != 6 or partial.stdout or partial.stderr:
            raise SystemExit(f'Partial input was not rejected: {mode}')
    return {
        'trig-cases.test-fixture': format_rows('angle_radians_bits\tsine_bits\tcosine_bits', trig_rows),
        'candidate-cases.test-fixture': candidate_text,
        'trig-grid.test-fixture': '# count\tsha256(le_u32 angle,sin,cos); bounded sample, not exhaustive\n'
            + f'{len(trig_grid())}\t{hashlib.sha256(trig_sample).hexdigest()}\n',
        'candidate-grid.test-fixture': '# count\txorshift32_seed\tsha256(le_u32 x,y,distance,angle,result_x,result_y)\n'
            + f'{CANDIDATE_SAMPLES}\t{SEED:08x}\t{hashlib.sha256(candidate_sample).hexdigest()}\n',
        'trig-rejections.test-fixture': format_rows('angle_radians_bits; wrapper-domain rejections', [(w,) for w in trig_bad]),
        'candidate-rejections.test-fixture': format_rows('target_x_bits\ttarget_y_bits\tdistance_bits\tangle_radians_bits; wrapper-domain rejections', candidate_bad),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos', type=pathlib.Path)
    parser.add_argument('glibc', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true')
    parser.add_argument('--receipt-dir', type=pathlib.Path)
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('This source-observation profile requires little-endian IEEE binary32')
    source = guarded_sources(args.mangos, args.glibc)
    _, package = build_harness(source)
    if args.check and PACKAGE.read_text(encoding='latin1') != package:
        raise SystemExit('Verbatim near-point candidate source package differs')
    with tempfile.TemporaryDirectory(prefix='near-point-candidate-oracle-') as temporary:
        binary = compile_oracle(source, pathlib.Path(temporary), args.sanitize, args.receipt_dir)
        fixtures = fixture_contents(binary)
    for name, content in fixtures.items():
        target = FIXTURES / name
        if args.check and target.read_text() != content:
            raise SystemExit(f'Near-point candidate fixture differs: {name}')
    if not args.check:
        FIXTURES.mkdir(parents=True, exist_ok=True)
        PACKAGE.write_text(package, encoding='latin1')
        for name, content in fixtures.items():
            (FIXTURES / name).write_text(content)
    for name, content in fixtures.items():
        count = sum(not line.startswith('#') for line in content.splitlines())
        print(f'{"Verified" if args.check else "Generated"} {name}: {count} rows; sha256={hashlib.sha256(content.encode()).hexdigest()}')
    print('Verified explicit source_sin/source_cos bindings, no host trig fallback, separate binary32 multiply/add')


if __name__ == '__main__':
    main()
