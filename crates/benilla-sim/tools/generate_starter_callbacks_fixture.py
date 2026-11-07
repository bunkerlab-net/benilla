#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Execute unchanged pinned skill callbacks against explicitly authored adapters.

Usage: generate_starter_callbacks_fixture.py UPSTREAM_ROOT [--check]
No client data, loaded-order claim, or implementation of Player::addSpell.
"""
import argparse
import hashlib
import pathlib
import re
import subprocess
import tempfile

PIN = '8ec338a1704e7dcb1c0213eb7ed58f9231ade40f'
HASHES = {
    'src/game/Entities/Player.cpp': 'd02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e',
    'src/game/Globals/SharedDefines.h': '4832b8e0da45211143acae7c42173fea42732e2acdbf190692725e51b020bfe8',
    'src/game/Spells/SpellAuraDefines.h': '42ca923dc8dfd4da733223ac09fce1288f938137501f83f4ab7a02b18df4e23f',
    'src/game/Spells/SpellEffectDefines.h': '175c870f4b379164de848951666d563d0164638c99b7b866f027f5af7c1c6aee',
    'src/game/Entities/Player.h': 'acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb',
}
SIGNATURES = {
    'SET_EXISTING': 'void Player::SetSkill(SkillStatusMap::iterator',
    'SET_ID': 'void Player::SetSkill(uint16 id,',
    'TRAIN': 'void Player::UpdateSkillTrainedSpells(',
}
HERE = pathlib.Path(__file__).resolve().parent
OUTPUT = HERE.parent / 'tests/fixtures/starter_callbacks_cmangos.tsv'


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
    if 'typedef std::unordered_map<uint32, SkillStatusData> SkillStatusMap;' not in sources['src/game/Entities/Player.h']:
        raise ValueError('skill map type drift')
    constants = [('src/game/Globals/SharedDefines.h', 'SKILL_DEFENSE', 95),
                 ('src/game/Spells/SpellAuraDefines.h', 'SPELL_AURA_MOD_SKILL', 30),
                 ('src/game/Spells/SpellAuraDefines.h', 'SPELL_AURA_MOD_SKILL_TALENT', 98),
                 ('src/game/Spells/SpellEffectDefines.h', 'SPELL_EFFECT_SKILL', 118)]
    for path, name, value in constants:
        if not re.search(r'\b' + name + r'\s*=\s*' + str(value) + r'\s*,', sources[path]):
            raise ValueError('constant drift: ' + name)
    if not re.search(r'#define PLAYER_MAX_SKILLS\s+127\b', sources['src/game/Entities/Player.h']):
        raise ValueError('skill capacity drift')
    parts = {key: body(sources['src/game/Entities/Player.cpp'], signature)
             for key, signature in SIGNATURES.items()}
    parts['STATUS'] = body(sources['src/game/Entities/Player.h'], 'enum SkillUpdateState') + ';\n'
    return parts


def assemble(parts, harness=None):
    if set(parts) != set(SIGNATURES) | {'STATUS'}:
        raise ValueError('source fragment scope')
    if harness is None:
        harness = (HERE / 'starter_callbacks_oracle.cpp').read_text()
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
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    parts = fragments(args.upstream)
    for key, text in parts.items():
        print(f'Unchanged {key} SHA256 {hashlib.sha256(text.encode()).hexdigest()}')
    with tempfile.TemporaryDirectory() as temp:
        source, binary = pathlib.Path(temp) / 'oracle.cpp', pathlib.Path(temp) / 'oracle'
        source.write_text(assemble(parts))
        subprocess.run(['g++', '-std=c++17', '-O0', '-Wall', '-Wextra', '-Werror',
                        '-fsanitize=undefined', '-fno-sanitize-recover=all', str(source), '-o', str(binary)], check=True)
        data = subprocess.check_output([str(binary)])
    if args.check:
        if OUTPUT.read_bytes() != data:
            raise SystemExit('fixture mismatch')
    else:
        OUTPUT.write_bytes(data)
    print(f'{len(data.splitlines()) - 3} authored cases; SHA256 {hashlib.sha256(data).hexdigest()}')


if __name__ == '__main__':
    main()
