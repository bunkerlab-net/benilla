#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run exact pinned Player environment bodies and timer/gate slices.

Usage: generate_environment_state_fixture.py /path/to/mangos-classic [--check] [--sanitize]
Authored bound liquid facts are inputs, not native geometry/feed evidence.
"""
import argparse
import hashlib
import os
import pathlib
import re
import shlex
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
PLAYER = 'src/game/Entities/Player.cpp'
HEADER = 'src/game/Entities/Player.h'
SPELL = 'src/game/Spells/SpellDefines.h'


def block(text, signature, suffix=''):
    if text.count(signature) != 1:
        raise SystemExit(f'Expected one exact source signature: {signature}')
    start = text.index(signature)
    opening = text.index('{', start)
    end, depth = opening + 1, 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    if text[end:end + len(suffix)] != suffix:
        raise SystemExit(f'Expected source suffix: {signature}')
    return text[start:end + len(suffix)]


def exact_line(text, pattern):
    matches = re.findall(pattern, text, flags=re.M)
    if len(matches) != 1:
        raise SystemExit(f'Expected one source line: {pattern}')
    return matches[0]


def guarded(root):
    sources = {}
    for line in (HERE / 'environment_state_source_hashes.sha256').read_text().splitlines():
        expected, path = line.split('  ', 1)
        raw = (root / path).read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {path}')
        sources[path] = raw.decode('latin1')
    return sources


def source_harness(source):
    declarations = block(source[HEADER], 'enum EnvironmentFlags\n', ';')
    declarations += '\n' + block(source[SPELL], 'enum SpellAuraInterruptFlags ', ';')
    declarations += '\n' + block(source[SPELL], 'enum TriggerCastFlags : uint32\n', ';')
    declarations += '\n#pragma pack(push,1)\n' + block(source['src/game/Server/DBCStructure.h'], 'struct LiquidTypeEntry\n', ';')
    declarations += '\n#pragma pack(pop)\n' + exact_line(source['src/game/Server/DBCfmt.h'], r'^const char LiquidTypefmt\[\] = ".*?";$')
    player = source[PLAYER]
    update = block(player, '    if (m_positionStatusUpdateTimer)\n')
    position = block(player, 'bool Player::SetPosition(')
    start = position.index('    if (m_positionStatusUpdateTimer)')
    end = position.index('\n    // code block for outdoor state and area-explore check')
    gate = position[start:end]
    mirror = block(source[HEADER], 'class MirrorTimer\n', ';')
    replacements = {
        '// PINNED_DECLARATIONS': declarations,
        '// PINNED_MIRROR_TYPES': block(mirror, 'enum Type\n', ';'),
        '// PINNED_UPDATE_TIMER': update,
        '// PINNED_POSITION_GATE': gate,
        '// PINNED_ENVIRONMENT_METHODS': '\n\n'.join(block(player, signature) for signature in (
            'void Player::SetEnvironmentFlags(', 'void Player::UpdateTerainEnvironmentFlags(')),
    }
    text = (HERE / 'environment_state_oracle.cpp').read_text()
    for marker, replacement in replacements.items():
        if text.count(marker) != 1:
            raise SystemExit(f'Expected one harness marker: {marker}')
        text = text.replace(marker, replacement)
    # Preserve the upstream copyright/license notice in the compiled material.
    return player[:player.index('#include')] + text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true')
    args = parser.parse_args()
    if sys.byteorder != 'little':
        raise SystemExit('This oracle profile requires little-endian IEEE binary32')
    root = args.source.resolve()
    sources = guarded(root)
    cpp = source_harness(sources)
    expected = {pathlib.Path(name) for name in (
        'environment-state.tsv', 'environment-state-dbc.tsv',
        'environment-state/store0/LiquidType.dbc.test-fixture',
        'environment-state/store1/LiquidType.dbc.test-fixture')}
    with tempfile.TemporaryDirectory(prefix='.environment-state-oracle-', dir=HERE) as temporary:
        work = pathlib.Path(temporary)
        (work / 'stubs/Platform').mkdir(parents=True)
        (work / 'stubs/Platform/Define.h').write_text('''#pragma once
#include <cstdint>
using int8=std::int8_t; using int16=std::int16_t; using int32=std::int32_t; using int64=std::int64_t;
using uint8=std::uint8_t; using uint16=std::uint16_t; using uint32=std::uint32_t; using uint64=std::uint64_t;
#define MANGOS_ENDIAN 0
#define MANGOS_BIG_ENDIAN 1
''')
        (work / 'oracle.cpp').write_text(cpp)
        compiler = shlex.split(os.environ.get('CXX', 'g++'))
        command = compiler + [
            '-std=c++17', '-O0', '-Wall', '-Wextra', '-Werror', '-fno-fast-math',
            '-ffp-contract=off', '-fno-strict-aliasing',
            '-I' + str(work / 'stubs'), '-I' + str(root / 'src/game'),
            '-I' + str(root / 'src/shared'), str(work / 'oracle.cpp'),
            str(root / 'src/shared/Database/DBCFileLoader.cpp'),
            '-o', str(work / 'oracle.test-fixture')]
        if args.sanitize:
            command[len(compiler):len(compiler)] = ['-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
        subprocess.run(command, check=True)
        output = work / 'output'
        output.mkdir()
        subprocess.run([str(work / 'oracle.test-fixture'), str(output)], check=True)
        actual = {p.relative_to(output) for p in output.rglob('*') if p.is_file()}
        if actual != expected:
            raise SystemExit('Unexpected oracle products')
        target = HERE / 'fixtures'
        if args.check:
            inventory = {pathlib.Path(name) for name in ('environment-state.tsv', 'environment-state-dbc.tsv')
                         if (target / name).is_file()}
            inventory |= {p.relative_to(target) for p in (target / 'environment-state').rglob('*') if p.is_file()}
            if inventory != expected:
                raise SystemExit('Unexpected committed oracle product inventory')
        for relative in sorted(actual):
            data = (output / relative).read_bytes()
            destination = target / relative
            if args.check:
                if not destination.is_file() or destination.read_bytes() != data:
                    raise SystemExit(f'Environment fixture differs: {relative}')
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
            print(f'{hashlib.sha256(data).hexdigest()}  {relative}')
        print(f'{"Verified" if args.check else "Generated"} {len(actual)} exact-source environment files')


if __name__ == '__main__':
    main()
