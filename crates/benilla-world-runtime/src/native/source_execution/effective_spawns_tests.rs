// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};

fn key(kind: GridSpawnKind) -> SpawnKey {
    SpawnKey { kind, guid: 7 }
}
fn row() -> ObjectMgrSpawnRow {
    ObjectMgrSpawnRow {
        ordinal: 0,
        guid: 7,
        entry: 10,
        map: 2,
        pose: [0.0; 4],
        respawn: [20, 10],
        radius: 0.0,
        movement: 0,
        mask: 1,
        event: 0,
        pools: [0, 0],
        spawn_template: 0,
        rotation: [0.0, 0.0, 0.0, 1.0],
    }
}
fn inputs() -> EffectiveSpawnInputs {
    EffectiveSpawnInputs {
        creatures: BTreeMap::from([
            (10, EffectiveSpawnTemplate::default()),
            (11, EffectiveSpawnTemplate::default()),
        ]),
        gameobjects: BTreeMap::from([(10, EffectiveSpawnTemplate::default())]),
        maps: BTreeMap::from([(0, 0), (2, 0), (3, 1)]),
        spawn_templates: BTreeSet::from([0, 5]),
        instances: Some(BTreeSet::new()),
        ..Default::default()
    }
}
struct Host {
    mt: Mt19937,
    cursor: DrawCursor,
    zones: usize,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            mt: Mt19937::from_seed(5489),
            cursor: DrawCursor::default(),
            zones: 0,
        }
    }
}
impl CreatureRandom for Host {
    fn creature_irand(&mut self, a: i32, b: i32) -> Result<i32> {
        SourceRandom::new(1, &mut self.mt, &mut self.cursor, &mut |_| {})
            .unwrap()
            .creature_irand(a, b)
    }
    fn creature_urand(&mut self, a: u32, b: u32) -> Result<u32> {
        SourceRandom::new(1, &mut self.mt, &mut self.cursor, &mut |_| {})
            .unwrap()
            .creature_urand(a, b)
    }
}
impl EffectiveSpawnOwner for Host {
    fn startup_random(&mut self) -> Result<&mut dyn CreatureRandom> {
        Ok(self)
    }
    fn original_zone(&mut self, _: u32, _: [f32; 3]) -> Result<u32> {
        self.zones += 1;
        Ok(12)
    }
    fn autoload_active(&mut self) -> Result<bool> {
        Ok(true)
    }
}
struct Unbound;
impl EffectiveSpawnOwner for Unbound {}
fn loaded(
    kind: GridSpawnKind,
    r: ObjectMgrSpawnRow,
    i: &EffectiveSpawnInputs,
) -> SourceEffectiveSpawns {
    let mut s = SourceEffectiveSpawns::default();
    s.inspect_row(kind, r, i, &mut Host::default()).unwrap();
    s
}
#[test]
fn pool_definitions_use_objectmgr_map_fields_without_promoting_source_skips() {
    let mut r = row();
    r.map = 99;
    let i = inputs();
    let spawns = loaded(GridSpawnKind::Creature, r, &i);
    assert_eq!(
        spawns
            .complete_data(key(GridSpawnKind::Creature))
            .unwrap_err()
            .id,
        "execution.effective_spawns.source_skipped_post_insert_owner"
    );
    let mut owner = EffectivePoolDefinitions {
        spawns: &spawns,
        maps: &i.maps,
    };
    assert_eq!(owner.spawn_map(PoolKind::Creature, 7).unwrap(), Some(99));
    assert_eq!(owner.spawn_map(PoolKind::GameObject, 7).unwrap(), None);
    assert!(!owner.map_exists(99).unwrap());
    assert!(owner.map_exists(2).unwrap());
    assert_eq!(
        owner.spawn_map(PoolKind::Child, 7).unwrap_err().id,
        "execution.pool.child_spawn_definition"
    );
}

#[test]
fn pool_definitions_propagate_unfinished_spawn_owners() {
    let mut r = row();
    r.entry = 0;
    let mut i = inputs();
    i.random
        .insert(key(GridSpawnKind::Creature), (false, vec![10, 11]));
    let mut spawns = SourceEffectiveSpawns::default();
    let error = spawns
        .inspect_row(GridSpawnKind::Creature, r, &i, &mut Unbound)
        .unwrap_err();
    let mut owner = EffectivePoolDefinitions {
        spawns: &spawns,
        maps: &i.maps,
    };
    assert_eq!(owner.spawn_map(PoolKind::Creature, 7).unwrap_err(), error);
}

#[test]
fn fixed_conditional_and_single_entry_random_keep_source_draws_and_reset_order() {
    let mut i = inputs();
    let k = key(GridSpawnKind::Creature);
    i.random.insert(k, (false, vec![11]));
    let mut h = Host::default();
    let mut s = SourceEffectiveSpawns::default();
    s.inspect_row(k.kind, row(), &i, &mut h).unwrap();
    assert_eq!(h.cursor.raw, 0);
    let mut r = row();
    r.entry = 0;
    s.inspect_row(k.kind, r.clone(), &i, &mut h).unwrap();
    assert_eq!(h.cursor.raw, 1);
    assert_eq!(s.complete_data(k).unwrap().unwrap().row.entry, 11);
    i.conditional.insert(7, [10, 11]);
    s.inspect_row(k.kind, r.clone(), &i, &mut h).unwrap();
    assert_eq!(h.cursor.raw, 1);
    let d = s.complete_data(k).unwrap().unwrap();
    assert_eq!(d.validation_entry, 10);
    assert_eq!(d.row.entry, 0);
    i.conditional.insert(7, [0, 11]);
    s.inspect_row(k.kind, r, &i, &mut h).unwrap();
    assert_eq!(s.complete_data(k).unwrap().unwrap().validation_entry, 11);
    assert_eq!(s.conditional_rows, 2);
}
#[test]
fn absent_random_list_leaves_zero_and_empty_present_list_refuses_undefined_index() {
    let mut i = inputs();
    let k = key(GridSpawnKind::Creature);
    let mut r = row();
    r.entry = 0;
    assert_eq!(
        loaded(k.kind, r.clone(), &i)
            .complete_data(k)
            .unwrap()
            .unwrap()
            .row
            .entry,
        0
    );
    i.random.insert(k, (false, vec![]));
    let mut s = SourceEffectiveSpawns::default();
    assert_eq!(
        s.inspect_row(k.kind, r.clone(), &i, &mut Host::default())
            .unwrap_err()
            .id,
        "execution.effective_spawns.empty_random_list_index_undefined"
    );
    i.random.insert(k, (false, vec![10]));
    assert_eq!(
        s.inspect_row(k.kind, r, &i, &mut Unbound).unwrap_err().id,
        "execution.effective_spawns.world_thread_startup_rng_position_owner"
    );
    assert!(s.pre_terrain_data(k).is_err());
}
#[test]
fn template_totem_and_display_skips_happen_before_map_insertion() {
    for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
        let mut r = row();
        r.entry = 99;
        assert!(loaded(kind, r, &inputs())
            .pre_terrain_data(key(kind))
            .unwrap()
            .is_none());
    }
    let mut i = inputs();
    i.creatures.get_mut(&10).unwrap().totem_ai = true;
    let s = loaded(GridSpawnKind::Creature, row(), &i);
    assert!(s.pre_terrain.is_empty());
    assert_eq!(s.records[0].reason, "totem_ai");
    i.gameobjects.get_mut(&10).unwrap().display = 99;
    assert!(loaded(GridSpawnKind::GameObject, row(), &i)
        .pre_terrain
        .is_empty());
    i.displays.insert(99);
    assert_eq!(
        loaded(GridSpawnKind::GameObject, row(), &i).complete_count(),
        1
    );
}
#[test]
fn map_and_xyz_skips_retain_inserted_fields_and_max_guid() {
    for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
        for bad_map in [true, false] {
            let mut r = row();
            if bad_map {
                r.map = 99;
            } else {
                r.pose[0] = 20000.0;
            }
            let s = loaded(kind, r, &inputs());
            assert!(s.pre_terrain_data(key(kind)).unwrap().is_some());
            assert_eq!(s.complete_count(), 0);
            assert_eq!(
                if kind == GridSpawnKind::Creature {
                    s.max_creature_guid
                } else {
                    s.max_gameobject_guid
                },
                7
            );
            assert_eq!(
                s.records[0].reason,
                if bad_map {
                    "missing_map"
                } else {
                    "invalid_xyz"
                }
            );
        }
        let mut r = row();
        r.pose[2] = f32::INFINITY;
        assert_eq!(loaded(kind, r, &inputs()).records[0].reason, "invalid_xyz");
    }
}
#[test]
fn respawn_radius_and_spawn_template_rewrites_follow_else_if_order() {
    let i = inputs();
    let k = key(GridSpawnKind::Creature);
    for (radius, movement, want_radius, want_movement) in [
        (-1.0, 1, 0.0, 1),
        (0.0, 1, 0.0, 0),
        (4.0, 0, 0.0, 0),
        (4.0, 2, 4.0, 2),
    ] {
        let mut r = row();
        r.radius = radius;
        r.movement = movement;
        r.spawn_template = 5;
        let s = loaded(k.kind, r, &i);
        let d = s.complete_data(k).unwrap().unwrap();
        assert_eq!(d.row.respawn, [20, 20]);
        assert_eq!((d.row.radius, d.row.movement), (want_radius, want_movement));
        assert_eq!(d.spawn_template, Some(5));
    }
    let mut r = row();
    r.spawn_template = 99;
    let s = loaded(k.kind, r, &i);
    assert_eq!(s.complete_data(k).unwrap().unwrap().spawn_template, Some(0));
    assert!(s.records.iter().any(|r| r.line == 2025));
    let mut r = row();
    r.respawn = [(-20i32) as u32, (-30i32) as u32];
    assert_eq!(
        loaded(GridSpawnKind::GameObject, r, &i)
            .complete_data(key(GridSpawnKind::GameObject))
            .unwrap()
            .unwrap()
            .row
            .respawn,
        [(-20i32) as u32; 2]
    );
}
#[test]
fn go_warning_rotation_orientation_and_dungeon_checks_preserve_map_entries() {
    let mut i = inputs();
    i.gameobjects.get_mut(&10).unwrap().despawn_at_action = true;
    let mut r = row();
    r.respawn = [0, 0];
    assert!(loaded(GridSpawnKind::GameObject, r, &i)
        .records
        .iter()
        .any(|r| r.reason == "zero_despawn_delay"));
    for index in 0..4 {
        for invalid in [-1.01, 1.01] {
            let mut r = row();
            r.rotation[index] = invalid;
            let s = loaded(GridSpawnKind::GameObject, r, &i);
            assert_eq!(s.complete_count(), 0);
            assert!(s
                .pre_terrain_data(key(GridSpawnKind::GameObject))
                .unwrap()
                .is_some());
            assert!(s
                .records
                .iter()
                .any(|r| r.reason == format!("invalid_rotation{index}")));
        }
        let mut r = row();
        r.rotation[index] = f32::NAN;
        assert_eq!(loaded(GridSpawnKind::GameObject, r, &i).complete_count(), 1);
    }
    let mut r = row();
    r.pose[3] = f32::NAN;
    assert_eq!(
        loaded(GridSpawnKind::Creature, r.clone(), &i).complete_count(),
        1
    );
    assert_eq!(loaded(GridSpawnKind::GameObject, r, &i).complete_count(), 0);
    let mut r = row();
    r.map = 3;
    assert_eq!(
        loaded(GridSpawnKind::GameObject, r.clone(), &i).complete_count(),
        0
    );
    i.instances.as_mut().unwrap().insert(3);
    assert_eq!(loaded(GridSpawnKind::GameObject, r, &i).complete_count(), 1);
}
#[test]
fn terrain_refuses_after_rewrites_and_before_conditional_reset_and_grid_effects() {
    let mut i = inputs();
    i.conditional.insert(7, [10, 11]);
    let mut r = row();
    r.map = 0;
    r.entry = 0;
    let k = key(GridSpawnKind::Creature);
    let mut s = SourceEffectiveSpawns::default();
    assert_eq!(
        s.inspect_row(k.kind, r, &i, &mut Unbound).unwrap_err().id,
        "execution.effective_spawns.terrain_original_zone_owner"
    );
    let d = s.pre_terrain_data(k).unwrap().unwrap();
    assert_eq!(d.row.entry, 10);
    assert_eq!(d.row.respawn, [20, 20]);
    assert_eq!(d.original_zone, None);
    assert!(s.complete_data(k).is_err());
    assert!(s.static_grids.is_empty());
    assert_eq!(
        s.pre_terrain_group_definition(k).unwrap().unwrap().entry,
        10
    );
}
#[test]
fn dynamic_creatures_also_grid_but_dynamic_gameobjects_do_not_and_mask_zero_still_active() {
    let mut i = inputs();
    i.creatures.get_mut(&10).unwrap().flags = 0x10_1000;
    i.gameobjects.get_mut(&10).unwrap().flags = 0x1002;
    for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
        let s = loaded(kind, row(), &i);
        assert_eq!(s.dynamic_guids[&(2, kind)], [7]);
        assert_eq!(s.static_grids.is_empty(), kind == GridSpawnKind::GameObject);
        let mut r = row();
        r.event = 1;
        let s = loaded(kind, r, &i);
        assert!(s.static_grids.is_empty());
        assert!(s.dynamic_guids.is_empty());
        let mut r = row();
        r.pools = [-1, 0];
        assert!(loaded(kind, r, &i).static_grids.is_empty());
    }
    let mut r = row();
    r.mask = 0;
    let s = loaded(GridSpawnKind::Creature, r, &i);
    assert!(s.static_grids.is_empty());
    assert_eq!(s.active[&(2, GridSpawnKind::Creature)], [7]);
}
#[test]
fn gameobject_addon_checks_previous_state_and_mutates_animation_before_warning() {
    let k = key(GridSpawnKind::GameObject);
    let mut s = loaded(k.kind, row(), &inputs());
    assert_eq!(
        s.apply_gameobject_addons(&[], &BTreeSet::new())
            .unwrap_err()
            .id,
        "execution.effective_spawns.empty_gameobject_addon_query_dereference_undefined"
    );
    s.apply_gameobject_addons(&[[7, 9, 99, 1], [7, 10, 1, 2]], &BTreeSet::new())
        .unwrap();
    let d = s.complete_data(k).unwrap().unwrap();
    assert_eq!((d.animprogress, d.go_state, d.string_id), (10, 99, 0));
    assert!(s.records.iter().any(|r| r.line == 2269));
    let mut s = loaded(k.kind, row(), &inputs());
    s.apply_gameobject_addons(
        &[[7, 8, (-1i32) as u32, 2], [7, 7, 2, 2]],
        &BTreeSet::from([2]),
    )
    .unwrap();
    let d = s.complete_data(k).unwrap().unwrap();
    assert_eq!((d.go_state, d.string_id), (2, 2));
}
#[test]
fn catalog04_effective_spawn_counts_or_explicit_absence() {
    let (Some(root), Some(occurrences)) = (
        std::env::var_os("BENILLA_F2_CATALOG"),
        std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES"),
    ) else {
        println!("SKIP effective spawns: set BENILLA_F2_CATALOG and BENILLA_F2_SPAWN_OCCURRENCES");
        return;
    };
    let mut c = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    assert_eq!(
        c.execution_effective_spawn_operands("creature", ["guid"])
            .unwrap_err()
            .id,
        "effective_spawns.operand_relation"
    );
    let mut rows = ObjectMgrSpawnRows::open(&mut c, Path::new(&occurrences)).unwrap();
    assert_eq!(
        rows.next("game_event").unwrap_err().id,
        "effective_spawns.joined_relation"
    );
    let s = SourceEffectiveSpawns::inspect_catalog_pre_terrain(&mut c, Path::new(&occurrences))
        .unwrap();
    let mut reasons = BTreeMap::new();
    for r in &s.records {
        *reasons.entry(r.reason.clone()).or_insert(0usize) += 1;
    }
    let mut refused = BTreeMap::new();
    for r in s.refusals.values() {
        *refused.entry(r.id.clone()).or_insert(0usize) += 1;
    }
    assert!(
        !refused.keys().any(|s| s.ends_with("_undefined")),
        "stock undefined reach: {refused:?}"
    );
    assert_eq!(s.random_rows, [3468, 1856]);
    assert_eq!(s.draws_needed, 5324);
    assert_eq!(s.single_entry_draws, [167, 57]);
    let mut raw_rows = [0u64; 2];
    let mut inserted = [0u64; 2];
    let mut complete = [0u64; 2];
    let mut skipped = BTreeMap::<String, [u64; 2]>::new();
    for record in &s.records {
        let slot = usize::from(record.key.kind == GridSpawnKind::GameObject);
        if record.line == 0 {
            raw_rows[slot] += 1;
        }
        if matches!(
            record.reason.as_str(),
            "missing_template"
                | "totem_ai"
                | "invalid_display"
                | "missing_map"
                | "invalid_xyz"
                | "invalid_rotation0"
                | "invalid_rotation1"
                | "invalid_rotation2"
                | "invalid_rotation3"
                | "invalid_map_orientation"
        ) {
            skipped.entry(record.reason.clone()).or_default()[slot] += 1;
        }
    }
    for key in s.pre_terrain.keys() {
        inserted[usize::from(key.kind == GridSpawnKind::GameObject)] += 1;
    }
    for key in &s.completed {
        complete[usize::from(key.kind == GridSpawnKind::GameObject)] += 1;
    }
    assert_eq!(raw_rows, [66508, 47702]);
    let instance_maps = s
        .refusals
        .iter()
        .filter(|(_, error)| {
            error.id == "execution.effective_spawns.loaded_instance_template_owner"
        })
        .map(|(key, _)| {
            assert_eq!(key.kind, GridSpawnKind::GameObject);
            s.pre_terrain[key].row.map
        })
        .collect::<BTreeSet<_>>();
    println!(
        "F4_EFFECTIVE_SPAWNS {}",
        serde_json::json!({"raw_rows":raw_rows,"pre_terrain_map_rows":inserted,"complete_rows":complete,"source_skipped_by_reason":skipped,"random_rows":s.random_rows,"single_entry_draws":s.single_entry_draws,"conditional_rows":s.conditional_rows,"draws_needed":s.draws_needed,"reasons":reasons,"refusals":refused,"loaded_instance_owner_distinct_maps":instance_maps.len(),"loaded_instance_owner_maps":instance_maps,"undefined_reach":0})
    );
}

#[test]
fn prerequisite_reader_rejects_relations_outside_its_allowlist() {
    VerifiedCatalog::with_authored_group_test_catalog(|mut catalog| {
        for table in [
            "creature",
            "spawn_group",
            "game_event",
            "conditions",
            "creature_template",
        ] {
            assert_eq!(
                catalog
                    .execution_effective_spawn_operands(table, ["guid"])
                    .unwrap_err()
                    .id,
                "effective_spawns.operand_relation"
            );
        }
    });
}

#[test]
fn creature_map_narrows_before_lookup_and_zone_narrows_after_terrain() {
    struct WideZone;
    impl EffectiveSpawnOwner for WideZone {
        fn original_zone(&mut self, _: u32, _: [f32; 3]) -> Result<u32> {
            Ok(65543)
        }
        fn autoload_active(&mut self) -> Result<bool> {
            Ok(false)
        }
    }
    let mut r = row();
    r.map = 65536;
    let mut s = SourceEffectiveSpawns::default();
    s.inspect_row(GridSpawnKind::Creature, r.clone(), &inputs(), &mut WideZone)
        .unwrap();
    let d = s
        .complete_data(key(GridSpawnKind::Creature))
        .unwrap()
        .unwrap();
    assert_eq!((d.row.map, d.original_zone), (0, Some(7)));
    let s = loaded(GridSpawnKind::GameObject, r, &inputs());
    assert_eq!(
        s.pre_terrain_data(key(GridSpawnKind::GameObject))
            .unwrap()
            .unwrap()
            .row
            .map,
        65536
    );
    assert_eq!(s.complete_count(), 0);
}

#[test]
fn coordinate_boundary_is_inclusive_and_fixed_zero_and_rotation_endpoints_load() {
    let limit = (f64::from(533.333_3_f32 * 64.0 / 2.0) - 0.5) as f32;
    for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
        for sign in [-1.0, 1.0] {
            let mut r = row();
            r.pose[0] = sign * limit;
            assert_eq!(loaded(kind, r.clone(), &inputs()).complete_count(), 1);
            r.pose[0] = sign * f32::from_bits(limit.to_bits() + 1);
            assert_eq!(loaded(kind, r, &inputs()).complete_count(), 0);
        }
    }
    let mut r = row();
    r.entry = 0;
    r.rotation = [-1.0, 1.0, -1.0, 1.0];
    assert_eq!(
        loaded(GridSpawnKind::GameObject, r, &inputs()).complete_count(),
        1
    );
}

#[test]
fn catalog04_pre_terrain_group_membership_census_or_explicit_absence() {
    use benilla_sim::source_spawn_groups::{GroupInputData, GroupPhase};
    let (Some(root), Some(spawn_path), Some(group_path)) = (
        std::env::var_os("BENILLA_F2_CATALOG"),
        std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES"),
        std::env::var_os("BENILLA_F2_GROUP_OCCURRENCES"),
    ) else {
        println!("SKIP pre-terrain group census: bind catalog, spawn and group occurrences");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let spawns =
        SourceEffectiveSpawns::inspect_catalog_pre_terrain(&mut catalog, Path::new(&spawn_path))
            .unwrap();
    let mut rows = catalog
        .execution_spawn_group_rows(Path::new(&group_path))
        .unwrap();
    let mut kinds = BTreeMap::new();
    let mut members = Vec::new();
    for phase in GroupPhase::ALL {
        while let Some(row) = GroupRows::next(&mut rows, phase).unwrap() {
            match row.data {
                GroupInputData::Template(t) => {
                    kinds.insert(t.id, t.kind);
                }
                GroupInputData::Member { group, guid, .. } => members.push((group, guid)),
                _ => {}
            }
        }
    }
    let mut owner = CatalogGroupLoadOwner::with_pre_terrain(&mut catalog, spawns);
    let mut passed = [0usize; 2];
    let mut missing = 0;
    let mut refused = BTreeMap::new();
    for (group, guid) in members {
        let creature = kinds[&group] == 0;
        let kind = if creature {
            GridSpawnKind::Creature
        } else {
            GridSpawnKind::GameObject
        };
        match owner.spawn(SpawnKey { kind, guid }) {
            Ok(Some(_)) => passed[usize::from(!creature)] += 1,
            Ok(None) => missing += 1,
            Err(e) => *refused.entry(e.id).or_insert(0usize) += 1,
        }
    }
    assert_eq!(passed, [1674, 2567]);
    assert_eq!(missing, 0);
    assert_eq!(
        refused,
        BTreeMap::from([(
            "execution.effective_spawns.world_thread_startup_rng_position_owner".to_owned(),
            341
        )])
    );
    drop(owner);
    let mut bound = catalog_group_census_owner(&mut catalog);
    let final_refusal = bound
        .contains(DbScriptLookup::SpawnGroup, 470001)
        .unwrap_err();
    assert_eq!(
        final_refusal.id,
        "execution.effective_spawns.world_thread_startup_rng_position_owner"
    );
    println!("F4_GROUP_PRE_TERRAIN passed={passed:?} missing={missing} refused={refused:?}; before=[1950,2632] owner refusals; complete_group_refusal={}",final_refusal.id);
}

#[test]
fn source_random_promotion_keeps_creature_active_flag_quirk_and_go_dynguid_flag() {
    let mut i = inputs();
    i.creatures.get_mut(&10).unwrap().flags = 0x1000;
    i.creatures.get_mut(&11).unwrap().flags = 0x10_0000;
    i.gameobjects.get_mut(&10).unwrap().flags = 2;
    i.load_random_entry(GridSpawnKind::Creature, 1, 10);
    i.load_random_entry(GridSpawnKind::Creature, 2, 11);
    i.load_random_entry(GridSpawnKind::Creature, 1, 99);
    i.load_random_entry(GridSpawnKind::GameObject, 3, 10);
    assert_eq!(
        i.random[&SpawnKey {
            kind: GridSpawnKind::Creature,
            guid: 1
        }],
        (true, vec![10])
    );
    assert_eq!(
        i.random[&SpawnKey {
            kind: GridSpawnKind::Creature,
            guid: 2
        }],
        (false, vec![11])
    );
    assert_eq!(
        i.random[&SpawnKey {
            kind: GridSpawnKind::GameObject,
            guid: 3
        }],
        (true, vec![10])
    );
}

#[test]
fn creature_then_gameobject_selection_uses_one_borrowed_f3_cursor() {
    let mut i = inputs();
    let mut h = Host::default();
    let mut s = SourceEffectiveSpawns::default();
    for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
        i.random.insert(key(kind), (false, vec![10]));
        let mut r = row();
        r.entry = 0;
        s.inspect_row(kind, r, &i, &mut h).unwrap();
        assert_eq!(s.complete_data(key(kind)).unwrap().unwrap().row.entry, 10);
    }
    assert_eq!(h.cursor.raw, 2);
    assert_eq!(s.random_rows, [1, 1]);
    assert_eq!(h.mt.state().index, 2);
}

#[test]
fn conditional_default_does_not_fall_back_when_nonzero_alliance_template_is_missing() {
    let mut i = inputs();
    i.conditional.insert(7, [99, 10]);
    let mut r = row();
    r.entry = 0;
    assert!(loaded(GridSpawnKind::Creature, r.clone(), &i)
        .pre_terrain
        .is_empty());
    i.conditional.insert(7, [0, 0]);
    assert_eq!(
        loaded(GridSpawnKind::Creature, r, &i)
            .complete_data(key(GridSpawnKind::Creature))
            .unwrap()
            .unwrap()
            .row
            .entry,
        0
    );
}

#[test]
fn startup_rows_keep_empty_query_returns_and_stop_at_first_missing_owner() {
    struct Rows {
        creatures: std::collections::VecDeque<ObjectMgrSpawnRow>,
        gameobjects: std::collections::VecDeque<ObjectMgrSpawnRow>,
        phases: Vec<GridSpawnKind>,
    }
    impl EffectiveSpawnRows for Rows {
        fn next_spawn(&mut self, kind: GridSpawnKind) -> Result<Option<ObjectMgrSpawnRow>> {
            self.phases.push(kind);
            Ok(if kind == GridSpawnKind::Creature {
                self.creatures.pop_front()
            } else {
                self.gameobjects.pop_front()
            })
        }
    }
    let mut rows = Rows {
        creatures: Default::default(),
        gameobjects: Default::default(),
        phases: vec![],
    };
    let mut s = SourceEffectiveSpawns::default();
    s.load_rows(&mut rows, &inputs(), &[], &mut Host::default())
        .unwrap();
    assert_eq!(
        rows.phases,
        [GridSpawnKind::Creature, GridSpawnKind::GameObject]
    );
    let mut r = row();
    r.map = 0;
    rows.creatures.push_back(r);
    rows.gameobjects.push_back(row());
    assert_eq!(
        s.load_rows(&mut rows, &inputs(), &[[7, 100, 1, 0]], &mut Unbound)
            .unwrap_err()
            .id,
        "execution.effective_spawns.terrain_original_zone_owner"
    );
    assert_eq!(rows.gameobjects.len(), 1);
    assert_eq!(
        s.pre_terrain_data(key(GridSpawnKind::Creature))
            .unwrap()
            .unwrap()
            .original_zone()
            .unwrap_err()
            .id,
        "execution.effective_spawns.terrain_original_zone_owner"
    );
    rows.creatures.clear();
    let mut s = SourceEffectiveSpawns::default();
    assert_eq!(
        s.load_rows(&mut rows, &inputs(), &[], &mut Host::default())
            .unwrap_err()
            .id,
        "execution.effective_spawns.empty_gameobject_addon_query_dereference_undefined"
    );
}

#[test]
fn startup_rows_use_reference_primary_keys_then_full_row_not_export_ordinals() {
    struct Rows([std::collections::VecDeque<ObjectMgrSpawnRow>; 2]);
    impl EffectiveSpawnRows for Rows {
        fn next_spawn(&mut self, kind: GridSpawnKind) -> Result<Option<ObjectMgrSpawnRow>> {
            Ok(self.0[usize::from(kind == GridSpawnKind::GameObject)].pop_front())
        }
    }
    for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
        let mut supplied = Vec::new();
        // Signed event ASC: spawn-row-order/game_event_{creature,gameobject}-schema.tsv:4-5 and tie-order-analysis.json:7,15; equal-event pose ties use the full row.
        for (ordinal, guid, event, x) in [
            (0, 9, 0, 0.0),
            (1, 7, 2, 0.0),
            (2, 7, -2, 1.0),
            (3, 7, -2, -1.0),
        ] {
            let mut r = row();
            r.ordinal = ordinal;
            r.guid = guid;
            r.event = event;
            r.pose[0] = x;
            supplied.push(r);
        }
        let mut rows = Rows(Default::default());
        rows.0[usize::from(kind == GridSpawnKind::GameObject)] = supplied.into();
        let mut spawns = SourceEffectiveSpawns::default();
        spawns
            .load_rows(
                &mut rows,
                &inputs(),
                &[[7, 100, 1, 0]],
                &mut Host::default(),
            )
            .unwrap();
        assert_eq!(
            spawns
                .records
                .iter()
                .filter(|r| r.line == 0)
                .map(|r| (r.key.guid, r.ordinal))
                .collect::<Vec<_>>(),
            [(7, 3), (7, 2), (7, 1), (9, 0)]
        );
        // The last composite-key row overwrites the earlier event memberships.
        assert_eq!(spawns.pre_terrain[&key(kind)].row.event, 2);
    }
}

#[test]
fn loaded_instance_owner_precedes_orientation_validation_and_never_uses_raw_ids() {
    let mut i = inputs();
    i.instances = None;
    let mut r = row();
    r.map = 3;
    r.pose[3] = f32::NAN;
    let mut s = SourceEffectiveSpawns::default();
    assert_eq!(
        s.inspect_row(GridSpawnKind::GameObject, r, &i, &mut Host::default())
            .unwrap_err()
            .id,
        "execution.effective_spawns.loaded_instance_template_owner"
    );
    let projection = s
        .pre_terrain_group_definition(key(GridSpawnKind::GameObject))
        .unwrap()
        .unwrap();
    assert_eq!(
        (projection.map, projection.entry, projection.spawn_mask),
        (3, 10, 1)
    );
    assert!(s.records.iter().any(|r| r.reason == "respawn_max_raised"));
}
