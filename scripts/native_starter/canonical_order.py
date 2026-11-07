"""Versioned native schedule for complete captured collections, not a loader receipt.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import hashlib
import json
from pathlib import Path
from classic_export.core import canonical, check, digest

_RAW = (Path(__file__).resolve().parents[2] / "crates/benilla-sim/src/canonical_order.json").read_bytes()
CONTRACT = json.loads(_RAW)
POLICY = CONTRACT["format"]
IDENTITY = hashlib.sha256(_RAW).hexdigest()


def ordered(table, rows):
    check(table in CONTRACT["keys"], "canonical_order.table")
    check(type(rows) is list, "canonical_order.missing_rows")
    keys = CONTRACT["keys"][table]
    for row in rows:
        check(type(row) is dict and all(key in row for key in keys), "canonical_order.missing_key")
        check(all(type(row[key]) is int for key in keys), "canonical_order.key_domain")
    return sorted(rows, key=lambda row: (*[row[key] for key in keys], canonical(row)))


def collection(source, table):
    """Only validated complete source lookups may call this adapter."""
    return ordered(table, source["tables"][table])


def learning(source, resolved):
    if not resolved:
        return {"policy": POLICY, "policy_sha256": IDENTITY, "status": "blocked",
                "reason": "canonical_order.learning_inputs_incomplete"}
    tables = {name: collection(source, name) for name in
              ("playercreateinfo_skills", "playercreateinfo_spell", "spell_learn_spell")}
    return {"policy": POLICY, "policy_sha256": IDENTITY, "status": "complete_inputs",
            "ordered_rows_sha256": digest(tables), "collections": tables,
            "first_entry": "requires_actual_normalized_skill_set",
            "source_sha256": digest(source)}


def relationships(source):
    names = ("creature_questrelation", "creature_involvedrelation")
    capture = source.get("quest_giver_presentation_capture")
    if capture is None:
        return {"policy": POLICY, "policy_sha256": IDENTITY, "status": "blocked",
                "reason": "canonical_order.relation_inputs_incomplete"}
    tables = {name: collection(source, name) for name in names}
    mechanics = source.get("quest_mechanics_capture")
    scripts = None if mechanics is None else ordered("dbscripts_on_quest_end", mechanics["rows"]["dbscripts_on_quest_end"])
    return {"policy": POLICY, "policy_sha256": IDENTITY, "status": "complete_inputs",
            "collections": tables, "ordered_rows_sha256": digest(tables),
            "scripts": scripts, "scripts_sha256": None if scripts is None else digest(scripts),
            "source_sha256": digest(source)}
