// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored SyntheticFixture lifecycle checks, never a source cache initializer.
use super::*;
use crate::creature_ai::movement::{Check, Revision, SegmentChecks};
fn point(x: f32) -> Point {
    Point::new(x, 0., 0.).unwrap()
}
fn pose(x: f32) -> Pose {
    Pose {
        position: point(x),
        orientation: Real::ZERO,
    }
}
fn target(x: f32) -> Target {
    Target {
        movement_flags: GroundMovementFlags::STOPPED,
        id: 1,
        pose: pose(x),
        combat_reach: Real::new(1.5).unwrap(),
        bounding_radius: Real::new(0.5).unwrap(),
        collision_height: Real::integer(2),
    }
}
fn input() -> UnitInputs {
    UnitInputs {
        health: 100,
        maximum_health: 100,
        walk_speed: Real::new(2.5).unwrap(),
        run_speed: Real::integer(7),
        settings: GroundSettings {
            combat_reach: Real::new(1.5).unwrap(),
            bounding_radius: Real::new(0.5).unwrap(),
            collision_height: Real::integer(2),
            no_wounded_slowdown: false,
            backpedal_enabled: true,
            fanning_enabled: false,
        },
        audit: MotionAudit {
            flight_swimming_transport: Evidence::Absent,
            immobilization: Evidence::Absent,
            non_melee_casting: Evidence::Absent,
            speed_auras: Evidence::Absent,
            combat_scripts: Evidence::Absent,
            target_airborne_or_transport: Evidence::Absent,
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
        pose(0.),
        0,
        0,
    )
    .unwrap()
}
fn seed(q: &RouteRequest) -> PathFinderCache {
    let mut refs = vec![0; usize::from(PATHFINDER_CACHE_POINT_LIMIT)];
    refs[0] = 10;
    refs[1] = 20;
    refs.extend([90, 90, 91]);
    PathFinderCache {
        binding: PathFinderCacheBinding {
            identity: q.binding.identity,
            owner_generation: 7,
            inventory: [3; 32],
            source_epoch: [4; 32],
            navigation_policy: [5; 32],
        },
        revision: 1,
        source: RouteSource::SyntheticFixture,
        poly_refs: refs,
        active_len: 2,
        point_limit: PATHFINDER_CACHE_POINT_LIMIT,
        path_type: 1,
        requested_end: q.goal,
        include_flags: 1,
        exclude_flags: 0,
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
struct SyntheticCache {
    queries: Vec<Query>,
    no_path: bool,
    near: Option<Point>,
    failure: u8,
    fail_bearing: bool,
    legacy: bool,
}
impl Authority for SyntheticCache {
    fn resolve(&mut self, query: &Query) -> Result<Reply, MotionError> {
        self.queries.push(query.clone());
        let fact = match query {
            Query::NearPoint(q) => Fact::NearPoint(self.near.unwrap_or(Point {
                x: q.target.pose.position.x.sub(q.distance)?,
                ..q.target.pose.position
            })),
            Query::Route(q) => {
                let mut after = q.prior_cache.as_deref().cloned().unwrap_or_else(|| seed(q));
                if q.prior_cache.is_some() {
                    after.revision += 1;
                    after.poly_refs.push(100 + after.revision);
                }
                after.requested_end = q.goal;
                after.path_type = if self.no_path { 8 } else { 1 };
                after.active_len = if self.no_path { 0 } else { 2 };
                let outcome = if self.no_path {
                    RouteOutcome::NoPath
                } else {
                    RouteOutcome::Complete(CompleteRoute {
                        source: RouteSource::SyntheticFixture,
                        path_identity: [8; 32],
                        points: vec![q.origin, q.goal],
                        segments: vec![checks()],
                    })
                };
                if self.legacy {
                    Fact::Route(outcome)
                } else {
                    let mut calculation = RouteCalculation {
                        source: RouteSource::SyntheticFixture,
                        outcome,
                        prior_cache: q.prior_cache.clone(),
                        after_cache: after,
                    };
                    match self.failure {
                        1 => calculation.prior_cache = None,
                        2 => calculation.after_cache.revision += 1,
                        3 => calculation.after_cache.binding.inventory[0] ^= 1,
                        4 => calculation.after_cache.binding.identity.geometry.0[0] ^= 1,
                        5 => calculation.after_cache.requested_end = point(999.),
                        6 => calculation.after_cache.include_flags = 3,
                        7 => calculation.after_cache.point_limit = 73,
                        8 => calculation.after_cache.poly_refs.truncate(74),
                        9 => calculation.after_cache.path_type = 8,
                        10 => calculation.after_cache.source = RouteSource::OwnerPathFinder,
                        11 => {
                            if let RouteOutcome::Complete(path) = &mut calculation.outcome {
                                path.segments[0].body_sweep = Check::Unknown;
                            }
                        }
                        12 => calculation
                            .after_cache
                            .poly_refs
                            .resize(MAX_PATHFINDER_CACHE_REFS + 1, 0),
                        13 => {
                            calculation.source = RouteSource::OwnerPathFinder;
                            calculation.after_cache.source = RouteSource::OwnerPathFinder;
                            if let RouteOutcome::Complete(path) = &mut calculation.outcome {
                                path.source = RouteSource::OwnerPathFinder;
                            }
                        }
                        15 => calculation.outcome = RouteOutcome::Missing,
                        16 => calculation.outcome = RouteOutcome::Incomplete,
                        17 => calculation.outcome = RouteOutcome::Truncated,
                        18 => calculation.outcome = RouteOutcome::Shortcut,
                        19 => {
                            *calculation
                                .prior_cache
                                .as_mut()
                                .unwrap()
                                .poly_refs
                                .last_mut()
                                .unwrap() ^= 1
                        }
                        _ => {}
                    }
                    Fact::RouteCalculation(Box::new(calculation))
                }
            }
            Query::Bearing(_) if self.fail_bearing => {
                return Err(MotionError::Missing("synthetic later bearing"));
            }
            Query::Bearing(_) => Fact::Bearing(Real::ZERO),
            Query::FacingTarget(_) => Fact::ObjectPosition(None),
            Query::Collider(_) => Fact::Collider(None),
        };
        let mut request = query.clone();
        if let Query::Route(q) = &mut request {
            if self.failure == 14 {
                q.binding.query_id += 1;
            }
            if self.failure == 20 {
                *q.prior_cache
                    .as_mut()
                    .unwrap()
                    .poly_refs
                    .last_mut()
                    .unwrap() ^= 1;
            }
        }
        Ok(Reply { request, fact })
    }
}
fn chase(s: &MotionState) -> ChaseSnapshot {
    let Generator::Chase(c) = s.generator() else {
        panic!("expected Chase")
    };
    c.clone()
}
fn last_route(a: &SyntheticCache) -> &RouteRequest {
    a.queries
        .iter()
        .rev()
        .find_map(|q| {
            if let Query::Route(q) = q {
                Some(q)
            } else {
                None
            }
        })
        .unwrap()
}
#[test]
fn synthetic_fixture_detached_chase_retains_complete_and_no_path_calculations() {
    let mut s = state();
    let mut a = SyntheticCache::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    let first = chase(&s).pathfinder_cache.unwrap();
    assert!(last_route(&a).prior_cache.is_none());
    assert_eq!(first.revision, 1);
    assert_eq!(&first.poly_refs[74..], &[90, 90, 91]);
    s.advance(250, Some(target(40.)), input(), &mut a).unwrap();
    assert_eq!(last_route(&a).prior_cache.as_ref(), Some(&first));
    let second = chase(&s).pathfinder_cache.unwrap();
    assert_eq!(second.revision, 2);
    a.no_path = true;
    s.advance(250, Some(target(60.)), input(), &mut a).unwrap();
    assert_eq!(last_route(&a).prior_cache.as_ref(), Some(&second));
    let third = chase(&s).pathfinder_cache.unwrap();
    assert_eq!(third.revision, 3);
    assert_eq!(third.active_len, 0);
    assert_eq!(
        &third.poly_refs[..second.poly_refs.len()],
        second.poly_refs.as_slice()
    );
    assert_eq!(chase(&s).requested_goal, Some(last_route(&a).goal));
    assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
}
#[test]
fn synthetic_fixture_initial_no_path_and_unreachable_replan_still_stage_cache() {
    let mut s = state();
    let mut a = SyntheticCache {
        no_path: true,
        ..Default::default()
    };
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    assert!(!s.is_moving());
    assert_eq!(chase(&s).pathfinder_cache.unwrap().path_type, 8);
    assert_eq!(chase(&s).requested_goal, Some(last_route(&a).goal));
    let mut s = state();
    a.no_path = false;
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    let first = chase(&s).pathfinder_cache.unwrap();
    a.near = Some(point(100.));
    let events = s.advance(250, Some(target(40.)), input(), &mut a).unwrap();
    assert!(events.contains(&Event::ChaseUnreachable));
    let after = chase(&s).pathfinder_cache.unwrap();
    assert_eq!(last_route(&a).prior_cache.as_ref(), Some(&first));
    assert_eq!(after.revision, 2);
    assert_eq!(after.requested_end, point(100.));
    assert_eq!(chase(&s).requested_goal, Some(point(100.)));
}
#[test]
fn synthetic_fixture_speed_only_replan_uses_last_failed_requested_endpoint() {
    let mut s = state();
    let mut a = SyntheticCache::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    a.no_path = true;
    s.advance(250, Some(target(40.)), input(), &mut a).unwrap();
    let failed_goal = last_route(&a).goal;
    s.speed_changed();
    a.near = Some(point(999.));
    s.advance(250, Some(target(20.)), input(), &mut a).unwrap();
    assert_eq!(last_route(&a).goal, failed_goal);
}
#[test]
fn synthetic_fixture_retarget_retains_cache_home_and_death_drop_it() {
    let mut s = state();
    let mut a = SyntheticCache::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    let cache = chase(&s).pathfinder_cache;
    let calls = a.queries.len();
    let mut other = target(30.);
    other.id = 2;
    s.start_chase(other, false, input(), &mut a).unwrap();
    assert_eq!(a.queries.len(), calls);
    assert_eq!(chase(&s).pathfinder_cache, cache);
    s.start_home(pose(-10.), false, false, input(), &mut a)
        .unwrap();
    assert_eq!(last_route(&a).kind, RouteKind::HomeForceDestination);
    assert!(last_route(&a).prior_cache.is_none());
    assert!(matches!(s.generator(), Generator::Home { .. }));
    s.start_chase(other, false, input(), &mut a).unwrap();
    assert!(last_route(&a).prior_cache.is_none());
    assert_eq!(chase(&s).pathfinder_cache.unwrap().revision, 1);
    s.stop_for_death().unwrap();
    assert!(matches!(s.generator(), Generator::Idle));
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    assert!(last_route(&a).prior_cache.is_none());
}
#[test]
fn synthetic_fixture_bad_prior_delta_evidence_or_reply_rolls_back_everything() {
    for failure in 1..=20 {
        let mut s = state();
        let mut a = SyntheticCache::default();
        s.start_chase(target(20.), false, input(), &mut a).unwrap();
        let before = s.clone();
        a.failure = failure;
        assert!(
            s.advance(250, Some(target(40.)), input(), &mut a).is_err(),
            "failure {failure}"
        );
        assert_eq!(s, before, "failure {failure}");
    }
}
#[test]
fn synthetic_fixture_later_bearing_failure_rolls_back_valid_no_path_proposal() {
    let mut s = state();
    let mut a = SyntheticCache {
        no_path: true,
        ..Default::default()
    };
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    let before = s.clone();
    a.fail_bearing = true;
    assert!(s.advance(250, Some(target(40.)), input(), &mut a).is_err());
    assert!(a
        .queries
        .iter()
        .any(|q| matches!(q, Query::Route(r) if r.prior_cache.is_some())));
    assert_eq!(s, before);
}
#[test]
fn synthetic_fixture_seed_does_not_establish_initial_owner_source_producer() {
    let mut s = state();
    let before = s.clone();
    let mut a = SyntheticCache {
        failure: 13,
        ..Default::default()
    };
    assert_eq!(
        s.start_chase(target(20.), false, input(), &mut a),
        Err(MotionError::Missing(
            "initial source pathfinder cache producer"
        ))
    );
    assert_eq!(s, before);
}
#[test]
fn legacy_synthetic_fixture_is_cache_free_and_cannot_erase_retained_state() {
    let mut s = state();
    let mut a = SyntheticCache {
        legacy: true,
        ..Default::default()
    };
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    assert!(chase(&s).pathfinder_cache.is_none());
    a.legacy = false;
    s.advance(250, Some(target(40.)), input(), &mut a).unwrap();
    let before = s.clone();
    a.legacy = true;
    assert!(s.advance(250, Some(target(60.)), input(), &mut a).is_err());
    assert_eq!(s, before);
}

#[test]
fn synthetic_fixture_backpedal_retains_same_generator_cache() {
    let mut s = state();
    let mut a = SyntheticCache::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    let cache = chase(&s).pathfinder_cache.unwrap();
    // Author an already-arrived same-Chase state; elapsed closeness triggers backpedal.
    let mut snapshot = s.snapshot();
    snapshot.spline = None;
    let Generator::Chase(c) = &mut snapshot.generator else {
        panic!("chase")
    };
    c.reached = true;
    c.closeness_ms = 1;
    s = MotionState::restore(snapshot).unwrap();
    let events = s.advance(1, Some(target(0.)), input(), &mut a).unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::SplineLaunched { walking: true, .. })));
    assert_eq!(last_route(&a).kind, RouteKind::Backpedal);
    assert_eq!(last_route(&a).prior_cache.as_ref(), Some(&cache));
    assert_eq!(chase(&s).pathfinder_cache.unwrap().revision, 2);
}
#[test]
fn synthetic_fixture_interruption_retains_cache_without_a_calculation() {
    let mut s = state();
    let mut a = SyntheticCache::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    let cache = chase(&s).pathfinder_cache.unwrap();
    // Arrive within reach of both the final endpoint and a target that moved closer.
    s.advance(1900, None, input(), &mut a).unwrap();
    let routes = a
        .queries
        .iter()
        .filter(|q| matches!(q, Query::Route(_)))
        .count();
    let events = s.advance(1, Some(target(15.)), input(), &mut a).unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::SplineInterrupted { .. })));
    assert_eq!(chase(&s).pathfinder_cache.as_ref(), Some(&cache));
    assert_eq!(
        a.queries
            .iter()
            .filter(|q| matches!(q, Query::Route(_)))
            .count(),
        routes
    );
}
#[test]
fn synthetic_fixture_restore_rejects_cache_identity_revision_and_endpoint_mismatch() {
    let mut s = state();
    let mut a = SyntheticCache::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    s.advance(250, Some(target(40.)), input(), &mut a).unwrap();
    for case in 0..11 {
        let mut snapshot = s.snapshot();
        let Generator::Chase(c) = &mut snapshot.generator else {
            panic!("chase")
        };
        let cache = c.pathfinder_cache.as_mut().unwrap();
        match case {
            0 => cache.binding.identity.creature += 1,
            1 => cache.revision = u64::MAX,
            2 => cache.requested_end = point(999.),
            3 => cache.binding.source_epoch[0] ^= 1,
            4 => cache.revision = 1,
            5 => cache.poly_refs.truncate(74),
            6 => {
                snapshot
                    .spline
                    .as_mut()
                    .unwrap()
                    .route_request
                    .prior_cache
                    .as_mut()
                    .unwrap()
                    .source = RouteSource::OwnerPathFinder
            }
            7 => cache.point_limit = 73,
            8 => c.pathfinder_cache = None,
            9 => cache.path_type = 8,
            10 => {
                cache.requested_end = point(999.);
                c.requested_goal = Some(point(999.));
            }
            _ => unreachable!(),
        }
        assert!(MotionState::restore(snapshot).is_err(), "case {case}");
    }
}

#[test]
fn synthetic_fixture_failed_home_does_not_discard_existing_chase_cache() {
    let mut s = state();
    let mut a = SyntheticCache::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    let before = s.clone();
    a.no_path = true;
    assert!(s
        .start_home(pose(-10.), false, false, input(), &mut a)
        .is_err());
    assert!(last_route(&a).prior_cache.is_none());
    assert_eq!(s, before);
}
#[test]
fn synthetic_fixture_revision_overflow_is_not_a_cache_reset() {
    let mut s = state();
    let mut a = SyntheticCache::default();
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    let q = last_route(&a);
    let mut request = q.clone();
    let mut before = seed(q);
    before.revision = u64::MAX;
    request.prior_cache = Some(Box::new(before.clone()));
    let mut after = before.clone();
    after.revision = 1;
    let proposal = RouteCalculation {
        source: RouteSource::SyntheticFixture,
        outcome: RouteOutcome::NoPath,
        prior_cache: Some(Box::new(before)),
        after_cache: PathFinderCache {
            path_type: 8,
            ..after
        },
    };
    assert!(proposal.validate_for(&request).is_err());
}

#[test]
fn legacy_bounded_detour_stays_cache_free_but_owner_pathfinder_requires_state() {
    struct LegacySource(RouteSource, SyntheticCache);
    impl Authority for LegacySource {
        fn resolve(&mut self, query: &Query) -> Result<Reply, MotionError> {
            let mut reply = self.1.resolve(query)?;
            if let Fact::Route(RouteOutcome::Complete(path)) = &mut reply.fact {
                path.source = self.0;
            }
            Ok(reply)
        }
    }
    let mut s = state();
    let mut a = LegacySource(
        RouteSource::BoundedDryDetour,
        SyntheticCache {
            legacy: true,
            ..Default::default()
        },
    );
    s.start_chase(target(20.), false, input(), &mut a).unwrap();
    assert!(chase(&s).pathfinder_cache.is_none());
    assert_eq!(MotionState::restore(s.snapshot()).unwrap(), s);
    let before = s.clone();
    a.0 = RouteSource::OwnerPathFinder;
    assert!(s.advance(250, Some(target(40.)), input(), &mut a).is_err());
    assert_eq!(s, before);
    let mut forged = s.snapshot();
    let spline = forged.spline.as_mut().unwrap();
    spline.route.source = RouteSource::OwnerPathFinder;
    spline.route_source = RouteSource::OwnerPathFinder;
    assert!(MotionState::restore(forged).is_err());
}
