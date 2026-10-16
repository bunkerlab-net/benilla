"""Pure capture planning/validation. No SQL execution or external data in this module.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

from decimal import Decimal, InvalidOperation
import hashlib
import json
import math
import re
from dataclasses import dataclass
from typing import Any

from .columns import (COLUMNS, STARTER_SCHEMA, LEARNING_SCHEMA, SPAWN_SCHEMA, REGISTRY_SCHEMA,
                      INITIAL_HOOK_SCHEMA, EVENT_TEXT_SCHEMA, EVENT_TEXT_PRIMARY_KEYS, EVENT_TEXT_ALIASES)

FORMAT = "benilla-classic-source-slice-v1"
STARTER_FORMAT = "benilla-classic-source-slice-v2"
CORE_REVISION = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
DATABASE_REVISION = "ec4f596146be6467ea93c57397858e329e2db852"
MAX_ROWS = 4096
MAX_QUESTS = 512
MAX_ITEMS = 2048
MAX_SPELLS = 512
MAX_ROW_BYTES = 65536
MAX_TABLE_BYTES = 16 * 1024 * 1024
MAX_BUNDLE_BYTES = 64 * 1024 * 1024
MAX_SCHEMA_BYTES = 2 * 1024 * 1024
MAX_TEXT_BYTES = 16384
KEYS = {
    "spell_area": ("spell", "area", "quest_start", "quest_start_active", "aura_spell", "racemask", "gender"),
    "spell_proc_event": ("entry",),
    "spell_scripts": ("Id",),
    "creature_spawn_data": ("Guid",),
    # Native SQL allows repeated Entry, but the loader overwrites by Entry without
    # ORDER BY. Reject duplicate Entries rather than choose a nondeterministic row.
    "creature_spawn_data_template": ("Entry",),
    "game_event_creature": ("guid", "event"), "pool_creature": ("guid",),
    "pool_creature_template": ("id",), "creature_addon": ("guid",),
    "creature_template_addon": ("entry",),
    "spell_chain": ("spell_id",), "spell_learn_spell": ("entry", "SpellID"),
    "creature_ai_scripts": ("id",),
    "broadcast_text": ("Id",), "broadcast_text_locale": ("Id", "Locale"),
    "creature": ("guid",), "creature_template": ("Entry",),
    "creature_model_info": ("modelid",), "quest_template": ("entry",),
    "creature_questrelation": ("id", "quest"),
    "creature_involvedrelation": ("id", "quest"), "item_template": ("entry",),
    "creature_template_classlevelstats": ("Class", "Level"),
    "creature_loot_template": ("entry", "item"),
    "reference_loot_template": ("entry", "item"), "spell_template": ("Id",),
    "playercreateinfo": ("race", "class"),
    # No upstream unique key: repeated item IDs are rejected, never collapsed/summed.
    "playercreateinfo_item": ("race", "class", "itemid"),
    "playercreateinfo_spell": ("race", "class", "Spell"),
    "playercreateinfo_action": ("race", "class", "button"),
    "playercreateinfo_skills": ("raceMask", "classMask", "skill"),
    "player_classlevelstats": ("class", "level"),
    "player_levelstats": ("race", "class", "level"), "player_xp_for_level": ("lvl",),
}
TEXT_COLUMNS = {
    "creature_ai_scripts": {"comment"},
    "broadcast_text": {"Text", "Text1"},
    "broadcast_text_locale": {"Locale", "Text_lang", "Text1_lang"},
    "spell_scripts": {"ScriptName"},
    "creature_spawn_data_template": {"Name"},
    "pool_creature": {"description"}, "pool_creature_template": {"description"},
    "creature_addon": {"auras"}, "creature_template_addon": {"auras"},
    "db_version": {"version", "creature_ai_version"},
    "playercreateinfo_spell": {"Note"}, "playercreateinfo_skills": {"note"},
    "creature_template": {"Name", "SubName", "AIName", "ScriptName"},
    "quest_template": {"Title", "Details", "Objectives", "OfferRewardText",
        "RequestItemsText", "EndText", "ObjectiveText1", "ObjectiveText2", "ObjectiveText3", "ObjectiveText4"},
    "item_template": {"name", "description", "ScriptName"},
    "creature_loot_template": {"comments"}, "reference_loot_template": {"comments"},
}
INTEGER_SQL = {"tinyint", "smallint", "mediumint", "int", "integer", "bigint", "bit"}
DECIMAL_SQL = {"float", "double", "decimal", "numeric", "real"}
TEXT_SQL = {"char", "varchar", "text", "tinytext", "mediumtext", "longtext"}

class ExportError(ValueError):
    """Safe error codes omit credentials, imported text and database exception details."""

def check(ok: bool, code: str) -> None:
    if not ok:
        raise ExportError(code)

def integer(value: Any, low: int, high: int, code: str) -> int:
    check(type(value) is int and low <= value <= high, code)
    return value

def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False,
                      allow_nan=False).encode("utf-8")

def digest(value: Any) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()

def _pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result = {}
    for key, value in pairs:
        check(key not in result, "json.duplicate_field")
        result[key] = value
    return result

def decode_json(data: bytes, max_bytes: int) -> Any:
    check(len(data) <= max_bytes, "json.byte_limit")
    try:
        return json.loads(data, object_pairs_hook=_pairs,
                          parse_constant=lambda _: (_ for _ in ()).throw(ExportError("json.nonfinite")))
    except (ValueError, UnicodeError, RecursionError) as error:
        if isinstance(error, ExportError):
            raise
        raise ExportError("json.invalid") from None

@dataclass(frozen=True)
class StarterSelection:
    race: int
    class_id: int
    level_cap: int

    @classmethod
    def parse(cls, value: Any) -> StarterSelection:
        check(type(value) is dict and set(value) == {"race", "class", "level_cap"}, "selection.starter_fields")
        race = integer(value["race"], 1, 8, "selection.starter_race")
        class_id = integer(value["class"], 1, 11, "selection.starter_class")
        check(class_id in {1, 2, 3, 4, 5, 7, 8, 9, 11}, "selection.starter_class")
        cap = integer(value["level_cap"], 1, 60, "selection.starter_level_cap")
        return cls(race, class_id, cap)

    def json(self) -> dict[str, int]:
        return {"race": self.race, "class": self.class_id, "level_cap": self.level_cap}


@dataclass(frozen=True)
class Selection:
    map_id: int
    minimum: tuple[int, int, int]
    maximum: tuple[int, int, int]
    quest_ids: tuple[int, ...] = ()
    spell_ids: tuple[int, ...] = ()
    starter: StarterSelection | None = None

    @classmethod
    def parse(cls, value: Any) -> Selection:
        expected = {"format_version", "map_id", "min_milliunits", "max_milliunits", "quest_ids", "spell_ids"}
        check(type(value) is dict, "selection.fields")
        version = integer(value.get("format_version"), 1, 2, "selection.version")
        if version == 2:
            expected.add("starter")
        check(set(value) == expected, "selection.fields")
        starter = StarterSelection.parse(value["starter"]) if version == 2 else None
        map_id = integer(value["map_id"], 0, 65535, "selection.map")
        coordinates = []
        for name in ["min_milliunits", "max_milliunits"]:
            row = value[name]
            check(type(row) is list and len(row) == 3, "selection.coordinates")
            coordinates.append(tuple(integer(x, -100000000, 100000000, "selection.coordinate") for x in row))
        check(all(a <= b for a, b in zip(*coordinates)), "selection.reversed_bounds")
        ids = []
        for name, limit in [("quest_ids", MAX_QUESTS), ("spell_ids", MAX_SPELLS)]:
            rows = value[name]
            check(type(rows) is list and len(rows) <= limit, "selection.id_limit")
            parsed = tuple(sorted(integer(x, 1, 0xffffffff, "selection.id") for x in rows))
            check(len(set(parsed)) == len(parsed), "selection.duplicate_id")
            ids.append(parsed)
        return cls(map_id, coordinates[0], coordinates[1], ids[0], ids[1], starter)

    def json(self) -> dict[str, Any]:
        result = {"format_version": 2 if self.starter is not None else 1, "map_id": self.map_id,
                "min_milliunits": list(self.minimum), "max_milliunits": list(self.maximum),
                "quest_ids": list(self.quest_ids), "spell_ids": list(self.spell_ids)}
        if self.starter is not None:
            result["starter"] = self.starter.json()
        return result


def required_tables(selection: Selection) -> set[str]:
    minimum = {"db_version", "creature", "creature_template", "creature_model_info",
               "creature_template_classlevelstats", "creature_questrelation",
               "creature_involvedrelation", "quest_template", "item_template"}
    if selection.spell_ids or selection.starter is not None:
        minimum.add("spell_template")
    if selection.starter is not None:
        minimum.update(STARTER_SCHEMA)
        minimum.update(LEARNING_SCHEMA)
        minimum.update(SPAWN_SCHEMA)
        minimum.update(REGISTRY_SCHEMA)
        minimum.add("creature_ai_scripts")
    return minimum


def schema_profile(inventory: Any, required: set[str]) -> dict[str, Any]:
    check(type(inventory) is dict and set(inventory) == {"format_version", "tables"}, "schema.fields")
    integer(inventory["format_version"], 1, 1, "schema.version")
    check(type(inventory["tables"]) is dict, "schema.tables")
    check(set(inventory["tables"]) <= set(COLUMNS), "schema.unreviewed_table")
    check(required <= set(inventory["tables"]), "schema.missing_table")
    profiles = {}
    for table, meta in inventory["tables"].items():
        fields = {"engine", "table_type", "columns"}
        if table in (REGISTRY_SCHEMA | INITIAL_HOOK_SCHEMA | EVENT_TEXT_SCHEMA) and type(meta) is dict and "primary_key" in meta:
            fields.add("primary_key")
        check(type(meta) is dict and set(meta) == fields, "schema.table_fields")
        if table in REGISTRY_SCHEMA and table in required:
            check(meta.get("primary_key") == ["Id"], "schema.registry_primary_key")
        if table in INITIAL_HOOK_SCHEMA and table in required:
            check(meta.get("primary_key") == list(KEYS[table]), "schema.initial_hook_primary_key")
        check(meta["table_type"] == "BASE TABLE", "schema.views_rejected")
        check(meta["engine"] in {"InnoDB", "MyISAM"}, "schema.engine")
        check(type(meta["columns"]) is list and len(meta["columns"]) <= 512, "schema.columns")
        by_name = {}
        for column in meta["columns"]:
            check(type(column) is dict and set(column) == {"name", "type", "nullable", "ordinal"}, "schema.column_fields")
            name = column["name"]
            check(type(name) is str and name.replace("_", "").isalnum() and len(name) <= 64, "schema.column_name")
            check(name.casefold() not in by_name, "schema.duplicate_column")
            check(type(column["type"]) is str and len(column["type"]) <= 128, "schema.column_type")
            check(type(column["nullable"]) is bool, "schema.nullability")
            integer(column["ordinal"], 1, 512, "schema.ordinal")
            by_name[name.casefold()] = column
        check(len({c["ordinal"] for c in meta["columns"]}) == len(meta["columns"]), "schema.duplicate_ordinal")
        wanted = set(c.casefold() for c in COLUMNS[table])
        missing = wanted - set(by_name)
        if table in required:
            check(not missing, f"schema.missing_columns:{table}")
        if table in (STARTER_SCHEMA | LEARNING_SCHEMA | SPAWN_SCHEMA | REGISTRY_SCHEMA | INITIAL_HOOK_SCHEMA) and table in required:
            check(wanted == set(by_name), f"schema.starter_columns:{table}")
            for ordinal, (name, kind, nullable) in enumerate((STARTER_SCHEMA | LEARNING_SCHEMA | SPAWN_SCHEMA | REGISTRY_SCHEMA | INITIAL_HOOK_SCHEMA)[table], 1):
                actual = by_name[name.casefold()]
                observed, expected = sql_type(actual["type"]), sql_type(kind)
                # Ignore display width for integer types, never size/signedness/nullability/order.
                equal = observed == expected
                if expected[0] in INTEGER_SQL:
                    equal = (observed[0], observed[3]) == (expected[0], expected[3])
                check(equal and actual["nullable"] == nullable and actual["ordinal"] == ordinal,
                      f"schema.starter_type:{table}:{name}")
        if table in EVENT_TEXT_SCHEMA and table in required:
            check(wanted == set(by_name), f"schema.event_text_columns:{table}")
            source_names = {}
            for ordinal, (name, kind, nullable) in enumerate(EVENT_TEXT_SCHEMA[table], 1):
                actual = by_name[name.casefold()]
                allowed = EVENT_TEXT_ALIASES.get(table, {}).get(name, (name,))
                check(actual["name"] in allowed, f"schema.event_text_spelling:{table}:{name}")
                observed, expected = sql_type(actual["type"]), sql_type(kind)
                equal = observed == expected
                if expected[0] in INTEGER_SQL:
                    equal = (observed[0], observed[3]) == (expected[0], expected[3])
                # The core base and the pinned ClassicDB dump differ
                # only in id signedness. Never generalize this to parameters.
                if table == "creature_ai_scripts" and name == "id":
                    equal = observed[0] == "int"
                check(equal and actual["nullable"] == nullable and actual["ordinal"] == ordinal,
                      f"schema.event_text_type:{table}:{name}")
                source_names[name] = actual["name"]
            check(meta.get("primary_key") == [source_names[name] for name in EVENT_TEXT_PRIMARY_KEYS[table]],
                  f"schema.event_text_primary_key:{table}")
        checked_columns = {}
        for name in COLUMNS[table]:
            if name.casefold() not in by_name:
                continue
            column = by_name[name.casefold()]
            family, precision, scale, unsigned = sql_type(column["type"])
            text = name in TEXT_COLUMNS.get(table, set())
            check(family in (TEXT_SQL if text else INTEGER_SQL | DECIMAL_SQL), f"schema.type_family:{table}:{name}")
            if (table == "creature" and name in {"position_x", "position_y", "position_z", "orientation", "spawndist"}) or (table == "creature_model_info" and name in {"bounding_radius", "combat_reach", "SpeedWalk", "SpeedRun"}):
                check(family in DECIMAL_SQL, f"schema.decimal_field:{table}:{name}")
            checked_columns[name] = column
        profiles[table] = {"columns": checked_columns,
                           "unreviewed_columns": sorted(set(by_name) - wanted),
                           "engine": meta["engine"]}
        if table in EVENT_TEXT_SCHEMA and table in required:
            profiles[table]["source_names"] = source_names
            profiles[table]["schema_variant"] = (
                "core_base_unsigned_event_id" if table == "creature_ai_scripts"
                and sql_type(by_name["id"]["type"])[3] else "classic_db")
    return profiles


def sql_type(raw: str) -> tuple[str, int | None, int | None, bool]:
    match = re.fullmatch(r"([a-z]+)(?:\((\d+)(?:,(\d+))?\))?( unsigned)?", raw.lower())
    check(match is not None, "schema.sql_type")
    family, first, second, unsigned = match.groups()
    check(family in INTEGER_SQL | DECIMAL_SQL | TEXT_SQL, "schema.sql_type")
    first = int(first) if first is not None else None
    second = int(second) if second is not None else None
    if family in {"decimal", "numeric"}:
        check(first is not None and second is not None and 1 <= first <= 65 and 0 <= second <= min(first, 30), "schema.decimal_type")
    elif family == "bit":
        check(first is not None and 1 <= first <= 64 and second is None, "schema.bit_type")
    else:
        check(second is None, "schema.sql_type")
        if family in {"char", "varchar"}:
            check(first is not None and 1 <= first <= 65535, "schema.string_type")
        elif family in INTEGER_SQL:
            check(first is None or 1 <= first <= 255, "schema.integer_type")
        else:
            check(first is None, "schema.sql_type")
    return family, first, second, bool(unsigned)


def normalize_cell(value: Any, column: dict[str, Any]) -> Any:
    if value is None:
        check(column["nullable"], "row.unexpected_null")
        return None
    family, precision, scale, unsigned = sql_type(column["type"])
    if family in TEXT_SQL:
        check(type(value) is str and len(value.encode("utf-8")) <= MAX_TEXT_BYTES, "row.text_limit")
        check("\0" not in value, "row.nul_text")
        check(family not in {"char", "varchar"} or len(value) <= precision, "row.sql_string_length")
        return value
    if family in INTEGER_SQL:
        integer(value, -(1 << 63), (1 << 64) - 1, "row.integer")
        unsigned = unsigned or family == "bit"
        bits = {"tinyint": 8, "smallint": 16, "mediumint": 24, "int": 32,
                "integer": 32, "bigint": 64, "bit": precision}[family]
        integer(value, 0 if unsigned else -(1 << (bits-1)),
                (1 << bits)-1 if unsigned else (1 << (bits-1))-1, "row.sql_integer_range")
        return value
    check(family in DECIMAL_SQL, "row.unreviewed_type")
    check(type(value) in {int, float, Decimal, str}, "row.decimal_type")
    if type(value) is float:
        check(math.isfinite(value), "row.nonfinite")
    check(len(str(value)) <= 96, "row.decimal_text_limit")
    try:
        number = Decimal(str(value))
    except InvalidOperation:
        raise ExportError("row.decimal") from None
    check(number.is_finite() and number.copy_abs() <= Decimal("1e30"), "row.decimal_range")
    # Bounded decimal strings retain database precision without gameplay rounding.
    check(number.as_tuple().exponent >= -60 and len(number.as_tuple().digits) <= 64, "row.decimal_precision")
    result = format(number, "f")
    check(len(result) <= 96, "row.decimal_precision")
    if "." in result:
        result = result.rstrip("0").rstrip(".")
    if number.is_zero():
        result = "0"
    if family in {"decimal", "numeric"}:
        integer_part, _, fractional_part = result.lstrip("-").partition(".")
        integer_digits = len(integer_part.lstrip("0"))
        check(integer_digits <= precision-scale and len(fractional_part) <= scale, "row.sql_decimal_precision")
    check(not unsigned or number >= 0, "row.sql_unsigned_decimal")
    return {"decimal": result}


def normalize_rows(table: str, rows: list[dict[str, Any]], profile: dict[str, Any], limit: int) -> list[dict[str, Any]]:
    check(table in COLUMNS, "query.unreviewed_table")
    check(len(rows) <= limit, f"rows.limit:{table}")
    normalized = []
    keys = set()
    total = 0
    columns = profile["columns"]
    check(set(columns) == set(COLUMNS[table]), f"schema.missing_columns:{table}")
    for row in rows:
        check(type(row) is dict and set(row) == set(columns), f"row.fields:{table}")
        converted = {name: normalize_cell(row[name], column) for name, column in columns.items()}
        key = tuple(converted[name] for name in KEYS[table])
        zero_keys = {"playercreateinfo_action": {"button"},
                     "playercreateinfo_skills": {"raceMask", "classMask"},
                     "creature_spawn_data_template": {"Entry"},
                     "spell_area": {"area", "quest_start", "quest_start_active", "racemask", "gender"}}.get(table, set())
        check(all((type(row_value) is str and 0 < len(row_value) <= 10)
                  if table == "broadcast_text_locale" and name == "Locale" else
                  (type(row_value) is int and row_value >= (-32768 if table == "game_event_creature" and name == "event"
                               else -0x800000 if table == "spell_area" and name == "aura_spell"
                               else 0 if name in zero_keys else 1))
                  for name, row_value in zip(KEYS[table], key)), f"row.key:{table}")
        check(key not in keys, f"row.duplicate_key:{table}")
        keys.add(key)
        size = len(canonical(converted))
        check(size <= MAX_ROW_BYTES, f"row.byte_limit:{table}")
        total += size
        check(total <= MAX_TABLE_BYTES, f"table.byte_limit:{table}")
        normalized.append(converted)
    return sorted(normalized, key=lambda row: tuple(row[name] for name in KEYS[table]))


def require_ids(table: str, rows: list[dict[str, Any]], ids: set[int]) -> None:
    present = {row[KEYS[table][0]] for row in rows}
    check(present == ids, f"reference.missing_or_unexpected:{table}")


def referenced_items(quests: list[dict[str, Any]]) -> set[int]:
    result = set()
    for row in quests:
        for name in COLUMNS["quest_template"]:
            if name in {"SrcItemId"} or name.startswith(("ReqItemId", "ReqSourceId", "RewItemId", "RewChoiceItemId")):
                if row[name]:
                    result.add(integer(row[name], 1, 0xffffffff, "reference.item_id"))
    return result


def report(tables: dict[str, list[dict[str, Any]]], profiles: dict[str, Any]) -> list[dict[str, Any]]:
    issues = []
    def add(table: str, key: int | str, reason: str) -> None:
        issues.append({"table": table, "key": key, "status": "blocked", "reason": reason})
    for table, rows in tables.items():
        if profiles[table]["unreviewed_columns"]:
            add(table, "schema", "schema.unreviewed_columns")
        for row in rows:
            key = ":".join(str(row[name]) for name in KEYS[table])
            if table == "creature":
                if not set(SPAWN_SCHEMA) <= set(tables):
                    add(table, key, "spawn.overrides_pools_events_not_captured")
                if row["MovementType"] or row["spawndist"] != {"decimal": "0"}:
                    add(table, key, "movement.native_rules_required")
            elif table == "creature_template":
                add(table, key, "creature.native_stats_combat_faction_required")
                if row["MinLevel"] != row["MaxLevel"]:
                    add(table, key, "creature.variable_level")
                if row["AIName"] or row["ScriptName"] or row["SpellList"] or row["CharmedSpellList"]:
                    add(table, key, "creature.ai_or_spell_dependency")
                if row["LootId"] or row["SkinningLootId"] or row["PickpocketLootId"]:
                    add(table, key, "creature.loot_dependency_not_captured")
            elif table == "quest_template":
                add(table, key, "quest.native_eligibility_credit_reward_required")
                if row["RewMoneyMaxLevel"]:
                    add(table, key, "quest.dynamic_xp")
                if row["RequiredCondition"] or row["StartScript"] or row["CompleteScript"]:
                    add(table, key, "quest.condition_or_script")
                if row["PrevQuestId"] or row["NextQuestId"] or row["ExclusiveGroup"] or row["BreadcrumbForQuestId"]:
                    add(table, key, "quest.chain")
                if any(row[f"ReqCreatureOrGOId{i}"] < 0 or row[f"ReqSpellCast{i}"] or row[f"ReqItemId{i}"] for i in range(1,5)):
                    add(table, key, "quest.nonkill_objective")
            elif table == "spell_template":
                add(table, key, "spell.native_effect_target_scaling_rules_required")
                effects = [row[f"Effect{i}"] for i in range(1,4)]
                if sum(bool(x) for x in effects) != 1:
                    add(table, key, "spell.multiple_or_missing_effects")
                if any(row[f"EffectApplyAuraName{i}"] or row[f"EffectTriggerSpell{i}"] for i in range(1,4)):
                    add(table, key, "spell.aura_or_trigger_dependency")
            elif table == "item_template":
                add(table, key, "item.native_inventory_equipment_rules_required")
            elif table == "spell_scripts":
                add(table, key, "spell.registry_row_requires_named_handler_semantics")
            elif table == "creature_ai_scripts":
                add(table, key, "creature.event_ai_matching_row_requires_execution")
    return sorted(issues, key=lambda x: (x["table"], str(x["key"]), x["reason"]))


def normalize_version_notes(rows: Any, profile: dict[str, Any]) -> list[dict[str, Any]]:
    check(type(rows) is list and len(rows) == 1 and type(rows[0]) is dict,
          "schema.db_version_singleton")
    columns = profile["columns"]
    check(set(columns) == set(COLUMNS["db_version"]) and set(rows[0]) == set(columns),
          "schema.version_fields")
    result = {name: normalize_cell(rows[0][name], column) for name, column in columns.items()}
    check(all(value is None or len(value.encode("utf-8")) <= 1024 for value in result.values()),
          "schema.version_value")
    return [result]
