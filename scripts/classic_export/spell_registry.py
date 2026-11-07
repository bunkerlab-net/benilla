"""Exact selected spell/aura numeric registry coverage, never guessed hook absence.

Pinned SpellScriptMgr initializes numeric maps empty and assigns them only through
SELECT Id, ScriptName. Unknown/empty names stay present rows requiring semantics.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from typing import Any

from .core import MAX_SPELLS, canonical, check, digest, integer

FORMAT = "benilla-classic-spell-script-selection-v1"
TABLE = "spell_scripts"


def selected_ids(tables: dict[str, Any]) -> list[int]:
    # Field::GetUInt32 casts signed SQL Id to uint32. Positive-only SELECT must
    # reject zero and the wrapped-negative half before it can claim absence.
    ids = sorted({integer(row["Id"], 1, 0x7fffffff, "spell_registry.selector_domain")
                  for row in tables["spell_template"]})
    check(len(ids) <= MAX_SPELLS, "spell_registry.selector_limit")
    return ids


def evidence(bundle: dict[str, Any]) -> dict[str, Any]:
    tables = bundle["tables"]
    return {"format": FORMAT, "selector": {"column": "Id", "ids": selected_ids(tables)},
            "filter": "all_matching_rows_no_loader_filters", "row_limit": MAX_SPELLS,
            "scope": "spell_and_aura_numeric_registry_for_selected_spells",
            "snapshot": {"consistency": bundle["consistency"],
                         "selection_sha256": digest(bundle["selection"]),
                         "schema_sha256": bundle["schema_sha256"],
                         "db_version_sha256": digest(bundle["db_version"]),
                         "table_sha256": {table: digest(tables[table])
                                          for table in ("spell_template", TABLE)}}}


def validate(bundle: dict[str, Any]) -> None:
    tables = bundle["tables"]
    check(TABLE in tables, "spell_registry.table_required")
    check(len(tables[TABLE]) <= MAX_SPELLS, "spell_registry.row_limit")
    check(canonical(bundle["spell_script_capture"]) == canonical(evidence(bundle)),
          "spell_registry.coverage_mismatch")
    ids = set(selected_ids(tables))
    check(all(row["Id"] in ids for row in tables[TABLE]), "spell_registry.unselected_row")


def scope(bundle: dict[str, Any], spell: int) -> dict[str, Any] | None:
    """Call after source validation; null is unknown, not captured-empty."""
    if "spell_script_capture" not in bundle:
        return None
    check(spell in selected_ids(bundle["tables"]), "spell_registry.unselected_spell")
    return {"status": "complete", "rows": [row for row in bundle["tables"][TABLE]
                                            if row["Id"] == spell],
            "coverage_sha256": digest(bundle["spell_script_capture"])}
