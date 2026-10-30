"""Raw incoming capture and bounded ordinary quest relationship normalization.

Derived from CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
src/game/Globals/ObjectMgr.cpp:4790-4876, ObjectMgr::LoadQuests.
Capture preserves raw edges; resolve refuses unsupported loader mutations.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from typing import Any
from .core import MAX_QUESTS, MAX_ROWS, INTEGER_SQL, canonical, check, digest, integer, sql_type

FORMAT = "benilla-classic-quest-relationship-exclusion-v1"
MODE = "quest_relationship_inbound"
FIELDS = ("entry", "PrevQuestId", "NextQuestId", "NextQuestInChain",
          "ExclusiveGroup", "BreadcrumbForQuestId")
SIGNED = {"PrevQuestId", "NextQuestId", "ExclusiveGroup"}


def projection(rows: list[dict[str, Any]]) -> list[dict[str, int]]:
    check(type(rows) is list and len(rows) <= MAX_ROWS, "quest_relationships.row_limit")
    result = []
    for row in rows:
        check(type(row) is dict and set(FIELDS) <= set(row), "quest_relationships.row_fields")
        result.append({name: integer(row[name], -0x7fffffff if name in SIGNED else
                                    (1 if name == "entry" else 0), 0x7fffffff,
                                    f"quest_relationships.numeric_domain:{row['entry']}:{name}:{row[name]}")
                       for name in FIELDS})
    check(len({row["entry"] for row in result}) == len(result), "quest_relationships.duplicate_entry")
    return sorted(result, key=lambda row: row["entry"])


def selected_ids(tables: dict[str, Any]) -> list[int]:
    rows = projection(tables["quest_template"])
    check(len(rows) <= MAX_QUESTS, "quest_relationships.root_limit")
    return [row["entry"] for row in rows]


def matches(row: dict[str, int], roots: list[int]) -> bool:
    return (row["NextQuestId"] in roots or -row["NextQuestId"] in roots
            or row["NextQuestInChain"] in roots or row["BreadcrumbForQuestId"] in roots)


def validate_schema(inventory: dict[str, Any]) -> None:
    schema_columns = {column["name"].casefold(): column
                      for column in inventory["tables"]["quest_template"]["columns"]}
    for name in FIELDS:
        column = schema_columns[name.casefold()]
        kind, _, _, unsigned = sql_type(column["type"])
        check(kind in INTEGER_SQL and column["nullable"] is False
              and (name not in SIGNED or not unsigned), "quest_relationships.schema_domain")


def evidence(bundle: dict[str, Any], inbound: list[dict[str, int]]) -> dict[str, Any]:
    validate_schema(bundle["schema"])
    roots = selected_ids(bundle["tables"])
    rows = projection(inbound)
    check(all(matches(row, roots) for row in rows), "quest_relationships.unselected_row")
    own = projection(bundle["tables"]["quest_template"])
    by_id = {row["entry"]: row for row in own}
    check(all(row["entry"] not in by_id or by_id[row["entry"]] == row for row in rows),
          "quest_relationships.conflicting_root")
    # Selected rows matching the scan must survive, even for a self-link.
    check(all(row in rows for row in own if matches(row, roots)), "quest_relationships.missing_root_edge")
    return {"format": FORMAT, "status": "complete", "scope": "raw_direct_inbound_exclusion_only",
            "roots": roots, "selectors": {"NextQuestId": sorted(roots + [-q for q in roots]),
                "NextQuestInChain": roots, "BreadcrumbForQuestId": roots},
            "row_limit": MAX_ROWS, "own": own, "inbound": rows,
            "own_sha256": digest(own), "inbound_sha256": digest(rows),
            "snapshot": {key: bundle[key] for key in
                         ("core_revision", "database_revision", "schema_sha256", "consistency")}
                        | {"selection_sha256": digest(bundle["selection"]),
                           "db_version_sha256": digest(bundle["db_version"]),
                           "quest_table_sha256": digest(bundle["tables"]["quest_template"])}}


def validate(bundle: dict[str, Any]) -> None:
    value = bundle["quest_relationship_capture"]
    check(type(value) is dict and type(value.get("inbound")) is list,
          "quest_relationships.evidence_fields")
    check(all(type(row) is dict and set(row) == set(FIELDS) for row in value["inbound"]),
          "quest_relationships.row_fields")
    check(canonical(value) == canonical(evidence(bundle, value["inbound"])),
          "quest_relationships.coverage_mismatch")


def blockers(bundle: dict[str, Any]) -> list[dict[str, str]]:
    """Call only after validation. Missing extension is unknown, not empty."""
    value = bundle.get("quest_relationship_capture")
    if value is None:
        return []  # Legacy reports stay valid and retain their existing holds.
    result = []
    for root in value["roots"]:
        for row in value["inbound"]:
            for field in ("NextQuestId", "NextQuestInChain", "BreadcrumbForQuestId"):
                if row[field] == root or (field == "NextQuestId" and row[field] == -root):
                    result.append({"table": "quest_template", "key": f"{root}:{row['entry']}:{field}",
                                   "status": "blocked", "reason": "quest.inbound_relationship"})
    return result


def resolve(bundle: dict[str, Any]) -> dict[str, Any]:
    """Normalize only closed ordinary selections after validating snapshot-bound evidence."""
    value = bundle.get("quest_relationship_capture")
    refusals = []

    def refuse(entry, field, target, reason):
        refusals.append({"quest": entry, "field": field, "target": target, "reason": reason})

    if value is None:
        refuse(0, "inbound", 0, "unknown_coverage")
        return {"status": "blocked", "refusals": refusals, "capture": None}
    validate(bundle)
    own = {row["entry"]: row for row in value["own"]}
    if len(own) > 1024:
        refuse(0, "closure", len(own), "closure_limit")
    if len(value["inbound"]) > 1024:
        refuse(0, "inbound", len(value["inbound"]), "closure_limit")
    for row in value["inbound"]:
        if row["entry"] not in own:
            for field in ("NextQuestId", "NextQuestInChain", "BreadcrumbForQuestId"):
                if abs(row[field]) in own:
                    refuse(row["entry"], field, row[field], "uncaptured_incoming_quest")
    for row in own.values():
        for field in FIELDS[1:]:
            target = row[field]
            if not target:
                continue
            if field in {"ExclusiveGroup", "BreadcrumbForQuestId"}:
                refuse(row["entry"], field, target, "unsupported_path")
            elif abs(target) not in own:
                refuse(row["entry"], field, target, "uncaptured_reference")
    for row in bundle["tables"]["quest_template"]:
        for field in ("RequiredCondition", "StartScript", "CompleteScript", "SrcItemId",
                      "SrcItemCount", "SrcSpell"):
            if row[field]:
                refuse(row["entry"], field, row[field], "unsupported_path")
        for slot in range(1, 5):
            for prefix in ("ReqSourceId", "ReqSourceCount", "ReqSpellCast"):
                field = f"{prefix}{slot}"
                if row[field]:
                    refuse(row["entry"], field, row[field], "unsupported_path")
    if refusals:
        return {"status": "blocked", "refusals": refusals, "capture": None}
    previous = {entry: [] for entry in own}
    previous_chain = {entry: [] for entry in own}
    for entry, row in own.items():
        # ObjectMgr.cpp:4804-4825 preserves Prev's sign and propagates Next's sign to the source ID.
        if row["PrevQuestId"]:
            previous[entry].append(row["PrevQuestId"])
        if row["NextQuestId"]:
            previous[abs(row["NextQuestId"])].append(entry if row["NextQuestId"] > 0 else -entry)
        # ObjectMgr.cpp:4790-4800 keeps reverse presentation chains separate from prerequisites.
        if row["NextQuestInChain"]:
            previous_chain[row["NextQuestInChain"]].append(entry)
    return {"status": "resolved", "refusals": [], "source_sha256": digest(bundle),
            "capture": {"roots": value["roots"], "own": value["own"], "inbound": value["inbound"]},
            "previous": {str(key): values for key, values in previous.items()},
            "previous_chain": {str(key): values for key, values in previous_chain.items()}}
