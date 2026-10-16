// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual terminal smooth-calculation geometry; initial cache inputs are authored fixtures.
use super::super::products::navigation::SourceSmoothRequest;
use super::super::{
    core::{Body, NonRenderParticipants, PlayerMotion},
    products::{self, InventoriedProducts},
};
use super::*;
use benilla_native_nav::{SourceOwnerFilter, SourceSmoothingBudget};
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture as fixture, NativeWorld,
};
use std::{collections::BTreeMap, time::Duration};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn configuration() -> PointPathConfiguration {
    PointPathConfiguration {
        warm: WarmCacheConfiguration {
            budget: benilla_native_nav::SourceWarmCacheBudget::default(),
        },
        optimize_path: true,
        normalize_z: false,
        ignore_normalization: false,
        budget: SourcePointPathBudget {
            smoothing_primitives: 256,
            postprocess_primitives: 221,
        },
    }
}

fn origin() -> ai::Point {
    ai::Point::new(-400., -400., 0.).unwrap()
}
fn setup(
    products: &InventoriedProducts,
    config: PointPathConfiguration,
    position: ai::Point,
) -> (
    NativeWorld,
    Session,
    AuthorityRequest,
    NavigationSnapshot,
    WarmCacheLineage,
) {
    let liquid = products.load_dry_liquid().unwrap();
    let navigation = products.load_navigation().unwrap();
    let binding = liquid.binding();
    let mut profile = fixture::profile();
    profile.content_digest = binding.content;
    profile.map_id = binding.map;
    profile.geometry_revision = geometry_revision(binding, navigation.binding(), config).unwrap();
    profile.npcs[0].ai.map_id = binding.map;
    profile.npcs[0].ai.content_revision = ai::movement::Revision(binding.content);
    profile.npcs[0].ai.spawn = position;
    profile.player.start = ai::Point::new(
        position.x.as_f32(),
        position.y.as_f32() + 16.,
        position.z.as_f32(),
    )
    .unwrap();
    profile
        .initialize_creature_capabilities(fixture::creature_capability_sources(&profile))
        .unwrap();
    let motion = profile.source_motion.as_ref().unwrap();
    let session = Session {
        world: [8; 16],
        profile: profile.digest(),
        policy: profile.authority_policy,
        content: profile.content_digest,
        geometry: profile.geometry_revision,
        map: profile.map_id,
        bodies: BTreeMap::from([(
            2,
            Body {
                radius: profile.npcs[0].ai.bounding_radius,
                height: motion.npc_policy.settings.collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: profile.player.bounding_radius,
                height: motion.player_collision_height,
            },
            speed: ai::Real::new(7.).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            binding.content,
            binding.map,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        player_environment: None,
    };
    let mut world =
        NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234).unwrap();
    let mut authority = fixture::SyntheticMotionAuthority::default();
    for tick in 1..=2 {
        world.step(tick, None, &mut authority).unwrap();
    }
    let mut context = authority
        .queries
        .into_iter()
        .find(|q| matches!(q.query, Query::SourceMotion(MotionQuery::Route(_))))
        .unwrap();
    let lineage = navigation
        .seed_bounded_warm_lineage(
            SourceSmoothRequest {
                snapshot: navigation.binding(),
                ticket: 1,
                from: [-400f32, -400., 0.].map(f32::to_bits),
                to: [-400f32, -200., 0.].map(f32::to_bits),
                filter: SourceOwnerFilter {
                    include_flags: 1,
                    exclude_flags: 0,
                },
                point_limit: 74,
                budget: SourceSmoothingBudget::default(),
            },
            deadline(),
        )
        .unwrap();
    // Explicitly authored warm-state unit fixture; controller tests separately
    // exercise a cache actually owned and issued by the running transaction.
    let Query::SourceMotion(MotionQuery::Route(route)) = &mut context.query else {
        unreachable!()
    };
    let nb = navigation.binding();
    route.request.prior_cache = Some(Box::new(motion::PathFinderCache {
        binding: motion::PathFinderCacheBinding {
            identity: route.request.binding.identity,
            owner_generation: route.owner.npc().generation,
            inventory: nb.inventory,
            source_epoch: nb.source_epoch,
            navigation_policy: nb.policy,
        },
        revision: 1,
        source: motion::RouteSource::SyntheticFixture,
        poly_refs: lineage.cache().storage().to_vec(),
        active_len: lineage.cache().active_len() as u16,
        point_limit: 74,
        path_type: 1,
        requested_end: route.request.goal,
        include_flags: 1,
        exclude_flags: 0,
    }));
    (world, session, context, navigation, lineage)
}

fn provider(
    products: &InventoriedProducts,
    world: &NativeWorld,
    session: &Session,
    navigation: NavigationSnapshot,
    lineage: WarmCacheLineage,
    config: PointPathConfiguration,
) -> SourceRoutePointPathProvider {
    SourceRoutePointPathProvider::bind(
        session,
        world.profile(),
        products.load_dry_liquid().unwrap(),
        navigation,
        lineage,
        config,
    )
    .unwrap()
}
fn request(
    provider: &SourceRoutePointPathProvider,
    context: &AuthorityRequest,
) -> PointPathRequest {
    PointPathRequest {
        snapshot: provider.binding(),
        context: context.clone(),
        first_ticket: 41,
    }
}
fn route(
    context: &mut AuthorityRequest,
) -> &mut benilla_world_runtime::native::CreatureRouteRequest {
    let Query::SourceMotion(MotionQuery::Route(route)) = &mut context.query else {
        panic!("route")
    };
    route
}
#[test]
fn actual_same_polygon_shortcut_bypasses_smoothing_and_preserves_runtime() {
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context, navigation, lineage) = setup(&products, config, origin());
    let before = world.checkpoint().unwrap();
    let retained = lineage.cache().clone();
    let provider = provider(&products, &world, &session, navigation, lineage, config);
    let bound = provider.begin(&session, &context).unwrap();
    let query = request(&provider, &context);
    let first = bound.query(query.clone(), deadline()).unwrap();
    assert_eq!(first, bound.query(query.clone(), deadline()).unwrap());
    assert_eq!(first.request(), &query);
    let warm = first.warm_dependency();
    assert_eq!(
        warm.filter_dependency()
            .liquid_observations()
            .map(|x| x.reply.request.ticket),
        [41, 42]
    );
    assert_eq!(warm.polygon_dependency().request().ticket, 43);
    assert_eq!(first.point_dependency().request().ticket, 44);
    let result = first.point_dependency().result();
    assert_eq!(result.path_type(), 1);
    assert_eq!(result.smooth_status(), None);
    assert_eq!(result.smooth_stop(), None);
    assert_eq!(
        result.points(),
        &[
            warm.polygon_dependency().result().from(),
            warm.polygon_dependency().result().to()
        ]
    );
    assert_eq!(result.actual_end(), warm.polygon_dependency().result().to());
    assert_eq!(result.after().storage(), retained.storage());
    assert_eq!(result.after().active_len(), 1);
    assert_eq!(world.checkpoint().unwrap(), before);
}
#[test]
fn actual_selected_warm_prefix_smooths_and_retains_prior_incomplete_type() {
    let products = products::authored_route_smoothing();
    for optimize in [false, true] {
        for previous in [1, 4] {
            let mut config = configuration();
            config.optimize_path = optimize;
            let (world, session, mut context, navigation, lineage) =
                setup(&products, config, origin());
            let q = route(&mut context);
            q.request.goal = ai::Point::new(-400., -200., 0.).unwrap();
            q.request.prior_cache.as_mut().unwrap().path_type = previous;
            let retained = lineage.cache().clone();
            let before = world.checkpoint().unwrap();
            let provider = provider(&products, &world, &session, navigation, lineage, config);
            let result = provider
                .begin(&session, &context)
                .unwrap()
                .query(request(&provider, &context), deadline())
                .unwrap();
            let points = result.point_dependency().result();
            assert_eq!(points.smooth_status(), Some(1 << 30));
            assert!(points.points().len() >= 2);
            assert_eq!(points.path_type(), previous);
            assert_eq!(points.after().storage(), retained.storage());
            assert_eq!(points.after().active_len(), 2);
            assert_eq!(points.actual_end(), [-400f32, -200., 0.].map(f32::to_bits));
            assert_eq!(
                result
                    .point_dependency()
                    .request()
                    .configuration
                    .previous_path_type,
                previous
            );
            assert_eq!(world.checkpoint().unwrap(), before);
        }
    }
}
#[test]
fn normalization_home_and_changed_frozen_configuration_are_unavailable() {
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context, navigation, lineage) = setup(&products, config, origin());
    let liquid = products.load_dry_liquid().unwrap().binding();
    for (normalize, ignore) in [(true, false), (false, true), (true, true)] {
        let mut bad = config;
        bad.normalize_z = normalize;
        bad.ignore_normalization = ignore;
        assert!(matches!(
            geometry_revision(liquid, navigation.binding(), bad),
            Err(Failure::UnsupportedQuery)
        ));
    }
    let provider = provider(&products, &world, &session, navigation, lineage, config);
    let mut home = context.clone();
    route(&mut home).request.kind = RouteKind::HomeForceDestination;
    assert!(matches!(
        provider.begin(&session, &home),
        Err(Failure::UnsupportedQuery)
    ));
    let bound = provider.begin(&session, &context).unwrap();
    let mut changed = request(&provider, &context);
    changed.snapshot.configuration.optimize_path = false;
    assert!(matches!(
        bound.query(changed, deadline()),
        Err(Failure::Binding)
    ));
    let mut changed = request(&provider, &context);
    route(&mut changed.context)
        .request
        .prior_cache
        .as_mut()
        .unwrap()
        .path_type = 4;
    assert!(matches!(
        bound.query(changed, deadline()),
        Err(Failure::Binding)
    ));
    let mut missing = session.clone();
    missing.participants = None;
    assert!(provider.begin(&missing, &context).is_err());
}
#[test]
fn terminal_budget_deadline_and_all_ticket_overflow_reject_without_publication() {
    let products = products::authored_route_smoothing();
    for empty in [false, true] {
        let mut config = configuration();
        if empty {
            config.budget.postprocess_primitives = 0;
        }
        let (world, session, context, navigation, lineage) = setup(&products, config, origin());
        let before = world.checkpoint().unwrap();
        let provider = provider(&products, &world, &session, navigation, lineage, config);
        let bound = provider.begin(&session, &context).unwrap();
        let query = request(&provider, &context);
        assert!(bound.query(query.clone(), Instant::now()).is_err());
        let mut overflow = query.clone();
        overflow.first_ticket = u64::MAX - 2;
        assert!(matches!(
            bound.query(overflow, deadline()),
            Err(Failure::Invalid)
        ));
        if empty {
            assert!(bound.query(query, deadline()).is_err());
        }
        assert_eq!(world.checkpoint().unwrap(), before);
    }
}
