"""Raw positive ACTION_T_TEXT dependencies, never EventAI execution or admission.

Pinned CreatureEventAIMgr.cpp:594-614 inspects signed TextId parameters in all
three action slots. Preserve negative values as unresolved, not creature_ai_texts
or a silent action. ObjectMgr.cpp:7782-7880 loads base broadcasts and locales.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from typing import Any

from . import event_ai
from .columns import COLUMNS
from .core import MAX_ROWS, KEYS, canonical, check, digest, integer, schema_profile

FORMAT = "benilla-classic-event-text-selection-v1"
TABLES = ("broadcast_text", "broadcast_text_locale")
MODE = "event_text_selectors"
SCOPE = "positive_action_t_text_broadcast_dependencies_only"


def references(tables: dict[str, Any]) -> tuple[list[dict[str, int]], list[dict[str, int]], list[dict[str, int]]]:
    positive, negative, unsupported = [], [], []
    for row in tables[event_ai.TABLE]:
        for action in range(1, 4):
            kind = row[f"action{action}_type"]
            origin = {"event_id": row["id"], "creature_id": row["creature_id"], "action_index": action}
            if kind not in (0, 1):
                unsupported.append({**origin, "action_type": kind})
            if kind != 1:
                continue
            for parameter in range(1, 4):
                value = integer(row[f"action{action}_param{parameter}"], -0x80000000, 0x7fffffff,
                                "event_text.parameter_domain")
                if value:
                    reference = {**origin, "parameter_index": parameter, "broadcast_id": value}
                    (positive if value > 0 else negative).append(reference)
    return positive, negative, unsupported


def selected_ids(tables: dict[str, Any]) -> list[int]:
    ids = sorted({r["broadcast_id"] for r in references(tables)[0]})
    check(len(ids) <= MAX_ROWS, "event_text.selector_limit")
    return ids


def evidence(bundle: dict[str, Any], profiles: dict[str, Any] | None = None) -> dict[str, Any]:
    from .capture import Query
    tables = bundle["tables"]
    profiles = profiles if profiles is not None else schema_profile(
        bundle["schema"], set(tables) | {"db_version"})
    ids = selected_ids(tables)
    positive, negative, unsupported = references(tables)
    captured = {}
    for table in TABLES:
        record = {"requested_ids": ids, "selector_column": "Id", "mode": MODE,
                  "row_limit": MAX_ROWS, "query_performed": False,
                  "source_columns": None, "query_sha256": None, "returned_keys": None,
                  "rows_sha256": None, "missing_ids": None, "status": "unknown"}
        if table in tables:
            names = tuple(profiles[table]["source_names"][name] for name in COLUMNS[table])
            query = Query(table, MODE, tuple(ids), source_names=names)
            rows = tables[table]
            present = {r["Id"] for r in rows}
            missing = sorted(set(ids) - present) if table == "broadcast_text" else []
            record.update(source_columns=dict(zip(COLUMNS[table], names)),
                          query_sha256=digest(query.sql()) if ids else None,
                          query_performed=bool(ids),
                          returned_keys=[[row[name] for name in KEYS[table]] for row in rows],
                          rows_sha256=digest(rows), missing_ids=missing,
                          status="missing" if missing else "complete")
        captured[table] = record
    states = [record["status"] for record in captured.values()]
    status = "missing" if "missing" in states else "unknown" if negative or "unknown" in states else "complete"
    return {"format": FORMAT, "scope": SCOPE, "status": status,
            "references": positive, "unresolved_negative_text_parameters": negative,
            "other_actions_not_resolved": unsupported,
            "tables": captured,
            "snapshot": {"consistency": bundle["consistency"],
                         "selection_sha256": digest(bundle["selection"]),
                         "schema_sha256": bundle["schema_sha256"],
                         "db_version_sha256": digest(bundle["db_version"]),
                         "table_sha256": {table: digest(tables[table]) for table in
                                          ("creature", "creature_template", event_ai.TABLE, *TABLES)
                                          if table in tables}}}


def validate(bundle: dict[str, Any], profiles: dict[str, Any]) -> None:
    tables = bundle["tables"]
    check(event_ai.TABLE in tables, "event_text.event_rows_required")
    ids = set(selected_ids(tables))
    for table in TABLES:
        # A present reviewed table must be queried even when it returns zero rows.
        # Missing schema is Unknown, never a manufactured complete-empty table.
        check((table in profiles) == (table in tables), "event_text.table_coverage_required")
        if table in tables:
            check(all(row["Id"] in ids for row in tables[table]), "event_text.unselected_row")
    check(canonical(bundle["event_text_capture"]) == canonical(evidence(bundle, profiles)),
          "event_text.coverage_mismatch")


def blockers(tables: dict[str, Any]) -> list[dict[str, Any]]:
    if event_ai.TABLE not in tables:
        return []
    findings = []
    def add(table, key, reason):
        findings.append({"table": table, "key": str(key), "status": "blocked", "reason": reason})
    ids = set(selected_ids(tables))
    for table in TABLES:
        if table not in tables:
            add(table, "schema", "event_text.table_missing_dependency_unknown")
    if "broadcast_text" in tables:
        for key in sorted(ids - {row["Id"] for row in tables["broadcast_text"]}):
            add("broadcast_text", key, "event_text.base_row_missing")
    _, negative, unsupported = references(tables)
    for key in sorted({row["event_id"] for row in negative}):
        add(event_ai.TABLE, key, "event_text.negative_parameter_dependency_unknown")
    for key in sorted({row["event_id"] for row in unsupported}):
        add(event_ai.TABLE, key, "event_text.other_action_dependencies_not_captured")
    return findings


def scope(bundle: dict[str, Any]) -> dict[str, Any]:
    """After bundle validation; absence is explicit and never complete-empty."""
    if "event_text_capture" not in bundle:
        return {"status": "unknown", "reason": "event_text.capture_absent", "scope": SCOPE}
    return {"status": bundle["event_text_capture"]["status"], "scope": SCOPE,
            "coverage_sha256": digest(bundle["event_text_capture"])}
