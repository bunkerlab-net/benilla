use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn catalog04_assistance_static_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        eprintln!("SKIP assistance catalog04 census: BENILLA_F5_AI_CATALOG is absent");
        return;
    };
    let mut catalog =
        crate::native::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let templates = catalog_creature_ai_census_inputs(&mut catalog).unwrap();
    let spawns = catalog
        .execution_event_ai_operands("creature", ["guid", "id"])
        .unwrap();
    let by_guid = spawns
        .iter()
        .map(|(_, [guid, entry])| (*guid, *entry))
        .collect::<BTreeMap<_, _>>();
    let resolver = super::super::motion_census_tests::MotionEntryAlternatives::load(&mut catalog);
    let alternatives = by_guid
        .iter()
        .map(|(&guid, &entry)| (guid, resolver.resolve(guid, entry)))
        .collect::<BTreeMap<_, _>>();
    let template_entries = templates.iter().map(|t| t.entry).collect::<BTreeSet<_>>();
    let no_entries = alternatives
        .iter()
        .filter(|(_, entries)| entries.is_empty())
        .map(|(guid, _)| *guid)
        .collect::<Vec<_>>();
    let unknown_templates = alternatives
        .iter()
        .filter(|(_, entries)| entries.iter().any(|e| !template_entries.contains(e)))
        .map(|(guid, _)| *guid)
        .collect::<Vec<_>>();
    let zero_raw = by_guid
        .iter()
        .filter(|(_, entry)| **entry == 0)
        .map(|(guid, _)| *guid)
        .collect::<BTreeSet<_>>();
    let count_spawns = |entries: &BTreeSet<u32>| {
        alternatives
            .values()
            .filter(|candidates| !candidates.is_disjoint(entries))
            .count()
    };
    let enabled = templates
        .iter()
        .filter(|t| t.extra_flags & 0x800 == 0)
        .map(|t| t.entry)
        .collect::<BTreeSet<_>>();
    let disabled = templates
        .iter()
        .filter(|t| t.extra_flags & 0x800 != 0)
        .map(|t| t.entry)
        .collect::<BTreeSet<_>>();
    let overrides = templates
        .iter()
        .filter(|t| t.call_for_help != 0)
        .map(|t| t.entry)
        .collect::<BTreeSet<_>>();
    let script = templates
        .iter()
        .filter(|t| t.script_name == "boss_emperor_dagran_thaurissan")
        .map(|t| t.entry)
        .collect::<BTreeSet<_>>();
    let bespoke = templates
        .iter()
        .filter(|t| t.script_name == "boss_scarlet_commander_mograine")
        .map(|t| t.entry)
        .collect::<BTreeSet<_>>();
    let mut help_rows = Vec::new();
    let mut throw_rows = Vec::new();
    let mut throw_loader_skips = Vec::new();
    let mut help_entries = BTreeSet::new();
    let mut throw_entries = BTreeSet::new();
    let mut deviation_rows = Vec::new();
    let mut raw_rows = 0;
    for row in catalog_event_ai_rows(&mut catalog).unwrap() {
        raw_rows += 1;
        for slot in 0..3 {
            let at = 12 + slot * 4;
            let kind = row.words[at] as u16;
            if kind != 39 && kind != 45 {
                continue;
            }
            let selector = row.words[1] as i32;
            let entries = if selector > 0 {
                BTreeSet::from([selector as u32])
            } else {
                alternatives
                    .get(&selector.unsigned_abs())
                    .cloned()
                    .unwrap_or_default()
            };
            let item = serde_json::json!({"row":row.words[0],"selector":selector,"entries":entries,"slot":slot+1,"params":&row.words[at+1..at+4]});
            if kind == 39 {
                help_rows.push(item);
                help_entries.extend(entries);
            } else {
                // CreatureEventAIMgr.cpp:909-913 skips these before enum conversion.
                if row.words[at + 1] >= 12 {
                    throw_loader_skips.push(item);
                    continue;
                }
                throw_rows.push(item);
                throw_entries.extend(entries);
            }
        }
    }
    let mut db_events = Vec::new();
    let mut db_assistance = Vec::new();
    let mut db_rows = 0;
    for table in [
        "dbscripts_on_creature_death",
        "dbscripts_on_creature_movement",
        "dbscripts_on_event",
        "dbscripts_on_go_template_use",
        "dbscripts_on_go_use",
        "dbscripts_on_gossip",
        "dbscripts_on_quest_end",
        "dbscripts_on_quest_start",
        "dbscripts_on_relay",
        "dbscripts_on_spell",
    ] {
        for (ordinal, [id, command, event, radius]) in catalog
            .unit_event_census_operands(table, ["id", "command", "datalong", "datalong2"])
            .unwrap()
        {
            db_rows += 1;
            if command != 35 {
                continue;
            }
            let item = serde_json::json!({"table":table,"ordinal":ordinal,"script":id,"command":command,"event":event,"radius":radius});
            if event > 1023 {
                deviation_rows.push(item.clone());
            }
            if event == AI_EVENT_CALL_ASSISTANCE {
                db_assistance.push(item.clone());
            }
            db_events.push(item);
        }
    }
    assert_eq!(db_rows, 7166);
    let mut spell_scripts = Vec::new();
    catalog
        .execution_spell_script_scan(|ordinal, spell, name| {
            if name == b"spell_gameobject_call_for_help_on_usage" {
                spell_scripts.push(serde_json::json!({"ordinal":ordinal,"spell":spell}));
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(templates.len(), 10384);
    assert_eq!(spawns.len(), 66436);
    assert_eq!(raw_rows, 11469);
    let mut report = serde_json::json!({
        "catalog":"catalog-04", "catalog_sha256":"757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6",
        "counting":"static alternatives, not active membership or recorded dispatch; sets overlap",
        "all_templates":templates.len(), "all_spawn_rows":spawns.len(),
        "spawn_entry_scope":"existing MotionEntryAlternatives random/group/squad/conditional/event resolver; overlapping possible outcomes, not a live selection",
        "spawn_raw_zero_entry_rows":zero_raw.len(),
        "spawn_raw_zero_resolved_rows":zero_raw.iter().filter(|g| !alternatives[g].is_empty()).count(),
        "spawn_no_entry_rows":no_entries.len(),"spawn_no_entry_guids":no_entries,
        "spawn_unknown_template_rows":unknown_templates.len(),"spawn_unknown_template_guids":unknown_templates,
        "spawn_mixed_assistance_alternatives":alternatives.values().filter(|entries| !entries.is_disjoint(&enabled) && !entries.is_disjoint(&disabled)).count(),
        "assistance_enabled_templates":enabled.len(),"assistance_enabled_spawn_rows":count_spawns(&enabled),
        "assistance_disabled_templates":disabled.len(),"assistance_disabled_spawn_rows":count_spawns(&disabled),
        "call_for_help_override_templates":overrides.len(),"call_for_help_override_spawn_rows":count_spawns(&overrides),
    });
    let extra = serde_json::json!({
        "eventai_help_action_slots":help_rows.len(),"eventai_help_rows":help_rows.iter().map(|r|r["row"].as_u64().unwrap()).collect::<BTreeSet<_>>().len(),
        "eventai_help_templates":help_entries.len(),"eventai_help_spawn_rows":count_spawns(&help_entries),"eventai_help":help_rows,
        "eventai_throw_slots":throw_rows.len(),"eventai_throw_templates":throw_entries.len(),"eventai_throw_spawn_rows":count_spawns(&throw_entries),"eventai_throw":throw_rows,
        "scriptdev_generic_help_templates":script.len(),"scriptdev_generic_help_spawn_rows":count_spawns(&script),"scriptdev_generic_help_entries":script,
        "scriptdev_bespoke_help_templates":bespoke.len(),"scriptdev_bespoke_help_spawn_rows":count_spawns(&bespoke),
        "scriptdev_bespoke_help_note":"MograineCallForHelp uses zone combat/AttackClosestEnemy, not this generic owner; F5.17 then F6, boss_mograine_and_whitemane.cpp:241-257",
        "gameobject_spell_help_bindings":spell_scripts.len(),"gameobject_spell_help":spell_scripts,
        "dbscript_send_ai_event_rows":db_events.len(),"dbscript_send_ai_event":db_events,
        "dbscript_call_assistance_rows":db_assistance.len(),"dbscript_call_assistance":db_assistance,
        "first_stopping_reasons":{
            "isolated_assistance_enabled":"F6 grid visitor after setting the real latch; no captured result is fabricated",
            "isolated_assistance_disabled":"source-defined no-call return after setting the latch; aggro linking/group event still schedules",
            "isolated_call_for_help":"F6 grid visitor after victim/pet/charm guards; NO_CALL_ASSIST does not gate CallForHelp",
            "eventai39_and_45":"now dispatch live Unit calls; F6 visitor with assistance composition, otherwise explicit live binding refusal; earlier readiness/target owners remain",
            "scriptdev":"F5.17 concrete emperor callback before generic help; no scripted instance constructed here",
            "spell_script":"F5.08/21 spell callback before F6 visitor, spell_scripts.cpp:363-378",
            "dbscript35":"F5.20 script target/callback binding, then F6 visitor for nonzero radius; script-template dynamic target binding is unavailable",
            "unit_update":"F5.08 cooldown/spell phase precedes events unless its actual owner is supplied"
        },
        "recorded_deviation":"Owner decision 2026-11-12 preserves raw u32 event numbers; AIDefines.h:22-56; ScriptMgr.cpp:2759-2775",
        "source_enum_out_of_range_rows":deviation_rows.len(),"source_enum_out_of_range":deviation_rows,
        "recorded_deviation_reach":"reaches the recorded Deviation once a DB-script executor exists",
        "receiver_null_ai_normal_stock_reach":"UNKNOWN",
        "receiver_null_ai_reach_source":"Creature.cpp:1738 inserts before1740 AI initialization; Map.cpp:493-503 AddToWorld/active/visibility callbacks; Creature.cpp:205-206 -> Object.cpp:2571-2578 -> Map.cpp:1579-1583,390-399 -> ObjectGridLoader.cpp:138 can enter nested initialization/callbacks; exact stock callback/visitor membership is unavailable",
        "pointer_order":"receiver list preserves traversal order, EntitiesMgr.h:46; helper AttackerSet requires recorded allocation, Unit.h:1239-1245",
        "newly_admitted_end_to_end":0,"native_format":91,"reference_execution_sealed":false
    });
    let serde_json::Value::Object(extra) = extra else {
        unreachable!()
    };
    report.as_object_mut().unwrap().extend(extra);
    report["eventai_throw_raw_slots"] =
        serde_json::json!(throw_rows.len() + throw_loader_skips.len());
    report["eventai_throw_loader_skipped_slots"] = serde_json::json!(throw_loader_skips.len());
    report["eventai_throw_loader_skips"] = serde_json::json!(throw_loader_skips);
    println!(
        "assistance_census {}",
        serde_json::to_string(&report).unwrap()
    );
    if let Some(path) = std::env::var_os("BENILLA_F5_ASSISTANCE_CENSUS") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    assert_eq!(
        deviation_rows
            .iter()
            .map(|row| (
                row["table"].as_str().unwrap(),
                row["script"].as_u64().unwrap(),
                row["command"].as_u64().unwrap(),
                row["event"].as_u64().unwrap(),
            ))
            .collect::<Vec<_>>(),
        [("dbscripts_on_gossip", 7166, 35, 7166)]
    );
}
