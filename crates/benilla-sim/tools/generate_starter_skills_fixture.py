#!/usr/bin/env python3
"""Pinned original default/step skill methods and range dispatch; synthetic rows only.
Usage: generate_starter_skills_fixture.py UPSTREAM_DIR OBJECTMGR_CPP
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile
from generate_starter_init_fixture import body
HASHES={
'Player.h':'acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb',
'Player.cpp':'d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e',
'SpellMgr.cpp':'d1a7064febbe3a7657e85931c4bda89a09adfabc89b7cbed23fdbffd6e37c22d',
'SpellMgr.h':'91decb42afdb694496271d704c0e8a48c9c6fb4866b6c817e72e4b7cd2d3b12d',
'SpellEffects.cpp':'d4335082b8b3ee60d9bc200fbb47e87887dbdbe8b78a474219b68b452744520d',
'ObjectMgr.cpp':'dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b',}
def main():
 source={}
 for name,digest in HASHES.items():
  path=pathlib.Path(sys.argv[2]) if name=='ObjectMgr.cpp' else pathlib.Path(sys.argv[1],name)
  data=path.read_bytes();assert hashlib.sha256(data).hexdigest()==digest,name;source[name]=data.decode()
 assert 'typedef std::unordered_map<uint32, SkillStatusData> SkillStatusMap;' in source['Player.h']
 p=source['Player.cpp'];mgr=source['SpellMgr.cpp'];h=source['SpellMgr.h'];obj=source['ObjectMgr.cpp']
 replacements={'LOAD_SPELLS':body(p,'void Player::_LoadSpells('),'DEFAULT_SPELLS':body(p,'void Player::learnDefaultSpells()'),'LOAD_SKILLS':body(p,'void Player::_LoadSkills('),'PRIMARY_PROFESSION':body(mgr,'bool IsPrimaryProfessionSkill('),'IS_PROFESSION':body(h,'inline bool IsProfessionSkill('),'IS_RIDING':body(h,'inline bool IsProfessionOrRidingSkill('),'SKILL_RANGE':body(obj,'SkillRangeType GetSkillRangeType('),'GET_INFO':body(p,'SkillRaceClassInfoEntry const* Player::GetSkillInfo('),'DEFAULT_SKILLS':body(p,'void Player::LearnDefaultSkills()'),'SET_STEP':body(p,'void Player::SetSkillStep(')}
 replacements['PROFICIENCY']=body(source['SpellEffects.cpp'],'void Spell::EffectProficiency(')
 harness=pathlib.Path(__file__).with_name('starter_skills_oracle.cpp').read_text()
 for key,value in replacements.items():
  marker='// PINNED_'+key+'\n';assert harness.count(marker)==1;harness=harness.replace(marker,value)
 with tempfile.TemporaryDirectory() as d:
  root=pathlib.Path(d);(root/'oracle.cpp').write_text(harness)
  subprocess.run(['g++','-std=c++17','-O0','-Wall','-Wextra','-Werror','-Wno-unused-but-set-variable','-fsanitize=undefined',str(root/'oracle.cpp'),'-o',str(root/'oracle')],check=True)
  data=subprocess.check_output([str(root/'oracle')])
 lines=data.splitlines(keepends=True);reload=b'# synthetic source first-entry skill reload, not game rows\n'+b''.join(l for l in lines if l.startswith(b'L\t'));phases=b''.join(l for l in lines if l.startswith(b'P\t'));proficiency=b'# synthetic original EffectProficiency cases, not game rows\n'+b''.join(l for l in lines if l.startswith(b'F\t'));data=b''.join(l for l in lines if not l.startswith((b'L\t',b'P\t',b'F\t')))
 output=pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/starter_skills_cmangos.tsv';output.write_bytes(data);print(len(data.splitlines()),hashlib.sha256(data).hexdigest())
 output.with_name('starter_reload_cmangos.tsv').write_bytes(reload);print(len(reload.splitlines()),hashlib.sha256(reload).hexdigest())
 output.with_name('starter_entry_phases_cmangos.tsv').write_bytes(phases);print(phases.decode().strip(),hashlib.sha256(phases).hexdigest())
 output.with_name('starter_proficiency_cmangos.tsv').write_bytes(proficiency);print(len(proficiency.splitlines()),hashlib.sha256(proficiency).hexdigest())
if __name__=='__main__':main()
