"""Captured creature-stat inputs for the existing Rust generator, never admission.

SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Creature.cpp:506-546,1265-1489;
ObjectMgr.cpp:400-576,719-798. Upstream GPL-2.0-or-later, CMaNGOS contributors.
"""
from __future__ import annotations

import struct
from typing import Any

from .dbc import binary32_bits

POLICY = "cmangos-8ec338a1-captured-creature-generation-v1"
FIELDS = {"Entry", "MinLevel", "MaxLevel", "UnitClass", "Rank", "Faction", "Scale",
          "CreatureType", "CreatureTypeFlags", "ExtraFlags", "RegenerateStats",
          "HealthMultiplier", "PowerMultiplier", "ArmorMultiplier", "DamageMultiplier",
          "DamageVariance", "StrengthMultiplier", "AgilityMultiplier", "StaminaMultiplier",
          "IntellectMultiplier", "SpiritMultiplier", "MinLevelHealth", "MaxLevelHealth",
          "MinLevelMana", "MaxLevelMana", "Armor", "MinMeleeDmg", "MaxMeleeDmg",
          "MeleeAttackPower", "RangedAttackPower", "MeleeBaseAttackTime", "DamageSchool",
          "SpeedWalk", "SpeedRun", "CorpseDecay", "EquipmentTemplateId", "UnitFlags",
          "NpcFlags", "StringId1", "StringId2"}
FIELDS.update(f"{name}{slot}" for name in ("DisplayId", "DisplayIdProbability", "StaticFlags")
              for slot in range(1, 5))
FIELDS.update(f"Resistance{name}" for name in ("Holy", "Fire", "Nature", "Frost", "Shadow", "Arcane"))
FLOATS = {"Scale", "HealthMultiplier", "PowerMultiplier", "ArmorMultiplier", "DamageMultiplier",
          "DamageVariance", "StrengthMultiplier", "AgilityMultiplier", "StaminaMultiplier",
          "IntellectMultiplier", "SpiritMultiplier", "MinMeleeDmg", "MaxMeleeDmg", "SpeedWalk", "SpeedRun"}


def scalar(value: Any) -> float:
    decimal = value["decimal"] if isinstance(value, dict) else str(value)
    return struct.unpack("<f", struct.pack("<I", binary32_bits(decimal)))[0]


def plan(creature: dict[str, Any], levels: list[dict[str, Any]],
         factions: Any, models: list[dict[str, Any]], displays: list[dict[str, Any]]) -> dict[str, Any]:
    refusals = []

    def refuse(reason: str) -> None:
        if reason not in refusals:
            refusals.append(reason)

    low, high = creature["MinLevel"], creature["MaxLevel"]
    if not 1 <= low <= high <= 20 or creature["UnitClass"] not in {1, 2, 4, 8}:
        refuse("level_or_class_domain")
    if creature["Rank"] != 0:
        refuse("non_normal_rank")
    if creature["ScriptName"]:
        refuse("script_name")
    if creature["AIName"] not in {"", "EventAI"}:
        refuse("ai_name")
    for field, mask in {"ExtraFlags": 0x10002, "NpcFlags": 0x13, "UnitFlags": 0x1300,
                        "CreatureTypeFlags": 0x40000, "StaticFlags1": 2, "StaticFlags2": 0x10,
                        "RegenerateStats": 15}.items():
        if creature[field] & ~mask:
            refuse("unsupported_flag_bits")
    if creature["ExtraFlags"] & 0x10000 and (
            not creature["StaticFlags1"] & 2 or not creature["UnitFlags"] & 0x100):
        refuse("civilian_requires_no_xp_and_player_immunity")
    if any(creature[f] for f in ("DynamicFlags", "StaticFlags3", "StaticFlags4",
                                "MechanicImmuneMask", "SchoolImmuneMask", "RacialLeader")):
        refuse("unsupported_flags")
    if creature["StaticFlags1"] & 2 and not creature["UnitFlags"] & 0x100:
        refuse("no_xp_requires_player_immunity")
    if creature["InhabitType"] not in {1, 2, 3} or creature["Civilian"] not in {0, 1}:
        refuse("inhabit_or_civilian_domain")
    if creature["Civilian"] and not creature["ExtraFlags"] & 0x10000:
        refuse("legacy_civilian_without_retained_extra_flag")
    if (creature["DamageSchool"] != 0 or creature["MeleeBaseAttackTime"] > 60000
            or not 1 <= creature["CreatureType"] <= 10):
        refuse("physical_mainhand_domain")
    floats = {field: scalar(creature[field]) for field in FLOATS}
    if any(abs(value) > 1000000 for value in floats.values()):
        refuse("numeric_domain")
    if any(floats[f] < 0 for f in ("HealthMultiplier", "PowerMultiplier", "ArmorMultiplier", "DamageMultiplier")):
        refuse("legacy_or_negative_multiplier")
    if not 0 <= floats["DamageVariance"] <= 2:
        refuse("damage_variance")
    for field in ("MinLevelHealth", "MaxLevelHealth", "MinLevelMana", "MaxLevelMana",
                  "Armor", "MeleeAttackPower", "RangedAttackPower"):
        if not 0 <= creature[field] <= 1000000:
            refuse("numeric_domain")
    if any(abs(creature[f"Resistance{name}"]) > 1000000
           for name in ("Holy", "Fire", "Nature", "Frost", "Shadow", "Arcane")):
        refuse("numeric_domain")
    selected = [r for r in levels if r["Class"] == creature["UnitClass"] and low <= r["Level"] <= high]
    keys = [(r["Class"], r["Level"]) for r in levels]
    if len(keys) != len(set(keys)):
        refuse("conflicting_classlevel_rows")
    if {r["Level"] for r in selected} != set(range(low, high + 1)):
        refuse("missing_classlevel_rows")
    for row in selected:
        if (row["BaseHealthExp0"] == 0 and row["Stamina"] == 0) or scalar(row["BaseDamageExp0"]) <= 0.1:
            refuse("classlevel_getter_domain")
        for field, value in row.items():
            if field not in {"Class", "Level"} and not 0 <= scalar(value) <= 1000000:
                refuse("classlevel_numeric_domain")
    if creature["UnitClass"] in {2, 8} and creature["RegenerateStats"] & 12 and any(
            row["BaseMana"] > min(row["Intellect"], 20) + max(row["Intellect"] - 20, 0) * 15
            for row in selected):
        refuse("active_power_regeneration")
    faction = factions.by_id.get(creature["Faction"]) if factions else None
    if faction is None:
        refuse("missing_faction_template")
    elif faction.words[2] & ~0x1800 or any(word & ~0xf for word in faction.words[3:6]):
        refuse("faction_template_domain")
    model_by_id = {r["modelid"]: r for r in models}
    display_by_id = {r["id"]: r for r in displays}
    choices = [{"id": creature[f"DisplayId{i}"], "probability": creature[f"DisplayIdProbability{i}"]}
               for i in range(1, 5)]
    if not any(c["id"] and c["probability"] for c in choices):
        refuse("display_probability_domain")
    if sum(c["probability"] for c in choices if c["id"]) > 0x7fffffff:
        refuse("display_probability_domain")
    for choice in choices:
        if choice["id"] and choice["id"] not in model_by_id:
            refuse("missing_model")
    for model in models:
        if model["modelid_alternative"] or model["gender"] not in {0, 1, 2}:
            refuse("model_substitution_domain")
        if model["modelid_other_gender"] == model["modelid"]:
            refuse("redundant_gender_model")
        if model["modelid_other_gender"] and model["modelid_other_gender"] not in model_by_id:
            refuse("missing_gender_model")
        numeric = {name: scalar(model[name]) for name in
                   ("bounding_radius", "combat_reach", "SpeedWalk", "SpeedRun")}
        if any(not 0 <= value <= 1000000 for value in numeric.values()) or numeric["bounding_radius"] <= 0:
            refuse("model_numeric_domain")
        walk = floats["SpeedWalk"] or numeric["SpeedWalk"]
        run = floats["SpeedRun"] or numeric["SpeedRun"]
        if not 0 < walk <= 400000 or not 0 < run <= 1000000 / 7:
            refuse("model_speed_domain")
        display = display_by_id.get(model["modelid"])
        if display is None or not 0 < struct.unpack("<f", struct.pack("<I", display["scale_binary32"]))[0] <= 1000000:
            refuse("missing_or_invalid_display")
    return {"policy": POLICY, "status": "refused" if refusals else "complete_inputs",
            "refusals": sorted(refusals), "entry": creature["Entry"],
            "level_roll": {"minimum": low, "maximum": high, "inclusive": True,
                           "draw_required": low != high, "rng_owner": "existing_world_counter"},
            "template_numeric_binary32": {field: binary32_bits(str(value)) for field, value in sorted(floats.items())},
            "display_choices": choices, "class_level_rows": selected,
            "faction_template_words": list(faction.words) if faction else None,
            "runtime_consumer": "CreatureCapturedInput::compile -> creature_init::prepare",
            "resolved_stats": None, "world_admission": False}
