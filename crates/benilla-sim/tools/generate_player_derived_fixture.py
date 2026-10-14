#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extract unchanged pinned CMaNGOS ancillary/item callback methods and compare bits.

Usage: generate_player_derived_fixture.py UPSTREAM_DIR [--check]
Synthetic mage levels1-9; no database/DBC/installation data. The harness resolves
empty aura lists, no shape/disarm/parry/block/dual-wield/ammo/enchantments. Native
weapon damage is 4.25-7.75 with2750ms delay (mode7:zero authored delay). Skills
including both bonuses: defense4, equipped4, unarmed2. Modes0-8 break a plain
weapon, agility weapon, intellect weapon, agility armor, strength weapon, spirit
weapon, stamina weapon, zero-delay plain weapon, repeated-agility/intellect weapon.
Stages: full UpdateAllStats, actual _ApplyItemBonuses(false) followed by committing
broken durability, then full UpdateAllStats again. Whole UpdateStats and item
bonus bodies retain callback ordering; resource/armor effects are outside output.
Create attributes [17+L,23+L,29+L,31+2L,37+L], item bonuses5 except mode8's added
intellect7 and second agility3. Modifier adapter is an exact flat BASE_VALUE add
and attribute dispatch only. Output43 integer cells: APbase2,totalbits2,damagebits6,
critbase3,critinternal3,critdisplay2,spellcrit7,avoidance6,spellbonus7,regen2,hit3.
This checks pinned-port parity, not independently verified retail coefficients.
"""
import argparse
import hashlib
import pathlib
import subprocess
import tempfile

HASHES = {
    'Player.cpp': 'd02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e',
    'Player.h': 'acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb',
    'Unit.cpp': '03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0',
    'Unit.h': '2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1',
    'StatSystem.cpp': '91aa0eb47c31aea9d97b64a97c1bdc350b4ea7e7fd30f1ce885a40b69112f1bb',
    'Object.cpp': '410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b',
}

def body(source, signature):
    assert source.count(signature) == 1, signature
    start = source.index(signature)
    end = source.index('{', start) + 1
    depth = 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end] + '\n'

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('upstream', type=pathlib.Path)
    ap.add_argument('--check', action='store_true')
    args = ap.parse_args()
    source = {}
    for name, digest in HASHES.items():
        data = (args.upstream / name).read_bytes()
        assert hashlib.sha256(data).hexdigest() == digest, name
        source[name] = data.decode()
    p,u,s,o = (source[x] for x in ['Player.cpp','Unit.cpp','StatSystem.cpp','Object.cpp'])
    chunks = []
    for text, signatures in [
        (p, ['float Player::GetMeleeCritFromAgility()', 'float Player::GetDodgeFromAgility(', 'float Player::GetSpellCritFromIntellect()', 'void Player::_ApplyItemBonuses(']),
        (u, ['float Unit::GetTotalStatValue(', 'float Unit::GetTotalAttackPowerValue(', 'float Unit::GetBaseWeaponDamage(', 'float Unit::GetAPMultiplier(', 'float Unit::OCTRegenMPPerSpirit()', 'int32 Unit::SpellBaseDamageBonusDone(']),
        (o, ['void Object::SetStatFloatValue(', 'void Object::SetStatInt32Value(']),
    ]:
        chunks.extend(body(text, signature) for signature in signatures)
    start=s.index('static const float PLAYER_BASE_DODGE[')
    chunks.append(s[start:s.index('};', start)+2]+'\n')
    for signature in ['void Player::UpdateAttackPowerAndDamage(', 'void Player::CalculateMinMaxDamage(', 'void Player::UpdateDamagePhysical(', 'void Player::UpdateDefenseBonusesMod()', 'void Player::UpdateBlockPercentage()', 'void Player::UpdateCritPercentage(', 'void Player::UpdateAllCritPercentages()', 'void Player::UpdateParryPercentage()', 'void Player::UpdateDodgePercentage()', 'void Player::UpdateSpellCritChance(', 'void Player::UpdateAllSpellCritChances()', 'void Player::UpdateWeaponHitChances(', 'void Player::UpdateSpellDamageBonus()', 'void Player::UpdateManaRegen()', 'bool Player::UpdateStats(', 'bool Player::UpdateAllStats()']:
        chunks.append(body(s, signature))
    assert '#define BASE_MINDAMAGE 1.0f' in source['Unit.h']
    assert '#define BASE_MAXDAMAGE 2.0f' in source['Unit.h']
    assert '#define BASE_ATTACK_TIME 2000' in source['Unit.h']
    assert 'return m_auraBaseMod[modGroup][FLAT_MOD] + m_auraBaseMod[modGroup][PCT_MOD];' in source['Player.h']
    skill=body(u,'uint32 Unit::GetWeaponSkillValue(')
    assert 'GetWeaponForAttack(attType, true, true)' in skill
    assert 'item ? item->GetSkill() : uint32(SKILL_UNARMED)' in skill
    loss=body(p,'void Player::DurabilityPointsLoss(')
    assert loss.index('_ApplyItemMods(item, item->GetSlot(), false);') < loss.index('item->SetUInt32Value(ITEM_FIELD_DURABILITY, pNewDurability);')
    harness=pathlib.Path(__file__).with_name('player_derived_oracle.cpp').read_text()
    assert harness.count('// PINNED_BODIES\n') == 1
    harness=harness.replace('// PINNED_BODIES\n', '\n'.join(chunks))
    with tempfile.TemporaryDirectory() as tmp:
        path=pathlib.Path(tmp)
        (path/'oracle.cpp').write_text(harness)
        subprocess.run(['g++','-std=c++17','-O0','-Wall','-Wextra','-Werror','-Wno-unused-parameter','-Wno-unused-but-set-variable','-fno-fast-math','-ffp-contract=off','-frounding-math','-fsanitize=undefined,float-cast-overflow',str(path/'oracle.cpp'),'-o',str(path/'oracle')],check=True)
        data=subprocess.check_output([str(path/'oracle')])
    output=pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/player_derived_cmangos.tsv'
    if args.check:
        assert output.read_bytes() == data, 'fixture changed'
    else:
        output.write_bytes(data)
    print(len(data.splitlines()), hashlib.sha256(data).hexdigest())

if __name__ == '__main__':
    main()
