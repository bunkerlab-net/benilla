use super::super::*;

#[test]
fn wander_constructor_keeps_radius_and_source_defaults() {
    let generator = SourceWanderGenerator::new([1., 2., 3.], -0.0, 7., false, true);
    assert_eq!(generator.kind(), 1);
    assert_eq!(generator.parameters().radius.to_bits(), (-0.0f32).to_bits());
    assert_eq!(generator.parameters().vertical_z, 7.);
    assert!(!generator.parameters().walk);
    assert!(generator.parameters().random_run);
    assert_eq!(generator.timer().expiry(), 0);
    assert_eq!(generator.timer().move_count(), 1);
}

use benilla_sim::source_rng::{DrawCursor, EventKind, Operation, RawGenerator, SourceRandom};
use std::collections::VecDeque;

struct Unbound;
impl MotionExecutionOwner for Unbound {}
fn setup() -> (SourceObjects, SourceUnits, SourceMotionMaster) {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects.get_mut(unit).unwrap().alive = true;
    objects
        .set_respawn_definition(unit, SourcePose([1., 2., 3., 4.].map(f32::to_bits)), true)
        .unwrap();
    let mut units = SourceUnits::default();
    units
        .register(
            &objects,
            unit,
            SourceUnitFields::creature(&motion_master_tests::template()),
        )
        .unwrap();
    let master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 1,
            default_path: 42,
        },
    );
    (objects, units, master)
}
#[derive(Default)]
struct Lifecycle {
    calls: Vec<(MotionCall, u32)>,
    queries: Vec<&'static str>,
    finalized: bool,
    client: bool,
    controlling: Option<ObjectHandle>,
}
impl MotionExecutionOwner for Lifecycle {
    fn motion_call(
        &mut self,
        master: &mut SourceMotionMaster,
        call: MotionCall,
        context: &mut MotionContext<'_>,
    ) -> Result<()> {
        self.calls
            .push((call, context.units.get(context.objects, master.unit)?.state));
        Ok(())
    }
    fn spline_finalized(&mut self, _: ObjectHandle) -> Result<bool> {
        self.queries.push("finalized");
        Ok(self.finalized)
    }
    fn client_controlled(&mut self, _: ObjectHandle) -> Result<bool> {
        self.queries.push("client");
        Ok(self.client)
    }
    fn client_controlling(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.queries.push("controlling");
        Ok(self.controlling)
    }
}

#[test]
fn creature_constructor_copies_respawn_not_current_pose_and_extra_flag() {
    let (mut objects, mut units, master) = setup();
    objects
        .set_pose(master.unit, SourcePose([99f32.to_bits(); 4]))
        .unwrap();
    let mut template = motion_master_tests::template();
    template.extra_flags = 0x20;
    *units.get_mut(&objects, master.unit).unwrap() = SourceUnitFields::creature(&template);
    let c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let generator = SourceWanderGenerator::from_creature(master.unit, &c, 17.).unwrap();
    assert_eq!(
        generator.parameters(),
        WanderParameters {
            center: [1., 2., 3.],
            radius: 17.,
            vertical_z: 0.,
            walk: true,
            random_run: true,
        }
    );
}

#[test]
fn creature_radius_has_no_constructor_clamp_or_extra_validation() {
    let (objects, mut units, master) = setup();
    let c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    for radius in [
        0.,
        -0.,
        -1.,
        f32::MIN_POSITIVE,
        f32::MAX,
        f32::INFINITY,
        f32::NAN,
    ] {
        let generator = SourceWanderGenerator::from_creature(master.unit, &c, radius).unwrap();
        assert_eq!(generator.parameters().radius.to_bits(), radius.to_bits());
    }
}

#[test]
fn respawn_coordinate_assertion_checks_xyz_but_not_orientation_or_radius() {
    let (mut objects, mut units, master) = setup();
    for (pose, accepted) in [
        ([17066.166, -17066.166, f32::MAX, f32::NAN], true),
        ([17066.168, 0., 0., 0.], false),
        ([f32::NAN, 0., 0., 0.], false),
        ([0., f32::INFINITY, 0., 0.], false),
        ([0., 0., f32::NEG_INFINITY, 0.], false),
    ] {
        objects
            .set_respawn_definition(master.unit, SourcePose(pose.map(f32::to_bits)), true)
            .unwrap();
        let c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let result = SourceWanderGenerator::from_creature(master.unit, &c, f32::NAN);
        if accepted {
            assert!(result.is_ok());
        } else {
            assert_eq!(
                result.err().unwrap().id,
                "execution.motion.respawn_coord_assertion"
            );
        }
    }
}

#[test]
fn initialize_pushes_wander_and_sets_roaming_before_first_f6_call() {
    let (objects, mut units, mut master) = setup();
    let unit = master.unit;
    units.get_mut(&objects, unit).unwrap().target = Some(unit);
    units.get_mut(&objects, unit).unwrap().state = UNIT_STATE_MELEE;
    master.bind_respawn_radius(5.);
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Lifecycle::default();
    assert_eq!(
        master.initialize(&mut c, &mut owner).unwrap_err().id,
        "execution.motion.f6_wander_pathfinder_constructor_owner"
    );
    assert_eq!(master.current_type(), 1);
    assert_eq!(master.len(), 1);
    assert_eq!(master.path_id().unwrap(), 42);
    assert_eq!(
        c.units.get(&objects, unit).unwrap().state,
        UNIT_STATE_MELEE | WANDER_ACTIVE
    );
    assert_eq!(c.units.get(&objects, unit).unwrap().target, Some(unit));
    assert!(owner.calls.is_empty());
    assert!(owner.queries.is_empty());
}

#[test]
fn selector_defines_wander_but_leaves_other_registered_constructors_refused() {
    assert_eq!(source_default_motion(1, false).unwrap(), 1);
    assert_eq!(source_default_motion(1, true).unwrap(), 0);
    for (kind, id) in [
        (2, "execution.motion.f5_waypoint_owner"),
        (3, "execution.motion.f5_fixed_path_owner"),
        (4, "execution.motion.f5_linear_waypoint_owner"),
    ] {
        assert_eq!(source_default_motion(kind, false).unwrap_err().id, id);
    }
}

#[test]
fn initialize_zero_and_negative_radius_still_construct_without_loader_repairs() {
    for radius in [0., -1.] {
        let (objects, mut units, mut master) = setup();
        master.bind_respawn_radius(radius);
        let mut c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        assert_eq!(
            master.initialize(&mut c, &mut Unbound).unwrap_err().id,
            "execution.motion.f6_wander_pathfinder_constructor_owner"
        );
        assert_eq!(master.current_type(), 1);
    }
}

#[test]
fn reset_zeroes_timer_before_roaming_and_f6_without_clearing_motion() {
    let (objects, mut units, mut master) = setup();
    let generator = SourceWanderGenerator::new([1.; 3], 5., 0., true, false);
    generator.add_to_random_pause_time(800, true);
    units.get_mut(&objects, master.unit).unwrap().state = WANDER_MOVING;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        generator
            .reset(&mut master, &mut c, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.motion.f6_wander_pathfinder_constructor_owner"
    );
    assert_eq!(generator.timer().expiry(), 0);
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state,
        WANDER_ACTIVE | WANDER_MOVING
    );
}

#[test]
fn interrupt_refuses_before_clearing_motion_without_real_interrupt_owner() {
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state = WANDER_ACTIVE | WANDER_MOVING;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    assert_eq!(
        generator
            .interrupt(&mut master, &mut c, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.motion.f6_interrupt_moving_owner"
    );
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state,
        WANDER_ACTIVE | WANDER_MOVING
    );
}

#[test]
fn interrupt_clears_only_motion_then_restores_walk_from_all_running_bits() {
    for running in [0, 0x40000, 0x400000, 0x4000000] {
        let (objects, mut units, mut master) = setup();
        units.get_mut(&objects, master.unit).unwrap().state =
            WANDER_ACTIVE | WANDER_MOVING | running;
        let mut c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
        generator.add_to_random_pause_time(700, true);
        let mut owner = Lifecycle::default();
        generator
            .interrupt(&mut master, &mut c, &mut owner)
            .unwrap();
        assert_eq!(
            owner.calls,
            [
                (
                    MotionCall::InterruptMoving,
                    WANDER_ACTIVE | WANDER_MOVING | running
                ),
                (
                    MotionCall::RestoreWalk(running == 0),
                    WANDER_ACTIVE | running
                ),
            ]
        );
        assert_eq!(generator.timer().expiry(), 700);
    }
}

#[test]
fn finalize_clears_both_states_before_control_spline_and_walk() {
    for client in [false, true] {
        let (objects, mut units, mut master) = setup();
        let f = units.get_mut(&objects, master.unit).unwrap();
        f.state = WANDER_ACTIVE | WANDER_MOVING | 0x4000000;
        f.flags |= 8;
        let mut c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
        let mut owner = Lifecycle {
            client,
            controlling: Some(master.unit),
            ..Default::default()
        };
        generator.finalize(&mut master, &mut c, &mut owner).unwrap();
        assert_eq!(
            owner.calls,
            [
                (MotionCall::RestoreClientControl(master.unit), 0x4000000),
                (
                    if client {
                        MotionCall::StopMovingForced
                    } else {
                        MotionCall::InterruptMoving
                    },
                    0x4000000
                ),
                (MotionCall::RestoreWalk(false), 0x4000000),
            ]
        );
        assert_eq!(owner.queries, ["controlling", "finalized", "client"]);
    }
}

#[test]
fn finalize_finished_spline_skips_stop_and_control_query_but_not_walk() {
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    let mut owner = Lifecycle {
        finalized: true,
        ..Default::default()
    };
    generator.finalize(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(owner.calls, [(MotionCall::RestoreWalk(true), 0)]);
    assert_eq!(owner.queries, ["finalized"]);
}

#[test]
fn finalize_missing_spline_owner_refuses_after_state_clearing() {
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state = WANDER_ACTIVE | WANDER_MOVING;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    assert_eq!(
        generator
            .finalize(&mut master, &mut c, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.motion.f6_spline_finalized_owner"
    );
    assert_eq!(c.units.get(&objects, master.unit).unwrap().state, 0);
}

#[test]
fn update_dead_returns_before_timer_state_or_spline_query() {
    let (mut objects, mut units, mut master) = setup();
    objects.get_mut(master.unit).unwrap().alive = false;
    let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    generator.add_to_random_pause_time(500, true);
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert!(!generator
        .update(&mut master, u32::MAX, &mut c, &mut Unbound)
        .unwrap());
    assert_eq!(generator.timer().expiry(), 500);
}

#[test]
fn update_all_no_free_move_bits_tick_timer_and_clear_motion_without_spline() {
    for blocked in [2, 4, 8, 0x10, 0x80, 0x100, 0x2000, 0x200000, 0x20000000] {
        let (objects, mut units, mut master) = setup();
        units.get_mut(&objects, master.unit).unwrap().state =
            WANDER_ACTIVE | WANDER_MOVING | blocked;
        let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
        generator.add_to_random_pause_time(500, true);
        let mut c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        assert!(generator
            .update(&mut master, 499, &mut c, &mut Unbound)
            .unwrap());
        assert_eq!(generator.timer().expiry(), 1);
        assert_eq!(
            c.units.get(&objects, master.unit).unwrap().state,
            WANDER_ACTIVE | blocked
        );
        assert!(generator
            .update(&mut master, u32::MAX, &mut c, &mut Unbound)
            .unwrap());
        assert_eq!(generator.timer().expiry(), 0);
    }
}

#[test]
fn update_active_spline_does_not_tick_pause() {
    let (objects, mut units, mut master) = setup();
    let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    generator.add_to_random_pause_time(500, true);
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert!(generator
        .update(&mut master, 500, &mut c, &mut Lifecycle::default())
        .unwrap());
    assert_eq!(generator.timer().expiry(), 500);
}

#[test]
fn update_finalized_ticks_pause_then_refuses_random_path_before_any_draw() {
    let (objects, mut units, mut master) = setup();
    let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    generator.add_to_random_pause_time(500, true);
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let mut owner = Lifecycle {
        finalized: true,
        ..Default::default()
    };
    assert!(generator
        .update(&mut master, 499, &mut c, &mut owner)
        .unwrap());
    assert_eq!(generator.timer().expiry(), 1);
    assert_eq!(
        generator
            .update(&mut master, 1, &mut c, &mut owner)
            .unwrap_err()
            .id,
        "execution.motion.f6_wander_random_path_owner"
    );
    assert_eq!(generator.timer().expiry(), 0);
    assert!(owner.calls.is_empty());
}

struct Rolls {
    values: VecDeque<u32>,
    calls: Vec<(u32, u32)>,
}
impl Rolls {
    fn new(values: &[u32]) -> Self {
        Self {
            values: values.iter().copied().collect(),
            calls: Vec::new(),
        }
    }
}
impl CreatureRandom for Rolls {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        panic!("Wander never requests irand")
    }
    fn creature_urand(&mut self, low: u32, high: u32) -> Result<u32> {
        self.calls.push((low, high));
        Ok(self.values.pop_front().expect("unexpected draw"))
    }
}

#[test]
fn kernel_only_pause_count_draws_before_delay_and_decrements_without_redraw() {
    let mut timer = SourceWanderTimer::default();
    let mut random = Rolls::new(&[3, 10000]);
    timer.location_result(1, false, &mut random).unwrap();
    assert_eq!(random.calls, [(1, 3), (3000, 10000)]);
    assert_eq!(timer.expiry(), 10000);
    assert_eq!(timer.move_count(), 3);
    timer.update(10000);
    timer.location_result(1, false, &mut random).unwrap();
    assert_eq!(timer.move_count(), 2);
    assert_eq!(timer.expiry(), 0);
    timer.location_result(-1, false, &mut random).unwrap();
    assert_eq!(timer.move_count(), 1);
    assert_eq!(random.calls.len(), 2);
    random.values.extend([1, 3000]);
    timer.location_result(1, false, &mut random).unwrap();
    assert_eq!(timer.expiry(), 3000);
    assert_eq!(timer.move_count(), 1);
}

#[test]
fn kernel_only_failed_launch_delays_100_or_500_without_rng_or_count_change() {
    for (controlled, expiry) in [(true, 100), (false, 500)] {
        let mut timer = SourceWanderTimer::default();
        let mut random = Rolls::new(&[]);
        timer.location_result(0, controlled, &mut random).unwrap();
        assert_eq!(timer.expiry(), expiry);
        assert_eq!(timer.move_count(), 1);
        assert!(random.calls.is_empty());
    }
}

#[test]
fn kernel_only_pause_reset_keeps_move_count_and_saturates_diff() {
    let mut timer = SourceWanderTimer::default();
    timer
        .location_result(1, false, &mut Rolls::new(&[2, 3000]))
        .unwrap();
    timer.update(2999);
    assert_eq!(timer.expiry(), 1);
    timer.update(u32::MAX);
    assert_eq!(timer.expiry(), 0);
    timer.add_pause(100, true);
    timer.reset();
    assert_eq!(timer.expiry(), 0);
    assert_eq!(timer.move_count(), 2);
}

#[test]
fn pause_unsigned_addition_and_signed_conversion_boundaries_are_defined() {
    let mut timer = SourceWanderTimer::default();
    timer.add_pause(999, false);
    assert_eq!(timer.expiry(), 0);
    timer.add_pause(-1, true);
    assert_eq!(timer.expiry(), u32::MAX);
    timer.add_pause(0, false);
    assert_eq!(timer.expiry(), 1);
    timer.add_pause(i32::MAX, true);
    timer.add_pause(0, false);
    assert_eq!(timer.expiry(), i32::MAX as u32);
    timer.add_pause(1, false);
    assert_eq!(timer.expiry(), 1);
    timer.add_pause(-1, true);
    timer.add_pause(2, false);
    assert_eq!(timer.expiry(), 1);
    timer.add_pause(100, true);
    timer.add_pause(-99, false);
    assert_eq!(timer.expiry(), 1);
    timer.add_pause(-2, false);
    assert_eq!(timer.expiry(), 1);
    timer.add_pause(i32::MIN, true);
    assert_eq!(timer.expiry(), 0x80000000);
    timer.add_pause(i32::MIN, false);
    assert_eq!(timer.expiry(), 1);
}

struct Words(VecDeque<u64>);
impl RawGenerator for Words {
    const MIN: u64 = 0;
    const MAX: u64 = u32::MAX as u64;
    fn next_word(&mut self) -> u64 {
        self.0.pop_front().expect("unexpected raw word")
    }
}
fn words(values: &[u64]) -> Words {
    Words(values.iter().copied().collect())
}

#[test]
fn kernel_only_f3_wrapper_identity_and_order_destination_walk_count_pause() {
    // The first walk word is rejected by F3's Lemire32 distribution.
    let mut engine = words(&[
        0x40000000, 0x80000000, 0x40000000, 0xffffffff, 0xffffffff, 0xffffffff,
    ]);
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    let mut sink = |e| events.push(e);
    {
        let mut random = SourceRandom::new(17, &mut engine, &mut cursor, &mut sink).unwrap();
        let [angle, range] = wander_destination_draws(&mut random, 10.).unwrap();
        assert_eq!(angle, std::f32::consts::FRAC_PI_2);
        assert_eq!(range, 5.);
        assert!(wander_walk_choice(true, false, &mut random).unwrap());
        let mut timer = SourceWanderTimer::default();
        timer.location_result(1, false, &mut random).unwrap();
        assert_eq!(timer.move_count(), 3);
        assert_eq!(timer.expiry(), 10000);
    }
    let calls = events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Begin {
                parent: None,
                operation,
                args,
                ..
            } => Some((operation, args)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        calls,
        [
            (Operation::RandNormF, [0; 3]),
            (Operation::RandNormF, [0; 3]),
            (Operation::Urand, [0, 99, 0]),
            (Operation::Urand, [1, 3, 0]),
            (Operation::Urand, [3000, 10000, 0]),
        ]
    );
    assert_eq!(cursor.raw, 6);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e.kind,
                EventKind::Rejected {
                    reason: benilla_sim::source_rng::Rejection::Lemire32,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(engine.0.is_empty());
}

#[test]
fn kernel_only_destination_zero_negative_and_large_radius_still_draw_twice() {
    for (radius, expected) in [(0., 0.), (-10., -5.), (f32::MAX, f32::MAX / 2.)] {
        let mut engine = words(&[0, 0x80000000]);
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        assert_eq!(
            wander_destination_draws(&mut random, radius).unwrap(),
            [0., expected]
        );
        assert_eq!(cursor.raw, 2);
    }
}

#[test]
fn kernel_only_walk_threshold_is_15_and_fixed_walk_never_draws() {
    for (roll, walk) in [(14, false), (15, true), (99, true)] {
        let mut random = Rolls::new(&[roll]);
        assert_eq!(wander_walk_choice(true, false, &mut random).unwrap(), walk);
        assert_eq!(random.calls, [(0, 99)]);
    }
    for walk in [true, false] {
        let mut random = Rolls::new(&[]);
        assert_eq!(wander_walk_choice(false, walk, &mut random).unwrap(), walk);
        assert!(random.calls.is_empty());
    }
}

#[test]
fn finalize_controlled_unit_refuses_missing_mover_owner_before_spline() {
    let (objects, mut units, mut master) = setup();
    let fields = units.get_mut(&objects, master.unit).unwrap();
    fields.state = WANDER_ACTIVE | WANDER_MOVING;
    fields.flags |= 8;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    assert_eq!(
        generator
            .finalize(&mut master, &mut c, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.motion.client_controlling_owner"
    );
    assert_eq!(c.units.get(&objects, master.unit).unwrap().state, 0);
}

#[test]
fn finalize_non_client_controlled_does_not_restore_beneficiary_control() {
    let (objects, mut units, mut master) = setup();
    units
        .get_mut(&objects, master.unit)
        .unwrap()
        .controlling_player = Some(master.unit);
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let generator = SourceWanderGenerator::new([0.; 3], 5., 0., true, false);
    let mut owner = Lifecycle {
        finalized: true,
        ..Default::default()
    };
    generator.finalize(&mut master, &mut c, &mut owner).unwrap();
    assert_eq!(owner.calls, [(MotionCall::RestoreWalk(true), 0)]);
    assert_eq!(owner.queries, ["finalized"]);
}
