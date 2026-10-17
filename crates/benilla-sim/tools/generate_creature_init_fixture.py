#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Hash-check pinned source, compile unchanged methods, emit synthetic NPC cases."""
import argparse
import hashlib
import pathlib
import re
import subprocess
import tempfile
HASHES = {
    "Unit.h": "2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1",
    "Creature.cpp": "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
    "StatSystem.cpp": "91aa0eb47c31aea9d97b64a97c1bdc350b4ea7e7fd30f1ce885a40b69112f1bb",
    "Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "Object.cpp": "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b",
    "Creature.h": "a92b0e95e5bf7827389fcca0acc0edaba48a1a083efbf5a01a752a4d171664fc",
    "ObjectMgr.cpp": "dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b",
}
def body(text, signature):
    start = text.index(signature)
    end = text.index("{", start) + 1
    depth = 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end] + "\n"
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("upstream", type=pathlib.Path)
    parser.add_argument("--check", action="store_true", help="compare without overwriting the fixture")
    args = parser.parse_args()
    sources = {}
    for name, digest in HASHES.items():
        raw = (args.upstream / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != digest:
            raise SystemExit("Wrong pinned source: " + name)
        sources[name] = raw.decode()
    methods = "".join(body(sources[file], signature) for file, signature in [
        ("Creature.cpp", "CreatureDataAddon const* Creature::GetCreatureAddon() const"),
        ("Creature.cpp", "bool Creature::LoadCreatureAddon(bool reload)"),
        ("Object.cpp", "void Object::SetByteValue(uint16 index, uint8 offset, uint8 value)"),
        ("Creature.cpp", "void Creature::SelectLevel("),
        ("Creature.cpp", "void Creature::SetVirtualItem("),
        ("Creature.cpp", "bool Creature::hasWeapon("),
        ("Creature.cpp", "void Creature::LoadEquipment("),
        ("StatSystem.cpp", "float Creature::GetConditionalTotalPhysicalDamageModifier("),
        ("Creature.cpp", "void Creature::RegenerateAll("),
        ("Creature.cpp", "void Creature::RegenerateHealth("),
        ("Creature.cpp", "void Creature::RegeneratePower("),
        ("Creature.cpp", "uint32 Creature::ChooseDisplayId("),
        ("Creature.cpp", "void Creature::SetBaseWalkSpeed("),
        ("Creature.cpp", "void Creature::SetBaseRunSpeed("),
        ("ObjectMgr.cpp", "CreatureModelInfo const* ObjectMgr::GetCreatureModelRandomGender("),
        ("StatSystem.cpp", "bool Creature::UpdateStats("),
        ("StatSystem.cpp", "bool Creature::UpdateAllStats("),
        ("StatSystem.cpp", "void Creature::UpdateArmor("),
        ("StatSystem.cpp", "void Creature::UpdateResistances("),
        ("StatSystem.cpp", "void Creature::UpdateAttackPowerAndDamage("),
        ("StatSystem.cpp", "void Creature::UpdateDamagePhysical("),
        ("StatSystem.cpp", "void Creature::UpdateMaxHealth("),
        ("StatSystem.cpp", "float Creature::GetHealthBonusFromStamina() const"),
        ("StatSystem.cpp", "float Unit::GetHealthBonusFromStamina(float stamina)"),
        ("StatSystem.cpp", "float Unit::GetHealthBonusFromStamina() const"),
        ("StatSystem.cpp", "float Unit::GetManaBonusFromIntellect(float intellect)"),
        ("StatSystem.cpp", "float Unit::GetManaBonusFromIntellect() const"),
        ("StatSystem.cpp", "void Unit::UpdateMaxPower("),
        ("Unit.cpp", "float Unit::GetSpeed(UnitMoveType mtype) const"),
        ("Unit.cpp", "void Unit::UpdateSpeed("),
        ("Object.cpp", "bool WorldObject::IsUsingNewSpawningSystem() const"),
        ("Unit.cpp", "float Unit::GetTotalStatValue("),
        ("Unit.cpp", "float Unit::GetTotalResistanceValue("),
    ])
    loader = body(sources["ObjectMgr.cpp"], "void ObjectMgr::LoadCreatureClassLvlStats()")
    lines = re.findall(r"^        cCLS\.(?:BaseMana|BaseRangedAttackPower|BaseArmor) -= [^\n]+$", loader, re.M)
    assert len(lines) == 3
    harness = pathlib.Path(__file__).with_name("creature_init_oracle.cpp").read_text()
    harness = harness.replace("// PINNED_METHODS\n", methods).replace("// PINNED_LOADER\n", "\n".join(lines) + "\n")
    speeds = sources["Unit.cpp"]
    start = speeds.index("float baseMoveSpeed[MAX_MOVE_TYPE]")
    end = speeds.index("};", start) + 2
    harness = harness.replace("// PINNED_BASE_SPEEDS\n", speeds[start:end] + "\n")
    # Preserve the numeric prefix; the remaining method only propagates updates.
    speed_setter = body(speeds, "void Unit::SetSpeedRate(")
    speed_prefix = speed_setter[:speed_setter.index("        propagateSpeedChange();")]
    harness = harness.replace("// PINNED_SPEED_RATE\n", speed_prefix + "    }\n}\n")
    harness = harness.replace("// PINNED_RESPAWN_DRAW\n", body(sources["Creature.h"], "uint32 GetRandomRespawnTime() const"))
    creature = sources["Creature.cpp"]
    start = creature.index("    m_respawnDelay = data->GetRandomRespawnTime();")
    end = creature.index("    m_deathState = ALIVE;", start)
    harness = harness.replace("// PINNED_TIMING\n", creature[start:end])
    harness = harness.replace("// PINNED_WEAPON_USABILITY\n", body(sources["Unit.h"], "bool CanUseEquippedWeapon("))
    start = creature.index("    // Load creature equipment\n")
    end = creature.index("    SetName(normalInfo->Name);", start)
    harness = harness.replace("// PINNED_EQUIPMENT_SELECTION\n", creature[start:end])
    header = sources["Creature.h"]
    start = header.index("enum VirtualItemSlot")
    end = header.index("struct CreatureCreatePos", start)
    harness = harness.replace("// PINNED_VIRTUAL_ITEM_TYPES\n", header[start:end])
    # All addon declarations, byte mutation and fresh ordinary-NPC defaults come
    # from the same hash-pinned inputs as the pre-existing initializer oracle.
    addon_types = body(header, "struct CreatureDataAddon").rstrip() + ";\n"
    for signature in ("enum UnitBytes1Offsets", "enum UnitBytes2Offsets", "enum SheathState", "enum UnitBytes2_Flags"):
        addon_types += body(sources["Unit.h"], signature).rstrip() + ";\n"
    harness = harness.replace("// PINNED_ADDON_TYPES\n", addon_types)
    harness = harness.replace("// PINNED_SHEATH_SETTER\n", body(sources["Unit.h"], "virtual void SetSheath("))
    update_entry = body(creature, "bool Creature::UpdateEntry(")
    start = update_entry.index("    // creatures always have melee weapon ready if any\n")
    end = update_entry.index("    if (preserveHPAndPower)", start)
    harness = harness.replace("// PINNED_FRESH_ADDON_BYTES\n", update_entry[start:end])
    if "// PINNED_" in harness:
        raise SystemExit("Unexpanded pinned source marker")
    with tempfile.TemporaryDirectory() as temp:
        root = pathlib.Path(temp)
        (root / "oracle.cpp").write_text(harness)
        subprocess.run(["g++", "-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter", "-Wno-unused-variable", "-Wno-unused-but-set-variable", "-fno-fast-math", "-ffp-contract=off", "-frounding-math", "-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all", str(root / "oracle.cpp"), "-o", str(root / "oracle")], check=True)
        data = subprocess.check_output([str(root / "oracle")])
    target = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures/creature_init_cmangos.tsv"
    # Existing vectors are a byte-for-byte compatibility boundary, independent of
    # the checked-in target; never silently refresh them while adding addon rows.
    rows = data.splitlines(keepends=True)
    if hashlib.sha256(b"".join(rows[:4278])).hexdigest() != "b7b6afac801d43b3ff42d9e7db11728bb918d295df8976c21ca0fe16503a30c5":
        raise SystemExit("Pre-addon vectors changed")
    if len(rows) != 4726 or any(not row.startswith(b"a ") for row in rows[4278:]):
        raise SystemExit("Wrong addon vector coverage")
    if args.check:
        if target.read_bytes() != data:
            raise SystemExit("Creature-init fixture differs from source oracle")
    else:
        target.write_bytes(data)
    print(len(data.splitlines()), hashlib.sha256(data).hexdigest())
if __name__ == "__main__":
    main()
