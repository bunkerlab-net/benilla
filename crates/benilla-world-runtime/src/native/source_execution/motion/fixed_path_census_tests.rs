use super::super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

#[test]
fn catalog04_fixed_and_linear_defaults_and_raw_prefixes_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP fixed/linear catalog census: BENILLA_F5_AI_CATALOG absent");
        return;
    };
    let mut catalog =
        super::super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false)
            .unwrap();
    let spawns = catalog.execution_motion_spawn_operands().unwrap();
    let template_motion = catalog
        .execution_event_ai_operands("creature_template", ["Entry", "MovementType"])
        .unwrap();
    let templates = catalog_creature_ai_census_inputs(&mut catalog)
        .unwrap()
        .into_iter()
        .map(|t| (t.entry, t))
        .collect::<BTreeMap<_, _>>();
    let creature_ids = spawns.iter().map(|s| s.guid).collect::<BTreeSet<_>>();
    let template_ids = templates.keys().copied().collect::<BTreeSet<_>>();
    let raw = [
        catalog
            .execution_waypoint_rows("creature_movement")
            .unwrap(),
        catalog
            .execution_waypoint_rows("creature_movement_template")
            .unwrap(),
        catalog.execution_waypoint_rows("waypoint_path").unwrap(),
    ];
    let global_refusal =
        SourceWaypointPaths::load_catalog(&mut catalog, &creature_ids, &template_ids, None)
            .err()
            .unwrap();
    assert_eq!(
        global_refusal.id,
        "execution.waypoint.scriptmgr_dbscript_loaded_maps_owner"
    );
    // Diagnostic selection only. Actual node loading remains SourceWaypointPaths::load.
    // WaypointManager.h:69-91 and .cpp:496-498 retain the waypoint blacklist quirk.
    let mut groups: [BTreeMap<u32, Vec<SourceWaypointRow>>; 3] = Default::default();
    let mut blacklist: [BTreeSet<u32>; 3] = Default::default();
    for (family, rows) in raw.iter().enumerate() {
        for row in rows {
            let key = if family == 1 {
                row.owner.wrapping_shl(8).wrapping_add(row.path)
            } else {
                row.owner
            };
            if row.point == 0 {
                blacklist[family].insert(key);
            }
            if (family == 0 && !creature_ids.contains(&row.owner))
                || (family == 1 && !template_ids.contains(&row.owner))
            {
                continue;
            }
            groups[family].entry(key).or_default().push(*row);
        }
    }
    for key in &blacklist[0] {
        groups[0].remove(key);
    }
    for key in blacklist[1].union(&blacklist[2]) {
        groups[1].remove(key);
    }
    let select = |entry: u32, guid: u32| {
        [
            (0, guid),
            (1, entry.wrapping_shl(8)),
            (2, entry.wrapping_shl(8)),
        ]
        .into_iter()
        .find_map(|(f, k)| groups[f].get(&k).map(|r| (f, r)))
    };
    let alternatives =
        super::super::motion_census_tests::MotionEntryAlternatives::load(&mut catalog);
    struct RawOwner {
        paths: Rc<SourceWaypointPaths>,
    }
    impl MotionExecutionOwner for RawOwner {
        fn waypoint_paths(&mut self) -> Result<Rc<SourceWaypointPaths>> {
            Ok(self.paths.clone())
        }
        fn waypoint_casting_for_movement(&mut self, _: ObjectHandle) -> Result<bool> {
            Ok(false)
        }
    }
    let mut template_counts = [0usize; 5];
    let mut spawn_counts = [0usize; 5];
    for (_, [_, kind]) in &template_motion {
        template_counts[if *kind < 5 { *kind as usize } else { 0 }] += 1;
    }
    for spawn in &spawns {
        spawn_counts[if spawn.radius == 0. && spawn.movement_type == 1 {
            0
        } else {
            usize::from(spawn.movement_type)
        }] += 1;
    }
    assert_eq!(template_counts, [7000, 3163, 209, 3, 9]);
    assert_eq!(spawn_counts, [20013, 43119, 2931, 38, 335]);
    let mut reasons = BTreeMap::<u8, BTreeMap<String, Vec<(u32, u32)>>>::new();
    let mut spawn_sets = BTreeMap::<u8, BTreeMap<String, BTreeSet<u32>>>::new();
    let mut template_reasons = BTreeMap::<u32, BTreeMap<String, Vec<u32>>>::new();
    let mut undefined = Vec::new();
    let mut one_node_linear_paths = Vec::new();
    for (_, [entry, kind]) in &template_motion {
        if !matches!(kind, 3 | 4) {
            continue;
        }
        let reason = match select(*entry, 0) {
            None => "raw_missing_path",
            Some((_, rows)) if rows.iter().any(|r| r.node.script != 0) => {
                "raw_scriptmgr_loaded_maps_dependency"
            }
            Some(_) => "raw_script_free_path",
        };
        template_reasons
            .entry(*kind)
            .or_default()
            .entry(reason.into())
            .or_default()
            .push(*entry);
    }
    for spawn in spawns.iter().filter(|s| matches!(s.movement_type, 3 | 4)) {
        let entries = alternatives.resolve(spawn.guid, spawn.entry);
        assert!(!entries.is_empty(), "unresolved stock entry {}", spawn.guid);
        for entry in entries {
            let selected = select(entry, spawn.guid);
            let reason = if !templates.contains_key(&entry) {
                "raw_unresolved_entry".to_owned()
            } else if selected.is_some() || spawn.movement_type == 3 {
                let mut selected_rows: [Vec<SourceWaypointRow>; 3] = Default::default();
                if let Some((family, rows)) = selected {
                    if spawn.movement_type == 4
                        && rows.iter().map(|r| r.point).collect::<BTreeSet<_>>().len() == 1
                    {
                        one_node_linear_paths.push((spawn.guid, entry));
                    }
                    selected_rows[family] = rows.clone();
                }
                // Fixed Initialize queries the old spline even when its selected
                // path is absent, PathMovementGenerator.cpp:63 before :72.
                match SourceWaypointPaths::load(selected_rows, &creature_ids, &template_ids, None) {
                    Err(e) => e.id,
                    Ok(paths) => {
                        let mut objects = SourceObjects::default();
                        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
                        objects
                            .assign_guid(
                                unit,
                                spawn.guid,
                                SourceGuid::with_entry(HighGuid::Creature, entry, spawn.guid),
                            )
                            .unwrap();
                        objects
                            .set_pose(unit, SourcePose(spawn.pose.map(f32::to_bits)))
                            .unwrap();
                        let mut units = SourceUnits::default();
                        units
                            .register(
                                &objects,
                                unit,
                                SourceUnitFields::creature(&templates[&entry]),
                            )
                            .unwrap();
                        let mut master = SourceMotionMaster::new(
                            unit,
                            MotionDefault {
                                movement_type: spawn.movement_type.into(),
                                default_path: 0,
                            },
                        );
                        let mut context = MotionContext {
                            objects: &objects,
                            units: &mut units,
                        };
                        let mut owner = RawOwner {
                            paths: Rc::new(paths),
                        };
                        let error = if spawn.movement_type == 3 {
                            let g = SourceFixedPathGenerator::load(
                                &master,
                                &context,
                                &mut owner,
                                0,
                                WaypointOrigin::None,
                                FixedPathOptions {
                                    flying: true,
                                    cyclic: true,
                                    ..Default::default()
                                },
                            )
                            .unwrap();
                            g.initialize(&mut master, &mut context, &mut owner)
                                .unwrap_err()
                        } else {
                            let g = SourceWaypointGenerator::linear();
                            g.initialize(&mut master, &mut context, &mut owner).unwrap();
                            g.initialize_path(
                                &mut master,
                                &mut context,
                                &mut owner,
                                WaypointPathRequest::default(),
                            )
                            .unwrap_err()
                        };
                        if error.id.ends_with("_undefined") {
                            undefined.push((spawn.guid, entry, error.id.clone()));
                        }
                        error.id
                    }
                }
            } else {
                "raw_missing_path".into()
            };
            reasons
                .entry(spawn.movement_type)
                .or_default()
                .entry(reason.clone())
                .or_default()
                .push((spawn.guid, entry));
            spawn_sets
                .entry(spawn.movement_type)
                .or_default()
                .entry(reason)
                .or_default()
                .insert(spawn.guid);
        }
    }
    assert!(
        undefined.is_empty(),
        "stock UB must stop before commit: {undefined:?}"
    );
    let report = serde_json::json!({
        "catalog_sha256":catalog.report()["catalog_sha256"], "scope":"raw ordinary defaults and retained effective-entry alternatives; not a live formation/event choice, loaded global manager, spline or F1 execution",
        "movement_order":["idle","wander","waypoint","fixed_path","linear_waypoint"], "templates_by_movement":template_counts, "spawns_by_movement":spawn_counts,
        "before":{"templates_defined":10372,"spawns_defined":66063,"fixed_templates_missing":3,"fixed_spawns_missing":38,"linear_templates_missing":9,"linear_spawns_missing":335},
        "after":{"templates_defined":10384,"spawns_defined":66436,"missing_constructors":0,"global_loader_refusal":global_refusal.id,"global_loader_reached_f6":0,"global_loader_blocked_spawns":373},
        "raw_template_reasons":template_reasons,"raw_alternative_reasons":reasons,"raw_spawn_sets_nonexclusive":spawn_sets,"undefined_prefix_rows":undefined,"raw_one_node_linear_paths":one_node_linear_paths,
        "later_undefined_sites":"linear --begin at WaypointMovementGenerator.cpp:624; returning ++begin end dereference :639,252; fixed offset iterator :84,100 and signed index addition :159,168,198. Arrival live reach requires F6 and is not claimed.",
        "format":91,"persisted_state_changed":false,"F1":"frozen unsealed"
    });
    if let Some(path) = std::env::var_os("BENILLA_F5_FIXED_PATH_CENSUS") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!(
        "F5_FIXED_LINEAR_CENSUS {}",
        serde_json::json!({"templates":template_counts,"spawns":spawn_counts,"reasons":reasons.iter().map(|(k, v)| (k, v.iter().map(|(r, rows)| (r, rows.len())).collect::<BTreeMap<_,_>>())).collect::<BTreeMap<_,_>>(),"spawn_sets":spawn_sets.iter().map(|(k,v)| (k,v.iter().map(|(r,rows)| (r,rows.len())).collect::<BTreeMap<_,_>>())).collect::<BTreeMap<_,_>>(),"one_node_linear_paths":one_node_linear_paths,"undefined_prefix_rows":undefined})
    );
}
