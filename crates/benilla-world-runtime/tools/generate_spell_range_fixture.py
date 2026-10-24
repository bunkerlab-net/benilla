#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Execute complete hash-pinned CMaNGOS spell-range bodies in a bounded host.

Usage: generate_spell_range_fixture.py SOURCE [--check] [--sanitize]
Inputs are authored binary32 probes, never game/database rows. See spell_range_README.md.
"""
import argparse
import hashlib
import os
import pathlib
import shlex
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
SPELL = 'src/game/Spells/Spell.cpp'
OBJECT = 'src/game/Entities/Object.cpp'
OBJECT_HEADER = 'src/game/Entities/Object.h'
UNIT_HEADER = 'src/game/Entities/Unit.h'
PLAYER_HEADER = 'src/game/Entities/Player.h'
DEFINES = 'src/game/Spells/SpellDefines.h'
TARGETS = 'src/game/Spells/SpellTargetDefines.h'
MANAGER = 'src/game/Spells/SpellMgr.h'
DBC = 'src/game/Server/DBCStructure.h'
SHARED = 'src/game/Globals/SharedDefines.h'
GUID = 'src/game/Entities/ObjectGuid.h'
UPDATE = 'src/game/Entities/UpdateFields.h'
OBJECT_DEFINES = 'src/game/Entities/ObjectDefines.h'
PLAYER_DEFINES = 'src/game/Entities/PlayerDefines.h'
COMMON = 'src/shared/Common.h'
SOURCES = {SPELL, OBJECT, OBJECT_HEADER, UNIT_HEADER, PLAYER_HEADER, DEFINES,
           TARGETS, MANAGER, DBC, SHARED, GUID, UPDATE, OBJECT_DEFINES, PLAYER_DEFINES, COMMON}
# Every block is inserted verbatim, including branches outside the bounded host.
BLOCKS = {
    'MOD_TYPES': (PLAYER_DEFINES, 'enum SpellModType\n', ';'),
    'MOD_PAIR': (PLAYER_DEFINES, 'struct SpellModifierPair\n', ';'),
    'TIME_CONSTANTS': (COMMON, 'enum TimeConstants\n', ';'),
    'GET_MIN_MAX_RANGE': (SPELL, 'std::pair<float, float> Spell::GetMinMaxRange(bool strict)', ''),
    'CHECK_RANGE': (SPELL, 'SpellCastResult Spell::CheckRange(bool strict)', ''),
    'GET_DISTANCE': (OBJECT, 'float WorldObject::GetDistance(const WorldObject* obj, bool is3D, DistanceCalculation distcalc) const', ''),
    'GET_MIN': (MANAGER, 'inline float GetSpellMinRange(SpellRangeEntry const* range)', ''),
    'GET_MAX': (MANAGER, 'inline float GetSpellMaxRange(SpellRangeEntry const* range)', ''),
    'NEXT_SWING': (MANAGER, 'inline bool IsNextMeleeSwingSpell(SpellEntry const* spellInfo)', ''),
    'RANGE_ENTRY': (DBC, 'struct SpellRangeEntry\n', ';'),
    'ATTRIBUTE_GETTER': (DBC, 'inline bool HasAttribute(SpellAttributes attribute) const', ''),
    'ATTRIBUTE_EX3_GETTER': (DBC, 'inline bool HasAttribute(SpellAttributesEx3 attribute) const', ''),
    'ATTRIBUTES': (DEFINES, 'enum SpellAttributes\n', ';'),
    'ATTRIBUTES_EX3': (DEFINES, 'enum SpellAttributesEx3\n', ';'),
    'RESULTS': (DEFINES, 'enum SpellCastResult : uint32\n', ';'),
    'FACING': (DEFINES, 'enum SpellFacingFlags\n', ';'),
    'SPELL_MOD_OP': (DEFINES, 'enum SpellModOp\n', ';'),
    'DISTANCE_ENUM': (OBJECT_HEADER, 'enum DistanceCalculation\n', ';'),
    'RANGE_FLAGS': (SHARED, 'enum SpellRangeFlags\n', ';'),
    'ATTACK_TYPE': (SHARED, 'enum WeaponAttackType ', ';'),
    'TARGET_FLAGS': (TARGETS, 'enum SpellCastTargetFlags\n', ';'),
    'TARGET_ENUM': (TARGETS, 'enum SpellTarget\n', ';'),
    'TYPE_ID': (GUID, 'enum TypeID\n', ';'),
    'OBJECT_FIELDS': (UPDATE, 'enum EObjectFields\n', ';'),
    'UNIT_FIELDS': (UPDATE, 'enum EUnitFields\n', ';'),
    'GET_X': (OBJECT_HEADER, 'float GetPositionX() const { return m_position.x; }', ''),
    'GET_Y': (OBJECT_HEADER, 'float GetPositionY() const { return m_position.y; }', ''),
    'GET_Z': (OBJECT_HEADER, 'float GetPositionZ() const { return m_position.z; }', ''),
    'GET_TYPE': (OBJECT_HEADER, 'uint8 GetTypeId() const { return m_objectTypeId; }', ''),
    'IS_PLAYER': (OBJECT_HEADER, 'inline bool IsPlayer() const { return GetTypeId() == TYPEID_PLAYER; }', ''),
    'BASE_REACH': (OBJECT_HEADER, 'virtual float GetCombatReach() const { return 0.f; }', ''),
    'UNIT_REACH': (UNIT_HEADER, 'float GetCombatReach() const override { return m_floatValues[UNIT_FIELD_COMBATREACH]; }', ''),
    'MOD_OWNER': (PLAYER_HEADER, 'Player* GetSpellModOwner() const override { return const_cast<Player*>(this); }', ''),
    'APPLY_MOD': (PLAYER_HEADER, 'template <class T> void Player::ApplySpellMod(uint32 spellId, SpellModOp op, T& basevalue, bool finalUse)', ''),
}


def block(text, signature, suffix=''):
    if text.count(signature) != 1:
        raise SystemExit(f'Expected one exact source signature: {signature}')
    start = text.index(signature)
    end = text.index('{', start) + 1
    depth = 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    if text[end:end + len(suffix)] != suffix:
        raise SystemExit(f'Expected source suffix: {signature}')
    return text[start:end + len(suffix)]


def guarded(root):
    sources = {}
    for line in (HERE / 'spell_range_source_hashes.sha256').read_text().splitlines():
        expected, name = line.split('  ', 1)
        raw = (root / name).read_bytes().replace(b'\r\n', b'\n')
        if hashlib.sha256(raw).hexdigest() != expected:
            raise SystemExit(f'Wrong pinned source hash: {name}')
        if name in sources:
            raise SystemExit(f'Duplicate source dependency: {name}')
        sources[name] = raw.decode('latin1')
    if set(sources) != SOURCES:
        raise SystemExit('Unexpected spell range source dependency inventory')
    return sources


def source_harness(sources):
    replacements = {name: block(sources[source], signature, suffix)
                    for name, (source, signature, suffix) in BLOCKS.items()}
    # Keep the entire source declaration; no handwritten leeway constant.
    lines = [line for line in sources[OBJECT_DEFINES].splitlines()
             if line.startswith('static constexpr float const MELEE_LEEWAY ')]
    if len(lines) != 1:
        raise SystemExit('Expected one source MELEE_LEEWAY declaration')
    replacements['MELEE_LEEWAY'] = lines[0]
    text = (HERE / 'spell_range_oracle.cpp').read_text()
    for marker, original in replacements.items():
        marker = '// PINNED_' + marker
        if text.count(marker + '\n') != 1:
            raise SystemExit(f'Expected one harness marker: {marker}')
        text = text.replace(marker + '\n', original + '\n')
    if '// PINNED_' in text:
        raise SystemExit('Unexpanded source marker')
    notices = '\n'.join(source[:source.index('*/') + 2] for source in sources.values())
    return notices + '\n' + text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=pathlib.Path)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--sanitize', action='store_true')
    args = parser.parse_args()
    cpp = source_harness(guarded(args.source.resolve()))
    with tempfile.TemporaryDirectory(prefix='.spell-range-oracle-', dir=HERE) as directory:
        work = pathlib.Path(directory)
        source = work / 'oracle.cpp'
        source.write_text(cpp)
        program = work / 'oracle.test-fixture'
        compiler = shlex.split(os.environ.get('CXX', 'g++'))
        command = compiler + ['-std=c++17', '-O0', '-Wall', '-Wextra', '-Werror',
                              '-pedantic-errors', '-fno-fast-math', '-ffp-contract=off',
                              '-frounding-math', str(source), '-o', str(program)]
        if args.sanitize:
            command[len(compiler):len(compiler)] = [
                '-fsanitize=undefined,float-cast-overflow', '-fno-sanitize-recover=all']
        print('Compile: ' + shlex.join(command), flush=True)
        subprocess.run(command, check=True)
        subprocess.run([str(program), str(work)], check=True)
        for mode in ('melee', 'ranged', 'next_swing', 'ranged_slot', 'instant',
                     'player_target', 'self_target', 'corpse', 'gameobject', 'destination',
                     'source', 'facing', 'modifier', 'missing_row'):
            result = subprocess.run([str(program), '--unsupported', mode],
                                    text=True, capture_output=True)
            if result.returncode != 2 or 'outside bounded source host: ' not in result.stderr:
                raise SystemExit(f'Unsupported branch did not fail closed: {mode}')
        for name in ('spell-range.tsv', 'spell-range-excluded.tsv'):
            data = (work / name).read_bytes().replace(b'\r\n', b'\n')
            destination = HERE / 'fixtures' / name
            if args.check:
                if not destination.is_file() or destination.read_bytes().replace(b'\r\n', b'\n') != data:
                    raise SystemExit(f'Spell range fixture differs: {name}')
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
            print(f'{hashlib.sha256(data).hexdigest()}  {name} ({len(data.splitlines()) - 2} rows)')
        print(f'{"Verified" if args.check else "Generated"} exact-source spell range fixtures')


if __name__ == '__main__':
    main()
