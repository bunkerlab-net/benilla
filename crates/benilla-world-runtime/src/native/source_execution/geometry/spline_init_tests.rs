use super::*;

struct Bound {
    thread: SourceSplineThread,
    passengers: SourcePassengers,
}
impl MotionExecutionOwner for Bound {
    fn spline_thread(&mut self) -> Result<&mut SourceSplineThread> {
        Ok(&mut self.thread)
    }
    fn pathfinder_passengers(&self) -> Result<&SourcePassengers> {
        Ok(&self.passengers)
    }
    fn spline_passengers_mut(&mut self) -> Result<&mut SourcePassengers> {
        Ok(&mut self.passengers)
    }
}
fn fixture() -> (SourceObjects, SourceUnits, ObjectHandle, Bound) {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects
        .assign_guid(unit, 1, SourceGuid::with_entry(HighGuid::Creature, 1, 1))
        .unwrap();
    objects.set_pose(unit, SourcePose([0; 4])).unwrap();
    let mut fields = SourceUnitFields::creature(&super::super::motion_master_tests::template());
    // Authored constructor inputs, never stock InitEntry or a live catalog census.
    fields.movement = Some(SourceCreatureMovement::construct());
    let mut units = SourceUnits::default();
    units.register(&objects, unit, fields).unwrap();
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
                movement_pose: SourcePose([0; 4]),
                ghost: false,
                pet: None,
                mini_pet: None,
            },
            &objects,
        )
        .unwrap();
    (
        objects,
        units,
        unit,
        Bound {
            thread: SourceSplineThread::new(7),
            passengers,
        },
    )
}
#[test]
fn authored_waypoint_reset_position_reads_world_or_existing_transport_offset() {
    let (mut objects, mut units, unit, mut owner) = fixture();
    let world_pose = [1., 2., 3., 4.];
    objects
        .set_pose(unit, SourcePose(world_pose.map(f32::to_bits)))
        .unwrap();
    let transport = objects.construct(1, SourceObjectKind::GameObject).unwrap();
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(owner.waypoint_position(unit, &context).unwrap(), world_pose);
    let offset = [5., 6., 7., 8.];
    let movement = owner.passengers.movement_mut(unit).unwrap();
    movement.transport = Some(transport);
    movement.offset = SourcePose(offset.map(f32::to_bits));
    assert_eq!(owner.waypoint_position(unit, &context).unwrap(), offset);
    owner.passengers = SourcePassengers::default();
    assert_eq!(
        owner.waypoint_position(unit, &context).unwrap_err().id,
        "execution.transport.world_object_transport_unbound"
    );
    owner.passengers.construct(unit, &objects).unwrap();
    assert_eq!(owner.waypoint_position(unit, &context).unwrap(), world_pose);
}

#[test]
fn constructor_reads_live_flags_without_writing_movement_info() {
    let (objects, mut units, unit, _) = fixture();
    for flag in [WALK, 0x0080_0000, 0x4000_0000, 0x800, 0x400, ROOT] {
        units
            .get_mut(&objects, unit)
            .unwrap()
            .movement
            .as_mut()
            .unwrap()
            .flags = Some(flag);
        let context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let init = SourceMoveSplineInit::construct(unit, &context).unwrap();
        assert_eq!(init.args.flags & RUNMODE != 0, flag & WALK == 0);
        assert_eq!(
            init.args.flags & spline::FLYING != 0,
            flag != WALK && flag != ROOT
        );
        assert_eq!(flags(&context, unit).unwrap(), flag);
    }
    units.get_mut(&objects, unit).unwrap().movement = None;
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        SourceMoveSplineInit::construct(unit, &context)
            .unwrap_err()
            .id,
        "execution.spline.movement_info_owner_176"
    );
}
#[test]
fn launch_reads_prospective_speed_then_commits_flags_and_exact_packet() {
    let (objects, mut units, unit, mut owner) = fixture();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    init.move_to([3., 4., 0.]);
    init.set_walk(false);
    assert_eq!(init.launch(unit, &mut context, &mut owner).unwrap(), 715);
    assert_eq!(init.args.velocity, 7f32.to_bits());
    assert_eq!(flags(&context, unit).unwrap(), FORWARD_SPLINE);
    assert_eq!(context.spline(unit).unwrap().id(), 1);
    let packets = &context.units.get(&objects, unit).unwrap().spline_outbound;
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].transport, None);
    assert_eq!(packets[0].excluded, None);
    assert_eq!(
        packets[0].movement,
        [0u32, 0, 0, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .chain([0])
            .chain(
                [256u32, 715, 1, 3f32.to_bits(), 4f32.to_bits(), 0]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
            )
            .collect::<Vec<_>>()
    );
}
#[test]
fn invalid_and_rooted_launches_return_source_zero_without_counter_or_flag_writes() {
    let (objects, mut units, unit, mut owner) = fixture();
    units
        .get_mut(&objects, unit)
        .unwrap()
        .movement
        .as_mut()
        .unwrap()
        .flags = Some(WALK | ROOT);
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    for velocity in [-1., f32::NAN, 1.] {
        let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
        init.move_to([1., 0., 0.]);
        init.set_walk(false);
        init.set_velocity(velocity);
        assert_eq!(init.launch(unit, &mut context, &mut owner).unwrap(), 0);
        assert_eq!(flags(&context, unit).unwrap(), WALK | ROOT);
        assert!(context.spline_finalized(unit).unwrap());
        assert!(!context.spline(unit).unwrap().initialized());
        assert_eq!(context.spline_current_index(unit).unwrap(), 1);
        assert_eq!(context.fixed_path_current_index(unit).unwrap(), 1);
        let state = context.fixed_path_spline(unit).unwrap();
        assert_eq!(state.destination, [0.; 3]);
        assert!(state.finalized);
    }
    assert_eq!(owner.thread.take_id(), 1);
}
#[test]
fn empty_rooted_path_is_repaired_and_launches_for_one_millisecond() {
    let (objects, mut units, unit, mut owner) = fixture();
    units
        .get_mut(&objects, unit)
        .unwrap()
        .movement
        .as_mut()
        .unwrap()
        .flags = Some(ROOT);
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    assert_eq!(init.launch(unit, &mut context, &mut owner).unwrap(), 1);
    assert_eq!(init.args.path, vec![[0; 3]; 2]);
    assert_eq!(flags(&context, unit).unwrap(), ROOT | FORWARD_SPLINE);
}
#[test]
fn unfinished_launch_uses_retained_spline_position_not_stale_world_pose() {
    let (objects, mut units, unit, mut owner) = fixture();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    init.move_to([10., 0., 0.]);
    init.set_velocity(10.);
    assert_eq!(init.launch(unit, &mut context, &mut owner).unwrap(), 1001);
    context
        .units
        .get_mut(&objects, unit)
        .unwrap()
        .movespline
        .update_state(500, |_| {})
        .unwrap();
    let expected = context.spline(unit).unwrap().compute_position().unwrap();
    let mut next = SourceMoveSplineInit::construct(unit, &context).unwrap();
    next.move_to([20., 0., 0.]);
    next.launch(unit, &mut context, &mut owner).unwrap();
    assert_eq!(next.args.path[0], [expected[0], expected[1], expected[2]]);
    assert_ne!(next.args.path[0], [0; 3]);
    assert_eq!(context.spline(unit).unwrap().id(), 2);
}
#[test]
fn finalized_stop_does_not_read_transport_but_forced_stop_emits_done() {
    struct Missing;
    impl MotionExecutionOwner for Missing {}
    let (objects, mut units, unit, mut owner) = fixture();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    init.stop(unit, false, &mut context, &mut Missing).unwrap();
    init.stop(unit, true, &mut context, &mut owner).unwrap();
    assert!(context.spline_finalized(unit).unwrap());
    assert!(!context.spline(unit).unwrap().initialized());
    assert_eq!(flags(&context, unit).unwrap(), WALK);
    assert_eq!(
        context.units.get(&objects, unit).unwrap().spline_outbound[0].movement,
        [0u32, 0, 0, 1]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .chain([1])
            .collect::<Vec<_>>()
    );
}
#[test]
fn in_world_empty_send_commits_id_flags_spline_and_outbound() {
    let (mut objects, mut units, unit, mut owner) = fixture();
    objects.get_mut(unit).unwrap().in_world = true;
    let before = serde_json::to_vec(units.get(&objects, unit).unwrap()).unwrap();
    let world = crate::native::synthetic_fixture::world();
    let saved = world.checkpoint().unwrap().encode().unwrap();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    init.move_to([1., 0., 0.]);
    init.launch(unit, &mut context, &mut owner).unwrap();
    assert_eq!(
        serde_json::to_vec(context.units.get(&objects, unit).unwrap()).unwrap(),
        before
    );
    assert_eq!(world.checkpoint().unwrap().encode().unwrap(), saved);
    assert_eq!(
        SourceManagers::default()
            .require_durable_checkpoint()
            .unwrap_err()
            .id,
        "execution.managers.f7_continuation_owner"
    );
    assert_eq!(context.spline(unit).unwrap().id(), 1);
    assert!(!context.spline_finalized(unit).unwrap());
    assert_eq!(flags(&context, unit).unwrap(), WALK | FORWARD_SPLINE);
    assert_eq!(
        context
            .units
            .get(&objects, unit)
            .unwrap()
            .spline_outbound
            .len(),
        1
    );
}
#[test]
fn transport_launch_uses_local_frame_and_transport_guid() {
    let (mut objects, mut units, unit, mut owner) = fixture();
    let transport = objects.construct(2, SourceObjectKind::GameObject).unwrap();
    objects
        .assign_guid(
            transport,
            2,
            SourceGuid::with_entry(HighGuid::GameObject, 9, 2),
        )
        .unwrap();
    objects
        .set_pose(
            transport,
            SourcePose([10f32.to_bits(), 20f32.to_bits(), 30f32.to_bits(), 0]),
        )
        .unwrap();
    objects
        .set_pose(
            unit,
            SourcePose([11f32.to_bits(), 22f32.to_bits(), 33f32.to_bits(), 0]),
        )
        .unwrap();
    owner.passengers.movement_mut(unit).unwrap().transport = Some(transport);
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    init.move_to([4., 6., 3.]);
    assert_eq!(init.launch(unit, &mut context, &mut owner).unwrap(), 2001);
    assert_eq!(
        init.args.path[0],
        [1f32.to_bits(), 2f32.to_bits(), 3f32.to_bits()]
    );
    assert_eq!(
        context.units.get(&objects, unit).unwrap().spline_outbound[0].transport,
        objects.get(transport).unwrap().guid
    );
}
#[test]
fn spline_counter_refuses_an_unproven_execution_thread_before_consuming_an_id() {
    let mut counter = SourceSplineThread::new(7);
    assert_eq!(
        counter.for_thread(8).unwrap_err().id,
        "execution.spline.execution_thread_owner_102"
    );
    assert_eq!(counter.for_thread(7).unwrap().take_id(), 1);
}

#[test]
fn thread_counters_are_shared_across_units_independent_across_threads_and_wrap() {
    let mut first = SourceSplineThread::new(7);
    let mut second = SourceSplineThread::new(8);
    assert_eq!(
        (first.take_id(), first.take_id(), second.take_id()),
        (1, 2, 1)
    );
    first.next = u32::MAX;
    assert_eq!(
        (first.take_id(), first.take_id(), first.take_id()),
        (u32::MAX, 0, 1)
    );
}
#[test]
fn completion_disables_flags_before_missing_map_clock_and_interrupt_does_not_run_early() {
    let (objects, mut units, unit, mut owner) = fixture();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    init.move_to([1., 0., 0.]);
    init.launch(unit, &mut context, &mut owner).unwrap();
    assert_eq!(
        source_update_spline_movement(unit, 1000, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.spline.map_current_clock_owner_11077"
    );
    assert!(context.spline_finalized(unit).unwrap());
    assert_eq!(flags(&context, unit).unwrap(), WALK);
    assert_eq!(
        context
            .units
            .get(&objects, unit)
            .unwrap()
            .spline_position_expiry,
        400
    );
    source_update_spline_movement(unit, 1000, &mut context, &mut owner).unwrap();
}
#[test]
fn interrupt_refuses_at_relocation_clock_before_interrupting_or_stopping() {
    let (objects, mut units, unit, mut owner) = fixture();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    init.move_to([10., 0., 0.]);
    init.launch(unit, &mut context, &mut owner).unwrap();
    assert_eq!(
        source_interrupt_moving(unit, false, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.spline.map_current_clock_owner_11077"
    );
    assert!(!context.spline_finalized(unit).unwrap());
    assert_eq!(
        context
            .units
            .get(&objects, unit)
            .unwrap()
            .spline_outbound
            .len(),
        1
    );
}
#[test]
fn launch_duration_undefined_boundary_preserves_prior_source_mutations() {
    for (end, succeeds) in [(2147483520f32, true), (2147483648f32, false)] {
        let (objects, mut units, unit, mut owner) = fixture();
        let mut context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
        init.move_to([end, 0., 0.]);
        init.set_velocity(1000.);
        let result = init.launch(unit, &mut context, &mut owner);
        if succeeds {
            assert_eq!(result.unwrap(), 2147483520);
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.spline.spline_common_duration_conversion_undefined"
            );
        }
        assert_eq!(context.spline(unit).unwrap().id(), 1);
        assert_eq!(flags(&context, unit).unwrap(), WALK | FORWARD_SPLINE);
    }
}
#[test]
fn facing_and_packet_conversions_refuse_at_source_undefined_boundaries() {
    assert_eq!(
        spline::wrap_facing((-1f32).to_bits()).unwrap(),
        (f32::from_bits(0x40c90fdb) - 1.).to_bits()
    );
    assert_eq!(spline::wrap_facing(0x80000000).unwrap(), 0x80000000);
    assert!(spline::wrap_facing((f32::from_bits(0x40c90fdb) * 2147483520.).to_bits()).is_ok());
    for value in [
        f32::INFINITY,
        f32::NAN,
        f32::from_bits(0x40c90fdb) * 2147483648.,
    ] {
        assert_eq!(
            spline::wrap_facing(value.to_bits()).unwrap_err().0,
            "facing_wrap_floor_conversion_undefined"
        );
    }
    assert!(spline::pack_xyz([536870880f32.to_bits(), 0, 0]).is_ok());
    assert_eq!(
        spline::pack_xyz([536870912f32.to_bits(), 0, 0])
            .unwrap_err()
            .0,
        "packet_xyz_conversion_undefined"
    );
    assert!(spline::pack_xyz([(-536870912f32).to_bits(), 0, 0]).is_ok());
    assert_eq!(
        spline::pack_xyz([(-536870976f32).to_bits(), 0, 0])
            .unwrap_err()
            .0,
        "packet_xyz_conversion_undefined"
    );
}
#[test]
fn validation_does_not_check_bounds_and_smooth_paths_bypass_bounds() {
    let (objects, mut units, unit, _) = fixture();
    let context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    init.move_by_path(&[[0.; 3], [256., 0., 0.], [0.; 3]], 0);
    init.set_velocity(1.);
    assert!(init.validate());
    assert!(!init.check_bounds());
    init.set_fly();
    assert!(init.check_bounds());
    init.args.path.clear();
    assert!(!init.validate());
}

#[test]
fn real_fixed_generator_launches_and_arrival_callback_precedes_next_generator() {
    let (objects, mut units, unit, mut owner) = fixture();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 0,
            default_path: 0,
        },
    );
    let generator = SourceFixedPathGenerator::from_points(
        &[[3., 4., 0.], [6., 8., 0.]],
        None,
        FixedPathOptions {
            flying: true,
            speed: 5.,
            ..Default::default()
        },
    );
    master.mutate(generator, &mut context, &mut owner).unwrap();
    assert_eq!((master.current_type(), master.len()), (3, 1));
    assert!(!context.spline_finalized(unit).unwrap());
    assert_ne!(
        context.units.get(&objects, unit).unwrap().state & WANDER_MOVING,
        0
    );
    context
        .units
        .get_mut(&objects, unit)
        .unwrap()
        .movespline
        .update_state(10000, |_| {})
        .unwrap();
    assert_eq!(
        master
            .update(10000, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.fixed_path.f5_ai_movement_inform_owner"
    );
    assert_eq!((master.current_type(), master.len()), (3, 1));
    assert_eq!(context.spline(unit).unwrap().id(), 1);
    assert_eq!(
        context
            .units
            .get(&objects, unit)
            .unwrap()
            .spline_outbound
            .len(),
        1
    );
}

#[test]
fn stack_interruption_relocates_before_interrupt_and_next_generator_launch() {
    let (objects, mut units, unit, mut owner) = fixture();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 0,
            default_path: 0,
        },
    );
    let make = || {
        SourceFixedPathGenerator::from_points(
            &[[3., 4., 0.], [6., 8., 0.]],
            None,
            FixedPathOptions {
                flying: true,
                speed: 5.,
                ..Default::default()
            },
        )
    };
    master.mutate(make(), &mut context, &mut owner).unwrap();
    assert_eq!(
        master
            .mutate(make(), &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.spline.map_current_clock_owner_11077"
    );
    assert_eq!(master.len(), 1);
    assert!(!context.spline_finalized(unit).unwrap());
    assert_eq!(context.spline(unit).unwrap().id(), 1);
    assert_eq!(
        context.units.get(&objects, unit).unwrap().state & (WANDER_ACTIVE | WANDER_MOVING),
        0
    );
    assert_eq!(
        context
            .units
            .get(&objects, unit)
            .unwrap()
            .spline_outbound
            .len(),
        1
    );
}

#[test]
fn facing_setters_preserve_union_bytes_not_written_by_source() {
    let (objects, mut units, unit, mut owner) = fixture();
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    assert_eq!(init.args.facing, [Some(0), Some(0), None]);
    init.set_facing(1.).unwrap();
    assert_eq!(init.args.facing, [Some(1f32.to_bits()), Some(0), None]);
    init.set_facing_target(SourceGuid::from_raw(0x1234_5678_9abc_def0));
    assert_eq!(
        init.args.facing,
        [Some(0x9abc_def0), Some(0x1234_5678), None]
    );
    init.move_by_path(&[[0.; 3], [1., 0., 0.], [2., 0., 0.]], 0);
    init.set_cyclic();
    init.launch(unit, &mut context, &mut owner).unwrap();
    context
        .units
        .get_mut(&objects, unit)
        .unwrap()
        .movespline
        .update_state(10000, |_| {})
        .unwrap();
    assert_eq!(
        context.spline(unit).unwrap().facing_words(),
        init.args.facing
    );
    init.set_facing_point([3., 4., 5.]);
    assert_eq!(
        init.args.facing,
        [
            Some(3f32.to_bits()),
            Some(4f32.to_bits()),
            Some(5f32.to_bits())
        ]
    );
    init.set_facing(2.).unwrap();
    assert_eq!(
        init.args.facing,
        [
            Some(2f32.to_bits()),
            Some(4f32.to_bits()),
            Some(5f32.to_bits())
        ]
    );
}

#[test]
fn transport_update_writes_local_offset_before_world_facing_and_clock() {
    let (mut objects, mut units, unit, mut owner) = fixture();
    let transport = objects.construct(2, SourceObjectKind::GameObject).unwrap();
    objects
        .assign_guid(
            transport,
            2,
            SourceGuid::with_entry(HighGuid::GameObject, 9, 2),
        )
        .unwrap();
    let transport_pose = SourcePose([10f32.to_bits(), 20f32.to_bits(), 30f32.to_bits(), 0]);
    objects.set_pose(transport, transport_pose).unwrap();
    let world = SourcePose([11f32.to_bits(), 22f32.to_bits(), 33f32.to_bits(), 0]);
    objects.set_pose(unit, world).unwrap();
    owner.passengers.movement_mut(unit).unwrap().transport = Some(transport);
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
    init.move_to([4., 6., 3.]);
    init.launch(unit, &mut context, &mut owner).unwrap();
    context
        .units
        .get_mut(&objects, unit)
        .unwrap()
        .movespline
        .update_state(500, |_| {})
        .unwrap();
    let local = context.spline(unit).unwrap().compute_position().unwrap();
    assert_eq!(
        source_update_spline_position(unit, false, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.spline.map_current_clock_owner_11077"
    );
    let state = owner.passengers.movement(unit).unwrap();
    assert_eq!(state.offset, SourcePose(local));
    assert_eq!(state.transport, Some(transport));
    assert_eq!((state.guid, state.time), (0, 0));
    assert_eq!(objects.pose(unit).unwrap(), world);
    assert_ne!(&local[..3], &world.0[..3]);
}

#[test]
fn stopped_init_relaunch_refuses_packet_index_but_fresh_init_is_defined() {
    for reuse in [true, false] {
        let (objects, mut units, unit, mut owner) = fixture();
        let mut context = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let mut init = SourceMoveSplineInit::construct(unit, &context).unwrap();
        init.stop(unit, true, &mut context, &mut owner).unwrap();
        if !reuse {
            init = SourceMoveSplineInit::construct(unit, &context).unwrap();
        }
        init.move_to([1., 0., 0.]);
        let result = init.launch(unit, &mut context, &mut owner);
        if reuse {
            assert_eq!(
                result.unwrap_err().id,
                "execution.spline.packet_first_point_read_undefined_45"
            );
            assert!(context.spline_finalized(unit).unwrap());
            assert!(!context.spline(unit).unwrap().initialized());
        } else {
            assert_eq!(result.unwrap(), 401);
            assert!(!context.spline_finalized(unit).unwrap());
        }
        assert_eq!(context.spline(unit).unwrap().id(), 2);
        assert_eq!(flags(&context, unit).unwrap(), WALK | FORWARD_SPLINE);
        assert_eq!(
            context
                .units
                .get(&objects, unit)
                .unwrap()
                .spline_outbound
                .len(),
            if reuse { 1 } else { 2 }
        );
    }
}

#[test]
fn rooted_fixed_launch_keeps_fresh_reads_and_reaches_source_arrival_callback() {
    struct Callback {
        bound: Bound,
        calls: Vec<(u32, u32)>,
    }
    impl MotionExecutionOwner for Callback {
        fn pathfinder_passengers(&self) -> Result<&SourcePassengers> {
            Ok(&self.bound.passengers)
        }
        fn fixed_path_inform(
            &mut self,
            master: &mut SourceMotionMaster,
            context: &mut MotionContext<'_>,
            kind: u32,
            point: u32,
        ) -> Result<()> {
            assert!(context.spline_finalized(master.unit)?);
            assert!(!context.spline(master.unit)?.initialized());
            self.calls.push((kind, point));
            Err(refuse("execution.fixed_path.f5_ai_movement_inform_owner"))
        }
    }
    let (objects, mut units, unit, bound) = fixture();
    units
        .get_mut(&objects, unit)
        .unwrap()
        .movement
        .as_mut()
        .unwrap()
        .flags = Some(ROOT | WALK);
    let mut owner = Callback {
        bound,
        calls: Vec::new(),
    };
    let mut context = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 0,
            default_path: 0,
        },
    );
    let generator = SourceFixedPathGenerator::from_points(
        &[[3., 4., 0.], [6., 8., 0.]],
        None,
        FixedPathOptions {
            flying: true,
            speed: 5.,
            ..Default::default()
        },
    );
    generator
        .initialize(&mut master, &mut context, &mut owner)
        .unwrap();
    assert_eq!(context.spline_current_index(unit).unwrap(), 1);
    let state = context.fixed_path_spline(unit).unwrap();
    assert_eq!(state.destination, [0.; 3]);
    assert!(state.finalized);
    assert!(!context.spline(unit).unwrap().initialized());
    assert!(!generator.progress().first_cycle);
    assert_eq!(
        context.units.get(&objects, unit).unwrap().state & WANDER_MOVING,
        0
    );
    assert_eq!(
        generator
            .update(&mut master, 0, &mut context, &mut owner)
            .unwrap_err()
            .id,
        "execution.fixed_path.f5_ai_movement_inform_owner"
    );
    assert_eq!(owner.calls, [(3, 1)]);
    assert_eq!(generator.progress().index, 0);
    assert_eq!(context.spline(unit).unwrap().id(), 0);
    assert!(context
        .units
        .get(&objects, unit)
        .unwrap()
        .spline_outbound
        .is_empty());
    assert_eq!(owner.bound.thread.take_id(), 1);
}
