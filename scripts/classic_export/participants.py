"""Conservative non-render participant acquisition. No runtime admission.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1 ObjectMgr.cpp:2081-2331, ScriptMgr.cpp:45-55,117-159.
"""
from dataclasses import dataclass
from decimal import Decimal
import hashlib
from .participants_columns import SCHEMA, KEYS, COLUMNS
from .core import canonical, check, digest, integer, normalize_cell, sql_type, MAX_ROWS

FORMAT = "benilla-classic-participants-v1"
MAX_SCAN_ROWS = 2_000_000
MAX_SCAN_BYTES = 512 * 1024 * 1024
SCRIPT_TABLES = tuple(t for t in SCHEMA if t.startswith("dbscripts_"))

@dataclass(frozen=True)
class Query:
    table: str

    def sql(self):
        check(self.table in SCHEMA, "participants.query_table")
        order = " ORDER BY `priority`" if self.table in SCRIPT_TABLES else ""
        return ("SELECT " + ",".join(f"`{n}`" for n in COLUMNS[self.table])
                + f" FROM `{self.table}`" + order, ())

def schema(inventory):
    for table, expected in SCHEMA.items():
        meta = inventory["tables"].get(table)
        if meta is None:
            continue
        check(meta.get("primary_key") == list(KEYS[table]), f"participants.primary_key:{table}")
        actual = meta["columns"]
        check(len(actual) == len(expected), f"participants.columns:{table}")
        for ordinal, (col, (name, kind, nullable)) in enumerate(zip(actual, expected), 1):
            observed, wanted = sql_type(col["type"]), sql_type(kind)
            if wanted[0] in {"int", "mediumint", "smallint", "tinyint"}:
                observed, wanted = (observed[0], observed[3]), (wanted[0], wanted[3])
            check(col["name"] == name and col["ordinal"] == ordinal
                  and col["nullable"] == nullable and observed == wanted,
                  f"participants.column:{table}:{name}")

def roots(bundle):
    return {"selection": bundle["selection"],
            "creature_guids": [r["guid"] for r in bundle["tables"]["creature"]],
            "creature_entries": sorted({r["Entry"] for r in bundle["tables"]["creature_template"]}),
            "quest_ids": sorted(r["entry"] for r in bundle["tables"]["quest_template"]),
            "spell_ids": sorted(r["Id"] for r in bundle["tables"].get("spell_template", []))}

def inside(row, selection):
    return all(Decimal(low)/1000 <= Decimal(row[f"position_{axis}"]["decimal"]) <= Decimal(high)/1000
               for axis, low, high in zip("xyz", selection["min_milliunits"], selection["max_milliunits"]))

def keep(table, row, raw, selection):
    if table == "gameobject":
        return row["map"] == selection["map_id"] and inside(row, selection)
    guids = {r["guid"] for r in raw.get("gameobject", [])}
    entries = {r["id"] for r in raw.get("gameobject", [])} | {r["entry"] for r in raw.get("gameobject_spawn_entry", [])}
    entries |= {r["entry"] for r in raw.get("transports", [])}
    if table == "gameobject_template":
        return row["entry"] in entries
    if table == "pool_gameobject_template":
        return row["id"] in entries
    if table in ("gameobject_addon", "gameobject_spawn_entry", "game_event_gameobject", "pool_gameobject"):
        return row["guid"] in guids
    # No script dispatch or TaxiPathNode exclusion is inferred. All transports,
    # summons and non-text script commands remain conservative candidates.
    if table in SCRIPT_TABLES:
        return row["command"] not in (0, 1)
    return True

def order_tables():
    return ("gameobject", "gameobject_spawn_entry", "transports", "gameobject_template",
            "gameobject_addon", "game_event_gameobject", "pool_gameobject",
            "pool_gameobject_template", "creature_ai_summons", *SCRIPT_TABLES)

def normalize(table, row, inventory):
    check(type(row) is dict and set(row) == set(COLUMNS[table]), "participants.row_fields")
    columns = {c["name"]: c for c in inventory["tables"][table]["columns"]}
    return {n: normalize_cell(v["decimal"] if type(v) is dict and set(v) == {"decimal"} else v,
                              columns[n]) for n, v in row.items()}

def capture(source, selection, inventory):
    schema(inventory)
    raw, observations = {}, {}
    for table in order_tables():
        if table not in inventory["tables"]:
            observations[table] = {"status": "unknown"}
            continue
        selected, positions, seen = [], [], set()
        hasher = hashlib.sha256()
        size = count = map_rows = 0
        for item in source.scan_participants(Query(table)):
            row = normalize(table, item, inventory)
            encoded = canonical(row)
            count += 1
            size += len(encoded)
            check(count <= MAX_SCAN_ROWS, "participants.scan_row_limit")
            check(size <= MAX_SCAN_BYTES, "participants.scan_byte_limit")
            if KEYS[table]:
                key = tuple(row[n] for n in KEYS[table])
                check(key not in seen, "participants.duplicate_key")
                seen.add(key)
            hasher.update(len(encoded).to_bytes(8, "little"))
            hasher.update(encoded)
            if table == "gameobject" and row["map"] == selection["map_id"]:
                map_rows += 1
            if keep(table, row, raw, selection):
                check(len(selected) < MAX_ROWS, "participants.selected_row_limit")
                selected.append(row)
                positions.append(count - 1)
        raw[table] = sorted(selected, key=canonical)
        observations[table] = {"status": "observed", "query_sha256": digest(Query(table).sql()),
            "result_row_count": count, "ordered_result_sha256": hasher.hexdigest(),
            "selected_positions": positions, "selected_sha256": digest(raw[table]),
            "selected_map_rows": map_rows if table == "gameobject" else None}
    check(len(canonical(raw)) <= 16 * 1024 * 1024, "participants.byte_limit")
    return raw, observations

def evidence(bundle, raw, observations):
    return {"format": FORMAT, "scope": "conservative_candidates_no_runtime_or_exterior_exclusion",
        "roots": roots(bundle), "rows": raw, "observations": observations,
        "snapshot": {k: bundle[k] for k in ("core_revision", "database_revision", "schema_sha256", "consistency")}
            | {"db_version_sha256": digest(bundle["db_version"]), "tables_sha256": digest(bundle["table_sha256"]),
               "rows_sha256": digest(raw), "observations_sha256": digest(observations)}}

def validate(bundle):
    value = bundle["participants_capture"]
    check(type(value) is dict and type(value.get("rows")) is dict
          and type(value.get("observations")) is dict, "participants.fields")
    schema(bundle["schema"])
    raw, observations = value["rows"], value["observations"]
    check(set(observations) == set(SCHEMA), "participants.tables_required")
    available = set(SCHEMA) & set(bundle["schema"]["tables"])
    check(set(raw) == available, "participants.tables_required")
    for table in order_tables():
        observation = observations[table]
        if table not in available:
            check(observation == {"status": "unknown"}, "participants.absent_table_unknown")
            continue
        rows = raw[table]
        check(type(rows) is list and len(rows) <= MAX_ROWS, "participants.selected_row_limit")
        check(rows == sorted([normalize(table, r, bundle["schema"]) for r in rows], key=canonical),
              "participants.row_order_or_shape")
        if KEYS[table]:
            check(len({tuple(r[n] for n in KEYS[table]) for r in rows}) == len(rows), "participants.duplicate_key")
        check(all(keep(table, r, raw, bundle["selection"]) for r in rows), "participants.unselected_row")
        check(type(observation) is dict and set(observation) == {"status", "query_sha256", "result_row_count",
              "ordered_result_sha256", "selected_positions", "selected_sha256", "selected_map_rows"},
              "participants.observation_fields")
        check(observation["status"] == "observed" and observation["query_sha256"] == digest(Query(table).sql()),
              "participants.loader_query")
        count = integer(observation["result_row_count"], 0, MAX_SCAN_ROWS, "participants.loader_count")
        fingerprint = observation["ordered_result_sha256"]
        check(type(fingerprint) is str and len(fingerprint) == 64 and fingerprint != "0" * 64
              and all(c in "0123456789abcdef" for c in fingerprint), "participants.loader_fingerprint")
        positions = observation["selected_positions"]
        check(type(positions) is list and len(positions) == len(rows), "participants.loader_positions")
        for p in positions:
            integer(p, 0, max(0, count - 1), "participants.loader_position")
        check(positions == sorted(set(positions)) and all(p < count for p in positions), "participants.loader_positions")
        check(observation["selected_sha256"] == digest(rows), "participants.projection_binding")
        if table == "gameobject":
            integer(observation["selected_map_rows"], len(rows), count, "participants.map_count")
        else:
            check(observation["selected_map_rows"] is None, "participants.map_count")
    check(value == evidence(bundle, raw, observations), "participants.coverage_mismatch")
    check(len(canonical(raw)) <= 16 * 1024 * 1024, "participants.byte_limit")
