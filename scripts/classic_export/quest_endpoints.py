"""Complete creature relations and exact endpoint spawn evidence.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
ObjectMgr.cpp:7940-8035. Selection limits availability, not relation loading.
"""
from decimal import Decimal
from .core import MAX_ROWS, canonical, check, digest, normalize_rows, schema_profile

FORMAT = "benilla-classic-quest-endpoints-v1"
MODE = "quest_endpoint_spawns"
TABLES = ("creature_questrelation", "creature_involvedrelation")


def inside(selection, row):
    return (row["map"] == selection["map_id"] and all(
        Decimal(low) <= Decimal(row[field]["decimal"]) * 1000 <= Decimal(high)
        for field, low, high in zip(("position_x", "position_y", "position_z"),
                                    selection["min_milliunits"], selection["max_milliunits"])))


def evidence(bundle, raw):
    check(type(raw) is dict and set(raw) == {*TABLES, "creature"}, "quest_endpoints.tables")
    profiles = schema_profile(bundle["schema"], {*TABLES, "creature"})
    for table, rows in raw.items():
        check(type(rows) is list and len(rows) <= MAX_ROWS and all(type(r) is dict for r in rows),
              "quest_endpoints.row_shape_or_order")
        decoded = [{k: (v["decimal"] if type(v) is dict and set(v) == {"decimal"} else v)
                    for k, v in row.items()} for row in rows]
        check(normalize_rows(table, decoded, profiles[table], MAX_ROWS) == rows,
              "quest_endpoints.row_shape_or_order")
    for table in TABLES:
        check(raw[table] == bundle["tables"][table], "quest_endpoints.relation_coverage_mismatch")
    entries = sorted({r["id"] for table in TABLES for r in raw[table]})
    check(all(r["id"] in entries for r in raw["creature"]), "quest_endpoints.unselected_spawn")
    selected = [r for r in raw["creature"] if inside(bundle["selection"], r)]
    check(selected == [r for r in bundle["tables"]["creature"] if r["id"] in entries],
          "quest_endpoints.selected_spawn_coverage_mismatch")
    endpoints = []
    for table in TABLES:
        for relation in raw[table]:
            spawns = [r for r in raw["creature"] if r["id"] == relation["id"]]
            placement = ("inside_selection" if any(inside(bundle["selection"], r) for r in spawns)
                         else "outside_selection" if spawns else "unspawned")
            endpoints.append({"table": table, **relation, "placement": placement,
                              "spawn_guids": [r["guid"] for r in spawns]})
    return {"format": FORMAT, "status": "complete_raw_projection",
            "selectors": {"quests": sorted(r["entry"] for r in bundle["tables"]["quest_template"]),
                          "entries": entries}, "rows": raw, "rows_sha256": digest(raw),
            "endpoints": endpoints,
            "snapshot": {k: bundle[k] for k in ("core_revision", "database_revision", "schema_sha256", "consistency")}
            | {"selection_sha256": digest(bundle["selection"]), "db_version_sha256": digest(bundle["db_version"]),
               "tables_sha256": digest(bundle["table_sha256"])}}


def validate(bundle):
    value = bundle["quest_endpoint_capture"]
    check(type(value) is dict and type(value.get("rows")) is dict, "quest_endpoints.fields")
    check(canonical(value) == canonical(evidence(bundle, value["rows"])), "quest_endpoints.binding_mismatch")


def proved(bundle, table, quest):
    value = bundle.get("quest_endpoint_capture")
    if value is None:
        return False
    relations = [r for r in value["endpoints"] if r["table"] == table and r["quest"] == quest]
    return bool(relations) and all(r["placement"] == "outside_selection" for r in relations)


def discharges(bundle, issue):
    return (issue["reason"] == "quest.relation_missing_or_outside_selection"
            and issue["table"] in TABLES and proved(bundle, issue["table"], int(issue["key"])))
