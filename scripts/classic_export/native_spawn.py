"""Bounded native spawn inputs, with explicit complete-empty versus unknown.

Pinned ObjectMgr::LoadCreatures joins spawn data, event and pool membership;
Creature::GetCreatureAddon selects the GUID row before the current-entry row.
This captures those inputs, not loader corrections or executable hooks.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

from typing import Any

from .columns import SPAWN_SCHEMA
from .core import MAX_ROWS, canonical, check, digest, integer

FORMAT = "benilla-classic-native-spawn-selection-v1"
TABLES = tuple(SPAWN_SCHEMA)
SELECTOR_COLUMNS = {table: fields[0][0] for table, fields in SPAWN_SCHEMA.items()}
GUID_TABLES = ("creature_spawn_data", "game_event_creature", "pool_creature", "creature_addon")
ENTRY_TABLES = ("pool_creature_template", "creature_template_addon")
HOOK_TABLES = ("game_event_creature", "pool_creature", "pool_creature_template")


def selectors(tables: dict[str, Any]) -> dict[str, list[int]]:
    guids = sorted({integer(r["guid"], 1, 0xffffffff, "native_spawn.guid_domain")
                    for r in tables["creature"]})
    entries = sorted({integer(r["Entry"], 1, 0xffffffff, "native_spawn.entry_domain")
                      for r in tables["creature_template"]})
    templates = {0}
    templates.update(integer(r["Id"], 0, 0xffffffff, "native_spawn.template_domain")
                     for r in tables["creature_spawn_data"])
    result = {table: guids for table in GUID_TABLES}
    result.update({table: entries for table in ENTRY_TABLES})
    result["creature_spawn_data_template"] = sorted(templates)
    check(all(len(ids) <= MAX_ROWS for ids in result.values()), "native_spawn.selector_limit")
    return result


def evidence(bundle: dict[str, Any]) -> dict[str, Any]:
    tables = bundle["tables"]
    return {"format": FORMAT, "filter": "all_matching_rows_no_loader_filters",
            "row_limit": MAX_ROWS, "scope": "named_tables_current_entries_and_selected_spawn_guids",
            "other_native_hooks_proven": False,
            "selectors": {table: {"column": SELECTOR_COLUMNS[table], "ids": ids}
                          for table, ids in selectors(tables).items()},
            "snapshot": {"consistency": bundle["consistency"],
                         "selection_sha256": digest(bundle["selection"]),
                         "schema_sha256": bundle["schema_sha256"],
                         "db_version_sha256": digest(bundle["db_version"]),
                         "table_sha256": {table: digest(tables[table]) for table in
                                          ("creature", "creature_template") + TABLES}}}


def validate(bundle: dict[str, Any]) -> None:
    tables = bundle["tables"]
    check(set(TABLES) <= set(tables), "native_spawn.tables_required")
    check(canonical(bundle["native_spawn_capture"]) == canonical(evidence(bundle)),
          "native_spawn.coverage_mismatch")
    for table, ids in selectors(tables).items():
        check(all(row[SELECTOR_COLUMNS[table]] in ids for row in tables[table]),
              "native_spawn.unselected_row")
    templates = {r["Entry"] for r in tables["creature_spawn_data_template"]}
    # Absent default is explicitly captured and blocks preparation. A dangling
    # explicit override must never silently fall back to zero in the capture.
    check({r["Id"] for r in tables["creature_spawn_data"] if r["Id"]} <= templates,
          "native_spawn.template_reference_missing")
    for row in tables["creature_spawn_data_template"]:
        integer(row["NpcFlags"], -1, 0x7fffffff, "native_spawn.npc_flags_domain")
        integer(row["UnitFlags"], -1, 0xffffffff, "native_spawn.unit_flags_domain")
        integer(row["EquipmentId"], -1, 0x7fffff, "native_spawn.equipment_domain")
    models = {r["modelid"] for r in tables["creature_model_info"]}
    check(model_roots(tables) <= models,
          "native_spawn.model_reference_missing")


def scope(bundle: dict[str, Any], entry: int) -> list[dict[str, Any]] | None:
    """Call only after validate_bundle. This proof is for current static entries.

    Null means legacy unknown coverage. Each complete table keeps actual rows,
    including negative events and zero-valued hooks; no hook is evaluated here.
    """
    if "native_spawn_capture" not in bundle:
        return None
    tables = bundle["tables"]
    result = []
    for spawn in tables["creature"]:
        if spawn["id"] != entry:
            continue
        guid = spawn["guid"]
        rows = {table: [r for r in tables[table] if r[SELECTOR_COLUMNS[table]] ==
                       (guid if table in GUID_TABLES else entry)]
                for table in GUID_TABLES + ENTRY_TABLES}
        binding = rows["creature_spawn_data"]
        template_id = binding[0]["Id"] if binding else 0
        template = next((r for r in tables["creature_spawn_data_template"]
                         if r["Entry"] == template_id), None)
        addon_table = ("creature_addon" if rows["creature_addon"] else
                       "creature_template_addon" if rows["creature_template_addon"] else None)
        result.append({"guid": guid, "entry": entry,
            "coverage_sha256": digest(bundle["native_spawn_capture"]),
            "tables": {table: {"status": "complete", "rows": value} for table, value in rows.items()},
            "spawn_template": {"id": template_id, "selection": "guid_binding" if template_id else "default_zero",
                               "status": "captured_row_present" if template else "captured_row_absent",
                               "row": template},
            "addon": {"status": "applicable_row_present" if addon_table else "captured_absent",
                      "table": addon_table, "row": rows[addon_table][0] if addon_table else None},
            "unimplemented_hook_tables": [table for table in HOOK_TABLES if rows[table]] +
                                         ([addon_table] if addon_table else []),
            "scope": "captured_current_static_entry_and_spawn_guid"})
    return result


def model_roots(tables: dict[str, Any], entry: int | None = None) -> set[int]:
    """All template alternatives plus each actually selected spawn override.

    Entry zero is captured for provenance but its model is not applicable when
    every selected GUID uses an explicit nonzero spawn-data template.
    """
    roots = {integer(row[f"DisplayId{i}"], 0, 0xffffffff, "native_spawn.model_domain")
             for row in tables["creature_template"] if entry is None or row["Entry"] == entry
             for i in range(1, 5)}
    bindings = {row["Guid"]: row["Id"] for row in tables.get("creature_spawn_data", [])}
    templates = {row["Entry"]: row for row in tables.get("creature_spawn_data_template", [])}
    for spawn in tables["creature"]:
        if entry is not None and spawn["id"] != entry:
            continue
        template = templates.get(bindings.get(spawn["guid"], 0))
        if template is not None:
            roots.add(integer(template["ModelId"], 0, 0xffffffff, "native_spawn.model_domain"))
    return roots - {0}


def model_closure(tables: dict[str, Any], roots: set[int]) -> set[int]:
    """Discover reachable other-gender IDs, including any missing legacy rows.

    Alternatives stay raw unsupported context; do not guess their applicability.
    """
    models = {row["modelid"]: row for row in tables["creature_model_info"]}
    selected, pending = set(), set(roots)
    while pending:
        current = pending.pop()
        integer(current, 1, 0xffffffff, "native_spawn.model_domain")
        selected.add(current)
        check(len(selected) <= MAX_ROWS, "native_spawn.model_limit")
        row = models.get(current)
        other = integer(row["modelid_other_gender"], 0, 0xffffffff,
                        "native_spawn.model_domain") if row else 0
        if other and other not in selected:
            pending.add(other)
    return selected
