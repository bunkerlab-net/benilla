"""Pinned LootMgr input checks and reference traversal, without loot execution.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import struct
from .core import check

STORES = ("creature_loot_template", "disenchant_loot_template", "fishing_loot_template",
          "gameobject_loot_template", "item_loot_template", "mail_loot_template",
          "pickpocketing_loot_template", "reference_loot_template", "skinning_loot_template")


def loot_status(row, items, conditions):
    """LootMgr.cpp:146-178,310-365, before cross-manager condition validation."""
    minimum, group, maximum = row["mincountorref"],row["groupid"],row["maxcount"]
    chance = struct.unpack("<f",struct.pack("<I",row["chanceorquestchance"]))[0]
    if maximum > 255:
        return "skip:maxcount"
    if row["condition_id"] and row["condition_id"] not in conditions:
        return "skip:missing_condition"
    if group >= 128:
        return "skip:group"
    if not minimum:
        return "skip:mincount"
    if minimum > 0:
        if row["item"] not in items:
            return "skip:missing_item"
        if not chance and not group:
            return "skip:ungrouped_zero"
        if chance and abs(chance) < struct.unpack("<f",struct.pack("<f",0.000001))[0]:
            return "skip:low_chance"
        if maximum < minimum:
            return "skip:count_range"
    else:
        check(minimum != -2147483648,"content.loot_reference_negation_undefined")
        if chance < 0:
            return "skip:negative_reference_chance"
        if not chance and not group:
            return "skip:zero_reference_chance"
    return "retained:condition_validation_pending" if row["condition_id"] else "retained:loader_validation_pending"


def check_reference_cycles(edges):
    """Active ancestors, not visited siblings, define a cycle, LootMgr.cpp:74-120."""
    finished, steps = set(), 0
    for root in sorted(edges):
        if root in finished:
            continue
        active, stack = set(), [(root,False)]
        while stack:
            node,exiting = stack.pop()
            if exiting:
                active.remove(node)
                finished.add(node)
                continue
            check(node not in active,"content.loot_reference_cycle")
            if node in finished:
                continue
            steps += 1
            check(steps <= 2000000,"content.view_limit")
            active.add(node)
            stack.append((node,True))
            stack.extend((target,False) for target in reversed(edges.get(node,[])))
