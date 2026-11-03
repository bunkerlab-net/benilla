"""Bounded source projection for loot::captured, not native-world admission.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f LootMgr.cpp:125-185,374-389,463-496,2396-2668.
"""
from __future__ import annotations

from classic_export import loot, loot_loader
from classic_export.core import ExportError, digest
from .dbc import binary32_bits
from . import loot_budget, item_plan, non_body_loot

ADAPTER = "benilla_sim::loot::captured::CapturedLoot::compile"
DISCHARGED = {"loot.source_loader_order_unproven", "loot.reference_generation_not_implemented",
              "loot.generation_and_item_admission_not_established"}
NON_BODY = {"loot.PickpocketLootId_outside_body_scope", "loot.SkinningLootId_outside_body_scope"}


def supported_plan(source, closure=None):
    if "loot_capture" not in source:
        return {"status": "not_captured", "adapter": ADAPTER, "refusals": []}
    tables = source["tables"]
    graph = loot.derived(tables)
    non_body = non_body_loot.plan(source, closure)
    refusals = [r for r in loot.unsupported(tables, graph) if r["reason"] not in DISCHARGED | NON_BODY]
    refusals.extend(loot_budget.refusals(tables))
    refusals.extend(non_body["refusals"])
    ordered = {}
    for table in loot.TABLES:
        rows = loot_loader.ordered_rows(source, table)
        if rows is None:
            refusals.append({"table": table, "key": "scope", "reason": "loot.whole_loader_order_unknown"})
        else:
            ordered[table] = []
            for position, row in rows:
                try:
                    bits = binary32_bits(row["ChanceOrQuestChance"]["decimal"])
                except ExportError:
                    bits = None
                    refusals.append({"table": table, "key": f"{row['entry']}:{row['item']}",
                                     "reason": "loot.binary32_chance_domain"})
                if bits == 0 and row["groupid"] == 0:
                    refusals.append({"table": table, "key": f"{row['entry']}:{row['item']}",
                                     "reason": "loot.ungrouped_binary32_zero_chance"})
                ordered[table].append({"source_position": position,
                    "chance_or_quest_chance_bits": bits, **row})
    items = {row["entry"]: row for row in tables.get("item_template", [])}
    admitted = []
    for entry in graph["leaf_item_ids"]:
        item = items.get(entry)
        reasons = []
        if item is None:
            reasons.append("item_template_missing")
        else:
            plan = item_plan.item_plan(item)
            reasons.extend(r["reason"] for r in plan["refusals"])
            if not reasons:
                admitted.append({"entry": entry, "item_template_sha256": digest(item),
                                 "acquisition": plan["carried_operation"],
                                 "equip_and_use_admitted": False})
        refusals.extend({"table": "item_template", "key": str(entry), "reason": "loot." + reason} for reason in reasons)
    return {"status": "blocked" if refusals else "supported_shape", "adapter": ADAPTER,
            "runtime_admitted": False, "ordered_rows": ordered, "leaf_items": admitted,
            "non_body_actions": non_body,
            "input_bindings": {"source_sha256": digest(source), "loader": source.get("loot_loader_capture")},
            "remaining_dependencies": ["explicit_loot_Rules", "independent_inventory_audit",
                                       "ordinary_corpse_host_audit_and_creature_rows", "client_item_query_metadata"],
            "refusals": refusals}


def discharges(issue, plan):
    return plan["status"] == "supported_shape" and issue["reason"] in DISCHARGED | NON_BODY
