// SPDX-License-Identifier: GPL-3.0-or-later
//! Structural pending-shape and genuine two-certificate discrimination. The
//! concrete corridor harness separately proves actual capture and publication.
use super::*;
use crate::native::{synthetic_fixture as f, synthetic_motion_fixture};
use benilla_native_nav::{
    SourceChaseFixedCacheOperation, SourceChaseFixedCacheRecipe, SourceMapQuery,
    SourceWanderReconstructionOperation, SourceWanderReconstructionRecipe, TileInput,
};
use std::time::{Duration, Instant};

fn pair(
    wander: &SourceWanderReconstructionChallenge,
    chase: &SourceChaseFixedCacheChallenge,
) -> ReconstructedSourceWanderAndChase {
    let slot = SourceMapQuerySlot {
        map_id: 0,
        instance_id: 0,
    };
    let config = SourceSamePolygonConfig {
        normalize_z: false,
        ignore_normalization: false,
    };
    let point = |x: f32, y: f32, z: f32| [x, y, z].map(f32::to_bits);
    SourceMapQuery::reconstruct_wander_and_chase(
        slot,
        include_bytes!(
            "../../../benilla-native-nav/tools/fixtures/direct-corridor/params.test-fixture"
        ),
        &[TileInput {
            grid: [31, 31],
            bytes: include_bytes!(
                "../../../benilla-native-nav/tools/fixtures/warm-cache/precision.test-fixture"
            ),
        }],
        &SourceWanderReconstructionRecipe {
            version: benilla_native_nav::SOURCE_WANDER_RECONSTRUCTION_VERSION,
            compatibility_version: benilla_native_nav::SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION,
            slot,
            query_capacity: 1024,
            config,
            filter: SourceOwnerFilter::GROUND,
            operation_count: 1,
            operations: vec![SourceWanderReconstructionOperation {
                ordinal: 0,
                from: point(5., 5., 0.),
                raw_end: point(6., 5., 0.25),
            }],
        },
        wander,
        &SourceChaseFixedCacheRecipe {
            version: benilla_native_nav::SOURCE_CHASE_FIXED_CACHE_VERSION,
            compatibility_version: benilla_native_nav::SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
            slot,
            query_capacity: 1024,
            point_limit: 74,
            config,
            filter: SourceOwnerFilter::GROUND,
            operation_count: 1,
            operations: vec![SourceChaseFixedCacheOperation {
                ordinal: 0,
                from: point(5., 5., 0.),
                to: point(5., 6., 0.),
            }],
        },
        chase,
        Instant::now() + Duration::from_secs(20),
    )
    .unwrap()
}
#[test]
fn dual_pair_requires_independent_attempts_original_revisions_and_complete_semantics() {
    if !benilla_native_nav::SUPPORTED_ABI {
        eprintln!("skipped: dual native certificate requires supported Detour ABI");
        return;
    }
    let wander = SourceWanderReconstructionChallenge::new();
    let chase = SourceChaseFixedCacheChallenge::new();
    let pair = pair(&wander, &chase);
    let ws = pair.wander().terminal().semantic_state();
    let cs = pair.chase().terminal().chase_semantic_state();
    assert_eq!(
        validate_pair(&pair, &wander, &chase, 1, &ws, 1, &cs),
        Ok(())
    );
    assert!(validate_pair(
        &pair,
        &SourceWanderReconstructionChallenge::new(),
        &chase,
        1,
        &ws,
        1,
        &cs
    )
    .is_err());
    assert!(validate_pair(
        &pair,
        &wander,
        &SourceChaseFixedCacheChallenge::new(),
        1,
        &ws,
        1,
        &cs
    )
    .is_err());
    for revision in [0, 2, u64::MAX] {
        assert!(validate_pair(&pair, &wander, &chase, revision, &ws, 1, &cs).is_err());
        assert!(validate_pair(&pair, &wander, &chase, 1, &ws, revision, &cs).is_err());
    }
    for mutation in 0..22 {
        let mut wrong = cs.clone();
        match mutation {
            0 => wrong.query_policy = "wrong",
            1 => wrong.slot.map_id += 1,
            2 => wrong.slot.instance_id += 1,
            3 => wrong.query_capacity = 512,
            4 => wrong.compatibility_id = "wrong",
            5 => wrong.compatibility_version += 1,
            6 => wrong.source_revision = "wrong",
            7 => wrong.config.normalize_z = true,
            8 => wrong.config.ignore_normalization = true,
            9 => wrong.filter.include_flags = 13,
            10 => wrong.filter.exclude_flags = 1,
            11 => wrong.point_limit -= 1,
            12 => wrong.initial_storage[73] = 1,
            13 => wrong.initial_active_len = 1,
            14 => wrong.retained = !wrong.retained,
            15 => wrong.from[0] ^= 1,
            16 => wrong.to[2] ^= 1,
            17 => wrong.actual_end[2] ^= 1,
            18 => wrong.storage[73] = 1,
            19 => wrong.active_len = 0,
            20 => wrong.path_type = 8,
            21 => wrong.points[0][0] ^= 1,
            _ => unreachable!(),
        }
        assert!(
            validate_pair(&pair, &wander, &chase, 1, &ws, 1, &wrong).is_err(),
            "Chase semantic field {mutation}"
        );
    }
    let mut wrong = ws.clone();
    wrong.storage[73] = 1;
    assert!(validate_pair(&pair, &wander, &chase, 1, &wrong, 1, &cs).is_err());
    let mut wrong = ws;
    wrong.points[0][0] ^= 1;
    assert!(validate_pair(&pair, &wander, &chase, 1, &wrong, 1, &cs).is_err());
}

// Authored shape comparison only: no replay recipe or refs become native authority.
fn snapshot() -> motion::MotionSnapshot {
    let profile = synthetic_motion_fixture::profile();
    let world = NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap();
    let mut snapshot = world.state.npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .snapshot();
    let binding = |query_id, now_ms| motion::QueryBinding {
        identity: snapshot.identity,
        query_id,
        now_ms,
    };
    let wc = binding(1, 0);
    let cc = binding(4, 100);
    let dispatch =
        |kind, first_use, construction: motion::QueryBinding, binding: motion::QueryBinding| {
            serde_json::from_value(serde_json::json!({
                "kind": kind, "first_use": first_use, "construction": construction,
                "dispatch_query_id": binding.query_id, "dispatch_now_ms": binding.now_ms,
            }))
            .unwrap()
        };
    let sample = motion::WanderRequest {
        prepared: binding(2, 50),
        center: f::point(0.),
        radius: f::real(2.),
        angle_draw: f::real(0.),
        radius_draw: f::real(0.5),
    };
    let wr = motion::RouteRequest {
        binding: binding(3, 50),
        origin: sample.center,
        launch_origin: sample.center,
        goal: sample.endpoint().unwrap(),
        kind: motion::RouteKind::Wander,
        wander: Some(Box::new(sample)),
        target: None,
        owner_collision_height: f::real(2.),
        point_limit: 74,
        prior_cache: None,
        pathfinder_dispatch: Some(dispatch("WanderInitialized", false, wc, binding(3, 50))),
    };
    let cr = motion::RouteRequest {
        binding: cc,
        origin: wr.goal,
        launch_origin: wr.goal,
        goal: f::point(2.),
        kind: motion::RouteKind::ChaseInitial,
        wander: None,
        target: Some(motion::Target {
            movement_flags: motion::GroundMovementFlags::STOPPED,
            id: SOLO_UNIT,
            pose: motion::Pose {
                position: f::point(3.),
                orientation: f::real(0.),
            },
            combat_reach: f::real(1.5),
            bounding_radius: f::real(0.5),
            collision_height: f::real(2.),
        }),
        owner_collision_height: f::real(2.),
        point_limit: 74,
        prior_cache: None,
        pathfinder_dispatch: Some(dispatch("ChaseLazy", true, cc, cc)),
    };
    let cache = |end| {
        let mut refs = vec![0; 74];
        refs[0] = 19;
        refs.extend([19, 19]);
        motion::PathFinderCache {
            binding: motion::PathFinderCacheBinding {
                identity: snapshot.identity,
                owner_generation: 1,
                inventory: [1; 32],
                source_epoch: [2; 32],
                navigation_policy: snapshot.identity.geometry.0,
            },
            revision: 1,
            source: motion::RouteSource::OwnerPathFinder,
            poly_refs: refs,
            active_len: 1,
            point_limit: 74,
            path_type: 1,
            requested_end: end,
            include_flags: 1,
            exclude_flags: 0,
        }
    };
    let route = |request: &motion::RouteRequest| motion::CompleteRoute {
        source: motion::RouteSource::OwnerPathFinder,
        path_identity: [7; 32],
        points: vec![request.origin, request.goal],
        segments: vec![ai::movement::SegmentChecks {
            loaded_coverage: ai::movement::Check::Passed,
            line_of_sight: ai::movement::Check::Passed,
            body_sweep: ai::movement::Check::Passed,
            continuous_ground_support: ai::movement::Check::Passed,
            dry_ground: ai::movement::Check::Passed,
        }],
    };
    let wr_route = route(&wr);
    let cr_route = route(&cr);
    let wander_cache = cache(wr.goal);
    let chase_cache = cache(cr.goal);
    let spline =
        motion::LinearSpline::launch(cr_route.points.clone(), cr.origin, f::real(1.25)).unwrap();
    snapshot.generator = motion::Generator::Chase(motion::ChaseSnapshot {
        target: SOLO_UNIT,
        walking: false,
        recheck_ms: 100,
        reached: false,
        reachable: true,
        speed_changed: false,
        last_target: cr.target.unwrap().pose.position,
        closeness_ms: 0,
        closeness_expired: false,
        mode: motion::ChaseMode::Normal,
        requested_goal: Some(cr.goal),
        pathfinder_cache: Some(Box::new(chase_cache)),
        pathfinder_construction: Some(cc),
    });
    snapshot.suspended_wander = Some(motion::WanderSnapshot {
        center: sample.center,
        radius: sample.radius,
        next_move_timer_ms: 100,
        next_move_count: 1,
        pathfinder_cache: Some(Box::new(wander_cache)),
        pathfinder_construction: wc,
        failed_route: None,
        normal_route: Some(Box::new(motion::WanderNormalRouteHead {
            request: wr,
            route: wr_route,
        })),
    });
    snapshot.spline = Some(motion::ActiveSplineSnapshot {
        route_request: cr,
        route: cr_route,
        local_home_cache: None,
        spline_id: 1,
        path_identity: [7; 32],
        route_source: motion::RouteSource::OwnerPathFinder,
        spline: spline.snapshot(),
        facing: motion::Facing::Target(SOLO_UNIT),
        walking: false,
    });
    snapshot.now_ms = 100;
    snapshot.processed_map_clock_ms = 100;
    snapshot.query_id = 4;
    snapshot.spline_id = 1;
    snapshot.map_pose.position = snapshot.spline.as_ref().unwrap().route_request.origin;
    snapshot
}
fn heads(snapshot: &motion::MotionSnapshot) -> Result<(), NativeError> {
    let motion::Generator::Chase(chase) = &snapshot.generator else {
        return Err(unsupported());
    };
    let wander = snapshot.suspended_wander.as_ref().ok_or_else(unsupported)?;
    current_chase_head(
        snapshot,
        chase,
        chase.pathfinder_cache.as_deref().ok_or_else(unsupported)?,
    )?;
    suspended_wander_head(
        snapshot,
        wander,
        wander.pathfinder_cache.as_deref().ok_or_else(unsupported)?,
    )?;
    Ok(())
}
#[test]
fn dual_heads_use_separate_current_calculations_and_preserve_duplicate_tail_refs() {
    let original = snapshot();
    assert_eq!(heads(&original), Ok(()));
    let wander = original.suspended_wander.as_ref().unwrap();
    assert_ne!(
        wander.normal_route.as_ref().unwrap().request,
        original.spline.as_ref().unwrap().route_request
    );
    assert_eq!(
        &wander.pathfinder_cache.as_ref().unwrap().poly_refs[74..],
        &[19, 19]
    );
    assert_eq!(snapshot(), original);
}
#[test]
fn dual_chase_rejects_newer_unlaunched_cache_and_nonordinary_spline() {
    for mutation in 0..8 {
        let mut wrong = snapshot();
        let motion::Generator::Chase(chase) = &mut wrong.generator else {
            unreachable!();
        };
        match mutation {
            0 => {
                chase.pathfinder_cache.as_mut().unwrap().revision += 1;
                wrong.query_id += 1;
            }
            1 => chase.mode = motion::ChaseMode::Backpedal,
            2 => chase.pathfinder_construction.as_mut().unwrap().query_id -= 1,
            3 => wrong.spline.as_mut().unwrap().route_request.kind = motion::RouteKind::Backpedal,
            4 => wrong.spline.as_mut().unwrap().route_request.goal = f::point(4.),
            5 => chase.pathfinder_cache.as_mut().unwrap().requested_end = f::point(4.),
            6 => chase.pathfinder_cache.as_mut().unwrap().path_type = 8,
            7 => chase.target += 1,
            _ => unreachable!(),
        }
        assert!(heads(&wrong).is_err(), "Chase mutation {mutation}");
    }
}
#[test]
fn dual_suspended_wander_rejects_missing_malformed_or_relabelled_normal_head() {
    for mutation in 0..9 {
        let mut wrong = snapshot();
        let wander = wrong.suspended_wander.as_mut().unwrap();
        match mutation {
            0 => wander.normal_route = None,
            1 => wander.pathfinder_construction.query_id += 1,
            2 => wander.pathfinder_cache.as_mut().unwrap().revision += 1,
            3 => {
                wander.normal_route.as_mut().unwrap().request.kind = motion::RouteKind::ChaseReplan
            }
            4 => wander.normal_route.as_mut().unwrap().request.goal = f::point(4.),
            5 => wander.normal_route.as_mut().unwrap().route.points[1] = f::point(4.),
            6 => {
                wander
                    .normal_route
                    .as_mut()
                    .unwrap()
                    .request
                    .pathfinder_dispatch = None
            }
            7 => {
                wander
                    .normal_route
                    .as_mut()
                    .unwrap()
                    .request
                    .wander
                    .as_mut()
                    .unwrap()
                    .radius = f::real(3.)
            }
            8 => wander.pathfinder_cache.as_mut().unwrap().path_type = 8,
            _ => unreachable!(),
        }
        assert!(
            heads(&wrong).is_err(),
            "suspended Wander mutation {mutation}"
        );
    }
}
#[test]
fn dual_pending_does_not_promote_cache_free_or_generic_saved_restore() {
    let profile = synthetic_motion_fixture::profile();
    let world =
        NativeWorld::synthetic_component_test(profile.clone(), [8; 16], f::player(), 1234).unwrap();
    let checkpoint = world.checkpoint().unwrap();
    assert!(NativeWorld::prepare_synthetic_wander_and_chase_reopen(
        profile.clone(),
        checkpoint.clone()
    )
    .is_err());
    assert_eq!(
        NativeWorld::restore_synthetic_component_test(profile, checkpoint.clone())
            .unwrap()
            .checkpoint()
            .unwrap(),
        checkpoint
    );
    let native = NpcMotionSnapshot {
        motion: snapshot(),
        active: true,
        combat_start: None,
    };
    assert!(native.validate_saved_session_reopen().is_err());
}
