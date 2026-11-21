"""Same-snapshot ordinary giver and presentation projection, never hook/admission proof.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
ObjectMgr.cpp:4231-4880,5775-5909,7940-8035; Player.cpp:11963-11972,12087-12181.
"""
from dataclasses import dataclass
from .columns import COLUMNS
from .quest_columns import KEYS, QUEST_TEXT
from .core import (MAX_ROWS, MAX_QUESTS, MAX_ROW_BYTES, MAX_TABLE_BYTES, INTEGER_SQL,
                   canonical, check, digest, integer, normalize_cell, normalize_rows, sql_type)
from . import event_text

FORMAT = "benilla-classic-quest-giver-presentation-v1"
MODE = "quest_giver_presentation"
DELAYS = ("IncompleteEmoteDelay", "CompleteEmoteDelay")
TABLES = set(KEYS) | set(event_text.TABLES)

@dataclass(frozen=True)
class QuestQuery:
    table: str
    ids: tuple[int, ...]
    mode: str = MODE
    limit: int = MAX_QUESTS

    def sql(self):
        check(self.table == "quest_template" and self.mode == MODE, "quest_presentation.query")
        check(0 < len(self.ids) <= MAX_QUESTS and tuple(sorted(set(self.ids))) == self.ids,
              "quest_presentation.selectors")
        for key in self.ids:
            integer(key, 1, 0x7fffffff, "quest_presentation.selector")
        integer(self.limit, 1, MAX_QUESTS, "quest_presentation.limit")
        columns = COLUMNS[self.table] + list(DELAYS)
        return ("SELECT " + ",".join(f"`{name}`" for name in columns)
                + " FROM `quest_template` WHERE `entry` IN (" + ",".join(["%s"]*len(self.ids))
                + ") ORDER BY `entry` LIMIT %s", self.ids + (self.limit+1,))


def schema(inventory):
    check(TABLES <= set(inventory["tables"]), "quest_presentation.missing_tables")
    for table in KEYS:
        meta = inventory["tables"][table]
        check(meta.get("primary_key") == list(KEYS[table]), f"quest_presentation.primary_key:{table}")
        check({c["name"].casefold() for c in meta["columns"]} == {c.casefold() for c in COLUMNS[table]},
              f"quest_presentation.schema_columns:{table}")
    columns = {c["name"]: c for c in inventory["tables"]["quest_template"]["columns"]}
    for field in DELAYS:
        check(field in columns and not columns[field]["nullable"]
              and sql_type(columns[field]["type"])[0] in INTEGER_SQL,
              f"quest_presentation.delay_schema:{field}")


def normalize_quests(raw, inventory):
    check(type(raw) is list and len(raw) <= MAX_QUESTS, "quest_presentation.quest_limit")
    columns = {c["name"]: c for c in inventory["tables"]["quest_template"]["columns"]}
    names = COLUMNS["quest_template"] + list(DELAYS)
    result = []
    total = 0
    for row in raw:
        check(type(row) is dict and set(row) == set(names), "quest_presentation.quest_fields")
        converted = {name: normalize_cell(row[name], columns[name]) for name in names}
        size = len(canonical(converted))
        total += size
        check(size <= MAX_ROW_BYTES and total <= MAX_TABLE_BYTES, "quest_presentation.quest_byte_limit")
        result.append(converted)
    check(len({row["entry"] for row in result}) == len(result), "quest_presentation.duplicate_quest")
    return sorted(result, key=lambda row: row["entry"])


def selectors(tables):
    entries = sorted({row["id"] for table in ("creature_questrelation", "creature_involvedrelation")
                      for row in tables[table]})
    return {"quests": sorted(row["entry"] for row in tables["quest_template"]),
            "entries": entries,
            "guids": sorted(row["guid"] for row in tables["creature"] if row["id"] in entries)}


def fetch(source, tables, inventory, profiles, account, get):
    schema(inventory)
    scope = selectors(tables)
    query = QuestQuery("quest_template", tuple(scope["quests"]))
    query.sql()
    raw = {"quest_template": account(normalize_quests(source.fetch(query), inventory))}
    for table, keys in (("questgiver_greeting", scope["entries"]),
                        ("locales_questgiver_greeting", scope["entries"]),
                        ("locales_quest", scope["quests"]), ("npc_gossip", scope["guids"])):
        raw[table] = get(table, set(keys))
    text_ids = sorted({0xffffff} | {row["textid"] for row in raw["npc_gossip"] if row["textid"]})
    for table in ("npc_text", "locales_npc_text", "npc_text_broadcast_text"):
        raw[table] = get(table, set(text_ids))
    broadcasts = {row[f"BroadcastTextId{i}"] for row in raw["npc_text_broadcast_text"]
                  for i in range(8) if row[f"BroadcastTextId{i}"]}
    for table in event_text.TABLES:
        raw[table] = get(table, broadcasts, mode=event_text.MODE)
    return raw


def evidence(bundle, raw):
    schema(bundle["schema"])
    check(type(raw) is dict and set(raw) == TABLES | {"quest_template"}, "quest_presentation.tables")
    scope = selectors(bundle["tables"])
    base = bundle["tables"]["quest_template"]
    check([{key: row[key] for key in COLUMNS["quest_template"]} for row in raw["quest_template"]] == base,
          "quest_presentation.base_disagreement")
    check([row["entry"] for row in raw["quest_template"]] == scope["quests"], "quest_presentation.quest_coverage")
    expected = {"questgiver_greeting": scope["entries"], "locales_questgiver_greeting": scope["entries"],
                "locales_quest": scope["quests"], "npc_gossip": scope["guids"]}
    text_ids = sorted({0xffffff} | {row["textid"] for row in raw["npc_gossip"] if row["textid"]})
    expected.update({table: text_ids for table in ("npc_text", "locales_npc_text", "npc_text_broadcast_text")})
    broadcast_ids = sorted({row[f"BroadcastTextId{i}"] for row in raw["npc_text_broadcast_text"]
                            for i in range(8) if row[f"BroadcastTextId{i}"]})
    expected.update({table: broadcast_ids for table in event_text.TABLES})
    for table, keys in expected.items():
        key = COLUMNS[table][0]
        check(all(row[key] in keys for row in raw[table]), f"quest_presentation.unselected_key:{table}")
    check(len(canonical(raw)) <= 16 * 1024 * 1024, "quest_presentation.byte_limit")
    return {"format": FORMAT, "status": "complete_raw_projection", "selectors": scope,
            "lookup_keys": expected, "rows": raw, "rows_sha256": digest(raw),
            "loaded_relation_order": None, "effective_giver_selection": None,
            "registered_callbacks": None, "global_spell_area_and_item_effects": None,
            "session_locale_and_dialog_policy": None,
            "snapshot": {key: bundle[key] for key in ("core_revision", "database_revision", "schema_sha256", "consistency")}
                        | {"selection_sha256": digest(bundle["selection"]), "db_version_sha256": digest(bundle["db_version"]),
                           "tables_sha256": digest(bundle["table_sha256"])}}


def validate(bundle):
    value = bundle["quest_giver_presentation_capture"]
    check(type(value) is dict and type(value.get("rows")) is dict, "quest_presentation.fields")
    from .core import schema_profile
    profiles = schema_profile(bundle["schema"], TABLES | {"quest_template"})
    for table, raw in value["rows"].items():
        check(table in TABLES | {"quest_template"}, "quest_presentation.table")
        check(type(raw) is list and len(raw) <= MAX_ROWS and all(type(row) is dict for row in raw),
              "quest_presentation.row_shape_or_limit")
        decoded = [{name: (cell["decimal"] if type(cell) is dict and set(cell) == {"decimal"} else cell)
                    for name, cell in row.items()} for row in raw]
        normal = normalize_quests(decoded, bundle["schema"]) if table == "quest_template" else normalize_rows(table, decoded, profiles[table], MAX_ROWS)
        check(normal == raw, "quest_presentation.row_shape_or_order")
    check(canonical(value) == canonical(evidence(bundle, value["rows"])), "quest_presentation.binding_mismatch")
