use super::*;
use std::collections::BTreeMap;
struct Unbound;
impl MotionExecutionOwner for Unbound {}
impl UnitAiExecutionOwner for Unbound {}
#[test]
fn catalog04_motion_defaults_and_eventai_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP F5 motion catalog census: BENILLA_F5_AI_CATALOG absent");
        return;
    };
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let config_path =
        std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").expect("recorded config required");
    let config = EventAiLoaderConfig::recorded(
        Path::new(&config_path),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    let template_motion = catalog
        .execution_event_ai_operands("creature_template", ["Entry", "MovementType"])
        .unwrap();
    let mut defined_templates = 0;
    let mut template_refusals = BTreeMap::<String, Vec<u32>>::new();
    for (_, [entry, raw]) in &template_motion {
        // ObjectMgr.cpp:533-536 repairs invalid template movement to Idle.
        let movement = if *raw >= 5 { 0 } else { *raw };
        match source_default_motion(movement, false) {
            Ok(0) => defined_templates += 1,
            Ok(_) => panic!("unexpected default"),
            Err(e) => template_refusals.entry(e.id).or_default().push(*entry),
        }
    }
    assert_eq!(template_motion.len(), 10384);
    let spawn_motion = catalog.execution_motion_spawn_operands().unwrap();
    let mut defined_spawns = 0;
    let mut spawn_refusals = BTreeMap::<String, Vec<u32>>::new();
    let mut radius_repairs = 0;
    let mut spawn_type_undefined = 0;
    for (guid, _, raw, radius) in &spawn_motion {
        let mut movement = u32::from(*raw);
        // ObjectMgr.cpp:1998-2008 has an else-if: a negative radius does not
        // also turn random movement into idle in this same loader pass.
        if *radius >= 0. && movement == 1 && *radius == 0. {
            movement = 0;
            radius_repairs += 1;
        }
        let default = match MotionDefault::from_spawn(movement as u8, 0) {
            Ok(default) => default,
            Err(e) => {
                spawn_type_undefined += 1;
                spawn_refusals.entry(e.id).or_default().push(*guid);
                continue;
            }
        };
        match source_default_motion(default.movement_type, false) {
            Ok(0) => defined_spawns += 1,
            Ok(_) => panic!("unexpected default"),
            Err(e) => spawn_refusals.entry(e.id).or_default().push(*guid),
        }
    }
    let templates = catalog_creature_ai_census_inputs(&mut catalog)
        .unwrap()
        .into_iter()
        .map(|t| (t.entry, t))
        .collect::<BTreeMap<_, _>>();
    let spawn_entries = spawn_motion
        .iter()
        .map(|(g, e, _, _)| (*g, *e))
        .collect::<BTreeMap<_, _>>();
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    let mut loader = EventAiCatalog::new(&mut catalog, config.clone());
    let mut loaded = 0;
    let mut root_rows = Vec::new();
    let mut other_refusals = BTreeMap::<String, usize>::new();
    let mut motion_actions = BTreeMap::<u16, usize>::new();
    let mut motion_dependent_rows = 0;
    for row in rows {
        let event = match load_event_ai_row(&mut loader, row) {
            Ok(EventAiLoad {
                disposition: EventAiDisposition::Loaded(e),
                ..
            }) => e,
            Err(e) => {
                *other_refusals.entry(e.id).or_default() += 1;
                continue;
            }
            Ok(_) => panic!("unexpected loader skip"),
        };
        loaded += 1;
        let dependent = event
            .actions
            .iter()
            .filter(|a| event_ai_action_owner(a.kind) == "execution.event_ai.f5_05_motion_owner")
            .map(|a| a.kind)
            .collect::<std::collections::BTreeSet<_>>();
        if !dependent.is_empty() {
            motion_dependent_rows += 1;
        }
        for kind in dependent {
            *motion_actions.entry(kind).or_default() += 1;
        }
        let entry = if event.entry != 0 {
            event.entry
        } else {
            spawn_entries[&event.guid]
        };
        let t = &templates[&entry];
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let mut units = SourceUnits::default();
        units
            .register(&objects, unit, SourceUnitFields::creature(t))
            .unwrap();
        // Isolated base-constructor probe, as in F5.04b. This does not supply
        // missing live factory motion operands or pretend root output succeeds.
        let mut motions = SourceMotions::default();
        motions
            .construct(
                &objects,
                unit,
                MotionDefault {
                    movement_type: 0,
                    default_path: 0,
                },
            )
            .unwrap();
        let mut missing = Unbound;
        let mut owner = SourceMotionDependencies {
            motions: &mut motions,
            owner: &mut missing,
        };
        match SourceCreatureAi::construct(
            &objects, unit, &mut units, t, false, 0, &config, &mut owner,
        ) {
            Ok(_) => {}
            Err(e) if e.id == "execution.motion.root_movement_info_output_owner" => {
                assert_eq!(
                    units.get(&objects, unit).unwrap().state & (UNIT_STATE_AI_ROOT | 0x10),
                    UNIT_STATE_AI_ROOT | 0x10
                );
                root_rows.push(event.id);
            }
            Err(e) => {
                *other_refusals.entry(e.id).or_default() += 1;
            }
        }
    }
    assert_eq!(loaded, 11126);
    assert_eq!(root_rows.len(), 39);
    assert_eq!(motion_dependent_rows, 1562);
    assert_eq!(
        spawn_type_undefined, 0,
        "normal stock undefined conversion must stop before commit"
    );
    let report = serde_json::json!({
        "catalog_sha256": catalog.report()["catalog_sha256"],
        "scope": "raw ordinary uncontrolled template and static spawn alternatives; not group/event substitutions, admitted live factories or F1 evidence",
        "templates": {"total": template_motion.len(), "defined_idle": defined_templates, "refusal_entries": template_refusals},
        "spawns": {"total": spawn_motion.len(), "defined_idle": defined_spawns, "radius_zero_random_to_idle": radius_repairs, "refusal_guids": spawn_refusals},
        "eventai": {"loaded": loaded, "past_f5_05_immobilization": root_rows.len(), "root_rows": root_rows, "next_refusal": "root_movement_info_output_owner", "motion_action_dependent_rows": motion_dependent_rows, "motion_action_types_nonexclusive": motion_actions, "new_end_to_end_rows": 0, "other_motion_action_rows_fully_unblocked": 0, "other_constructor_or_loader_refusals": other_refusals},
        "action64": {"rows_with_owned_immobilization_body": motion_actions.get(&64).copied().unwrap_or(0), "remaining_owner": "root_movement_info_output_owner", "end_to_end_rows": 0},
        "undefined": {"spawn_default_enum_conversion_candidates": spawn_type_undefined, "pre_initialize_path_reads_in_this_ordinary_creature_census": 0, "live_player_possession_path_read_reach": "unavailable; path getter refuses only at the actual read"},
        "format": 91, "format_change": false, "F1": "frozen unsealed"
    });
    if let Some(path) = std::env::var_os("BENILLA_F5_MOTION_CENSUS") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    let summary = serde_json::json!({
        "templates_total":template_motion.len(), "templates_defined":defined_templates,
        "template_refusals":template_refusals.iter().map(|(k,v)| (k,v.len())).collect::<BTreeMap<_,_>>(),
        "spawns_total":spawn_motion.len(), "spawns_defined":defined_spawns,
        "spawn_refusals":spawn_refusals.iter().map(|(k,v)| (k,v.len())).collect::<BTreeMap<_,_>>(),
        "radius_repairs":radius_repairs,"past_f5_05":root_rows.len(),"motion_rows":motion_dependent_rows,
        "motion_action_types":motion_actions,"undefined_spawn_types":spawn_type_undefined
    });
    println!(
        "F5_MOTION_CENSUS {}",
        serde_json::to_string(&summary).unwrap()
    );
}
