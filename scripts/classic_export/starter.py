"""Selected native starter inputs, not player creation or a gameplay compiler.
SPDX-License-Identifier: GPL-3.0-or-later

Schema/load semantics: CMaNGOS Classic 8ec338a, ObjectMgr::LoadPlayerInfo;
actions/DBC dependencies: Player::Create/IsActionButtonDataValid/LearnDefaultSkills.
"""
from __future__ import annotations

from decimal import Decimal
from typing import Any

from .columns import STARTER_SCHEMA
from .core import KEYS, Selection, StarterSelection, check, integer

STARTER_LIMITS = {
    "playercreateinfo": 1, "playercreateinfo_item": 2048,
    "playercreateinfo_spell": 512, "playercreateinfo_action": 120,
    "playercreateinfo_skills": 512, "player_classlevelstats": 60,
    "player_levelstats": 60, "player_xp_for_level": 59,
}
ACTION_SPELL = 0
ACTION_ITEM = 128


def predicate(table: str, starter: StarterSelection) -> tuple[str, tuple[int, ...]]:
    """Fixed parameterized predicates, including the upstream zero-mask wildcard."""
    check(table in STARTER_SCHEMA, "query.starter_table")
    s = StarterSelection.parse(starter.json())
    if table == "playercreateinfo_skills":
        return ("(`raceMask`=0 OR (`raceMask` & %s)<>0) AND "
                "(`classMask`=0 OR (`classMask` & %s)<>0)",
                (1 << (s.race-1), 1 << (s.class_id-1)))
    if table == "player_classlevelstats":
        return "`class`=%s AND `level` BETWEEN %s AND %s", (s.class_id, 1, s.level_cap)
    if table == "player_xp_for_level":
        return "`lvl`>=%s AND `lvl`<%s", (1, s.level_cap)
    where, params = "`race`=%s AND `class`=%s", (s.race, s.class_id)
    if table == "player_levelstats":
        where += " AND `level` BETWEEN %s AND %s"
        params += (1, s.level_cap)
    return where, params


def keys(tables: dict[str, list[dict[str, Any]]]) -> dict[str, list[list[int]]]:
    return {table: [[row[name] for name in KEYS[table]] for row in tables[table]]
            for table in sorted(STARTER_SCHEMA)}


def references(tables: dict[str, list[dict[str, Any]]]) -> tuple[set[int], set[int]]:
    items = {r["itemid"] for r in tables.get("playercreateinfo_item", [])}
    spells = {r["Spell"] for r in tables.get("playercreateinfo_spell", [])}
    for row in tables.get("playercreateinfo_action", []):
        if row["type"] == ACTION_ITEM:
            items.add(integer(row["action"], 1, 0xffffff, "starter.action_item"))
        elif row["type"] == ACTION_SPELL:
            spells.add(integer(row["action"], 1, 0xffffff, "starter.action_spell"))
    return items, spells


def validate_rows(tables: dict[str, list[dict[str, Any]]], starter: StarterSelection) -> None:
    """Require each selected level exactly once; never upstream gap-fill or coerce."""
    s = StarterSelection.parse(starter.json())
    check(set(STARTER_SCHEMA) <= set(tables), "starter.missing_tables")
    for table in STARTER_SCHEMA:
        for row in tables[table]:
            if "race" in row:
                check(row["race"] == s.race, "starter.unselected_race")
            if "class" in row:
                check(row["class"] == s.class_id, "starter.unselected_class")
    check(len(tables["playercreateinfo"]) == 1, "starter.missing_creation")
    for table in ["player_classlevelstats", "player_levelstats"]:
        check({r["level"] for r in tables[table]} == set(range(1, s.level_cap+1)),
              f"starter.missing_or_unexpected_levels:{table}")
    check({r["lvl"] for r in tables["player_xp_for_level"]} == set(range(1, s.level_cap)),
          "starter.missing_or_unexpected_xp_levels")
    for row in tables["player_classlevelstats"]:
        integer(row["basehp"], 1, 65535, "starter.basehp")
        integer(row["basemana"], 0, 65535, "starter.basemana")
    for row in tables["player_levelstats"]:
        integer(row["str"], 1, 255, "starter.str")
        for name in ["agi", "sta", "inte", "spi"]:
            integer(row[name], 0, 255, f"starter.{name}")
    for row in tables["player_xp_for_level"]:
        integer(row["xp_for_next_level"], 1, 0xffffffff, "starter.xp")
    for row in tables["playercreateinfo_item"]:
        integer(row["amount"], 1, 255, "starter.item_amount")
    for row in tables["playercreateinfo_action"]:
        integer(row["button"], 0, 119, "starter.action_button")
        integer(row["action"], 0, 0xffffff, "starter.action_value")
        integer(row["type"], 0, 255, "starter.action_type")
    for row in tables["playercreateinfo_skills"]:
        integer(row["step"], 0, 16, "starter.skill_step")
        check((row["raceMask"] == 0 or row["raceMask"] & (1 << (s.race-1))) and
              (row["classMask"] == 0 or row["classMask"] & (1 << (s.class_id-1))),
              "starter.unselected_skill_mask")
    references(tables)  # Validate item/spell action IDs before attempting closure queries.


def blockers(tables: dict[str, list[dict[str, Any]]], selection: Selection) -> list[dict[str, str]]:
    s = selection.starter
    if s is None:
        return []
    issues = []
    selected_key = f"{s.race}:{s.class_id}"
    def add(table: str, key: str, reason: str) -> None:
        issues.append({"table": table, "key": key, "status": "blocked", "reason": reason})
    for reason in ["player.native_creation_progression_rules_required",
                   "player.chr_races_classes_map_dbc_not_captured",
                   "player.faction_reputation_dbc_not_captured",
                   "player.char_start_outfit_dbc_items_not_captured",
                   "player.start_configuration_not_captured"]:
        add("playercreateinfo", selected_key, reason)
    add("player_classlevelstats", selected_key, "player.equipment_aura_stat_modifiers_required")
    add("playercreateinfo_skills", selected_key, "player.skill_line_race_class_tiers_ability_dbc_not_captured")
    add("playercreateinfo_spell", selected_key, "player.spell_learning_passive_dependencies_not_captured")
    creation = tables["playercreateinfo"][0]
    inside = creation["map"] == selection.map_id and all(
        Decimal(low)/1000 <= Decimal(creation[name]["decimal"]) <= Decimal(high)/1000
        for name, low, high in zip(("position_x", "position_y", "position_z"), selection.minimum, selection.maximum))
    if not inside:
        add("playercreateinfo", selected_key, "player.start_outside_selection")
    learned = {r["Spell"] for r in tables["playercreateinfo_spell"]}
    for row in tables["playercreateinfo_action"]:
        key = f"{selected_key}:{row['button']}"
        if row["type"] not in {ACTION_SPELL, ACTION_ITEM}:
            add("playercreateinfo_action", key, "player.action_type_not_supported")
        elif row["type"] == ACTION_SPELL and row["action"] not in learned:
            add("playercreateinfo_action", key, "player.action_spell_learning_not_resolved")
    for row in tables["item_template"]:
        key = str(row["entry"])
        if row["RequiredSkill"] or row["RequiredReputationFaction"]:
            add("item_template", key, "item.skill_faction_dbc_not_captured")
        if row["startquest"]:
            add("item_template", key, "item.startquest_dependency_not_captured")
    return issues
