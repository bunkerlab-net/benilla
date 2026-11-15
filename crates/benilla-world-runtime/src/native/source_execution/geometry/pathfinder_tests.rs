// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};

pub(in super::super) fn geometry(entry: u16, flags: u8) -> Option<SourceTerrainManager> {
    let mut geometry = SourceTerrainManager::default();
    let config = b"mmap.enabled = 1\nmmap.ignoreMapIds = \"\"\nmmap.preload = 0\nvmap.enableLOS = 1\nvmap.enableHeight = 1\n";
    geometry
        .mmap
        .initialize_recorded_world(config, &Sha256Id::of(config))
        .unwrap();
    geometry.mmap.files.map_bytes(
        0,
        include_bytes!("../../../../../benilla-native-nav/tools/fixtures/authored.mmap").to_vec(),
    );
    let result = geometry.mmap.load_map_instance(0, 7);
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (entry, flags);
        assert_eq!(
            result.unwrap_err().id,
            "execution.mmap.reference_cpu_fp_binding"
        );
        assert_eq!(geometry.mmap.mesh_count(), 0);
        assert_eq!(geometry.mmap.tile_count(), 0);
        assert!(geometry.mmap.query(0, 7).is_none());
        assert_eq!(geometry.pathfinder_constructor_completions, 0);
        None
    }
    #[cfg(target_arch = "x86_64")]
    {
        result.unwrap();
        for x in [31, 32] {
            for y in [31, 32] {
                geometry.mmap.files.source_tile_absent(0, x, y).unwrap();
            }
        }
        geometry
            .vmap
            .initialize_recorded_world([0], config, &Sha256Id::of(config))
            .unwrap();
        geometry
            .vmap
            .collision
            .files
            .observe(b"000.vmtree", benilla_native_vmap::live::FileState::Absent)
            .unwrap();
        let mut bytes = vec![0; 40];
        bytes[..8].copy_from_slice(b"MAPSz1.4");
        bytes[16..20].copy_from_slice(&40u32.to_le_bytes());
        bytes.extend(b"MHGT");
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(0f32.to_le_bytes());
        bytes.extend(0f32.to_le_bytes());
        if flags != 0 {
            bytes[24..28].copy_from_slice(&56u32.to_le_bytes());
            bytes.extend(b"MLIQ");
            bytes.extend([3, flags]);
            bytes.extend(entry.to_le_bytes());
            bytes.extend([0, 0, 128, 128]);
            bytes.extend(8f32.to_le_bytes());
        }
        for x in [31, 32] {
            for y in [31, 32] {
                geometry.files.bytes(0, x, y, bytes.clone());
            }
        }
        geometry.liquid_dbc = SourceLiquidDbc::authored();
        let mut mt = Mt19937::from_seed(5489);
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
        geometry.load_terrain(0, &mut random).unwrap();
        assert_eq!(random.cursor().raw, 1);
        Some(geometry)
    }
}
pub(in super::super) fn setup() -> (SourceObjects, SourceUnits, SourceMotionMaster) {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let object = objects.get_mut(unit).unwrap();
    object.alive = true;
    object.map = Some(MapIdentity {
        map: 0,
        instance: 7,
        creation_generation: 1,
    });
    object.pose = Some(SourcePose([0f32; 4].map(f32::to_bits)));
    let mut units = SourceUnits::default();
    units
        .register(
            &objects,
            unit,
            SourceUnitFields::creature(&motion_master_tests::template()),
        )
        .unwrap();
    (
        objects,
        units,
        SourceMotionMaster::new(
            unit,
            MotionDefault {
                movement_type: 1,
                default_path: 0,
            },
        ),
    )
}
#[test]
fn constructor_keeps_real_instance_query_defaults_and_consumes_no_rng() {
    let Some(mut geometry) = geometry(0, 0) else {
        return;
    };
    let (objects, mut units, master) = setup();
    let mut mt = Mt19937::from_seed(5489);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| panic!("constructor drew RNG");
    let random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    let p = SourcePathFinder::construct(
        &mut geometry,
        master.unit,
        &MotionContext {
            objects: &objects,
            units: &mut units,
        },
        true,
    )
    .unwrap();
    assert_eq!(random.cursor().raw, 0);
    assert_eq!(geometry.pathfinder_constructor_completions, 1);
    let query = p.read_default_query().unwrap().unwrap();
    assert!(query.same_query(&geometry.mmap.query(0, 7).unwrap()));
    assert_eq!(geometry.mmap.query_capacity(0, query).unwrap(), 1024);
    assert!(p.ignore_normalization);
    assert_eq!(p.cache.storage(), &[0; 74]);
    assert_eq!(p.cached_points, vec![0.; 222]);
    assert_eq!(p.smooth_refs, vec![0; 74]);
    assert_eq!(p.path_type, 0);
    assert!(!p.use_straight_path && !p.force_destination && !p.straight_line);
}
#[test]
fn live_wet_query_precedes_destination_draws_and_or_is_retained() {
    for (entry, flags, expected) in [(21, 1, 4), (22, 2, 4), (23, 4, 8), (24, 8, 8), (21, 17, 1)] {
        let Some(mut geometry) = geometry(entry, flags) else {
            return;
        };
        let (mut objects, mut units, master) = setup();
        let f = units.get_mut(&objects, master.unit).unwrap();
        f.can_walk = false;
        f.can_swim = false;
        let mut p = SourcePathFinder::construct(
            &mut geometry,
            master.unit,
            &MotionContext {
                objects: &objects,
                units: &mut units,
            },
            false,
        )
        .unwrap();
        assert_eq!(p.filter.include_flags, expected);
        assert_eq!(p.filter.exclude_flags, 0);
        objects
            .set_pose(master.unit, SourcePose([0., 0., -3., 0.].map(f32::to_bits)))
            .unwrap();
        p.update_filter(&mut geometry, &objects).unwrap();
        assert_eq!(p.filter.include_flags, expected);
        p.points.push([1.; 3]);
        p.clear();
        assert!(p.points.is_empty());
    }
    let Some(mut geometry) = geometry(21, 1) else {
        return;
    };
    geometry.liquid_dbc = SourceLiquidDbc::default();
    let (objects, mut units, master) = setup();
    let g = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    let error = g
        .initialize(
            &mut SourceMotionMaster::new(master.unit, master.default),
            &mut MotionContext {
                objects: &objects,
                units: &mut units,
            },
            &mut geometry,
        )
        .unwrap_err();
    assert_eq!(error.id, "execution.terrain.LiquidType_dbc_owner");
    assert!(g.pathfinder().is_none());
    assert_eq!(geometry.pathfinder_constructor_completions, 0);
}
#[test]
fn capability_masks_and_disabled_unwritten_query_refuse_only_at_read() {
    for (walk, swim, expected) in [
        (false, false, 0),
        (true, false, 1),
        (false, true, 12),
        (true, true, 13),
    ] {
        let Some(mut geometry) = geometry(0, 0) else {
            return;
        };
        let (objects, mut units, master) = setup();
        let f = units.get_mut(&objects, master.unit).unwrap();
        f.can_walk = walk;
        f.can_swim = swim;
        let p = SourcePathFinder::construct(
            &mut geometry,
            master.unit,
            &MotionContext {
                objects: &objects,
                units: &mut units,
            },
            false,
        )
        .unwrap();
        assert_eq!(p.filter.include_flags, expected);
        assert!(p.read_default_query().unwrap().is_some());
    }
    for extra in [0x4000, 0x6000] {
        let Some(mut geometry) = geometry(0, 0) else {
            return;
        };
        let (objects, mut units, master) = setup();
        let f = units.get_mut(&objects, master.unit).unwrap();
        f.mmap_extra_flags = extra;
        f.flags |= 8;
        let p = SourcePathFinder::construct(
            &mut geometry,
            master.unit,
            &MotionContext {
                objects: &objects,
                units: &mut units,
            },
            false,
        )
        .unwrap();
        assert_eq!(geometry.pathfinder_constructor_completions, 1);
        assert_eq!(
            p.read_default_query().unwrap_err().id,
            "execution.pathfinder.undefined_default_query_read"
        );
    }
    let Some(mut geometry) = geometry(0, 0) else {
        return;
    };
    geometry.mmap.unload_instance(0, 7);
    let (objects, mut units, master) = setup();
    let p = SourcePathFinder::construct(
        &mut geometry,
        master.unit,
        &MotionContext {
            objects: &objects,
            units: &mut units,
        },
        false,
    )
    .unwrap();
    assert!(p.read_default_query().unwrap().is_none());
}
#[test]
fn wander_and_home_prove_construction_before_the_next_named_refusal() {
    let Some(mut geometry) = geometry(0, 0) else {
        return;
    };
    let (mut objects, mut units, mut master) = setup();
    objects.get_mut(master.unit).unwrap().active = true;
    // CombatManager.cpp:161-176: this constructor fixture is already active and evading.
    units.get_mut(&objects, master.unit).unwrap().evade_home = true;
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let g = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    assert_eq!(
        g.initialize(&mut master, &mut context, &mut geometry)
            .unwrap_err()
            .id,
        "execution.motion.client_controlling_owner"
    );
    assert!(g.pathfinder().is_some());
    assert_eq!(geometry.pathfinder_constructor_completions, 1);
    struct Home {
        geometry: SourceTerrainManager,
    }
    impl MotionExecutionOwner for Home {
        fn pathfinder_geometry(&mut self) -> Result<&mut SourceTerrainManager> {
            Ok(&mut self.geometry)
        }
        fn combat_start_position(&mut self, _: ObjectHandle) -> Result<[f32; 4]> {
            Ok([1.; 4])
        }
        fn home_distance(&mut self, _: ObjectHandle, target: [f32; 4]) -> Result<f32> {
            Ok(target[..3].iter().map(|v| v * v).sum())
        }
    }
    let mut owner = Home { geometry };
    assert_eq!(
        SourceHomeGenerator::new(true)
            .initialize(&mut master, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.pathfinder.transport_owner"
    );
    assert_eq!(owner.geometry.pathfinder_constructor_completions, 2);
}

#[test]
fn forcing_overrides_map_ignore_but_not_global_disable_and_fields_are_transient() {
    for enabled in [false, true] {
        for (extra, controlled, selected) in [
            (0, false, false),
            (0x2000, false, true),
            (0, true, true),
            (0x6000, true, false),
        ] {
            let Some(mut geometry) = geometry(0, 0) else {
                return;
            };
            let config = format!(
                "mmap.enabled = {}\nmmap.ignoreMapIds = \"0\"\nmmap.preload = 0\n",
                u8::from(enabled)
            );
            geometry
                .mmap
                .initialize_recorded_world(config.as_bytes(), &Sha256Id::of(config.as_bytes()))
                .unwrap();
            let (objects, mut units, master) = setup();
            let f = units.get_mut(&objects, master.unit).unwrap();
            f.mmap_extra_flags = extra;
            f.set_flag(8, controlled);
            let serialized = serde_json::to_value(&f).unwrap();
            for key in ["can_walk", "can_swim", "mmap_extra_flags"] {
                assert!(serialized.get(key).is_none());
            }
            let p = SourcePathFinder::construct(
                &mut geometry,
                master.unit,
                &MotionContext {
                    objects: &objects,
                    units: &mut units,
                },
                false,
            )
            .unwrap();
            assert_eq!(
                matches!(p.default_query, SourceDefaultQuery::Lookup(_)),
                enabled && selected
            );
            assert!(p.current_query.is_none());
            assert!(p.current_mesh_map.is_none());
        }
    }
}
#[test]
fn bound_initialization_executes_target_drop_before_melee_owner_and_random_update() {
    struct Owner {
        geometry: SourceTerrainManager,
    }
    impl MotionExecutionOwner for Owner {
        fn pathfinder_geometry(&mut self) -> Result<&mut SourceTerrainManager> {
            Ok(&mut self.geometry)
        }
        fn client_controlling(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
            Ok(None)
        }
        fn spline_finalized(&mut self, _: ObjectHandle) -> Result<bool> {
            Ok(true)
        }
    }
    let (objects, mut units, mut master) = setup();
    let Some(geometry) = geometry(0, 0) else {
        return;
    };
    let mut owner = Owner { geometry };
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let f = context.units.get_mut(&objects, master.unit).unwrap();
    f.has_ai = true;
    f.target = Some(master.unit);
    let g = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    assert_eq!(
        g.initialize(&mut master, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.motion.wander_melee_attack_stop_owner"
    );
    assert_eq!(
        context.units.get(&objects, master.unit).unwrap().target,
        None
    );
    assert_eq!(owner.geometry.pathfinder_constructor_completions, 1);
    context.units.get_mut(&objects, master.unit).unwrap().has_ai = false;
    g.initialize(&mut master, &mut context, &mut owner).unwrap();
    assert_eq!(
        g.update(&mut master, 1, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.motion.f6_wander_random_path_owner"
    );
    assert_eq!(owner.geometry.pathfinder_constructor_completions, 2);
}

#[test]
fn pet_and_player_overrides_refuse_by_name_instead_of_borrowing_creature_capabilities() {
    for (kind, reason) in [
        (
            SourceObjectKind::Pet,
            "execution.pathfinder.pet_can_swim_owner",
        ),
        (
            SourceObjectKind::Player,
            "execution.pathfinder.player_water_owner",
        ),
    ] {
        let Some(mut geometry) = geometry(0, 0) else {
            return;
        };
        let (mut objects, mut units, master) = setup();
        objects.get_mut(master.unit).unwrap().kind = kind;
        assert_eq!(
            SourcePathFinder::construct(
                &mut geometry,
                master.unit,
                &MotionContext {
                    objects: &objects,
                    units: &mut units
                },
                false
            )
            .unwrap_err()
            .id,
            reason
        );
        assert_eq!(geometry.pathfinder_constructor_completions, 0);
    }
}

#[test]
fn nearest_binds_constructor_filter_cache_and_real_selected_instance_query() {
    use benilla_native_nav::live::DistanceDeclaration;
    let Some(mut geometry) = geometry(0, 0) else {
        return;
    };
    geometry.mmap.unload_map(0).unwrap();
    geometry.mmap.files.map_bytes(
        0,
        include_bytes!(
            "../../../../../benilla-native-nav/tools/fixtures/direct-corridor/params.test-fixture"
        )
        .to_vec(),
    );
    geometry.mmap.files.tile_bytes(
        0,
        31,
        31,
        include_bytes!(
            "../../../../../benilla-native-nav/tools/fixtures/warm-cache/precision.test-fixture"
        )
        .to_vec(),
    );
    geometry.mmap.load_map_instance(0, 7).unwrap();
    geometry.mmap.load_tile(0, 31, 31).unwrap();
    let (objects, mut units, master) = setup();
    units.get_mut(&objects, master.unit).unwrap().can_walk = true;
    let mut path = SourcePathFinder::construct(
        &mut geometry,
        master.unit,
        &MotionContext {
            objects: &objects,
            units: &mut units,
        },
        false,
    )
    .unwrap();
    let mut distance = PathDistance::new(DistanceDeclaration::BuildPolyStart);
    assert_eq!(
        path.nearest_poly(&geometry, [5., 0., 5.], &mut distance)
            .unwrap_err()
            .id,
        "execution.pathfinder.current_mesh_owner"
    );
    // Authored current-map selection, not an implementation of SetCurrentNavMesh.
    path.current_query = path.read_default_query().unwrap().cloned();
    path.current_mesh_map = Some(0);
    let found = path
        .nearest_poly(&geometry, [5., 0., 5.], &mut distance)
        .unwrap()
        .unwrap();
    assert_eq!(found.polygon, 0x0010_0000_0000_0000);
    assert!(found.appended);
    assert_eq!(&path.cache.storage()[74..], &[found.polygon]);
    geometry.mmap.unload_instance(0, 7);
    assert_eq!(
        path.nearest_poly(&geometry, [5., 0., 5.], &mut distance)
            .unwrap_err()
            .id,
        "execution.mmap.stale_query"
    );
}
