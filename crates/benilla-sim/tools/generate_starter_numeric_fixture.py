#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Execute unchanged pinned numeric lookups against explicitly authored adapters.

Usage: generate_starter_numeric_fixture.py UPSTREAM_ROOT [--check | --verify-source]
No client data, loaded-store observation, trigger-policy proof, or admission change.
The generated translation unit retains the upstream GPL-2.0-or-later notice.
"""
import argparse
import hashlib
import pathlib
import re
import subprocess
import tempfile

PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
HASHES = {
    'src/game/Spells/SpellMgr.cpp': 'd1a7064febbe3a7657e85931c4bda89a09adfabc89b7cbed23fdbffd6e37c22d',
    'src/game/Server/DBCStructure.h': '354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50',
    'src/game/Spells/SpellDefines.h': '07c3fd11a40c993ab8feeb718d21a7a2ae9b5c1700efd2e03ac53e76c6f938bf',
    'src/game/Entities/Player.h': 'acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb',
    'src/game/Entities/Unit.h': '2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1',
    'src/game/Entities/UpdateFields.h': 'b1c6f19ea63d7bb27a8da0da4834fbc36244e1df5cc6888cac761484645ad2a8',
    'src/game/Globals/SharedDefines.h': '4832b8e0da45211143acae7c42173fea42732e2acdbf190692725e51b020bfe8',
}
SIGNATURES = {
    'DURATION': 'int32 GetSpellDuration(',
    'MAX_DURATION': 'int32 GetSpellMaxDuration(',
    'CAST_TIME': 'uint32 GetSpellCastTime(',
}
EXTRA_PARTS = {'LICENSE', 'CAST_ROW', 'DURATION_ROW'}
HERE = pathlib.Path(__file__).resolve().parent
OUTPUT = HERE.parent / 'tests/fixtures/starter_numeric_cmangos.tsv'


def checked_text(path, digest):
    raw = path.read_bytes().replace(b'\r\n', b'\n')
    if b'\r' in raw or hashlib.sha256(raw).hexdigest() != digest:
        raise ValueError(f'altered source: {path}')
    return raw.decode()


def body(text, signature):
    if text.count(signature) != 1:
        raise ValueError(f'nonunique signature: {signature}')
    start = text.index(signature)
    opening = text.index('{', start)
    depth, end = 1, opening + 1
    while depth and end < len(text):
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    if depth:
        raise ValueError('unclosed source body')
    return text[start:end] + '\n'


def fragments(root):
    sources = {name: checked_text(root / name, digest) for name, digest in HASHES.items()}
    spell_mgr = sources['src/game/Spells/SpellMgr.cpp']
    parts = {key: body(spell_mgr, signature) for key, signature in SIGNATURES.items()}
    parts['LICENSE'] = spell_mgr[:spell_mgr.index('*/') + 2] + '\n'
    dbc = sources['src/game/Server/DBCStructure.h']
    parts['CAST_ROW'] = body(dbc, 'struct SpellCastTimesEntry\n') + ';\n'
    parts['DURATION_ROW'] = body(dbc, 'struct SpellDurationEntry\n') + ';\n'
    # Keep source signedness; baseLevel makes the source arithmetic unsigned.
    for declaration in ('uint32    baseLevel;', 'uint32    DurationIndex;', 'uint32    CastingTimeIndex;'):
        if declaration not in dbc:
            raise ValueError('SpellEntry field drift: ' + declaration)
    if not re.search(r'virtual uint32 GetSpellRank\(SpellEntry const\* spellInfo\) const;', sources['src/game/Entities/Unit.h']):
        raise ValueError('rank return type drift')
    constants = [
        ('Spells/SpellDefines.h', 'SPELL_ATTR_USES_RANGED_SLOT', '0x00000002'),
        ('Spells/SpellDefines.h', 'SPELL_ATTR_IS_ABILITY', '0x00000010'),
        ('Spells/SpellDefines.h', 'SPELL_ATTR_IS_TRADESKILL', '0x00000020'),
        ('Spells/SpellDefines.h', 'SPELL_ATTR_EX3_IGNORE_CASTER_MODIFIERS', '0x20000000'),
        ('Spells/SpellDefines.h', 'SPELLMOD_CASTING_TIME', '10'),
        ('Entities/Player.h', 'TRADE_SLOT_NONTRADED', '6'),
        ('Globals/SharedDefines.h', 'RANGED_ATTACK', '2'),
        ('Entities/UpdateFields.h', 'OBJECT_END', '0x06'),
        ('Entities/UpdateFields.h', 'UNIT_MOD_CAST_SPEED', r'0x8b\s*\+\s*OBJECT_END'),
    ]
    for path, name, value in constants:
        if not re.search(r'\b' + name + r'\s*=\s*' + value + r'\b', sources['src/game/' + path]):
            raise ValueError('constant drift: ' + name)
    return parts


def assemble(parts, harness=None):
    if set(parts) != set(SIGNATURES) | EXTRA_PARTS:
        raise ValueError('source fragment scope')
    if harness is None:
        harness = (HERE / 'starter_numeric_oracle.cpp').read_text()
    for key, fragment in parts.items():
        marker = '// PINNED_' + key + '\n'
        if harness.count(marker) != 1:
            raise ValueError('marker drift: ' + key)
        harness = harness.replace(marker, fragment)
    if '// PINNED_' in harness:
        raise ValueError('unresolved marker')
    return harness


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('upstream', type=pathlib.Path)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--check', action='store_true')
    mode.add_argument('--verify-source', action='store_true')
    args = parser.parse_args()
    parts = fragments(args.upstream)
    for key, text in parts.items():
        print(f'Unchanged {key} SHA256 {hashlib.sha256(text.encode()).hexdigest()}')
    assembled = assemble(parts)
    if args.verify_source:
        return
    with tempfile.TemporaryDirectory() as temp:
        source, binary = pathlib.Path(temp) / 'oracle.cpp', pathlib.Path(temp) / 'oracle'
        source.write_text(assembled)
        subprocess.run(['g++', '-std=c++17', '-O0', '-Wall', '-Wextra', '-Werror',
                        '-fno-fast-math', '-ffp-contract=off',
                        '-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all',
                        str(source), '-o', str(binary)], check=True)
        subprocess.run([str(binary), '--check-exclusions'], check=True)
        data = subprocess.check_output([str(binary)])
    if args.check:
        if OUTPUT.read_bytes() != data:
            raise SystemExit('fixture mismatch')
    else:
        OUTPUT.write_bytes(data)
    print(f'{len(data.splitlines()) - 4} authored cases; SHA256 {hashlib.sha256(data).hexdigest()}')


if __name__ == '__main__':
    main()
