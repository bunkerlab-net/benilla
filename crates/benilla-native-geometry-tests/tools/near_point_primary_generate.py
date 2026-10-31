#!/usr/bin/env python3
"""Observe only first-candidate acceptance from pinned CMaNGOS source, offline.

Initial XY, ground height and actual static/dynamic LOS are authored callbacks.
All source fallback entries throw; no Rust, game assets or network are used.
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
PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
PACKAGE = HERE / 'near_point_primary_source_fragments.cpp.txt'
FIXTURE = HERE / 'fixtures/near_point_primary/primary-acceptance.test-fixture'
MANIFEST = HERE / 'near_point_primary_source_hashes.sha256'
FLOAT_FIELDS = ('pos_x', 'pos_y', 'pos_z', 'distance', 'searcher_radius', 'target_radius',
                'angle', 'candidate_x', 'candidate_y', 'ground', 'target_x', 'target_y',
                'target_z', 'collision_height')
INPUT_FIELDS = FLOAT_FIELDS + ('detect', 'static_los', 'dynamic_los')
OUTPUT_FIELDS = ('outcome', 'result_x', 'result_y', 'result_z', 'xy_calls', 'ground_calls',
                 'ground_x', 'ground_y', 'ground_z', 'ground_swim', 'static_calls', 'dynamic_calls',
                 'los_src_x', 'los_src_y', 'los_src_z', 'los_dest_x', 'los_dest_y', 'los_dest_z',
                 'ignore_m2', 'config_calls', 'radius_calls', 'fallback_calls', 'trace')


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def value(word):
    return struct.unpack('<f', struct.pack('<I', word))[0]


def guarded_sources(root):
    source = {}
    for line in MANIFEST.read_text().splitlines():
        expected, name = line.split('  ', 1)
        raw = (root / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}; expected CMaNGOS {PIN}')
        source[name] = raw.decode('latin1')
    return source


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
    lines = re.findall(expression, source, flags=re.M)
    if len(lines) != 1:
        raise SystemExit(f'Expected one source declaration: {expression}')
    return lines[0]


def source_fragments(source):
    objects = 'src/game/Entities/Object.cpp'
    selector = 'src/game/Maps/ObjectPosSelector.cpp'
    constants = []
    pieces = []
    for name, expression in (
        ('src/game/Entities/ObjectDefines.h', r'^static constexpr float const DEFAULT_WORLD_OBJECT_SIZE\s+.*$'),
        ('src/game/Maps/GridDefines.h', r'^#define INVALID_HEIGHT\s+.*$'),
        (selector, r'^#define OCCUPY_POS_ANGLE_ATAN_FACTOR\s+.*$'),
    ):
        text = exact_line(source[name], expression)
        pieces.append((name, text))
        constants.append(text)
    los_declaration = exact_line(source['src/game/Entities/Object.h'],
        r'^        bool IsWithinLOS\(float ox, float oy, float oz, bool ignoreM2Model = false\) const;$')
    config = exact_line(source['src/game/World/World.cpp'],
        r'^    setConfig\(CONFIG_BOOL_DETECT_POS_COLLISION, "DetectPosCollision", true\);$')
    pieces.extend([('src/game/Entities/Object.h', los_declaration),
                   ('src/game/World/World.cpp', config)])
    selected = []
    method_specs = (
        (selector, 'ObjectPosSelector::ObjectPosSelector(', 25),
        (selector, 'bool ObjectPosSelector::CheckAngle(', 81),
        (selector, 'bool ObjectPosSelector::CheckOriginalAngle(', 94),
        ('src/game/Maps/Map.cpp', 'bool Map::IsInLineOfSight(', 2668),
        ('src/game/Entities/Unit.cpp', 'void Unit::UpdateAllowedPositionZ(', 11835),
        (objects, 'bool WorldObject::IsWithinLOS(', 1387),
        (objects, 'void WorldObject::GetNearPointAt(', 2306),
    )
    for name, signature, line in method_specs:
        text = source[name]
        fragment = exact_method(text, signature)
        if text[:text.index(signature)].count('\n') + 1 != line:
            raise SystemExit(f'Pinned source location drift: {signature}')
        pieces.append((name, fragment))
        selected.append(fragment)
    # Context only: neither method below executes in this scalar-boundary oracle.
    for signature in ('void WorldObject::GetNearPoint2dAt(', 'bool WorldObject::IsWithinLOSInMap('):
        pieces.append((objects, exact_method(source[objects], signature)))
    package = f'// Verbatim pinned-source package: CMaNGOS Classic {PIN}.\n'
    package += '// InitialXY, ground and LOS replies are authored callbacks; fallback never executes.\n'
    for name, fragment in pieces:
        notice = source[name][:source[name].index('*/') + 2]
        package += f'\n// BEGIN {name}\n{notice}\n{fragment}\n// END {name}\n'
    header = 'src/game/Maps/ObjectPosSelector.h'
    package += f'\n// BEGIN COMPLETE {header}\n{source[header]}\n// END COMPLETE {header}\n'
    return '\n'.join(constants), los_declaration, '\n\n'.join(selected[:3]), \
        '\n\n'.join(selected[3:]), package


def build_harness(source):
    constants, declaration, selector, methods, package = source_fragments(source)
    text = (HERE / 'near_point_primary_oracle.cpp').read_text()
    for marker, fragment in (
        ('// PINNED_CONSTANTS', constants),
        ('// PINNED_LOS_DECLARATION', declaration),
        ('// PINNED_SELECTOR_METHODS', selector),
        ('// PINNED_MAP_UNIT_OBJECT_METHODS', methods),
    ):
        if text.count(marker) != 1:
            raise SystemExit(f'Expected one harness marker: {marker}')
        text = text.replace(marker, fragment)
    notices = '\n'.join(text[:text.index('*/') + 2] for text in source.values())
    return notices + '\n' + text, package


def run(command, **kwargs):
    print('+ ' + shlex.join(map(str, command)), file=sys.stderr, flush=True)
    return subprocess.run(command, check=True, **kwargs)


def compile_oracle(source, work, sanitize, receipts=None):
    harness, _ = build_harness(source)
    (work / 'oracle.cpp').write_text(harness, encoding='latin1')
    # The complete original selector declaration and real std::multimaps execute.
    (work / 'ObjectPosSelector.h').write_text(source['src/game/Maps/ObjectPosSelector.h'], encoding='latin1')
    (work / 'Common.h').write_text('#pragma once\n// No Common.h declarations are required by this selector header.\n')
    flags = ['-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off', '-frounding-math',
             '-fno-builtin', '-fno-strict-aliasing', '-Wall', '-Wextra', '-Werror']
    if sanitize:
        flags += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
    binary = work / 'oracle'
    run(['g++', *flags, '-I' + str(work), str(work / 'oracle.cpp'), '-o', str(binary)])
    symbols = run(['nm', '-C', str(binary)], stdout=subprocess.PIPE).stdout
    assembly = run(['objdump', '-d', '-C', str(binary)], stdout=subprocess.PIPE).stdout
    for signature in ('WorldObject::GetNearPointAt(', 'WorldObject::IsWithinLOS(',
                      'ObjectPosSelector::CheckOriginalAngle()', 'Unit::UpdateAllowedPositionZ(',
                      'Map::IsInLineOfSight('):
        if signature not in symbols.decode():
            raise SystemExit(f'Missing compiled original method: {signature}')
    if re.search(rb'\b[v]?fmadd', assembly):
        raise SystemExit('Unexpected fused multiply-add in source observation profile')
    if receipts:
        receipts.mkdir(parents=True, exist_ok=True)
        (receipts / 'oracle.cpp').write_text(harness, encoding='latin1')
        (receipts / 'ObjectPosSelector.h').write_text(source['src/game/Maps/ObjectPosSelector.h'], encoding='latin1')
        (receipts / 'symbols.txt').write_bytes(symbols)
        (receipts / 'oracle.disassembly.txt').write_bytes(assembly)
        (receipts / 'compiler-version.txt').write_bytes(run(['g++', '--version'], stdout=subprocess.PIPE).stdout)
    return binary


def cases():
    rows = []
    def add(label, **changes):
        values = dict(zip(FLOAT_FIELDS, (3, 7, 0, 2, 1, 1, 0.5, 5, 8, 0, 3, 7, 0, 2)))
        values.update(detect=1, static_los=1, dynamic_los=1)
        values.update(changes)
        row = tuple(bits(values[name]) if name in FLOAT_FIELDS else values[name] for name in INPUT_FIELDS)
        rows.append((label, row))
    for z in (-10.0, -0.0, 0.0, 7.0, 100.0):
        for ground in (-200000.0, -100001.0, -100000.0, value(bits(-100000.0) - 1),
                       -10.0, -0.0, 0.0, 7.0, 100.0):
            for static_los in (0, 1):
                for dynamic_los in (0, 1):
                    add('ground_and_los_matrix', pos_z=z, target_z=z, ground=ground,
                        static_los=static_los, dynamic_los=dynamic_los)
    for sign in (1, -1):
        for word in range(bits(4.0) - 2, bits(4.0) + 3):
            add('strict_height_difference_boundary', ground=sign * value(word))
    for distance, searcher_radius, target_radius in (
        (0, 0, 0), (-0.0, -0.0, -0.0), (0, 1, 0), (0, 0, 1),
        (value(1), 0, 0), (0, value(1), 0), (0, 0, value(1)),
        (1, value(0x33800000), value(0x33800000)),
        (value(0x3f800001), value(0x33800000), value(0x33800000)),
        (1, 1, 1), (2, 3, 4),
    ):
        for ground in (0, value(1), value(0x3f7fffff), 1, value(0x3f800001), 3):
            add('ordered_sum_and_zero_radius', distance=distance, searcher_radius=searcher_radius,
                target_radius=target_radius, ground=ground)
    for sign in (0, 0x80000000):
        for height_sign in (0, 0x80000000):
            add('signed_zero_preservation', pos_z=value(sign), ground=value(height_sign),
                target_z=value(sign), collision_height=0)
            add('candidate_signed_zero_forwarding', candidate_x=value(sign), candidate_y=value(height_sign))
    for detect in (0, 1):
        for ground in (-200000.0, -100000.0, 0, 4, 100):
            for static_los, dynamic_los in ((0, 0), (1, 0), (1, 1)):
                add('collision_disabled_control' if not detect else 'collision_enabled_control',
                    detect=detect, ground=ground, static_los=static_los, dynamic_los=dynamic_los)
    add('actual_target_pose_is_los_origin', pos_x=1, pos_y=2, pos_z=3, ground=4,
        target_x=101, target_y=-202, target_z=303, candidate_x=11, candidate_y=22, collision_height=1.5)
    add('endpoint_is_adjusted_feet', pos_z=10, target_z=10, ground=12, collision_height=1.75)
    add('initial_xy_not_radius_expanded', distance=2, searcher_radius=7, target_radius=9)
    add('left_associated_distance_sum', distance=1, searcher_radius=value(0x33800000),
        target_radius=value(0x33800000), ground=1)
    add('invalid_ground_still_accepts', pos_z=7, target_z=7, ground=-200000)
    add('invalid_threshold_still_accepts', pos_z=7, target_z=7, ground=-100000)
    add('no_extra_support_tolerance', pos_z=0, ground=3.5)
    return rows


def observe(binary, rows):
    payload = b''.join(struct.pack('<17I', *row) for _, row in rows)
    output = run([str(binary)], input=payload, stdout=subprocess.PIPE).stdout.decode().splitlines()
    if len(output) != len(rows):
        raise SystemExit('Wrong source observation count')
    result = [f'# CMaNGOS {PIN}; complete first-candidate path, authored scalar callbacks',
              '# No loaded products, full-selector/libm parity, fallback search, or admission proof',
              '# detect=0 is a source-only disabled-control; the production bounded slice requires true',
              '# Binary32 words are hex; flags/counts are decimal; uncalled LOS argument words are zero',
              '# Trace I=initialXY C=config R=radius H=ground P=target-position S=static-LOS D=dynamic-LOS F=fallback-entry',
              '# case\tlabel\t' + '\t'.join(INPUT_FIELDS + OUTPUT_FIELDS)]
    for number, ((label, row), observed) in enumerate(zip(rows, output, strict=True)):
        if len(observed.split('\t')) != len(OUTPUT_FIELDS):
            raise SystemExit('Wrong source observation column count')
        fields = [f'{word:08x}' if i < 14 else str(word) for i, word in enumerate(row)]
        result.append(f'{number}\t{label}\t' + '\t'.join(fields) + '\t' + observed)
    return ('\n'.join(result) + '\n').encode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true')
    parser.add_argument('--receipt-dir', type=pathlib.Path)
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('This source observation profile requires little-endian IEEE binary32')
    source = guarded_sources(args.source.resolve())
    _, package = build_harness(source)
    if args.check and PACKAGE.read_text(encoding='latin1') != package:
        raise SystemExit('Verbatim primary source package differs')
    with tempfile.TemporaryDirectory(prefix='near-point-primary-oracle-') as temporary:
        binary = compile_oracle(source, pathlib.Path(temporary), args.sanitize, args.receipt_dir)
        result = observe(binary, cases())
    if args.check:
        if FIXTURE.read_bytes() != result:
            raise SystemExit('Primary acceptance source fixture differs')
    else:
        PACKAGE.write_text(package, encoding='latin1')
        FIXTURE.parent.mkdir(parents=True, exist_ok=True)
        FIXTURE.write_bytes(result)
    print(f'{FIXTURE}: {"verified" if args.check else "generated"} {len(cases())} rows; '
          f'sha256={hashlib.sha256(result).hexdigest()}')
    print(f'{PACKAGE}: source package {"verified" if args.check else "generated"}')


if __name__ == '__main__':
    main()
