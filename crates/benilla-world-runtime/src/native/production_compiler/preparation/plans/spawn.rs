// SPDX-License-Identifier: GPL-3.0-or-later
// Independent translation of native_starter/spawn_plan.py. No runtime admission.
use super::*;
pub(in super::super) const FORMAT: &str = "cmangos-8ec338a1-selected-static-spawn-plan-v1";
const CONTEXT_FIELDS: [(&str, &[&str]); 5] = [
    ("creature_conditional_spawn", &["Guid"]),
    ("spawn_group_spawn", &["Guid"]),
    ("creature_linking", &["guid", "master_guid"]),
    ("creature_linking_template", &["entry", "master_entry"]),
    ("game_event_creature_data", &["guid"]),
];
// ObjectMgr.cpp:622-694 validates both tables; Creature.cpp:2296-2302
// selects the whole GUID row before any execution support is considered.
fn addon_refusals(
    source: &Value,
    dependency: &Value,
    dbcs: &Tables,
    reasons: &mut BTreeSet<String>,
) {
    let raw = ["creature_addon", "creature_template_addon"]
        .map(|table| dependency["tables"][table]["rows"].as_array().unwrap());
    let lookup = |table: &str, id: i128| {
        u32::try_from(id)
            .ok()
            .and_then(|id| dbcs.get(table).and_then(|t| t.get(id)))
    };
    for addons in raw {
        if addons.len() > 1 {
            reasons.insert("spawn.addon_loader_invalid".into());
        }
        for addon in addons {
            if int(&addon["sheath_state"]) > 2 {
                reasons.insert("spawn.addon_loader_invalid".into());
            }
            for (field, table) in [("mount", "CreatureDisplayInfo"), ("emote", "Emotes")] {
                if nonzero(&addon[field]) && lookup(table, int(&addon[field])).is_none() {
                    reasons.insert("spawn.addon_loader_lookup_required".into());
                }
            }
            let text = addon["auras"].as_str().unwrap_or("");
            let parse = |part: &str| {
                part.parse::<u32>().ok().filter(|id| {
                    *id != 0 && part.len() <= 10 && part.bytes().all(|b| b.is_ascii_digit())
                })
            };
            if !text.is_empty() && text.split(' ').any(|part| parse(part).is_none()) {
                reasons.insert("spawn.addon_loader_invalid".into());
                continue;
            }
            let mut ids = BTreeSet::new();
            for part in text.split(' ').filter(|_| !text.is_empty()) {
                let id = parse(part).unwrap();
                if !ids.insert(id) {
                    reasons.insert("spawn.addon_loader_invalid".into());
                }
                let capture = source.get("addon_loader_capture");
                let spells = capture.map_or_else(
                    || rows(source, "spell_template"),
                    |c| c["rows"].as_array().unwrap().as_slice(),
                );
                let Some(spell) = spells.iter().find(|s| int(&s["Id"]) == i128::from(id)) else {
                    reasons.insert(
                        if capture.is_some() {
                            "spawn.addon_loader_invalid"
                        } else {
                            "spawn.addon_loader_lookup_required"
                        }
                        .into(),
                    );
                    continue;
                };
                let effects = [1, 2, 3].map(|i| int(&spell[format!("Effect{i}")]));
                if !effects
                    .iter()
                    .any(|e| [6, 27, 35, 119, 3, 77, 64].contains(e))
                {
                    reasons.insert("spawn.addon_loader_invalid".into());
                }
                if !dbcs.contains_key("SpellCastTimes") {
                    reasons.insert("spawn.addon_loader_lookup_required".into());
                }
                if lookup("SpellCastTimes", int(&spell["CastingTimeIndex"]))
                    .is_some_and(|r| (r.word(1) as i32) > 0)
                {
                    reasons.insert("spawn.addon_loader_invalid".into());
                }
                for (slot, effect) in effects.into_iter().enumerate() {
                    if effect == 0 {
                        break;
                    }
                    let slot = slot + 1;
                    if int(&spell[format!("EffectImplicitTargetA{slot}")]) != 1
                        || ![0, 1].contains(&int(&spell[format!("EffectImplicitTargetB{slot}")]))
                    {
                        reasons.insert("spawn.addon_loader_strict_setting_required".into());
                        break;
                    }
                }
            }
        }
    }
    if let Some(addon) = raw.into_iter().find_map(|rows| rows.first()) {
        if nonzero(&addon["mount"])
            || nonzero(&addon["stand_state"])
            || nonzero(&addon["emote"])
            || ![0, 1, 2].contains(&int(&addon["sheath_state"]))
            || !(addon["auras"].is_null() || addon["auras"] == "")
        {
            reasons.insert("spawn.addon_execution_required".into());
        }
    }
}
pub(in super::super) fn plan(
    source: &Value,
    entry: i128,
    native: &Value,
    dbcs: &Tables,
) -> Result<Value> {
    let templates: BTreeMap<_, _> = rows(source, "creature_template")
        .iter()
        .map(|r| (int(&r["Entry"]), r))
        .collect();
    let mut selected = Vec::new();
    let mut refusals = Vec::new();
    let mut count = 0;
    for row in rows(source, "creature")
        .iter()
        .filter(|r| int(&r["id"]) == entry)
    {
        count += 1;
        let guid = int(&row["guid"]);
        let mut reasons = BTreeSet::new();
        let context: BTreeMap<_, Vec<_>> = CONTEXT_FIELDS
            .iter()
            .map(|(table, fields)| {
                (
                    *table,
                    rows(source, table)
                        .iter()
                        .filter(|r| {
                            fields.iter().any(|f| {
                                int(&r[*f])
                                    == if *table == "creature_linking_template" {
                                        entry
                                    } else {
                                        guid
                                    }
                            })
                        })
                        .collect(),
                )
            })
            .collect();
        let dependency = native
            .as_array()
            .and_then(|values| values.iter().find(|value| int(&value["guid"]) == guid));
        if let Some(dependency) = dependency {
            addon_refusals(source, dependency, dbcs, &mut reasons);
        }
        if let Some(dependency) =
            dependency.filter(|_| source.get("spawn_context_capture").is_some())
        {
            for (table, values) in &context {
                if !values.is_empty() {
                    reasons.insert(format!("spawn.unsupported_{table}"));
                }
            }
            for table in [
                "game_event_creature",
                "pool_creature",
                "pool_creature_template",
            ] {
                if !dependency["tables"][table]["rows"]
                    .as_array()
                    .unwrap()
                    .is_empty()
                {
                    reasons.insert(format!("spawn.unsupported_{table}"));
                }
            }
            if rows(source, "creature_spawn_entry")
                .iter()
                .any(|r| int(&r["guid"]) == guid)
            {
                reasons.insert("spawn.raw_entry_alternative_unimplemented".into());
            }
            let template = &dependency["spawn_template"]["row"];
            if template.is_null() {
                reasons.insert("creature.default_spawn_template_absent".into());
            } else if nonzero(&template["RelayId"])
                || nonzero(&template["StringId"])
                || int(&template["SpawnFlags"]) & !1 != 0
                || ![-1, int(&templates[&entry]["NpcFlags"])].contains(&int(&template["NpcFlags"]))
                || int(&template["EquipmentId"]) < -1
                || int(&template["UnitFlags"]) < -1
                || int(&template["UnitFlags"]) > 0xffffffff
            {
                reasons.insert("spawn.override_execution_required".into());
            }
        } else {
            reasons.insert("spawn.context_coverage_required".to_string());
        }
        if int(&row["spawnMask"]) != 1 || entry == 0 {
            reasons.insert("spawn.static_identity_required".into());
        }
        let minimum = int(&row["spawntimesecsmin"]);
        let maximum = int(&row["spawntimesecsmax"]);
        if minimum.max(maximum) > 0xffffffff / 9 {
            reasons.insert("spawn.respawn_timer_overflow".into());
        }
        let movement = int(&row["MovementType"]);
        if ![0, 1].contains(&movement) {
            reasons.insert("spawn.movement_type_unimplemented".into());
        }
        if ["position_x", "position_y", "position_z"].iter().any(|f| {
            compare_decimal(
                row[*f]["decimal"].as_str().unwrap().trim_start_matches('-'),
                "17066.166",
            ) == std::cmp::Ordering::Greater
        }) {
            reasons.insert("spawn.coordinate_domain".into());
        }
        let orientation = row["orientation"]["decimal"].as_str().unwrap();
        if compare_decimal(orientation, "0") == std::cmp::Ordering::Less
            || compare_decimal(orientation, "6.283185307179586") == std::cmp::Ordering::Greater
        {
            reasons.insert("spawn.orientation_domain".into());
        }
        let distance = row["spawndist"]["decimal"].as_str().unwrap();
        let negative_radius = compare_decimal(distance, "0") == std::cmp::Ordering::Less;
        let radius = if movement == 1 && !negative_radius {
            distance
        } else {
            "0"
        };
        if movement == 1 && negative_radius {
            reasons.insert("spawn.negative_random_radius_repair".into());
        }
        if compare_decimal(radius, "150") == std::cmp::Ordering::Greater {
            reasons.insert("spawn.wander_radius_domain".into());
        }
        if !reasons.is_empty() {
            refusals.extend(
                reasons
                    .into_iter()
                    .map(|reason| json!({"guid":num(guid),"reason":reason})),
            );
        } else {
            let dependency = dependency.unwrap();
            let position: Vec<_> = ["position_x", "position_y", "position_z"]
                .iter()
                .map(|f| dbc::binary32(row[*f]["decimal"].as_str().unwrap()))
                .collect::<Result<_>>()?;
            selected.push(json!({"guid":num(guid),"entry":num(entry),"map":row["map"],"position_binary32":position,"orientation_binary32":dbc::binary32(orientation)?,"movement":if radius!="0"{"wander"}else{"idle"},"radius_binary32":dbc::binary32(radius)?,"respawn_seconds":[num(minimum),num(minimum.max(maximum))],"spawn_template":dependency["spawn_template"]["row"],"input_binding":digest(&json!({"row":row,"native":dependency,"context":context,"alternatives":source["spawn_entry_capture"]}))}));
        }
    }
    Ok(
        json!({"format":FORMAT,"entry":num(entry),"status":if refusals.is_empty() && count!=0{"supported_shape"}else{"blocked"},"spawns":selected,"refusals":refusals,"cold_loader_order_proven":false,"assistance_traversal_proven":false,"production_admission":false}),
    )
}
pub(in super::super) fn discharges(issue: &Value, creatures: &Value) -> bool {
    let raw_key = issue["key"]
        .as_array()
        .filter(|key| key.len() == 1)
        .map_or(&issue["key"], |key| &key[0]);
    let key = if let Some(text) = raw_key.as_str() {
        if text.is_empty()
            || text.len() > 1 && text.starts_with('0')
            || !text.bytes().all(|b| b.is_ascii_digit())
        {
            return false;
        }
        text.parse::<u64>().ok()
    } else {
        raw_key.as_u64()
    };
    let Some(key) = key else {
        return false;
    };
    let reason = issue["reason"].as_str().unwrap();
    if issue["table"] == "creature"
        && [
            "movement.native_rules_required",
            "spawn.overrides_pools_events_not_captured",
            "creature.applicable_spawn_hooks_require_execution",
        ]
        .contains(&reason)
    {
        return creatures
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|c| c["spawn_context_plan"]["spawns"].as_array().unwrap())
            .any(|s| s["guid"].as_u64() == Some(key));
    }
    issue["table"] == "creature_template"
        && [
            "creature.deterministic_respawn_selection_required",
            "creature.unexported_groups_assistance_context",
        ]
        .contains(&reason)
        && creatures.as_array().unwrap().iter().any(|c| {
            c["spawn_context_plan"]["status"] == "supported_shape"
                && c["entry"].as_u64() == Some(key)
        })
}
pub(in super::super) fn consumes(table: &str, row: &Value, creatures: &Value) -> bool {
    let mut selected = creatures
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["spawn_context_plan"]["spawns"].as_array().unwrap());
    match table {
        "creature" | "creature_addon" => selected.any(|s| s["guid"] == row["guid"]),
        "creature_spawn_data" => selected.any(|s| s["guid"] == row["Guid"]),
        "creature_spawn_data_template" => {
            selected.any(|s| s["spawn_template"]["Entry"] == row["Entry"])
        }
        "creature_template_addon" => selected.any(|s| s["entry"] == row["entry"]),
        _ => false,
    }
}
