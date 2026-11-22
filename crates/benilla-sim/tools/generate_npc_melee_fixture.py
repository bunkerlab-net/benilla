#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Hash-check source, extract unchanged methods, run synthetic C++ differential oracle."""
import hashlib,pathlib,re,subprocess,sys,tempfile
HASHES={
'StatSystem.cpp':'91aa0eb47c31aea9d97b64a97c1bdc350b4ea7e7fd30f1ce885a40b69112f1bb',
'Unit.cpp':'03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0',
'Unit.h':'2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1',
'Player.cpp':'d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e',
'Util.h':'1f53eafa77ab89935c905f331d9ee7e5e2fabf1fca7f812e2e913f19243c5878',
'UnitAuraProcHandler.cpp':'04e5059490e9a7a0435c4b94b3f041aa5f26552d0948886f3d2ec4a61712294e',
}
def body(s,signature):
 start=s.index(signature);end=s.index('{',start)+1;depth=1
 while depth:
  depth+=(s[end]=='{')-(s[end]=='}');end+=1
 return s[start:end]+'\n'
def main():
 sources={}
 for name,digest in HASHES.items():
  raw=pathlib.Path(sys.argv[1],name).read_bytes()
  if hashlib.sha256(raw).hexdigest()!=digest:raise SystemExit('Wrong upstream revision: '+name)
  sources[name]=raw.decode()
 u,p,h,util=(sources[n]for n in('Unit.cpp','Player.cpp','Unit.h','Util.h'))
 methods=''.join(body(u,s) for s in[
 'float Unit::CalculateEffectiveMissChance(', 'float Unit::CalculateEffectiveDodgeChance(',
 'float Unit::CalculateEffectiveParryChance(', 'float Unit::CalculateEffectiveBlockChance(',
 'float Unit::CalculateEffectiveCritChance(', 'float Unit::CalculateEffectiveCrushChance(',
 'MeleeHitOutcome Unit::RollMeleeOutcomeAgainst(', 'uint32 Unit::CalculateDamage(',
 'void Unit::CalculateMeleeDamage(', 'uint32 Unit::CalculateCritAmount(CalcDamageInfo*',
 'float Unit::CalcArmorReducedDamage('])+body(p,'float Player::GetDodgeFromAgility(')
 methods+=body(sources['StatSystem.cpp'],'void Creature::UpdateAttackPowerAndDamage(')+body(sources['StatSystem.cpp'],'void Creature::UpdateDamagePhysical(')
 enums=body(h,'enum MeleeHitOutcome')+';\n'+body(h,'enum UnitCombatDieSide')+';\n#define NUM_UNIT_COMBAT_DIE_SIDES (UNIT_COMBAT_DIE_HIT + 1)\n'
 die=body(util,'inline uint32 chance_u(')+body(util,'template<class Side, Side Default, uint8 Sides>')+';\n'
 flags=sorted(set(re.findall(r'\b(?:HITINFO|VICTIMSTATE|PROC_FLAG|PROC_EX)_[A-Z0-9_]+',methods)))
 flags='enum{'+','.join(f+'='+str(i+1)for i,f in enumerate(flags))+'};\n'
 skill=body(p,'void Player::UpdateCombatSkills(')
 quadratic=re.search(r'baseChance = \(room \* room\) / \(7.0 \* skillMax\) \* levelCapScale;',skill).group().replace('skillMax','maximum')
 final=re.search(r'const float finalChance =[^;]+;',skill).group()
 # Proc-skill gate provenance is checked independently, including post-death suppression.
 proc=sources['UnitAuraProcHandler.cpp']
 assert 'data.victim && data.victim->IsAlive() && data.procFlagsVictim' in proc
 assert 'PROC_EX_NORMAL_HIT | PROC_EX_CRITICAL_HIT | PROC_EX_MISS | PROC_EX_DODGE | PROC_EX_PARRY | PROC_EX_BLOCK' in proc
 assert 'target->GetCreatureType() != CREATURE_TYPE_CRITTER' in proc
 harness=pathlib.Path(__file__).with_name('npc_melee_oracle.cpp').read_text()
 for key,value in {'ENUMS':enums,'DIE':die,'FLAGS':flags,'METHODS':methods,'SKILL_QUADRATIC':quadratic,'SKILL_FINAL':final}.items():
  marker='// PINNED_'+key+'\n';assert harness.count(marker)==1;harness=harness.replace(marker,value)
 with tempfile.TemporaryDirectory()as temp:
  root=pathlib.Path(temp);(root/'oracle.cpp').write_text(harness)
  subprocess.run(['g++','-std=c++17','-O0','-Wall','-Wextra','-Werror','-Wno-unused-parameter','-Wno-unused-variable','-Wno-unused-but-set-variable','-fno-fast-math','-ffp-contract=off','-frounding-math','-fsanitize=undefined,float-cast-overflow','-fno-sanitize-recover=all',str(root/'oracle.cpp'),'-o',str(root/'oracle')],check=True)
  data=subprocess.check_output([str(root/'oracle')])
 path=pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/npc_melee_cmangos.tsv';path.write_bytes(data)
 print(len(data.splitlines()),hashlib.sha256(data).hexdigest())
if __name__=='__main__':main()
