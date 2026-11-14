#!/usr/bin/env python3
"""Hash-check pinned CMaNGOS; insert unchanged methods/blocks and run C++ oracle.

Usage: python3 tools/generate_mage_resource_fixture.py /path/to/upstream/files
Synthetic numeric and state probes only, with no database/DBC content.
"""
# SPDX-License-Identifier: GPL-3.0-or-later
import hashlib
import pathlib
import subprocess
import sys
import tempfile

HASHES = {
    "Player.cpp": "d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e",
    "Player.h": "acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb",
    "Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "Unit.h": "2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1",
    "StatSystem.cpp": "91aa0eb47c31aea9d97b64a97c1bdc350b4ea7e7fd30f1ce885a40b69112f1bb",
    "SpellMgr.cpp": "d1a7064febbe3a7657e85931c4bda89a09adfabc89b7cbed23fdbffd6e37c22d",
    "SpellMgr.h": "91decb42afdb694496271d704c0e8a48c9c6fb4866b6c817e72e4b7cd2d3b12d",
    "Spell.cpp": "ebcabd3073f188541e6a1abcc82a8d0ee6d48d360dc5ab16d9e63b3e87d75201",
}


def body(text, signature):
    start = text.index(signature)
    opening = text.index("{", start)
    depth, end = 1, opening + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end] + "\n"


def main():
    source = {}
    for name, digest in HASHES.items():
        data = pathlib.Path(sys.argv[1], name).read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise SystemExit("Wrong upstream revision: " + name)
        source[name] = data.decode()
    p, u, stats = source["Player.cpp"], source["Unit.cpp"], source["StatSystem.cpp"]
    timer = body(u, "void Unit::Update(const uint32 diff)")
    timer = timer[timer.index("    if (m_lastManaUseTimer)"):timer.index("    if (uint32 base_att")]
    schedule = body(p, "void Player::Update(const uint32 diff)")
    schedule = schedule[schedule.index("    if (IsAlive())\n    {\n        m_regenTimer"):schedule.index("    if (m_deathState == JUST_DIED)")]
    # Checked dependencies for synthetic adapter methods that replace server state.
    assert "void SetLastManaUse() { m_lastManaUseTimer = 5000; }" in source["Unit.h"]
    assert "#define REGEN_TIME_FULL         2000" in source["Unit.h"]
    assert "return GetSkill(id, true, true);" in source["Player.h"]
    load = body(source["SpellMgr.cpp"], "void SpellMgr::LoadSkillLineAbilityMaps()")
    assert "for (uint32 row = 0; row < rows; ++row)" in load
    assert "mSkillLineAbilityMapBySpellId.insert(SkillLineAbilityMap::value_type(entry->spellId, entry));" in load
    unit_update = body(u, "void Unit::Update(const uint32 diff)")
    assert unit_update.index("m_events.Update(diff);") < unit_update.index("_UpdateSpells(diff);") < unit_update.index("if (m_lastManaUseTimer)")
    spells = body(u, "void Unit::_UpdateSpells(uint32 time)")
    assert "i_holder->UpdateHolder(time);" in spells
    player_update = body(p, "void Player::Update(const uint32 diff)")
    assert player_update.index("Unit::Update(diff);") < player_update.index("m_regenTimer += diff;")
    take = body(source["Spell.cpp"], "void Spell::TakePower()")
    assert "powerType == POWER_MANA && m_powerCost > 0 && !m_spellInfo->HasAttribute(SPELL_ATTR_EX2_DONT_BLOCK_MANA_REGEN)" in take
    harness = pathlib.Path(__file__).with_name("mage_resource_oracle.cpp").read_text()
    blocks = {
        "GET_SKILL": body(p, "uint16 Player::GetSkill("),
        "RANK": body(p, "uint32 Player::GetSpellRank("),
        "CRIT": body(p, "float Player::GetSpellCritFromIntellect()"),
        "UPDATE_CRIT": body(stats, "void Player::UpdateSpellCritChance("),
        "SPIRIT": body(u, "float Unit::OCTRegenMPPerSpirit()"),
        "UPDATE_REGEN": body(stats, "void Player::UpdateManaRegen()"),
        "REGEN": body(p, "void Player::Regenerate(Powers power, uint32 diff)"),
        "BASE_MISS": body(u, "float Unit::GetMissChance(SpellSchoolMask"),
        "MANA_TIMER": timer,
        "SCHEDULE": schedule,
    }
    for marker, block in blocks.items():
        assert harness.count("// PINNED_" + marker + "\n") == 1
        harness = harness.replace("// PINNED_" + marker + "\n", block)
    output = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures/mage_resource_cmangos.tsv"
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror", "-fno-fast-math", "-ffp-contract=off", "-frounding-math", "-fsanitize=undefined,float-cast-overflow", str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        data = subprocess.check_output([str(root / "oracle")])
        output.write_bytes(data)
        print(len(data.splitlines()), hashlib.sha256(data).hexdigest())


if __name__ == "__main__":
    main()
