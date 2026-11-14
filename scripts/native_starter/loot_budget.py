"""Checked planning bounds shared with the existing Rust loot catalog policy.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

from collections import defaultdict
from decimal import Decimal
from classic_export import loot


def refusals(tables):
    indexed = {}
    errors = []
    count = sum(len(tables.get(table, [])) for table in loot.TABLES)
    if count > 4096:
        errors.append({"table": "loot", "key": "scope", "reason": "loot.retained_closure_limit"})
    for table in loot.TABLES:
        templates = defaultdict(list)
        for row in tables.get(table, []):
            templates[row["entry"]].append(row)
        indexed[table] = templates
        errors.extend({"table": table, "key": str(entry), "reason": "loot.template_row_limit"}
                      for entry, rows in templates.items() if len(rows) > 1024)

    def budget(rows, references):
        passes, work, draws = 1, 1, 0
        groups = defaultdict(lambda: [0, 0])
        for row in rows:
            chance = abs(Decimal(row["ChanceOrQuestChance"]["decimal"]))
            if row["groupid"]:
                groups[row["groupid"]][chance == 0] += 1
            else:
                work += 1
                draws += chance < 100
                if row["mincountOrRef"] < 0:
                    child = references.get(-row["mincountOrRef"], (0, 0, 0))
                    repeats = row["maxcount"]
                    passes += child[0] * repeats
                    work += child[1] * repeats
                    draws += child[2] * repeats
                else:
                    draws += 1
        for explicit, equal in groups.values():
            work += 2 * (explicit + equal)
            draws += explicit + bool(explicit) + 2 * equal + bool(explicit + equal)
        work += max(groups, default=0)
        return passes, work, draws

    references = {entry: budget(rows, {}) for entry, rows in indexed[loot.REFERENCE].items()}
    for entry, rows in indexed[loot.BODY].items():
        passes, work, draws = budget(rows, references)
        if max(passes, work, draws + 2) > 4096:
            errors.append({"table": loot.BODY, "key": str(entry), "reason": "loot.processing_budget"})
    return errors
