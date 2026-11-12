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
    let mut wander_templates = Vec::new();
    let mut waypoint_templates = Vec::new();
    for (_, [entry, raw]) in &template_motion {
        // ObjectMgr.cpp:533-536 repairs invalid template movement to Idle.
        let movement = if *raw >= 5 { 0 } else { *raw };
        match source_default_motion(movement, false) {
            Ok(kind) => {
                defined_templates += 1;
                if kind == 1 {
                    wander_templates.push(*entry);
                }
                if kind == 2 {
                    waypoint_templates.push(*entry);
                }
            }
            Err(e) => template_refusals.entry(e.id).or_default().push(*entry),
        }
    }
    assert_eq!(template_motion.len(), 10384);
    let template_defaults = template_motion
        .iter()
        .map(|(_, [entry, kind])| (*entry, *kind))
        .collect::<BTreeMap<_, _>>();
    let spawn_motion = catalog.execution_motion_spawn_operands().unwrap();
    let mut defined_spawns = 0;
    let mut spawn_refusals = BTreeMap::<String, Vec<u32>>::new();
    let mut radius_repairs = 0;
    let mut spawn_type_undefined = 0;
    let mut wander_spawns = Vec::new();
    let mut waypoint_spawns = Vec::new();
    let mut negative_radius_repairs = 0;
    let templates = catalog_creature_ai_census_inputs(&mut catalog)
        .unwrap()
        .into_iter()
        .map(|t| (t.entry, t))
        .collect::<BTreeMap<_, _>>();
    let alternatives = MotionEntryAlternatives::load(&mut catalog);
    let mut wander_constructor_alternatives = 0;
    let mut wander_constructed_spawns = Vec::new();
    let mut wander_missing_effective_template = BTreeMap::new();
    for spawn in &spawn_motion {
        let (guid, entry, raw, radius, pose) = (
            &spawn.guid,
            &spawn.entry,
            &spawn.movement_type,
            &spawn.radius,
            &spawn.pose,
        );
        let mut movement = u32::from(*raw);
        // ObjectMgr.cpp:1998-2008 has an else-if: a negative radius does not
        // also turn random movement into idle in this same loader pass.
        if *radius >= 0. && movement == 1 && *radius == 0. {
            movement = 0;
            radius_repairs += 1;
        }
        let radius = if *radius < 0. {
            negative_radius_repairs += 1;
            0.
        } else {
            *radius
        };
        let default = match MotionDefault::from_spawn(movement as u8, 0) {
            Ok(default) => default,
            Err(e) => {
                spawn_type_undefined += 1;
                spawn_refusals.entry(e.id).or_default().push(*guid);
                continue;
            }
        };
        match source_default_motion(default.movement_type, false) {
            Ok(kind) => {
                defined_spawns += 1;
                if kind == 2 {
                    waypoint_spawns.push(*guid);
                }
                if kind == 1 {
                    wander_spawns.push(*guid);
                    let entries = alternatives.resolve(*guid, *entry);
                    if entries.is_empty() || entries.iter().any(|e| !templates.contains_key(e)) {
                        let failure = refuse("execution.motion.missing_effective_template_binding");
                        wander_missing_effective_template.insert(*guid, (failure, *entry, entries));
                        continue;
                    }
                    for effective_entry in &entries {
                        let mut objects = SourceObjects::default();
                        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
                        objects
                            .set_respawn_definition(unit, SourcePose(pose.map(f32::to_bits)), true)
                            .unwrap();
                        let mut units = SourceUnits::default();
                        units
                            .register(
                                &objects,
                                unit,
                                SourceUnitFields::creature(&templates[effective_entry]),
                            )
                            .unwrap();
                        let mut master = SourceMotionMaster::new(unit, default);
                        master.bind_respawn_radius(radius);
                        let mut context = MotionContext {
                            objects: &objects,
                            units: &mut units,
                        };
                        let failure = master.initialize(&mut context, &mut Unbound).unwrap_err();
                        assert_eq!(
                            failure.id,
                            "execution.motion.f6_wander_pathfinder_constructor_owner"
                        );
                        assert_eq!(master.current_type(), 1);
                        assert_eq!(
                            context.units.get(&objects, unit).unwrap().state & WANDER_ACTIVE,
                            WANDER_ACTIVE
                        );
                        wander_constructor_alternatives += 1;
                    }
                    wander_constructed_spawns.push(*guid);
                    spawn_refusals
                        .entry("execution.motion.f6_wander_pathfinder_constructor_owner".into())
                        .or_default()
                        .push(*guid);
                }
            }
            Err(e) => spawn_refusals.entry(e.id).or_default().push(*guid),
        }
    }
    let spawn_entries = spawn_motion
        .iter()
        .map(|spawn| (spawn.guid, spawn.entry))
        .collect::<BTreeMap<_, _>>();
    let spawn_defaults = spawn_motion
        .iter()
        .map(|spawn| {
            (
                spawn.guid,
                if spawn.radius == 0. && spawn.movement_type == 1 {
                    0
                } else {
                    u32::from(spawn.movement_type)
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    let mut loader = EventAiCatalog::new(&mut catalog, config.clone());
    let mut loaded = 0;
    let mut root_rows = Vec::new();
    let mut other_refusals = BTreeMap::<String, usize>::new();
    let mut motion_actions = BTreeMap::<u16, usize>::new();
    let mut motion_dependent_rows = 0;
    let mut motion_rows_with_wander_default = Vec::new();
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
        let motion_dependent = !dependent.is_empty();
        if motion_dependent {
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
        // This is a raw default-alternative gate change, not action dispatch.
        let default = if event.entry != 0 {
            template_defaults[&entry]
        } else {
            spawn_defaults[&event.guid]
        };
        if motion_dependent && default == 1 {
            motion_rows_with_wander_default.push(event.id);
        }
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
    assert_eq!(wander_templates.len(), 3163);
    assert_eq!(wander_spawns.len(), 43119);
    assert_eq!(waypoint_templates.len(), 209);
    assert_eq!(waypoint_spawns.len(), 2931);
    assert_eq!(defined_templates, 10384);
    assert_eq!(defined_spawns, 66436);
    assert_eq!(
        spawn_type_undefined, 0,
        "normal stock undefined conversion must stop before commit"
    );
    let report = serde_json::json!({
        "catalog_sha256": catalog.report()["catalog_sha256"],
        "scope": "raw ordinary uncontrolled template and static spawn alternatives; not group/event substitutions, admitted live factories or F1 evidence",
        "templates": {"total": template_motion.len(), "generator_defined": defined_templates, "before_generator_defined": 10372, "wander_generator_defined_entries": wander_templates, "missing_constructor_entries": template_refusals},
        "spawns": {"total": spawn_motion.len(), "generator_defined": defined_spawns, "before_generator_defined": 66063, "wander_raw_family_guids": wander_spawns, "wander_constructed_f6_refusal_guids": wander_constructed_spawns, "missing_effective_template_bindings": wander_missing_effective_template, "radius_negative_to_zero": negative_radius_repairs, "radius_zero_random_to_idle": radius_repairs, "refusal_guids": spawn_refusals},
        "wander_constructor_alternatives": wander_constructor_alternatives,
        "waypoint_raw_factory_availability": {
            "templates": waypoint_templates, "spawns": waypoint_spawns,
            "before_templates_defined": 10163,
            "before_spawns_defined": 63132,
            "note": "Factory availability only. Global loader and raw selected-path dependencies are reported by the separate waypoint census."
        },
        "entry_scope": "retained random-loader, group/squad, conditional and event alternatives; no live group/condition/event selection or RNG choice is claimed",
        "eventai": {"loaded": loaded, "past_f5_05_immobilization": root_rows.len(), "root_rows": root_rows, "next_refusal": "root_movement_info_output_owner", "motion_action_dependent_rows": motion_dependent_rows, "motion_action_types_nonexclusive": motion_actions, "new_end_to_end_rows": 0, "other_motion_action_rows_fully_unblocked": 0, "other_constructor_or_loader_refusals": other_refusals},
        "wander_eventai_status": {"motion_dependent_rows": motion_dependent_rows, "raw_default_constructor_gate_changed_rows": motion_rows_with_wander_default, "changed_dispatch_status_rows": 0, "new_end_to_end_rows": 0, "reason": "default constructor advances to F6; action dependencies and real invocation bindings remain unavailable"},
        "action64": {"rows_with_owned_immobilization_body": motion_actions.get(&64).copied().unwrap_or(0), "remaining_owner": "root_movement_info_output_owner", "end_to_end_rows": 0},
        "undefined": {"spawn_default_enum_conversion_candidates": spawn_type_undefined, "pre_initialize_path_reads_in_this_ordinary_creature_census": 0, "live_player_possession_path_read_reach": "unavailable; path getter refuses only at the actual read"},
        "wander_undefined": {"new_undefined_sites": 0, "stock_undefined_reach": 0, "pause_arithmetic": "unsigned addition wraps; pinned uint32-to-int32 conversion is implementation-defined, not undefined; boundary tests preserve it"},
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
        "wander_templates":wander_templates.len(), "wander_spawns_raw_family":wander_spawns.len(),
        "wander_spawns_constructed_f6":wander_constructed_spawns.len(),
        "wander_constructor_alternatives":wander_constructor_alternatives,
        "wander_missing_effective_template_bindings":wander_missing_effective_template.len(),
        "before_templates_defined":10372,
        "before_spawns_defined":66063,
        "motion_rows_changed_status":0, "negative_radius_repairs":negative_radius_repairs,
        "motion_rows_raw_wander_default_gate_changed":motion_rows_with_wander_default.len(),
        "motion_action_types":motion_actions,"undefined_spawn_types":spawn_type_undefined
    });
    println!(
        "F5_MOTION_CENSUS {}",
        serde_json::to_string(&summary).unwrap()
    );
}

type EntrySet = std::collections::BTreeSet<u32>;
#[derive(Default)]
pub(super) struct MotionEntryAlternatives {
    random: BTreeMap<u32, EntrySet>,
    group: BTreeMap<u32, EntrySet>,
    conditional: BTreeMap<u32, EntrySet>,
    event: BTreeMap<u32, EntrySet>,
}
impl MotionEntryAlternatives {
    /// Alternatives only, not loader acceptance, team/event state or random choice.
    pub(super) fn load(catalog: &mut super::super::production_compiler::VerifiedCatalog) -> Self {
        let mut this = Self::default();
        for (_, [guid, entry]) in catalog
            .motion_census_operands("creature_spawn_entry", ["guid", "entry"])
            .unwrap()
        {
            this.random.entry(guid).or_default().insert(entry);
        }
        let creature_groups = catalog
            .motion_census_operands("spawn_group", ["Id", "Type"])
            .unwrap()
            .into_iter()
            .filter(|(_, [_, kind])| *kind == 0)
            .map(|(_, [id, _])| id)
            .collect::<EntrySet>();
        let mut group_entries = BTreeMap::<u32, EntrySet>::new();
        // ObjectMgr.cpp:1355-1394,1426-1460; SpawnGroupDefines.h:63-67.
        for (_, [group, entry]) in catalog
            .motion_census_operands("spawn_group_entry", ["Id", "Entry"])
            .unwrap()
        {
            if creature_groups.contains(&group) {
                group_entries.entry(group).or_default().insert(entry);
            }
        }
        let mut squads = BTreeMap::<(u32, u32), EntrySet>::new();
        let mut squad_groups = EntrySet::new();
        for (_, [group, guid, entry]) in catalog
            .motion_census_operands("spawn_group_squad", ["Id", "Guid", "Entry"])
            .unwrap()
        {
            if creature_groups.contains(&group) {
                squad_groups.insert(group);
                squads.entry((group, guid)).or_default().insert(entry);
            }
        }
        // ObjectMgr.cpp:1250-1324; SpawnGroup.cpp:191-202. Squads override entries.
        for (_, [group, guid]) in catalog
            .motion_census_operands("spawn_group_spawn", ["Id", "Guid"])
            .unwrap()
        {
            if !creature_groups.contains(&group) {
                continue;
            }
            let entries = if squad_groups.contains(&group) {
                squads.get(&(group, guid))
            } else {
                this.random.get(&guid).or_else(|| group_entries.get(&group))
            };
            if let Some(entries) = entries {
                this.group.entry(guid).or_default().extend(entries);
            }
        }
        // Creature.cpp:3031-3056 retains both possible dungeon-team alternatives.
        for (_, [guid, alliance, horde]) in catalog
            .motion_census_operands(
                "creature_conditional_spawn",
                ["Guid", "EntryAlliance", "EntryHorde"],
            )
            .unwrap()
        {
            this.conditional
                .entry(guid)
                .or_default()
                .extend([alliance, horde]);
        }
        // GameEventMgr.cpp:410-418; Creature.cpp:322-324,1648-1650.
        for (_, [guid, entry]) in catalog
            .motion_census_operands("game_event_creature_data", ["guid", "entry_id"])
            .unwrap()
        {
            this.event.entry(guid).or_default().insert(entry);
        }
        this
    }
    pub(super) fn resolve(&self, guid: u32, raw_entry: u32) -> EntrySet {
        // ObjectMgr.cpp:1916-1932,1957,2048-2050 resolves random entries at load.
        // Creature.cpp:1634-1653 then uses group, conditional, random(0), event.
        let mut entries = EntrySet::from([raw_entry]);
        let mut needs_entry = entries.remove(&0);
        let random_loader = if self.conditional.contains_key(&guid) {
            None
        } else {
            self.random.get(&guid)
        };
        for candidates in [
            random_loader,
            self.group.get(&guid),
            self.conditional.get(&guid),
            self.random.get(&0),
        ] {
            if !needs_entry {
                break;
            }
            if let Some(candidates) = candidates {
                entries.extend(candidates.iter().copied().filter(|entry| *entry != 0));
                needs_entry = candidates.is_empty() || candidates.contains(&0);
            }
        }
        // Include inactive-event originals and each retained nonzero override.
        if let Some(events) = self.event.get(&guid) {
            entries.extend(events);
        }
        entries.remove(&0);
        entries
    }
}

#[test]
fn entry_census_random_loader_then_group_conditional_and_event_alternatives() {
    let mut alternatives = MotionEntryAlternatives::default();
    alternatives.random.insert(5, EntrySet::from([10, 11]));
    alternatives.group.insert(5, EntrySet::from([12]));
    alternatives.conditional.insert(5, EntrySet::from([13, 0]));
    alternatives.event.insert(5, EntrySet::from([14]));
    assert_eq!(alternatives.resolve(5, 9), EntrySet::from([9, 14]));
    assert_eq!(alternatives.resolve(5, 0), EntrySet::from([12, 14]));
    alternatives.group.clear();
    assert_eq!(alternatives.resolve(5, 0), EntrySet::from([13, 14]));
    alternatives.random.insert(0, EntrySet::from([20]));
    assert_eq!(alternatives.resolve(5, 0), EntrySet::from([13, 14, 20]));
    alternatives.conditional.clear();
    assert_eq!(alternatives.resolve(5, 0), EntrySet::from([10, 11, 14]));
}

#[test]
fn entry_census_unbound_entry_stays_empty_without_an_authored_template() {
    let mut alternatives = MotionEntryAlternatives::default();
    assert!(alternatives.resolve(5, 0).is_empty());
    alternatives.random.insert(0, EntrySet::from([20]));
    assert_eq!(alternatives.resolve(5, 0), EntrySet::from([20]));
    assert_eq!(alternatives.resolve(5, 999999), EntrySet::from([999999]));
}

/// Raw selected-path probes cannot bypass the global ScriptMgr loader refusal.
/// They bind only the selected table rows, not a live complete WaypointManager.
#[test]
fn catalog04_waypoint_loader_and_raw_prefix_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP waypoint catalog census: BENILLA_F5_AI_CATALOG absent");
        return;
    };
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let spawns = catalog.execution_motion_spawn_operands().unwrap();
    let templates = catalog_creature_ai_census_inputs(&mut catalog)
        .unwrap()
        .into_iter()
        .map(|t| (t.entry, t))
        .collect::<BTreeMap<_, _>>();
    let creature_ids = spawns.iter().map(|s| s.guid).collect::<EntrySet>();
    let template_ids = templates.keys().copied().collect::<EntrySet>();
    let raw = [
        catalog
            .execution_waypoint_rows("creature_movement")
            .unwrap(),
        catalog
            .execution_waypoint_rows("creature_movement_template")
            .unwrap(),
        catalog.execution_waypoint_rows("waypoint_path").unwrap(),
    ];
    let external = catalog.execution_waypoint_rows("script_waypoint").unwrap();
    let global =
        SourceWaypointPaths::load_catalog(&mut catalog, &creature_ids, &template_ids, None)
            .err()
            .unwrap();
    assert_eq!(
        global.id,
        "execution.waypoint.scriptmgr_dbscript_loaded_maps_owner"
    );
    struct MissingSpellOwners;
    impl SpellLoaderOwner for MissingSpellOwners {}
    let occurrences = std::env::var_os("BENILLA_F5_DB_SCRIPT_OCCURRENCES");
    let global_after = occurrences
        .as_ref()
        .ok_or_else(|| refuse("execution.db_scripts.sql_occurrences_owner"))
        .and_then(|path| {
            let spells = SourceSpellStores::load(&mut catalog, &mut MissingSpellOwners)?;
            let mut groups = catalog_group_census_owner(&mut catalog);
            let scripts = SourceDbScriptMaps::load_catalog(
                &mut catalog,
                Path::new(path),
                &[DbScriptType::Relay, DbScriptType::CreatureMovement],
                Some(&spells),
                &mut groups,
                None,
            )?;
            SourceWaypointPaths::load_catalog_with_db_scripts(
                &mut catalog,
                &creature_ids,
                &template_ids,
                &scripts,
            )
        });
    let global_after_refusal = global_after.as_ref().err().map(|e| e.id.as_str());
    if occurrences.is_some() {
        // Pre-terrain products reach the first unresolved startup random entry.
        let group_owner = if std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES").is_some() {
            "execution.effective_spawns.world_thread_startup_rng_position_owner"
        } else {
            "execution.spawn_groups.objectmgr_load_gameobjects_effective_spawn_owner"
        };
        assert_eq!(global_after_refusal, Some(group_owner));
    }
    let mut groups: [BTreeMap<u32, Vec<SourceWaypointRow>>; 3] = Default::default();
    let mut blacklists: [EntrySet; 3] = Default::default();
    for (family, rows) in raw.iter().enumerate() {
        for row in rows {
            let key = if family == 1 {
                row.owner.wrapping_shl(8).wrapping_add(row.path)
            } else {
                row.owner
            };
            if row.point == 0 {
                blacklists[family].insert(key);
            }
            if (family == 0 && !creature_ids.contains(&row.owner))
                || (family == 1 && !template_ids.contains(&row.owner))
            {
                continue;
            }
            groups[family].entry(key).or_default().push(*row);
        }
    }
    for key in &blacklists[0] {
        groups[0].remove(key);
    }
    for key in blacklists[1].union(&blacklists[2]) {
        groups[1].remove(key);
    }
    let alternatives = MotionEntryAlternatives::load(&mut catalog);
    let mut reasons = BTreeMap::<String, Vec<(u32, u32)>>::new();
    let mut spawn_reasons = BTreeMap::<String, EntrySet>::new();
    let mut template_reasons = BTreeMap::<String, Vec<u32>>::new();
    let mut origins = BTreeMap::<String, usize>::new();
    let template_motion = catalog
        .execution_event_ai_operands("creature_template", ["Entry", "MovementType"])
        .unwrap();
    let selected = |entry: u32, guid: u32| {
        [
            (0, guid),
            (1, entry.wrapping_shl(8)),
            (2, entry.wrapping_shl(8)),
        ]
        .into_iter()
        .find_map(|(family, key)| groups[family].get(&key).map(|rows| (family, rows)))
    };
    for (_, [entry, kind]) in &template_motion {
        if *kind != 2 {
            continue;
        }
        let reason = match selected(*entry, 0) {
            None => "raw_missing_path",
            Some((_, rows)) if rows.iter().any(|r| r.node.script != 0) => {
                "raw_scriptmgr_loaded_maps_dependency"
            }
            Some(_) => "raw_script_free_path",
        };
        template_reasons
            .entry(reason.into())
            .or_default()
            .push(*entry);
    }
    struct RawOwner {
        paths: std::rc::Rc<SourceWaypointPaths>,
    }
    impl MotionExecutionOwner for RawOwner {
        fn waypoint_paths(&mut self) -> Result<std::rc::Rc<SourceWaypointPaths>> {
            Ok(self.paths.clone())
        }
        // Prefix probe constructs a fresh Unit with null spell slots,
        // Unit.cpp:350-351. This is not a live casting-state default.
        fn waypoint_casting_for_movement(&mut self, _: ObjectHandle) -> Result<bool> {
            Ok(false)
        }
    }
    let mut alternatives_count = 0;
    let mut waypoint_spawns = 0;
    let mut undefined = Vec::new();
    for spawn in spawns.iter().filter(|s| s.movement_type == 2) {
        waypoint_spawns += 1;
        let entries = alternatives.resolve(spawn.guid, spawn.entry);
        if entries.is_empty() {
            reasons
                .entry("raw_unresolved_entry".into())
                .or_default()
                .push((spawn.guid, spawn.entry));
            spawn_reasons
                .entry("raw_unresolved_entry".into())
                .or_default()
                .insert(spawn.guid);
        }
        for entry in entries {
            alternatives_count += 1;
            let reason = if !templates.contains_key(&entry) {
                "raw_unresolved_entry".to_owned()
            } else if let Some((family, rows)) = selected(entry, spawn.guid) {
                *origins
                    .entry(["guid", "entry", "waypoint_path"][family].into())
                    .or_default() += 1;
                if rows.iter().any(|r| r.node.script != 0) {
                    "raw_scriptmgr_loaded_maps_dependency".into()
                } else {
                    let mut selected_rows: [Vec<SourceWaypointRow>; 3] = Default::default();
                    selected_rows[family] = rows.clone();
                    match SourceWaypointPaths::load(
                        selected_rows,
                        &creature_ids,
                        &template_ids,
                        None,
                    ) {
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
                                    movement_type: 2,
                                    default_path: 0,
                                },
                            );
                            let mut context = MotionContext {
                                objects: &objects,
                                units: &mut units,
                            };
                            let generator = SourceWaypointGenerator::default();
                            let mut owner = RawOwner {
                                paths: std::rc::Rc::new(paths),
                            };
                            generator
                                .initialize(&mut master, &mut context, &mut owner)
                                .unwrap();
                            let e = generator
                                .initialize_path(
                                    &mut master,
                                    &mut context,
                                    &mut owner,
                                    WaypointPathRequest::default(),
                                )
                                .unwrap_err();
                            if e.id.ends_with("_undefined") {
                                undefined.push((spawn.guid, entry, e.id.clone()));
                            }
                            e.id
                        }
                    }
                }
            } else {
                "raw_missing_path".into()
            };
            reasons
                .entry(reason.clone())
                .or_default()
                .push((spawn.guid, entry));
            spawn_reasons.entry(reason).or_default().insert(spawn.guid);
        }
    }
    assert_eq!(waypoint_spawns, 2931);
    assert_eq!(template_reasons.values().map(Vec::len).sum::<usize>(), 209);
    assert!(
        undefined.is_empty(),
        "stock undefined prefix rows must stop before commit: {undefined:?}"
    );
    let report = serde_json::json!({
        "catalog_sha256":catalog.report()["catalog_sha256"],
        "scope":"global manager loader and separately raw ordinary default-path probes; no complete loader acceptance, live formation/event choice, spline, arrival or F1 execution is claimed",
        "before":{"waypoint_templates_missing_constructor":209,"waypoint_spawns_missing_constructor":2931,"templates_generator_defined":10163,"spawns_generator_defined":63132},
        "after":{"waypoint_templates_factory_defined":209,"waypoint_spawns_factory_defined":2931,"templates_generator_defined":10372,"spawns_generator_defined":66063,"global_loader_refusal":global.id,"global_loader_blocked_spawns":waypoint_spawns,"global_loader_reached_f6":0},
        "dbscript_loader_before":{"refusal":global.id,"blocked_spawns":waypoint_spawns},
        "quest_flags_before":{"refusal":"execution.db_scripts.effective_quest_flags_owner","blocked_spawns":waypoint_spawns},
        "dbscript_loader_after":{"refusal":global_after_refusal,
            "blocked_spawns":if global_after.is_err(){waypoint_spawns}else{0},
            "loaded_manager":global_after.is_ok(),"CheckDbscript":"wired to validated map projection"},
        "raw_table_rows":{"creature_movement":raw[0].len(),"creature_movement_template":raw[1].len(),"waypoint_path":raw[2].len(),"script_waypoint":external.len()},
        "raw_template_path_reasons":template_reasons,
        "raw_spawn_entry_alternatives":alternatives_count,
        "raw_prefix_reason_alternatives":reasons,
        "raw_prefix_reason_spawn_sets_nonexclusive":spawn_reasons,
        "raw_selected_origins":origins,
        "arrival_script_owner_reach":"unavailable; global loader refuses before construction and all raw script-free prefix probes stop at F6",
        "undefined_prefix_rows":undefined,
        "later_undefined_reach":"null-AI indexed arrival requires an F6 spline; not reached in this prefix, live reach unavailable",
        "format":91,"format_change":false,"F1":"frozen unsealed","native_capability":"uninhabited"
    });
    if let Some(path) = std::env::var_os("BENILLA_F5_WAYPOINT_CENSUS") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!(
        "F5_WAYPOINT_CENSUS {}",
        serde_json::json!({
            "global_loader_refusal":global.id,"global_loader_reached_f6":0,
            "dbscript_loader_after_refusal":global_after_refusal,
            "dbscript_loader_after_blocked_spawns":if global_after.is_err(){waypoint_spawns}else{0},
            "raw_templates":template_reasons.iter().map(|(k,v)| (k,v.len())).collect::<BTreeMap<_,_>>(),
            "raw_spawn_entry_alternatives":alternatives_count,
            "raw_prefix_reasons":reasons.iter().map(|(k,v)| (k,v.len())).collect::<BTreeMap<_,_>>(),
            "raw_spawn_sets_nonexclusive":spawn_reasons.iter().map(|(k,v)| (k,v.len())).collect::<BTreeMap<_,_>>(),
            "undefined_prefix_rows":undefined.len()
        })
    );
}
