// SPDX-License-Identifier: GPL-3.0-or-later
//! Native execution/certificate discrimination; full app publication is tested
//! in the concrete corridor harness, which owns loaded composite geometry.
use super::*;
use benilla_native_nav::{
    SourceMapQuery, SourceWanderReconstructionOperation, SourceWanderReconstructionRecipe,
    TileInput, SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION, SOURCE_WANDER_RECONSTRUCTION_VERSION,
};
use std::time::{Duration, Instant};

fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}

fn certificate(
    challenge: &SourceWanderReconstructionChallenge,
    minimum_distance: bool,
) -> ReconstructedSourceWanderTerminal {
    let slot = SourceMapQuerySlot {
        map_id: 0,
        instance_id: 0,
    };
    let query = SourceMapQuery::load(
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
    )
    .unwrap();
    query
        .reconstruct_wander(
            &SourceWanderReconstructionRecipe {
                version: SOURCE_WANDER_RECONSTRUCTION_VERSION,
                compatibility_version: SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION,
                slot,
                query_capacity: 1024,
                config: SourceSamePolygonConfig {
                    normalize_z: false,
                    ignore_normalization: false,
                },
                filter: SourceOwnerFilter::GROUND,
                operation_count: 1,
                operations: vec![SourceWanderReconstructionOperation {
                    ordinal: 0,
                    from: point(5., 5., 0.),
                    raw_end: if minimum_distance {
                        point(5.05, 5., 0.)
                    } else {
                        point(6., 5., 0.25)
                    },
                }],
            },
            challenge,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap()
}

#[test]
fn pending_reopen_native_certificate_checks_attempt_revision_and_every_semantic_field() {
    if !benilla_native_nav::SUPPORTED_ABI {
        eprintln!("skipped: native Wander certificate requires supported Detour ABI");
        return;
    }
    let challenge = SourceWanderReconstructionChallenge::new();
    let certificate = certificate(&challenge, false);
    let expected = certificate.terminal().semantic_state();
    assert_eq!(
        validate_certificate(&certificate, &challenge, 1, &expected),
        Ok(())
    );
    assert_eq!(
        validate_certificate(
            &certificate,
            &SourceWanderReconstructionChallenge::new(),
            1,
            &expected,
        ),
        Err(NativeError::AuthorityBinding),
    );
    for revision in [0, 2, u64::MAX] {
        assert_eq!(
            validate_certificate(&certificate, &challenge, revision, &expected),
            Err(NativeError::AuthorityBinding),
        );
    }
    for changed in 0..16 {
        let mut wrong = expected.clone();
        match changed {
            0 => wrong.slot.map_id += 1,
            1 => wrong.slot.instance_id += 1,
            2 => wrong.query_capacity = 512,
            3 => wrong.config.normalize_z = true,
            4 => wrong.config.ignore_normalization = true,
            5 => wrong.filter.include_flags = 13,
            6 => wrong.filter.exclude_flags = 1,
            7 => wrong.from[0] ^= 1,
            8 => wrong.raw_end[2] ^= 1,
            9 => wrong.actual_end[2] ^= 1,
            10 => wrong.storage[0] ^= 1,
            11 => wrong.storage[73] = 1,
            12 => *wrong.storage.last_mut().unwrap() ^= 1,
            13 => wrong.active_len = 0,
            14 => wrong.path_type = 8,
            15 => wrong.points[0][0] ^= 1,
            _ => unreachable!(),
        }
        assert_eq!(
            validate_certificate(&certificate, &challenge, 1, &wrong),
            Err(NativeError::AuthorityBinding),
            "semantic field mutation {changed}",
        );
    }
    let mut wrong = expected;
    wrong.no_path_reason = Some(benilla_native_nav::SourceWanderNoPathReason::MinimumDistance);
    assert_eq!(
        validate_certificate(&certificate, &challenge, 1, &wrong),
        Err(NativeError::AuthorityBinding),
    );
}

#[test]
fn pending_reopen_cannot_promote_an_ordinary_cache_free_checkpoint() {
    use crate::native::{synthetic_fixture as f, synthetic_motion_fixture};
    let profile = synthetic_motion_fixture::profile();
    let world =
        NativeWorld::synthetic_component_test(profile.clone(), [8; 16], f::player(), 1234).unwrap();
    let checkpoint = world.checkpoint().unwrap();
    assert!(
        NativeWorld::prepare_synthetic_wander_reopen(profile.clone(), checkpoint.clone()).is_err()
    );
    let restored =
        NativeWorld::restore_synthetic_component_test(profile, checkpoint.clone()).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
}

#[test]
fn pending_reopen_draw_pair_is_counter_addressed_exact_and_read_only() {
    use crate::native::{synthetic_fixture as f, synthetic_motion_fixture};
    let profile = synthetic_motion_fixture::profile();
    let world = NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap();
    let mut state = world.state.clone();
    let owner = state.npcs[&2].id;
    let mut draws = Vec::new();
    for purpose in [
        DrawPurpose::WanderAngle { npc: owner },
        DrawPurpose::WanderRadius { npc: owner },
    ] {
        state
            .draw_range(0, purpose, 0, i64::from(u32::MAX), &mut draws)
            .unwrap();
    }
    let draws: [Draw; 2] = draws.try_into().unwrap();
    let request = motion::WanderRequest {
        prepared: motion::QueryBinding {
            identity: state.npcs[&2]
                .source_motion
                .as_ref()
                .unwrap()
                .motion
                .snapshot()
                .identity,
            query_id: 1,
            now_ms: state.now_ms,
        },
        center: state.npcs[&2].ai.position(),
        radius: f::real(3.),
        angle_draw: motion::wander_normalized_word(draws[0].value as u32).unwrap(),
        radius_draw: motion::wander_normalized_word(draws[1].value as u32).unwrap(),
    };
    let before = snapshot_state(world.profile_digest(), &state);
    assert_eq!(validate_draw_pair(&state, owner, &draws, &request), Ok(()));
    assert_eq!(snapshot_state(world.profile_digest(), &state), before);
    for changed in 0..9 {
        let mut wrong = draws.clone();
        match changed {
            0 => wrong[0].cast_id = 1,
            1 => wrong[0].purpose = DrawPurpose::WanderRadius { npc: owner },
            2 => {
                wrong[1].purpose = DrawPurpose::WanderRadius {
                    npc: crate::NpcId {
                        generation: owner.generation + 1,
                        ..owner
                    },
                }
            }
            3 => wrong[0].low = 1,
            4 => wrong[0].high -= 1,
            5 => wrong[0].first_counter += 1,
            6 => wrong[1].first_counter += 1,
            7 => wrong[0].consumed += 1,
            8 => wrong[1].value ^= 1,
            _ => unreachable!(),
        }
        assert_eq!(
            validate_draw_pair(&state, owner, &wrong, &request),
            Err(NativeError::AuthorityBinding),
            "draw mutation {changed}",
        );
    }
    for angle in [true, false] {
        let mut wrong = request;
        if angle {
            wrong.angle_draw = f::real(1.);
        } else {
            wrong.radius_draw = f::real(1.);
        }
        assert_eq!(
            validate_draw_pair(&state, owner, &draws, &wrong),
            Err(NativeError::AuthorityBinding),
        );
    }
    state.rng_counter -= 1;
    assert_eq!(
        validate_draw_pair(&state, owner, &draws, &request),
        Err(NativeError::AuthorityBinding),
    );
}

#[test]
fn pending_reopen_wander_head_rejects_metadata_and_inert_cache_relabeling() {
    use crate::native::{synthetic_fixture as f, synthetic_motion_fixture};
    let world = NativeWorld::synthetic_component_test(
        synthetic_motion_fixture::profile(),
        [8; 16],
        f::player(),
        1234,
    )
    .unwrap();
    let identity = world.state.npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .snapshot()
        .identity;
    // Comparison-only fixture: dispatch provenance is separately checked by
    // validate_wander_operation before this exact saved-head comparison runs.
    let expected = motion::RouteRequest {
        binding: motion::QueryBinding {
            identity,
            query_id: 4,
            now_ms: 100,
        },
        origin: f::point(1.),
        launch_origin: f::point(1.),
        goal: f::point(2.),
        kind: motion::RouteKind::Wander,
        wander: Some(Box::new(motion::WanderRequest {
            prepared: motion::QueryBinding {
                identity,
                query_id: 3,
                now_ms: 100,
            },
            center: f::point(0.),
            radius: f::real(3.),
            angle_draw: f::real(0.25),
            radius_draw: f::real(0.5),
        })),
        target: None,
        owner_collision_height: f::real(2.),
        point_limit: motion::PATHFINDER_CACHE_POINT_LIMIT,
        prior_cache: Some(Box::new(motion::PathFinderCache {
            binding: motion::PathFinderCacheBinding {
                identity,
                owner_generation: 1,
                inventory: [1; 32],
                source_epoch: [2; 32],
                navigation_policy: identity.geometry.0,
            },
            revision: 1,
            source: motion::RouteSource::OwnerPathFinder,
            poly_refs: vec![1; usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)],
            active_len: 1,
            point_limit: motion::PATHFINDER_CACHE_POINT_LIMIT,
            path_type: 1,
            requested_end: f::point(1.),
            include_flags: 1,
            exclude_flags: 0,
        })),
        pathfinder_dispatch: None,
    };
    assert_eq!(validate_wander_head_request(&expected, &expected), Ok(()));
    for changed in 0..12 {
        let mut wrong = expected.clone();
        match changed {
            0 => wrong.binding.query_id -= 1,
            1 => wrong.binding.now_ms -= 50,
            2 => wrong.binding.identity.creature += 1,
            3 => wrong.wander.as_mut().unwrap().prepared.query_id -= 1,
            4 => wrong.wander.as_mut().unwrap().angle_draw = f::real(0.5),
            5 => wrong.wander.as_mut().unwrap().radius_draw = f::real(0.25),
            6 => wrong.prior_cache.as_mut().unwrap().revision += 1,
            7 => wrong.prior_cache.as_mut().unwrap().poly_refs[73] ^= 1,
            8 => wrong.launch_origin = f::point(1.5),
            9 => wrong.owner_collision_height = f::real(3.),
            10 => wrong.point_limit -= 1,
            11 => wrong.kind = motion::RouteKind::WanderPrepare,
            _ => unreachable!(),
        }
        // All cases retain the same semantic native from/end positions.
        assert_eq!(wrong.origin, expected.origin);
        assert_eq!(wrong.goal, expected.goal);
        assert_eq!(
            validate_wander_head_request(&wrong, &expected),
            Err(NativeError::AuthorityBinding),
            "saved head relabeling {changed}",
        );
    }
}

/// Authored comparison fixtures contain no native query authority. Actual native
/// reconstruction and atomic publication are exercised by the corridor harness.
fn failed_snapshot(retained_success: bool) -> motion::MotionSnapshot {
    use crate::native::{synthetic_fixture as f, synthetic_motion_fixture};
    let world = NativeWorld::synthetic_component_test(
        synthetic_motion_fixture::profile(),
        [8; 16],
        f::player(),
        1234,
    )
    .unwrap();
    let mut snapshot = world.state.npcs[&2]
        .source_motion
        .as_ref()
        .unwrap()
        .motion
        .snapshot();
    let identity = snapshot.identity;
    let construction = motion::QueryBinding {
        identity,
        query_id: 1,
        now_ms: 0,
    };
    let center = ai::Point::new(5., 5., 0.).unwrap();
    let radius = f::real(0.05);
    let sample = |query_id, now_ms| motion::WanderRequest {
        prepared: motion::QueryBinding {
            identity,
            query_id: query_id - 1,
            now_ms,
        },
        center,
        radius,
        angle_draw: f::real(0.),
        radius_draw: f::real(0.5),
    };
    let request = |origin, query_id, now_ms, prior_cache| {
        let wander = sample(query_id, now_ms);
        motion::RouteRequest {
            binding: motion::QueryBinding {
                identity,
                query_id,
                now_ms,
            },
            origin,
            launch_origin: origin,
            goal: wander.endpoint().unwrap(),
            kind: motion::RouteKind::Wander,
            wander: Some(Box::new(wander)),
            target: None,
            owner_collision_height: f::real(2.),
            point_limit: motion::PATHFINDER_CACHE_POINT_LIMIT,
            prior_cache,
            pathfinder_dispatch: Some(
                serde_json::from_value(serde_json::json!({
                    "kind": "WanderInitialized",
                    "first_use": false,
                    "construction": construction,
                    "dispatch_query_id": query_id,
                    "dispatch_now_ms": now_ms,
                }))
                .unwrap(),
            ),
        }
    };
    let mut cache = motion::PathFinderCache {
        binding: motion::PathFinderCacheBinding {
            identity,
            owner_generation: 1,
            inventory: [1; 32],
            source_epoch: [2; 32],
            navigation_policy: identity.geometry.0,
        },
        revision: 1,
        source: motion::RouteSource::OwnerPathFinder,
        poly_refs: vec![0; usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)],
        active_len: 0,
        point_limit: motion::PATHFINDER_CACHE_POINT_LIMIT,
        path_type: 8,
        requested_end: ai::Point::new(0., 0., 0.).unwrap(),
        include_flags: 1,
        exclude_flags: 0,
    };
    let (origin, query_id, prior) = if retained_success {
        let prior_request = request(ai::Point::new(4., 5., 0.).unwrap(), 3, 50, None);
        let points = vec![prior_request.origin, prior_request.goal];
        let mut spline =
            motion::LinearSpline::launch(points.clone(), points[0], f::real(1.25)).unwrap();
        spline.advance(spline.duration_ms());
        snapshot.spline_id = 1;
        snapshot.spline = Some(motion::ActiveSplineSnapshot {
            route_request: prior_request.clone(),
            route: motion::CompleteRoute {
                source: motion::RouteSource::OwnerPathFinder,
                path_identity: [7; 32],
                points,
                segments: vec![ai::movement::SegmentChecks {
                    loaded_coverage: ai::movement::Check::Passed,
                    line_of_sight: ai::movement::Check::Passed,
                    body_sweep: ai::movement::Check::Passed,
                    continuous_ground_support: ai::movement::Check::Passed,
                    dry_ground: ai::movement::Check::Passed,
                }],
            },
            local_home_cache: None,
            spline_id: 1,
            path_identity: [7; 32],
            route_source: motion::RouteSource::OwnerPathFinder,
            spline: spline.snapshot(),
            facing: motion::Facing::Tangent,
            walking: true,
        });
        cache.poly_refs[0] = 19;
        cache.requested_end = prior_request.goal;
        let mut prior = cache.clone();
        prior.active_len = 1;
        prior.path_type = 1;
        cache.revision = 2;
        (prior_request.goal, 6, Some(Box::new(prior)))
    } else {
        (center, 3, None)
    };
    snapshot.now_ms = 100;
    snapshot.processed_map_clock_ms = 100;
    snapshot.query_id = query_id;
    snapshot.map_pose.position = origin;
    snapshot.generator = motion::Generator::Wander(motion::WanderSnapshot {
        center,
        radius,
        next_move_timer_ms: 500,
        next_move_count: 1,
        pathfinder_cache: Some(Box::new(cache)),
        pathfinder_construction: construction,
        normal_route: None,
        failed_route: Some(Box::new(motion::WanderFailedRouteHead {
            request: request(origin, query_id, 100, prior),
            reason: motion::WanderNoPathReason::MinimumDistance,
        })),
    });
    snapshot
}

fn selected_head(snapshot: &motion::MotionSnapshot) -> Result<SelectedWanderHead<'_>, NativeError> {
    let motion::Generator::Wander(wander) = &snapshot.generator else {
        unreachable!();
    };
    current_wander_head(
        snapshot,
        wander,
        wander.pathfinder_cache.as_deref().unwrap(),
    )
}

#[test]
fn pending_reopen_failed_head_selects_exact_cold_or_retained_operation() {
    for retained in [false, true] {
        let snapshot = failed_snapshot(retained);
        motion::MotionState::restore(snapshot.clone()).unwrap();
        let motion::Generator::Wander(wander) = &snapshot.generator else {
            unreachable!();
        };
        let failed = wander.failed_route.as_deref().unwrap();
        let (head, points, reason) = selected_head(&snapshot).unwrap();
        assert_eq!(head, &failed.request);
        assert!(points.is_empty());
        assert_eq!(
            reason,
            Some(benilla_native_nav::SourceWanderNoPathReason::MinimumDistance)
        );
        if let Some(old_spline) = &snapshot.spline {
            assert_ne!(head, &old_spline.route_request);
            assert_eq!(
                validate_wander_head_request(&old_spline.route_request, head),
                Err(NativeError::AuthorityBinding),
            );
        }
        let cache = wander.pathfinder_cache.as_deref().unwrap();
        assert_eq!(cache.path_type, 8);
        assert_eq!(cache.active_len, 0);
        let expected_actual_end = failed
            .request
            .prior_cache
            .as_ref()
            .map_or(ai::Point::new(0., 0., 0.).unwrap(), |prior| {
                prior.requested_end
            });
        assert_eq!(cache.requested_end, expected_actual_end);
    }
}

#[test]
fn pending_reopen_failed_head_rejects_wrong_reason_missing_head_and_stale_spline() {
    for retained in [false, true] {
        for change in 0..10 {
            let mut snapshot = failed_snapshot(retained);
            let motion::Generator::Wander(wander) = &mut snapshot.generator else {
                unreachable!();
            };
            match change {
                0 => wander.failed_route = None,
                1 => {
                    wander.failed_route.as_mut().unwrap().reason =
                        motion::WanderNoPathReason::EndpointMissing
                }
                2 => {
                    wander.failed_route.as_mut().unwrap().reason =
                        motion::WanderNoPathReason::EndpointHeight
                }
                3 => wander.pathfinder_cache.as_mut().unwrap().revision += 1,
                4 => wander.pathfinder_cache.as_mut().unwrap().requested_end.x = ai::Real::ONE,
                5 => wander.pathfinder_cache.as_mut().unwrap().poly_refs[73] = 9,
                6 => wander.pathfinder_cache.as_mut().unwrap().active_len = 1,
                7 => snapshot.query_id += 1,
                8 => snapshot.map_pose.position.x = ai::Real::ONE,
                9 => {
                    wander.failed_route.as_mut().unwrap().request.kind =
                        motion::RouteKind::WanderPrepare
                }
                _ => unreachable!(),
            }
            assert!(
                selected_head(&snapshot).is_err(),
                "retained={retained}, mutation={change}"
            );
        }
    }
    let mut snapshot = failed_snapshot(true);
    snapshot.spline.as_mut().unwrap().spline.finalized = false;
    assert!(selected_head(&snapshot).is_err());
    let mut snapshot = failed_snapshot(true);
    snapshot
        .spline
        .as_mut()
        .unwrap()
        .route_request
        .binding
        .query_id = snapshot.query_id;
    assert!(selected_head(&snapshot).is_err());
}

#[test]
fn pending_reopen_failed_head_cannot_be_relabelled_by_same_output_metadata() {
    let snapshot = failed_snapshot(true);
    let (expected, _, _) = selected_head(&snapshot).unwrap();
    for change in 0..5 {
        let mut request = expected.clone();
        match change {
            0 => request.binding.query_id -= 1,
            1 => request.binding.now_ms -= 50,
            2 => request.wander.as_mut().unwrap().prepared.query_id -= 1,
            3 => request.prior_cache.as_mut().unwrap().revision += 1,
            4 => request.prior_cache.as_mut().unwrap().poly_refs[73] ^= 1,
            _ => unreachable!(),
        }
        assert_eq!(request.origin, expected.origin);
        assert_eq!(request.goal, expected.goal);
        assert_eq!(
            validate_wander_head_request(&request, expected),
            Err(NativeError::AuthorityBinding),
        );
    }
}

#[test]
fn pending_reopen_failed_head_field_is_required_in_current_snapshot_format() {
    for retained in [false, true] {
        let snapshot = failed_snapshot(retained);
        for field in ["failed_route", "normal_route"] {
            let mut encoded = serde_json::to_value(&snapshot).unwrap();
            encoded["generator"]["Wander"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(serde_json::from_value::<motion::MotionSnapshot>(encoded).is_err());
        }
        let mut encoded = serde_json::to_value(&snapshot).unwrap();
        encoded["generator"]["Wander"]["failed_route"] = serde_json::Value::Null;
        let decoded = serde_json::from_value::<motion::MotionSnapshot>(encoded).unwrap();
        assert!(motion::MotionState::restore(decoded).is_err());
    }
}

#[test]
fn pending_reopen_minimum_distance_certificate_keeps_exact_failed_native_semantics() {
    if !benilla_native_nav::SUPPORTED_ABI {
        eprintln!("skipped: native Wander certificate requires supported Detour ABI");
        return;
    }
    let challenge = SourceWanderReconstructionChallenge::new();
    let certificate = certificate(&challenge, true);
    let expected = certificate.terminal().semantic_state();
    assert_eq!(expected.path_type, 8);
    assert_eq!(expected.active_len, 0);
    assert!(expected.points.is_empty());
    assert_eq!(expected.actual_end, point(0., 0., 0.));
    assert_eq!(
        expected.no_path_reason,
        Some(benilla_native_nav::SourceWanderNoPathReason::MinimumDistance),
    );
    assert_eq!(
        validate_certificate(&certificate, &challenge, 1, &expected),
        Ok(())
    );
    for mutation in 0..7 {
        let mut wrong = expected.clone();
        match mutation {
            0 => wrong.no_path_reason = None,
            1 => {
                wrong.no_path_reason =
                    Some(benilla_native_nav::SourceWanderNoPathReason::EndpointMissing)
            }
            2 => wrong.raw_end[0] ^= 1,
            3 => wrong.actual_end = wrong.raw_end,
            4 => wrong.storage[73] = 1,
            5 => wrong.path_type = 1,
            6 => wrong.points.push(wrong.from),
            _ => unreachable!(),
        }
        assert_eq!(
            validate_certificate(&certificate, &challenge, 1, &wrong),
            Err(NativeError::AuthorityBinding),
            "failed semantic mutation {mutation}",
        );
    }
}

#[test]
fn pending_reopen_normal_head_requires_exact_current_spline() {
    // Selector-only comparison fixture. Full semantic validation and real fresh
    // reconstruction are exercised by the concrete loaded component harness.
    let mut snapshot = failed_snapshot(true);
    let spline = snapshot.spline.as_ref().unwrap().clone();
    let motion::Generator::Wander(wander) = &mut snapshot.generator else {
        unreachable!();
    };
    wander.pathfinder_cache = wander
        .failed_route
        .as_ref()
        .unwrap()
        .request
        .prior_cache
        .clone();
    wander.failed_route = None;
    wander.normal_route = Some(Box::new(motion::WanderNormalRouteHead {
        request: spline.route_request.clone(),
        route: spline.route.clone(),
    }));
    let (request, points, reason) = selected_head(&snapshot).unwrap();
    assert_eq!(request, &spline.route_request);
    assert_eq!(
        points,
        spline
            .route
            .points
            .iter()
            .copied()
            .map(bits)
            .collect::<Vec<_>>()
    );
    assert_eq!(reason, None);
    for mutation in 0..6 {
        let mut wrong = snapshot.clone();
        let motion::Generator::Wander(wander) = &mut wrong.generator else {
            unreachable!();
        };
        match mutation {
            0 => wander.normal_route = None,
            1 => {
                wander
                    .normal_route
                    .as_mut()
                    .unwrap()
                    .request
                    .binding
                    .query_id += 1
            }
            2 => wander.normal_route.as_mut().unwrap().route.path_identity[0] ^= 1,
            3 => wrong.spline = None,
            4 => {
                wrong.spline.as_mut().unwrap().route_request.kind = motion::RouteKind::ChaseInitial
            }
            5 => wander.pathfinder_cache.as_mut().unwrap().revision += 1,
            _ => unreachable!(),
        }
        assert!(selected_head(&wrong).is_err(), "mutation {mutation}");
    }
}
