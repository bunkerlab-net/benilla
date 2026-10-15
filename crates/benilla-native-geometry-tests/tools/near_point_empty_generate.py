#!/usr/bin/env python3
"""Unchanged-source full GetNearPointAt empty-list trace, authored scalar replies.

Usage: near_point_empty_generate.py MANGOS_ROOT GLIBC_ATAN_ROOT GLIBC_FLOAT_ROOT
No native/Rust producer, loaded geometry, game assets, network or gameplay proof.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shlex
import shutil
import struct
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
SIM_TOOLS = HERE.parents[1] / 'benilla-sim/tools'
spec = importlib.util.spec_from_file_location('empty_oracle_atan', SIM_TOOLS / 'source_atan_generate.py')
atan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(atan)
import near_point_candidate_generate as candidate
import near_point_primary_generate as primary

PIN = primary.PIN
GLIBC_PIN = candidate.GLIBC_PIN
MANIFEST = HERE / 'near_point_empty_source_hashes.sha256'
PACKAGE = HERE / 'near_point_empty_source_fragments.test-fixture'
FIXTURES = HERE / 'fixtures/near_point_empty'
FLOAT_FIELDS = ('pos_x', 'pos_y', 'pos_z', 'distance', 'searcher_radius', 'target_radius',
                'angle', 'target_x', 'target_y', 'target_z', 'collision_height')
INPUT_FIELDS = FLOAT_FIELDS + ('detect', 'ground_script', 'static_script', 'dynamic_script')
OUTPUT_FIELDS = ('outcome', 'result_x', 'result_y', 'result_z', 'xy_calls', 'height_calls',
                 'static_calls', 'dynamic_calls', 'config_calls', 'radius_calls', 'trace')
HEIGHT_FIELDS = ('height_index', 'xy_index', 'raw_angle', 'query_x', 'query_y', 'query_z',
                 'ground_reply', 'adjusted_z', 'static_called', 'dynamic_called',
                 'static_reply', 'dynamic_reply', 'los_src_x', 'los_src_y', 'los_src_z',
                 'los_dest_x', 'los_dest_y', 'los_dest_z', 'swim', 'ignore_m2')
bits, value = candidate.bits, candidate.value
exact_method, exact_line = primary.exact_method, primary.exact_line


def guarded_sources(mangos, glibc, glibcfloat):
    roots = {'cmangos': mangos, 'glibc': glibc, 'glibcfloat': glibcfloat}
    result = {}
    for line in MANIFEST.read_text().splitlines():
        if not line or line.startswith('#'):
            continue
        expected, qualified = line.split()
        family, name = qualified.split('/', 1)
        raw = (roots[family] / name).read_bytes().replace(b'\r\n', b'\n')
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit('Wrong pinned source hash: ' + qualified)
        result[qualified] = raw
    return result


def build_harness(sources):
    source = {name.removeprefix('cmangos/'): raw.decode('latin1')
              for name, raw in sources.items() if name.startswith('cmangos/')}
    obj = source['src/game/Entities/Object.cpp']
    grid = source['src/game/Maps/GridDefines.h']
    constants = '\n'.join((
        exact_line(source['src/game/Entities/ObjectDefines.h'], r'^static constexpr float const DEFAULT_WORLD_OBJECT_SIZE\s+.*$'),
        exact_line(grid, r'^#define INVALID_HEIGHT\s+.*$'),
        exact_line(source['src/shared/Common.h'], r'^#\s*define M_PI_F\s+.*$'),
    ))
    macros = '\n'.join(exact_line(grid, rf'^#define {name}\s+.*$') for name in (
        'MAX_NUMBER_OF_GRIDS', 'SIZE_OF_GRIDS', 'MAP_SIZE', 'MAP_HALFSIZE'))
    methods = []
    for name, signature, line in (
        ('src/game/Maps/Map.cpp', 'bool Map::IsInLineOfSight(', 2668),
        ('src/game/Entities/Unit.cpp', 'void Unit::UpdateAllowedPositionZ(', 11835),
        ('src/game/Entities/Object.cpp', 'bool WorldObject::IsWithinLOS(', 1387),
        ('src/game/Entities/Object.cpp', 'void WorldObject::GetNearPointAt(', 2306),
    ):
        text = source[name]
        if text[:text.index(signature)].count('\n') + 1 != line:
            raise SystemExit('Pinned source location drift: ' + signature)
        methods.append((name, exact_method(text, signature)))
    near2d = exact_method(obj, 'void WorldObject::GetNearPoint2dAt(')
    normalize = exact_method(grid, '    inline void NormalizeMapCoord(float& c)')
    declaration = exact_line(source['src/game/Entities/Object.h'],
        r'^        bool IsWithinLOS\(float ox, float oy, float oz, bool ignoreM2Model = false\) const;$')
    replacements = (
        ('// PINNED_CONSTANTS', constants), ('// PINNED_MAP_MACROS', macros),
        ('// PINNED_NORMALIZE_MAP_COORD', normalize),
        ('// PINNED_LOS_DECLARATION', declaration),
        ('// PINNED_COMPLETE_SELECTOR_CPP', source['src/game/Maps/ObjectPosSelector.cpp']),
        ('// PINNED_GET_NEAR_POINT_2D', near2d),
        ('// PINNED_MAP_UNIT_OBJECT_METHODS', '\n\n'.join(text for _, text in methods)),
    )
    harness = (HERE / 'near_point_empty_oracle.cpp').read_text()
    for marker, replacement in replacements:
        if harness.count(marker) != 1:
            raise SystemExit('Expected one harness marker: ' + marker)
        harness = harness.replace(marker, replacement)
    package = f'// CMaNGOS {PIN}; glibc {GLIBC_PIN}; unchanged source package.\n'
    package += '// Test-only numeric trace. Ground/static/dynamic replies are authored scalars.\n'
    fragments = [
        ('src/game/Entities/ObjectDefines.h', constants.splitlines()[0]),
        ('src/game/Maps/GridDefines.h', constants.splitlines()[1] + '\n' + macros + '\n' + normalize),
        ('src/shared/Common.h', constants.splitlines()[2]),
        ('src/game/Entities/Object.h', declaration),
        ('src/game/World/World.cpp', exact_line(source['src/game/World/World.cpp'],
            r'^    setConfig\(CONFIG_BOOL_DETECT_POS_COLLISION, "DetectPosCollision", true\);$')),
        ('src/game/Entities/Object.cpp', near2d), *methods,
    ]
    for name, fragment in fragments:
        notice = source[name][:source[name].index('*/') + 2]
        package += f'\n// BEGIN {name}\n{notice}\n{fragment}\n// END {name}\n'
    for name, raw in sources.items():
        if name.startswith(('glibc/', 'glibcfloat/')) or name in (
            'cmangos/src/game/Maps/ObjectPosSelector.cpp', 'cmangos/src/game/Maps/ObjectPosSelector.h'):
            package += f'\n// BEGIN COMPLETE {name}\n{raw.decode("latin1")}\n// END COMPLETE {name}\n'
    notices = '\n'.join(source[name][:source[name].index('*/') + 2] for name, _ in fragments)
    return notices + '\n' + harness, package


def run(command, **kwargs):
    print('+ ' + shlex.join(map(str, command)), file=sys.stderr, flush=True)
    return subprocess.run(command, check=True, **kwargs)


def verify_symbols(symbols, undefined, assembly):
    for name in ('atan', 'source_cos', 'source_sin'):
        if not re.search(r'\bT ' + name + '$', symbols, re.M):
            raise SystemExit('Missing pinned numeric symbol: ' + name)
    if re.search(r'\b(?:atan|atanf|atan2|atan2f|sin|sinf|sincos|sincosf|cos|cosf)(?:@|\s|$)', undefined):
        raise SystemExit('Host transcendental fallback is forbidden')
    for name in ('WorldObject::GetNearPointAt(', 'WorldObject::GetNearPoint2dAt(',
                 'WorldObject::IsWithinLOS(', 'Unit::UpdateAllowedPositionZ(',
                 'Map::IsInLineOfSight(', 'ObjectPosSelector::NextAngle(',
                 'ObjectPosSelector::NextSideAngle(', 'ObjectPosSelector::NextUsedAngle('):
        if name not in symbols:
            raise SystemExit('Missing unchanged source symbol: ' + name)
    if re.search(r'\b(?:v?fmadd|v?fmsub|v?fnmadd|v?fnmsub)', assembly):
        raise SystemExit('Unexpected fused arithmetic')
    for name, target in (('traced_cos', 'source_cos'), ('traced_sin', 'source_sin')):
        match = re.search(r'<' + name + r'\(float\)>:\n(.*?)(?=\n\n)', assembly, re.S)
        if not match or not re.search(r'(?:call|jmp)[^\n]*<' + target + '>', match.group(1)):
            raise SystemExit('Missing trace forwarding binding: ' + name)
    match = re.search(r'<WorldObject::GetNearPoint2dAt\([^\n]+>:\n(.*?)(?=\n\n)', assembly, re.S)
    if not match or match.group(1).count('mulss') != 2 or match.group(1).count('addss') != 2:
        raise SystemExit('Candidate separate binary32 multiply/add profile drift')
    if not all(name in match.group(1) for name in ('traced_cos(float)', 'traced_sin(float)')):
        raise SystemExit('Candidate must call both trace bindings')


def compile_oracle(sources, work, sanitize=False, receipts=None):
    # Reuse the existing audited original-source compilers unchanged, then link
    # their original numeric objects into the full-path observer. No host libm
    # atan/trig is used, and no numeric source body is reconstructed here.
    atan_work, trig_work = work / 'atan', work / 'trig'
    atan_work.mkdir(); trig_work.mkdir()
    atan_sources = {key: raw for key, raw in sources.items() if not key.startswith('glibcfloat/')}
    atan.compile_oracle(atan_sources, atan_work, sanitize, receipts / 'atan' if receipts else None)
    trig_sources = {key.removeprefix('cmangos/'): raw.decode('latin1') for key, raw in sources.items()
                    if key.startswith('cmangos/')}
    trig_sources.update({key.removeprefix('glibcfloat/'): raw.decode('latin1')
                         for key, raw in sources.items() if key.startswith('glibcfloat/')})
    candidate.compile_oracle(trig_sources, trig_work, sanitize, receipts / 'trig' if receipts else None)
    (work / 'Maps').mkdir(); (work / 'Entities').mkdir()
    (work / 'Maps/ObjectPosSelector.h').write_bytes(sources['cmangos/src/game/Maps/ObjectPosSelector.h'])
    (work / 'Common.h').write_text('#pragma once\n// Original selector needs no Common.h declarations.\n')
    (work / 'Entities/Object.h').write_text('#pragma once\n// Authored WorldObject declaration precedes original selector CPP.\n')
    harness, _ = build_harness(sources)
    (work / 'oracle.cpp').write_text(harness, encoding='latin1')
    flags = ['-O2', '-march=x86-64', '-msse2', '-mfpmath=sse', '-mno-fma', '-fno-fast-math',
             '-ffp-contract=off', '-frounding-math', '-fno-builtin', '-fno-strict-aliasing',
             '-Wall', '-Wextra', '-Werror']
    if sanitize:
        flags += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
    command = ['g++', '-std=c++20', *flags, '-I' + str(work), str(work / 'oracle.cpp'),
               str(atan_work / 'atan.o'), *[str(trig_work / (name + '.o'))
                   for name in ('s_sincosf_data.c', 's_sinf.c', 's_cosf.c')], '-lm', '-o', str(work / 'oracle')]
    run(command)
    binary = work / 'oracle'
    symbols = run(['nm', '-C', str(binary)], stdout=subprocess.PIPE).stdout.decode()
    undefined = run(['nm', '-u', str(binary)], stdout=subprocess.PIPE).stdout.decode()
    assembly = run(['objdump', '-d', '-C', str(binary)], stdout=subprocess.PIPE).stdout.decode()
    verify_symbols(symbols, undefined, assembly)
    if receipts:
        receipts.mkdir(parents=True, exist_ok=True)
        shutil.copy2(work / 'oracle.cpp', receipts / 'oracle.cpp')
        shutil.copy2(work / 'Maps/ObjectPosSelector.h', receipts / 'ObjectPosSelector.h')
        for path in trig_work.iterdir():
            if path.suffix in ('.h', '.c', '.cpp'):
                shutil.copy2(path, receipts / 'trig' / path.name)
        for name in ('Common.h', 'Entities/Object.h'):
            destination = receipts / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(work / name, destination)
        (receipts / 'symbols.txt').write_text(symbols)
        (receipts / 'undefined-symbols.txt').write_text(undefined)
        (receipts / 'oracle.disassembly.txt').write_text(assembly)
        (receipts / 'profile.json').write_text(json.dumps({
            'gcc': '14.2.0', 'g++': '14.2.0', 'target': 'x86_64-linux-gnu',
            'cmangos_revision': PIN, 'glibc_revision': GLIBC_PIN, 'command': command,
            'numeric_objects': 'Unchanged source_atan_generate.py O2 profile and near_point_candidate_generate.py O0 profile',
            'rounding': 'FE_TONEAREST; FTZ=DAZ=0; FLT_EVAL_METHOD=0; unfused SSE2',
            'sanitize': sanitize, 'scope': 'authored scalar height/LOS replies; full original empty-list near point',
        }, indent=2) + '\n')
    return binary


def cases():
    rows = []
    def add(label, replies=((0, 1, 1),), **changes):
        values = dict(zip(FLOAT_FIELDS, (3, 7, 0, 2, 1, 1, 0.5, 3, 7, 0, 2)))
        values.update(changes)
        words = tuple(bits(values[field]) for field in FLOAT_FIELDS) + (changes.get('detect', 1),)
        rows.append((label, words, [(bits(ground), static, dynamic) for ground, static, dynamic in replies]))
    add('primary_accept')
    add('first_plus_static_reject', replies=((0, 0, 1), (0, 1, 1)))
    add('first_plus_dynamic_reject', replies=((0, 1, 0), (0, 1, 1)))
    add('first_minus_mixed_reject', replies=((0, 0, 1), (0, 1, 0), (0, 1, 1)))
    add('later_success_vertical_and_static', replies=((4, 1, 1), (5, 1, 1), (0, 0, 1), (0, 1, 1)))
    add('later_success_alternating_los', replies=((0, 1, 0), (0, 0, 1), (0, 1, 0), (0, 1, 1)))
    add('all_static_blocked', replies=((0, 0, 1),))
    add('all_dynamic_blocked', replies=((0, 1, 0),))
    add('all_vertical_blocked', replies=((4, 1, 1),))
    add('last_adjusted_z_reaches_final_height', replies=((1, 0, 1), (3, 0, 1), (5, 0, 1),
        (7, 0, 1), (9, 0, 1), (-200000, 1, 1)),
        searcher_radius=value(0x7f7fffff))
    # These fixtures each run all angles. Explicit final invalid height preserves
    # the last adjusted Z, proving the source does not reset final Z to posZ.
    add('exhausted_final_invalid_preserves_last_z', replies=((1, 0, 1), (3, 0, 1), (5, 0, 1),
        (7, 0, 1), (9, 0, 1), (11, 0, 1), (13, 0, 1), (-200000, 1, 1)),
        searcher_radius=1, distance=1)
    for delta in (-1, 0, 1):
        add(f'exact_pi_neighbor_{delta}', replies=((0, 0, 1),), searcher_radius=value(0x40142315 + delta),
            distance=value(0x3d800000), angle=0)
    for size, distance, label in ((0, 0, 'zero_defaults'), (-0.0, -0.0, 'negative_zero_defaults'),
            (value(1), value(0x7f7fffff), 'zero_half_angle_628'),
            (value(0x7f7fffff), value(0x7f7fffff), 'overflow_ratio'),
            (value(1), 1, 'subnormal_radius')):
        add(label, replies=((0, 0, 1),), searcher_radius=size, distance=distance, angle=0)
    add('negative_raw_fallback_angle', replies=((0, 0, 1), (0, 0, 1), (0, 1, 1)), angle=0)
    add('upper_raw_fallback_angle', replies=((0, 0, 1),), angle=value(0x40c90fdb),
        searcher_radius=value(0x40142315), distance=value(0x3d800000))
    add('clamped_xy_each_attempt', replies=((0, 0, 1), (0, 0, 1), (0, 1, 1)),
        pos_x=32000, pos_y=-32000, distance=10000)
    add('actual_target_pose_los_origin', pos_x=1, pos_y=2, pos_z=3, target_x=101,
        target_y=-202, target_z=303, collision_height=1.5, replies=((4, 0, 1), (5, 1, 1)))
    add('left_associated_sum', distance=1, searcher_radius=value(0x33800000),
        target_radius=value(0x33800000), replies=((1, 1, 1), (0, 1, 1)))
    add('zero_distance_no_vertical_accept', distance=0, searcher_radius=0, target_radius=0)
    for z in (0.0, -0.0):
        for ground in (0.0, -0.0):
            add('signed_zero_z', pos_z=z, replies=((ground, 1, 1),))
    for ground in (-200000, -100000, value(bits(-100000)-1)):
        add('ground_validity_threshold', pos_z=7, replies=((ground, 1, 1),))
    for ground in (value(bits(4)-1), 4, value(bits(4)+1), -4):
        add('strict_vertical_boundary', replies=((ground, 1, 1), (0, 1, 1)))
    add('collision_disabled', detect=0, replies=((100, 0, 0),))
    # Authored scalar companions for separately tested loaded fixture locations.
    # Their expected geometry replies are inputs here, never oracle evidence of
    # the actual terrain/VMap data, traversal or provider behavior.
    add('loaded_wall_scalar_companion', pos_x=value(0x46850155), pos_y=value(0x46855355),
        pos_z=7, target_x=value(0x46850155), target_y=value(0x46855355), target_z=7,
        angle=0, distance=2.5, searcher_radius=0.5, target_radius=0.5,
        replies=((7, 0, 1),) * 5 + ((7, 1, 1),))
    add('loaded_flat_exhaustion_scalar_companion', pos_x=value(0x4684f155), pos_y=value(0x46855355),
        pos_z=11, target_x=value(0x4684f155), target_y=value(0x46855355), target_z=11,
        angle=0, distance=2.5, searcher_radius=0.5, target_radius=0.5, replies=((7, 1, 1),))
    add('loaded_carried_z_scalar_companion', pos_x=value(0x46852555), pos_y=value(0x46855355),
        pos_z=11, target_x=value(0x46852555), target_y=value(0x46855355), target_z=11,
        angle=0, distance=0.5, searcher_radius=0.5, target_radius=0.5,
        replies=((9, 1, 1),) * 5 + ((7, 1, 1),) * 3)
    add('loaded_subnormal_scalar_companion', pos_x=value(0x80800000), pos_y=value(0x44480000),
        pos_z=7, target_x=value(0x80800000), target_y=value(0x44480000), target_z=7,
        angle=0, distance=value(0x00800001), searcher_radius=0.5, target_radius=0.5,
        replies=((7, 1, 1),))
    add('loaded_blocked_exhaustion_scalar_companion', pos_x=value(0x46852d55), pos_y=value(0x46855355),
        pos_z=7, target_x=value(0x46852d55), target_y=value(0x46855355), target_z=7,
        angle=0, distance=0.5, searcher_radius=0.5, target_radius=0.5, collision_height=3,
        replies=((7, 0, 1),))
    return rows


def encode(rows):
    payload = bytearray()
    for _, words, replies in rows:
        payload += struct.pack('<13I', *words, len(replies))
        for reply in replies:
            payload += struct.pack('<3I', *reply)
    return bytes(payload)


def observe(binary, rows):
    lines = run([str(binary)], input=encode(rows), stdout=subprocess.PIPE).stdout.decode().splitlines()
    summaries = ['# Full unchanged-source empty-list GetNearPointAt; AUTHORED scalar callbacks only',
                 '# No loaded terrain/VMap/dynamic-tree, world-startup or gameplay-admission proof',
                 '# Script lists have equal lengths; callback ordinal selects an entry, then repeats the final entry',
                 '# Binary32 words hex; flags/counts decimal; source-only detect=0 control',
                 '# case\tlabel\t' + '\t'.join(INPUT_FIELDS + OUTPUT_FIELDS)]
    heights = ['# Actual ordered height/conditional-LOS callback observations; AUTHORED replies only',
               '# xy_index=-1 means no new XY call before this height query; raw_angle=00000000 is then an unused slot',
               '# Uncalled LOS slots are zero; static/dynamic replies remain authored input even when uncalled',
               '# case\t' + '\t'.join(HEIGHT_FIELDS)]
    cursor = 0
    for index, (label, words, replies) in enumerate(rows):
        summary = lines[cursor].split('\t'); cursor += 1
        if summary[0] != 'S' or len(summary) != len(OUTPUT_FIELDS) + 1:
            raise SystemExit('Wrong summary observation schema')
        fields = [f'{w:08x}' for w in words[:11]] + [str(words[11])]
        fields += [','.join(f'{reply[0]:08x}' for reply in replies)]
        fields += [','.join(str(reply[n]) for reply in replies) for n in (1, 2)]
        summaries.append('\t'.join([str(index), label, *fields, *summary[1:]]))
        for _ in range(int(summary[6])):
            height = lines[cursor].split('\t'); cursor += 1
            if height[0] != 'H' or len(height) != len(HEIGHT_FIELDS) + 1:
                raise SystemExit('Wrong height observation schema')
            heights.append('\t'.join([str(index), *height[1:]]))
    if cursor != len(lines):
        raise SystemExit('Unexpected extra source observations')
    return {'cases.test-fixture': '\n'.join(summaries) + '\n',
            'heights.test-fixture': '\n'.join(heights) + '\n'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mangos', type=Path); parser.add_argument('glibc', type=Path)
    parser.add_argument('glibcfloat', type=Path)
    parser.add_argument('--check', action='store_true'); parser.add_argument('--sanitize', action='store_true')
    parser.add_argument('--receipt-dir', type=Path)
    args = parser.parse_args()
    if sys.byteorder != 'little': raise SystemExit('Little-endian source profile required')
    sources = guarded_sources(args.mangos, args.glibc, args.glibcfloat)
    _, package = build_harness(sources)
    if args.check and PACKAGE.read_text(encoding='latin1') != package:
        raise SystemExit('Verbatim empty-selector source package differs')
    with tempfile.TemporaryDirectory(prefix='near-point-empty-source-') as temporary:
        binary = compile_oracle(sources, Path(temporary), args.sanitize, args.receipt_dir)
        fixtures = observe(binary, cases())
    for name, text in fixtures.items():
        if args.check and (FIXTURES / name).read_text() != text:
            raise SystemExit('Empty-selector fixture differs: ' + name)
    if not args.check:
        PACKAGE.write_text(package, encoding='latin1'); FIXTURES.mkdir(parents=True, exist_ok=True)
        for name, text in fixtures.items(): (FIXTURES / name).write_text(text)
    for name, text in fixtures.items():
        count = sum(not line.startswith('#') for line in text.splitlines())
        print(f'{"Verified" if args.check else "Generated"} {name}: {count} rows; sha256={hashlib.sha256(text.encode()).hexdigest()}')


if __name__ == '__main__': main()
