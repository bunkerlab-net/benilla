// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine habitat-3 witnesses initialized from the same actual authored dry bytes.
use super::super::super::{
    core::{EmptySelectorNearPointPolicy, PrimaryNearPointPolicy},
    products::{ground_height::SourceHeightConfiguration, vmap::QueryBudget},
    source_creature_initialization::{self as initialization, tests as initial},
    source_near_point::{self, SourceNearPointConfiguration, SourceNearPointProvider},
    source_near_point_empty::{self, SourceEmptySelectorNearPointProvider},
};
use super::*;
use benilla_world_runtime::native::{Facts, MotionFacts};
use std::sync::Arc;

fn setup_swimming(
    products: &Arc<products::InventoriedProducts>,
    config: SourceHeightConfiguration,
    revision: [u8; 32],
) -> (NativeWorld, Session, AuthorityRequest) {
    setup_swimming_seed(products, config, revision, 7)
}

fn setup_swimming_seed(
    products: &Arc<products::InventoriedProducts>,
    config: SourceHeightConfiguration,
    revision: [u8; 32],
    seed: u64,
) -> (NativeWorld, Session, AuthorityRequest) {
    setup_swimming_location(products, config, revision, seed, false)
}

fn setup_swimming_location(
    products: &Arc<products::InventoriedProducts>,
    config: SourceHeightConfiguration,
    revision: [u8; 32],
    seed: u64,
    open_region: bool,
) -> (NativeWorld, Session, AuthorityRequest) {
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let initialization_snapshot = Arc::clone(products)
        .load_map_height_owned(config, Some(&participants))
        .unwrap();
    let mut profile = initial::profile(
        initialization::snapshot_binding(&initialization_snapshot),
        revision,
    );
    if open_region {
        // Authored covered terrain at Z=7, beyond the stacked model and before
        // the wall. Retain the raw spawn coordinate in the source closure before
        // any generation/model/liquid work, rather than rewriting a live query.
        let mid = 0.5f32 * (64.0 * 533.333_3);
        profile.npcs[0].ai.spawn = ai::Point::new(mid - 46., mid - 1., 7.).unwrap();
        profile.player.start = ai::Point::new(mid - 40., mid - 1., 7.).unwrap();
        let mut source = profile
            .creature_capabilities
            .as_ref()
            .unwrap()
            .source(2)
            .unwrap()
            .clone();
        let benilla_world_runtime::native::CreatureInitializationClosure::DryWater {
            respawn_position,
            ..
        } = &mut source.initialization
        else {
            unreachable!()
        };
        *respawn_position = profile.npcs[0].ai.spawn;
        profile.creature_capabilities = Some(
            benilla_world_runtime::native::CreatureCapabilityCatalog::validate_sources(
                benilla_world_runtime::native::CreatureCapabilityRows::Complete(vec![source]),
            )
            .unwrap(),
        );
        profile.validate().unwrap();
    }
    let provider = initialization::SourceCreatureInitializationProvider::bind(
        [8; 16],
        &profile,
        initialization_snapshot,
    )
    .unwrap();
    let mut authority = initial::InitialAuthority {
        provider,
        initializations: vec![],
        motion: Default::default(),
    };
    let mut world = NativeWorld::synthetic_component_test_with_initialization(
        profile,
        [8; 16],
        f::player(),
        seed,
        None,
        &mut authority,
    )
    .unwrap();
    assert_eq!(authority.initializations.len(), 1);
    // The legacy synthetic movement authority intentionally rejects Swimming. Capture
    // the real staged request, then answer it with the loaded provider under test.
    for tick in 1..=4 {
        let result = world.step(tick, None, &mut authority);
        if authority
            .motion
            .queries
            .iter()
            .any(|q| matches!(q.query, Query::SourceMotion(MotionQuery::NearPoint(_))))
        {
            break;
        }
        result.unwrap();
    }
    let context = authority
        .motion
        .queries
        .into_iter()
        .find(|q| matches!(q.query, Query::SourceMotion(MotionQuery::NearPoint(_))))
        .unwrap();
    let profile = world.profile();
    let motion = profile.source_motion.as_ref().unwrap();
    let session = Session {
        world: [8; 16],
        profile: profile.digest(),
        policy: profile.authority_policy,
        content: profile.content_digest,
        geometry: revision,
        map: profile.map_id,
        bodies: BTreeMap::from([(
            2,
            Body {
                radius: profile.npcs[0].ai.bounding_radius,
                height: motion.npc(2).unwrap().npc_policy.settings.collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: profile.player.bounding_radius,
                height: motion.player_collision_height,
            },
            speed: ai::Real::new(7.).unwrap(),
        }),
        participants: Some(participants),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        source_location: None,
        player_environment: None,
        same_polygon_route: None,
    };
    (world, session, context)
}

fn assert_swimming(reply: &GroundHeightReply, context: &AuthorityRequest) {
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &context.query else {
        unreachable!()
    };
    let initialization = near.searcher.initialization().unwrap();
    assert_eq!(near.searcher.inhabit_type(), 3);
    assert!(near.searcher.capabilities().unwrap().can_swim());
    assert_eq!(
        near.searcher.allowed_position_z_branch().unwrap(),
        AllowedPositionZBranch::Swimming
    );
    let AllowedHeightObservation::Swimming(source) = &reply.source else {
        panic!("Swimming must execute its own source branch")
    };
    assert!(source.ground.request.swim);
    assert_eq!(
        source.request.collision_height,
        initialization.request.collision_height.bits()
    );
    assert_eq!(
        source.water_or_ground.min_water_depth,
        initialization.request.collision_height.bits()
    );
    assert_eq!(
        source.water_or_ground.classification_height,
        2.03128f32.to_bits()
    );
    assert_eq!(source.water_or_ground.required_type, 15);
    assert_eq!(
        source.water_or_ground.liquid.request.position[2],
        reply.ground_height
    );
    assert_eq!(source.water_or_ground.max_z, reply.ground_height);
    assert_eq!(source.allowed_z, reply.allowed_z);
}

#[test]
fn actual_initialized_habitat_three_uses_swimming_branch_in_primary_and_empty_providers() {
    for empty in [false, true] {
        let (products, height) = products::authored_ground_height();
        let products = Arc::new(products);
        let proof = NonRenderParticipants::authored_fixture(height.content, height.map);
        let snapshot = Arc::clone(&products)
            .load_map_height_owned(height, Some(&proof))
            .unwrap();
        let config = SourceNearPointConfiguration {
            height,
            detect_position_collision: true,
            vmap_line_of_sight_enabled: true,
        };
        let revision = if empty {
            source_near_point_empty::geometry_revision(&snapshot, config)
        } else {
            source_near_point::geometry_revision(&snapshot, config)
        }
        .unwrap();
        let (world, mut session, context) =
            setup_swimming_location(&products, height, revision, 7, true);
        if empty {
            session.empty_selector_near_point =
                Some(EmptySelectorNearPointPolicy::from_profile(world.profile()).unwrap());
        } else {
            session.primary_near_point =
                Some(PrimaryNearPointPolicy::from_profile(world.profile()).unwrap());
        }
        let inspect_snapshot = Arc::clone(&products)
            .load_map_height_owned(height, Some(&proof))
            .unwrap();
        let inspect = if empty {
            SourceGroundHeightProvider::bind_with_empty_selector(
                &session,
                world.profile(),
                inspect_snapshot,
                config,
            )
        } else {
            SourceGroundHeightProvider::bind_with_primary(
                &session,
                world.profile(),
                inspect_snapshot,
                config,
            )
        }
        .unwrap();
        let candidate = inspect
            .begin(&session, &context)
            .unwrap()
            .derive_initial_candidate(1, deadline())
            .unwrap();
        assert_swimming(&candidate.ground, &context);
        let before = world.checkpoint().unwrap();
        let facts = if empty {
            SourceEmptySelectorNearPointProvider::bind(&session, world.profile(), snapshot, config)
                .unwrap()
                .resolve(&session, &context, deadline())
        } else {
            SourceNearPointProvider::bind(&session, world.profile(), snapshot, config)
                .unwrap()
                .resolve(&session, &context, deadline())
        }
        .unwrap();
        let Facts::SourceMotion(MotionFacts::Spatial(reply)) = facts else {
            panic!("expected source motion")
        };
        assert!(matches!(
            reply.fact,
            benilla_sim::creature_motion::Fact::NearPoint(_)
        ));
        assert_eq!(world.checkpoint().unwrap(), before);
    }
}

#[test]
fn swimming_empty_sequence_preserves_carried_z_order_and_poison_after_valid_probes() {
    let (products, config) = products::authored_ground_height();
    let products = Arc::new(products);
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = Arc::clone(&products)
        .load_map_height_owned(config, Some(&proof))
        .unwrap();
    let (world, session, context) =
        setup_swimming(&products, config, geometry_revision(snapshot.binding()));
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    let mid = 0.5f32 * (64.0 * 533.333_3);
    let mut context = context;
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut context.query else {
        unreachable!()
    };
    // Explicit probe derivatives preserve the initialized source witness; they do
    // not claim these test poses were committed to the world.
    near.request.target.pose.position = ai::Point::new(mid - 31., mid - 1., 10.).unwrap();
    near.request.distance = ai::Real::new(10.).unwrap();
    near.request.angle = NearPointAngle::Absolute(ai::Real::ZERO);
    let mut sequence = provider
        .begin(&session, &context)
        .unwrap()
        .empty_selector_sequence()
        .unwrap();
    let mut budget = QueryBudget::new(deadline());
    let first = sequence.next_with_budget(&mut budget).unwrap().unwrap();
    assert_eq!(first.phase, SequentialGroundPhase::Primary);
    assert_swimming(&first.ground, &context);
    assert_eq!(first.ground.allowed_z, 9f32.to_bits());
    let mut previous = first.ground.allowed_z;
    let mut count = 0;
    loop {
        let reply = sequence.next_with_budget(&mut budget).unwrap().unwrap();
        assert_swimming(&reply.ground, &context);
        if reply.phase == SequentialGroundPhase::Final {
            assert_eq!(
                reply.ground.request.position,
                [
                    first.ground.request.position[0],
                    first.ground.request.position[1],
                    previous
                ]
            );
            assert_eq!(previous, 7f32.to_bits());
            assert_eq!(reply.ground.allowed_z, previous);
            assert_eq!(reply.los_ticket, None);
            break;
        }
        assert_eq!(reply.phase, SequentialGroundPhase::Free);
        assert_eq!(reply.ground.request.position[2], 10f32.to_bits());
        assert_eq!(reply.ground.request.ticket, 2 * (count + 1) + 1);
        previous = reply.ground.allowed_z;
        count += 1;
    }
    assert!(count > 0);
    assert_eq!(sequence.next_with_budget(&mut budget), Ok(None));

    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut context.query else {
        unreachable!()
    };
    near.request.target.pose.position = ai::Point::new(mid - 532., mid - 10., 10.).unwrap();
    near.request.distance = ai::Real::new(4.).unwrap();
    let mut sequence = provider
        .begin(&session, &context)
        .unwrap()
        .empty_selector_sequence()
        .unwrap();
    let mut budget = QueryBudget::new(deadline());
    assert_eq!(
        sequence
            .next_with_budget(&mut budget)
            .unwrap()
            .unwrap()
            .phase,
        SequentialGroundPhase::Primary
    );
    let mut valid = 0;
    loop {
        match sequence.next_with_budget(&mut budget) {
            Ok(Some(reply)) => {
                assert_eq!(reply.phase, SequentialGroundPhase::Free);
                valid += 1;
            }
            Err(error) => {
                assert_eq!(error, Failure::MissingGeometry);
                break;
            }
            Ok(None) => panic!("missing geometry is not exhaustion"),
        }
    }
    assert!(valid > 0);
    assert_eq!(
        sequence.next_with_budget(&mut QueryBudget::new(deadline())),
        Err(Failure::MissingGeometry)
    );
}

#[test]
fn copied_foreign_world_or_future_initialization_cannot_supply_swimming_height() {
    let (products, config) = products::authored_ground_height();
    let products = Arc::new(products);
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = Arc::clone(&products)
        .load_map_height_owned(config, Some(&proof))
        .unwrap();
    let (world, session, context) =
        setup_swimming(&products, config, geometry_revision(snapshot.binding()));
    let provider = SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
    assert!(provider.begin(&session, &context).is_ok());
    for case in 0..3 {
        let mut wire = serde_json::to_value(&context).unwrap();
        let initialization = &mut wire["query"]["SourceMotion"]["NearPoint"]["searcher"]
            ["initialization"]["request"];
        match case {
            0 => initialization["binding"]["world"][0] = 99.into(),
            1 => {
                let future_tick = context.binding.tick + 1;
                initialization["binding"]["tick"] = future_tick.into();
                initialization["now_ms"] = (world.profile().clock_origin_ms
                    + future_tick * u64::from(world.profile().tick_ms))
                .into();
            }
            _ => initialization["snapshot"]["source_epoch"][0] = 99.into(),
        }
        let changed: AuthorityRequest = serde_json::from_value(wire).unwrap();
        assert!(
            matches!(provider.begin(&session, &changed), Err(Failure::Binding)),
            "case {case}"
        );
    }
}

#[test]
fn selected_generation_height_is_used_even_when_initial_session_height_differs() {
    let (products, config) = products::authored_ground_height();
    let products = Arc::new(products);
    let proof = NonRenderParticipants::authored_fixture(config.content, config.map);
    let mut heights = std::collections::BTreeSet::new();
    let mut differs_from_initial = false;
    for seed in 1..=16 {
        let snapshot = Arc::clone(&products)
            .load_map_height_owned(config, Some(&proof))
            .unwrap();
        let (world, session, context) = setup_swimming_seed(
            &products,
            config,
            geometry_revision(snapshot.binding()),
            seed,
        );
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &context.query else {
            unreachable!()
        };
        let selected = near
            .searcher
            .initialization()
            .unwrap()
            .request
            .collision_height
            .bits();
        heights.insert(selected);
        differs_from_initial |= selected != session.bodies[&2].height.as_f32().to_bits();
        let provider =
            SourceGroundHeightProvider::bind(&session, world.profile(), snapshot).unwrap();
        let result = provider
            .begin(&session, &context)
            .unwrap()
            .derive_initial_candidate(1, deadline())
            .unwrap();
        assert_swimming(&result.ground, &context);
    }
    assert_eq!(heights.len(), 2);
    assert!(differs_from_initial);
}

#[test]
fn swimming_stacked_fixture_still_requires_fallback_instead_of_relaxing_primary_guards() {
    let (products, height) = products::authored_ground_height();
    let products = Arc::new(products);
    let proof = NonRenderParticipants::authored_fixture(height.content, height.map);
    let snapshot = Arc::clone(&products)
        .load_map_height_owned(height, Some(&proof))
        .unwrap();
    let config = SourceNearPointConfiguration {
        height,
        detect_position_collision: true,
        vmap_line_of_sight_enabled: true,
    };
    let revision = source_near_point::geometry_revision(&snapshot, config).unwrap();
    let (world, mut session, context) = setup_swimming(&products, height, revision);
    session.primary_near_point =
        Some(PrimaryNearPointPolicy::from_profile(world.profile()).unwrap());
    let provider =
        SourceNearPointProvider::bind(&session, world.profile(), snapshot, config).unwrap();
    assert_eq!(
        provider.resolve(&session, &context, deadline()),
        Err(Failure::UnsupportedNearPointFallback)
    );
}
