"""Bounded selected body-loot dependencies, never loader order or generation.

Pinned LootMgr.cpp:135 reads whole tables without ORDER BY; this selected,
key-ordered projection cannot supply its accepted-row positions. Negative
mincountOrRef selects the reference namespace (325-365), not an item prototype.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

from decimal import Decimal
from typing import Any

from .core import (CORE_REVISION, DATABASE_REVISION, KEYS, MAX_ITEMS, MAX_ROWS,
                   MAX_TABLE_BYTES, canonical, check, digest, integer)

FORMAT = "benilla-classic-body-loot-selection-v1"
SCOPE = "selected_body_loot_dependencies_only"
MODE = "body_loot_selectors"
TABLES = ("creature_loot_template", "reference_loot_template")
BODY, REFERENCE = TABLES


def creatures(tables: dict[str, Any]) -> list[dict[str, int]]:
    fields = ("Entry", "LootId", "PickpocketLootId", "SkinningLootId", "MinLootGold", "MaxLootGold")
    result = [{name: row[name] for name in fields} for row in tables["creature_template"]]
    for row in result:
        for name in fields:
            integer(row[name], 1 if name == "Entry" else 0, 0xffffff, "loot.creature_field_domain")
    return result


def roots(tables: dict[str, Any]) -> set[int]:
    return {row["LootId"] for row in creatures(tables) if row["LootId"]}


def reference_ids(rows: list[dict[str, Any]]) -> set[int]:
    return {-row["mincountOrRef"] for row in rows if row["mincountOrRef"] < 0}


def leaf_ids(tables: dict[str, Any]) -> set[int]:
    ids = {row["item"] for table in TABLES for row in tables.get(table, [])
           if row["mincountOrRef"] > 0 and row["item"] > 0}
    check(len(ids) <= MAX_ITEMS, "loot.item_limit")
    return ids


def capture_tables(tables: dict[str, Any], profiles: dict[str, Any], get) -> None:
    """Use the existing snapshot/query/accounting reader, never a new SQL path."""
    if BODY in profiles:
        selected = roots(tables)
        tables[BODY] = get(BODY, selected, mode=MODE)
        check(all(row["entry"] in selected for row in tables[BODY]), "loot.unselected_row")
    if REFERENCE not in profiles:
        return
    tables[REFERENCE] = []
    pending = reference_ids(tables.get(BODY, []))
    visited: set[int] = set()
    retained_bytes = 0
    while pending:
        check(len(visited | pending) <= MAX_ROWS, "loot.reference_id_limit")
        rows = get(REFERENCE, pending, mode=MODE)
        check(all(row["entry"] in pending for row in rows), "loot.unselected_row")
        tables[REFERENCE].extend(rows)
        check(len(tables[REFERENCE]) <= MAX_ROWS, "loot.accumulated_row_limit")
        retained_bytes += sum(len(canonical(row)) for row in rows)
        check(retained_bytes <= MAX_TABLE_BYTES, "loot.accumulated_table_byte_limit")
        visited.update(pending)  # Missing IDs are visited too; never retry forever.
        pending = reference_ids(rows) - visited
    tables[REFERENCE].sort(key=lambda row: (row["entry"], row["item"]))


def _cycles(ids: set[int], rows: list[dict[str, Any]]) -> list[list[int]]:
    """Iterative strongly connected components: bounded O(vertices + edges)."""
    graph = {key: set() for key in ids}
    reverse = {key: set() for key in ids}
    for row in rows:
        if row["mincountOrRef"] < 0:
            source, target = row["entry"], -row["mincountOrRef"]
            graph[source].add(target)
            reverse[target].add(source)
    seen, order = set(), []
    for start in sorted(ids):
        if start in seen:
            continue
        seen.add(start)
        stack = [(start, iter(sorted(graph[start])))]
        while stack:
            current, children = stack[-1]
            child = next(children, None)
            if child is None:
                order.append(current)
                stack.pop()
            elif child not in seen:
                seen.add(child)
                stack.append((child, iter(sorted(graph[child]))))
    seen, cycles = set(), []
    for start in reversed(order):
        if start in seen:
            continue
        component, pending = [], [start]
        seen.add(start)
        while pending:
            current = pending.pop()
            component.append(current)
            for child in sorted(reverse[current]):
                if child not in seen:
                    seen.add(child)
                    pending.append(child)
        if len(component) > 1 or start in graph[start]:
            cycles.append(sorted(component))
    return sorted(cycles)


def _record(table: str, ids: set[int], rows: list[dict[str, Any]], present: bool) -> dict[str, Any]:
    from .capture import Query
    ordered = sorted(ids)
    missing = sorted(ids - {row["entry"] for row in rows}) if present else None
    return {"requested_ids": ordered, "mode": MODE, "selector_column": "entry",
            "row_limit": MAX_ROWS, "query_performed": present and bool(ids),
            "query_sha256": digest(Query(table, MODE, tuple(ordered)).sql()) if present and ids else None,
            "returned_keys": [[row[name] for name in KEYS[table]] for row in rows] if present else None,
            "rows_sha256": digest(rows) if present else None, "missing_ids": missing,
            "status": "unknown" if not present else "not_needed" if not ids else "missing" if missing else "complete"}


def derived(tables: dict[str, Any]) -> dict[str, Any]:
    """Reconstruct selector rounds from source rows, not supplied evidence/IDs."""
    selected = roots(tables)
    check(len(selected) <= MAX_ROWS, "loot.root_id_limit")
    body = tables.get(BODY, [])
    check(all(row["entry"] in selected for row in body), "loot.unselected_row")
    records = {BODY: [_record(BODY, selected, body, BODY in tables)], REFERENCE: []}
    indexed: dict[int, list[dict[str, Any]]] = {}
    for row in tables.get(REFERENCE, []):
        indexed.setdefault(row["entry"], []).append(row)
    requested = reference_ids(body)
    visited: set[int] = set()
    while requested - visited:
        check(len(requested) <= MAX_ROWS, "loot.reference_id_limit")
        batch = requested - visited
        rows = sorted((row for key in batch for row in indexed.get(key, [])),
                      key=lambda row: (row["entry"], row["item"]))
        records[REFERENCE].append(_record(REFERENCE, batch, rows, REFERENCE in tables))
        visited.update(batch)
        requested.update(reference_ids(rows))
    if not records[REFERENCE]:
        records[REFERENCE].append(_record(REFERENCE, set(), [], REFERENCE in tables))
    check(set(indexed) <= requested, "loot.unselected_row")
    captured = {}
    for table, queries in records.items():
        missing = sorted({key for query in queries for key in query["missing_ids"] or []})
        ids = sorted({key for query in queries for key in query["requested_ids"]})
        captured[table] = {"requested_ids": ids, "queries": queries,
                           "missing_ids": missing if table in tables else None,
                           "status": "unknown" if table not in tables else "not_needed" if not ids
                           else "missing" if missing else "complete"}
    edges = [{"table": table, "entry": row["entry"], "item": row["item"],
              "raw_mincountOrRef": row["mincountOrRef"], "target_id": -row["mincountOrRef"],
              **{name: row[name] for name in ("ChanceOrQuestChance", "groupid", "maxcount", "condition_id")}}
             for table in TABLES for row in tables.get(table, []) if row["mincountOrRef"] < 0]
    return {"root_ids": sorted(selected), "reference_ids": sorted(requested),
            "reference_edges": edges, "leaf_item_ids": sorted(leaf_ids(tables)),
            "cyclic_reference_components": _cycles(requested, tables.get(REFERENCE, [])),
            "tables": captured}


def quest_leaf(row: dict[str, Any], table: str) -> bool:
    """Bounded body leaf only; LootStore::LoadLootTable/IsValidItemTemplate at the pin."""
    chance = Decimal(row["ChanceOrQuestChance"]["decimal"])
    return (table == BODY and chance < 0 and chance.copy_abs() >= Decimal("0.000001")
            and row["condition_id"] == 0 and row["groupid"] == 0 and row["item"] > 0
            and 0 < row["mincountOrRef"] <= row["maxcount"] <= 255)


def quest_inputs(tables: dict[str, Any]) -> list[dict[str, Any]]:
    """Interpreted rows still need whole-loader, item, solo-owner and explicit rate evidence."""
    return [{"entry": row["entry"], "item": row["item"],
             "quest_only": True,
             "chance": str(Decimal(row["ChanceOrQuestChance"]["decimal"]).copy_abs()),
             "minimum_count": row["mincountOrRef"], "maximum_count": row["maxcount"],
             "rate_policy": "explicit_CONFIG_FLOAT_RATE_DROP_ITEM_QUEST",
             "owner_policy": "active_incomplete_ReqItem_below_required_count",
             "runtime_admitted": False}
            for row in tables.get(BODY, []) if quest_leaf(row, BODY)]


def unsupported(tables: dict[str, Any], graph: dict[str, Any]) -> list[dict[str, Any]]:
    findings = []
    def add(table, key, reason):
        findings.append({"table": table, "key": str(key), "status": "blocked", "reason": reason})
    add(BODY, "scope", "loot.source_loader_order_unproven")
    add(BODY, "scope", "loot.generation_and_item_admission_not_established")
    for table, record in graph["tables"].items():
        if record["status"] == "unknown":
            add(table, "schema", "loot.table_missing_dependency_unknown")
        for key in record["missing_ids"] or []:
            add(table, key, "loot.selected_template_missing")
    for creature in creatures(tables):
        for field in ("PickpocketLootId", "SkinningLootId"):
            if creature[field]:
                add("creature_template", creature["Entry"], f"loot.{field}_outside_body_scope")
    for component in graph["cyclic_reference_components"]:
        for key in component:
            add(REFERENCE, key, "loot.reference_cycle_not_executable")
    for table in TABLES:
        for row in tables.get(table, []):
            key = f"{row['entry']}:{row['item']}"
            chance = Decimal(row["ChanceOrQuestChance"]["decimal"])
            reasons = []
            if row["condition_id"]:
                reasons.append("condition_not_resolved")
            if row["condition_id"] > 0xffff:
                reasons.append("condition_uint16_conversion_unproven")
            if chance < 0 and not quest_leaf(row, table):
                reasons.append("quest_leaf_shape_unsupported")
            if not row["mincountOrRef"]:
                reasons.append("zero_mincount_unsupported")
            if not row["maxcount"] or row["mincountOrRef"] > row["maxcount"]:
                reasons.append("count_shape_unsupported")
            if row["groupid"] >= 128:
                reasons.append("group_domain_unsupported")
            if not chance and not row["groupid"]:
                reasons.append("ungrouped_zero_chance_unsupported")
            if chance and chance.copy_abs() < Decimal("0.000001"):
                reasons.append("small_chance_loader_conversion_unproven")
            if row["mincountOrRef"] > 0 and not row["item"]:
                reasons.append("zero_leaf_item_unsupported")
            if row["mincountOrRef"] < 0:
                reasons.append("reference_generation_not_implemented")
                if table == REFERENCE:
                    reasons.append("nested_reference_not_executable")
                if row["groupid"]:
                    reasons.append("grouped_reference_not_executable")
            for reason in reasons:
                add(table, key, "loot." + reason)
    return sorted(findings, key=lambda row: (row["table"], row["key"], row["reason"]))


def evidence(bundle: dict[str, Any]) -> dict[str, Any]:
    tables = bundle["tables"]
    graph = derived(tables)
    selected_creatures = creatures(tables)
    nonbody = [{"creature_entry": row["Entry"], "field": field, "loot_id": row[field],
                "status": "unknown", "reason": "outside_body_scope"}
               for row in selected_creatures for field in ("PickpocketLootId", "SkinningLootId") if row[field]]
    return {"format": FORMAT, "scope": SCOPE, "requested_mode": MODE,
            "selected_rows_complete": all(record["status"] in {"complete", "not_needed"}
                                          for record in graph["tables"].values()),
            "query_order": "entry_item_key_order", "loader_order": "unproven",
            "creatures": selected_creatures, "non_body_dependencies": nonbody, **graph,
            "unsupported": unsupported(tables, graph),
            "snapshot": {"consistency": bundle["consistency"],
                         "core_revision": CORE_REVISION, "database_revision": DATABASE_REVISION,
                         "revision_claims_verified_against_installation": False,
                         "selection_sha256": digest(bundle["selection"]),
                         "schema_sha256": bundle["schema_sha256"],
                         "db_version_sha256": digest(bundle["db_version"]),
                         "table_sha256": {table: digest(tables[table]) for table in
                                          ("creature", "creature_template", *TABLES, "item_template", "spell_template")
                                          if table in tables}}}


def validate(bundle: dict[str, Any], profiles: dict[str, Any]) -> None:
    tables = bundle["tables"]
    for table in TABLES:
        check((table in profiles) == (table in tables), "loot.table_coverage_required")
    # Unlike required_ids, leaf identities are recomputed from actual loot rows.
    check(leaf_ids(tables) <= {row["entry"] for row in tables["item_template"]}, "loot.leaf_item_missing")
    check(canonical(bundle["loot_capture"]) == canonical(evidence(bundle)), "loot.coverage_mismatch")


def blockers(tables: dict[str, Any], requested: bool) -> list[dict[str, Any]]:
    return unsupported(tables, derived(tables)) if requested else []
