"""Complete selected reputation lookups, not initialized aura or award evidence.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1 ObjectMgr.cpp:7413-7664; shared/Database/Field.h:53-59.
"""
from dataclasses import dataclass
from .core import canonical, check, digest, integer, normalize_cell, sql_type

FORMAT = "benilla-classic-reputation-policy-v1"
LIMIT = 4096
RATE = "reputation_reward_rate"
SPILL = "reputation_spillover_template"
KILL = "creature_onkill_reputation"
FIELDS = {RATE: ["faction", "quest_rate", "creature_rate", "spell_rate"],
          SPILL: ["faction", *[f"{name}{i}" for i in range(1, 5)
                              for name in ("faction", "rate_", "rank_")]],
          KILL: ["creature_id", "RewOnKillRepFaction1", "RewOnKillRepFaction2",
                 "IsTeamAward1", "MaxStanding1", "RewOnKillRepValue1",
                 "IsTeamAward2", "MaxStanding2", "RewOnKillRepValue2", "TeamDependent"]}


def floating(name):
    return name.endswith("_rate") or name.startswith("rate_")


def domain(table, name):
    if floating(name):
        return "float", False
    if table == RATE or name == "creature_id":
        return "mediumint", True
    if table == SPILL:
        return ("tinyint" if name.startswith("rank_") else "smallint"), True
    if name.startswith("RewOnKillRepFaction"):
        return "smallint", False
    if name.startswith("RewOnKillRepValue"):
        return "mediumint", False
    return "tinyint", name == "TeamDependent"


def expected_schema():
    return {table: {"columns": {name.lower(): [*domain(table, name), False] for name in names},
                    "primary_key": [names[0].lower()]} for table, names in FIELDS.items()}


@dataclass(frozen=True)
class SchemaQuery:
    keys: bool
    limit: int = 128

    def sql(self):
        check(type(self.keys) is bool and self.limit == 128, "reputation.schema_query")
        if self.keys:
            columns = "TABLE_NAME AS table_name,COLUMN_NAME AS name,ORDINAL_POSITION AS ordinal"
            table, condition = "KEY_COLUMN_USAGE", " AND CONSTRAINT_NAME='PRIMARY'"
        else:
            columns = "TABLE_NAME AS table_name,COLUMN_NAME AS name,COLUMN_TYPE AS kind,IS_NULLABLE AS nullable"
            table, condition = "COLUMNS", ""
        return (f"SELECT {columns} FROM information_schema.{table} WHERE TABLE_SCHEMA=DATABASE() "
                f"AND TABLE_NAME IN (%s,%s,%s){condition} ORDER BY TABLE_NAME,ORDINAL_POSITION LIMIT %s",
                (RATE, SPILL, KILL, 129))


def capture_schema(source):
    result = {table: {"columns": {}, "primary_key": []} for table in FIELDS}
    for row in source.fetch(SchemaQuery(False)):
        check(set(row) == {"table_name", "name", "kind", "nullable"}
              and row["table_name"] in FIELDS, "reputation.schema")
        columns = result[row["table_name"]]["columns"]
        name = row["name"].lower()
        check(name not in columns and row["nullable"] in {"YES", "NO"}, "reputation.schema")
        family, _, _, unsigned = sql_type(row["kind"])
        columns[name] = [family, unsigned, row["nullable"] == "YES"]
    for row in source.fetch(SchemaQuery(True)):
        check(set(row) == {"table_name", "name", "ordinal"} and row["table_name"] in FIELDS,
              "reputation.schema")
        keys = result[row["table_name"]]["primary_key"]
        check(row["ordinal"] == len(keys) + 1, "reputation.schema")
        keys.append(row["name"].lower())
    check(result == expected_schema(), "reputation.schema")
    return result


@dataclass(frozen=True)
class Query:
    table: str
    ids: tuple[int, ...]
    limit: int = LIMIT

    def sql(self):
        check(self.table in FIELDS and self.limit == LIMIT, "reputation.query")
        validate_ids(list(self.ids))
        check(bool(self.ids), "reputation.empty_query")
        columns = ",".join((f"CAST(`{n}` AS CHAR)" if floating(n) else f"`{n}`")
                           + f" AS `{n}`" for n in FIELDS[self.table])
        key = FIELDS[self.table][0]
        return (f"SELECT {columns} FROM `{self.table}` WHERE `{key}` IN "
                f"({','.join(['%s'] * len(self.ids))}) ORDER BY `{key}` LIMIT %s",
                self.ids + (LIMIT + 1,))


def validate_ids(ids):
    check(type(ids) is list and len(ids) <= LIMIT, "reputation.selector_limit")
    for value in ids:
        integer(value, 1, 0xffffffff, "reputation.selector")
    check(ids == sorted(set(ids)), "reputation.selector_order")


def normalize(table, rows):
    check(type(rows) is list and len(rows) <= LIMIT, "reputation.row_limit")
    result = []
    for row in rows:
        check(type(row) is dict and set(row) == set(FIELDS[table]), "reputation.row_fields")
        converted = {}
        for name, value in row.items():
            kind, unsigned = domain(table, name)
            column = {"type": kind + (" unsigned" if unsigned else ""), "nullable": False}
            if floating(name) and type(value) is dict:
                check(set(value) == {"decimal"}, "reputation.decimal")
                value = value["decimal"]
            converted[name] = normalize_cell(value, column)
        result.append(converted)
    validate_ids([r[FIELDS[table][0]] for r in result])
    return result


def evidence(bundle, faction_ids, faction_sha256, lookups, policy_schema):
    validate_ids(faction_ids)
    creatures = sorted(r["Entry"] for r in bundle["tables"]["creature_template"])
    validate_ids(creatures)
    return {"format": FORMAT, "faction_ids": faction_ids, "faction_sha256": faction_sha256,
            "creature_ids": creatures, "lookups": lookups, "policy_schema": policy_schema,
            "binding": {"selection_sha256": digest(bundle["selection"]),
                        "schema_sha256": bundle["schema_sha256"],
                        "table_sha256": digest(bundle["table_sha256"]),
                        "db_version_sha256": digest(bundle["db_version"]),
                        "consistency": bundle["consistency"], "lookups_sha256": digest(lookups)}}


def validate(bundle):
    value = bundle["reputation_policy_capture"]
    check(type(value) is dict, "reputation.extension")
    check(canonical(value.get("policy_schema")) == canonical(expected_schema()), "reputation.schema")
    ids = value.get("faction_ids")
    validate_ids(ids)
    sha = value.get("faction_sha256")
    check(type(sha) is str and len(sha) == 64 and all(c in "0123456789abcdef" for c in sha),
          "reputation.faction_hash")
    lookups = value.get("lookups")
    check(type(lookups) is dict and set(lookups) == set(FIELDS), "reputation.lookup_tables")
    creatures = sorted(r["Entry"] for r in bundle["tables"]["creature_template"])
    for table, records in lookups.items():
        selected = creatures if table == KILL else ids
        check(type(records) is list and len(records) == len(selected), "reputation.lookup_coverage")
        for record, entry in zip(records, selected):
            check(type(record) is dict and set(record) == {"id", "row"}
                  and type(record["id"]) is int and record["id"] == entry, "reputation.lookup_selection")
            if record["row"] is not None:
                row = record["row"]
                check(normalize(table, [row]) == [row], "reputation.noncanonical_row")
                check(row[FIELDS[table][0]] == entry, "reputation.row_selection")
    check(canonical(value) == canonical(evidence(bundle, ids, sha, lookups, value["policy_schema"])),
          "reputation.coverage_mismatch")


def capture(source, bundle, faction):
    """Run in the caller's unchanged read-only snapshot; every absence is explicit."""
    policy_schema = capture_schema(source)
    ids = sorted(row.words[0] for row in faction.rows)
    validate_ids(ids)
    creatures = sorted(r["Entry"] for r in bundle["tables"]["creature_template"])
    lookups = {}
    for table in FIELDS:
        selected = creatures if table == KILL else ids
        rows = normalize(table, source.fetch(Query(table, tuple(selected)))) if selected else []
        keyed = {r[FIELDS[table][0]]: r for r in rows}
        check(set(keyed) <= set(selected), "reputation.row_selection")
        lookups[table] = [{"id": entry, "row": keyed.get(entry)} for entry in selected]
    result = dict(bundle)
    result["reputation_policy_capture"] = evidence(bundle, ids, faction.sha256, lookups, policy_schema)
    validate(result)
    return result


def recapture(source, bundle, dbcs, quiescent=False):
    """Extend the same operator-quiescent epoch, as skill_training.recapture does."""
    from .capture import validate_bundle
    validate_bundle(bundle)
    check(quiescent and bundle["consistency"] == "operator_quiescent_snapshot",
          "reputation.unchanged_quiescent_snapshot_required")
    inventory = source.inspect()
    check(inventory == bundle["schema"], "reputation.schema_changed")
    consistency = source.start_read({"MyISAM", "InnoDB"}, quiescent)
    try:
        check(source.version_notes() == bundle["db_version"], "reputation.db_version_changed")
        check(consistency == bundle["consistency"], "reputation.consistency_changed")
        result = capture(source, bundle, dbcs["Faction"])
        check(source.version_notes() == bundle["db_version"], "reputation.db_version_changed")
        validate_bundle(result)
        return result
    finally:
        source.finish_read()
