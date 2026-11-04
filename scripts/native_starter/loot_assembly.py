"""Checked raw loot inputs, separate from whole-host admission.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1 LootMgr.cpp:125-185,2619-2669.
"""
from classic_export import loot
from . import item_plan


def plan(source, ordered, raw_refusals):
    tables = source["tables"]
    items = {row["entry"]: row for row in tables["item_template"]}
    inputs, metadata, refusals = [], [], list(raw_refusals)
    for entry in sorted(loot.leaf_ids(tables)):
        item = items.get(entry)
        if item is None:
            refusals.append({"table": "item_template", "key": str(entry),
                             "reason": "loot.item_template_missing"})
            continue
        checked = item_plan.item_plan(item)
        refusals.extend({"table": "item_template", "key": str(entry),
                         "reason": "loot." + row["reason"]} for row in checked["refusals"])
        if checked["captured_input"] is not None:
            inputs.append(checked["captured_input"])
            # Complete query operands, not a client display or spell-query absence proof.
            metadata.append({"entry": entry, "item_template_sha256": checked["item_template_sha256"],
                             "row": item})
    return {"status": "blocked" if refusals else "checked_projection",
            "runtime_admitted": False, "creatures": loot.creatures(tables),
            "body_row_count": len(ordered.get(loot.BODY, [])),
            "reference_row_count": len(ordered.get(loot.REFERENCE, [])),
            "leaf_item_count": len(loot.leaf_ids(tables)), "item_inputs": inputs,
            "client_metadata_inputs": metadata,
            "remaining_dependencies": ["independent_inventory_audit", "client_item_query_binding",
                                       "ordinary_corpse_host_audit_and_creature_rows"],
            "refusals": refusals}
