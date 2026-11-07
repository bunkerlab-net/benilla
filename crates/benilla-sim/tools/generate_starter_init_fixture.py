#!/usr/bin/env python3
"""Hash-checked CMaNGOS stat bodies, disabled-modifier and creation-order oracle.
Synthetic numeric inputs only. Usage: generate_starter_init_fixture.py UPSTREAM_DIR
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile
HASHES={
'Player.cpp':'d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e',
'Unit.cpp':'03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0',
'SpellAuras.cpp':'5e60510d23b789f8f0fb584da8f05a5be675f1cb4024ce997c989cde00105fde',
'StatSystem.cpp':'91aa0eb47c31aea9d97b64a97c1bdc350b4ea7e7fd30f1ce885a40b69112f1bb',
}
def body(text,signature):
 start=text.index(signature);opening=text.index('{',start);depth=1;end=opening+1
 while depth:
  depth+=(text[end]=='{')-(text[end]=='}');end+=1
 return text[start:end]+'\n'
def main():
 source={}
 for name,digest in HASHES.items():
  data=pathlib.Path(sys.argv[1],name).read_bytes()
  assert hashlib.sha256(data).hexdigest()==digest,name
  source[name]=data.decode()
 u,p,s=source['Unit.cpp'],source['Player.cpp'],source['StatSystem.cpp']
 assert 'm_canModifyStats = false;' in u
 create=body(p,'bool Player::Create(')
 assert 'SetCanModifyStats' not in create
 prefix=create[create.index('    // base stats and related field values'):create.index('    // original action bar')]
 assert prefix.index('InitStatsForLevel();')<prefix.index('UpdateMaxHealth();')<prefix.index('LearnDefaultSkills();')<prefix.index('learnDefaultSpells();')
 load=body(p,'bool Player::LoadFromDB(')
 assert load.index('SetCanModifyStats(true);')<load.index('UpdateAllStats();')<load.index('uint32 savedhealth = fields[45].GetUInt32();')
 mod=body(u,'bool Unit::HandleStatModifier(')
 # Unchanged branch that mutates neutral UnitMods while CanModifyStats=false.
 # Only BASE_VALUE and TOTAL_PCT are used in these probes. Keep exact arithmetic.
 begin=mod.index('    switch (modifierType)');end=mod.index('    if (!CanModifyStats())')
 mod=mod[begin:end]
 # TOTAL_VALUE's unrelated attack-power bookkeeping is never reached. Declare adapters.
 mod='float val; enum class AttackPowerModSign { MOD_SIGN_POS, MOD_SIGN_NEG }; enum class AttackPowerMod { MELEE_ATTACK_POWER, RANGED_ATTACK_POWER }; constexpr int UNIT_MOD_ATTACK_POWER=-1,UNIT_MOD_ATTACK_POWER_RANGED=-2; float m_attackPowerMod[2][2]{};\n'+mod+'\n(void)m_attackPowerMod; (void)modify;'
 item=body(p,'void Player::_ApplyItemBonuses('); item=item[:item.index('    if (proto->IsWeapon())')]+ '}\n'
 replacements={
 'AURA_TOTAL_STAT':body(source['SpellAuras.cpp'],'void Aura::HandleModTotalPercentStat('), 'ITEM_BONUSES':item,
 'MODIFIER':mod,'CREATE_PREFIX':prefix,'TOTAL_STAT':body(u,'float Unit::GetTotalStatValue('),'TOTAL_RESIST':body(u,'float Unit::GetTotalResistanceValue('),
 'HEALTH_BONUS':body(s,'float Unit::GetHealthBonusFromStamina(float'),'HEALTH_GETTER':body(s,'float Unit::GetHealthBonusFromStamina()'),
 'MANA_BONUS':body(s,'float Unit::GetManaBonusFromIntellect(float'),'MANA_GETTER':body(s,'float Unit::GetManaBonusFromIntellect()'),
 'MAX_HEALTH':body(s,'void Unit::UpdateMaxHealth()'),'MAX_MANA':body(s,'void Unit::UpdateMaxPower('),'ARMOR':body(s,'void Player::UpdateArmor()'),'UPDATE_ALL':body(s,'bool Player::UpdateAllStats()'),}
 harness=pathlib.Path(__file__).with_name('starter_init_oracle.cpp').read_text()
 for key,value in replacements.items():
  marker='// PINNED_'+key+'\n';assert harness.count(marker)==1;harness=harness.replace(marker,value)
 with tempfile.TemporaryDirectory() as d:
  root=pathlib.Path(d);(root/'oracle.cpp').write_text(harness)
  subprocess.run(['g++','-std=c++17','-O0','-Wall','-Wextra','-Werror','-Wno-misleading-indentation','-fno-fast-math','-ffp-contract=off','-frounding-math','-fsanitize=undefined,float-cast-overflow',str(root/'oracle.cpp'),'-o',str(root/'oracle')],check=True)
  data=subprocess.check_output([str(root/'oracle')])
  equipment=subprocess.check_output([str(root/'oracle'),'--equipment'])
 output=pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/starter_init_cmangos.tsv';output.write_bytes(data);print(len(data.splitlines()),hashlib.sha256(data).hexdigest())
 output.with_name('starter_equipment_cmangos.tsv').write_bytes(equipment);print(len(equipment.splitlines()),hashlib.sha256(equipment).hexdigest())
if __name__=='__main__':main()
