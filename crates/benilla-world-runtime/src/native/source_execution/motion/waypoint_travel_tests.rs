use super::*;

#[test]
fn source_constructor_and_speed_type_precedence() {
    let mut m = SourceCreatureMovement::construct();
    assert_eq!(m.flags, Some(0x100));
    assert_eq!(m.rates, [Some(1.); 6]);
    for (flags, kind, speed) in [
        (0, 1, 7f32),
        (0x100, 0, 2.5),
        (2, 2, 4.5),
        (0x102, 0, 2.5),
        (0x200100, 3, 4.722222),
        (0x200102, 4, 2.5),
    ] {
        m.flags = Some(flags);
        assert_eq!(source_movement_speed_type(flags), kind);
        assert_eq!(m.speed().unwrap().to_bits(), speed.to_bits());
    }
    m.flags = None;
    assert_eq!(
        m.speed().unwrap_err().id,
        "execution.waypoint.movement_flags_owner_261"
    );
    m.flags = Some(0);
    m.rates[1] = None;
    assert_eq!(
        m.speed().unwrap_err().id,
        "execution.waypoint.speed_rate_owner_262"
    );
}
#[test]
fn creation_flags_follow_source_order_and_preserve_existing_swimming() {
    let mut template = super::super::super::motion_master_tests::template();
    template.inhabit_type = 6;
    let spawn = LoadedCreatureSpawnTemplate {
        npc_flags: -1,
        unit_flags: -1,
        faction: 0,
        model_id: 0,
        equipment_id: -1,
        cur_health: 0,
        cur_mana: 0,
        spawn_flags: 7,
        relay_id: 0,
        string_id: 0,
    };
    let mut movement = SourceCreatureMovement::construct();
    let mut state = 0;
    movement
        .init_entry_flags(
            &template,
            false,
            false,
            Ok(Some(&spawn)),
            || Ok(true),
            |movement| movement.set_walk(&mut state, 0, false, true).map(|_| ()),
        )
        .unwrap();
    assert_eq!(movement.flags, Some(0x40200400));
    template.inhabit_type = 1;
    movement
        .init_entry_flags(
            &template,
            false,
            false,
            Ok(None),
            || panic!("short-circuited query"),
            |_| panic!("no spawn settings"),
        )
        .unwrap();
    assert_eq!(movement.flags, Some(0x40200000));
    movement.set_walk(&mut state, 0, true, false).unwrap();
    assert_eq!(movement.speed().unwrap().to_bits(), 4.722222f32.to_bits());
    let mut movement = SourceCreatureMovement::construct();
    template.inhabit_type = 6;
    assert_eq!(
        movement
            .init_entry_flags(
                &template,
                false,
                false,
                Ok(Some(&spawn)),
                || Err(refuse("authored.swimmable")),
                |_| panic!("swimmable refusal precedes SetWalk")
            )
            .unwrap_err()
            .id,
        "authored.swimmable"
    );
    assert_eq!(movement.flags, Some(0x500));
    movement
        .init_entry_flags(
            &template,
            true,
            true,
            Ok(Some(&spawn)),
            || Ok(false),
            |movement| movement.set_walk(&mut state, 0, false, true).map(|_| ()),
        )
        .unwrap();
    assert_eq!(movement.flags, Some(0x400));
}
#[test]
fn append_keeps_first_point_and_overwrites_accumulated_time() {
    let mut path = vec![[0.; 3]];
    // Loop sum is 2000 + 4000 + 8000; return is direct-to-end 3000, not 14000.
    let generated = [[99.; 3], [2., 0., 0.], [4., 0., 0.], [8., 0., 0.]];
    assert_eq!(
        source_waypoint_append_path(&mut path, &generated, [3., 0., 0.], 1.).unwrap(),
        3000
    );
    assert_eq!(path, [[0.; 3], [2., 0., 0.], [4., 0., 0.], [8., 0., 0.]]);
    assert_eq!(
        source_waypoint_append_path(&mut path, &[], [9., 0., 0.], 1.).unwrap(),
        1000
    );
}
#[test]
fn each_compound_assignment_converts_before_appending() {
    let mut path = vec![[0.; 3]];
    // Each term fits uint32, but the third firstPoint-relative accumulation does not.
    let generated = [
        [0.; 3],
        [2_000_000., 0., 0.],
        [2_000_000., 0., 0.],
        [2_000_000., 0., 0.],
    ];
    assert_eq!(
        source_waypoint_append_path(&mut path, &generated, [1., 0., 0.], 1.)
            .unwrap_err()
            .id,
        "execution.waypoint.travel_conversion_undefined_279"
    );
    assert_eq!(path.len(), 3);
}
#[test]
fn conversion_boundaries_and_ieee_zero_division_are_reached_at_each_assignment() {
    for accumulation in [true, false] {
        let site = if accumulation { "279" } else { "284" };
        for (value, expected) in [
            (-0.999, Some(0)),
            (-1., None),
            (0., Some(0)),
            (f32::from_bits(0x4f7fffff), Some(4294967040)),
            (4294967296., None),
            (f32::NAN, None),
            (f32::INFINITY, None),
            (f32::NEG_INFINITY, None),
        ] {
            match expected {
                Some(n) => assert_eq!(travel_uint(value, 1., accumulation).unwrap(), n),
                None => assert_eq!(
                    travel_uint(value, 1., accumulation).unwrap_err().id,
                    format!("execution.waypoint.travel_conversion_undefined_{site}")
                ),
            }
        }
        for speed in [0., -0.] {
            let mut path = vec![[0.; 3]];
            let generated = if accumulation {
                vec![[0.; 3], [1., 0., 0.]]
            } else {
                vec![]
            };
            assert_eq!(
                source_waypoint_append_path(&mut path, &generated, [1., 0., 0.], speed)
                    .unwrap_err()
                    .id,
                format!("execution.waypoint.zero_speed_conversion_undefined_{site}")
            );
            assert_eq!(path.len(), 1);
        }
    }
    assert_eq!(
        distance([f32::MAX, 0., 0.], [0.; 3]).unwrap(),
        f32::INFINITY
    );
    let mut path = vec![[0.; 3]];
    assert_eq!(
        source_waypoint_append_path(&mut path, &[], [f32::MAX, 0., 0.], f32::INFINITY)
            .unwrap_err()
            .id,
        "execution.waypoint.travel_conversion_undefined_284"
    );
    assert_eq!(
        source_waypoint_append_path(&mut path, &[], [0.; 3], 1.).unwrap(),
        0
    );
    assert_eq!(
        source_waypoint_append_path(&mut vec![], &[], [0.; 3], 1.)
            .unwrap_err()
            .id,
        "execution.waypoint.path_back_undefined_259"
    );
}
#[test]
fn movement_component_does_not_change_inventory_bytes() {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut units = SourceUnits::default();
    units
        .register(
            &objects,
            unit,
            SourceUnitFields::creature(&super::super::super::motion_master_tests::template()),
        )
        .unwrap();
    let before = serde_json::to_vec(&units.inventory(&objects).unwrap()).unwrap();
    units.get_mut(&objects, unit).unwrap().movement = Some(SourceCreatureMovement::construct());
    assert_eq!(
        serde_json::to_vec(&units.inventory(&objects).unwrap()).unwrap(),
        before
    );
}

fn lookahead(nodes: &[(f32, u32)], speed: f32) -> (Result<u32>, VecDeque<i32>) {
    let path: SourceWaypointPath = nodes
        .iter()
        .enumerate()
        .map(|(i, &(x, delay))| {
            (
                i as u32,
                SourceWaypointNode {
                    position: [x, 0., 0.],
                    orientation: None,
                    delay,
                    script: 0,
                },
            )
        })
        .collect();
    let mut generated = vec![path[&0].position];
    let mut indexes = VecDeque::new();
    let result = source_waypoint_lookahead(
        &path,
        0,
        &mut generated,
        0,
        &mut indexes,
        |cursor, _| Ok(Some((cursor + 1) % path.len() as u32)),
        |points, end| {
            let start = *points.last().unwrap();
            source_waypoint_append_path(points, &[start, end], end, speed)
        },
    );
    (result, indexes)
}
#[test]
fn lookahead_proves_nontermination_without_iteration_cap() {
    let (result, indexes) = lookahead(&[(0., 0)], 2.5);
    assert_eq!(
        result.unwrap_err().id,
        "execution.waypoint.minimum_path_time_nonterminating_cycle_370"
    );
    assert_eq!(indexes, [0]);
    let (result, indexes) = lookahead(&[(0., 0), (0.0001, 0)], 1.);
    assert_eq!(
        result.unwrap_err().id,
        "execution.waypoint.minimum_path_time_nonterminating_cycle_370"
    );
    assert_eq!(indexes, [0, 1]);
}
#[test]
fn lookahead_wraps_until_minimum_or_delayed_node() {
    let (result, indexes) = lookahead(&[(0., 0), (2., 0)], 1.);
    assert_eq!(result.unwrap(), 1);
    assert_eq!(indexes, [0, 1, 2]);
    let (result, indexes) = lookahead(&[(0., 1), (2., 0)], 1.);
    assert_eq!(result.unwrap(), 0);
    assert!(indexes.is_empty());
    let (result, indexes) = lookahead(&[(0., 0), (2., 1)], 1.);
    assert_eq!(result.unwrap(), 1);
    assert_eq!(indexes, [0]);
}

#[test]
fn default_motion_owner_uses_only_bound_live_movement() {
    struct Owner;
    impl MotionExecutionOwner for Owner {}
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut units = SourceUnits::default();
    units
        .register(
            &objects,
            unit,
            SourceUnitFields::creature(&super::super::super::motion_master_tests::template()),
        )
        .unwrap();
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        Owner.waypoint_speed(unit, &context).unwrap_err().id,
        "execution.waypoint.movement_speed_owner_261"
    );
    context.units.get_mut(&objects, unit).unwrap().movement =
        Some(SourceCreatureMovement::construct());
    assert_eq!(Owner.waypoint_speed(unit, &context).unwrap(), 2.5);
    let movement = context
        .units
        .get_mut(&objects, unit)
        .unwrap()
        .movement
        .as_mut()
        .unwrap();
    movement.set_speed_rate(0, -1.);
    assert_eq!(movement.speed().unwrap().to_bits(), 0x3ccccccc);
    movement.set_speed_rate(0, f32::NAN);
    assert!(movement.speed().unwrap().is_nan());
}

#[test]
fn every_float_accumulation_truncates_instead_of_retaining_fractions() {
    let mut generated = vec![[0.0009, 0., 0.]; 300];
    generated.push([4_294_967., 0., 0.]);
    let mut path = vec![[0.; 3]];
    assert_eq!(
        source_waypoint_append_path(&mut path, &generated, [1., 0., 0.], 1.).unwrap(),
        1000
    );
    assert_eq!(path.len(), 301);
}

#[test]
fn defined_ieee_and_fractional_negative_results_are_not_rejected_early() {
    for generated in [vec![], vec![[0.; 3], [1., 0., 0.]]] {
        assert_eq!(
            source_waypoint_append_path(
                &mut vec![[0.; 3]],
                &generated,
                [1., 0., 0.],
                f32::INFINITY
            )
            .unwrap(),
            0
        );
        assert_eq!(
            source_waypoint_append_path(&mut vec![[0.; 3]], &generated, [1., 0., 0.], -2000.)
                .unwrap(),
            0
        );
    }
    for (generated, site) in [(vec![], 284), (vec![[0.; 3]; 2], 279)] {
        assert_eq!(
            source_waypoint_append_path(&mut vec![[0.; 3]], &generated, [0.; 3], 0.)
                .unwrap_err()
                .id,
            format!("execution.waypoint.zero_speed_conversion_undefined_{site}")
        );
    }
}

#[test]
fn shared_rate_clamp_preserves_nan_until_each_travel_conversion() {
    for bits in [0x7fc12345, 0x7f812345] {
        let mut movement = SourceCreatureMovement::construct();
        movement.set_speed_rate(0, f32::from_bits(bits));
        assert_eq!(movement.rates[0].unwrap().to_bits(), bits);
        let speed = movement.speed().unwrap();
        assert!(speed.is_nan());
        for (generated, site) in [(vec![], 284), (vec![[0.; 3], [1., 0., 0.]], 279)] {
            let mut path = vec![[0.; 3]];
            assert_eq!(
                source_waypoint_append_path(&mut path, &generated, [1., 0., 0.], speed)
                    .unwrap_err()
                    .id,
                format!("execution.waypoint.travel_conversion_undefined_{site}")
            );
            assert_eq!(path, [[0.; 3]]);
        }
    }
}
