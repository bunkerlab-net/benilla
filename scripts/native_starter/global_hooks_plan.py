"""Source-bounded applicability, without executing scripts or conditions.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from classic_export.core import digest
from classic_export.global_hooks import CONTRACT


def rank_domain(ids, rows, dbcs):
    # A superset deliberately retains invalid/raw edges. Loader rejection cannot
    # introduce an edge outside SQL, Talent, or SkillLineAbility at this pin.
    edges = []
    for r in rows["spell_chain"]:
        edges.extend((r["spell_id"], abs(r[k])) for k in ("prev_spell", "first_spell"))
    talent = dbcs.get("Talent")
    abilities = dbcs.get("SkillLineAbility")
    if talent:
        for r in talent.rows:
            ranks = [r.words[i] for i in range(4, 9) if r.words[i]]
            edges.extend(zip(ranks, ranks[1:]))
    if abilities:
        edges.extend((r.words[2], r.words[8]) for r in abilities.rows)
        edges.append((2366, 2368))
    result = set(ids)
    while True:
        before = len(result)
        for a, b in edges:
            if a and b and (a in result or b in result):
                result.update((a, b))
        if before == len(result):
            return sorted(result)


def callback(source, kind, entry, name):
    manifest = CONTRACT["manifest"]
    capture = source["global_hook_capture"]
    registration = manifest["registrations"].get(name)
    status, reason = "absent", "empty_object_script_name"
    if name:
        if capture["configuration"] != manifest["configuration"]:
            status, reason = "unknown", "nondefault_build_configuration_not_reviewed"
        elif registration is None or registration["kind"] != kind:
            status, reason = "unknown", "registration_source_not_reviewed"
        else:
            status, reason = "present_unsupported", "registered_callback_execution_required"
    return {"kind": kind, "id": entry, "name": name, "status": status, "reason": reason,
            "registration": registration if name else None}


def reviewed_creature(source, row):
    value = source.get("global_hook_capture")
    return (value is not None and value["configuration"] == CONTRACT["manifest"]["configuration"]
            and row["ScriptName"] == "npc_garments_of_quests")


def supported(source, dbcs):
    capture = source.get("global_hook_capture")
    if capture is None:
        return None
    rows, tables = capture["rows"], source["tables"]
    ids = sorted(r["Id"] for r in tables["spell_template"])
    domain = rank_domain(ids, rows, dbcs)
    numeric = {r["Id"]: r for r in rows["spell_template"]}
    rank_complete = all(dbcs.get(t) is not None for t in ("Talent", "SkillLineAbility"))
    spells = []
    for spell in ids:
        inherited = rank_domain([spell], rows, dbcs)
        families = {}
        for table in ("spell_proc_event", "spell_affect", "spell_threat", "spell_elixir"):
            applicable = [r for r in rows[table] if r["entry"] in inherited]
            intrinsic = (numeric[spell]["procFlags"] != 0 if table == "spell_proc_event" else
                         any(numeric[spell][f"EffectItemType{i}"] and numeric[spell][f"EffectApplyAuraName{i}"] in (107, 108, 109, 112)
                             for i in range(1, 4)) if table == "spell_affect" else False)
            status = "present_unsupported" if applicable or intrinsic else "absent" if rank_complete else "unknown"
            families[table] = {"status": status, "rows": applicable, "intrinsic": intrinsic}
        groups = [r for r in rows["spell_group_spell"] if r["SpellId"] in inherited]
        families["spell_group"] = {"status": "present_unsupported" if groups else "absent" if rank_complete else "unknown", "rows": groups}
        spells.append({"id": spell, "rank_closure": inherited, "families": families,
                       "bonus": {"status": "complete_inputs", "owner": "spell_template",
                                 "coefficients": {n: v for n, v in numeric[spell].items() if n.startswith("EffectBonus")}},
                       "registry": [callback(source, CONTRACT["manifest"]["registrations"].get(r["ScriptName"], {}).get("kind", "spell"), spell, r["ScriptName"])
                                    for r in rows["spell_scripts"] if r["Id"] == spell]})
    areas = []
    for ordinal, row in enumerate(rows["spell_area"]):
        paths = []
        if row["spell"] in ids:
            paths.append("spell_location_check")
        if abs(row["aura_spell"]) in domain:
            paths.append("aura_apply")
        if row["area"]:
            paths.append("location_or_quest_transition_requires_location_owner")
        condition = row["condition_id"] & 0xffff
        areas.append({"ordinal": ordinal, "row": row, "area_indexed": bool(row["area"]),
                      "condition_branch": "condition" if condition else "legacy_gender_race_quest",
                      "condition": next((r for r in rows["conditions"] if r["condition_entry"] == condition), None) if condition else None,
                      "paths": paths, "status": "present_unsupported" if any(p != "location_or_quest_transition_requires_location_owner" for p in paths)
                      else "unknown" if paths else "unreachable_by_selected_spell_aura_and_area_indices"})
    callbacks = [callback(source, "creature", r["Entry"], r["ScriptName"]) for r in tables["creature_template"]]
    callbacks += [callback(source, "item", r["entry"], r["ScriptName"]) for r in tables["item_template"]]
    for table in ("world_template", "instance_template"):
        match = [r for r in rows[table] if r["map"] == source["selection"]["map_id"]]
        callbacks += [callback(source, "map", source["selection"]["map_id"], r["ScriptName"]) for r in match]
    items = [{"id": r["entry"], "spells": [r[f"spellid_{i}"] for i in range(1, 6) if r[f"spellid_{i}"]],
              "deletion_callback": callback(source, "item", r["entry"], r["ScriptName"]),
              "status": "present_unsupported" if r["ScriptName"] or r["area"] or r["Map"] or any(r[f"spellid_{i}"] for i in range(1, 6)) else "absent",
              "operands_sha256": digest(r)} for r in tables["item_template"]]
    return {"format": "cmangos-8ec338a1-global-hook-applicability-v1", "status": "diagnostic_only",
            "capture_sha256": digest(capture), "configuration": capture["configuration"],
            "registration_manifest_sha256": capture["registration_manifest_sha256"],
            "rank_inputs": {t: dbcs[t].sha256 if dbcs.get(t) else None for t in ("Talent", "SkillLineAbility")},
            "rank_semantics": "conservative_raw_connected_superset_not_effective_loader",
            "spell_area_loader_order": "captured_cursor_reference_loader_receipt_not_observed",
            "first_entry_location": "requires_source_location_owner_not_playercreateinfo_zone",
            "spells": spells, "spell_area": areas, "callbacks": callbacks, "item_effects": items,
            "registered_callbacks_resolved": all(r["status"] == "absent" for r in callbacks),
            "global_quest_effects_resolved": False,
            "quest_transition_domain": "unknown_requires_source_location_aura_and_inventory_owners",
            "item_effect_scope": "selected_item_spell_and_ScriptDevAI_callbacks_only",
            "effective_learning_proven": False, "runtime_export": None}
