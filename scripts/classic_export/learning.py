"""Bounded server learning/rank dependencies and exact-ID recapture requests.

Schema: pinned CMaNGOS sql/base/mangos.sql; selectors are never supplied as SQL.
These records prove captured selector coverage, not native loader execution.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations
from typing import Any

from .core import (CORE_REVISION, MAX_ITEMS, MAX_ROWS, MAX_SPELLS, Selection,
                   canonical, check, integer, normalize_rows)

FORMAT = "benilla-classic-learning-selection-v1"
REQUEST_FORMAT = "benilla-native-dependency-request-v1"
TABLES = ("spell_chain", "spell_learn_spell")


def request(value: Any, selection: Selection) -> dict[str, Any]:
    fields = {"format", "core_revision", "selection", "source_sha256", "dbc_sha256", "item_ids", "spell_ids"}
    check(type(value) is dict and set(value) == fields, "learning.request_fields")
    check(value["format"] == REQUEST_FORMAT and value["core_revision"] == CORE_REVISION,
          "learning.request_profile")
    check(selection.starter is not None and canonical(value["selection"]) == canonical(selection.json()),
          "learning.request_selection")
    def hash_value(v):
        check(type(v) is str and len(v) == 64 and all(c in "0123456789abcdef" for c in v),
              "learning.request_hash")
    hash_value(value["source_sha256"])
    check(type(value["dbc_sha256"]) is dict and len(value["dbc_sha256"]) <= 32,
          "learning.request_dbc_hashes")
    for name, digest in value["dbc_sha256"].items():
        check(type(name) is str and name.isascii() and name.isalnum() and len(name) <= 64,
              "learning.request_dbc_name")
        hash_value(digest)
    for field, limit in (("item_ids", MAX_ITEMS), ("spell_ids", MAX_SPELLS)):
        ids = value[field]
        check(type(ids) is list and len(ids) <= limit, "learning.request_id_limit")
        for entry in ids:
            integer(entry, 1, 0xffffffff, "learning.request_id")
        check(ids == sorted(set(ids)), "learning.request_id_order")
    return value


def evidence(tables: dict[str, Any], dependency_request: dict[str, Any] | None = None,
             global_learning_rows: list[dict[str, Any]] | None = None) -> dict[str, Any]:
    return {"format": FORMAT, "spell_ids": [r["Id"] for r in tables["spell_template"]],
            "tables": list(TABLES), "filter": "all_matching_rows_no_loader_filters",
            "row_limit": MAX_ROWS, "dependency_request": dependency_request,
            "global_learn_spell_probe": {"predicate": "all_rows_limit_one",
                                         "rows": global_learning_rows}}


def references(tables: dict[str, Any]) -> set[int]:
    ids = set()
    for row in tables.get("spell_chain", []):
        integer(row["rank"], 0, 127, "learning.rank_domain")
        for name in ("prev_spell", "first_spell", "req_spell"):
            value = integer(row[name], 0, 0x7fffff, "learning.chain_reference")
            if value:
                ids.add(value)
    for row in tables.get("spell_learn_spell", []):
        integer(row["Active"], 0, 1, "learning.active_domain")
        ids.add(integer(row["SpellID"], 1, 65535, "learning.learned_spell"))
    return ids


def validate(value: Any, tables: dict[str, Any], selection: Selection, profile: dict[str, Any]) -> None:
    check(set(TABLES) <= set(tables), "learning.tables_required")
    check(type(value) is dict, "learning.coverage_fields")
    dependency_request = value.get("dependency_request")
    if dependency_request is not None:
        request(dependency_request, selection)
    probe = value.get("global_learn_spell_probe")
    check(type(probe) is dict and set(probe) == {"predicate", "rows"}, "learning.probe_fields")
    rows = probe["rows"]
    check(type(rows) is list and normalize_rows("spell_learn_spell", rows, profile, 1) == rows,
          "learning.probe_rows")
    check(canonical(value) == canonical(evidence(tables, dependency_request, rows)), "learning.coverage_mismatch")
    check(bool(rows) or not tables["spell_learn_spell"], "learning.empty_probe_with_selected_rows")
    spells = {r["Id"] for r in tables["spell_template"]}
    if rows and rows[0]["entry"] in spells:
        check(rows[0] in tables["spell_learn_spell"], "learning.probe_missing_selected_row")
    for table, key in (("spell_chain", "spell_id"), ("spell_learn_spell", "entry")):
        check(all(r[key] in spells for r in tables[table]), "learning.unselected_row")
    check(references(tables) <= spells, "learning.reference_missing")
    if dependency_request is not None:
        check(set(dependency_request["spell_ids"]) <= spells, "learning.request_spell_missing")
        check(set(dependency_request["item_ids"]) <= {r["entry"] for r in tables["item_template"]},
              "learning.request_item_missing")
