"""Learning-context hook proofs, not active spell admission.

CMaNGOS 8ec338a1 Player.cpp:3390-3448 dispatches only actual learn casts.
SpellMgr.cpp:2796-2866 keeps location-indexed SpellArea execution separate.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from classic_export.core import digest
from classic_export.global_hooks import CONTRACT
from .global_hooks_plan import rank_domain
from .dbc import binary32_bits
import struct


def supported(source, dbcs, spells, loader, numeric):
    indexed = {s["id"]: s for s in spells}
    capture = source.get("global_hook_capture")
    complete = (capture is not None and
                capture.get("registration_manifest_sha256") == digest(CONTRACT["manifest"]) and
                capture.get("configuration") == CONTRACT["manifest"]["configuration"])
    links = {r["entry"]: r["links"] for r in loader.get("learned_spells", [])}
    talents = set(loader.get("talent_spell_ids", []))
    roots = loader.get("default_spell_candidates", [])
    rows = capture["rows"] if capture else None
    start = source["tables"]["playercreateinfo"][0]
    selection = source["selection"]
    inside = start["map"] == selection["map_id"] and all(
        selection["min_milliunits"][i] <=
        struct.unpack("<f", struct.pack("<I", binary32_bits(start[key]["decimal"])))[0] * 1000 <=
        selection["max_milliunits"][i]
        for i, key in enumerate(("position_x", "position_y", "position_z")))
    results = []
    for root in roots:
        pending, closure, casts = [root], set(), set()
        unknown = False
        while pending:
            spell = pending.pop()
            if spell in closure:
                continue
            closure.add(spell)
            row = indexed.get(spell)
            if row is None or row.get("server_numeric_words") is None:
                unknown = True
                continue
            w = row["server_numeric_words"]
            # A conservative potential-cast superset. A nonempty set is never
            # promoted to an actual occurrence stream or a successful seal.
            if w[6] & 0x40 or w[7] & 0x80000000 or 44 in w[61:64] or spell in talents:
                casts.add(spell)
            rank = next((r for r in loader.get("ranks", []) if r["spell"] == spell), None)
            if rank and rank["previous"]:
                pending.append(rank["previous"])
            pending.extend(r["spell_id"] for r in links.get(spell, []) if not r["auto_learned"])
        reasons = []
        if not inside:
            reasons.append("first_entry_location_outside_selection")
        if numeric["status"] != "checked" or not loader["loaded_learning_maps_resolved"] or unknown:
            reasons.append("learning_context_inputs_unknown")
        if not complete:
            reasons.append("registration_manifest_unknown")
        if complete and casts:
            domain = set(rank_domain(casts, rows, dbcs))
            for table in ("spell_proc_event", "spell_affect", "spell_threat", "spell_elixir"):
                if any(r["entry"] in domain for r in rows[table]):
                    reasons.append("reached_" + table + "_unsupported")
            for spell in sorted(casts):
                w = indexed[spell]["server_numeric_words"]
                if w[24]:
                    reasons.append("reached_intrinsic_proc_unsupported")
                if any(w[103+i] and w[91+i] in (107, 108, 109, 112) for i in range(3)):
                    reasons.append("reached_intrinsic_affect_unsupported")
                for registration in rows["spell_scripts"]:
                    if registration["Id"] == spell:
                        name = registration["ScriptName"]
                        known = CONTRACT["manifest"]["registrations"].get(name)
                        reasons.append("reached_callback_unsupported" if known else "reached_registration_unknown")
            if any(r["SpellId"] in domain for r in rows["spell_group_spell"]):
                reasons.append("reached_spell_group_unsupported")
            if any(r["spell_id"] in casts or r["prev_spell"] in casts or
                   r["first_spell"] in casts or r["req_spell"] in casts for r in rows["spell_chain"]):
                reasons.append("reached_rank_relation_unsupported")
            for area in rows["spell_area"]:
                if area["spell"] in casts or abs(area["aura_spell"]) in casts:
                    reasons.append("reached_spell_area_unsupported")
                elif area["area"]:
                    reasons.append("first_entry_source_location_required")
        reasons = sorted(set(reasons))
        results.append({"id": root, "learning_closure": sorted(closure),
                        "potential_cast_ids": sorted(casts),
                        "status": "unknown" if any("unknown" in r for r in reasons) else "blocked" if reasons else
                                  "pending_source_seal" if casts else "hooks_unreachable",
                        "effective_learning_proven": False,
                        "refusals": reasons})
    return {"format": "cmangos-8ec338a1-reached-learning-hooks-v1",
            "scope": "creation_and_first_entry_learning_not_active_use",
            "source_identity": digest(source), "selection_sha256": digest(source["selection"]),
            "location_source_epoch": None,
            "location": "source_grid_and_area_catalog_required_not_sql_zone",
            "roots": results, "discharged_root_count": 0,
            "unreachable_hook_root_count": sum(r["status"] == "hooks_unreachable" for r in results),
            "effective_learning_proven": False, "runtime_export": None}
