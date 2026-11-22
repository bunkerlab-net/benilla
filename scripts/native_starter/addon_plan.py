"""Whole-row addon selection after pinned ObjectMgr loader checks.
SPDX-License-Identifier: GPL-3.0-or-later
CMaNGOS 8ec338a1 ObjectMgr.cpp:582-716; Creature.cpp:2296-2344;
SpellMgr.h:77-88,924-948. Loader repairs remain refusals, not silent repairs.
"""


def resolve(source, tables, dependency):
    raw = {name: dependency["tables"][name]["rows"]
           for name in ("creature_addon", "creature_template_addon")}
    reasons = set()
    def lookup(table, key):
        dbc = tables.get(table)
        return dbc.by_id.get(key) if dbc else None
    for rows in raw.values():
        if len(rows) > 1:
            reasons.add("spawn.addon_loader_invalid")
        for row in rows:
            if row["sheath_state"] > 2:
                reasons.add("spawn.addon_loader_invalid")
            if row["mount"] and lookup("CreatureDisplayInfo", row["mount"]) is None:
                reasons.add("spawn.addon_loader_lookup_required")
            if row["emote"] and lookup("Emotes", row["emote"]) is None:
                reasons.add("spawn.addon_loader_lookup_required")
            text = row["auras"] or ""
            # The bounded input accepts canonical positive SQL spell lists only.
            # Other atoi/spacing shapes need an explicit loader disposition.
            parts = text.split(" ") if text else []
            if any(not p.isascii() or not p.isdecimal() or len(p) > 10 or not 0 < int(p) <= 0xffffffff for p in parts):
                reasons.add("spawn.addon_loader_invalid")
                continue
            ids = [int(p) for p in parts]
            if len(ids) != len(set(ids)):
                reasons.add("spawn.addon_loader_invalid")
            for spell_id in ids:
                capture = source.get("addon_loader_capture")
                spell = next((s for s in (capture["rows"] if capture else source["tables"]["spell_template"])
                              if s["Id"] == spell_id), None)
                if spell is None:
                    reasons.add("spawn.addon_loader_invalid" if capture else "spawn.addon_loader_lookup_required")
                    continue
                effects = [spell[f"Effect{i}"] for i in range(1, 4)]
                if not any(e in {6, 27, 35, 119, 3, 77, 64} for e in effects):
                    reasons.add("spawn.addon_loader_invalid")
                cast_table = tables.get("SpellCastTimes")
                if cast_table is None:
                    reasons.add("spawn.addon_loader_lookup_required")
                cast = lookup("SpellCastTimes", spell["CastingTimeIndex"])
                if cast and 0 < cast.words[1] < 0x80000000:
                    reasons.add("spawn.addon_loader_invalid")
                # IsOnlySelfTargeting returns immediately at the first empty slot.
                # A self-only spell passes under either strict-log setting.
                for i, effect in enumerate(effects, 1):
                    if not effect:
                        break
                    if spell[f"EffectImplicitTargetA{i}"] != 1 or spell[f"EffectImplicitTargetB{i}"] not in {0, 1}:
                        reasons.add("spawn.addon_loader_strict_setting_required")
                        break
    table = next((name for name, rows in raw.items() if rows), None)
    effective = raw[table][0] if table else None
    if effective and (effective["mount"] or effective["stand_state"] or effective["emote"]
                      or effective["sheath_state"] not in {0, 1, 2} or effective["auras"] not in {None, ""}):
        reasons.add("spawn.addon_execution_required")
    return sorted(reasons)
