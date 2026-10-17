"""Bounded EventAI selector coverage; never executes or filters event scripts.

Pinned CreatureEventAIMgr::LoadCreatureEventAI_Scripts uses signed creature_id:
positive template entries and negative spawn GUIDs. CreatureEventAI::InitAI adds
both lists. Any matching row blocks this no-events profile, even debug/zero-chance.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

from typing import Any

from .core import MAX_ROWS, canonical, check, integer

TABLE = "creature_ai_scripts"
FORMAT = "benilla-classic-event-ai-selection-v1"
MAX_SELECTORS = 2 * MAX_ROWS


def selectors(tables: dict[str, list[dict[str, Any]]]) -> list[int]:
    # Avoid INT32_MIN negation and all selector coercion; unsupported extreme
    # native IDs fail capture rather than becoming a false no-events claim.
    selected = {integer(row["Entry"], 1, 0x7fffffff, "event_ai.entry_domain")
                for row in tables["creature_template"]}
    selected.update(-integer(row["guid"], 1, 0x7fffffff, "event_ai.guid_domain")
                    for row in tables["creature"])
    check(len(selected) <= MAX_SELECTORS, "event_ai.selector_limit")
    return sorted(selected)


def evidence(tables: dict[str, list[dict[str, Any]]]) -> dict[str, Any]:
    return {"format": FORMAT, "selector_column": "creature_id", "selectors": selectors(tables),
            "row_limit": MAX_ROWS, "filter": "all_matching_rows_no_event_filters"}


def validate(value: Any, tables: dict[str, list[dict[str, Any]]]) -> None:
    check(TABLE in tables, "event_ai.table_required")
    check(canonical(value) == canonical(evidence(tables)), "event_ai.coverage_mismatch")
    selected = set(selectors(tables))
    for row in tables[TABLE]:
        integer(row["id"], 1, 0xffffffff, "event_ai.id_domain")
        selector = integer(row["creature_id"], -0x7fffffff, 0x7fffffff, "event_ai.selector_domain")
        check(selector in selected, "event_ai.unselected_row")


def scope(tables: dict[str, list[dict[str, Any]]], entry: int) -> dict[str, Any] | None:
    """Absence evidence for the captured current entry and its selected spawns.

    Not evidence about event/pool/random entry substitutions or uncaptured GUIDs.
    """
    if TABLE not in tables:
        return None
    entries = {entry} | {-r["guid"] for r in tables["creature"] if r["id"] == entry}
    matching = [row for row in tables[TABLE] if row["creature_id"] in entries]
    return {"selectors": sorted(entries), "matching_rows": matching,
            "status": "matching_rows_present" if matching else "captured_selectors_empty",
            "scope": "captured_current_entry_and_selected_spawn_guids"}
