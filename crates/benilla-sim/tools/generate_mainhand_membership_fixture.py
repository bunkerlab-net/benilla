#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate GPL source-extracted pristine mainhand membership and callback observations.
Uses the same pinned source hashes and exact methods as the existing derived oracle.
No equip scripts/enchants/aura modifiers, offhand/ranged equipment, disarm, combat GCD,
resource outputs, or zero Delay are claimed. Source RemoveItem/EquipItem order is
asserted from the pinned source before the adapter runs those ordering boundaries.
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
    remove=body(p,'void Player::RemoveItem(')
    equip=body(p,'Item* Player::EquipItem(')
    assert remove.index('_ApplyItemMods(pItem, slot, false)') < remove.index('m_items[slot] = nullptr')
    assert equip.index('VisualizeItem(slot, pItem)') < equip.index('_ApplyItemMods(pItem, slot, true)')
    dependent=body(s,'void Player::UpdateWeaponDependantStats(')
    assert 'UpdateWeaponHitChances(attType)' in dependent and 'Crit' not in dependent
    harness=pathlib.Path(__file__).with_name('mainhand_membership_oracle.cpp').read_text()
    assert harness.count('// PINNED_BODIES\n') == 1
    harness=harness.replace('// PINNED_BODIES\n', '\n'.join(chunks))
    with tempfile.TemporaryDirectory() as tmp:
        path=pathlib.Path(tmp)
        (path/'oracle.cpp').write_text(harness)
        subprocess.run(['g++','-std=c++17','-O0','-Wall','-Wextra','-Werror','-Wno-unused-parameter','-Wno-unused-but-set-variable','-fno-fast-math','-ffp-contract=off','-frounding-math','-fsanitize=undefined,float-cast-overflow',str(path/'oracle.cpp'),'-o',str(path/'oracle')],check=True)
        data=subprocess.check_output([str(path/'oracle')])
    output=pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/mainhand_membership_cmangos.tsv'
    if args.check:
        assert output.read_bytes() == data, 'fixture changed'
    else:
        output.write_bytes(data)
    print(len(data.splitlines()), hashlib.sha256(data).hexdigest())

if __name__ == '__main__':
    main()
