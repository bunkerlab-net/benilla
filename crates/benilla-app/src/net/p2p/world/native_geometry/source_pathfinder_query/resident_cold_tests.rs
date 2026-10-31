// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine generated geometry and actual NativeWorld dispatch reach only the
//! private resident cold terminal. Authored motion replies advance the test world;
//! neither those replies nor numeric terminal observations certify a movement Route.
use super::super::{
    core::{Body, Failure, NonRenderParticipants, PlayerMotion, Session},
    products::{
        source_height::{CompositionError, PreparedSourceHeight},
        source_query::SourceMapQuerySnapshot,
    },
    source_route_filter::resident::{self, ResidentFilterError, ResidentFilterProvider},
};
use benilla_native_nav::{
    SourceOwnerFilter, SourceSamePolygonBudget, SourceSamePolygonConfig,
    SOURCE_COLD_DISTANCE_COMPATIBILITY_ID, SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
};
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture as motion_fixture, AuthorityRequest, Command,
    CreatureRouteRequest, Input, MotionQuery, NativeWorld, OwnedInput, Profile, Query,
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    time::{Duration, Instant},
};

const CONFIG: SourceSamePolygonConfig = SourceSamePolygonConfig {
    normalize_z: false,
    ignore_normalization: false,
};
fn until() -> Instant {
    Instant::now() + Duration::from_secs(60)
}
fn bits(point: ai::Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}
fn route(context: &AuthorityRequest) -> &CreatureRouteRequest {
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        panic!("expected actual route dispatch")
    };
    route
}
fn route_mut(context: &mut AuthorityRequest) -> &mut CreatureRouteRequest {
    let Query::SourceMotion(MotionQuery::Route(route)) = &mut context.query else {
        panic!("expected actual route dispatch")
    };
    route
}
fn session(p: &Profile) -> Session {
    let settings = p.source_motion.as_ref().unwrap();
    Session {
        world: [8; 16],
        profile: p.digest(),
        policy: p.authority_policy,
        content: p.content_digest,
        geometry: p.geometry_revision,
        map: p.map_id,
        bodies: BTreeMap::from([(
            2,
            Body {
                radius: p.npcs[0].ai.bounding_radius,
                height: settings
                    .npc(2)
                    .unwrap()
                    .npc_policy
                    .settings
                    .collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: p.player.bounding_radius,
                height: settings.player_collision_height,
            },
            speed: ai::Real::new(7.0).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            p.content_digest,
            p.map_id,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        source_location: None,
        player_environment: None,
        same_polygon_route: None,
    }
}
fn first_dispatch(p: &Profile) -> (NativeWorld, AuthorityRequest) {
    let mut world =
        NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), 1234).unwrap();
    let mut authority = motion_fixture::SyntheticMotionAuthority::default();
    for tick in 1..=8 {
        world.step(tick, None, &mut authority).unwrap();
        if let Some(context) = std::mem::take(&mut authority.queries)
            .into_iter()
            .find(|q| matches!(q.query, Query::SourceMotion(MotionQuery::Route(_))))
        {
            assert_eq!(
                route(&context).request.kind,
                motion::RouteKind::ChaseInitial
            );
            assert!(context
                .require_pathfinder_dispatch(p)
                .unwrap()
                .first_construction());
            assert_eq!(
                context
                    .require_pathfinder_dispatch(p)
                    .unwrap()
                    .constructor_kind(),
                motion::PathfinderConstructorKind::ChaseLazy,
            );
            assert!(route(&context).request.prior_cache.is_none());
            assert_eq!(route(&context).request.point_limit, 74);
            assert_eq!(
                route(&context).request.launch_origin,
                route(&context).request.origin
            );
            return (world, context);
        }
    }
    panic!("no genuine first Chase dispatch")
}

pub(crate) fn assert_resident_cold_terminal(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    replacement_query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    mut change_source: impl FnMut(bool),
) {
    p.geometry_revision = resident::geometry_revision(height.binding(), query.binding());
    assert_eq!(query.binding(), replacement_query.binding());
    let s = session(&p);
    let mut provider = ResidentFilterProvider::bind(&s, &p, height, query).unwrap();
    let mut other = ResidentFilterProvider::bind(&s, &p, height, replacement_query).unwrap();
    assert_eq!(provider.retained_constructor_count(), 0);
    let (mut world, context) = first_dispatch(&p);
    assert!(!route(&context)
        .require_owner_ignore_pathfinding(&p)
        .unwrap()
        .ignore_pathfinding());
    let prior = provider.observe(&context, 101, until(), || false).unwrap();
    assert_eq!(provider.retained_constructor_count(), 1);
    let flags = provider
        .filter_for(&prior, &context, until(), || false)
        .unwrap();
    assert_eq!(flags, SourceOwnerFilter::GROUND);
    assert_eq!(prior.observations().map(|r| r.request().ticket), [101, 102]);

    let mut stages = Vec::new();
    let terminal = provider
        .observe_cold_same_polygon_observed(
            &prior,
            &context,
            105,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
            || false,
            |stage| stages.push(stage),
        )
        .unwrap();
    assert_eq!(stages, [0, 1, 2, 3, 4, 5]);
    let observed = provider
        .terminal_for(&terminal, &context, until(), || false)
        .unwrap();
    let request = &route(&context).request;
    assert_eq!(observed.points, [bits(request.origin), bits(request.goal)]);
    assert_ne!(observed.points[0], observed.points[1]);
    assert_eq!(observed.actual_end, bits(request.goal));
    assert_eq!(observed.path_type, 1);
    assert_eq!(observed.active_len, 1);
    assert_eq!(observed.filter, flags);
    assert_eq!(observed.query_capacity, 1024);
    assert!(observed.before_storage_zero);
    assert_eq!(
        observed.compatibility_id,
        SOURCE_COLD_DISTANCE_COMPATIBILITY_ID
    );
    assert_eq!(
        observed.compatibility_version,
        SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION
    );
    assert_eq!(observed.liquid_tickets, [105, 106]);
    assert_eq!(observed.liquid_masks, [15, 10]);
    assert_eq!(provider.retained_constructor_count(), 1);
    // Calculate-time updateFilter is new work. The constructor's point tickets
    // remain unchanged and cannot themselves serve as its new liquid observations.
    assert_eq!(prior.observations().map(|r| r.request().ticket), [101, 102]);

    // Equal serialized bytes, a distinct live event, changed body/route/context,
    // and equal-byte replacement providers cannot inherit the exact dependency.
    let decoded: AuthorityRequest =
        serde_json::from_value(serde_json::to_value(&context).unwrap()).unwrap();
    assert_eq!(decoded, context);
    let (_, independent) = first_dispatch(&p);
    assert_eq!(independent, context);
    let mut invalid = vec![decoded, independent];
    let mut changed = context.clone();
    changed.binding.world[0] ^= 1;
    invalid.push(changed);
    let mut changed = context.clone();
    changed.binding.tick += 1;
    invalid.push(changed);
    let mut changed = context.clone();
    changed.binding.npc.as_mut().unwrap().generation += 1;
    invalid.push(changed);
    let mut changed = context.clone();
    let r = route_mut(&mut changed);
    r.owner_position.x = ai::Real::new(r.owner_position.x.as_f32() + 1.0).unwrap();
    r.request.origin = r.owner_position;
    r.request.launch_origin = r.owner_position;
    invalid.push(changed);
    let mut changed = context.clone();
    route_mut(&mut changed).request.point_limit = 73;
    invalid.push(changed);
    let mut changed = context.clone();
    route_mut(&mut changed).request.launch_origin = route(&context).request.goal;
    invalid.push(changed);
    let mut changed = context.clone();
    route_mut(&mut changed).owner_ignore_pathfinding = None;
    invalid.push(changed);
    for kind in [
        motion::RouteKind::HomeForceDestination,
        motion::RouteKind::Wander,
    ] {
        let mut changed = context.clone();
        route_mut(&mut changed).request.kind = kind;
        invalid.push(changed);
    }
    for invalid in invalid {
        let mut stages = Vec::new();
        assert!(provider
            .observe_cold_same_polygon_observed(
                &prior,
                &invalid,
                109,
                CONFIG,
                SourceSamePolygonBudget::default(),
                until(),
                || false,
                |stage| stages.push(stage),
            )
            .is_err());
        assert!(stages.is_empty());
        assert!(provider
            .terminal_for(&terminal, &invalid, until(), || false)
            .is_err());
    }
    assert!(other
        .observe_cold_same_polygon(
            &prior,
            &context,
            109,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
            || false,
        )
        .is_err());
    assert!(other
        .terminal_for(&terminal, &context, until(), || false)
        .is_err());
    for ticket in [0, 101, 102, u64::MAX] {
        let mut stages = Vec::new();
        assert!(provider
            .observe_cold_same_polygon_observed(
                &prior,
                &context,
                ticket,
                CONFIG,
                SourceSamePolygonBudget::default(),
                until(),
                || false,
                |stage| stages.push(stage),
            )
            .is_err());
        assert!(stages.is_empty());
    }

    // A valid constructor point filter supplies no owner-ignore receipt. Prove
    // that this independent calculate gate stops after selection, before tiles
    // and the fresh liquid pair, rather than merely testing context inequality.
    let mut missing_ignore = context.clone();
    route_mut(&mut missing_ignore).owner_ignore_pathfinding = None;
    let missing_ignore_prior = provider
        .observe(&missing_ignore, 151, until(), || false)
        .unwrap();
    let mut stages = Vec::new();
    assert!(provider
        .observe_cold_same_polygon_observed(
            &missing_ignore_prior,
            &missing_ignore,
            155,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
            || false,
            |stage| stages.push(stage),
        )
        .is_err());
    assert_eq!(stages, [0]);

    assert!(matches!(
        provider.observe_cold_same_polygon(
            &prior,
            &context,
            109,
            CONFIG,
            SourceSamePolygonBudget::default(),
            Instant::now(),
            || false,
        ),
        Err(ResidentFilterError::Context(Failure::Deadline))
    ));
    assert!(matches!(
        provider.terminal_for(&terminal, &context, Instant::now(), || false),
        Err(ResidentFilterError::Context(Failure::Deadline))
    ));
    assert!(matches!(
        provider.terminal_for(&terminal, &context, until(), || true),
        Err(ResidentFilterError::Source(CompositionError::Cancelled))
    ));
    // Every exposed source phase has a cancellation guard before publication.
    // Retained selection may survive failure; neither the constructor filter nor
    // a prior private terminal is replaced, and retry uses fresh point work.
    for stop in 0..=5 {
        let cancelled = Cell::new(false);
        let mut stages = Vec::new();
        let result = provider.observe_cold_same_polygon_observed(
            &prior,
            &context,
            200 + u64::from(stop) * 4,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
            || cancelled.get(),
            |stage| {
                stages.push(stage);
                if stage == stop {
                    cancelled.set(true);
                }
            },
        );
        assert!(result.is_err(), "cancel after stage {stop}");
        assert_eq!(stages.last(), Some(&stop));
        assert_eq!(
            provider
                .terminal_for(&terminal, &context, until(), || false)
                .unwrap()
                .points,
            observed.points
        );
        let retry = provider
            .observe_cold_same_polygon(
                &prior,
                &context,
                202 + u64::from(stop) * 4,
                CONFIG,
                SourceSamePolygonBudget::default(),
                until(),
                || false,
            )
            .unwrap();
        assert_eq!(
            provider
                .terminal_for(&retry, &context, until(), || false)
                .unwrap()
                .points,
            observed.points
        );
        assert_eq!(provider.retained_constructor_count(), 1);
    }
    // Source mutation is observed after selection, owner/tile gates, around each
    // liquid query, and native terminal work. Restoring exact authored inputs allows a
    // legitimate retry; historical ownership alone never bypasses fresh checks.
    for stop in 0..=5 {
        let mut reached = false;
        let result = provider.observe_cold_same_polygon_observed(
            &prior,
            &context,
            300 + u64::from(stop) * 4,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
            || false,
            |stage| {
                if stage == stop {
                    reached = true;
                    change_source(true);
                }
            },
        );
        assert!(reached, "source change at stage {stop}");
        assert!(result.is_err(), "changed source after stage {stop}");
        assert!(provider
            .terminal_for(&terminal, &context, until(), || false)
            .is_err());
        change_source(false);
        let retry = provider
            .observe_cold_same_polygon(
                &prior,
                &context,
                302 + u64::from(stop) * 4,
                CONFIG,
                SourceSamePolygonBudget::default(),
                until(),
                || false,
            )
            .unwrap();
        assert_eq!(
            provider
                .terminal_for(&retry, &context, until(), || false)
                .unwrap()
                .points,
            observed.points
        );
        assert_eq!(provider.retained_constructor_count(), 1);
    }
    // Expiry after native work still discards the candidate rather than exposing
    // a terminal whose only check preceded a synchronous native primitive.
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut reached = false;
    let result = provider.observe_cold_same_polygon_observed(
        &prior,
        &context,
        401,
        CONFIG,
        SourceSamePolygonBudget::default(),
        deadline,
        || false,
        |stage| {
            if stage == 5 {
                reached = true;
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            }
        },
    );
    assert!(reached);
    assert!(result.is_err());
    let retry = provider
        .observe_cold_same_polygon(
            &prior,
            &context,
            405,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
            || false,
        )
        .unwrap();
    assert_eq!(
        provider
            .terminal_for(&retry, &context, until(), || false)
            .unwrap()
            .points,
        observed.points
    );

    for config in [
        SourceSamePolygonConfig {
            normalize_z: true,
            ignore_normalization: false,
        },
        SourceSamePolygonConfig {
            normalize_z: false,
            ignore_normalization: true,
        },
    ] {
        assert!(provider
            .observe_cold_same_polygon(
                &prior,
                &context,
                409,
                config,
                SourceSamePolygonBudget::default(),
                until(),
                || false,
            )
            .is_err());
    }
    for budget in [
        SourceSamePolygonBudget {
            lookup_primitives: 0,
            terminal_primitives: 1,
        },
        SourceSamePolygonBudget {
            lookup_primitives: 5,
            terminal_primitives: 0,
        },
    ] {
        assert!(provider
            .observe_cold_same_polygon(&prior, &context, 409, CONFIG, budget, until(), || false,)
            .is_err());
    }

    let mut expired = provider.observe(&context, 501, until(), || false).unwrap();
    expired.expire_for_test();
    let mut stages = Vec::new();
    assert!(provider
        .observe_cold_same_polygon_observed(
            &expired,
            &context,
            505,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
            || false,
            |stage| stages.push(stage),
        )
        .is_err());
    assert!(stages.is_empty());
    let fresh = provider.observe(&context, 509, until(), || false).unwrap();
    let refreshed = provider
        .observe_cold_same_polygon(
            &fresh,
            &context,
            513,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
            || false,
        )
        .unwrap();
    assert_eq!(
        provider
            .terminal_for(&refreshed, &context, until(), || false)
            .unwrap()
            .points,
        observed.points
    );

    // A genuine later Chase dispatch cannot enter this cold-first-only method,
    // even with freshly queried, valid updateFilter evidence for that dispatch.
    let first_tick = world.state().tick;
    let mut authority = motion_fixture::SyntheticMotionAuthority::default();
    for tick in first_tick + 1..=first_tick + 10 {
        let input = OwnedInput {
            authenticated_owner: world.state().player.owner,
            input: Input {
                world: world.state().world,
                tick,
                player: world.state().player,
                sequence: world.state().next_sequence,
                command: Command::MovementPose {
                    destination: ai::Point::new(
                        p.player.start.x.as_f32(),
                        p.player.start.y.as_f32() + (tick - first_tick) as f32 * 0.6,
                        p.player.start.z.as_f32(),
                    )
                    .unwrap(),
                    orientation: world.state().orientation,
                    movement_flags: motion::GroundMovementFlags::from_bits(1).unwrap(),
                },
            },
        };
        world.step(tick, Some(input), &mut authority).unwrap();
        for next in std::mem::take(&mut authority.queries) {
            let Query::SourceMotion(MotionQuery::Route(next_route)) = &next.query else {
                continue;
            };
            if next_route
                .require_pathfinder_dispatch(&p)
                .unwrap()
                .first_construction()
            {
                continue;
            }
            let updated = provider
                .observe_update(&fresh, &next, 601, until(), || false)
                .unwrap();
            assert_eq!(
                provider
                    .filter_for(&updated, &next, until(), || false)
                    .unwrap(),
                flags
            );
            let mut stages = Vec::new();
            assert!(provider
                .observe_cold_same_polygon_observed(
                    &updated,
                    &next,
                    605,
                    CONFIG,
                    SourceSamePolygonBudget::default(),
                    until(),
                    || false,
                    |stage| stages.push(stage),
                )
                .is_err());
            assert!(stages.is_empty());
            assert!(provider
                .terminal_for(&terminal, &next, until(), || false)
                .is_err());
            return;
        }
    }
    panic!("no actual retained Chase dispatch")
}

pub(crate) fn assert_resident_missing_tile_short_circuits(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
) {
    p.geometry_revision = resident::geometry_revision(height.binding(), query.binding());
    let mut provider = ResidentFilterProvider::bind(&session(&p), &p, height, query).unwrap();
    let (_, context) = first_dispatch(&p);
    let prior = provider.observe(&context, 101, until(), || false).unwrap();
    assert_eq!(provider.retained_constructor_count(), 1);
    assert_eq!(prior.observations().map(|r| r.request().mask), [15, 10]);
    // The constructor point is genuinely dry. The other endpoint's absent tile
    // nevertheless prevents calculate-time liquid work and the private terminal.
    for ticket in [105, 109] {
        let mut stages = Vec::new();
        let result = provider.observe_cold_same_polygon_observed(
            &prior,
            &context,
            ticket,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
            || false,
            |stage| stages.push(stage),
        );
        assert!(result.is_err());
        assert_eq!(stages, [0, 1]);
        assert_eq!(
            provider
                .filter_for(&prior, &context, until(), || false)
                .unwrap(),
            SourceOwnerFilter::GROUND
        );
        assert_eq!(provider.retained_constructor_count(), 1);
    }
}

pub(crate) fn assert_resident_different_polygon_rejected(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
) {
    p.geometry_revision = resident::geometry_revision(height.binding(), query.binding());
    let mut provider = ResidentFilterProvider::bind(&session(&p), &p, height, query).unwrap();
    let (_, context) = first_dispatch(&p);
    let prior = provider.observe(&context, 101, until(), || false).unwrap();
    // Both endpoints have native tiles and calculate-time updateFilter completes,
    // but the real native lookup declines the conditional same-polygon branch.
    // Repeating the operation neither publishes a fallback nor adds a query.
    for ticket in [105, 109] {
        let mut stages = Vec::new();
        let result = provider.observe_cold_same_polygon_observed(
            &prior,
            &context,
            ticket,
            CONFIG,
            SourceSamePolygonBudget::default(),
            until(),
            || false,
            |stage| stages.push(stage),
        );
        assert!(matches!(
            result,
            Err(ResidentFilterError::Terminal(
                benilla_native_nav::SourceSamePolygonError::NonSamePolygonUnsupported
            ))
        ));
        assert_eq!(stages, [0, 1, 2, 3, 4]);
        assert_eq!(provider.retained_constructor_count(), 1);
        assert_eq!(
            provider
                .filter_for(&prior, &context, until(), || false)
                .unwrap(),
            SourceOwnerFilter::GROUND
        );
    }
}
