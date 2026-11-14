// SPDX-License-Identifier: GPL-3.0-or-later
// ObjectMgr.cpp:7413-7664; Field.h:53-59; Player.cpp:6304-6428 at 8ec338a1.
use super::super::extensions::reputation::{KILL, RATE, SPILL};
use super::*;
use benilla_sim::{classic_spell::SpellFloat, reputation as rep};
const RANKS: [&str; 8] = [
    "Hated",
    "Hostile",
    "Unfriendly",
    "Neutral",
    "Friendly",
    "Honored",
    "Revered",
    "Exalted",
];
fn bits(row: &Value, name: &str) -> Result<u32> {
    dbc::binary32(row[name]["decimal"].as_str().unwrap())
}
pub(super) fn supported(source: &Value, table: &dbc::Table, rules: &Value) -> Result<Value> {
    let Some(value) = source.get("reputation_policy_capture") else {
        return Ok(json!("require_independent_sql_resolution"));
    };
    let mut plan = json!({"format":"cmangos-8ec338a1-reputation-policy-inputs-v1", "status":"blocked", "capture_binding":digest(value), "configuration":rules.get("reputation"), "configuration_binding":if rules.is_null(){Value::Null}else{json!(digest(rules))}, "aura_binding":"requires_actual_initialized_aura_list", "consumer_certified":false, "policies":null, "kill_sources":[], "loader_dispositions":[], "refusals":[]});
    let factions: BTreeMap<_, _> = (0..table.count)
        .map(|i| {
            let r = table.row(i);
            (r.word(0), r)
        })
        .collect();
    let faction_ids: Vec<_> = factions.keys().copied().collect();
    if value["faction_sha256"] != table.sha256 || value["faction_ids"] != json!(faction_ids) {
        plan["refusals"] = json!(["reputation.complete_faction_binding_required"]);
        return Ok(plan);
    }
    let mut refusals = BTreeSet::new();
    if rules.is_null() {
        refusals.insert("reputation.explicit_configuration_required");
    }
    let lookups = &value["lookups"];
    let mut rates = BTreeMap::new();
    let mut spills = BTreeMap::new();
    let mut dispositions = Vec::new();
    for record in lookups[RATE].as_array().unwrap() {
        let id = int(&record["id"]) as u32;
        let row = &record["row"];
        let (mut status, mut result) = ("absent", json!("Absent"));
        if !row.is_null() {
            let words = [
                bits(row, "quest_rate")?,
                bits(row, "creature_rate")?,
                bits(row, "spell_rate")?,
            ];
            let negative = words
                .iter()
                .any(|b| b & 0x80000000 != 0 && b & 0x7fffffff != 0);
            status = if negative {
                "skipped_negative_rate"
            } else {
                "accepted"
            };
            if !negative {
                result =
                    json!({"Present":{"quest":words[0], "creature":words[1], "spell":words[2]}});
            }
        }
        rates.insert(id, result);
        dispositions.push(json!({"table":RATE, "id":id, "status":status}));
    }
    for record in lookups[SPILL].as_array().unwrap() {
        let id = int(&record["id"]) as u32;
        let row = &record["row"];
        let (mut status, mut result, mut warnings) = ("absent", json!("Absent"), Vec::new());
        if !row.is_null() {
            let targets: Vec<_> = (1..=4)
                .map(|i| int(&row[format!("faction{i}")]) as u32)
                .collect();
            for (i, target) in targets.iter().enumerate() {
                if factions[&id].word(18) == 0 {
                    break;
                }
                if *target == 0 {
                    continue;
                }
                let reason = match factions.get(target) {
                    None => Some("missing_faction"),
                    Some(f) if (f.word(1) as i32) < 0 => Some("unlisted_faction"),
                    Some(_) if int(&row[format!("rank_{}", i + 1)]) >= 8 => Some("invalid_rank"),
                    _ => None,
                };
                if let Some(reason) = reason {
                    warnings.push(json!({"slot":i,"reason":reason}));
                }
            }
            status = if factions[&id].word(18) == 0 {
                "skipped_no_team"
            } else if targets.iter().any(|t| *t != 0 && !factions.contains_key(t)) {
                "skipped_missing_target"
            } else {
                "accepted"
            };
            if status == "accepted" {
                let mut slots = Vec::new();
                for (i, target) in targets.iter().enumerate() {
                    let rank = int(&row[format!("rank_{}", i + 1)]) as usize;
                    if *target != 0 && rank >= 8 {
                        refusals.insert("reputation.spillover_rank_unrepresentable");
                    }
                    slots.push(json!({"faction":target,"rate":bits(row,&format!("rate_{}",i+1))?,"maximum_rank":RANKS.get(rank).unwrap_or(&"Hated")}));
                }
                result = json!({"Present":slots});
            }
        }
        spills.insert(id, result);
        dispositions.push(json!({"table":SPILL,"id":id,"status":status,"warnings":warnings}));
    }
    let mut kills = Vec::new();
    for record in lookups[KILL].as_array().unwrap() {
        let id = int(&record["id"]) as u32;
        let row = &record["row"];
        let (mut status, mut converted) = ("absent", Value::Null);
        if let Some(row) = row.as_object() {
            let converted_row: Map<_, _> = row
                .iter()
                .map(|(n, v)| {
                    let v = if ["IsTeamAward1", "IsTeamAward2", "TeamDependent"]
                        .contains(&n.as_str())
                    {
                        json!(int(v) > 0)
                    } else if n.starts_with("RewOnKillRepValue") {
                        v.clone()
                    } else {
                        json!(int(v) as u32)
                    };
                    (n.clone(), v)
                })
                .collect();
            let valid = ["RewOnKillRepFaction1", "RewOnKillRepFaction2"]
                .iter()
                .all(|n| {
                    int(&converted_row[*n]) == 0
                        || factions.contains_key(&(int(&converted_row[*n]) as u32))
                });
            status = if valid {
                "accepted"
            } else {
                "skipped_missing_faction"
            };
            if valid {
                converted = Value::Object(converted_row);
            }
        }
        kills.push(json!({"entry":id,"admission":if converted.is_null(){"ProvenAbsentOnKillRow"}else{"PresentOnKillRow"},"row":converted}));
        dispositions.push(json!({"table":KILL,"id":id,"status":status}));
    }
    for result in rates.values().chain(spills.values()) {
        if let Some(present) = result.get("Present") {
            let words: Vec<u32> = if let Some(obj) = present.as_object() {
                obj.values().map(|v| int(v) as u32).collect()
            } else {
                present
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| int(&s["rate"]) as u32)
                    .collect()
            };
            if words.iter().any(|w| SpellFloat::from_bits(*w).is_err()) {
                refusals.insert("reputation.binary32_adapter_domain");
            }
        }
    }
    if refusals.is_empty() {
        let inputs = faction_ids
            .iter()
            .map(|id| {
                let reward_rates: rep::Resolved<rep::RewardRates> =
                    serde_json::from_value(rates[id].clone())
                        .map_err(|_| fail("reputation.policy_adapter"))?;
                let spillover: rep::Resolved<[rep::Spillover; 4]> =
                    serde_json::from_value(spills[id].clone())
                        .map_err(|_| fail("reputation.policy_adapter"))?;
                Ok(rep::FactionPolicyLookup {
                    faction: *id,
                    reward_rates: match reward_rates {
                        rep::Resolved::Absent => None,
                        rep::Resolved::Present(r) => Some(r),
                    },
                    spillover: match spillover {
                        rep::Resolved::Absent => None,
                        rep::Resolved::Present(s) => Some(s.map(|s| rep::RawSpillover {
                            faction: s.faction,
                            rate: s.rate,
                            maximum_rank: s.maximum_rank as u32,
                        })),
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let faction_rows: Vec<_> = factions
            .values()
            .map(|r| rep::Faction {
                id: r.word(0),
                reputation_list_id: r.word(1) as i32,
                race_masks: std::array::from_fn(|i| r.word(2 + i)),
                class_masks: std::array::from_fn(|i| r.word(6 + i)),
                base_values: std::array::from_fn(|i| r.word(10 + i) as i32),
                flags: std::array::from_fn(|i| r.word(14 + i)),
                team: r.word(18),
            })
            .collect();
        let policies = rep::compile_faction_policies(&faction_rows, &inputs)
            .map_err(|_| fail("reputation.policy_adapter"))?;
        plan["policies"] = json!(policies.iter().map(|p|json!({"faction":p.faction,"reward_rates":p.reward_rates,"spillover":p.spillover})).collect::<Vec<_>>());
        plan["status"] = json!("complete_inputs");
    }
    plan["refusals"] = json!(refusals);
    plan["kill_sources"] = json!(kills);
    plan["loader_dispositions"] = json!(dispositions);
    Ok(plan)
}
