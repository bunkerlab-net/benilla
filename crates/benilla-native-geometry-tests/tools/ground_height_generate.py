#!/usr/bin/env python3
"""Generate source-exact ground-height callback observations, offline.

Usage: ground_height_generate.py /path/to/mangos-classic [--check] [--sanitize]
The full original Map and Unit bodies execute with authored static replies. The
empty dynamic mode runs original DynamicMapTree::getHeight, RegularGrid2D and
G3D ray math. No native asset parser, Rust compiler, game data or network is used.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import argparse
import hashlib
import pathlib
import re
import shlex
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
import generate_vmap_fixture as base

HERE = pathlib.Path(__file__).resolve().parent
PIN = base.PIN
EXTRA_MANIFEST = HERE / 'ground_height_source_hashes.sha256'
PACKAGE = HERE / 'ground_height_source_fragments.cpp.txt'


def guarded_sources(root):
    # Preserve every existing VMap/G3D full-file hash guard, including headers.
    source = base.guarded_sources(root)
    for line in EXTRA_MANIFEST.read_text().splitlines():
        expected, name = line.split('  ', 1)
        raw = (root / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}; expected CMaNGOS {PIN}')
        source[name] = raw.decode('latin1')
    return source


def exact_line(source, expression):
    found = re.findall(expression, source, flags=re.M)
    if len(found) != 1:
        raise SystemExit(f'Expected one source declaration: {expression}')
    return found[0]


def exact_method(source, signature, trailer=''):
    if source.count(signature) != 1:
        raise SystemExit(f'Expected one source method: {signature}')
    return base.method(source, signature, trailer)


def source_fragments(source):
    definitions = source['src/game/Maps/GridDefines.h']
    map_cpp = source['src/game/Maps/Map.cpp']
    unit_cpp = source['src/game/Entities/Unit.cpp']
    dynamic = source['src/game/vmap/DynamicTree.cpp']
    constants = '\n'.join(exact_line(definitions, rf'^#define {name}\s+.*?$') for name in (
        'INVALID_HEIGHT', 'INVALID_HEIGHT_VALUE', 'DEFAULT_HEIGHT_SEARCH', 'DEFAULT_WATER_SEARCH')) + '\n'
    dynamic_methods = exact_method(dynamic, 'struct DynamicTreeIntersectionCallback\n', ';')
    dynamic_methods += exact_method(dynamic, 'float DynamicMapTree::getHeight(')
    map_unit = exact_method(map_cpp, 'float Map::GetHeight(')
    map_unit += exact_method(unit_cpp, 'void Unit::UpdateAllowedPositionZ(')
    for source_text, signature, expected_line in (
        (map_cpp, 'float Map::GetHeight(', 2758),
        (unit_cpp, 'void Unit::UpdateAllowedPositionZ(', 11835),
        (dynamic, 'float DynamicMapTree::getHeight(', 267),
    ):
        if source_text[:source_text.index(signature)].count('\n') + 1 != expected_line:
            raise SystemExit(f'Pinned source location drift: {signature}')
    pieces = [('src/game/Maps/GridDefines.h', constants),
              ('src/game/Maps/Map.cpp', exact_method(map_cpp, 'float Map::GetHeight(')),
              ('src/game/Entities/Unit.cpp', exact_method(unit_cpp, 'void Unit::UpdateAllowedPositionZ(')),
              ('src/game/vmap/DynamicTree.cpp', dynamic_methods)]
    package = f'// Extracted verbatim from CMaNGOS Classic {PIN}.\n'
    package += '// Context/package only; authored declarations and experiments are in ground_height_oracle.cpp.\n'
    for name, fragment in pieces:
        original = source[name]
        # Original copyright/license header, not a substituted SPDX assertion.
        notice = original[:original.index('*/') + 2]
        package += f'\n// BEGIN {name}\n{notice}\n{fragment}// END {name}\n'
    regular_name = 'src/game/vmap/RegularGrid.h'
    package += f'\n// BEGIN COMPLETE {regular_name}\n' + source[regular_name]
    package += f'\n// END COMPLETE {regular_name}\n'
    return constants, dynamic_methods, map_unit, package


def build_harness(source):
    constants, dynamic_methods, map_unit, package = source_fragments(source)
    text = (HERE / 'ground_height_oracle.cpp').read_text()
    for marker, replacement in (
        ('// PINNED_HEIGHT_CONSTANTS', constants),
        ('// PINNED_DYNAMIC_METHODS', dynamic_methods),
        ('// PINNED_MAP_UNIT_METHODS', map_unit),
    ):
        if text.count(marker) != 1:
            raise SystemExit(f'Expected one harness marker: {marker}')
        text = text.replace(marker, replacement)
    # Each source family carries the original notice into the compiled unit.
    notices = '\n'.join(source[name][:source[name].index('*/') + 2] for name in (
        'src/game/Maps/Map.cpp', 'src/game/Entities/Unit.cpp', 'src/game/vmap/DynamicTree.cpp',
        'src/game/Maps/GridDefines.h'))
    return notices + '\n' + text, package


def run(command, **kwargs):
    print('+ ' + shlex.join(map(str, command)), file=sys.stderr, flush=True)
    return subprocess.run(command, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true',
                        help='Enable undefined-behavior and float-cast-overflow traps')
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('This source-observation profile requires little-endian IEEE binary32')
    root = args.source.resolve()
    source = guarded_sources(root)
    harness, package = build_harness(source)
    if args.check and PACKAGE.read_text() != package:
        raise SystemExit('Verbatim ground-height source package differs')
    with tempfile.TemporaryDirectory(prefix='ground-height-oracle-') as temporary:
        work = pathlib.Path(temporary)
        (work / 'stubs/Util').mkdir(parents=True)
        # Only unrelated error/log plumbing is substituted. Empty traversal never
        # allocates a node or invokes this assertion/model-intersection boundary.
        (work / 'stubs/Util/Errors.h').write_text('#pragma once\n#include <cstdlib>\n#define MANGOS_ASSERT(x) do { if (!(x)) std::abort(); } while (false)\n')
        (work / 'oracle.cpp').write_text(harness, encoding='latin1')
        command = ['g++', '-std=c++17', '-O0', '-fno-fast-math', '-ffp-contract=off',
                   '-fno-strict-aliasing', '-ffunction-sections', '-fdata-sections',
                   '-I' + str(work / 'stubs'), '-I' + str(root / 'src/game/vmap'),
                   '-I' + str(root / base.G3D), str(work / 'oracle.cpp')]
        command += [str(root / base.G3D / (name + '.cpp')) for name in ('Ray', 'Vector3', 'g3dmath')]
        if args.sanitize:
            command += ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
        command += ['-Wl,--gc-sections', '-o', str(work / 'oracle')]
        run(command)
        results = {}
        for profile, filename in (
            ('finite', 'ground-height.test-fixture'),
            ('nonfinite', 'ground-height-nonfinite.test-fixture'),
        ):
            result = run([str(work / 'oracle'), profile], stdout=subprocess.PIPE).stdout
            target = HERE / 'fixtures' / filename
            if args.check and target.read_bytes() != result:
                raise SystemExit(f'Ground-height oracle mismatch: {filename}')
            results[target] = result
    if not args.check:
        PACKAGE.write_text(package, encoding='latin1')
        for target, result in results.items():
            target.write_bytes(result)
    for target, result in results.items():
        count = sum(not line.startswith(b'#') for line in result.splitlines())
        print(f'{target}: {"verified" if args.check else "generated"} {count} rows; sha256={hashlib.sha256(result).hexdigest()}')
    print(f'{PACKAGE}: source package {"verified" if args.check else "generated"}')


if __name__ == '__main__':
    main()
