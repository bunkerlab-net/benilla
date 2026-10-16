// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic typed-claim/malicious fixtures for the admission contract only.
//! These authored claims execute no native PathFinder, terrain, or body queries.
use super::*;

#[derive(Default)]
struct SyntheticOwnerClaim(SyntheticCache);
impl Authority for SyntheticOwnerClaim {
    fn resolve(&mut self, query: &Query) -> Result<Reply, MotionError> {
        let mut reply = self.0.resolve(query)?;
        if let Fact::RouteCalculation(c) = &mut reply.fact {
            c.source = RouteSource::OwnerPathFinder;
            c.after_cache.source = RouteSource::OwnerPathFinder;
            if let RouteOutcome::Complete(path) = &mut c.outcome {
                path.source = RouteSource::OwnerPathFinder;
            }
        }
        Ok(reply)
    }
}
fn first_claim() -> (MotionState, RouteRequest, RouteCalculation) {
    let mut state = state();
    let mut authority = SyntheticOwnerClaim::default();
    state
        .start_chase(target(20.), false, input(), &mut authority)
        .unwrap();
    let request = last_route(&authority.0).clone();
    let calculation = RouteCalculation {
        source: RouteSource::OwnerPathFinder,
        outcome: RouteOutcome::Complete(state.snapshot().spline.unwrap().route),
        prior_cache: None,
        after_cache: *chase(&state).pathfinder_cache.unwrap(),
    };
    (state, request, calculation)
}

#[test]
fn synthetic_typed_claim_first_then_retained_restore_is_structurally_coherent() {
    let (mut state, first, calculation) = first_claim();
    assert!(first.pathfinder_dispatch.unwrap().first_construction());
    assert_eq!(calculation.after_cache.revision, 1);
    calculation.validate_for(&first).unwrap();
    let serialized = serde_json::to_vec(&state.snapshot()).unwrap();
    let restored = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(MotionState::restore(restored).unwrap(), state);
    let mut authority = SyntheticOwnerClaim::default();
    state
        .advance(250, Some(target(40.)), input(), &mut authority)
        .unwrap();
    let retained = last_route(&authority.0);
    assert!(!retained.pathfinder_dispatch.unwrap().first_construction());
    assert_eq!(
        retained.prior_cache.as_deref(),
        Some(&calculation.after_cache)
    );
    assert_eq!(chase(&state).pathfinder_cache.unwrap().revision, 2);
    assert_eq!(MotionState::restore(state.snapshot()).unwrap(), state);
}

#[test]
fn malicious_first_claim_rejects_missing_or_retained_dispatch_and_invalid_terminal() {
    let (_, request, calculation) = first_claim();
    for case in 0..24 {
        let mut q = request.clone();
        let mut c = calculation.clone();
        match case {
            0 => q.pathfinder_dispatch = None,
            1 => {
                q.binding.query_id += 1;
                q.pathfinder_dispatch =
                    Some(PathfinderDispatch::new(Some(request.binding), q.binding).unwrap());
            }
            2 => q.binding.identity.creature += 1,
            3 => q.binding.now_ms += 1,
            4 => c.after_cache.revision = 2,
            5 => c.after_cache.binding.identity.geometry.0[0] ^= 1,
            6 => c.after_cache.binding.inventory = [0; 32],
            7 => c.after_cache.binding.source_epoch = [0; 32],
            8 => c.after_cache.binding.navigation_policy = [0; 32],
            9 => c.after_cache.path_type = 8,
            10 => c.after_cache.active_len = 0,
            11 => c.after_cache.requested_end = point(999.),
            12 => c.after_cache.point_limit = 73,
            13 => c.after_cache.source = RouteSource::SyntheticFixture,
            14 => c.after_cache.poly_refs.truncate(73),
            15 => c.outcome = RouteOutcome::NoPath,
            16 => c.outcome = RouteOutcome::Incomplete,
            17 => c.outcome = RouteOutcome::Truncated,
            18 => c.outcome = RouteOutcome::Shortcut,
            19 => c.outcome = RouteOutcome::Missing,
            20 => {
                q.kind = RouteKind::HomeForceDestination;
                q.pathfinder_dispatch = None;
            }
            21 => {
                let RouteOutcome::Complete(path) = &mut c.outcome else {
                    unreachable!()
                };
                path.source = RouteSource::BoundedDryDetour;
            }
            22 => c.after_cache.include_flags = 3,
            23 => c.after_cache.revision = 0,
            _ => unreachable!(),
        }
        assert!(c.validate_for(&q).is_err(), "case {case}");
    }
}

#[test]
fn malicious_first_claim_cannot_replace_endpoint_identity_or_any_segment_predicate() {
    let (_, q, original) = first_claim();
    for case in 0..6 {
        let mut c = original.clone();
        let RouteOutcome::Complete(path) = &mut c.outcome else {
            unreachable!()
        };
        match case {
            0 => path.points[0] = point(1.),
            1 => path.points[1] = point(1.),
            2 => path.path_identity = [0; 32],
            3 => path.segments.clear(),
            4 => path.points.clear(),
            5 => path.points.resize(75, q.goal),
            _ => unreachable!(),
        }
        assert!(c.validate_for(&q).is_err(), "case {case}");
    }
    for failed in [Check::Unknown, Check::Failed] {
        for index in 0..5 {
            let mut c = original.clone();
            let RouteOutcome::Complete(path) = &mut c.outcome else {
                unreachable!()
            };
            let checks = &mut path.segments[0];
            match index {
                0 => checks.loaded_coverage = failed,
                1 => checks.line_of_sight = failed,
                2 => checks.body_sweep = failed,
                3 => checks.continuous_ground_support = failed,
                4 => checks.dry_ground = failed,
                _ => unreachable!(),
            }
            assert!(c.validate_for(&q).is_err(), "predicate {index}: {failed:?}");
        }
    }
}

#[test]
fn malicious_retained_claim_needs_exact_prior_increment_and_nonfirst_dispatch() {
    let (mut state, first, initial) = first_claim();
    let mut authority = SyntheticOwnerClaim::default();
    state
        .advance(250, Some(target(40.)), input(), &mut authority)
        .unwrap();
    let request = last_route(&authority.0).clone();
    let calculation = RouteCalculation {
        source: RouteSource::OwnerPathFinder,
        outcome: RouteOutcome::Complete(state.snapshot().spline.unwrap().route),
        prior_cache: Some(Box::new(initial.after_cache)),
        after_cache: *chase(&state).pathfinder_cache.unwrap(),
    };
    calculation.validate_for(&request).unwrap();
    for case in 0..8 {
        let mut q = request.clone();
        let mut c = calculation.clone();
        match case {
            0 => q.pathfinder_dispatch = None,
            1 => q.pathfinder_dispatch = Some(PathfinderDispatch::new(None, q.binding).unwrap()),
            2 => c.prior_cache = None,
            3 => c.prior_cache.as_mut().unwrap().poly_refs[74] ^= 1,
            4 => c.after_cache.revision = 1,
            5 => c.after_cache.binding.source_epoch[0] ^= 1,
            6 => c.after_cache.poly_refs.truncate(74),
            7 => {
                q.prior_cache = None;
                c.prior_cache = None;
                c.after_cache.revision = 1;
            }
            _ => unreachable!(),
        }
        assert!(c.validate_for(&q).is_err(), "case {case}");
    }
    // Even a complete prior/after pair cannot turn an actual first event into a retained use.
    let mut first_with_prior = first;
    first_with_prior.prior_cache = calculation.prior_cache.clone();
    assert!(calculation.validate_for(&first_with_prior).is_err());
}

#[test]
fn malicious_first_snapshot_cannot_drop_or_relabel_current_cache_and_constructor() {
    let (state, _, _) = first_claim();
    for case in 0..11 {
        let mut snapshot = state.snapshot();
        let Generator::Chase(chase) = &mut snapshot.generator else {
            unreachable!()
        };
        let cache = chase.pathfinder_cache.as_mut().unwrap();
        match case {
            0 => chase.pathfinder_cache = None,
            1 => cache.source = RouteSource::SyntheticFixture,
            2 => cache.revision = 2,
            3 => cache.path_type = 8,
            4 => cache.active_len = 0,
            5 => {
                cache.requested_end = point(999.);
                chase.requested_goal = Some(point(999.));
            }
            6 => chase.pathfinder_construction = None,
            7 => chase.pathfinder_construction.as_mut().unwrap().query_id += 1,
            8 => {
                snapshot
                    .spline
                    .as_mut()
                    .unwrap()
                    .route_request
                    .pathfinder_dispatch = None
            }
            9 => snapshot.generator = Generator::Idle,
            10 => cache.binding.identity.creature += 1,
            _ => unreachable!(),
        }
        assert!(MotionState::restore(snapshot).is_err(), "case {case}");
    }
}

#[test]
fn synthetic_typed_claim_no_path_keeps_first_spline_but_no_loaded_restore_authority() {
    let (mut state, _, initial) = first_claim();
    let first_spline = state.snapshot().spline.unwrap().route_request;
    let mut authority = SyntheticOwnerClaim(SyntheticCache {
        no_path: true,
        ..Default::default()
    });
    state
        .advance(250, Some(target(40.)), input(), &mut authority)
        .unwrap();
    assert_eq!(state.snapshot().spline.unwrap().route_request, first_spline);
    let cache = chase(&state).pathfinder_cache.unwrap();
    assert_eq!(cache.revision, initial.after_cache.revision + 1);
    assert_eq!(cache.path_type, 8);
    assert_eq!(MotionState::restore(state.snapshot()).unwrap(), state);
}

#[test]
fn synthetic_typed_claim_malformed_first_and_later_retained_failure_roll_back() {
    let mut state = state();
    let before = state.clone();
    let mut authority = SyntheticOwnerClaim(SyntheticCache {
        failure: 11,
        ..Default::default()
    });
    assert!(state
        .start_chase(target(20.), false, input(), &mut authority)
        .is_err());
    assert_eq!(state, before);
    let failed_request = last_route(&authority.0).clone();
    authority.0.failure = 0;
    state
        .start_chase(target(20.), false, input(), &mut authority)
        .unwrap();
    assert_eq!(last_route(&authority.0), &failed_request);
    let before = state.clone();
    authority.0.no_path = true;
    authority.0.fail_bearing = true;
    assert!(state
        .advance(250, Some(target(40.)), input(), &mut authority)
        .is_err());
    assert_eq!(state, before);
}

#[test]
fn synthetic_typed_unreachable_complete_retains_first_spline_and_newer_cache() {
    let (mut state, _, _) = first_claim();
    let first_spline = state.snapshot().spline.unwrap().route_request;
    let mut authority = SyntheticOwnerClaim(SyntheticCache {
        near: Some(point(999.)),
        ..Default::default()
    });
    let events = state
        .advance(250, Some(target(40.)), input(), &mut authority)
        .unwrap();
    assert!(events.contains(&Event::ChaseUnreachable));
    assert_eq!(state.snapshot().spline.unwrap().route_request, first_spline);
    let cache = chase(&state).pathfinder_cache.unwrap();
    assert_eq!(cache.revision, 2);
    assert_eq!(cache.path_type, 1);
    assert_eq!(cache.requested_end, point(999.));
    assert_eq!(MotionState::restore(state.snapshot()).unwrap(), state);
}

#[test]
fn malicious_retained_snapshot_cannot_reset_dispatch_or_change_prior_current_lineage() {
    let (mut state, _, _) = first_claim();
    let mut authority = SyntheticOwnerClaim::default();
    state
        .advance(250, Some(target(40.)), input(), &mut authority)
        .unwrap();
    for case in 0..11 {
        let mut snapshot = state.snapshot();
        let spline = snapshot.spline.as_mut().unwrap();
        let Generator::Chase(chase) = &mut snapshot.generator else {
            unreachable!()
        };
        let current = chase.pathfinder_cache.as_mut().unwrap();
        match case {
            0 => {
                spline.route_request.pathfinder_dispatch =
                    Some(PathfinderDispatch::new(None, spline.route_request.binding).unwrap())
            }
            1 => spline.route_request.prior_cache = None,
            2 => current.binding.inventory[0] ^= 1,
            3 => current.revision = spline.route_request.prior_cache.as_ref().unwrap().revision,
            4 => current.poly_refs.truncate(74),
            5 => current.source = RouteSource::SyntheticFixture,
            6 => {
                spline.route_request.prior_cache.as_mut().unwrap().source =
                    RouteSource::SyntheticFixture
            }
            7 => current.path_type = 8,
            8 => chase.pathfinder_cache = None,
            9 => spline.route_request.prior_cache.as_mut().unwrap().path_type = 8,
            10 => {
                spline
                    .route_request
                    .prior_cache
                    .as_mut()
                    .unwrap()
                    .active_len = 0
            }
            _ => unreachable!(),
        }
        assert!(MotionState::restore(snapshot).is_err(), "case {case}");
    }
}

#[test]
fn synthetic_interrupted_first_source_cache_still_requires_initial_normal_terminal() {
    let (state, _, _) = first_claim();
    let mut interrupted = state.snapshot();
    interrupted.spline = None;
    MotionState::restore(interrupted.clone()).unwrap();
    for empty in [false, true] {
        let mut malformed = interrupted.clone();
        let Generator::Chase(chase) = &mut malformed.generator else {
            unreachable!()
        };
        let cache = chase.pathfinder_cache.as_mut().unwrap();
        if empty {
            cache.active_len = 0;
        } else {
            cache.path_type = 8;
        }
        assert!(MotionState::restore(malformed).is_err());
    }
}
