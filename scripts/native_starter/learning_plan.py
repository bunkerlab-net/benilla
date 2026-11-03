"""Selected ObjectMgr/SpellMgr loader plan, never effective learning or admission.

CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, ObjectMgr.cpp:3225-3330;
SpellMgr.cpp:1284-1770,2340-2389. Upstream GPL-2.0-or-later, CMaNGOS contributors.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from collections import defaultdict
from classic_export.learning import LOADER_FORMAT
from . import canonical_order


def supported_plan(source, dbcs, closure):
    tables = source["tables"]
    coverage = source.get("learning_capture")
    refusals = []

    def refuse(table, key, reason):
        refusals.append({"table": table, "key": [key] if key else [], "reason": reason})

    required = ("SkillLine", "SkillRaceClassInfo", "SkillTiers", "SkillLineAbility", "Talent")
    for name in required:
        if name not in dbcs:
            refuse(name, 0, "player.learning_loader_table_missing")
    if not coverage or coverage["format"] != LOADER_FORMAT:
        refuse("spell_chain", 0, "player.learning_global_chain_probe_not_captured")
    if not closure["captured_inputs_complete"]:
        refuse("spell_template", 0, "player.learning_loader_closure_incomplete")
    if refusals:
        return {"format": "benilla-native-starter-learning-plan-v3", "status": "blocked",
                "loaded_learning_maps_resolved": False, "effective_learning_proven": False,
                "refusals": refusals}
    lines = dbcs["SkillLine"].by_id
    tiers = dbcs["SkillTiers"].by_id
    infos = defaultdict(list)
    for row in sorted(dbcs["SkillRaceClassInfo"].rows, key=lambda r: r.key):
        if row.words[1] in lines:
            infos[row.words[1]].append(row.words)
    race, cls = source["selection"]["starter"]["race"], source["selection"]["starter"]["class"]
    race_mask, class_mask = 1 << (race - 1), 1 << (cls - 1)
    defaults, dispositions, seen = [], [], set()
    for row in canonical_order.collection(source, "playercreateinfo_skills"):
        skill, step = row["skill"], row["step"]
        if not 0 < skill <= 65535 or not 0 <= step <= 16:
            refuse("playercreateinfo_skills", skill, "player.learning_skill_step_domain")
            continue
        if row["raceMask"] and not row["raceMask"] & race_mask or row["classMask"] and not row["classMask"] & class_mask:
            dispositions.append({"skill": skill, "disposition": "unmatched_sql_masks"})
            continue
        if skill not in lines:
            refuse("SkillLine", skill, "player.learning_loader_table_missing")
            continue
        obtainable = False
        for info in infos[skill]:
            if not info[2] & race_mask or not info[3] & class_mask:
                continue
            if step:
                tier = tiers.get(info[6])
                if tier is None:
                    refuse("SkillTiers", info[6], "player.learning_loader_table_missing")
                    continue
                if not tier.words[16 + step]:
                    continue
            obtainable = True
            break
        dispositions.append({"skill": skill, "disposition": "loaded" if obtainable else "unobtainable"})
        if obtainable:
            if skill in seen:
                refuse("playercreateinfo_skills", skill, "player.learning_duplicate_effective_skill")
            seen.add(skill)
            defaults.append({"skill": skill, "step": step})
    spells = {r["Id"]: r for r in tables["spell_template"]}
    abilities = sorted(closure["ability_candidates"], key=lambda r: r["words"][0])
    ranks, talents, positions = {}, set(), {}

    def existence(spell):
        if spell not in spells:
            refuse("spell_template", spell, "player.learning_server_existence_unknown")
            return False
        return True

    def chain(spell, prev, first, rank, req=0):
        return {"spell": spell, "previous": prev, "first": first, "rank": rank, "required": req}

    selected = set(closure["spell_ids"])
    for row in sorted(dbcs["Talent"].rows, key=lambda r: r.key):
        w = row.words
        if not selected.intersection(w[4:9]):
            continue
        for slot, spell in enumerate(w[4:9]):
            if not spell:
                continue
            if spell in positions:
                refuse("Talent", spell, "player.learning_ambiguous_talent_membership")
            positions[spell] = w, slot
            if not existence(spell):
                continue
            talents.add(spell)
            if w[5]:
                ranks[spell] = chain(spell, w[3 + slot] if slot else 0, w[4], slot + 1)
        if w[20]:
            existence(w[20])
    predecessors = {}
    ability_spells = {row["words"][2] for row in abilities}
    for row in abilities:
        w = row["words"]
        spell, forward = w[2], 2368 if w[2] == 2366 else w[8]
        if not forward or not existence(spell) or not existence(forward) or forward not in ability_spells:
            continue
        prev = ranks.get(forward, {}).get("previous", predecessors.get(forward, spell))
        if prev != spell:
            refuse("SkillLineAbility", forward, "player.learning_rank_predecessor_conflict")
        if forward not in ranks:
            predecessors[forward] = spell
    for spell in predecessors:
        cursor, path = spell, []
        while cursor not in ranks:
            if cursor in path or len(path) > 30:
                refuse("SkillLineAbility", spell, "player.learning_rank_cycle_or_depth")
                break
            if cursor not in predecessors:
                ranks[cursor] = chain(cursor, 0, cursor, 1)
                break
            path.append(cursor)
            cursor = predecessors[cursor]
        if cursor not in ranks:
            continue
        for current in reversed(path):
            prev = predecessors[current]
            if ranks[prev]["rank"] >= 32:
                refuse("SkillLineAbility", current, "player.learning_rank_cycle_or_depth")
                break
            ranks[current] = chain(current, prev, ranks[prev]["first"], ranks[prev]["rank"] + 1)
    for row in tables["spell_chain"]:
        spell, prev, first, rank, req = (row[k] for k in ("spell_id", "prev_spell", "first_spell", "rank", "req_spell"))
        for reference in {spell, prev, first, req} - {0}:
            existence(reference)
        derived = ranks.get(spell)
        if derived:
            if (derived["previous"], derived["first"], derived["rank"]) != (prev, first, rank):
                refuse("spell_chain", spell, "player.learning_sql_derived_rank_conflict")
            derived["required"] = req
        else:
            if not first or (spell == first) != (rank <= 1) or (spell == first) != (prev == 0):
                refuse("spell_chain", spell, "player.learning_chain_integrity")
            if spell in positions:
                w, slot = positions[spell]
                if rank != slot + 1 or first != w[4] or req != w[20] or slot and prev != w[3 + slot]:
                    refuse("spell_chain", spell, "player.learning_chain_integrity")
            ranks[spell] = chain(spell, prev, first, rank, req)
    for spell, row in ranks.items():
        if row["previous"]:
            prev = ranks.get(row["previous"])
            if not prev or prev["first"] != row["first"] or prev["rank"] + 1 != row["rank"]:
                refuse("spell_chain", spell, "player.learning_chain_integrity")
        if row["required"]:
            req = ranks.get(row["required"])
            if not req or req["first"] == row["first"] or req["required"]:
                refuse("spell_chain", spell, "player.learning_chain_integrity")
    learned = defaultdict(list)
    if coverage["global_learn_spell_probe"]["rows"]:
        for row in canonical_order.collection(source, "spell_learn_spell"):
            if row["SpellID"] in talents:
                refuse("spell_learn_spell", row["entry"], "player.learning_sql_talent_target")
            if existence(row["entry"]) and existence(row["SpellID"]):
                learned[row["entry"]].append({"spell_id": row["SpellID"], "active": bool(row["Active"]), "auto_learned": False})
        for spell, row in sorted(spells.items()):
            for slot in range(1, 4):
                if row[f"Effect{slot}"] != 36:
                    continue
                target = row[f"EffectTriggerSpell{slot}"]
                if not existence(target) or any(link["spell_id"] == target for link in learned[spell]):
                    continue
                auto = (row[f"EffectImplicitTargetA{slot}"] == 5 or spell in talents
                        or row["Attributes"] & 0x40 != 0 or any(row[f"Effect{i}"] == 44 for i in range(1, 4)))
                learned[spell].append({"spell_id": target, "active": True, "auto_learned": auto})
    degrees = dict.fromkeys(selected, 0)
    for links in learned.values():
        for link in links:
            degrees[link["spell_id"]] += 1
    ready = [spell for spell, count in degrees.items() if count == 0]
    visited = 0
    while ready:
        visited += 1
        for link in learned.get(ready.pop(), []):
            degrees[link["spell_id"]] -= 1
            if degrees[link["spell_id"]] == 0:
                ready.append(link["spell_id"])
    if visited != len(selected):
        refuse("spell_learn_spell", 0, "player.learning_spell_cycle")
    order = canonical_order.learning(source, not refusals)
    return {"format": "benilla-native-starter-learning-plan-v3",
            "status": "blocked" if refusals else "supported",
            "loaded_learning_maps_resolved": not refusals, "effective_learning_proven": False,
            "source_identity": closure["dependency_request"]["source_sha256"],
            "selected_spell_ids": sorted(selected), "default_skill_candidates": defaults,
            "default_spell_candidates": [r["Spell"] for r in canonical_order.collection(source, "playercreateinfo_spell")],
            "root_order": "canonical_native_schedule" if not refusals else "unknown_incomplete_inputs",
            "canonical_order": order,
            "skill_dispositions": dispositions, "ranks": [ranks[k] for k in sorted(ranks)],
            "talent_spell_ids": sorted(talents),
            "learned_spells": [{"entry": k, "links": learned[k]} for k in sorted(learned)],
            "numeric_overlay": "requires_independent_complete_effective_projection",
            "initial_casts": "requires_independent_hooks_numeric_order_and_source_dispatch",
            "refusals": refusals}
