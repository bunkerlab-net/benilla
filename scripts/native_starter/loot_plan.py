"""Bounded source projection for loot::captured, not native-world admission.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f LootMgr.cpp:125-185,374-389,463-496,2396-2668.
"""
from __future__ import annotations

from classic_export import loot, loot_loader
from classic_export.core import ExportError, digest
from .dbc import binary32_bits
from . import loot_budget

ADAPTER = "benilla_sim::loot::captured::CapturedLoot::compile"
DISCHARGED = {"loot.source_loader_order_unproven", "loot.reference_generation_not_implemented",
              "loot.generation_and_item_admission_not_established"}


def supported_plan(source):
    if "loot_capture" not in source:
        return {"status": "not_captured", "adapter": ADAPTER, "refusals": []}
    tables = source["tables"]
    graph = loot.derived(tables)
    refusals = [r for r in loot.unsupported(tables, graph) if r["reason"] not in DISCHARGED]
    refusals.extend(loot_budget.refusals(tables))
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
            bag = item["class"] == 1 and item["InventoryType"] == 18 and 1 <= item["ContainerSlots"] <= 36 and item["stackable"] == 1
            if not (0 <= item["Quality"] <= 6 and 0 < item["displayid"] <= 0xffffffff and 0 <= item["class"] < 16
                    and 0 <= item["InventoryType"] < 29 and 1 <= item["stackable"] <= 255
                    and 0 <= item["maxcount"] <= 0x7fffffff and 0 <= item["bonding"] <= 4
                    and 0 <= item["MaxDurability"] <= 0xffffffff):
                reasons.append("item_template_domain")
            for field in ("startquest", "RandomProperty", "ExtraFlags", "Duration", "area", "Map", "ScriptName", "BagFamily", "itemset", "lockid"):
                if item[field]:
                    reasons.append("item_" + field + "_unsupported")
            if item["Flags"] & ~0x800:
                reasons.append("item_flags_unsupported")
            if item["class"] == 11 or item["InventoryType"] == 27 or not bag and (
                    item["class"] == 1 or item["InventoryType"] == 18 or item["ContainerSlots"]):
                reasons.append("item_container_shape_unsupported")
            if any(item[f"spellid_{i}"] and item[f"spelltrigger_{i}"] for i in range(1, 6)):
                reasons.append("item_equip_spell_unsupported")
            for i in range(1, 6):
                unsigned = ("spellid", "spelltrigger", "spellcategory")
                signed = ("spellcharges", "spellcooldown", "spellcategorycooldown")
                if (any(not 0 <= item[f"{name}_{i}"] <= 0xffffffff for name in unsigned)
                        or any(not -0x80000000 <= item[f"{name}_{i}"] <= 0x7fffffff for name in signed)):
                    reasons.append(f"item_spell_slot_{i}_projection_domain")
                ppm = item[f"spellppmRate_{i}"]
                try:
                    binary32_bits(ppm["decimal"] if type(ppm) is dict else str(ppm))
                except ExportError:
                    reasons.append(f"item_spell_slot_{i}_binary32_domain")
            if not reasons:
                admitted.append({"entry": entry, "item_template_sha256": digest(item),
                                 "acquisition": "empty_carried_bag" if bag else "backpack_item",
                                 "equip_and_use_admitted": False})
        refusals.extend({"table": "item_template", "key": str(entry), "reason": "loot." + reason} for reason in reasons)
    return {"status": "blocked" if refusals else "supported_shape", "adapter": ADAPTER,
            "runtime_admitted": False, "ordered_rows": ordered, "leaf_items": admitted,
            "input_bindings": {"source_sha256": digest(source), "loader": source.get("loot_loader_capture")},
            "remaining_dependencies": ["explicit_loot_Rules", "independent_inventory_audit",
                                       "ordinary_corpse_host_audit_and_creature_rows", "client_item_query_metadata"],
            "refusals": refusals}


def discharges(issue, plan):
    return plan["status"] == "supported_shape" and issue["reason"] in DISCHARGED
