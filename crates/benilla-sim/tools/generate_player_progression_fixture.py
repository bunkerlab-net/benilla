#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build a hash-checked CMaNGOS player-progression oracle (synthetic inputs only).

Usage: generate_player_progression_fixture.py UPSTREAM_DIR OBJECTMGR_CPP [--check]
Pinned upstream: cmangos/mangos-classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
No game database or client-install rows are included.

The generated translation unit inserts whole, unchanged GiveXP, GiveLevel,
UpdateSkillsForLevel, GetSkillInfo, both SetSkill overloads, GetSkillRangeType,
profession classifiers, Unit resource setters/getters, and stat numeric bodies.
The adapter records unsupported dependent-system callbacks, rather than pretending
that their bodies are ported. There are no auras, pets, groups or items. Profiles 0/1 have neutral
stat modifiers; profiles 2/3/4 apply the explicit constant modifiers below. XP is non-kill XP (victim=null, groupRate=1); levels are 1 through 9.

Synthetic rows for level L: next XP=100+37*L below the configured level cap,
and zero at the cap through the unchanged ObjectMgr::GetXPForLevel getter. The
loader-sized XP vector has exactly cap elements; no above-cap inputs are used. Profile 0: create health=40+7*L,
create mana=55+11*L, create stat i=12+2*L+3*i. Profile 1: health=200-7*L,
mana=300-11*L, stat i=12+2*L+3*i. Stat order is strength/agility/stamina/
intellect/spirit. Source player create powers are mana/create mana, rage/1000,
focus/0, energy/100, happiness/0. Played time starts at 77, free talents at 0,
and session level at start_level. The global skill cap is the unchanged source
World getter: cap>60 ? 300+((cap-60)*75)/10 : cap*5. Ordinary cap9 gives45;
all fixture inputs use this canonical getter, with no raw global-cap overrides.
Each case has exactly one existing skill, so
std::unordered_map iteration order cannot affect the fixture.

Profiles 2/3/4 use profile0 create rows. Modifier tuples are (BASE_VALUE,
BASE_PCT,TOTAL_VALUE,TOTAL_PCT), every literal f32; all unlisted slots neutral.
Profile2: each stat i=(3+i,1.05,-2,1.17); health=(7,1.10,-3,1.07);
mana=(11,.95,5,1.03); armor=(13,1,7,1.15).
Profile3: strength=(0,1,-100,1); agility=(0,1,0,0);
stamina=(1.25,.5,2.25,1.125); intellect=(-3,1.01,.5,.93);
spirit=(0,1,0,-.5); health=(1.25,.75,-1.5,1.25);
mana=(2.5,1.125,-1.25,.875); armor=(5.5,.9,-3.25,1.375).
Profile4: health=(.5,1,0,1); mana=(-.5,1,0,1), to probe exact half-away
resource rounding. Rage/focus/energy/happiness modifiers are always neutral.

Skill cases, with default status 0=UNCHANGED, masks race1/class128, flags0:
 0: weapon id60001 cat6, value3, max=start_level*5, step2
 1: case0, flags16 (MAXIMIZED)
 2: armor id60002 cat8, value1, max1, step0
 3: class id60003 cat7, value3, max=start_level*5, step1
 4: language id60004 cat10, value7, max300, step0
 5: profession id60005 cat11, value3, max75, step1
 6: generic id60006 cat12, value3, max=start_level*5, step0
 7: case0, status3=DELETED
 8: case0, absent SkillLine lookup
 9: case1, wrong info race mask2
10: defense id95 cat6, value3, max=start_level*5, step0
11: case1, status2=NEW
12: fist id473 cat6, value1, max1, step0
13: case1, max1 (source skips even a level-range skill)
Status1=CHANGED. SkillLine/SkillRaceClassInfo lookups exist except where stated.

Rows X call GiveXP(amount), L call GiveLevel(target), S call
UpdateSkillsForLevel(bool(amount)). The maximize column is the world config.
CSV cells hold the five base/effective stats; packets are semicolon-separated
CSV u32 payloads of SMSG_LEVELUP_INFO (12 words). '-' denotes an empty packet,
trace or XP-log cell. Trace tokens record writes/callbacks at adapter boundaries;
full SetMaxPower/SetPower bodies retain source early returns and clamping order.
Repeated armor/defense callbacks are intentionally retained. Trace alone does
not prove the behavior inside attack, crit, defense, talent, trained-spell, regen,
spell-damage or session callbacks. These are explicitly logged host boundaries.

A few raw boundary rows have old XP >= next XP or rage > maximum: these exercise
source arithmetic/clamping, not a claim that validated game snapshots allow it.
GetCreateStat retains its source float return type. Negative float-to-u32 stat
packet conversion is not portable C++; descending-stat GiveLevel transitions are
excluded, and UBSan plus float-cast-overflow remain enabled. Profile 1 still
exercises negative health/mana packet deltas through defined signed-to-u32
wrapping. The generator never replaces these source expressions with Rust logic.
"""
import argparse
import hashlib
import pathlib
import re
import subprocess
import tempfile

HASHES = {
    'DBCEnums.h': 'cfd4b3d04cc31773173e9d47bb187492f7f144e3cf0f783d8fbf2d7969ebd4ce',
    'DBCStructure.h': '354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50',
    'Player.cpp': 'd02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e',
    'Player.h': 'acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb',
    'World.cpp': '405e2a0def584dc2e4a9a9bc2861aac4aacefcecc8ec2fa08856324e32d866dd',
    'World.h': '3fa53d2883533f4d205c9aeaf13af224670d12f5e623cc1fd79dc9eca4de16a3',
    'Unit.cpp': '03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0',
    'Unit.h': '2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1',
    'StatSystem.cpp': '91aa0eb47c31aea9d97b64a97c1bdc350b4ea7e7fd30f1ce885a40b69112f1bb',
    'SpellMgr.cpp': 'd1a7064febbe3a7657e85931c4bda89a09adfabc89b7cbed23fdbffd6e37c22d',
    'SpellMgr.h': '91decb42afdb694496271d704c0e8a48c9c6fb4866b6c817e72e4b7cd2d3b12d',
    'SharedDefines.h': '4832b8e0da45211143acae7c42173fea42732e2acdbf190692725e51b020bfe8',
    'Object.cpp': '410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b',
    'ObjectMgr.cpp': 'dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b',
}


def body(source, signature):
    if source.count(signature) != 1:
        raise ValueError('Signature is not unique: ' + signature)
    start = source.index(signature)
    end = source.index('{', start) + 1
    depth = 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end] + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('upstream', type=pathlib.Path)
    parser.add_argument('objectmgr', type=pathlib.Path)
    parser.add_argument('--check', action='store_true', help='compare without rewriting fixture')
    args = parser.parse_args()
    sources = {}
    for name, digest in HASHES.items():
        path = args.objectmgr if name == 'ObjectMgr.cpp' else args.upstream / name
        raw = path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != digest:
            raise SystemExit('Wrong upstream revision: ' + str(path))
        sources[name] = raw.decode()
    p, u, s = (sources[n] for n in ('Player.cpp', 'Unit.cpp', 'StatSystem.cpp'))
    assert 'typedef std::unordered_map<uint32, SkillStatusData> SkillStatusMap;' in sources['Player.h']
    assert 'float GetCreateStat(Stats stat) const { return m_createStats[stat]; }' in sources['Unit.h']
    assert 'uint16 GetSkillMaxForLevel(Unit const* target = nullptr) const { return (target ? GetLevelForTarget(target) : GetLevel()) * 5; }' in sources['Unit.h']
    for name, value in [('RAGE', 1000), ('FOCUS', 100), ('ENERGY', 100), ('HAPPINESS', 1000000)]:
        assert f'POWER_{name}_DEFAULT' in sources['Unit.h']
        assert re.search(rf'POWER_{name}_DEFAULT\s*=\s*{value}\b', sources['Unit.h'])
    assert re.search(r'SKILL_FLAG_MAXIMIZED\s*=\s*0x010\b', sources['DBCEnums.h'])
    assert 'mPlayerXPperLevel.resize(sWorld.getConfig(CONFIG_UINT32_MAX_PLAYER_LEVEL));' in sources['ObjectMgr.cpp']
    assert re.search(r'#define\s+MAX_LEVEL\s+100\b', sources['DBCEnums.h'])
    assert re.search(r'#define\s+DEFAULT_MAX_LEVEL\s+60\b', sources['DBCEnums.h'])
    assert 'setConfigMinMax(CONFIG_UINT32_MAX_PLAYER_LEVEL, "MaxPlayerLevel", DEFAULT_MAX_LEVEL, 1, MAX_LEVEL);' in sources['World.cpp']
    assert 'setConfig(CONFIG_BOOL_ALWAYS_MAX_SKILL_FOR_LEVEL, "AlwaysMaxSkillForLevel", false);' in sources['World.cpp']
    replacements = {
        'GLOBAL_SKILL_CAP': body(sources['World.h'], 'uint16 GetConfigMaxSkillValue() const'),
        'GET_XP_FOR_LEVEL': body(sources['ObjectMgr.cpp'], 'uint32 ObjectMgr::GetXPForLevel('),
        'LEVEL_STRUCT': body(sources['Player.h'], 'struct PlayerLevelInfo\n') + ';\n',
        'CLASS_STRUCT': body(sources['Player.h'], 'struct PlayerClassLevelInfo\n') + ';\n',
        'PRIMARY_PROFESSION': body(sources['SpellMgr.cpp'], 'bool IsPrimaryProfessionSkill('),
        'IS_PROFESSION': body(sources['SpellMgr.h'], 'inline bool IsProfessionSkill('),
        'IS_RIDING': body(sources['SpellMgr.h'], 'inline bool IsProfessionOrRidingSkill('),
        'SKILL_RANGE': body(sources['ObjectMgr.cpp'], 'SkillRangeType GetSkillRangeType('),
        'GET_SKILL_INFO': body(p, 'SkillRaceClassInfoEntry const* Player::GetSkillInfo('),
        'SET_SKILL_ITER': body(p, 'void Player::SetSkill(SkillStatusMap::iterator'),
        'SET_SKILL_ID': body(p, 'void Player::SetSkill(uint16 id'),
        'UPDATE_SKILLS': body(p, 'void Player::UpdateSkillsForLevel('),
        'GIVE_XP': body(p, 'void Player::GiveXP('),
        'GIVE_LEVEL': body(p, 'void Player::GiveLevel('),
        'MODIFIER_VALUE': body(u, 'float Unit::GetModifierValue('),
        'TOTAL_STAT': body(u, 'float Unit::GetTotalStatValue('),
        'TOTAL_RESIST': body(u, 'float Unit::GetTotalResistanceValue('),
        'CREATE_POWERS': body(u, 'uint32 Unit::GetCreatePowers('),
        'SET_HEALTH': body(u, 'void Unit::SetHealth('),
        'SET_MAX_HEALTH': body(u, 'void Unit::SetMaxHealth('),
        'SET_POWER': body(u, 'void Unit::SetPower('),
        'SET_MAX_POWER': body(u, 'void Unit::SetMaxPower('),
        'HEALTH_BONUS': body(s, 'float Unit::GetHealthBonusFromStamina(float'),
        'HEALTH_GETTER': body(s, 'float Unit::GetHealthBonusFromStamina()'),
        'MANA_BONUS': body(s, 'float Unit::GetManaBonusFromIntellect(float'),
        'MANA_GETTER': body(s, 'float Unit::GetManaBonusFromIntellect()'),
        'MAX_HEALTH': body(s, 'void Unit::UpdateMaxHealth()'),
        'MAX_POWER': body(s, 'void Unit::UpdateMaxPower('),
        'ARMOR': body(s, 'void Player::UpdateArmor()'),
        'RESISTANCES': body(s, 'void Player::UpdateResistances('),
        'UPDATE_ALL': body(s, 'bool Player::UpdateAllStats()'),
        'STAT_INT32': body(sources['Object.cpp'], 'void Object::SetStatInt32Value('),
    }
    harness = pathlib.Path(__file__).with_name('player_progression_oracle.cpp').read_text()
    for key, value in replacements.items():
        marker = '// PINNED_' + key + '\n'
        if harness.count(marker) != 1:
            raise ValueError('Missing or duplicate marker: ' + marker)
        harness = harness.replace(marker, value)
    assert '// PINNED_' not in harness
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / 'oracle.cpp').write_text(harness)
        subprocess.run(['g++', '-std=c++17', '-O0', '-Wall', '-Wextra', '-Werror',
                        '-fno-fast-math', '-ffp-contract=off', '-frounding-math',
                        '-fsanitize=undefined,float-cast-overflow',
                        '-fno-sanitize-recover=all', str(root / 'oracle.cpp'),
                        '-o', str(root / 'oracle')], check=True)
        data = subprocess.check_output([str(root / 'oracle')])
    path = pathlib.Path(__file__).resolve().parents[1] / 'tests/fixtures/player_progression_cmangos.tsv'
    if args.check:
        if path.read_bytes() != data:
            raise SystemExit('Fixture differs: ' + str(path))
    else:
        path.write_bytes(data)
    rows = sum(not line.startswith(b'#') for line in data.splitlines())
    print(f'{rows} rows {len(data)} bytes sha256={hashlib.sha256(data).hexdigest()}')


if __name__ == '__main__':
    main()
