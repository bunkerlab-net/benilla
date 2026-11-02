"""CMaNGOS 8ec338a1 LootMgr.cpp:125-185,2606-2615 returned-order projection.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import hashlib
from dataclasses import dataclass
from typing import Any

from .core import canonical, check, digest, integer, normalize_cell
from . import loot

FORMAT = "benilla-classic-loot-loader-order-v1"
FIELDS = ("entry", "item", "ChanceOrQuestChance", "groupid", "mincountOrRef", "maxcount", "condition_id")
MAX_SCAN_ROWS = 2_000_000
MAX_SCAN_BYTES = 512 * 1024 * 1024


@dataclass(frozen=True)
class Query:
    table: str

    def sql(self):
        check(self.table in loot.TABLES, "loot.loader_table")
        return f"SELECT {', '.join(FIELDS)} FROM {self.table}", ()


def capture(source, tables, profiles):
    result = {}
    for table in loot.TABLES:
        if table not in profiles:
            continue
        selected = {(r["entry"], r["item"]): r for r in tables.get(table, [])}
        positions = []
        hasher = hashlib.sha256()
        size = count = 0
        for raw in source.scan_loot(Query(table)):
            check(type(raw) is dict and set(raw) == set(FIELDS), "loot.loader_fields")
            row = {name: normalize_cell(raw[name], profiles[table]["columns"][name]) for name in FIELDS}
            count += 1
            check(count <= MAX_SCAN_ROWS, "loot.loader_row_limit")
            encoded = canonical(row)
            size += len(encoded)
            check(size <= MAX_SCAN_BYTES, "loot.loader_byte_limit")
            hasher.update(len(encoded).to_bytes(8, "little"))
            hasher.update(encoded)
            key = (row["entry"], row["item"])
            if key in selected:
                check(all(row[n] == selected[key][n] for n in FIELDS), "loot.loader_projection_mismatch")
                positions.append({"position": count - 1, "key": list(key)})
        check(len(positions) == len(selected) and len({tuple(r["key"]) for r in positions}) == len(selected),
              "loot.loader_selected_coverage")
        result[table] = {"query_sha256": digest(Query(table).sql()), "result_row_count": count,
                         "ordered_result_sha256": hasher.hexdigest(), "selected_rows": positions,
                         "selected_projection_sha256": digest(tables.get(table, []))}
    return {"format": FORMAT, "tables": result}


def validate(bundle):
    extension = bundle.get("loot_loader_capture")
    if extension is None:
        return
    check(type(extension) is dict and set(extension) == {"format", "tables"}
          and extension["format"] == FORMAT and type(extension["tables"]) is dict,
          "loot.loader_extension")
    expected = set(loot.TABLES) & set(bundle["tables"])
    check(set(extension["tables"]) == expected, "loot.loader_tables")
    for table, record in extension["tables"].items():
        check(type(record) is dict and set(record) == {"query_sha256", "result_row_count", "ordered_result_sha256",
              "selected_rows", "selected_projection_sha256"}, "loot.loader_record")
        check(record["query_sha256"] == digest(Query(table).sql()), "loot.loader_query")
        count = integer(record["result_row_count"], 0, MAX_SCAN_ROWS, "loot.loader_count")
        fingerprint = record["ordered_result_sha256"]
        check(type(fingerprint) is str and len(fingerprint) == 64 and fingerprint != "0" * 64
              and all(c in "0123456789abcdef" for c in fingerprint), "loot.loader_fingerprint")
        rows = bundle["tables"][table]
        check(record["selected_projection_sha256"] == digest(rows), "loot.loader_projection_binding")
        positions = record["selected_rows"]
        check(type(positions) is list and len(positions) == len(rows), "loot.loader_positions")
        keys, previous = set(), -1
        for selected in positions:
            check(type(selected) is dict and set(selected) == {"position", "key"}, "loot.loader_position")
            position = integer(selected["position"], 0, max(0, count - 1), "loot.loader_position")
            check(previous < position < count, "loot.loader_position_order")
            previous = position
            key = selected["key"]
            check(type(key) is list and len(key) == 2 and all(type(n) is int for n in key), "loot.loader_key")
            check(tuple(key) not in keys, "loot.loader_duplicate_key")
            keys.add(tuple(key))
        check(keys == {(r["entry"], r["item"]) for r in rows}, "loot.loader_key_coverage")


def ordered_rows(bundle, table):
    validate(bundle)
    extension = bundle.get("loot_loader_capture")
    if extension is None or table not in extension["tables"]:
        return None
    indexed = {(r["entry"], r["item"]): r for r in bundle["tables"][table]}
    return [(r["position"], indexed[tuple(r["key"])]) for r in extension["tables"][table]["selected_rows"]]
