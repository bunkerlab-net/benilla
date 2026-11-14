#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extract pinned unchanged CMaNGOS death bodies; synthetic callback adapters only."""
import hashlib
import pathlib
import subprocess
import sys
import tempfile

HASHES = {
    'Unit.cpp': '03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0',
    'Player.cpp': 'd02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e',
    'SpellAuras.cpp': '5e60510d23b789f8f0fb584da8f05a5be675f1cb4024ce997c989cde00105fde',
    'SpellMgr.h': '91decb42afdb694496271d704c0e8a48c9c6fb4866b6c817e72e4b7cd2d3b12d',
    'SpellDefines.h': '07c3fd11a40c993ab8feeb718d21a7a2ae9b5c1700efd2e03ac53e76c6f938bf',
    'Spell.h': '62ee5ad8a5d5c3dab9a75923daadcf01b59c86ae8bce82752db3b9d3db694dee',
    'Spell.cpp': 'ebcabd3073f188541e6a1abcc82a8d0ee6d48d360dc5ab16d9e63b3e87d75201',
    'Unit.h': '2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1',
}

def body(source, signature):
    start = source.index(signature)
    end = source.index('{', start) + 1
    depth = 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end] + '\n'

def main():
    sources = {}
    for name, digest in HASHES.items():
        raw = pathlib.Path(sys.argv[1], name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != digest:
            raise SystemExit('Wrong upstream revision: ' + name)
        sources[name] = raw.decode()
    u, p = sources['Unit.cpp'], sources['Player.cpp']
    methods = ''.join(body(u, signature) for signature in (
        'void Unit::Kill(', 'void Unit::RemoveAllAurasOnDeath(',
        'void Unit::SetDeathState(', 'void Unit::InterruptSpell(',
        'void Unit::InterruptNonMeleeSpells(',
    )) + ''.join(body(p, signature) for signature in (
        'void Player::SetDeathState(', 'void Player::KillPlayer(',
        'void Player::UpdateCorpseReclaimDelay(', 'uint32 Player::getCorpseReclaimDelayHelper(',
        'uint32 Player::GetCorpseReclaimDelay(',
    ))
    update = body(p, 'void Player::Update(')
    # Verbatim, adjacent-order control blocks from the whole Player::Update.
    just_died = 'if (m_deathState == JUST_DIED)\n        KillPlayer();'
    assert just_died in update
    death_timer = body(update, 'if (m_deathTimer > 0  && !GetMap()->Instanceable())')
    assert update.index(just_died) < update.index(death_timer.rstrip())
    # Verify delegated semantic boundaries separately; the full dependency graph
    # is intentionally not replaced by an emulator or a test database.
    assert 'getThreatManager().clearReferences();' in body(u, 'void Unit::DeleteThreatList(')
    assert 'getHostileRefManager().deleteReferences();' in body(u, 'void Unit::DeleteThreatList(')
    combat = body(u, 'void Unit::CombatStop(')
    ordered = ['AttackStop(true, includingCast, includingCombo);', 'RemoveAllAttackers();',
               'DeleteThreatList();', 'GetCombatManager().StopCombatTimer();', 'ClearInCombat();']
    assert [combat.index(call) for call in ordered] == sorted(combat.index(call) for call in ordered)
    clear = body(u, 'void Unit::ClearInCombat(')
    assert 'UNIT_FLAG_IN_COMBAT' in clear and 'UNIT_FLAG_PET_IN_COMBAT' in clear
    assert 'pvpInfo.inPvPCombat = false;' in clear
    assert 'i = 0;' in body(u, 'void Unit::ClearAllReactives(')
    assert 'ModifyAuraState(AURA_STATE_DEFENSE, false);' in body(u, 'void Unit::ClearAllReactives(')
    assert 'm_spellState <= SPELL_STATE_DELAYED || m_spellState == SPELL_STATE_CHANNELING' in sources['Spell.h']
    assert 'm_isPassive      = IsPassiveSpell(spellproto);' in sources['SpellAuras.cpp']
    assert 'm_isDeathPersist = IsDeathPersistentSpell(spellproto);' in sources['SpellAuras.cpp']
    assert 'SPELL_ATTR_EX3_ALLOW_AURA_WHILE_DEAD);' in body(sources['SpellMgr.h'], 'inline bool IsDeathPersistentSpell(')
    assert 'SPELL_ATTR_EX3_ALLOW_AURA_WHILE_DEAD       = 0x00100000' in sources['SpellDefines.h']
    assert 'SPELL_ATTR_PASSIVE                         = 0x00000040' in sources['SpellDefines.h']
    cancel = body(sources['Spell.cpp'], 'void Spell::cancel(')
    assert 'm_caster->ResetGCD(m_spellInfo);' in cancel
    assert 'finish(false);' in cancel
    assert 'RemoveAurasByCasterSpell' in cancel
    assert 'RemoveDynObject' in cancel and 'RemoveGameObject' in cancel and 'RemoveCreature' in cancel
    assert 'spellProto->CasterAuraState == flag' in body(u, 'void Unit::ModifyAuraState(')
    harness = pathlib.Path(__file__).with_name('player_death_oracle.cpp').read_text()
    for key, value in {'METHODS': methods, 'JUST_DIED': just_died, 'TIMER': death_timer}.items():
        marker = '// PINNED_' + key + '\n'
        assert harness.count(marker) == 1
        harness = harness.replace(marker, value + '\n')
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / 'oracle.cpp').write_text(harness)
        subprocess.run(['g++', '-std=c++17', '-O0', '-Wall', '-Wextra', '-Werror',
                        '-Wno-unused-parameter', '-Wno-unused-variable', '-Wno-unused-but-set-variable',
                        '-fsanitize=undefined', '-fno-sanitize-recover=all',
                        str(root / 'oracle.cpp'), '-o', str(root / 'oracle')], check=True)
        data = subprocess.check_output([str(root / 'oracle')])
    path = pathlib.Path(__file__).resolve().parents[1] / 'tests/fixtures/player_death_cmangos.tsv'
    path.write_bytes(data)
    print(len(data.splitlines()), hashlib.sha256(data).hexdigest())

if __name__ == '__main__':
    main()
