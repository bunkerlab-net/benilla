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
                    raw_end: point(6., 5., 0.25),
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
    let certificate = certificate(&challenge);
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
