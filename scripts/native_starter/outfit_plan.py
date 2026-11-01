"""Source-ordered item operations, not a claim of resolved skill/spell learning.

SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
Player.cpp:857-985,7858-8042,9432-9555,9779-9847.
Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
"""
from __future__ import annotations


def equip_slots(item):
    cls, sub, kind = item["class"], item["subclass"], item["InventoryType"]
    supported = (
        cls == 0 and 0 <= sub <= 8 and kind == 0
        or cls == 15 and sub == 0 and kind == 0
        or cls == 4 and 0 <= sub <= 4 and kind in {*range(13), 16, 19, 20}
        or cls == 2 and sub in {0, 1, 4, 5, 6, 7, 8, 10, 13, 15, 20}
        and kind in {0, 13, 17, 21})
    if not supported:
        return None
    if kind == 0:
        return []
    if 1 <= kind <= 10:
        return [kind - 1]
    return {11: [10, 11], 12: [12, 13], 13: [15], 17: [15], 21: [15],
            16: [14], 19: [18], 20: [4]}[kind]


def supported_plan(grants, items):
    """Return guarded operations only when every selected item is in the bounded domain.

    Runtime CanUseItem still resolves class/race, level and learned proficiency.
    The capacity bound includes failed equips, so this report cannot hide overflow.
    """
    steps, reasons, counts = [], [], {}
    for grant in grants:
        entry, count = grant["item_id"], grant["count"]
        item = items.get(entry)
        if item is None:
            reasons.append({"item_id": entry, "reason": "item.placement_row_missing"})
            continue
        slots = equip_slots(item)
        if slots is None:
            reasons.append({"item_id": entry, "reason": "item.placement_type_unsupported"})
            continue
        stack = item["stackable"]
        if type(count) is not int or not 1 <= count <= 10_000 or not 1 <= stack <= 10_000:
            reasons.append({"item_id": entry, "reason": "item.placement_stack_domain"})
            continue
        if item["RequiredSkill"] or item["RequiredSkillRank"] or item["requiredspell"]:
            reasons.append({"item_id": entry, "reason": "item.placement_required_skill_spell_unsupported"})
            continue
        if any(item[name] for name in ("Flags", "BagFamily", "ContainerSlots", "itemset",
                "RandomProperty", "Duration", "ScriptName", "block", "requiredhonorrank",
                "RequiredReputationFaction")):
            reasons.append({"item_id": entry, "reason": "item.placement_property_unsupported"})
            continue
        if slots and any(item[f"spellid_{i}"] and item[f"spelltrigger_{i}"] != 0
                         for i in range(1, 6)):
            reasons.append({"item_id": entry, "reason": "item.placement_equip_spell_unsupported"})
            continue
        counts[entry] = counts.get(entry, 0) + count
        if item["maxcount"] > 0 and counts[entry] > item["maxcount"]:
            reasons.append({"item_id": entry, "reason": "item.placement_unique_count"})
        steps.append({**grant, "operation": "equip_one_then_store_remainder",
                      "candidate_slots": slots, "stackable": stack})
    if reasons:
        return None, reasons
    if sum((count + items[entry]["stackable"] - 1) // items[entry]["stackable"]
           for entry, count in counts.items()) > 16:
        return None, [{"reason": "item.placement_backpack_capacity"}]
    return {"policy": "cmangos-8ec338a1-starting-items-v1", "grants": steps,
            "second_pass": "backpack_slot_order_equip_else_merge_then_first_empty",
            "backpack_slots": 16, "equipment_resolved": False,
            "runtime_consumer": "starter_init::CapturedStarterItems::compile"}, []
