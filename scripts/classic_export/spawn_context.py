"""Selected static spawn exclusions, not cold-loader order or gameplay admission.

Pinned ObjectMgr::LoadCreatures/LoadSpawnGroups, CreatureLinkingMgr loaders,
GameEventMgr and PoolManager own these contexts. Reject every group membership,
including groups without formations, rather than invent their spawn policy.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from typing import Any
from .columns import SPAWN_CONTEXT_SCHEMA
from .core import MAX_ROWS, canonical, check, digest, integer
from . import spawn_entries

FORMAT = "benilla-classic-spawn-context-exclusion-v1"
MODE = "spawn_context_selectors"
TABLES = tuple(SPAWN_CONTEXT_SCHEMA)
FIELDS = {"creature_conditional_spawn": ("Guid",), "spawn_group_spawn": ("Guid",),
          "creature_linking": ("guid", "master_guid"),
          "creature_linking_template": ("entry", "master_entry"),
          "game_event_creature_data": ("guid",)}


def selectors(tables: dict[str, Any]) -> dict[str, list[int]]:
    guids = spawn_entries.selected_guids(tables)
    entries = sorted({integer(row["id"], 0, 0xffffff, "spawn_context.entry")
                      for row in tables["creature"]} - {0})
    return {table: entries if table == "creature_linking_template" else guids
            for table in TABLES}


def evidence(bundle: dict[str, Any]) -> dict[str, Any]:
    selected = selectors(bundle["tables"])
    for table, ids in selected.items():
        rows = bundle["tables"][table]
        check(type(rows) is list and len(rows) <= MAX_ROWS, "spawn_context.row_limit")
        check(all(any(row[field] in ids for field in FIELDS[table]) for row in rows),
              "spawn_context.unselected_row")
    return {"format": FORMAT, "status": "complete", "filter": "all_matching_rows_no_loader_filters",
            "selectors": {table: {"columns_or": list(FIELDS[table]), "ids": ids}
                          for table, ids in selected.items()},
            "source_loader_order_proven": False, "assistance_traversal_proven": False,
            "snapshot": {key: bundle[key] for key in
                         ("core_revision", "database_revision", "schema_sha256", "consistency")}
                        | {"selection_sha256": digest(bundle["selection"]),
                           "db_version_sha256": digest(bundle["db_version"]),
                           "table_sha256": {table: digest(bundle["tables"][table]) for table in
                                            ("creature", "creature_spawn_entry") + TABLES}}}


def validate(bundle: dict[str, Any]) -> None:
    check("spawn_entry_capture" in bundle and "native_spawn_capture" in bundle,
          "spawn_context.related_coverage_required")
    check(canonical(bundle["spawn_context_capture"]) == canonical(evidence(bundle)),
          "spawn_context.coverage_mismatch")


def scope(bundle: dict[str, Any], guid: int, entry: int) -> dict[str, Any] | None:
    if "spawn_context_capture" not in bundle:
        return None
    return {table: [row for row in bundle["tables"][table]
                    if any(row[field] == (entry if table == "creature_linking_template" else guid)
                           for field in FIELDS[table])] for table in TABLES}
