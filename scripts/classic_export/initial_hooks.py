"""Selected initial-cast area and proc inputs; never a blanket no-hooks claim.

No conditions or proc execution is implemented. Every matching raw row survives.
The consumer must establish the admitted effect path and rank domain separately.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from typing import Any
from .core import MAX_ROWS, MAX_SPELLS, canonical, check, digest, integer

FORMAT = "benilla-classic-initial-hook-selection-v1"
TABLES = ("spell_area", "spell_proc_event")
SELECTOR_COLUMNS = {"spell_area": "spell", "spell_proc_event": "entry"}
LIMITS = {"spell_area": MAX_ROWS, "spell_proc_event": MAX_SPELLS}
PRIMARY_KEYS = {"spell_area": ["spell", "area", "quest_start", "quest_start_active", "aura_spell", "racemask", "gender"],
                "spell_proc_event": ["entry"]}


def selected_ids(tables: dict[str, Any]) -> list[int]:
    # Both native SQL keys are unsigned MEDIUMINT. Reject an unsupported domain
    # before reading either table, rather than manufacture a selected-empty result.
    ids = sorted({integer(row["Id"], 1, 0xffffff, "initial_hooks.selector_domain")
                  for row in tables["spell_template"]})
    check(len(ids) <= MAX_SPELLS, "initial_hooks.selector_limit")
    return ids


def evidence(bundle: dict[str, Any]) -> dict[str, Any]:
    tables = bundle["tables"]
    ids = selected_ids(tables)
    return {"format": FORMAT,
            "selectors": {table: {"column": SELECTOR_COLUMNS[table], "ids": ids} for table in TABLES},
            "filter": "all_matching_rows_no_loader_filters", "row_limits": LIMITS,
            "scope": "selected_raw_area_and_proc_inputs_rank_semantics_not_resolved",
            "snapshot": {"consistency": bundle["consistency"],
                         "selection_sha256": digest(bundle["selection"]),
                         "schema_sha256": bundle["schema_sha256"],
                         "db_version_sha256": digest(bundle["db_version"]),
                         "table_sha256": {table: digest(tables[table]) for table in
                                          ("spell_template", "spell_chain", "spell_learn_spell", *TABLES)}}}


def validate(bundle: dict[str, Any]) -> None:
    tables = bundle["tables"]
    check(set(TABLES) <= set(tables), "initial_hooks.tables_required")
    check({"spell_chain", "spell_learn_spell"} <= set(tables), "initial_hooks.learning_required")
    check(canonical(bundle["initial_hook_capture"]) == canonical(evidence(bundle)),
          "initial_hooks.coverage_mismatch")
    ids = set(selected_ids(tables))
    for table in TABLES:
        check(len(tables[table]) <= LIMITS[table], "initial_hooks.row_limit")
        check(all(row[SELECTOR_COLUMNS[table]] in ids for row in tables[table]),
              "initial_hooks.unselected_row")


def scope(bundle: dict[str, Any], spell: int) -> dict[str, Any] | None:
    """Only after source validation. No evidence is represented by null."""
    if "initial_hook_capture" not in bundle:
        return None
    check(spell in selected_ids(bundle["tables"]), "initial_hooks.unselected_spell")
    return {"status": "complete", "coverage_sha256": digest(bundle["initial_hook_capture"]),
            **{table: [row for row in bundle["tables"][table]
                       if row[SELECTOR_COLUMNS[table]] == spell] for table in TABLES}}
