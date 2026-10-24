// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual warm straight geometry; initial cache inputs are authored fixtures.
use super::super::products::navigation::SourceSmoothRequest;
use super::super::{
    core::{Body, NonRenderParticipants, PlayerMotion},
    products::{self, InventoriedProducts},
};
use super::*;
use benilla_native_nav::{SourceOwnerFilter, SourceSmoothingBudget};
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world_runtime::native::{
    synthetic_creature_model_fixture as model, synthetic_fixture as f,
    synthetic_motion_fixture as fixture, CreatureCollisionWidthRows, CreatureCollisionWidthSource,
    NativeWorld,
};
use std::{collections::BTreeMap, time::Duration};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn configuration() -> StraightPathConfiguration {
    StraightPathConfiguration {
        warm: WarmCacheConfiguration {
            budget: benilla_native_nav::SourceWarmCacheBudget::default(),
        },
        optimize_path: true,
        normalize_z: false,
        ignore_normalization: false,
        budget: SourceStraightPathBudget::default(),
        fallback_budget: SourcePointPathBudget::default(),
    }
}

fn origin() -> ai::Point {
    ai::Point::new(-400., -400., 0.).unwrap()
}
fn setup_seed(
    products: &InventoriedProducts,
    config: StraightPathConfiguration,
    position: ai::Point,
    seed_from: ai::Point,
    seed_to: ai::Point,
    raw_width: f32,
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
    let mut profile = model::profile();
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
    profile
        .initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(vec![
            CreatureCollisionWidthSource::from_initial_model(
                2,
                model::selection(),
                &model::catalog_with_width(raw_width),
            )
            .unwrap(),
        ]))
        .unwrap();
    // Explicit authored control rows feed the real runtime bit receipt. Preserve
    // the existing movement fixture while adding its required player initialization.
    let initialized = f::initialized_profile();
    profile.npcs[0].combat = initialized.npcs[0].combat.clone();
    if let benilla_world_runtime::native::AbilityAdmission::Available(spec) =
        &mut profile.abilities[0].admission
    {
        spec.spell_rank = 1;
    }
    profile
        .initialize_player(&f::initialized_starter_input(), f::float(100.), 1)
        .unwrap();
    profile
        .initialize_interactions(
            benilla_world_runtime::native::synthetic_combat_fixture::source_rows(&profile),
        )
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
        participants: Some(NonRenderParticipants::authored_fixture(
            binding.content,
            binding.map,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        source_location: None,
        player_environment: None,
        same_polygon_route: None,
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
                from: [seed_from.x.bits(), seed_from.y.bits(), seed_from.z.bits()],
                to: [seed_to.x.bits(), seed_to.y.bits(), seed_to.z.bits()],
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
    config: StraightPathConfiguration,
) -> SourceRouteStraightPathProvider {
    SourceRouteStraightPathProvider::bind(
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
    provider: &SourceRouteStraightPathProvider,
    context: &AuthorityRequest,
) -> StraightPathRequest {
    StraightPathRequest {
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
fn setup(
    products: &InventoriedProducts,
    config: StraightPathConfiguration,
    position: ai::Point,
) -> (
    NativeWorld,
    Session,
    AuthorityRequest,
    NavigationSnapshot,
    WarmCacheLineage,
) {
    setup_seed(
        products,
        config,
        position,
        origin(),
        ai::Point::new(-400., -200., 0.).unwrap(),
        1.375,
    )
}
fn snake_point(bits: [u32; 3]) -> ai::Point {
    ai::Point::new(
        f32::from_bits(bits[0]),
        f32::from_bits(bits[1]),
        f32::from_bits(bits[2]),
    )
    .unwrap()
}
fn snake_setup(
    config: StraightPathConfiguration,
    raw_width: f32,
) -> (
    InventoriedProducts,
    NativeWorld,
    Session,
    AuthorityRequest,
    NavigationSnapshot,
    WarmCacheLineage,
) {
    let products = super::super::products::navigation::tests::straight_ground_products();
    let start = snake_point([0xc3a4aaaa, 0xc3d82aaa, 0x3d800000]);
    let end = snake_point([0xc39caaaa, 0xc3cd2aaa, 0x3fb80000]);
    let (world, session, mut context, navigation, lineage) =
        setup_seed(&products, config, start, end, end, raw_width);
    route(&mut context).request.goal = end;
    (products, world, session, context, navigation, lineage)
}
#[test]
fn owned_width_bits_and_actual_same_polygon_straight_do_not_publish_or_retry() {
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context, navigation, lineage) = setup(&products, config, origin());
    let before = world.checkpoint().unwrap();
    let old_cache = lineage.cache().clone();
    let provider = provider(&products, &world, &session, navigation, lineage, config);
    let bound = provider.begin(&session, &context).unwrap();
    let query = request(&provider, &context);
    let result = bound.query(query.clone(), deadline()).unwrap();
    assert_eq!(result, bound.query(query.clone(), deadline()).unwrap());
    assert_eq!(result.request(), &query);
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        unreachable!()
    };
    let expected = route
        .require_collision_width(world.profile())
        .unwrap()
        .width()
        .bits();
    let receipt = route
        .require_owner_ignore_pathfinding(world.profile())
        .unwrap();
    assert_eq!(result.owner_ignore_pathfinding_dependency(), receipt);
    assert_eq!(receipt.npc(), route.owner.npc());
    assert!(!receipt.ignore_pathfinding());
    let straight = result.straight_dependency();
    assert_eq!(straight.request().collision_width_bits, expected);
    assert_eq!(straight.result().collision_width_bits(), expected);
    assert_eq!(straight.request().ticket, 44);
    assert_eq!(straight.result().path_type(), 1);
    assert!(straight.result().ray().is_none());
    assert_eq!(
        straight.result().points(),
        &[straight.request().from, straight.request().to]
    );
    assert_eq!(straight.result().after().storage(), old_cache.storage());
    assert!(result.fallback_dependency().is_none());
    let tiles = result.tile_dependency();
    assert_eq!(tiles.request().ticket, 41);
    assert!(tiles.result().both_present());
    assert_eq!(tiles.request().from, straight.request().from);
    assert_eq!(tiles.request().to, straight.request().to);
    let filter = result.filter_dependency();
    assert_eq!(filter.request().context, context);
    assert_eq!(filter.owner_position(), straight.request().from);
    assert_eq!(filter.filter(), old_cache.filter());
    assert_eq!(filter.continued_from_ticket(), None);
    assert_eq!(
        filter.liquid_observations().map(|o| o.reply.request.ticket),
        [42, 43]
    );
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn clipped_and_no_path_terminals_retry_from_full_terminal_cache_type_and_original_goal() {
    for (width, expected) in [(0.1, 4), (0.4, 8)] {
        let config = configuration();
        let (products, world, session, context, navigation, lineage) = snake_setup(config, width);
        let before = world.checkpoint().unwrap();
        let original = lineage.cache().clone();
        let provider = provider(&products, &world, &session, navigation, lineage, config);
        let result = provider
            .begin(&session, &context)
            .unwrap()
            .query(request(&provider, &context), deadline())
            .unwrap();
        let straight = result.straight_dependency();
        assert_eq!(straight.result().path_type(), expected);
        assert_eq!(straight.result().ray().unwrap().hit_bits, 0.25f32.to_bits());
        if expected == 4 {
            assert_ne!(straight.result().end(), straight.request().to);
        }
        let fallback = result.fallback_dependency().unwrap();
        assert_eq!(fallback.before_cache(), straight.result().after());
        assert_ne!(fallback.before_cache(), &original);
        assert_eq!(
            fallback.filter_dependency().filter(),
            straight.result().after().filter()
        );
        assert_eq!(
            fallback.filter_dependency().continued_from_ticket(),
            Some(42)
        );
        assert_eq!(
            fallback
                .filter_dependency()
                .liquid_observations()
                .map(|o| o.reply.request.ticket),
            [46, 47]
        );
        let warm = fallback.polygon_dependency();
        assert_eq!(warm.request().ticket, 48);
        assert_eq!(warm.request().from, straight.request().from);
        assert_eq!(warm.request().to, straight.request().to);
        let smooth = fallback.point_dependency();
        assert_eq!(smooth.request().ticket, 49);
        assert_eq!(smooth.request().configuration.previous_path_type, expected);
        assert_eq!(
            smooth.result().path_type(),
            if expected == 4 { 4 } else { 1 }
        );
        assert!(smooth.result().smooth_status().is_some());
        assert_eq!(world.checkpoint().unwrap(), before);
    }
}

#[test]
fn short_without_no_path_or_incomplete_does_not_enter_retry() {
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, mut context, navigation, lineage) = setup(&products, config, origin());
    route(&mut context).request.goal = ai::Point::new(-400., -108., 0.).unwrap();
    let provider = provider(&products, &world, &session, navigation, lineage, config);
    let result = provider
        .begin(&session, &context)
        .unwrap()
        .query(request(&provider, &context), deadline())
        .unwrap();
    assert_eq!(result.straight_dependency().result().path_type(), 32);
    assert_eq!(
        result.straight_dependency().result().after().active_len(),
        0
    );
    assert!(result.fallback_dependency().is_none());
}

#[test]
fn missing_forged_generation_width_and_cache_or_context_edits_are_rejected() {
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context, navigation, lineage) = setup(&products, config, origin());
    let before = world.checkpoint().unwrap();
    let provider = provider(&products, &world, &session, navigation, lineage, config);
    for edit in 0..14 {
        let mut changed = context.clone();
        let q = route(&mut changed);
        match edit {
            0 => q.collision_width = None,
            1..=3 => {
                let mut width = serde_json::to_value(&q.collision_width).unwrap();
                match edit {
                    1 => width["width"] = serde_json::json!(2.0f32.to_bits()),
                    2 => width["npc"]["generation"] = serde_json::json!(1),
                    _ => width["native_display_id"] = serde_json::json!(31),
                }
                q.collision_width = serde_json::from_value(width).unwrap();
            }
            4 => q.request.prior_cache = None,
            5 => q.request.prior_cache.as_mut().unwrap().binding.source_epoch[0] ^= 1,
            6 => q.request.prior_cache.as_mut().unwrap().binding.inventory[0] ^= 1,
            7 => {
                q.request
                    .prior_cache
                    .as_mut()
                    .unwrap()
                    .binding
                    .navigation_policy[0] ^= 1
            }
            8 => q.request.prior_cache.as_mut().unwrap().poly_refs[73] = 42,
            9 => q.request.prior_cache.as_mut().unwrap().revision = 0,
            10 => q.owner_position = ai::Point::new(-400., -399., 0.).unwrap(),
            11 => q.request.kind = RouteKind::HomeForceDestination,
            12 => {
                q.request
                    .prior_cache
                    .as_mut()
                    .unwrap()
                    .binding
                    .owner_generation += 1
            }
            _ => changed.binding.tick += 1,
        }
        assert!(provider.begin(&session, &changed).is_err(), "edit {edit}");
    }
    let bound = provider.begin(&session, &context).unwrap();
    for edit in 0..8 {
        let mut query = request(&provider, &context);
        match edit {
            0 => query.snapshot.navigation.map += 1,
            1 => query.snapshot.navigation.source_epoch[0] ^= 1,
            2 => query.snapshot.liquid.inventory[0] ^= 1,
            3 => query.snapshot.configuration.optimize_path = false,
            4 => query.first_ticket = 0,
            5 => query.first_ticket = u64::MAX - 7,
            6 => {
                route(&mut query.context)
                    .request
                    .prior_cache
                    .as_mut()
                    .unwrap()
                    .revision += 1
            }
            _ => route(&mut query.context).request.goal = origin(),
        }
        assert!(bound.query(query, deadline()).is_err());
    }
    let mut missing = session.clone();
    missing.participants = None;
    assert!(provider.begin(&missing, &context).is_err());
    assert!(bound
        .query(request(&provider, &context), Instant::now())
        .is_err());
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn exact_loaded_budget_failure_never_becomes_source_no_path_or_successful_retry() {
    for stage in 0..4 {
        let mut config = configuration();
        match stage {
            0 => config.budget.native_primitives = 0,
            1 => config.budget.raycast_polygons = 0,
            2 => config.warm.budget.lookup_primitives = 0,
            _ => config.fallback_budget.postprocess_primitives = 0,
        }
        let (products, world, session, context, navigation, lineage) = snake_setup(config, 0.1);
        let before = world.checkpoint().unwrap();
        let provider = provider(&products, &world, &session, navigation, lineage, config);
        assert_eq!(
            provider
                .begin(&session, &context)
                .unwrap()
                .query(request(&provider, &context), deadline()),
            Err(Failure::MissingGeometry)
        );
        assert_eq!(world.checkpoint().unwrap(), before);
    }
}

#[test]
fn normalization_and_each_frozen_budget_affect_binding_or_reject() {
    let products = products::authored_route_smoothing();
    let liquid = products.load_dry_liquid().unwrap().binding();
    let nav = products.load_navigation().unwrap().binding();
    let config = configuration();
    let original = geometry_revision(liquid, nav, config).unwrap();
    for edit in 0..10 {
        let mut changed = config;
        match edit {
            0 => changed.optimize_path = false,
            1 => changed.budget.lookup_primitives -= 1,
            2 => changed.budget.raycast_polygons -= 1,
            3 => changed.budget.sampled_points -= 1,
            4 => changed.budget.native_primitives -= 1,
            5 => changed.fallback_budget.postprocess_primitives -= 1,
            6 => changed.warm.budget.corridor_expansions -= 1,
            7 => changed.fallback_budget.smoothing_primitives -= 1,
            8 => changed.normalize_z = true,
            _ => changed.ignore_normalization = true,
        }
        if edit < 8 {
            assert_ne!(geometry_revision(liquid, nav, changed).unwrap(), original);
        } else {
            assert_eq!(
                geometry_revision(liquid, nav, changed),
                Err(Failure::UnsupportedQuery)
            );
        }
    }
}

#[test]
fn typed_retry_rejects_clipped_goal_wrong_tickets_and_pre_straight_type() {
    use super::super::products::navigation::{SourcePointError, SourceWarmError};
    let config = configuration();
    let (products, world, session, context, navigation, lineage) = snake_setup(config, 0.1);
    let provider = provider(&products, &world, &session, navigation, lineage, config);
    let bound = provider.begin(&session, &context).unwrap();
    let result = bound
        .query(request(&provider, &context), deadline())
        .unwrap();
    let straight = result.straight_dependency();
    let fallback = result.fallback_dependency().unwrap();
    for edit in 0..5 {
        let mut warm = fallback.polygon_dependency().request();
        match edit {
            0 => warm.to = straight.result().end(),
            1 => warm.from = warm.to,
            2 => warm.ticket -= 1,
            3 => warm.snapshot.source_epoch[0] ^= 1,
            _ => warm.snapshot.inventory[0] ^= 1,
        }
        assert_eq!(
            provider
                .navigation
                .source_straight_warm(straight, warm, deadline()),
            Err(SourceWarmError::Identity)
        );
    }
    let mut points = fallback.point_dependency().request();
    points.configuration.previous_path_type = straight.request().configuration.previous_path_type;
    assert_eq!(
        provider
            .navigation
            .source_straight_point_path(&fallback.polygons, points, deadline()),
        Err(SourcePointError::Identity)
    );
    for ticket in [42, 43, 44, 45, 47] {
        let request = RouteFilterRequest {
            snapshot: provider.binding().liquid,
            context: context.clone(),
            first_ticket: ticket,
        };
        assert_eq!(
            bound.filter.query_retained_update(
                request,
                straight.result().after().filter(),
                Some(result.filter_dependency()),
                deadline(),
            ),
            Err(Failure::Binding)
        );
    }
}

#[test]
fn source_no_path_that_clears_active_without_tail_keeps_cold_retry_unavailable() {
    use super::super::products::navigation::SourceWarmError;
    let products = products::authored_route_smoothing();
    let config = configuration();
    // The active scan stops when squared distance is <1. Across that strict
    // threshold near the -266.66666 seam, these sub-0.1 endpoints select different
    // polygons. BuildPointPath's empty-points shortcut clears only active length.
    let start = ai::Point::new(-400., -265.7, 0.).unwrap();
    let (world, session, mut context, navigation, lineage) = setup(&products, config, start);
    let q = route(&mut context);
    q.request.goal = ai::Point::new(-400., -265.65, 0.).unwrap();
    let width = q
        .require_collision_width(world.profile())
        .unwrap()
        .width()
        .bits();
    let from = [
        q.request.origin.x.bits(),
        q.request.origin.y.bits(),
        q.request.origin.z.bits(),
    ];
    let to = [
        q.request.goal.x.bits(),
        q.request.goal.y.bits(),
        q.request.goal.z.bits(),
    ];
    let before = world.checkpoint().unwrap();
    let provider = provider(&products, &world, &session, navigation, lineage, config);
    let straight = provider
        .navigation
        .source_straight_path(
            &provider.lineage,
            SourceStraightRequest {
                snapshot: provider.binding().navigation,
                ticket: 44,
                from,
                to,
                collision_width_bits: width,
                configuration: SourcePointPathConfig {
                    optimize_path: config.optimize_path,
                    normalize_z: false,
                    ignore_normalization: false,
                    force_destination: false,
                    previous_path_type: 1,
                },
                budget: config.budget,
            },
            deadline(),
        )
        .unwrap();
    assert_eq!(straight.result().path_type(), 8);
    assert_eq!(straight.result().after().active_len(), 0);
    assert_eq!(straight.result().after().storage().len(), 74);
    assert_eq!(
        provider.navigation.source_straight_warm(
            &straight,
            SourceWarmRequest {
                snapshot: provider.binding().navigation,
                ticket: 48,
                from,
                to,
                budget: config.warm.budget,
            },
            deadline()
        ),
        Err(SourceWarmError::Native(
            benilla_native_nav::SourceWarmCacheError::ColdDistanceUndefined
        ))
    );
    assert_eq!(
        provider
            .begin(&session, &context)
            .unwrap()
            .query(request(&provider, &context), deadline()),
        Err(Failure::MissingGeometry)
    );
    assert_eq!(world.checkpoint().unwrap(), before);
}

fn observed_ticket(observation: Observation<'_>) -> (&'static str, u64) {
    match observation {
        Observation::OwnerIgnorePathfinding(receipt, ticket) => {
            assert!(!receipt.ignore_pathfinding());
            assert_eq!(receipt.projected_unit_state(), 0);
            ("owner-ignore-pathfinding", ticket)
        }
        Observation::Tiles(reply) => ("tiles", reply.request().ticket),
        Observation::Filter(reply) => ("filter", reply.request().first_ticket),
        Observation::Straight(reply) => ("straight", reply.request().ticket),
        Observation::Warm(reply) => ("warm", reply.request().ticket),
        Observation::Points(reply) => ("points", reply.request().ticket),
    }
}

#[test]
fn actual_tile_pairs_precede_each_filter_and_retry_keeps_original_unsnapped_goal() {
    let config = configuration();
    let (products, world, session, context, navigation, lineage) = snake_setup(config, 0.1);
    let provider = provider(&products, &world, &session, navigation, lineage, config);
    let bound = provider.begin(&session, &context).unwrap();
    let mut observed = vec![];
    let result = bound
        .query_observed(request(&provider, &context), deadline(), |o| {
            observed.push(observed_ticket(o));
        })
        .unwrap();
    assert_eq!(
        observed,
        [
            ("owner-ignore-pathfinding", 41),
            ("tiles", 41),
            ("filter", 42),
            ("straight", 44),
            ("owner-ignore-pathfinding", 45),
            ("tiles", 45),
            ("filter", 46),
            ("warm", 48),
            ("points", 49),
        ]
    );
    let receipt = result.owner_ignore_pathfinding_dependency();
    assert_eq!(
        receipt,
        result
            .fallback_dependency()
            .unwrap()
            .owner_ignore_pathfinding_dependency()
    );
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        unreachable!()
    };
    assert_eq!(
        receipt,
        route
            .require_owner_ignore_pathfinding(world.profile())
            .unwrap()
    );
    let first = result.tile_dependency();
    let second = result.fallback_dependency().unwrap().tile_dependency();
    assert_eq!(first.request().ticket, 41);
    assert_eq!(second.request().ticket, 45);
    assert_eq!(first.result(), second.result());
    let straight = result.straight_dependency();
    for tiles in [first, second] {
        assert!(tiles.result().both_present());
        assert_eq!(tiles.request().from, straight.request().from);
        assert_eq!(tiles.request().to, straight.request().to);
        assert_eq!(tiles.result().from(), straight.request().from);
        assert_eq!(tiles.result().to(), straight.request().to);
        assert!(tiles.result().start().present());
        assert!(tiles.result().end().unwrap().present());
        assert_ne!(tiles.request().to, straight.result().end());
    }
}

#[test]
fn loaded_missing_endpoint_short_circuits_before_filters_despite_nearby_polygon() {
    use super::super::products::navigation::RouteRequest;
    let products = products::authored_route_smoothing();
    let config = configuration();
    let outside = ai::Point::new(-400., -534., 0.).unwrap();
    let inside = ai::Point::new(-400., -530., 0.).unwrap();
    let bits = |p: ai::Point| [p.x.bits(), p.y.bits(), p.z.bits()];
    for (from, to, missing_start) in [(outside, inside, true), (inside, outside, false)] {
        let (world, session, mut context, navigation, lineage) = setup(&products, config, from);
        route(&mut context).request.goal = to;
        let nearby = navigation
            .query(
                RouteRequest {
                    snapshot: navigation.binding(),
                    ticket: 31,
                    from: bits(from),
                    to: bits(to),
                },
                deadline(),
            )
            .unwrap();
        assert_ne!(
            if missing_start {
                nearby.route.start.point
            } else {
                nearby.route.end.point
            },
            bits(outside),
            "a snapped polygon lookup is not the source HaveTile predicate"
        );
        let before = world.checkpoint().unwrap();
        let provider = provider(&products, &world, &session, navigation, lineage, config);
        let bound = provider.begin(&session, &context).unwrap();
        let mut observed = vec![];
        assert_eq!(
            bound.query_observed(request(&provider, &context), deadline(), |o| {
                if let Observation::Tiles(reply) = &o {
                    assert!(!reply.result().both_present());
                    assert_eq!(reply.result().start().present(), !missing_start);
                    if missing_start {
                        assert_eq!(reply.result().start().coordinates(), [-1, 0]);
                        assert!(reply.result().end().is_none());
                    } else {
                        let end = reply.result().end().unwrap();
                        assert_eq!(end.coordinates(), [-1, 0]);
                        assert!(!end.present());
                    }
                }
                observed.push(observed_ticket(o));
            }),
            Err(Failure::MissingGeometry)
        );
        assert_eq!(observed, [("owner-ignore-pathfinding", 41), ("tiles", 41)]);
        assert_eq!(world.checkpoint().unwrap(), before);
    }
}

#[test]
fn expired_tile_prefix_never_reaches_its_filter_including_fresh_retry() {
    for expire_ticket in [41, 45] {
        let config = configuration();
        let (products, world, session, context, navigation, lineage) = snake_setup(config, 0.1);
        let before = world.checkpoint().unwrap();
        let provider = provider(&products, &world, &session, navigation, lineage, config);
        let bound = provider.begin(&session, &context).unwrap();
        let expires = Instant::now() + Duration::from_millis(30);
        let mut observed = vec![];
        assert_eq!(
            bound.query_observed(request(&provider, &context), expires, |o| {
                let event = observed_ticket(o);
                observed.push(event);
                if event == ("tiles", expire_ticket) {
                    std::thread::sleep(
                        expires.saturating_duration_since(Instant::now())
                            + Duration::from_millis(1),
                    );
                }
            }),
            Err(Failure::MissingGeometry)
        );
        let expected = [
            ("owner-ignore-pathfinding", 41),
            ("tiles", 41),
            ("filter", 42),
            ("straight", 44),
            ("owner-ignore-pathfinding", 45),
            ("tiles", 45),
        ];
        assert_eq!(
            observed,
            expected[..if expire_ticket == 41 { 2 } else { 6 }]
        );
        assert_eq!(world.checkpoint().unwrap(), before);
    }
}

#[test]
fn source_invalid_coordinates_and_full_ticket_reservation_precede_observations() {
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context, navigation, lineage) = setup(&products, config, origin());
    let provider = provider(&products, &world, &session, navigation, lineage, config);
    for position in [origin(), ai::Point::new(17_067., 0., 0.).unwrap()] {
        let mut changed = context.clone();
        route(&mut changed).request.goal = position;
        let bound = provider.begin(&session, &changed).unwrap();
        let mut query = request(&provider, &changed);
        if position == origin() {
            query.first_ticket = u64::MAX - 7;
        }
        let mut observed = vec![];
        assert_eq!(
            bound.query_observed(query, deadline(), |o| observed.push(observed_ticket(o))),
            Err(Failure::Invalid)
        );
        assert!(observed.is_empty());
    }
    // Full reservation ending exactly at MAX is legal, even without a retry.
    let bound = provider.begin(&session, &context).unwrap();
    let mut query = request(&provider, &context);
    query.first_ticket = u64::MAX - 8;
    let result = bound.query(query, deadline()).unwrap();
    assert_eq!(result.tile_dependency().request().ticket, u64::MAX - 8);
    assert_eq!(result.straight_dependency().request().ticket, u64::MAX - 5);
}

#[test]
fn owned_ignore_pathfinding_missing_or_edited_receipt_stops_before_tiles_or_filter() {
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context, navigation, lineage) = setup(&products, config, origin());
    let before = world.checkpoint().unwrap();
    let provider = provider(&products, &world, &session, navigation, lineage, config);
    for edit in 0..8 {
        let mut changed = context.clone();
        let q = route(&mut changed);
        if edit == 0 {
            q.owner_ignore_pathfinding = None;
        } else {
            let mut value =
                serde_json::to_value(q.owner_ignore_pathfinding.as_ref().unwrap()).unwrap();
            match edit {
                1 => value["npc"]["generation"] = (q.owner.npc().generation + 1).into(),
                2..=4 => {
                    let field =
                        ["profile_binding", "source_binding", "capability_binding"][edit - 2];
                    value[field][0] = (value[field][0].as_u64().unwrap() ^ 1).into();
                }
                5 => value["policy"] = "forged-owner-policy".into(),
                6 => {
                    value["projected_unit_state"] =
                        benilla_world_runtime::native::UNIT_STAT_IGNORE_PATHFINDING.into()
                }
                _ => value["live_abilities"][0]["id"] = 60009.into(),
            }
            q.owner_ignore_pathfinding = Some(serde_json::from_value(value).unwrap());
        }
        let bound = provider.begin(&session, &changed).unwrap();
        let mut observed = vec![];
        assert_eq!(
            bound.query_observed(request(&provider, &changed), deadline(), |o| observed
                .push(observed_ticket(o))),
            Err(if edit == 0 {
                Failure::MissingGeometry
            } else {
                Failure::Binding
            }),
            "edit {edit}"
        );
        assert!(observed.is_empty(), "edit {edit}");
    }
    // Source coordinate rejection still wins when the owner receipt is missing.
    let mut changed = context.clone();
    let q = route(&mut changed);
    q.owner_ignore_pathfinding = None;
    q.request.goal = ai::Point::new(17_067., 0., 0.).unwrap();
    let mut observed = vec![];
    assert_eq!(
        provider.begin(&session, &changed).unwrap().query_observed(
            request(&provider, &changed),
            deadline(),
            |o| observed.push(observed_ticket(o))
        ),
        Err(Failure::Invalid)
    );
    assert!(observed.is_empty());
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn expiry_after_owned_bit_validation_stops_each_attempt_before_have_tile() {
    for expire_ticket in [41, 45] {
        let config = configuration();
        let (products, world, session, context, navigation, lineage) = snake_setup(config, 0.1);
        let before = world.checkpoint().unwrap();
        let provider = provider(&products, &world, &session, navigation, lineage, config);
        let bound = provider.begin(&session, &context).unwrap();
        let expires = Instant::now() + Duration::from_millis(30);
        let mut observed = vec![];
        assert_eq!(
            bound.query_observed(request(&provider, &context), expires, |o| {
                let event = observed_ticket(o);
                observed.push(event);
                if event == ("owner-ignore-pathfinding", expire_ticket) {
                    std::thread::sleep(
                        expires.saturating_duration_since(Instant::now())
                            + Duration::from_millis(1),
                    );
                }
            }),
            Err(Failure::MissingGeometry)
        );
        let expected = [
            ("owner-ignore-pathfinding", 41),
            ("tiles", 41),
            ("filter", 42),
            ("straight", 44),
            ("owner-ignore-pathfinding", 45),
        ];
        assert_eq!(
            observed,
            expected[..if expire_ticket == 41 { 1 } else { 5 }]
        );
        assert_eq!(world.checkpoint().unwrap(), before);
    }
}
