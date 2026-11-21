"""Same-snapshot completion scripts and ordinary quest-gossip dependencies.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1 ScriptMgr.cpp:117-159; Player.cpp:11516-12003.
"""
from dataclasses import dataclass
from .quest_mechanics_columns import SCHEMA, KEYS, COLUMNS
from .core import canonical, check, digest, integer, normalize_cell, sql_type, MAX_ROWS, MAX_TABLE_BYTES
from . import event_text

FORMAT = "benilla-classic-quest-mechanics-v1"
MODE = "quest_mechanics"
TABLES = set(SCHEMA) | {"npc_text", "npc_text_broadcast_text", "locales_npc_text"} | set(event_text.TABLES)

@dataclass(frozen=True)
class MechanicsQuery:
    table: str
    ids: tuple[int, ...]
    mode: str = MODE
    limit: int = MAX_ROWS

    def sql(self):
        check(self.table in SCHEMA and self.mode == MODE, "quest_mechanics.query")
        check(0 < len(self.ids) <= MAX_ROWS and tuple(sorted(set(self.ids))) == self.ids, "quest_mechanics.selectors")
        for key in self.ids:
            integer(key, 0 if self.table.startswith("gossip_") else 1, 0x7fffffff, "quest_mechanics.selector")
        integer(self.limit, 1, MAX_ROWS, "quest_mechanics.limit")
        first = {"dbscripts_on_quest_end": "id", "gossip_menu": "entry",
                 "gossip_menu_option": "menu_id", "conditions": "condition_entry"}[self.table]
        order = "priority" if self.table == "dbscripts_on_quest_end" else ",".join(KEYS[self.table])
        return ("SELECT " + ",".join(f"`{name}`" for name in COLUMNS[self.table])
                + f" FROM `{self.table}` WHERE `{first}` IN (" + ",".join(["%s"] * len(self.ids))
                + ") ORDER BY " + ",".join(f"`{name}`" for name in order.split(",")) + " LIMIT %s",
                self.ids + (self.limit + 1,))

def schema(inventory):
    check(TABLES <= set(inventory["tables"]), "quest_mechanics.missing_tables")
    for table, expected in SCHEMA.items():
        meta = inventory["tables"][table]
        check(meta.get("primary_key") == list(KEYS[table]), f"quest_mechanics.primary_key:{table}")
        actual = meta["columns"]
        check(len(actual) == len(expected), f"quest_mechanics.columns:{table}")
        for ordinal, (column, (name, kind, nullable)) in enumerate(zip(actual, expected), 1):
            observed, wanted = sql_type(column["type"]), sql_type(kind)
            if wanted[0] in {"int", "mediumint", "smallint", "tinyint"}:
                observed, wanted = (observed[0], observed[3]), (wanted[0], wanted[3])
            check(column["name"] == name and column["ordinal"] == ordinal and column["nullable"] == nullable
                  and observed == wanted, f"quest_mechanics.column:{table}:{name}")

def normalize(table, raw, inventory):
    check(type(raw) is list and len(raw) <= MAX_ROWS, "quest_mechanics.row_limit")
    columns = {column["name"]: column for column in inventory["tables"][table]["columns"]}
    result = []
    keys = set()
    for row in raw:
        check(type(row) is dict and set(row) == set(COLUMNS[table]), "quest_mechanics.row_fields")
        row = {key: normalize_cell(value["decimal"] if type(value) is dict and set(value) == {"decimal"} else value,
                                   columns[key]) for key, value in row.items()}
        if KEYS[table]:
            key = tuple(row[name] for name in KEYS[table])
            check(key not in keys, "quest_mechanics.duplicate_key")
            keys.add(key)
        check(len(canonical(row)) <= 65536, "quest_mechanics.row_bytes")
        result.append(row)
    check(len(canonical(result)) <= MAX_TABLE_BYTES, "quest_mechanics.table_bytes")
    return sorted(result, key=canonical)

def selectors(tables):
    givers = sorted({r["id"] for t in ("creature_questrelation", "creature_involvedrelation") for r in tables[t]})
    return {"scripts": sorted({r["CompleteScript"] for r in tables["quest_template"] if r["CompleteScript"]}),
            "menus": sorted({0} | {r["GossipMenuId"] for r in tables["creature_template"] if r["Entry"] in givers}),
            "entries": givers}

def fetch(source, tables, inventory, profiles, account, get):
    schema(inventory)
    scope = selectors(tables)
    def query(table, keys):
        if not keys:
            return []
        plan = MechanicsQuery(table, tuple(sorted(keys)))
        plan.sql()
        return account(normalize(table, source.fetch(plan), inventory))
    raw = {"dbscripts_on_quest_end": query("dbscripts_on_quest_end", scope["scripts"]),
           "gossip_menu": query("gossip_menu", scope["menus"]),
           "gossip_menu_option": query("gossip_menu_option", scope["menus"])}
    ids = {r["condition_id"] for rows in raw.values() for r in rows if r["condition_id"]}
    raw["conditions"] = query("conditions", ids)
    text_ids = {r["text_id"] for r in raw["gossip_menu"] if r["text_id"]}
    for table in ("npc_text", "npc_text_broadcast_text", "locales_npc_text"):
        raw[table] = get(table, text_ids)
    broadcasts = {r["dataint"] for r in raw["dbscripts_on_quest_end"] if r["command"] == 0 and r["dataint"] > 0}
    broadcasts |= {r[f"BroadcastTextId{i}"] for r in raw["npc_text_broadcast_text"] for i in range(8) if r[f"BroadcastTextId{i}"]}
    broadcasts |= {r[field] for r in raw["gossip_menu_option"] for field in ("option_broadcast_text", "box_broadcast_text") if r[field] > 0}
    for table in event_text.TABLES:
        raw[table] = get(table, broadcasts, mode=event_text.MODE)
    return raw

def evidence(bundle, raw):
    schema(bundle["schema"])
    check(type(raw) is dict and set(raw) == TABLES, "quest_mechanics.tables")
    scope = selectors(bundle["tables"])
    conditions = sorted({r["condition_id"] for t in ("dbscripts_on_quest_end", "gossip_menu", "gossip_menu_option")
                         for r in raw[t] if r["condition_id"]})
    text_ids = sorted({r["text_id"] for r in raw["gossip_menu"] if r["text_id"]})
    broadcasts = sorted({r["dataint"] for r in raw["dbscripts_on_quest_end"] if r["command"] == 0 and r["dataint"] > 0}
        | {r[f"BroadcastTextId{i}"] for r in raw["npc_text_broadcast_text"] for i in range(8) if r[f"BroadcastTextId{i}"]}
        | {r[field] for r in raw["gossip_menu_option"] for field in ("option_broadcast_text", "box_broadcast_text") if r[field] > 0})
    lookups = {"dbscripts_on_quest_end": scope["scripts"], "gossip_menu": scope["menus"],
               "gossip_menu_option": scope["menus"], "conditions": conditions,
               **{t: text_ids for t in ("npc_text", "npc_text_broadcast_text", "locales_npc_text")},
               **{t: broadcasts for t in event_text.TABLES}}
    first = {"dbscripts_on_quest_end": "id", "gossip_menu": "entry", "gossip_menu_option": "menu_id",
             "conditions": "condition_entry", "npc_text": "ID", "npc_text_broadcast_text": "Id",
             "locales_npc_text": "entry", "broadcast_text": "Id", "broadcast_text_locale": "Id"}
    for table, keys in lookups.items():
        check(all(r[first[table]] in keys for r in raw[table]), f"quest_mechanics.unselected_key:{table}")
    check(len(canonical(raw)) <= 16 * 1024 * 1024, "quest_mechanics.byte_limit")
    return {"format": FORMAT, "selectors": scope, "lookup_keys": lookups, "rows": raw,
            "rows_sha256": digest(raw), "loaded_script_order": None,
            "snapshot": {k: bundle[k] for k in ("core_revision", "database_revision", "schema_sha256", "consistency")}
                | {"selection_sha256": digest(bundle["selection"]), "tables_sha256": digest(bundle["table_sha256"]),
                   "db_version_sha256": digest(bundle["db_version"])}}

def validate(bundle):
    value = bundle["quest_mechanics_capture"]
    check(type(value) is dict and type(value.get("rows")) is dict, "quest_mechanics.fields")
    from .core import normalize_rows, schema_profile
    profiles = schema_profile(bundle["schema"], TABLES)
    for table, raw in value["rows"].items():
        check(table in TABLES, "quest_mechanics.table")
        if table in SCHEMA:
            normalized = normalize(table, raw, bundle["schema"])
        else:
            check(type(raw) is list and all(type(row) is dict for row in raw), "quest_mechanics.row_shape")
            decoded = [{k: (v["decimal"] if type(v) is dict and set(v) == {"decimal"} else v)
                        for k, v in row.items()} for row in raw]
            normalized = normalize_rows(table, decoded, profiles[table], MAX_ROWS)
        check(normalized == raw, "quest_mechanics.row_order_or_shape")
    check(canonical(value) == canonical(evidence(bundle, value["rows"])), "quest_mechanics.binding_mismatch")

def supported(bundle):
    """Resolve raw executable shapes, without inventing loaded order or host proof."""
    value = bundle.get("quest_mechanics_capture")
    if value is None:
        return {"status": "unknown", "scripts": [], "givers": [], "refusals": []}
    validate(bundle)
    raw = value["rows"]
    refusals = []
    def refuse(field, target, reason, quest=0):
        refusals.append({"quest": quest, "field": field, "target": target, "reason": reason})
    broadcasts = {r["Id"]: r for r in raw["broadcast_text"]}
    scripts = set()
    for script in value["selectors"]["scripts"]:
        rows = [r for r in raw["dbscripts_on_quest_end"] if r["id"] == script]
        start = len(refusals)
        if not rows:
            refuse("quest_end_scripts.id", script, "missing_script")
        for row in rows:
            if row["command"] not in (0, 1):
                refuse("quest_end_scripts.command", row["command"], "unsupported_command")
                continue
            if row["delay"] == 0:
                refuse("quest_end_scripts.delay", 0, "immediate_script_not_supported")
            for field in ("buddy_entry", "search_radius", "data_flags", "condition_id",
                          "datalong2", "datalong3", "dataint2", "dataint3", "dataint4"):
                if row[field]:
                    refuse("quest_end_scripts." + field, row[field], "unsupported_path")
            for field in ("datafloat", "x", "y", "z", "o", "speed"):
                if row[field] != {"decimal": "0"}:
                    refuse("quest_end_scripts." + field, 0, "unsupported_path")
            if row["command"] == 0:
                text = broadcasts.get(row["dataint"])
                if row["datalong"] or row["dataint"] <= 0:
                    refuse("quest_end_scripts.dataint", row["dataint"], "unsupported_talk_shape")
                elif text is None:
                    refuse("broadcast_text", row["dataint"], "unknown_lookup")
                elif any(text[f] for f in ("ChatTypeID", "LanguageID", "SoundEntriesID1", "SoundEntriesID2",
                       *(f"EmoteID{i}" for i in range(1, 4)), *(f"EmoteDelay{i}" for i in range(1, 4)))):
                    refuse("broadcast_text", row["dataint"], "unsupported_speech_effects")
            elif row["dataint"]:
                refuse("quest_end_scripts.dataint", row["dataint"], "unsupported_random_emote")
        if len(refusals) == start:
            scripts.add(script)
    conditions = {r["condition_entry"]: r for r in raw["conditions"]}
    quest_ids = {r["entry"] for r in bundle["tables"]["quest_template"]}
    for row in raw["conditions"]:
        if (row["type"] not in (0, 8, 9, 22) or row["flags"] & ~1 or row["value3"] or row["value4"]
                or row["value2"] > 2 or (row["type"] != 9 and row["value2"])
                or (row["type"] == 0 and row["value1"])):
            refuse("conditions.type", row["type"], "unsupported_condition")
        elif row["type"] and row["value1"] not in quest_ids:
            refuse("conditions.value1", row["value1"], "uncaptured_reference")
    for table in ("gossip_menu", "gossip_menu_option"):
        for row in raw[table]:
            if row["condition_id"] and row["condition_id"] not in conditions:
                refuse("conditions.condition_entry", row["condition_id"], "unknown_lookup")
    givers = []
    texts = {r["ID"] for r in raw["npc_text"]} | {r["Id"] for r in raw["npc_text_broadcast_text"]}
    for giver in bundle["tables"]["creature_template"]:
        if giver["Entry"] not in value["selectors"]["entries"]:
            continue
        start = len(refusals)
        menu = giver["GossipMenuId"]
        if menu == 0 and giver["NpcFlags"] & 1:
            refuse("npc_gossip", giver["Entry"], "guid_gossip_text_not_supported")
        menus = [r for r in raw["gossip_menu"] if r["entry"] == menu]
        if menu and not menus:
            refuse("gossip_menu", menu, "unknown_lookup")
        if menu and menus and not any(r["condition_id"] == 0 for r in menus):
            refuse("gossip_menu", menu, "unconditional_text_required")
        seen_conditions = set()
        for row in menus:
            if row["condition_id"] in seen_conditions:
                refuse("gossip_menu.condition_id", row["condition_id"], "ambiguous_loaded_order")
            seen_conditions.add(row["condition_id"])
        for row in menus:
            if row["script_id"]:
                refuse("gossip_menu.script_id", row["script_id"], "unsupported_path")
            if row["text_id"] not in texts:
                refuse("npc_text", row["text_id"], "unknown_lookup")
        options = [r for r in raw["gossip_menu_option"] if r["menu_id"] == menu]
        if not options:
            options = [r for r in raw["gossip_menu_option"] if r["menu_id"] == 0]
        for row in options:
            if row["condition_id"] and row["option_id"] == 2 and not giver["NpcFlags"] & 1:
                refuse("gossip_menu_option.condition_id", row["condition_id"],
                       "conditional_direct_quest_menu_not_supported")
            if not row["npc_option_npcflag"] & giver["NpcFlags"]:
                continue
            if row["option_id"] not in (2, 99):
                refuse("gossip_menu_option.option_id", row["option_id"], "unsupported_option")
            for field in ("action_menu_id", "action_poi_id", "action_script_id", "box_coded", "box_money", "box_broadcast_text"):
                if row[field]:
                    refuse("gossip_menu_option." + field, row[field], "unsupported_path")
            if row["box_text"]:
                refuse("gossip_menu_option.box_text", row["id"], "unsupported_path")
        if len(refusals) == start:
            givers.append(giver["Entry"])
    return {"status": "resolved" if not refusals else "blocked", "scripts": sorted(scripts),
            "givers": givers, "refusals": refusals, "loaded_script_order": None,
            "source_sha256": digest(value)}
