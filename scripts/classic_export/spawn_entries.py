"""Raw selected-GUID alternative-entry exclusion evidence, not spawn resolution.

Pinned ObjectMgr::LoadCreatureSpawnEntry retains alternatives in source query
order. This diagnostic capture neither reproduces that order nor selects entries.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from typing import Any
from .core import MAX_ROWS, canonical, check, digest, integer

FORMAT = "benilla-classic-spawn-entry-exclusion-v1"
TABLE = "creature_spawn_entry"
MODE = "spawn_entry_selectors"


def selected_guids(tables: dict[str, Any]) -> list[int]:
    rows = tables["creature"]
    check(type(rows) is list and len(rows) <= MAX_ROWS, "spawn_entries.root_limit")
    guids = [integer(row["guid"], 1, 0xffffffff, "spawn_entries.guid_domain") for row in rows]
    check(len(set(guids)) == len(guids), "spawn_entries.duplicate_root")
    return sorted(guids)


def evidence(bundle: dict[str, Any]) -> dict[str, Any]:
    roots = selected_guids(bundle["tables"])
    rows = bundle["tables"][TABLE]
    check(type(rows) is list and len(rows) <= MAX_ROWS, "spawn_entries.row_limit")
    keys = []
    for row in rows:
        check(type(row) is dict and set(row) == {"guid", "entry"}, "spawn_entries.row_fields")
        guid = integer(row["guid"], 1, 0xffffffff, "spawn_entries.guid_domain")
        # A zero alternative is raw unsupported evidence, not a dropped loader row.
        entry = integer(row["entry"], 0, 0xffffff, "spawn_entries.entry_domain")
        check(guid in roots, "spawn_entries.unselected_row")
        keys.append((guid, entry))
    check(keys == sorted(set(keys)), "spawn_entries.row_order_or_duplicate")
    return {"format": FORMAT, "status": "complete", "scope": "raw_selected_guid_exclusion_only",
            "roots": roots, "selector": {"column": "guid", "ids": roots},
            "filter": "all_matching_rows_no_loader_filters", "row_limit": MAX_ROWS,
            "source_loader_order_proven": False, "other_native_hooks_proven": False,
            "rows_sha256": digest(rows),
            "snapshot": {key: bundle[key] for key in
                         ("core_revision", "database_revision", "schema_sha256", "consistency")}
                        | {"selection_sha256": digest(bundle["selection"]),
                           "db_version_sha256": digest(bundle["db_version"]),
                           "creature_sha256": digest(bundle["tables"]["creature"])}}


def validate(bundle: dict[str, Any]) -> None:
    check(canonical(bundle["spawn_entry_capture"]) == canonical(evidence(bundle)),
          "spawn_entries.coverage_mismatch")


def blockers(bundle: dict[str, Any]) -> list[dict[str, str]]:
    """Presence is diagnostic-only, including alternatives for positive-ID roots."""
    if "spawn_entry_capture" not in bundle:
        return []
    return [{"table": TABLE, "key": f"{row['guid']}:{row['entry']}",
             "status": "blocked", "reason": "spawn.raw_entry_alternative_unimplemented"}
            for row in bundle["tables"][TABLE]]
