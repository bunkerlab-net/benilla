// SPDX-License-Identifier: GPL-3.0-or-later
// Authored synthetic points and route evidence, never source database/asset rows.
use benilla_sim::creature_ai::{
    movement::{Check, Revision, SegmentChecks},
    Point, Real,
};
use benilla_sim::creature_motion::*;
fn p(x: f32, y: f32, z: f32) -> Point {
    Point::new(x, y, z).unwrap()
}
fn r(x: f32) -> Real {
    Real::new(x).unwrap()
}
fn pose(x: f32, y: f32, z: f32) -> Pose {
    Pose {
        position: p(x, y, z),
        orientation: Real::ZERO,
    }
}
fn input() -> UnitInputs {
    UnitInputs {
        audit: MotionAudit {
            flight_swimming_transport: Evidence::Absent,
            immobilization: Evidence::Absent,
            non_melee_casting: Evidence::Absent,
            speed_auras: Evidence::Absent,
            combat_scripts: Evidence::Absent,
            target_airborne_or_transport: Evidence::Absent,
        },
        health: 100,
        maximum_health: 100,
        walk_speed: r(2.5),
        run_speed: r(7.0),
        settings: GroundSettings {
            combat_reach: r(1.5),
            bounding_radius: r(0.5),
            collision_height: r(2.0),
            no_wounded_slowdown: false,
            backpedal_enabled: true,
            fanning_enabled: true,
        },
    }
}
fn state() -> MotionState {
    MotionState::new(
        Identity {
            creature: 17,
            map: 0,
            content: Revision([1; 32]),
            geometry: Revision([2; 32]),
        },
        pose(0., 0., 0.),
        0,
        0,
    )
    .unwrap()
}
fn target(x: f32) -> Target {
    Target {
        movement_flags: GroundMovementFlags::STOPPED,
        id: 1,
        pose: pose(x, 0., 0.),
        combat_reach: r(1.5),
        bounding_radius: r(0.5),
        collision_height: r(2.0),
    }
}
fn checks() -> SegmentChecks {
    SegmentChecks {
        loaded_coverage: Check::Passed,
        line_of_sight: Check::Passed,
        body_sweep: Check::Passed,
        continuous_ground_support: Check::Passed,
        dry_ground: Check::Passed,
    }
}
#[derive(Default)]
struct Synthetic {
    queries: Vec<Query>,
    fail: bool,
    corrupt: bool,
    partial: bool,
    unknown: bool,
    collider: Option<u64>,
    facing_target: Option<Point>,
}
impl Authority for Synthetic {
    fn resolve(&mut self, q: &Query) -> Result<Reply, MotionError> {
        self.queries.push(q.clone());
        if self.fail {
            return Err(MotionError::Missing("synthetic failure"));
        }
        let fact = match q {
            Query::FacingTarget(_) => Fact::ObjectPosition(self.facing_target),
            Query::NearPoint(n) => {
                // This authored fixture only resolves axis-aligned synthetic near points.
                let direction = if matches!(n.angle, NearPointAngle::Absolute(_))
                    || n.owner.position.x.as_f32() < n.target.pose.position.x.as_f32()
                {
                    -1.0
                } else {
                    1.0
                };
                Fact::NearPoint(p(
                    n.target.pose.position.x.as_f32() + direction * n.distance.as_f32(),
                    0.,
                    0.,
                ))
            }
            Query::Route(q) => {
                let mut c = checks();
                if self.unknown {
                    c.dry_ground = Check::Unknown;
                }
                Fact::Route(if self.partial {
                    RouteOutcome::Truncated
                } else {
                    RouteOutcome::Complete(CompleteRoute {
                        source: RouteSource::SyntheticFixture,
                        path_identity: [3; 32],
                        points: vec![q.origin, q.goal],
                        segments: vec![c],
                    })
                })
            }
            Query::Bearing(q) => {
                // Test-only libc oracle; production kernel never runs host trig.
                let mut a = (q.toward.y.as_f32() - q.origin.y.as_f32())
                    .atan2(q.toward.x.as_f32() - q.origin.x.as_f32());
                if a < 0.0 {
                    a += std::f32::consts::TAU;
                }
                Fact::Bearing(r(a))
            }
            Query::Collider(_) => Fact::Collider(self.collider),
        };
        let mut request = q.clone();
        if self.corrupt {
            if let Query::Route(ref mut r) = request {
                r.binding.query_id += 1;
            }
        }
        Ok(Reply { request, fact })
    }
}
fn paths() -> Vec<Vec<Point>> {
    vec![
        vec![p(0., 0., 0.), p(7., 0., 0.)],
        vec![p(0., 0., 0.), p(3., 4., 0.), p(3., 4., 5.), p(-1., 2., 8.)],
        vec![
            p(-22.125, 103.75, 0.25),
            p(-14.13, 103.753, 0.27),
            p(-12.1, 105.1, 0.3),
        ],
        vec![
            p(0., 0., 0.),
            p(0., 0., 0.),
            p(1., 0., 0.),
            p(1., 0., 0.),
            p(2., 0., 0.),
        ],
        vec![p(1., 1., 1.), p(1., 1., 1.)],
        vec![p(32000., -32000., 30000.), p(31999.99, -31999.9, 29999.3)],
        vec![
            p(0., 0., 0.),
            p(0.00001, 0., 0.),
            p(0.00002, 0., 0.),
            p(0.00003, 0., 0.),
        ],
    ]
}
#[test]
fn pinned_cpp_numeric_oracle() {
    let paths = paths();
    let mut count = 0;
    for line in include_str!("fixtures/creature_motion_cmangos.tsv").lines() {
        let mut f = line.split_whitespace();
        let kind = f.next().unwrap();
        let n = f.map(|x| x.parse::<u32>().unwrap()).collect::<Vec<_>>();
        match kind {
            "S" => assert_eq!(
                chase_speed(
                    Real::from_bits(n[0]).unwrap(),
                    n[1] != 0,
                    n[2],
                    n[3],
                    n[4] != 0
                )
                .unwrap()
                .bits(),
                n[5],
                "{line}"
            ),
            "L" => {
                let path = paths[n[0] as usize].clone();
                let mut s =
                    LinearSpline::launch(path.clone(), path[0], Real::from_bits(n[1]).unwrap())
                        .unwrap();
                s.advance(n[2]);
                assert_eq!(s.duration_ms(), n[3], "{line}");
                assert_eq!(s.snapshot().segment, n[4] as usize, "{line}");
                assert_eq!(s.finalized(), n[5] != 0, "{line}");
                let p = s.position().unwrap();
                assert_eq!(
                    [p.x.bits(), p.y.bits(), p.z.bits()],
                    [n[6], n[7], n[8]],
                    "{line}"
                );
            }
            _ => panic!("oracle kind"),
        };
        count += 1;
    }
    assert!(count > 1000);
}
#[test]
fn spline_bends_segments_and_discards_overshoot() {
    let mut s = LinearSpline::launch(
        vec![p(0., 0., 0.), p(3., 4., 0.), p(3., 4., 5.)],
        p(0., 0., 0.),
        r(5.),
    )
    .unwrap();
    assert_eq!(s.snapshot().timestamps_ms, vec![0, 1001, 2001]);
    assert_eq!(
        s.advance(1001),
        vec![SplineEvent::NextSegment { segment: 1 }]
    );
    assert_eq!(s.position().unwrap(), p(3., 4., 0.));
    assert_eq!(s.advance(60_000), vec![SplineEvent::Arrived]);
    assert_eq!(s.position().unwrap(), p(3., 4., 5.));
    assert_eq!(s.elapsed_ms(), 2001);
    assert!(s.advance(100).is_empty());
    assert_eq!(LinearSpline::restore(s.snapshot()).unwrap(), s);
}
#[test]
fn corrupted_spline_timestamp_segment_and_limits_fail() {
    let s = LinearSpline::launch(vec![p(0., 0., 0.), p(7., 0., 0.)], p(0., 0., 0.), r(7.)).unwrap();
    let mut bad = s.snapshot();
    bad.timestamps_ms[1] += 1;
    assert!(LinearSpline::restore(bad).is_err());
    let mut bad = s.snapshot();
    bad.elapsed_ms = 1001;
    assert!(LinearSpline::restore(bad).is_err());
    assert!(LinearSpline::launch(vec![p(0., 0., 0.); 75], p(0., 0., 0.), r(7.)).is_err());
}
#[test]
fn chase_progress_is_spline_first_and_map_relocations_follow_source_cadence() {
    let mut s = state();
    let mut a = Synthetic::default();
    let t = target(20.);
    let events = s.start_chase(t, false, input(), &mut a).unwrap();
    assert!(matches!(events[0], Event::SplineLaunched { .. }));
    assert_eq!(s.pose().position, p(0., 0., 0.));
    let first = s.advance(100, Some(t), input(), &mut a).unwrap();
    assert!(first
        .iter()
        .any(|e| matches!(e, Event::Relocated { at_ms: 100, .. })));
    let map = s.pose();
    let spline = s.spline_position().unwrap();
    assert!(spline.x.as_f32() > 0.0 && spline.x.as_f32() < 1.0);
    assert_eq!(map.position, spline);
    for _ in 0..3 {
        let e = s.advance(100, Some(t), input(), &mut a).unwrap();
        assert!(!e.iter().any(|e| matches!(e, Event::Relocated { .. })));
        assert_eq!(s.pose(), map);
    }
    assert!(s.spline_position().unwrap().x.as_f32() > map.position.x.as_f32());
    assert!(s
        .advance(100, Some(t), input(), &mut a)
        .unwrap()
        .iter()
        .any(|e| matches!(e, Event::Relocated { at_ms: 500, .. })));
    assert_eq!(s.last_move_ms(), 500);
    assert!(s.moving_and_running());
}
#[test]
fn target_replan_waits_for_250ms_poll_and_reanchors_running_spline() {
    let mut s = state();
    let mut a = Synthetic::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    s.advance(100, Some(target(20.)), input(), &mut a).unwrap();
    let count = |a: &Synthetic| {
        a.queries
            .iter()
            .filter(|q| matches!(q, Query::Route(_)))
            .count()
    };
    assert_eq!(count(&a), 1);
    s.advance(100, Some(target(30.)), input(), &mut a).unwrap();
    s.advance(100, Some(target(30.)), input(), &mut a).unwrap();
    assert_eq!(count(&a), 1);
    let events = s.advance(100, Some(target(30.)), input(), &mut a).unwrap();
    assert_eq!(count(&a), 2);
    let relocation = events
        .iter()
        .position(|e| matches!(e, Event::Relocated { .. }))
        .unwrap();
    let launch = events
        .iter()
        .position(|e| matches!(e, Event::SplineLaunched { .. }))
        .unwrap();
    assert!(relocation < launch);
    let snapshot = s.snapshot();
    assert_eq!(snapshot.spline.as_ref().unwrap().spline.elapsed_ms, 0);
    assert_eq!(
        snapshot.spline.as_ref().unwrap().spline.points[0],
        s.pose().position
    );
}
#[test]
fn source_home_reanchors_without_fabricating_immediate_relocation() {
    let mut s = state();
    let mut a = Synthetic::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    s.advance(100, Some(target(20.)), input(), &mut a).unwrap();
    s.advance(100, Some(target(20.)), input(), &mut a).unwrap();
    let old_map = s.pose();
    let computed = s.spline_position().unwrap();
    assert_ne!(old_map.position, computed);
    let destination = Pose {
        position: p(0., 0., 0.),
        orientation: r(1.0),
    };
    let events = s
        .start_home(destination, false, false, input(), &mut a)
        .unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::Relocated { .. })));
    assert_eq!(s.pose(), old_map);
    let Query::Route(q) = a.queries.last().unwrap() else {
        panic!()
    };
    assert_eq!(q.origin, old_map.position);
    assert_eq!(q.launch_origin, computed);
    assert_eq!(
        s.snapshot().spline.as_ref().unwrap().spline.points[0],
        computed
    );
    let e = s.advance(1000, None, input(), &mut a).unwrap();
    assert!(e.contains(&Event::HomeReached {
        restore_inactive: true
    }));
    assert_eq!(s.pose().orientation, r(1.0));
    assert!(!s.is_moving());
    assert_eq!(s.generator(), &Generator::Idle);
}
#[test]
fn late_route_failure_rolls_back_clock_path_pose_and_query_ids() {
    for mode in 0..3 {
        let mut s = state();
        let mut a = Synthetic::default();
        s.start_chase(target(20.), false, input(), &mut a).unwrap();
        s.advance(100, Some(target(20.)), input(), &mut a).unwrap();
        let before = s.clone();
        a.partial = mode == 0;
        a.unknown = mode == 1;
        a.corrupt = mode == 2;
        assert!(s.advance(300, Some(target(40.)), input(), &mut a).is_err());
        assert_eq!(s, before);
    }
}
#[test]
fn motion_snapshot_roundtrip_and_facing_uses_old_map_origin() {
    let mut s = state();
    let mut a = Synthetic::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    s.advance(100, Some(target(20.)), input(), &mut a).unwrap();
    let Query::Bearing(q) = a
        .queries
        .iter()
        .find(|q| matches!(q, Query::Bearing(_)))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(q.origin, p(0., 0., 0.));
    assert_ne!(q.origin, s.pose().position);
    let bytes = serde_json::to_vec(&s.snapshot()).unwrap();
    let copy = MotionState::restore(serde_json::from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(s, copy);
    let mut restored = copy;
    assert_eq!(
        s.advance(500, Some(target(20.)), input(), &mut a).unwrap(),
        restored
            .advance(500, Some(target(20.)), input(), &mut Synthetic::default())
            .unwrap()
    );
    assert_eq!(s, restored);
}
#[test]
fn source_stationary_refresh_is_strict_and_derived_from_last_relocation() {
    let mut s = state();
    let mut a = Synthetic::default();
    s.advance(3000, None, input(), &mut a).unwrap();
    assert!(!s.stationary_melee_refresh_eligible(false));
    s.advance(1, None, input(), &mut a).unwrap();
    assert!(s.stationary_melee_refresh_eligible(false));
    assert!(!s.stationary_melee_refresh_eligible(true));
}
#[test]
fn wounded_running_chase_and_unslowed_home_use_current_health() {
    let mut i = input();
    i.health = 10;
    let mut s = state();
    let mut a = Synthetic::default();
    s.start_chase(target(20.), false, i, &mut a).unwrap();
    let speed = s.snapshot().spline.unwrap().spline.velocity;
    assert!(speed.as_f32() < 7.0);
    s.start_home(pose(0., 0., 0.), false, false, i, &mut a)
        .unwrap();
    assert_eq!(s.snapshot().spline.unwrap().spline.velocity, r(7.0));
    assert_eq!(chase_speed(r(7.), false, 30, 100, false).unwrap(), r(7.));
    assert_eq!(chase_speed(r(7.), false, 29, 100, true).unwrap(), r(7.));
    assert_eq!(chase_speed(r(2.5), true, 1, 100, false).unwrap(), r(2.5));
}
#[test]
fn source_notify_is_event_phase_and_respawn_force_replaces_pending() {
    let mut n = AggroNotify::new();
    n.added_to_world(100).unwrap();
    assert!(!n.can_aggro());
    assert!(n.advance_events(100, true).unwrap());
    assert!(n.can_aggro());
    n.relocated(1000).unwrap();
    n.relocated(1).unwrap();
    assert!(!n.advance_events(999, true).unwrap());
    assert!(n.advance_events(1, true).unwrap());
    n.relocated(1).unwrap();
    n.respawned(5000).unwrap();
    assert!(!n.can_aggro());
    assert!(!n.advance_events(4999, true).unwrap());
    assert!(n.advance_events(1, true).unwrap());
    n.respawned(5000).unwrap();
    n.entered_combat();
    assert!(n.can_aggro());
    assert_eq!(n.snapshot().pending_at_ms, None);
}
#[test]
fn unknown_collision_never_means_empty_and_source_backpedal_uses_walk() {
    let mut s = state();
    let mut a = Synthetic::default();
    s.start_chase(target(0.), false, input(), &mut a).unwrap();
    s.advance(1, Some(target(0.)), input(), &mut a).unwrap();
    let e = s.advance(2000, Some(target(0.)), input(), &mut a).unwrap();
    assert!(e
        .iter()
        .any(|e| matches!(e, Event::SplineLaunched { walking: true, .. })));
    assert!(!s.moving_and_running());
    let mut s = state();
    s.start_chase(target(4.), false, input(), &mut a).unwrap();
    s.advance(1, Some(target(4.)), input(), &mut a).unwrap();
    let before = s.clone();
    a.collider = Some(999);
    assert!(matches!(
        s.advance(2000, Some(target(4.)), input(), &mut a),
        Err(MotionError::Unsupported(_))
    ));
    assert_eq!(s, before);
}

#[test]
fn current_movement_domain_must_be_resolved_on_every_tick() {
    let mut s = state();
    let mut a = Synthetic::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    for fact in [Evidence::Unknown, Evidence::Present] {
        let mut i = input();
        i.audit.speed_auras = fact;
        let before = s.clone();
        assert!(s.advance(100, Some(target(20.)), i, &mut a).is_err());
        assert_eq!(s, before);
    }
}

#[test]
fn adversarial_snapshot_indexes_and_unearned_home_arrival_fail_closed() {
    let spline =
        LinearSpline::launch(vec![p(0., 0., 0.), p(1., 0., 0.)], p(0., 0., 0.), r(7.)).unwrap();
    let mut bad = spline.snapshot();
    bad.segment = usize::MAX;
    assert!(LinearSpline::restore(bad).is_err());
    let mut s = state().snapshot();
    s.generator = Generator::Home {
        destination: pose(100., 0., 0.),
        was_active: false,
    };
    assert!(MotionState::restore(s).is_err());
    let mut s = state();
    let mut a = Synthetic::default();
    s.start_home(pose(1., 0., 0.), false, false, input(), &mut a)
        .unwrap();
    let mut bad = s.snapshot();
    bad.generator = Generator::Home {
        destination: pose(2., 0., 0.),
        was_active: false,
    };
    assert!(MotionState::restore(bad).is_err());
}
#[test]
fn kernel_motion_token_cannot_be_serialized_deserialized_or_authority_supplied() {
    use benilla_sim::creature_ai::{movement::*, MotionCommand};
    let mut s = state();
    let mut a = Synthetic::default();
    let t = target(20.);
    s.start_chase(t, false, input(), &mut a).unwrap();
    let command = MotionCommand {
        command_id: 1,
        purpose: MovementPurpose::Chase { target: 1 },
        home: None,
    };
    let update = s
        .advance_for_command(&command, 1, 100, Some(t), input(), &mut a)
        .unwrap();
    assert!(serde_json::to_vec(update.movement.result()).is_err());
    assert!(serde_json::from_str::<MovementResult>(r#"{"SourceSpline":{"position":0}}"#).is_err());
    struct Forged(MovementOutcome);
    impl GeometryAuthority for Forged {
        fn resolve(
            &mut self,
            _: &MovementRequest,
        ) -> Result<MovementOutcome, benilla_sim::creature_ai::AiError> {
            Ok(self.0.clone())
        }
    }
    let request = update.movement.request().clone();
    let mut forged = Forged(MovementOutcome {
        request_id: request.request_id,
        map_id: request.map_id,
        content_revision: request.content_revision,
        geometry_revision: request.geometry_revision,
        result: update.movement.result().clone(),
    });
    assert!(resolve_movement(&mut forged, request).is_err());
}

#[test]
fn a_zero_duration_middle_segment_restores_at_an_exact_boundary() {
    let mut spline = LinearSpline::launch(
        vec![p(0., 0., 0.), p(0., 0., 0.), p(0., 0., 0.), p(1., 0., 0.)],
        p(0., 0., 0.),
        r(7.),
    )
    .unwrap();
    spline.advance(1);
    let mut restored = LinearSpline::restore(spline.snapshot()).unwrap();
    assert_eq!(spline.advance(1), restored.advance(1));
    assert_eq!(spline, restored);
}

#[test]
fn retarget_retains_the_old_spline_facing_object_until_replan() {
    let mut s = state();
    let mut a = Synthetic::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    let mut replacement = target(20.);
    replacement.id = 2;
    s.start_chase(replacement, false, input(), &mut a).unwrap();
    a.facing_target = Some(p(0., 20., 0.));
    s.advance(100, Some(replacement), input(), &mut a).unwrap();
    assert_eq!(s.pose().orientation, r(std::f32::consts::FRAC_PI_2));
    assert!(a.queries.iter().any(|q| matches!(
        q,
        Query::FacingTarget(FacingTargetRequest { target: 1, .. })
    )));
}

#[test]
fn home_wraps_tau_and_walk_token_uses_effective_walk_rate() {
    use benilla_sim::creature_ai::{movement::MovementPurpose, MotionCommand};
    let mut s = state();
    let mut a = Synthetic::default();
    let destination = Pose {
        position: p(1., 0., 0.),
        orientation: r(std::f32::consts::TAU),
    };
    s.start_home(destination, true, false, input(), &mut a)
        .unwrap();
    assert_eq!(
        s.snapshot().spline.unwrap().facing,
        Facing::Angle(Real::ZERO)
    );
    let command = MotionCommand {
        command_id: 1,
        purpose: MovementPurpose::Home,
        home: Some(destination.position),
    };
    let advanced = s
        .advance_for_command(&command, 1, 100, None, input(), &mut a)
        .unwrap();
    assert_eq!(advanced.movement.request().speed, r(2.5));
    assert_eq!(s.pose().orientation, Real::ZERO);
}

#[test]
fn grounded_source_flags_distinguish_motion_turning_walking_and_stop() {
    let turn = GroundMovementFlags::from_bits(0x10).unwrap();
    assert!(!turn.is_moving());
    let running = GroundMovementFlags::from_bits(1).unwrap();
    assert!(running.moving_and_running());
    let walking = GroundMovementFlags::from_bits(0x101).unwrap();
    assert!(walking.is_moving());
    assert!(!walking.moving_and_running());
    assert!(!GroundMovementFlags::STOPPED.is_moving());
    for unsupported in [0x40, 0x2000, 0x00400000, 0x02000000, 0x08000000, 0x80000000] {
        assert!(GroundMovementFlags::from_bits(unsupported).is_err());
        assert!(serde_json::from_str::<GroundMovementFlags>(&unsupported.to_string()).is_err());
    }
}

#[test]
fn saved_spline_facing_must_be_half_open_even_when_map_pose_allows_tau() {
    let mut s = state();
    let mut a = Synthetic::default();
    s.start_home(pose(1., 0., 0.), false, false, input(), &mut a)
        .unwrap();
    let mut bad = s.snapshot();
    let tau = r(std::f32::consts::TAU);
    bad.spline.as_mut().unwrap().facing = Facing::Angle(tau);
    if let Generator::Home {
        ref mut destination,
        ..
    } = bad.generator
    {
        destination.orientation = tau;
    }
    assert!(MotionState::restore(bad).is_err());
}

#[test]
fn saved_route_evidence_is_revalidated_and_bound_to_corrected_spline() {
    let mut s = state();
    let mut a = Synthetic::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    let mut bad = s.snapshot();
    bad.spline.as_mut().unwrap().route.segments[0].dry_ground = Check::Unknown;
    assert!(MotionState::restore(bad).is_err());
    let mut bad = s.snapshot();
    bad.spline.as_mut().unwrap().route_request.launch_origin = p(5., 0., 0.);
    assert!(MotionState::restore(bad).is_err());
    let mut bad = s.snapshot();
    bad.spline.as_mut().unwrap().route_request.binding.now_ms = 1;
    assert!(MotionState::restore(bad).is_err());
}

#[test]
fn event_phase_launch_consumes_one_diff_while_post_ai_launch_consumes_none() {
    use benilla_sim::creature_ai::{movement::MovementPurpose, MotionCommand};
    let mut event = state();
    let mut a = Synthetic::default();
    let t = target(20.);
    let command = MotionCommand {
        command_id: 1,
        purpose: MovementPurpose::Chase { target: 1 },
        home: None,
    };
    let update = event
        .advance_for_event_command(
            &command,
            1,
            100,
            Some(t),
            input(),
            &mut a,
            FrameStart::Chase {
                target: t,
                walking: false,
            },
        )
        .unwrap();
    assert_eq!(event.now_ms(), 100);
    assert_eq!(event.snapshot().spline.unwrap().spline.elapsed_ms, 100);
    assert!(event.pose().position.x.as_f32() > 0.0);
    assert_eq!(update.movement.request().origin, p(0., 0., 0.));
    for query in a.queries {
        let at = match query {
            Query::NearPoint(q) => q.binding.now_ms,
            Query::Route(q) => q.binding.now_ms,
            Query::Bearing(q) => q.binding.now_ms,
            Query::Collider(q) => q.binding.now_ms,
            Query::FacingTarget(q) => q.binding.now_ms,
        };
        assert_eq!(at, 100);
    }
    let mut after = state();
    let mut a = Synthetic::default();
    after.advance(100, Some(t), input(), &mut a).unwrap();
    after.start_chase(t, false, input(), &mut a).unwrap();
    assert_eq!(after.now_ms(), 100);
    assert_eq!(after.snapshot().spline.unwrap().spline.elapsed_ms, 0);
    assert_eq!(after.pose().position, p(0., 0., 0.));
    // Reopening reconstructs the different launch phases and advances each once.
    for original in [&mut event, &mut after] {
        let bytes = serde_json::to_vec(&original.snapshot()).unwrap();
        let mut reopened = MotionState::restore(serde_json::from_slice(&bytes).unwrap()).unwrap();
        let mut left = Synthetic::default();
        let mut right = Synthetic::default();
        assert_eq!(
            original.advance(100, Some(t), input(), &mut left).unwrap(),
            reopened.advance(100, Some(t), input(), &mut right).unwrap()
        );
        assert_eq!(*original, reopened);
        assert_eq!(left.queries, right.queries);
    }
}

fn source_ai_profile() -> benilla_sim::creature_ai::Profile {
    use benilla_sim::creature_ai as ai;
    ai::Profile {
        creature_id: 17,
        map_id: 0,
        content_revision: Revision([1; 32]),
        spawn: p(0., 0., 0.),
        clock_origin_ms: 0,
        level: 1,
        reaction: ai::Reaction::Defensive,
        neutral_to_all: false,
        detection: r(20.),
        aggro_rate: Real::ONE,
        combat_reach: r(1.5),
        bounding_radius: r(0.5),
        melee_leeway: r(2.66),
        run_speed: r(7.),
        home_speed: r(7.),
        mainhand_interval_ms: 2000,
        pursuit_ms: 15000,
        leash_radius: r(30.),
        hard_leash_radius: None,
        corpse_decay_ms: 3000,
        respawn_delay_seconds: 5,
        unsupported: vec![],
    }
}
fn source_observation(
    sequence: u64,
    s: &MotionState,
    movement: Option<benilla_sim::creature_ai::movement::ValidatedMovement>,
) -> benilla_sim::creature_ai::Observation {
    use benilla_sim::creature_ai as ai;
    ai::Observation {
        sequence,
        elapsed_ms: 100,
        geometry_revision: Revision([2; 32]),
        target: Some(ai::Target {
            id: 1,
            position: p(20., 0., 0.),
            level: 1,
            combat_reach: r(1.5),
            bounding_radius: r(0.5),
            moving_and_running: false,
            alive: true,
            online: true,
            can_attack_in_combat: true,
            can_attack_on_sight: true,
            visible: true,
            accessible: true,
            line_of_sight: true,
        }),
        can_initiate_attack: true,
        casting_non_melee: false,
        stationary_melee_refresh_eligible: s.stationary_melee_refresh_eligible(false),
        creature_moving_and_running: s.moving_and_running(),
        movement,
        receipts: vec![],
    }
}
struct Observation(benilla_sim::creature_ai::Observation);
impl benilla_sim::creature_ai::CombatAuthority for Observation {
    fn observe(
        &mut self,
        _: u64,
    ) -> Result<benilla_sim::creature_ai::Observation, benilla_sim::creature_ai::AiError> {
        Ok(self.0.clone())
    }
}
#[test]
fn receipt_event_launch_drives_actual_ai_positions_and_rejects_a_wrong_clock_token() {
    use benilla_sim::creature_ai as ai;
    let mut creature = ai::SoloCreatureAi::new(source_ai_profile()).unwrap();
    let mut motion = state();
    let mut spatial = Synthetic::default();
    let mut observation = source_observation(1, &motion, None);
    observation.receipts.push(ai::Receipt {
        id: 1,
        event: ai::CombatEvent::AttackedBy { target: 1 },
    });
    let receipt = creature.begin_receipts(&observation).unwrap();
    let command = creature.motion().unwrap().clone();
    let update = motion
        .advance_for_event_command(
            &command,
            1,
            100,
            Some(target(20.)),
            input(),
            &mut spatial,
            FrameStart::Chase {
                target: target(20.),
                walking: false,
            },
        )
        .unwrap();
    let mut finish = source_observation(1, &motion, Some(update.movement));
    finish.receipts = observation.receipts;
    creature
        .finish_receipts(finish, receipt, &mut |_, _, _| {
            Ok(ai::MainhandResult::Unchanged)
        })
        .unwrap();
    assert_eq!(creature.position(), motion.pose().position);
    assert!(creature.position().x.as_f32() > 0.0);
    let before = creature.clone();
    let mut wrong = motion.snapshot();
    wrong.now_ms += 100;
    wrong.processed_map_clock_ms += 100;
    let mut wrong = MotionState::restore(wrong).unwrap();
    let update = wrong
        .advance_for_command(&command, 2, 100, Some(target(20.)), input(), &mut spatial)
        .unwrap();
    assert!(creature
        .step(&mut Observation(source_observation(
            2,
            &wrong,
            Some(update.movement)
        )))
        .is_err());
    assert_eq!(creature, before);
    for sequence in 2..=30 {
        let update = motion
            .advance_for_command(
                &command,
                sequence,
                100,
                Some(target(20.)),
                input(),
                &mut spatial,
            )
            .unwrap();
        creature
            .step(&mut Observation(source_observation(
                sequence,
                &motion,
                Some(update.movement),
            )))
            .unwrap();
        assert_eq!(creature.position(), motion.pose().position);
    }
    assert!(!motion.is_moving());
    assert_eq!(creature.position(), p(17.5, 0., 0.));
}

#[test]
fn source_command_rejects_a_different_target_without_advancing() {
    use benilla_sim::creature_ai::{movement::MovementPurpose, MotionCommand};
    let mut motion = state();
    let mut authority = Synthetic::default();
    let t = target(20.);
    motion
        .start_chase(t, false, input(), &mut authority)
        .unwrap();
    let command = MotionCommand {
        command_id: 1,
        purpose: MovementPurpose::Chase { target: t.id },
        home: None,
    };
    let before = motion.clone();
    let mut wrong = t;
    wrong.id += 1;
    assert!(motion
        .advance_for_command(&command, 1, 100, Some(wrong), input(), &mut authority)
        .is_err());
    assert_eq!(motion, before);
    motion
        .advance_for_command(&command, 1, 100, None, input(), &mut authority)
        .unwrap();
}

#[test]
fn early_map_event_launch_and_death_stop_preserve_source_clocks_and_distinct_map_pose() {
    use benilla_sim::creature_ai::{movement::MovementPurpose, MotionCommand};
    let mut s = state();
    let mut a = Synthetic::default();
    let t = target(20.);
    s.begin_map_frame(100).unwrap();
    s.start_chase(t, false, input(), &mut a).unwrap();
    assert_eq!(s.snapshot().spline.as_ref().unwrap().spline.elapsed_ms, 0);
    let command = MotionCommand {
        command_id: 1,
        purpose: MovementPurpose::Chase { target: 1 },
        home: None,
    };
    s.advance_for_command(&command, 1, 100, Some(t), input(), &mut a)
        .unwrap();
    assert_eq!(s.now_ms(), 100);
    assert_eq!(s.snapshot().processed_map_clock_ms, 100);
    s.advance(100, Some(t), input(), &mut a).unwrap();
    let before = s.clone();
    let computed = s.spline_position().unwrap();
    assert_ne!(computed, s.pose().position);
    let prior = s.snapshot().spline_id;
    let Event::SplineInterrupted { spline_id, pose } = s.stop_for_death().unwrap() else {
        panic!()
    };
    assert_eq!(spline_id, prior + 1);
    assert_eq!(pose.position, computed);
    assert_eq!(s.pose(), before.pose());
    assert_eq!(s.last_move_ms(), before.last_move_ms());
    assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
}
#[test]
fn visitor_radius_uses_detection_integer_max_and_world_aggro_rate() {
    assert_eq!(visitor_radius(18, Real::ONE).unwrap(), r(45.));
    assert_eq!(visitor_radius(60, r(0.5)).unwrap(), r(30.));
    assert_eq!(visitor_radius(44, r(0.)).unwrap(), Real::ZERO);
}
#[test]
fn source_death_phase_distinguishes_pre_creature_death_from_inside_unit_death() {
    use benilla_sim::creature_ai as ai;
    for (entered_alive, expected) in [(false, ai::Phase::Despawned), (true, ai::Phase::Corpse)] {
        let mut profile = source_ai_profile();
        profile.corpse_decay_ms = 0;
        let mut creature = ai::SoloCreatureAi::new(profile).unwrap();
        let mut observation = source_observation(1, &state(), None);
        observation.receipts = vec![ai::Receipt {
            id: 1,
            event: ai::CombatEvent::Died,
        }];
        let effects = creature
            .source_combat_event_with_phase(&observation, entered_alive)
            .unwrap();
        assert!(effects.contains(&ai::Effect::Died));
        observation.receipts.clear();
        let token = creature
            .begin_source_receipts_in_phase(&observation, entered_alive)
            .unwrap();
        creature
            .finish_receipts_with_hooks(observation, token, &mut |_| {
                panic!("dead creature AI hook")
            })
            .unwrap();
        assert_eq!(creature.phase(), expected);
        creature.validate_fixed_step(100).unwrap();
        let restored = ai::SoloCreatureAi::restore(
            creature.checkpoint(),
            source_ai_profile_with_zero_corpse(),
            100,
            1,
            1,
        )
        .unwrap();
        restored.validate_fixed_step(100).unwrap();
    }
}
#[test]
fn source_mode_never_polls_sight_in_later_ai_and_faces_before_mainhand() {
    use benilla_sim::creature_ai as ai;
    let mut profile = source_ai_profile();
    profile.reaction = ai::Reaction::Aggressive;
    let mut creature = ai::SoloCreatureAi::new(profile).unwrap();
    let mut o = source_observation(1, &state(), None);
    o.target.as_mut().unwrap().position = p(2., 0., 0.);
    let token = creature.begin_source_receipts(&o, false).unwrap();
    creature
        .finish_receipts_with_hooks(o.clone(), token, &mut |_| panic!("no source visitor"))
        .unwrap();
    assert_eq!(creature.phase(), ai::Phase::Idle);
    o.sequence = 2;
    creature.source_sight_event(&o).unwrap();
    let token = creature.begin_source_receipts(&o, false).unwrap();
    let mut hooks = vec![];
    creature
        .finish_receipts_with_hooks(o, token, &mut |hook| {
            hooks.push(hook);
            Ok(ai::MainhandResult::Unchanged)
        })
        .unwrap();
    assert!(matches!(
        hooks.as_slice(),
        [
            ai::OrderedHook::FaceTarget { .. },
            ai::OrderedHook::Mainhand { .. }
        ]
    ));
}

#[test]
fn empty_combat_origin_binds_source_home_relocation_to_respawn() {
    use benilla_sim::creature_ai as ai;
    let mut profile = source_ai_profile();
    profile.spawn = p(10., 0., 0.);
    // Authored idle relocation leaves the respawn coordinate unchanged.
    let mut saved = serde_json::to_value(
        ai::SoloCreatureAi::new(profile.clone())
            .unwrap()
            .checkpoint(),
    )
    .unwrap();
    saved["position"] = serde_json::to_value(p(0., 0., 0.)).unwrap();
    let mut creature = ai::SoloCreatureAi::restore(
        serde_json::from_value(saved).unwrap(),
        profile.clone(),
        0,
        0,
        1,
    )
    .unwrap();
    let mut motion = state();
    motion
        .initialize_default(DefaultMovement::Wander {
            center: profile.spawn,
            radius: r(4.),
        })
        .unwrap();
    let enemy = source_observation(1, &motion, None).target.unwrap();
    creature.source_help_attack_start(&enemy, 0).unwrap();
    assert_eq!(creature.combat_start_position(), p(0., 0., 0.));
    let mut spatial = Synthetic::default();
    motion
        .start_chase_with_relocation(target(20.), false, input(), &mut spatial)
        .unwrap();
    let command = creature.motion().unwrap().clone();
    for sequence in 1..=3 {
        let target = (sequence != 3).then(|| target(20.));
        let update = motion
            .advance_for_command(&command, sequence, 100, target, input(), &mut spatial)
            .unwrap();
        let mut observation = source_observation(sequence, &motion, Some(update.movement));
        if sequence == 3 {
            observation.target = None;
        }
        creature.step(&mut Observation(observation)).unwrap();
    }
    assert_eq!(creature.phase(), ai::Phase::ReturningHome);
    assert_eq!(creature.motion().unwrap().home, Some(profile.spawn));
    assert_ne!(motion.spline_position().unwrap(), creature.position());
    let before = creature.clone();
    let mut wrong = motion.clone();
    let (_, receipt) = wrong
        .start_home_with_relocation(pose(0., 0., 0.), false, false, input(), &mut spatial)
        .unwrap();
    assert!(creature
        .apply_source_relocation(receipt.unwrap(), Revision([2; 32]))
        .is_err());
    assert_eq!(creature, before);
    let (_, receipt) = motion
        .start_home_with_relocation(pose(10., 0., 0.), false, false, input(), &mut spatial)
        .unwrap();
    creature
        .apply_source_relocation(receipt.unwrap(), Revision([2; 32]))
        .unwrap();
    assert_eq!(creature.position(), motion.pose().position);
    assert_eq!(
        ai::SoloCreatureAi::restore(creature.checkpoint(), profile, 300, 3, 1).unwrap(),
        creature,
    );
}

fn source_ai_profile_with_zero_corpse() -> benilla_sim::creature_ai::Profile {
    let mut p = source_ai_profile();
    p.corpse_decay_ms = 0;
    p
}

#[test]
fn wander_interruption_at_stale_map_pose_mints_exact_ai_relocation() {
    use benilla_sim::creature_ai as ai;
    struct WanderHost {
        spatial: Synthetic,
        draws: std::collections::VecDeque<u32>,
    }
    impl Authority for WanderHost {
        fn wander_draw(&mut self, _: WanderDraw) -> Result<u32, MotionError> {
            self.draws
                .pop_front()
                .ok_or(MotionError::Missing("test draw"))
        }
        fn resolve(&mut self, query: &Query) -> Result<Reply, MotionError> {
            if matches!(query,Query::Route(q) if q.kind==RouteKind::WanderPrepare) {
                return Ok(Reply {
                    request: query.clone(),
                    fact: Fact::WanderPrepared,
                });
            }
            self.spatial.resolve(query)
        }
    }
    let mut creature = ai::SoloCreatureAi::new(source_ai_profile()).unwrap();
    let mut motion = state();
    motion
        .initialize_default(DefaultMovement::Wander {
            center: p(0., 0., 0.),
            radius: r(4.),
        })
        .unwrap();
    let mut spatial = WanderHost {
        spatial: Synthetic::default(),
        draws: std::collections::VecDeque::from([0, 0x8000_0000, 3, 3000]),
    };
    for sequence in 1..=3 {
        let update = motion
            .advance_default(sequence, 100, Some(target(20.)), input(), &mut spatial)
            .unwrap();
        creature
            .step(&mut Observation(source_observation(
                sequence,
                &motion,
                Some(update.movement),
            )))
            .unwrap();
    }
    let old = creature.position();
    assert_eq!(motion.pose().position, old);
    assert_ne!(motion.spline_position().unwrap(), old);
    let enemy = source_observation(4, &motion, None).target.unwrap();
    creature
        .source_help_attack_start(&enemy, motion.now_ms())
        .unwrap();
    let before = creature.clone();
    let (events, proof) = motion
        .start_chase_with_relocation(target(20.), false, input(), &mut spatial)
        .unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Relocated {
            notify_map: false,
            ..
        }
    )));
    let proof = proof.unwrap();
    assert_ne!(motion.pose().position, old);
    assert!(creature
        .apply_source_relocation(proof.clone(), Revision([9; 32]))
        .is_err());
    assert_eq!(creature, before);
    let mut wrong_target_profile = source_ai_profile();
    wrong_target_profile.spawn = old;
    let mut wrong_target = ai::SoloCreatureAi::new(wrong_target_profile).unwrap();
    let mut enemy2 = enemy.clone();
    enemy2.id = 99;
    wrong_target
        .source_help_attack_start(&enemy2, motion.now_ms())
        .unwrap();
    let wrong_before = wrong_target.clone();
    assert!(wrong_target
        .apply_source_relocation(proof.clone(), Revision([2; 32]))
        .is_err());
    assert_eq!(wrong_target, wrong_before);
    creature
        .apply_source_relocation(proof.clone(), Revision([2; 32]))
        .unwrap();
    assert_eq!(creature.position(), motion.pose().position);
    let after = creature.clone();
    assert!(creature
        .apply_source_relocation(proof, Revision([2; 32]))
        .is_err());
    assert_eq!(creature, after);
    let command = creature.motion().unwrap().clone();
    let update = motion
        .advance_for_command(&command, 4, 100, Some(target(20.)), input(), &mut spatial)
        .unwrap();
    creature
        .step(&mut Observation(source_observation(
            4,
            &motion,
            Some(update.movement),
        )))
        .unwrap();
    assert_eq!(creature.position(), motion.pose().position);

    // Removing Chase exposes and interrupts the same default Wander a second
    // time before Home Initialize. Idle-default Home retains its separate stale
    // origin/launch correction test above.
    let update = motion
        .advance_for_command(&command, 5, 100, None, input(), &mut spatial)
        .unwrap();
    let mut observation = source_observation(5, &motion, Some(update.movement));
    observation.target = None;
    let combat = creature.clone();
    creature.step(&mut Observation(observation)).unwrap();
    assert_eq!(creature.phase(), ai::Phase::ReturningHome);
    let command = creature.motion().unwrap().clone();
    let destination = Pose {
        position: command.home.unwrap(),
        orientation: Real::ZERO,
    };
    let old = creature.position();
    let computed = motion.spline_position().unwrap();
    assert_ne!(computed, old);
    let before_home = motion.clone();
    let mut wrong_home_motion = before_home.clone();
    let (_, wrong_home) = wrong_home_motion
        .start_home_with_relocation(pose(9., 0., 0.), false, false, input(), &mut spatial)
        .unwrap();
    let before = creature.clone();
    assert!(creature
        .apply_source_relocation(wrong_home.unwrap(), Revision([2; 32]))
        .is_err());
    assert_eq!(creature, before);
    let (events, receipt) = motion
        .start_home_with_relocation(destination, false, false, input(), &mut spatial)
        .unwrap();
    let receipt = receipt.unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::SplineInterrupted { .. })));
    assert_eq!(motion.pose().position, computed);
    let mut wrong_phase = combat;
    let wrong_before = wrong_phase.clone();
    assert!(wrong_phase
        .apply_source_relocation(receipt.clone(), Revision([2; 32]))
        .is_err());
    assert_eq!(wrong_phase, wrong_before);
    assert!(creature
        .apply_source_relocation(receipt.clone(), Revision([9; 32]))
        .is_err());
    assert_eq!(creature, before);
    creature
        .apply_source_relocation(receipt.clone(), Revision([2; 32]))
        .unwrap();
    assert_eq!(creature.position(), computed);
    let after = creature.clone();
    assert!(creature
        .apply_source_relocation(receipt, Revision([2; 32]))
        .is_err());
    assert_eq!(creature, after);
    let query = spatial
        .spatial
        .queries
        .iter()
        .rev()
        .find_map(|q| match q {
            Query::Route(q) => Some(q),
            _ => None,
        })
        .unwrap();
    assert_eq!(query.kind, RouteKind::HomeForceDestination);
    assert_eq!(query.origin, computed);
    assert_eq!(query.launch_origin, computed);
    assert_eq!(query.goal, destination.position);
    assert!(query.prior_cache.is_none());
    assert_eq!(
        query.pathfinder_dispatch.unwrap().constructor_kind(),
        PathfinderConstructorKind::HomeLocal
    );
    assert_eq!(MotionState::restore(motion.snapshot()).unwrap(), motion);
}
