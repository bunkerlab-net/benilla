"""Exact-ID preparation compiler; outputs typed source inputs, never runnable rules.

SPDX-License-Identifier: GPL-3.0-or-later
See PROVENANCE.md for the bounded Player::Create outfit quantity translation.
"""
from __future__ import annotations

from dataclasses import asdict, dataclass
from typing import Any

from classic_export.capture import validate_bundle
from classic_export import event_ai, native_spawn, spell_registry, initial_hooks, quest_relationships
from classic_export.columns import COLUMNS, STARTER_SCHEMA, LEARNING_SCHEMA
from classic_export.core import (CORE_REVISION, STARTER_FORMAT, KEYS, Selection,
                                 canonical, check, digest, integer)
from .learning import discover
from .talent import inputs as talent_inputs
from .dbc import DbcRow, DbcTable, LAYOUTS, binary32_bits, finite_float, signed

PROFILE = "cmangos-8ec338a1-native-starter-bindings-v1"
FORMAT = "benilla-native-starter-preparation-v1"
MAX_REPORT_BYTES = 128 * 1024 * 1024
MAX_FIELDS = 250_000
MAX_BINDING_BYTES = 32 * 1024 * 1024

@dataclass(frozen=True)
class Request:
    race: int
    class_id: int
    gender: int

    @classmethod
    def parse(cls, value: Any) -> Request:
        check(type(value) is dict and set(value) == {"race", "class", "gender"}, "request.fields")
        race = integer(value["race"], 1, 8, "request.race")
        class_id = integer(value["class"], 1, 11, "request.class")
        check(class_id in {1, 2, 3, 4, 5, 7, 8, 9, 11}, "request.class")
        gender = integer(value["gender"], 0, 1, "request.gender")
        return cls(race, class_id, gender)

    def json(self) -> dict[str, int]:
        return {"race": self.race, "class": self.class_id, "gender": self.gender}

@dataclass(frozen=True)
class LevelInputs:
    level: int
    create_health: int
    create_mana: int
    strength: int
    agility: int
    stamina: int
    intellect: int
    spirit: int
    xp_for_next_level: int | None

@dataclass(frozen=True)
class OutfitGrant:
    source: str
    source_slot: int
    item_id: int
    count: int | None


def outfit_quantity(item: dict[str, Any]) -> int:
    """Player::Create's item quantity only, not CanEquip/StoreNewItemInBestSlots.

    No quantity defaults. Native food category 11 and drink category 59 use the
    source's strict Stackable > 4 / > 2 branches; all other items use BuyCount.
    """
    count = integer(item["BuyCount"], 1, 0x7fffffff, "item.outfit_buy_count")
    if item["class"] == 0 and item["subclass"] == 5:
        if item["spellcategory_1"] == 11 and item["stackable"] > 4:
            count = 4
        elif item["spellcategory_1"] == 59 and item["stackable"] > 2:
            count = 2
    return count

# Source SQL to actual client Spell.dbc fields. Server overrides remain visible
# differences, never silently replaced by client values. All 3 effect slots stay.
SPELL_FIELDS = {
    "Id": 0, "School": 1, "Category": 2, "Dispel": 4, "Mechanic": 5,
    **{name: i for i, name in enumerate(("Attributes", "AttributesEx", "AttributesEx2", "AttributesEx3", "AttributesEx4", "Stances", "StancesNot", "Targets", "TargetCreatureType", "RequiresSpellFocus", "CasterAuraState", "TargetAuraState", "CastingTimeIndex", "RecoveryTime", "CategoryRecoveryTime", "InterruptFlags", "AuraInterruptFlags", "ChannelInterruptFlags", "procFlags", "procChance", "procCharges", "maxLevel", "baseLevel", "spellLevel", "DurationIndex", "powerType", "manaCost", "manaCostPerlevel", "manaPerSecond", "manaPerSecondPerLevel", "rangeIndex", "speed"), 6)},
    "StackAmount": 39, "EquippedItemClass": 58, "EquippedItemSubClassMask": 59,
    "EquippedItemInventoryTypeMask": 60, "ManaCostPercentage": 156,
    "StartRecoveryCategory": 157, "StartRecoveryTime": 158,
    "SpellFamilyName": 160, "DmgClass": 164, "PreventionType": 165,
}
for _name, _start in (("Effect", 61), ("EffectDieSides", 64), ("EffectBaseDice", 67),
                      ("EffectDicePerLevel", 70), ("EffectRealPointsPerLevel", 73),
                      ("EffectBasePoints", 76), ("EffectMechanic", 79),
                      ("EffectImplicitTargetA", 82), ("EffectImplicitTargetB", 85),
                      ("EffectRadiusIndex", 88), ("EffectApplyAuraName", 91),
                      ("EffectAmplitude", 94), ("EffectMultipleValue", 97),
                      ("EffectChainTarget", 100), ("EffectItemType", 103),
                      ("EffectMiscValue", 106), ("EffectTriggerSpell", 109),
                      ("EffectPointsPerComboPoint", 112)):
    SPELL_FIELDS.update({f"{_name}{i+1}": _start+i for i in range(3)})

FLOAT_SPELL_FIELDS = {"speed"} | {f"{name}{i}" for name in
    ("EffectDicePerLevel", "EffectRealPointsPerLevel", "EffectMultipleValue", "EffectPointsPerComboPoint")
    for i in range(1, 4)}

# A preparation disposition is not a gameplay support assertion. Every source
# field appears exactly once. No zero-valued flag is automatically declared inert.
CONSUMED = {
    **{table: set(COLUMNS[table]) for table in LEARNING_SCHEMA},
    **{table: set(COLUMNS[table]) for table in initial_hooks.TABLES},
    "creature_ai_scripts": {"id", "creature_id"},
    "spell_scripts": {"Id", "ScriptName"},
    "playercreateinfo": set(COLUMNS["playercreateinfo"]),
    "player_classlevelstats": set(COLUMNS["player_classlevelstats"]),
    "player_levelstats": set(COLUMNS["player_levelstats"]),
    "player_xp_for_level": set(COLUMNS["player_xp_for_level"]),
    "playercreateinfo_item": set(COLUMNS["playercreateinfo_item"]),
    "playercreateinfo_spell": {"race", "class", "Spell"},
    "playercreateinfo_action": set(COLUMNS["playercreateinfo_action"]),
    "playercreateinfo_skills": {"raceMask", "classMask", "skill", "step"},
}
BLOCKED_TABLE_REASON = {
    **{table: "creature.native_spawn_dependency_execution_required" for table in native_spawn.TABLES},
    "creature": "spawn.overrides_pools_events_not_captured",
    "creature_template": "creature.native_stats_combat_faction_required",
    "creature_model_info": "creature.model_selection_movement_reach_rules_required",
    "creature_template_classlevelstats": "creature.native_stats_combat_faction_required",
    "quest_template": "quest.native_eligibility_credit_reward_required",
    "creature_questrelation": "quest.native_eligibility_credit_reward_required",
    "creature_involvedrelation": "quest.native_eligibility_credit_reward_required",
    "item_template": "item.native_inventory_equipment_rules_required",
    "spell_template": "spell.native_effect_target_scaling_rules_required",
    "creature_loot_template": "loot.native_chance_condition_rng_required",
    "reference_loot_template": "loot.native_chance_condition_rng_required",
}

# SpellDefines.h / SpellMgr::IsPassiveSpell at the pinned core revision.
SPELL_ATTR_PASSIVE = 0x00000040
SPELL_ATTR_DO_NOT_DISPLAY = 0x00000080
SPELL_ATTR_EX_CAST_WHEN_LEARNED = 0x80000000


def classify_spell_admission(tables: dict[str, Any], spells: list[dict[str, Any]],
                             issues: list[dict[str, Any]],
                             learning_dependencies: dict[str, Any] | None = None) -> dict[str, Any]:
    """Unavailable active abilities are not, by themselves, an entry veto.

    This classification never proves effective learning or grants a spell. Any
    initial passive/item/quest dependency wins over an active-only dependency.
    Global unresolved learning/stat/equipment requirements remain entry blockers.
    """
    source = {r["Id"]: r for r in tables["spell_template"]}
    bound = {r["id"]: r for r in spells}
    initial = {r["Spell"] for r in tables["playercreateinfo_spell"]}
    passive = {i for i in initial if source[i]["Attributes"] & SPELL_ATTR_PASSIVE}
    learning = {i for i in initial if source[i]["Attributes"] & SPELL_ATTR_DO_NOT_DISPLAY
                or source[i]["AttributesEx"] & SPELL_ATTR_EX_CAST_WHEN_LEARNED
                or any(source[i][f"Effect{slot}"] in {36, 44, 118} for slot in range(1, 4))}
    learning.update(r["entry"] for r in tables.get("spell_learn_spell", []) if r["entry"] in initial)
    role_fields = {"Attributes", "AttributesEx", "Effect1", "Effect2", "Effect3"}
    active_candidates = {i for i in initial-passive-learning if bound[i]["dbc_only_inputs"] is not None
                         and not role_fields.intersection(bound[i]["server_client_differences"])}
    active = set(active_candidates)
    critical = passive | learning
    if learning_dependencies is not None:
        for edge in learning_dependencies["edges"]:
            if edge["to_kind"] == "spell" and edge["role"] in {"ability_spell", "learn_spell", "prev_spell", "first_spell", "req_spell"}:
                if edge["to_id"] not in active_candidates:
                    critical.add(edge["to_id"])
    for row in tables["item_template"]:
        critical.update(row[f"spellid_{i}"] for i in range(1, 6) if row[f"spellid_{i}"] > 0)
        if row["requiredspell"]:
            critical.add(row["requiredspell"])
    for row in tables["quest_template"]:
        critical.update(row[name] for name in ("SrcSpell", "RewSpell", "RewSpellCast") if row[name])

    def closure(roots: set[int]) -> set[int]:
        result = set(roots)
        pending = list(roots)
        learned = {}
        for link in tables.get("spell_learn_spell", []):
            learned.setdefault(link["entry"], set()).add(link["SpellID"])
        for link in tables.get("spell_chain", []):
            learned.setdefault(link["spell_id"], set()).update(
                link[field] for field in ("prev_spell", "first_spell", "req_spell") if link[field])
        while pending:
            parent = pending.pop()
            row = source.get(parent)
            for entry in learned.get(parent, set()):
                if entry not in result:
                    result.add(entry)
                    pending.append(entry)
            if row:
                for slot in range(1, 4):
                    entry = row[f"EffectTriggerSpell{slot}"]
                    if entry and entry not in result:
                        result.add(entry)
                        pending.append(entry)
        return result

    critical = closure(critical)
    active -= critical
    closures = {i: closure({i}) for i in sorted(active)}
    active_only = set().union(*closures.values()) - critical if closures else set()
    owners: dict[tuple[str, int], set[int]] = {}
    def own(table: str, key: int, spell: int) -> None:
        owners.setdefault((table, key), set()).add(spell)
    for entry, row in source.items():
        for table in ("spell_template", "spell_scripts", "Spell"):
            own(table, entry, entry)
        for table, field in (("SpellCastTimes", "CastingTimeIndex"), ("SpellRange", "rangeIndex"),
                             ("SpellCategory", "Category"), ("SpellDuration", "DurationIndex"),
                             ("SpellFocusObject", "RequiresSpellFocus")):
            if row[field] or table in {"SpellCastTimes", "SpellRange"}:
                own(table, row[field], entry)
        for slot in range(1, 4):
            if row[f"EffectRadiusIndex{slot}"]:
                own("SpellRadius", row[f"EffectRadiusIndex{slot}"], entry)
        dbc = bound[entry]["dbc_only_inputs"]
        if dbc:
            for item in set(dbc["totems"]+dbc["reagents"])-{0}:
                if item > 0:
                    own("item_template", item, entry)
    unavailable = {i: {"id": i, "status": "unavailable", "presentation": "visible_unavailable",
                       "cast_admission": "reject", "trigger_dependency_ids": sorted(closures[i]-{i}),
                       "reason_ids": set()} for i in active}
    for issue in issues:
        key = issue["key"]
        key = key[0] if isinstance(key, list) and len(key) == 1 else key
        if isinstance(key, str) and key.isdecimal():
            key = int(key)
        related = owners.get((issue["table"], key), set()) if type(key) is int else set()
        if key == [] and issue["reason"].startswith("dbc."):
            related = set().union(*(ids for (table, _), ids in owners.items() if table == issue["table"]))
        # A skill-learning missing-row finding is entry-critical even when that
        # ID also appears under an unavailable active spell's trigger closure.
        can_scope = issue["reason"].startswith(("spell.", "dbc."))
        if issue["table"] == "item_template":
            can_scope = issue["reason"] == "spell.dbc_reagent_or_totem_not_captured"
        if can_scope and related and related <= active_only:
            roots = sorted(i for i, deps in closures.items() if deps & related)
            issue["admission_scope"] = {"kind": "unavailable_active_ability", "spell_ids": roots}
            for i in roots:
                unavailable[i]["reason_ids"].add(issue["reason"])
        else:
            issue["admission_scope"] = {"kind": "selection_entry"}
    for ability in unavailable.values():
        ability["reason_ids"] = sorted(ability["reason_ids"] | {"spell.native_effect_target_scaling_rules_required"})
    return {"available_abilities": [],
            "unavailable_active_abilities": [unavailable[i] for i in sorted(unavailable)],
            "entry_critical_spell_ids": sorted(critical),
            "learning_sensitive_initial_spell_ids": sorted(learning),
            "unresolved_initial_spell_roles": sorted(initial-passive-learning-active_candidates),
            "effective_learning_proven": False}

class Compiler:
    def __init__(self, source: dict[str, Any], dbcs: dict[str, DbcTable], request: Request):
        self.source, self.dbcs, self.request = source, dbcs, request
        self.tables = source["tables"]
        self.issues: list[dict[str, Any]] = []
        self.dependencies: set[tuple[str, int, str]] = set()
        self.selected_dbc: dict[tuple[str, tuple[int, ...]], DbcRow] = {}
        self.used_dbc: dict[tuple[str, tuple[int, ...]], set[int]] = {}
        self.binding_bytes = 0

    def issue(self, table: str, key: Any, reason: str, **context: Any) -> None:
        self.issues.append({"table": table, "key": key, "reason": reason, **context})

    def need(self, table: str, entry: int, reason: str) -> None:
        self.dependencies.add((table, entry, reason))
        self.issue(table, [entry], reason)

    def use(self, row: DbcRow, columns: set[int]) -> DbcRow:
        key = row.table, row.key
        if key not in self.selected_dbc:
            self.binding_bytes += len(canonical(row.raw()))
            check(self.binding_bytes <= MAX_BINDING_BYTES, "compiler.dbc_binding_byte_limit")
        self.selected_dbc[key] = row
        self.used_dbc.setdefault(key, set()).update(columns)
        return row

    def lookup(self, table: str, entry: int, columns: set[int]) -> DbcRow | None:
        dbc = self.dbcs.get(table)
        row = dbc.by_id.get(entry) if dbc else None
        if row is None:
            self.need(table, entry, "dbc.required_record_missing")
        else:
            self.use(row, columns)
        return row

    def identity(self) -> dict[str, Any]:
        s = self.request
        creation = self.tables["playercreateinfo"][0]
        race = self.lookup("ChrRaces", s.race, {0, 1, 2, 4, 5, 8, 14, 15, 16, *range(17, 25)})
        cls = self.lookup("ChrClasses", s.class_id, {0, 3, 14, 15, *range(5, 13)})
        map_row = self.lookup("Map", creation["map"], {0, 1, 2, 19})
        base = self.dbcs.get("CharBaseInfo")
        pairs = [r for r in base.rows if r.key == (s.race, s.class_id)] if base else []
        if pairs:
            self.use(pairs[0], {0, 1})
        else:
            self.issue("CharBaseInfo", [s.race, s.class_id], "player.race_class_pair_missing")
        if race and race.words[1] & 1:
            self.issue("ChrRaces", [s.race], "player.race_not_playable")
        if cls and cls.words[3] not in {0, 1, 3}:
            self.issue("ChrClasses", [s.class_id], "player.power_type_unsupported")
        if map_row and map_row.words[2] != 0:
            self.issue("Map", [creation["map"]], "player.instance_start_unsupported")
        return {"selection": s.json(), "start": creation,
                "display_id": race.words[4+s.gender] if race else None,
                "faction_template_id": race.words[2] if race else None,
                "power_type": cls.words[3] if cls else None,
                "spell_family": cls.words[15] if cls else None,
                "class_file": cls.text[14] if cls else None,
                "race_file": race.text[15] if race else None,
                "map_directory": map_row.text[1] if map_row else None,
                "race_class_pair_found": bool(pairs)}

    def progression(self) -> list[dict[str, Any]]:
        levels = {r["level"]: r for r in self.tables["player_levelstats"]}
        xp = {r["lvl"]: r["xp_for_next_level"] for r in self.tables["player_xp_for_level"]}
        result = []
        for row in self.tables["player_classlevelstats"]:
            stats = levels[row["level"]]
            result.append(asdict(LevelInputs(row["level"], row["basehp"], row["basemana"],
                stats["str"], stats["agi"], stats["sta"], stats["inte"], stats["spi"], xp.get(row["level"]))))
        return result

    def outfit(self) -> dict[str, Any]:
        s = self.request
        packed = s.race | s.class_id << 8 | s.gender << 16
        table = self.dbcs.get("CharStartOutfit")
        # Compare all four packed bytes, as Player::Create does; do not discard
        # the high outfit byte or resolve duplicate variants by arbitrary order.
        rows = [r for r in table.rows if r.words[0] != 0 and r.words[1] == packed] if table else []
        grants = []
        items = {r["entry"]: r for r in self.tables["item_template"]}
        if len(rows) != 1:
            self.issue("CharStartOutfit", [s.race, s.class_id, s.gender],
                       "player.outfit_missing" if not rows else "player.outfit_ambiguous")
        for row in rows:
            self.use(row, set(range(14)))
        if len(rows) == 1:
            for slot, word in enumerate(rows[0].words[2:14]):
                entry = signed(word)
                if entry <= 0:
                    continue
                item = items.get(entry)
                count = None
                if item is None:
                    self.need("item_template", entry, "player.outfit_item_not_captured")
                else:
                    try:
                        count = outfit_quantity(item)
                    except ValueError:
                        self.issue("item_template", [entry], "item.outfit_quantity_outside_domain")
                grants.append(asdict(OutfitGrant("CharStartOutfit", slot, entry, count)))
        for slot, item in enumerate(self.tables["playercreateinfo_item"]):
            grants.append(asdict(OutfitGrant("playercreateinfo_item", slot, item["itemid"], item["amount"])))
        self.issue("playercreateinfo_item", [s.race, s.class_id], "player.native_equipment_placement_required")
        return {"outfit_record_id": rows[0].words[0] if len(rows) == 1 else None,
                "grants_in_source_order": grants, "equipment_slots": None}

    def factions(self, identity: dict[str, Any]) -> None:
        ids = {r["Faction"] for r in self.tables["creature_template"]}
        if identity["faction_template_id"] is not None:
            ids.add(identity["faction_template_id"])
        for entry in sorted(ids):
            row = self.lookup("FactionTemplate", entry, set(range(14)))
            if row:
                faction_ids = {row.words[1], *row.words[6:14]} - {0}
                for faction in sorted(faction_ids):
                    self.lookup("Faction", faction, {0, 1})
        # All factions can seed initial reputation, not just encountered templates.
        table = self.dbcs.get("Faction")
        if table:
            for row in table.rows:
                if signed(row.words[1]) >= 0:
                    self.use(row, {0, 1})
        self.issue("Faction", [], "player.native_reputation_initialization_required")

    def skills(self) -> dict[str, Any]:
        self.learning = discover(self)
        self.issue("SkillRaceClassInfo", [], "player.native_skill_obtainability_and_learning_required")
        return {"sql_candidates": self.tables["playercreateinfo_skills"],
                "race_class_candidates": self.learning["race_class_candidates"],
                "ability_candidates": self.learning["ability_candidates"],
                "dependency_closure": self.learning, "effective_skills": None}

    def spells(self) -> list[dict[str, Any]]:
        result = []
        item_ids = {r["entry"] for r in self.tables["item_template"]}
        initial = {r["Spell"] for r in self.tables["playercreateinfo_spell"]}
        for source in self.tables["spell_template"]:
            entry = source["Id"]
            row = self.lookup("Spell", entry, set(SPELL_FIELDS.values()) | set(range(40, 58)) |
                              {159, 161, 162, 163, 167, 168, 169})
            differences = []
            server_words = list(row.words) if row else None
            if row:
                for name, column in SPELL_FIELDS.items():
                    value = source[name]
                    if name in FLOAT_SPELL_FIELDS:
                        check(type(value) is dict and set(value) == {"decimal"}, "spell.source_float_domain")
                        bits = binary32_bits(value["decimal"])
                        finite_float(row.words[column], "spell.dbc_nonfinite")
                        # SQL canonical decimal zero has no sign; both zeros compare equal.
                        matches = bits == row.words[column] or (bits & 0x7fffffff == 0 and row.words[column] & 0x7fffffff == 0)
                    else:
                        integer(value, -0x80000000, 0xffffffff, "spell.source_integer_domain")
                        matches = value & 0xffffffff == row.words[column]
                    server_words[column] = bits if name in FLOAT_SPELL_FIELDS else value & 0xffffffff
                    if not matches:
                        differences.append(name)
                        self.issue("spell_template", [entry], "spell.server_client_field_mismatch", field=name)
            cast = self.lookup("SpellCastTimes", source["CastingTimeIndex"], {0, 1, 2, 3})
            range_row = self.lookup("SpellRange", source["rangeIndex"], {0, 1, 2, 3})
            if source["Category"]:
                self.lookup("SpellCategory", source["Category"], {0, 1})
            duration = (self.lookup("SpellDuration", source["DurationIndex"], {0, 1, 2, 3})
                        if source["DurationIndex"] or
                        (self.dbcs.get("SpellDuration") and 0 in self.dbcs["SpellDuration"].by_id)
                        else None)
            if source["RequiresSpellFocus"]:
                self.lookup("SpellFocusObject", source["RequiresSpellFocus"], {0})
            for slot in range(1, 4):
                if source[f"EffectRadiusIndex{slot}"]:
                    self.lookup("SpellRadius", source[f"EffectRadiusIndex{slot}"], {0, 1, 2, 3})
                # EffectItemType is also a spell-family mask for modifier auras.
                # An unknown effect must never turn that mask into an item ID.
                if source[f"EffectItemType{slot}"]:
                    self.issue("spell_template", [entry], "spell.effect_item_type_semantics_required",
                               slot=slot, field_value=source[f"EffectItemType{slot}"])
            if row:
                for entry_id in {*row.words[40:42], *[signed(w) for w in row.words[42:50]]}:
                    if entry_id > 0 and entry_id not in item_ids:
                        self.need("item_template", entry_id, "spell.dbc_reagent_or_totem_not_captured")
            range_input = None
            if range_row:
                low, high = range_row.words[1:3]
                finite_float(low, "spell.range_nonfinite")
                finite_float(high, "spell.range_nonfinite")
                compare_low = 0 if low & 0x7fffffff == 0 else low
                compare_high = 0 if high & 0x7fffffff == 0 else high
                if compare_low & 0x80000000 or compare_high & 0x80000000 or compare_low > compare_high:
                    self.issue("SpellRange", [source["rangeIndex"]], "spell.range_domain")
                range_input = {"id": source["rangeIndex"], "minimum_binary32": low,
                               "maximum_binary32": high, "flags": range_row.words[3]}
            effects = [{name: source[f"{name}{slot}"] for name in
                        ("Effect", "EffectDieSides", "EffectBaseDice", "EffectBasePoints",
                         "EffectImplicitTargetA", "EffectImplicitTargetB", "EffectApplyAuraName",
                         "EffectAmplitude", "EffectMiscValue", "EffectItemType", "EffectTriggerSpell", "EffectBonusCoefficient",
                         "EffectBonusCoefficientFromAP")} for slot in range(1, 4)]
            # These labels choose the next port, not a support allowlist. Every
            # effect, attribute word and target selector remains runtime-blocked.
            for slot, effect in enumerate(effects, 1):
                if effect["Effect"]:
                    branch = {2: "school_damage", 6: "apply_aura", 10: "heal"}.get(effect["Effect"], "other")
                    self.issue("spell_template", [entry], "spell.effect_execution_required",
                               slot=slot, effect_id=effect["Effect"], branch=branch)
                if effect["EffectImplicitTargetA"] or effect["EffectImplicitTargetB"]:
                    self.issue("spell_template", [entry], "spell.target_selector_execution_required",
                               slot=slot, target_a=effect["EffectImplicitTargetA"],
                               target_b=effect["EffectImplicitTargetB"])
            for name in ("Attributes", "AttributesEx", "AttributesEx2", "AttributesEx3", "AttributesEx4", "AttributesServerside"):
                if source[name]:
                    self.issue("spell_template", [entry], "spell.attribute_bits_execution_required",
                               field=name, bits=source[name])
            if source["speed"]["decimal"] != "0":
                self.issue("spell_template", [entry], "spell.projectile_timing_execution_required")
            registry_scope = spell_registry.scope(self.source, entry)
            if registry_scope is None:
                self.issue("spell_template", [entry], "spell.registry_coverage_not_proven")
            elif registry_scope["rows"]:
                self.issue("spell_scripts", [entry], "spell.registry_row_requires_named_handler_semantics")
            result.append({"id": entry, "initial_sql_spell": entry in initial,
                "script_registry": registry_scope,
                "initial_hooks": initial_hooks.scope(self.source, entry),
                "name_locales": [row.text[i] for i in range(120, 128)] if row else None,
                "cast_time": {"id": source["CastingTimeIndex"], "base_ms": signed(cast.words[1]),
                              "per_level_ms": signed(cast.words[2]), "minimum_ms": signed(cast.words[3])} if cast else None,
                "range": range_input, "effects": effects, "speed": source["speed"],
                "duration": {"id": source["DurationIndex"], "base_ms": signed(duration.words[1]),
                             "per_level_ms": signed(duration.words[2]), "maximum_ms": signed(duration.words[3])}
                            if duration else None,
                "server_client_differences": differences,
                "source_row": source, "server_numeric_words": server_words,
                "dbc_only_inputs": {"totems": list(row.words[40:42]),
                    "reagents": [signed(w) for w in row.words[42:50]], "reagent_counts": list(row.words[50:58]),
                    "max_target_level": row.words[159], "family_mask_words": list(row.words[161:163]),
                    "max_affected_targets": row.words[163], "damage_multiplier_binary32": list(row.words[167:170])} if row else None})
            self.issue("spell_template", [entry], "spell.server_overlay_and_learning_closure_required")
        return result

    def creature_inputs(self) -> list[dict[str, Any]]:
        result = []
        for creature in self.tables["creature_template"]:
            entry = creature["Entry"]
            event_scope = event_ai.scope(self.tables, entry)
            spawn_scope = native_spawn.scope(self.source, entry)
            model_ids = native_spawn.model_closure(self.tables, native_spawn.model_roots(self.tables, entry))
            display_inputs = []
            available_models = {row["modelid"]: row for row in self.tables["creature_model_info"]}
            for model_id in sorted(model_ids):
                model = available_models.get(model_id)
                if model is None:
                    self.need("creature_model_info", model_id, "creature.gender_model_not_captured")
                elif model["modelid_alternative"]:
                    self.issue("creature_model_info", [model_id], "creature.alternative_model_context_required",
                               alternative=model["modelid_alternative"])
                display = self.lookup("CreatureDisplayInfo", model_id, {0, 1, 3, 4, 9})
                if display is not None:
                    scale = display.words[4]
                    finite_float(scale, "creature.display_scale_nonfinite")
                    if scale & 0x80000000 or scale == 0:
                        self.issue("CreatureDisplayInfo", [model_id], "creature.display_scale_nonpositive")
                    display_inputs.append({"id": model_id, "model_id": display.words[1],
                        "extended_display_info_id": display.words[3], "scale_binary32": scale,
                        "size_class": signed(display.words[9]), "source": display.raw()})
            result.append({"entry": entry, "source_row": creature,
                "spawns": [r for r in self.tables["creature"] if r["id"] == entry],
                "class_level_inputs": [r for r in self.tables["creature_template_classlevelstats"]
                                       if r["Class"] == creature["UnitClass"]
                                       and creature["MinLevel"] <= r["Level"] <= creature["MaxLevel"]],
                "model_inputs": [r for r in self.tables["creature_model_info"]
                                 if r["modelid"] in model_ids],
                "display_inputs": display_inputs, "selected_display_ids": sorted(model_ids),
                "event_ai_capture": event_scope,
                "native_spawn_dependencies": spawn_scope,
                "resolved_combat_stats": None, "ai_profile": None})
            self.issue("creature_template", [entry], "creature.ai_selection_and_native_melee_required",
                       ai_name=creature["AIName"], scripted=bool(creature["ScriptName"]),
                       movement_type=creature["MovementType"], unit_class=creature["UnitClass"])
            if spawn_scope is None:
                self.issue("creature_template", [entry], "creature.spawn_dependency_coverage_not_proven")
            else:
                for spawn in spawn_scope:
                    if spawn["spawn_template"]["row"] is None:
                        self.issue("creature", [spawn["guid"]], "creature.default_spawn_template_absent")
                    if spawn["unimplemented_hook_tables"]:
                        self.issue("creature", [spawn["guid"]], "creature.applicable_spawn_hooks_require_execution",
                                   tables=spawn["unimplemented_hook_tables"])
            self.issue("creature_template", [entry],
                       "creature.unexported_groups_addons_assistance_context" if spawn_scope is None
                       else "creature.unexported_groups_assistance_context")
            if event_scope is None:
                self.issue("creature_template", [entry], "creature.event_ai_applicability_absence_not_proven")
            elif event_scope["matching_rows"]:
                self.issue("creature_template", [entry], "creature.event_ai_matching_rows_require_execution",
                           event_ids=[r["id"] for r in event_scope["matching_rows"]])
            # Even an empty captured selector set does not cover substituted
            # entries, pools, groups or other hooks; those blockers remain.
            self.issue("creature_template", [entry], "creature.explicit_world_rules_and_aggro_context_required")
            self.issue("creature_template", [entry], "creature.deterministic_respawn_selection_required")
        return result

    def candidate_paths(self, spells: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Describe actual structural paths. These are NOT eligible/playable claims."""
        spawns = self.tables["creature"]
        paths = []
        spell_candidates = [s["id"] for s in spells if s["initial_sql_spell"] and
                            any(e["Effect"] in {2, 10} for e in s["effects"])]
        for quest in self.tables["quest_template"]:
            objectives = [{"slot": i, "entry": quest[f"ReqCreatureOrGOId{i}"],
                           "count": quest[f"ReqCreatureOrGOCount{i}"],
                           "spell": quest[f"ReqSpellCast{i}"]} for i in range(1, 5)
                          if quest[f"ReqCreatureOrGOId{i}"] or quest[f"ReqCreatureOrGOCount{i}"]]
            item_objectives = [{"slot": i, "item": quest[f"ReqItemId{i}"],
                                "count": quest[f"ReqItemCount{i}"]} for i in range(1, 5)
                               if quest[f"ReqItemId{i}"] or quest[f"ReqItemCount{i}"]]
            starts = {r["id"] for r in self.tables["creature_questrelation"] if r["quest"] == quest["entry"]}
            finishes = {r["id"] for r in self.tables["creature_involvedrelation"] if r["quest"] == quest["entry"]}
            targets = {o["entry"] for o in objectives}
            paths.append({"quest_id": quest["entry"], "title": quest["Title"],
                "giver_spawn_guids": [r["guid"] for r in spawns if r["id"] in starts],
                "finisher_spawn_guids": [r["guid"] for r in spawns if r["id"] in finishes],
                "target_spawn_guids": [r["guid"] for r in spawns if r["id"] in targets],
                "objectives": objectives, "initial_direct_effect_spell_ids": spell_candidates,
                "item_objectives": item_objectives,
                "previous_quest": quest["PrevQuestId"],
                "next_quest": quest["NextQuestId"],
                "next_in_chain": quest["NextQuestInChain"],
                "eligibility": "not_evaluated", "runtime_profile": None})
        return paths

    def dispositions(self, relationships_resolved: bool) -> list[dict[str, Any]]:
        result = []
        for table, rows in sorted(self.tables.items()):
            for row in rows:
                consumed = CONSUMED.get(table, set())
                if table == "quest_template" and relationships_resolved:
                    consumed = consumed | set(quest_relationships.FIELDS)
                comments = {"Note", "note", "comments"} & set(row)
                blocked = set(row) - consumed - comments
                reason = BLOCKED_TABLE_REASON.get(table, "player.native_creation_progression_rules_required")
                if table == "creature" and "native_spawn_capture" in self.source:
                    reason = "creature.native_spawn_resolution_required"
                result.append({"table": table, "key": [row[k] for k in KEYS[table]],
                    "capability": PROFILE, "consumed_by_preparation": sorted(consumed),
                    "inert_source_annotations": sorted(comments),
                    "blocked_for_gameplay": {reason: sorted(blocked)} if blocked else {}})
        return result


def compile_starter(source: Any, dbcs: dict[str, DbcTable], request: Request,
                    dbc_errors: list[dict[str, Any]] | None = None,
                    extraction_manifest: dict[str, Any] | None = None) -> dict[str, Any]:
    validate_bundle(source)
    check(source["format"] == STARTER_FORMAT, "compiler.source_v2_required")
    s = Selection.parse(source["selection"]).starter
    request = Request.parse(request.json())
    check((s.race, s.class_id) == (request.race, request.class_id), "compiler.selection_mismatch")
    check(sum(len(row) for rows in source["tables"].values() for row in rows) <= MAX_FIELDS,
          "compiler.field_limit")
    check(set(dbcs) <= set(LAYOUTS), "compiler.unknown_dbc")
    c = Compiler(source, dbcs, request)
    c.issues.extend(dbc_errors or [])
    relationships = quest_relationships.resolve(source)
    for refusal in relationships["refusals"]:
        c.issue("quest_template", [refusal["quest"]], "quest.chain", edge=refusal)
    observed_errors = {i["table"] for i in dbc_errors or []}
    for missing in sorted(set(LAYOUTS) - set(dbcs) - observed_errors):
        c.issue(missing, [], "dbc.file_missing")
    # The capture knew no DBCs. Replace its coarse missing-input messages by the
    # exact resolver's missing records and remaining rule blockers; retain the
    # original findings separately. This does not erase any gameplay requirement.
    replaced = {
        "player.chr_races_classes_map_dbc_not_captured",
        "player.faction_reputation_dbc_not_captured",
        "player.char_start_outfit_dbc_items_not_captured",
        "player.skill_line_race_class_tiers_ability_dbc_not_captured",
    }
    if relationships["status"] == "resolved":
        replaced.update({"quest.chain", "quest.inbound_relationship"})
    c.issues.extend({"table": i["table"], "key": i["key"], "reason": i["reason"],
                     "origin": "source_capture"} for i in source["report"]["unsupported"]
                    if i["reason"] not in replaced)
    identity = c.identity()
    outfit = c.outfit()
    c.factions(identity)
    skills = c.skills()
    spells = c.spells()
    talents = talent_inputs(dbcs.get("Talent"), set(c.learning["spell_ids"]))
    creatures = c.creature_inputs()
    display_table = dbcs.get("CreatureDisplayInfo")
    display_ids = sorted({entry for creature in creatures for entry in creature["selected_display_ids"]})
    display_evidence = {"coverage": "complete_raw_table" if display_table else "not_captured",
        "table_sha256": display_table.sha256 if display_table else None,
        "row_count": len(display_table.rows) if display_table else None,
        "selected_ids": display_ids,
        "missing_ids": [entry for entry in display_ids if entry not in display_table.by_id]
                       if display_table else None}
    c.issue("runtime", [], "native.runtime_profile_not_implemented")
    dispositions = c.dispositions(relationships["status"] == "resolved")
    dbc_bindings = []
    for key, row in sorted(c.selected_dbc.items()):
        used = c.used_dbc[key]
        # Packed tables are accounted in actual stored words/bytes, not falsely
        # as 41 dwords. CharStartOutfit has 38 words, CharBaseInfo has two bytes.
        dbc_bindings.append({"table": row.table, **row.raw(),
            "consumed_by_preparation": sorted(used),
            "blocked_for_gameplay": sorted(set(range(len(row.words))) - used),
            "capability": PROFILE})
    issues = sorted({canonical(i): i for i in c.issues}.values(), key=canonical)
    admission = classify_spell_admission(c.tables, spells, issues, c.learning)
    output = {"format": FORMAT, "capability": PROFILE, "core_revision": CORE_REVISION,
        "client_build_claim": 5875, "client_build_authenticity_verified": False,
        "source_sha256": digest(source), "request": request.json(),
        "extraction_manifest": extraction_manifest,
        "dbc_manifest": [t.manifest() for _, t in sorted(dbcs.items())],
        "status": "blocked", "runtime_export": None,
        "ability_admission": admission,
        "dependency_request": c.learning["dependency_request"],
        "native_spawn_capture": source.get("native_spawn_capture"),
        "spell_script_capture": source.get("spell_script_capture"),
        "initial_hook_capture": source.get("initial_hook_capture"),
        "creature_display_inputs": display_evidence,
        "quest_relationships": relationships,
        "player": {"identity": identity, "level_inputs": c.progression(), "outfit": outfit,
                   "initial_sql_spells": source["tables"]["playercreateinfo_spell"],
                   "actions": source["tables"]["playercreateinfo_action"], "skills": skills},
        "spells": spells, "creatures": creatures, "talent_inputs": talents,
        "candidate_paths": c.candidate_paths(spells),
        "required_additional_rows": [{"table": t, "id": i, "reason": r}
                                     for t, i, r in sorted(c.dependencies)],
        "source_field_dispositions": dispositions, "dbc_bindings": dbc_bindings,
        "source_capture_findings": [{**i, "disposition": "replaced_by_exact_dependency_report"
                                     if i["reason"] in replaced else "retained"}
                                    for i in source["report"]["unsupported"]],
        "unsupported": issues}
    output["preparation_sha256"] = digest(output)
    check(len(canonical(output)) <= MAX_REPORT_BYTES, "compiler.report_byte_limit")
    return output


def verify_preparation(preparation: Any, source: Any, dbcs: dict[str, DbcTable],
                       dbc_errors: list[dict[str, Any]] | None = None,
                       extraction_manifest: dict[str, Any] | None = None) -> None:
    """Recompile all decisions; a rewritten hash cannot remove an unsupported field."""
    check(type(preparation) is dict and "request" in preparation, "compiler.preparation_fields")
    expected = compile_starter(source, dbcs, Request.parse(preparation["request"]), dbc_errors,
                               extraction_manifest)
    check(canonical(expected) == canonical(preparation), "compiler.preparation_mismatch")
