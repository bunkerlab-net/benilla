// SPDX-License-Identifier: GPL-3.0-or-later
//! Player.cpp:3390-3448 learning reachability, separate from active use.
use super::super::global_hooks::{domain, rank_edges, CONTRACT};
use super::*;

fn array(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

pub(super) fn plan(c: &Compiler<'_>, spells: &Value, loader: &Value, numeric: &Value) -> Value {
    let indexed: BTreeMap<_, _> = array(spells).iter().map(|s| (int(&s["id"]), s)).collect();
    let capture = &c.source["global_hook_capture"];
    let complete = !capture.is_null()
        && capture["registration_manifest_sha256"] == digest(&CONTRACT["manifest"])
        && capture["configuration"] == CONTRACT["manifest"]["configuration"];
    let raw = &capture["rows"];
    let edges = if capture.is_null() {
        vec![]
    } else {
        rank_edges(raw, c.dbcs)
    };
    let talents: BTreeSet<_> = array(&loader["talent_spell_ids"]).iter().map(int).collect();
    let mut results = vec![];
    let start = &rows(c.source, "playercreateinfo")[0];
    let selection = &c.source["selection"];
    let inside = start["map"] == selection["map_id"]
        && ["position_x", "position_y", "position_z"]
            .iter()
            .enumerate()
            .all(|(i, key)| {
                start[*key]["decimal"]
                    .as_str()
                    .and_then(|v| v.parse::<f32>().ok())
                    .is_some_and(|v| {
                        let milli = f64::from(v) * 1000.0;
                        milli >= int(&selection["min_milliunits"][i]) as f64
                            && milli <= int(&selection["max_milliunits"][i]) as f64
                    })
            });
    for root in array(&loader["default_spell_candidates"]) {
        let mut pending = vec![int(root)];
        let mut closure = BTreeSet::new();
        let mut casts = BTreeSet::new();
        let mut unknown = false;
        while let Some(spell) = pending.pop() {
            if !closure.insert(spell) {
                continue;
            }
            let Some(row) = indexed.get(&spell) else {
                unknown = true;
                continue;
            };
            let Some(words) = row["server_numeric_words"].as_array() else {
                unknown = true;
                continue;
            };
            let w = |i| int(&words[i]) as u32;
            // Potential casts are never presented as an observed cast stream.
            if w(6) & 0x40 != 0
                || w(7) & 0x8000_0000 != 0
                || (61..64).any(|i| w(i) == 44)
                || talents.contains(&spell)
            {
                casts.insert(spell);
            }
            if let Some(rank) = array(&loader["ranks"])
                .iter()
                .find(|r| int(&r["spell"]) == spell)
            {
                if int(&rank["previous"]) != 0 {
                    pending.push(int(&rank["previous"]));
                }
            }
            if let Some(links) = array(&loader["learned_spells"])
                .iter()
                .find(|r| int(&r["entry"]) == spell)
            {
                pending.extend(
                    array(&links["links"])
                        .iter()
                        .filter(|r| r["auto_learned"] == false)
                        .map(|r| int(&r["spell_id"])),
                );
            }
        }
        let mut reasons = BTreeSet::new();
        if !inside {
            reasons.insert("first_entry_location_outside_selection");
        }
        if numeric["status"] != "checked"
            || loader["loaded_learning_maps_resolved"] != true
            || unknown
        {
            reasons.insert("learning_context_inputs_unknown");
        }
        if !complete {
            reasons.insert("registration_manifest_unknown");
        }
        if complete && !casts.is_empty() {
            let inherited = domain(casts.iter().copied(), &edges);
            for (table, reason) in [
                ("spell_proc_event", "reached_spell_proc_event_unsupported"),
                ("spell_affect", "reached_spell_affect_unsupported"),
                ("spell_threat", "reached_spell_threat_unsupported"),
                ("spell_elixir", "reached_spell_elixir_unsupported"),
            ] {
                if array(&raw[table])
                    .iter()
                    .any(|r| inherited.contains(&int(&r["entry"])))
                {
                    reasons.insert(reason);
                }
            }
            for spell in &casts {
                let words = indexed[spell]["server_numeric_words"].as_array().unwrap();
                let w = |i| int(&words[i]) as u32;
                if w(24) != 0 {
                    reasons.insert("reached_intrinsic_proc_unsupported");
                }
                if (0..3).any(|i| w(103 + i) != 0 && matches!(w(91 + i), 107 | 108 | 109 | 112)) {
                    reasons.insert("reached_intrinsic_affect_unsupported");
                }
                for registration in array(&raw["spell_scripts"])
                    .iter()
                    .filter(|r| int(&r["Id"]) == *spell)
                {
                    let name = registration["ScriptName"].as_str().unwrap();
                    reasons.insert(
                        if CONTRACT["manifest"]["registrations"].get(name).is_some() {
                            "reached_callback_unsupported"
                        } else {
                            "reached_registration_unknown"
                        },
                    );
                }
            }
            if array(&raw["spell_group_spell"])
                .iter()
                .any(|r| inherited.contains(&int(&r["SpellId"])))
            {
                reasons.insert("reached_spell_group_unsupported");
            }
            if array(&raw["spell_chain"]).iter().any(|r| {
                ["spell_id", "prev_spell", "first_spell", "req_spell"]
                    .iter()
                    .any(|key| casts.contains(&int(&r[*key])))
            }) {
                reasons.insert("reached_rank_relation_unsupported");
            }
            for area in array(&raw["spell_area"]) {
                if casts.contains(&int(&area["spell"]))
                    || casts.contains(&int(&area["aura_spell"]).abs())
                {
                    reasons.insert("reached_spell_area_unsupported");
                } else if int(&area["area"]) != 0 {
                    reasons.insert("first_entry_source_location_required");
                }
            }
        }
        results.push(json!({"id":root,"learning_closure":numbers(closure),"potential_cast_ids":numbers(casts.iter().copied()),
            "status":if reasons.iter().any(|r|r.contains("unknown")){"unknown"}else if !reasons.is_empty(){"blocked"}else if !casts.is_empty(){"pending_source_seal"}else{"hooks_unreachable"},
            "effective_learning_proven":false,"refusals":reasons}));
    }
    json!({"format":"cmangos-8ec338a1-reached-learning-hooks-v1","scope":"creation_and_first_entry_learning_not_active_use",
        "source_identity":c.source_sha,"selection_sha256":digest(&c.source["selection"]),
        "location_source_epoch":null,
        "location":"source_grid_and_area_catalog_required_not_sql_zone",
        "discharged_root_count":0,
        "unreachable_hook_root_count":results.iter().filter(|r|r["status"]=="hooks_unreachable").count(),
        "roots":results,"effective_learning_proven":false,"runtime_export":null})
}

#[cfg(test)]
#[path = "reached_hooks_tests.rs"]
mod tests;
