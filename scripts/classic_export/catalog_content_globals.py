"""Player masks, locale fallback inputs and service slot order at the pin.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from .core import check


def mask_pairs(races, classes):
    """ObjectMgr.cpp:3259-3287 retains every mask bit and zero wildcard."""
    if races and not races & 0xff or classes and not classes & 0x5df:
        return []
    return [[race,klass] for race in range(1,9) for klass in range(1,12)
            if (not races or races & 1 << (race-1)) and (not classes or classes & 1 << (klass-1))]


def skill_pairs(row, infos, tiers):
    if row["step"] > 16:
        return []
    result = []
    for race,klass in mask_pairs(row["racemask"],row["classmask"]):
        for info in infos:
            if info[1] != row["skill"] or not info[2] & 1 << (race-1) or not info[3] & 1 << (klass-1):
                continue
            if row["step"]:
                tier = tiers.get(info[6])
                if tier is None or not tier[16+row["step"]]:
                    continue
            result.append([race,klass])
            break
    return result


def locale_state(raw):
    """ObjectMgr.cpp:2515-2539 stores only nonempty converted locale strings."""
    if raw is None:
        return "null:fallback"
    if not raw:
        return "empty:fallback"
    return "nul_prefix:fallback" if raw.startswith(b"\0") else "override_input"


def service_order(rows):
    """ORDER BY slot, ObjectMgr.cpp:9022, with ties explicitly unproven."""
    check(len(rows)*128 <= 8388608,"content.view_limit")
    ordered = sorted(rows,key=lambda r:(r[1],r[2]))
    ties, by_key = [], {}
    for entry,slot,ordinal in ordered:
        by_key.setdefault((entry,slot),[]).append(ordinal)
    for (entry,slot),ordinals in sorted(by_key.items()):
        if len(ordinals) > 1:
            ties.append([entry,slot,ordinals])
    return ordered,ties
