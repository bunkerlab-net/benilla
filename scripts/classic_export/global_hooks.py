"""Bounded raw hook cursors and declared pinned registration configuration.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from dataclasses import dataclass
import json
from pathlib import Path
from .core import canonical, check, digest, integer, normalize_cell, sql_type

CONTRACT = json.loads(Path(__file__).with_name("global_hooks_contract.json").read_text())
TABLES = CONTRACT["tables"]
LIMIT = CONTRACT["row_limit"]
FORMAT = "benilla-classic-global-hooks-v1"


@dataclass(frozen=True)
class Query:
    table: str
    ids: tuple[int, ...] = ()
    limit: int = LIMIT

    def sql(self):
        check(self.table in TABLES and self.limit == LIMIT, "global_hooks.query")
        spec = TABLES[self.table]
        columns = ",".join((f"CAST(`{n}` AS CHAR)" if c["type"] == "float" else f"`{n}`")
                           + f" AS `{n}`" for n, c in spec["columns"].items())
        where, params = "", ()
        if spec["scope"] == "lookup":
            check(bool(self.ids) and len(self.ids) <= LIMIT
                  and list(self.ids) == sorted(set(self.ids)), "global_hooks.lookup_selection")
            for value in self.ids:
                integer(value, 1, 0xffffffff, "global_hooks.lookup_selection")
            where = " WHERE `Id` IN (" + ",".join(["%s"] * len(self.ids)) + ")"
            params = self.ids
        else:
            check(not self.ids, "global_hooks.global_query")
        # SpellMgr's unordered query is replayable as this exact retained cursor,
        # not represented as an observation of a separately running server.
        return f"SELECT {columns} FROM `{self.table}`{where} LIMIT %s", params + (LIMIT + 1,)


@dataclass(frozen=True)
class SchemaQuery:
    keys: bool
    limit: int = 4096

    def sql(self):
        check(type(self.keys) is bool and self.limit == LIMIT, "global_hooks.schema_query")
        names = tuple(TABLES)
        if self.keys:
            fields = "TABLE_NAME AS table_name,COLUMN_NAME AS name,ORDINAL_POSITION AS ordinal"
            table, condition = "KEY_COLUMN_USAGE", " AND CONSTRAINT_NAME='PRIMARY'"
        else:
            fields = "TABLE_NAME AS table_name,COLUMN_NAME AS name,COLUMN_TYPE AS kind,IS_NULLABLE AS nullable"
            table, condition = "COLUMNS", ""
        return (f"SELECT {fields} FROM information_schema.{table} WHERE TABLE_SCHEMA=DATABASE() "
                f"AND TABLE_NAME IN ({','.join(['%s'] * len(names))}){condition} "
                "ORDER BY TABLE_NAME,ORDINAL_POSITION LIMIT %s", names + (LIMIT + 1,))


def expected_schema():
    return {t: {"columns": {n.lower(): [sql_type(c["type"])[0], sql_type(c["type"])[3], c["nullable"]]
                            for n, c in s["columns"].items()}, "primary_key": [k.lower() for k in s["keys"]]}
            for t, s in TABLES.items()}


def capture_schema(source):
    result = {t: {"columns": {}, "primary_key": []} for t in TABLES}
    for row in source.fetch(SchemaQuery(False)):
        table, name = row["table_name"], row["name"].lower()
        check(table in TABLES, "global_hooks.schema")
        wanted = {n.lower() for n in TABLES[table]["columns"]}
        if name not in wanted:
            continue
        check(name not in result[table]["columns"] and row["nullable"] in {"YES", "NO"}, "global_hooks.schema")
        kind = sql_type(row["kind"])
        result[table]["columns"][name] = [kind[0], kind[3], row["nullable"] == "YES"]
    for row in source.fetch(SchemaQuery(True)):
        table = row["table_name"]
        check(table in TABLES and row["ordinal"] == len(result[table]["primary_key"]) + 1, "global_hooks.schema")
        result[table]["primary_key"].append(row["name"].lower())
    check(result == expected_schema(), "global_hooks.schema")
    return result


def normalize(table, rows):
    check(type(rows) is list and len(rows) <= LIMIT, "global_hooks.row_limit")
    spec = TABLES[table]
    result, seen = [], set()
    for row in rows:
        check(type(row) is dict and set(row) == set(spec["columns"]), "global_hooks.row_fields")
        converted = {}
        for name, column in spec["columns"].items():
            value = row[name]
            if type(value) is dict:
                check(set(value) == {"decimal"}, "global_hooks.decimal")
                value = value["decimal"]
            converted[name] = normalize_cell(value, column)
        key = tuple(converted[n] for n in spec["keys"])
        check(key not in seen, "global_hooks.duplicate_row")
        seen.add(key)
        result.append(converted)
    return result


def numeric_ids(bundle, rows):
    ids = {r["Id"] for r in bundle["tables"]["spell_template"]}
    ids.update(r[f"spellid_{i}"] for r in bundle["tables"]["item_template"] for i in range(1, 6))
    for table, keys in {"spell_area": ["spell", "aura_spell"], "spell_chain": ["spell_id", "prev_spell", "first_spell", "req_spell"],
                        "spell_proc_event": ["entry"], "spell_affect": ["entry"], "spell_threat": ["entry"],
                        "spell_elixir": ["entry"], "spell_group_spell": ["SpellId"]}.items():
        ids.update(abs(r[k]) for r in rows[table] for k in keys)
    ids.discard(0)
    result = sorted(ids)
    check(len(result) <= LIMIT, "global_hooks.lookup_limit")
    for value in result:
        integer(value, 1, 0xffffffff, "global_hooks.lookup_selection")
    return result


def evidence(bundle, rows, schema, configuration):
    return {"format": FORMAT, "contract_sha256": digest(CONTRACT), "configuration": configuration,
            "registration_manifest_sha256": digest(CONTRACT["manifest"]), "schema": schema,
            "rows": rows, "spell_ids": numeric_ids(bundle, rows),
            "cursor_semantics": "captured_select_cursor_not_observed_reference_loader",
            "binding": {"selection_sha256": digest(bundle["selection"]), "schema_sha256": bundle["schema_sha256"],
                        "table_sha256": digest(bundle["table_sha256"]), "db_version_sha256": digest(bundle["db_version"]),
                        "consistency": bundle["consistency"], "rows_sha256": digest(rows)}}


def validate(bundle):
    value = bundle["global_hook_capture"]
    check(type(value) is dict, "global_hooks.extension")
    config = value.get("configuration")
    check(type(config) is dict and set(config) == set(CONTRACT["manifest"]["configuration"])
          and all(type(v) is bool for v in config.values()), "global_hooks.configuration")
    check(value.get("schema") == expected_schema(), "global_hooks.schema")
    rows = value.get("rows")
    check(type(rows) is dict and set(rows) == set(TABLES), "global_hooks.tables")
    for table, records in rows.items():
        check(normalize(table, records) == records, "global_hooks.noncanonical_row")
    selected = numeric_ids(bundle, rows)
    present = {r["Id"] for r in rows["spell_template"]}
    check(present <= set(selected), "global_hooks.lookup_selection")
    # Row-or-null lookup coverage is the selector plus the complete matching cursor.
    # Bind overlapping projections to the original source, not another SQL epoch.
    for table, key in (("spell_template", "Id"), ("spell_area", "spell"), ("spell_proc_event", "entry"),
                       ("spell_chain", "spell_id"), ("spell_scripts", "Id")):
        names = TABLES[table]["columns"]
        original = (bundle["spell_numeric_capture"]["rows"] if table == "spell_template" and "spell_numeric_capture" in bundle
                    else bundle["tables"].get(table, []))
        for row in original:
            projection = {n: row[n] for n in names if n in row}
            check(any(all(record[n] == v for n, v in projection.items()) for record in rows[table]),
                  "global_hooks.overlapping_source_changed")
        if table != "spell_template" and table in bundle["tables"]:
            selected_spells = {r["Id"] for r in bundle["tables"]["spell_template"]}
            before = sorted((r for r in bundle["tables"][table] if r[key] in selected_spells), key=canonical)
            after = sorted((r for r in rows[table] if r[key] in selected_spells), key=canonical)
            check(before == after, "global_hooks.overlapping_source_changed")
    check(canonical(value) == canonical(evidence(bundle, rows, value["schema"], config)), "global_hooks.coverage_mismatch")


def recapture(source, bundle, configuration, quiescent=False):
    from .capture import validate_bundle
    validate_bundle(bundle)
    check(quiescent and bundle["consistency"] == "operator_quiescent_snapshot", "global_hooks.unchanged_snapshot_required")
    check(source.inspect() == bundle["schema"], "global_hooks.source_schema_changed")
    consistency = source.start_read({"MyISAM", "InnoDB"}, quiescent)
    try:
        check(consistency == bundle["consistency"] and source.version_notes() == bundle["db_version"], "global_hooks.source_epoch_changed")
        schema = capture_schema(source)
        rows = {t: normalize(t, source.fetch(Query(t))) for t in TABLES if t != "spell_template"}
        rows["spell_template"] = normalize("spell_template", source.fetch(Query("spell_template", tuple(numeric_ids(bundle, rows)))))
        result = dict(bundle)
        result["global_hook_capture"] = evidence(bundle, rows, schema, configuration)
        check(source.version_notes() == bundle["db_version"], "global_hooks.source_epoch_changed")
        validate_bundle(result)
        return result
    finally:
        source.finish_read()
