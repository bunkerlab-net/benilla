// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeMap;
fn source_host() -> bool {
    if cfg!(target_arch = "x86_64") {
        return true;
    }
    assert!(matches!(
        benilla_native_nav::live::MapMesh::new(include_bytes!(
            "../../../../../benilla-native-nav/tools/fixtures/authored.mmap"
        )),
        Err(benilla_native_nav::live::LiveError::NumericBinding)
    ));
    false
}

fn setup(
    mesh: bool,
) -> Option<(
    SourceTerrainManager,
    SourceObjects,
    SourceUnits,
    ObjectHandle,
)> {
    let mut geometry = super::super::tests::geometry(0, 0)?;
    let config = b"mmap.enabled = 1\nmmap.ignoreMapIds = \"\"\nmmap.preload = 0\nPathFinder.OptimizePath = 0\nPathFinder.NormalizeZ = 0\n";
    geometry
        .mmap
        .initialize_recorded_world(config, &Sha256Id::of(config))
        .unwrap();
    geometry.pathfinder_map_types = Some(BTreeMap::from([(0, 0)]));
    if mesh {
        geometry.mmap.unload_map(0).unwrap();
        geometry.mmap.files.map_bytes(0,include_bytes!("../../../../../benilla-native-nav/tools/fixtures/direct-corridor/params.test-fixture").to_vec());
        geometry.mmap.files.tile_bytes(0,31,31,include_bytes!("../../../../../benilla-native-nav/tools/fixtures/warm-cache/precision.test-fixture").to_vec());
        geometry.mmap.load_map_instance(0, 7).unwrap();
        geometry.mmap.load_tile(0, 31, 31).unwrap();
    }
    let (objects, mut units, master) = super::super::tests::setup();
    units.get_mut(&objects, master.unit).unwrap().can_walk = true;
    Some((geometry, objects, units, master.unit))
}
#[test]
fn calculate_invalid_coordinates_leave_the_previous_state_untouched_124_128() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, objects, mut units, unit)) = setup(false) else {
        return;
    };
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
    path.points.push([9.; 3]);
    assert!(!path
        .calculate(
            &mut geometry,
            &context,
            Err(refuse("execution.pathfinder.transport_owner")),
            [0.; 3],
            [f32::NAN, 0., 0.],
            true,
            false
        )
        .unwrap());
    assert_eq!(path.points, [[9.; 3]]);
    assert_eq!(geometry.pathfinder_calculate_completions, 0);
}
#[test]
fn calculate_stops_at_unbound_transport_before_default_query_86() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, objects, mut units, unit)) = setup(false) else {
        return;
    };
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
    assert_eq!(
        path.calculate(
            &mut geometry,
            &context,
            Err(refuse("execution.pathfinder.transport_owner")),
            [0.; 3],
            [1.; 3],
            true,
            false
        )
        .unwrap_err()
        .id,
        "execution.pathfinder.transport_owner"
    );
    assert_eq!(path.end, [1.; 3]);
    assert!(path.current_query.is_none());
}
#[test]
fn absent_endpoint_tile_shortcut_and_same_poly_have_distinct_types_169_517() {
    if !source_host() {
        return;
    }
    for mesh in [false, true] {
        let Some((mut geometry, objects, mut units, unit)) = setup(mesh) else {
            return;
        };
        let context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
        path.transport = SourcePathTransport::None;
        assert!(path
            .calculate(
                &mut geometry,
                &context,
                Ok(None),
                [5., 5., 0.],
                [6., 5., 0.],
                true,
                false
            )
            .unwrap());
        assert_eq!(
            path.path_type,
            if mesh { NORMAL } else { NORMAL | NOT_USING }
        );
        assert_eq!(path.cache.active_len(), usize::from(mesh));
        assert_eq!(path.points, [[5., 5., 0.], [6., 5., 0.]]);
        assert_eq!(geometry.pathfinder_calculate_completions, 1);
    }
}
#[test]
fn dungeon_point_limits_use_strict_thresholds_and_retain_shorter_limit_400_408() {
    let mut cache = PathCache::new();
    for (distance, expected) in [
        (100., 74),
        (100.00001, 148),
        (200., 148),
        (200.00002, 222),
        (300., 222),
        (300.00003, 296),
        (1., 296),
    ] {
        cache.dungeon_limit(distance);
        assert_eq!(cache.point_limit(), expected);
    }
}
#[test]
fn force_destination_uses_strict_point_three_squared_threshold_969() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, objects, mut units, unit)) = setup(true) else {
        return;
    };
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    for (actual, keeps_prefix) in [(6., true), (4., false)] {
        let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
        path.transport = SourcePathTransport::None;
        path.current_mesh_map = Some(0);
        path.current_query = path.read_default_query().unwrap().cloned();
        path.start = [0., 5., 0.];
        path.end = [10., 5., 0.];
        path.actual_end = path.end;
        path.force_destination = true;
        path.straight_line = true;
        path.path_type = INCOMPLETE;
        path.build_point_path(&geometry, yzx(path.start), yzx([actual, 5., 0.]))
            .unwrap();
        assert_eq!(path.path_type, NORMAL | NOT_USING);
        assert_eq!(path.actual_end, path.end);
        assert_eq!(path.points.len() > 2, keeps_prefix);
    }
}
#[test]
fn exact_full_straight_step_limit_is_short_before_force_destination_953() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, objects, mut units, unit)) = setup(true) else {
        return;
    };
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
    path.transport = SourcePathTransport::None;
    path.current_mesh_map = Some(0);
    path.current_query = path.read_default_query().unwrap().cloned();
    path.start = [5., 5., 0.];
    path.end = [297., 5., 0.];
    path.actual_end = path.end;
    path.straight_line = true;
    path.force_destination = true;
    path.path_type = INCOMPLETE;
    path.build_point_path(&geometry, yzx(path.start), yzx(path.end))
        .unwrap();
    assert_eq!(path.path_type, SHORT);
    assert_eq!(path.points, [path.start, path.end]);
}
#[test]
fn normalize_refuses_real_allowed_z_after_shortcut_points_exist_999() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, objects, mut units, unit)) = setup(false) else {
        return;
    };
    let config = b"mmap.enabled = 1\nmmap.ignoreMapIds = \"\"\nmmap.preload = 0\nPathFinder.NormalizeZ = 1\n";
    geometry
        .mmap
        .initialize_recorded_world(config, &Sha256Id::of(config))
        .unwrap();
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, false).unwrap();
    path.transport = SourcePathTransport::None;
    assert_eq!(
        path.calculate(
            &mut geometry,
            &context,
            Ok(None),
            [0.; 3],
            [1.; 3],
            true,
            false
        )
        .unwrap_err()
        .id,
        "execution.pathfinder.allowed_position_z_owner_999"
    );
    assert_eq!(path.points, [[0.; 3], [1.; 3]]);
    assert_eq!(geometry.pathfinder_calculate_completions, 0);
}

#[test]
fn multi_poly_reuses_the_old_subrange_and_retains_incomplete_type_563_789() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, objects, mut units, unit)) = setup(true) else {
        return;
    };
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
    path.calculate(
        &mut geometry,
        &context,
        Ok(None),
        [5., 5., 0.],
        [5., 15., 0.],
        false,
        false,
    )
    .unwrap();
    assert_eq!(path.path_type, NORMAL);
    assert_eq!(path.cache.active_len(), 2);
    assert_eq!(
        path.points,
        [[5., 5., 0.], [5., 9., 0.5], [5., 13., 0.5], [5., 15., 0.]]
    );
    let storage = path.cache.storage().to_vec();
    path.path_type = INCOMPLETE;
    path.calculate(
        &mut geometry,
        &context,
        Ok(None),
        [5., 5., 0.],
        [5., 15., 0.],
        false,
        false,
    )
    .unwrap();
    assert_eq!(path.path_type, INCOMPLETE);
    assert_eq!(path.cache.storage(), storage);
}
#[test]
fn stepped_points_use_detail_height_but_preserve_the_original_endpoint_837_845() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, objects, mut units, unit)) = setup(true) else {
        return;
    };
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
    path.current_mesh_map = Some(0);
    path.current_query = path.read_default_query().unwrap().cloned();
    path.start = [5., 5., -2.];
    path.end = [5., 15., -2.];
    path.actual_end = path.end;
    path.straight_line = true;
    path.path_type = NORMAL;
    path.build_point_path(&geometry, yzx(path.start), yzx(path.end))
        .unwrap();
    assert_eq!(
        path.points,
        [[5., 5., -2.], [5., 9., 0.], [5., 13., 0.], [5., 15., -2.]]
    );
}
#[test]
fn wet_calculate_shortcuts_without_random_points_separate_los_call_432_434_1432() {
    if !source_host() {
        return;
    }
    let Some((geometry, objects, mut units, unit)) = setup(true) else {
        return;
    };
    let Some(mut wet) = super::super::tests::geometry(1, 1) else {
        return;
    };
    wet.mmap = geometry.mmap;
    wet.pathfinder_map_types = geometry.pathfinder_map_types;
    let f = units.get_mut(&objects, unit).unwrap();
    f.can_walk = false;
    f.can_swim = true;
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut path = SourcePathFinder::construct(&mut wet, unit, &context, true).unwrap();
    path.calculate(
        &mut wet,
        &context,
        Ok(None),
        [5., 5., 0.],
        [5., 15., 0.],
        false,
        false,
    )
    .unwrap();
    assert_eq!(path.path_type, NORMAL | NOT_USING);
    assert_eq!(path.points, [[5., 5., 0.], [5., 15., 0.]]);
    assert_eq!(path.cache.active_len(), 0);
}

#[test]
fn force_destination_rebuilds_at_exact_point_three_and_keeps_only_below_969() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, objects, mut units, unit)) = setup(true) else {
        return;
    };
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    for (z, active) in [
        (f32::from_bits(0x3f7f_fffc), 1),
        (1., 0),
        (f32::from_bits(0x3f80_0001), 0),
    ] {
        let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
        path.current_mesh_map = Some(0);
        path.current_query = path.read_default_query().unwrap().cloned();
        path.cache.same_poly(0x0010_0000_0000_0000);
        path.start = [0.; 3];
        path.end = [3., 1., 0.];
        path.actual_end = path.end;
        path.force_destination = true;
        path.straight_line = true;
        path.path_type = INCOMPLETE;
        path.build_point_path(&geometry, yzx(path.start), yzx([2., 0., z]))
            .unwrap();
        assert_eq!(path.cache.active_len(), active);
        assert_eq!(path.path_type, NORMAL | NOT_USING);
        assert_eq!(path.actual_end, [3., 1., 0.]);
    }
}
#[test]
fn home_consumes_bound_passenger_state_and_calculates_before_spline_init_66_70() {
    if !source_host() {
        return;
    }
    let Some((geometry, mut objects, mut units, unit)) = setup(false) else {
        return;
    };
    objects.get_mut(unit).unwrap().active = true;
    units.get_mut(&objects, unit).unwrap().evade_home = true;
    let mut passengers = SourcePassengers::default();
    passengers
        .bind_unit(
            unit,
            PassengerMovement {
                transport: None,
                guid: 0,
                time: 0,
                offset: SourcePose([0; 4]),
                on_transport: false,
                movement_pose: objects.pose(unit).unwrap(),
                ghost: false,
                pet: None,
                mini_pet: None,
            },
            &objects,
        )
        .unwrap();
    struct Home {
        geometry: SourceTerrainManager,
        passengers: SourcePassengers,
    }
    impl MotionExecutionOwner for Home {
        fn pathfinder_geometry(&mut self) -> Result<&mut SourceTerrainManager> {
            Ok(&mut self.geometry)
        }
        fn pathfinder_passengers(&self) -> Result<&SourcePassengers> {
            Ok(&self.passengers)
        }
        fn combat_start_position(&mut self, _: ObjectHandle) -> Result<[f32; 4]> {
            Ok([1.; 4])
        }
        fn home_distance(&mut self, _: ObjectHandle, _: [f32; 4]) -> Result<f32> {
            Ok(3.)
        }
    }
    let mut owner = Home {
        geometry,
        passengers,
    };
    let mut master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 0,
            default_path: 0,
        },
    );
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        SourceHomeGenerator::new(true)
            .initialize(&mut master, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.spline.movement_info_owner_176"
    );
    assert_eq!(owner.geometry.pathfinder_constructor_completions, 1);
    assert_eq!(owner.geometry.pathfinder_calculate_completions, 1);
}

#[test]
fn step_count_refuses_exact_undefined_conversion_boundary_813() {
    for (value, expected) in [
        (-0.99999994, 0),
        (-0., 0),
        (0., 0),
        (1.99, 1),
        (f32::from_bits(0x4f7f_ffff), 4294967040),
    ] {
        assert_eq!(step_count(value).unwrap(), expected);
    }
    for value in [
        -1.,
        f32::from_bits(0x4f80_0000),
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ] {
        assert_eq!(
            step_count(value).unwrap_err().id,
            "execution.pathfinder.step_count_conversion_undefined_813"
        );
    }
}
#[test]
fn ray_hit_consumes_width_before_threshold_and_keeps_source_end_replacement_723_744() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, objects, mut units, unit)) = setup(true) else {
        return;
    };
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    for (width, expected) in [
        (None, None),
        (Some(0.), Some(INCOMPLETE)),
        (Some(0.2), Some(NOPATH)),
    ] {
        let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
        path.collision_width = width;
        let result = path.calculate(
            &mut geometry,
            &context,
            Ok(None),
            [5., 5., 0.],
            [25., 5., 0.],
            false,
            true,
        );
        if let Some(expected) = expected {
            result.unwrap();
            assert_eq!(path.path_type, expected);
            if expected == INCOMPLETE {
                assert_eq!(path.end, [10., 5., 0.]);
                assert_eq!(path.points, [[5., 5., 0.], [9., 5., 0.], [10., 5., 0.]]);
            } else {
                assert!(path.points.is_empty());
            }
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.pathfinder.collision_width_owner_723"
            );
            assert!(!path.cache.active().is_empty());
            assert!(path.points.is_empty());
        }
    }
}
#[test]
fn attached_query_never_substitutes_the_default_map_query_87() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, mut objects, mut units, unit)) = setup(false) else {
        return;
    };
    let transport = objects.construct(1, SourceObjectKind::Transport).unwrap();
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
    assert_eq!(
        path.calculate(
            &mut geometry,
            &context,
            Ok(Some(transport)),
            [0.; 3],
            [1.; 3],
            true,
            false
        )
        .unwrap_err()
        .id,
        "execution.pathfinder.transport_model_query_owner_87"
    );
    assert!(path.current_query.is_none());
    assert_eq!(geometry.pathfinder_calculate_completions, 0);
    assert!(SourceHomeGenerator::new(true).runs_home());
    assert!(!SourceHomeGenerator::new(false).runs_home());
}

#[test]
fn disabled_query_defers_attachment_until_active_normalization_167_993() {
    if !source_host() {
        return;
    }
    for (normalize, bound, expected) in [
        (false, false, None),
        (true, false, Some("execution.pathfinder.transport_owner")),
        (
            true,
            true,
            Some("execution.pathfinder.allowed_position_z_owner_999"),
        ),
    ] {
        let Some((mut geometry, objects, mut units, unit)) = setup(false) else {
            return;
        };
        let config=format!("mmap.enabled = 0\nmmap.ignoreMapIds = \"\"\nmmap.preload = 0\nPathFinder.NormalizeZ = {}\n",u8::from(normalize));
        geometry
            .mmap
            .initialize_recorded_world(config.as_bytes(), &Sha256Id::of(config.as_bytes()))
            .unwrap();
        let context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, false).unwrap();
        assert!(matches!(path.default_query, SourceDefaultQuery::Unwritten));
        let attachment = if bound {
            Ok(None)
        } else {
            Err(refuse("execution.pathfinder.transport_owner"))
        };
        let result = path.calculate(
            &mut geometry,
            &context,
            attachment,
            [0.; 3],
            [1.; 3],
            false,
            false,
        );
        if let Some(expected) = expected {
            assert_eq!(result.unwrap_err().id, expected);
        } else {
            assert!(result.unwrap());
            assert_eq!(path.path_type, NORMAL | NOT_USING);
        }
        assert!(path.current_query.is_none());
        assert_eq!(path.points, [[0.; 3], [1.; 3]]);
    }
}
#[test]
fn normalization_reads_the_recorded_key_before_ignore_normalization_990() {
    if !source_host() {
        return;
    }
    let Some((mut geometry, objects, mut units, unit)) = setup(false) else {
        return;
    };
    let config = b"mmap.enabled = 1\nmmap.ignoreMapIds = \"\"\nmmap.preload = 0\n";
    geometry
        .mmap
        .initialize_recorded_world(config, &Sha256Id::of(config))
        .unwrap();
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
    assert_eq!(
        path.calculate(
            &mut geometry,
            &context,
            Ok(None),
            [0.; 3],
            [1.; 3],
            false,
            false
        )
        .unwrap_err()
        .id,
        "execution.spawn_loader.config_missing.PathFinder.NormalizeZ"
    );
    assert_eq!(path.points, [[0.; 3], [1.; 3]]);
}

#[test]
fn dungeon_map_owner_grows_limits_before_the_missing_poly_branch_395_432() {
    if !source_host() {
        return;
    }
    for kind in [0, 1, 2, 3] {
        for (distance, dungeon_limit) in [
            (100., 74),
            (101., 148),
            (200., 148),
            (201., 222),
            (300., 222),
            (301., 296),
        ] {
            let Some((mut geometry, objects, mut units, unit)) = setup(true) else {
                return;
            };
            geometry.pathfinder_map_types = Some(BTreeMap::from([(0, kind)]));
            let context = MotionContext {
                objects: &objects,
                units: &mut units,
            };
            let mut path =
                SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
            path.calculate(
                &mut geometry,
                &context,
                Ok(None),
                [5., 5., 0.],
                [5., 5. + distance, 0.],
                true,
                false,
            )
            .unwrap();
            assert_eq!(
                path.cache.point_limit(),
                if kind == 1 || kind == 2 {
                    dungeon_limit
                } else {
                    74
                }
            );
            assert_eq!(path.path_type, NOPATH);
            assert_eq!(path.points, [[5., 5., 0.], [5., 5. + distance, 0.]]);
        }
    }
}
#[test]
fn newly_enabled_default_pointer_refuses_only_after_the_attachment_read_93() {
    if !source_host() {
        return;
    }
    for forced_at_construction in [false, true] {
        let Some((mut geometry, objects, mut units, unit)) = setup(false) else {
            return;
        };
        let config=b"mmap.enabled = 1\nmmap.ignoreMapIds = \"0\"\nmmap.preload = 0\nPathFinder.NormalizeZ = 0\n";
        geometry
            .mmap
            .initialize_recorded_world(config, &Sha256Id::of(config))
            .unwrap();
        if forced_at_construction {
            units.get_mut(&objects, unit).unwrap().flags |= 8;
        }
        let context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
        context.units.get_mut(&objects, unit).unwrap().flags |= 8;
        let result = path.calculate(
            &mut geometry,
            &context,
            Ok(None),
            [0.; 3],
            [1.; 3],
            false,
            false,
        );
        if forced_at_construction {
            assert!(result.unwrap());
            assert_eq!(path.path_type, NORMAL | NOT_USING);
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.pathfinder.undefined_default_query_read"
            );
            assert_eq!(path.transport, SourcePathTransport::None);
            assert!(path.current_query.is_none());
            assert_eq!(path.end, [1.; 3]);
        }
    }
}

#[test]
fn set_current_mesh_checks_query_lifetime_at_the_first_dereference_97() {
    if !source_host() {
        return;
    }
    for unload in [false, true] {
        let Some((mut geometry, objects, mut units, unit)) = setup(false) else {
            return;
        };
        let context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let mut path = SourcePathFinder::construct(&mut geometry, unit, &context, true).unwrap();
        if unload {
            assert!(geometry.mmap.unload_instance(0, 7));
        }
        let result = path.calculate(
            &mut geometry,
            &context,
            Ok(None),
            [0.; 3],
            [1.; 3],
            true,
            false,
        );
        assert_eq!(path.transport, SourcePathTransport::None);
        assert_eq!(path.end, [1.; 3]);
        if unload {
            assert_eq!(result.unwrap_err().id, "execution.mmap.stale_query");
            assert!(path.current_mesh_map.is_none());
            assert!(path.points.is_empty());
            assert_eq!(geometry.pathfinder_calculate_completions, 0);
        } else {
            assert!(result.unwrap());
            assert_eq!(path.current_mesh_map, Some(0));
            assert_eq!(path.points, [[0.; 3], [1.; 3]]);
            assert_eq!(geometry.pathfinder_calculate_completions, 1);
        }
    }
}
