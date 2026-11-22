use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn catalog04_unit_events_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        eprintln!("SKIP catalog04_unit_events_census: BENILLA_F5_AI_CATALOG absent");
        return;
    };
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let templates = catalog_creature_ai_census_inputs(&mut catalog).unwrap();
    let spawns = catalog
        .execution_event_ai_operands("creature", ["guid", "id"])
        .unwrap()
        .into_iter()
        .map(|(_, [guid, entry])| (guid, entry))
        .collect::<BTreeMap<_, _>>();
    let mut delayed_eventai = Vec::new();
    let mut immediate_eventai = Vec::new();
    let mut despawn_templates = BTreeSet::new();
    for row in catalog_event_ai_rows(&mut catalog).unwrap() {
        for slot in 0..3 {
            let at = 12 + slot * 4;
            // GetUInt16 action narrowing precedes dispatch, CreatureEventAIMgr.cpp:567.
            if row.words[at] as u16 != 41 {
                continue;
            }
            let delay = row.words[at + 1];
            let selector = row.words[1] as i32;
            let entry = if selector > 0 {
                Some(selector as u32)
            } else {
                spawns.get(&selector.unsigned_abs()).copied()
            };
            let item = serde_json::json!({"row":row.words[0], "slot":slot+1, "delay_ms":delay, "entry":entry});
            if delay != 0 {
                if let Some(entry) = entry {
                    despawn_templates.insert(entry);
                }
                delayed_eventai.push(item);
            } else {
                immediate_eventai.push(item);
            }
        }
    }
    let groups = catalog
        .unit_event_census_operands("spawn_group", ["Id", "Type"])
        .unwrap()
        .into_iter()
        .map(|(_, [id, kind])| (id, kind))
        .collect::<BTreeMap<_, _>>();
    let mut db_delayed = Vec::new();
    let mut db_immediate = 0;
    let mut db_scripts = BTreeSet::new();
    let mut group_delayed = Vec::new();
    let mut total_db_rows = 0;
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
        for (ordinal, [id, command, a, b]) in catalog
            .unit_event_census_operands(table, ["id", "command", "datalong", "datalong2"])
            .unwrap()
        {
            total_db_rows += 1;
            // ScriptMgr.cpp:2257-2269,3305-3313; ScriptMgr.h:143,483-487.
            if command == 18 {
                if a == 0 {
                    db_immediate += 1;
                } else {
                    db_scripts.insert((table, id));
                    db_delayed.push(
                        serde_json::json!({"table":table,"ordinal":ordinal,"id":id,"delay_ms":a}),
                    );
                }
            } else if command == 58 && b != 0 {
                group_delayed.push(serde_json::json!({"table":table,"ordinal":ordinal,"id":id,
                    "group":a,"group_kind":groups.get(&a),"delay_ms":b}));
            }
        }
    }
    let spells = catalog
        .execution_event_ai_operands(
            "spell_template",
            ["Id", "procFlags", "Effect1", "Effect2", "Effect3"],
        )
        .unwrap();
    let proc_spells = spells
        .iter()
        .filter(|(_, words)| words[1] & 1 != 0)
        .map(|(_, words)| words[0])
        .collect::<Vec<_>>();
    // Hardcoded delayed sites still stop at their unimplemented effect/aura owner.
    // SpellEffects.cpp:419-425,650-656,1230-1237; SpellAuras.cpp:1521-1527,2247-2255.
    let delayed_spell_templates = spells
        .iter()
        .filter(|(_, words)| [9976, 17678, 28000, 28819, 24937].contains(&words[0]))
        .map(|(_, words)| words[0])
        .collect::<Vec<_>>();
    let mut proc_rows = Vec::new();
    catalog
        .execution_spell_scan("spell_proc_event", |ordinal, values| {
            // Proc flags are column 6, SpellMgr.cpp:695,724.
            if values[6].as_u64().unwrap() as u32 & 1 != 0 {
                proc_rows.push((ordinal, values[0].as_u64().unwrap()));
            }
            Ok(())
        })
        .unwrap();
    let mut aura_scripts = Vec::new();
    catalog.execution_spell_script_scan(|ordinal, spell, name| {
        // The only selected AuraScript heartbeat overrides in the pinned tree:
        // world/spell_scripts.cpp:160-173,456-457.
        if matches!(name, b"spell_food_animation" | b"spell_drink_animation") {
            aura_scripts.push(serde_json::json!({"ordinal":ordinal,"spell":spell,"script":String::from_utf8_lossy(name)}));
        }
        Ok(())
    }).unwrap();
    assert_eq!(templates.len(), 10384);
    assert_eq!(spells.len(), 22374);
    assert_eq!(total_db_rows, 7166);
    let report = serde_json::json!({
        "catalog":"catalog-04", "counting":"static call candidates, not recorded dispatch or gameplay admission",
        "eventai_delayed_action_slots":delayed_eventai.len(), "eventai_delayed_templates":despawn_templates.len(),
        "eventai_delayed_rows":delayed_eventai.iter().map(|v| v["row"].as_u64().unwrap()).collect::<BTreeSet<_>>().len(),
        "eventai_immediate_action_slots":immediate_eventai.len(), "eventai_delayed":delayed_eventai,
        "dbscript_delayed_rows":db_delayed.len(),"dbscript_delayed_scripts":db_scripts.len(),
        "dbscript_immediate_rows":db_immediate,"dbscript_delayed":db_delayed,
        "dbscript_group_delayed_rows":group_delayed.len(),"dbscript_group_delayed":group_delayed,
        "hardcoded_delayed_spell_templates":delayed_spell_templates.len(),
        "hardcoded_delayed_spell_ids":delayed_spell_templates,
        "heartbeat_script_names":aura_scripts.iter().map(|v| v["script"].as_str().unwrap()).collect::<BTreeSet<_>>().len(),
        "heartbeat_scriptdev_template_constructor_refusals":templates.iter().filter(|t| !t.script_name.is_empty()).count(),
        "heartbeat_potential_creature_templates":templates.len(),
        "heartbeat_potential_spawn_rows":spawns.len(),
        "heartbeat_sql_proc_flag_spells":proc_spells.len(),"heartbeat_sql_proc_flag_ids":proc_spells,
        "heartbeat_proc_event_rows":proc_rows.len(),"heartbeat_proc_event_ids":proc_rows,
        "heartbeat_script_bindings":aura_scripts.len(),"heartbeat_script_bindings_rows":aura_scripts,
        "now_executes":"direct delayed scheduling, due forced-despawn calls, abort/delete and heartbeat control flow",
        "end_to_end_catalog_rows_now_admitted":0,
        "refuses_at_owner":{"eventai":"existing action-41 execution owner", "dbscripts":"F5.20 script dispatch and live target owner",
            "unit_update":"F5.08 cooldown/spells before events", "heartbeat":"F5.12 aura holder owner; F5.14 proc; selected AI callback"},
        "new_undefined_source_sites":0,"new_undefined_stock_reach":0,
        "native_format":91,"reference_execution_sealed":false
    });
    println!(
        "unit_events_census {}",
        serde_json::to_string(&report).unwrap()
    );
    if let Some(path) = std::env::var_os("BENILLA_F5_UNIT_EVENTS_CENSUS") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
