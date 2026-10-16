// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded queries and genuine generation-zero model/player-cache witnesses. The
//! initial warm cache is authored; controller tests exercise staged cache ownership.
use super::super::{
    core::{Body, NonRenderParticipants, PlayerMotion},
    products::{dry_route::tests as fixtures, navigation::SourceSmoothRequest},
    source_route_warm_cache::WarmCacheConfiguration,
};
use super::*;
use benilla_native_nav::{
    SourceOwnerFilter, SourcePointPathBudget, SourceSmoothingBudget, SourceStraightPathBudget,
    SourceWarmCacheBudget,
};
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world_runtime::native::{
    synthetic_creature_model_fixture as models, synthetic_fixture as f,
    synthetic_motion_fixture as motion_fixture, synthetic_player_model_fixture as player_models,
    CreatureCollisionWidthRows, CreatureCollisionWidthSource, NativeWorld,
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn point(x: f32, y: f32, z: f32) -> ai::Point {
    ai::Point::new(x, y, z).unwrap()
}
fn bits(p: ai::Point) -> [u32; 3] {
    [p.x.bits(), p.y.bits(), p.z.bits()]
}
fn config() -> ChaseGateConfiguration {
    ChaseGateConfiguration {
        straight: StraightPathConfiguration {
            warm: WarmCacheConfiguration {
                budget: SourceWarmCacheBudget::default(),
            },
            optimize_path: true,
            normalize_z: false,
            ignore_normalization: false,
            budget: SourceStraightPathBudget::default(),
            fallback_budget: SourcePointPathBudget::default(),
        },
        line_of_sight_enabled: true,
    }
}
fn route(
    context: &mut AuthorityRequest,
) -> &mut benilla_world_runtime::native::CreatureRouteRequest {
    let Query::SourceMotion(MotionQuery::Route(route)) = &mut context.query else {
        panic!("route");
    };
    route
}
fn setup(
    position: ai::Point,
) -> (
    NativeWorld,
    Session,
    AuthorityRequest,
    SourceRouteChaseGateProvider,
) {
    let products = fixtures::products();
    let environment = fixtures::environment(Arc::clone(&products));
    let projection = products.load_dry_route_projection().unwrap();
    let (liquid, nav, vmap) = (
        projection.liquid(),
        projection.navigation(),
        projection.static_vmap(),
    );
    let mut profile = player_models::unowned_profile();
    profile.content_digest = liquid.binding().content;
    profile.map_id = liquid.binding().map;
    profile.geometry_revision =
        geometry_revision(liquid.binding(), nav.binding(), vmap.binding(), config()).unwrap();
    profile.npcs[0].ai.map_id = profile.map_id;
    profile.npcs[0].ai.content_revision = ai::movement::Revision(profile.content_digest);
    profile.npcs[0].ai.spawn = position;
    profile.npcs[0].display_id = 34;
    profile.player.start = point(
        position.x.as_f32(),
        position.y.as_f32() + 16.,
        position.z.as_f32(),
    );
    profile
        .initialize_creature_capabilities(motion_fixture::creature_capability_sources(&profile))
        .unwrap();
    let catalog = models::catalog_with_height_fields(1.375, 1.6, 2.5, 1.75, false);
    profile
        .initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(vec![
            CreatureCollisionWidthSource::from_initial_model(2, models::selection(), &catalog)
                .unwrap(),
        ]))
        .unwrap();
    profile.initialize_creature_collision_heights().unwrap();
    profile
        .initialize_player_collision_height(&player_models::catalog(), &catalog)
        .unwrap();
    profile
        .initialize_player_environment(
            environment.environment_binding(),
            environment.environment_catalog().clone(),
        )
        .unwrap();
    let source = profile.source_motion.as_ref().unwrap();
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
                height: source.npc_policy.settings.collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: profile.player.bounding_radius,
                height: source.player_collision_height,
            },
            speed: ai::Real::new(7.).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            profile.content_digest,
            profile.map_id,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        player_environment: Some(
            PlayerEnvironmentPolicy::from_profile(&profile, f::player()).unwrap(),
        ),
    };
    let mut world =
        NativeWorld::synthetic_component_test(profile.clone(), session.world, f::player(), 1234)
            .unwrap();
    let mut capture = motion_fixture::SyntheticMotionAuthority::default();
    for tick in 1..=2 {
        world.step(tick, None, &mut capture).unwrap();
    }
    let mut context = capture
        .queries
        .into_iter()
        .find(|q| matches!(&q.query, Query::SourceMotion(MotionQuery::Route(_))))
        .unwrap();
    let lineage = seed(&projection);
    let q = route(&mut context);
    let nb = nav.binding();
    q.request.prior_cache = Some(Box::new(motion::PathFinderCache {
        binding: motion::PathFinderCacheBinding {
            identity: q.request.binding.identity,
            owner_generation: q.owner.npc().generation,
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
        requested_end: q.request.goal,
        include_flags: 1,
        exclude_flags: 0,
    }));
    let provider = SourceRouteChaseGateProvider::bind(
        &session,
        &profile,
        f::player(),
        projection,
        lineage,
        config(),
    )
    .unwrap();
    (world, session, context, provider)
}
fn seed(projection: &DryRouteProjection) -> WarmCacheLineage {
    projection
        .navigation()
        .seed_bounded_warm_lineage(
            SourceSmoothRequest {
                snapshot: projection.navigation().binding(),
                ticket: 1,
                from: bits(point(-400., -430., 0.)),
                to: bits(point(-400., -200., 0.)),
                filter: SourceOwnerFilter {
                    include_flags: 1,
                    exclude_flags: 0,
                },
                point_limit: 74,
                budget: SourceSmoothingBudget::default(),
            },
            deadline(),
        )
        .unwrap()
}

fn request(
    provider: &SourceRouteChaseGateProvider,
    context: &AuthorityRequest,
) -> ChaseGateRequest {
    ChaseGateRequest {
        snapshot: provider.binding(),
        context: context.clone(),
        first_ticket: 41,
    }
}
#[test]
fn actual_dry_gate_orders_distinct_los_owner_cache_then_fresh_calculate_queries() {
    let (world, session, context, provider) = setup(point(-400., -430., 0.));
    let before = world.checkpoint().unwrap();
    let bound = provider.begin(&session, &context).unwrap();
    let mut observations = vec![];
    let query = request(&provider, &context);
    let result = bound
        .query_observed(query.clone(), deadline(), |o| observations.push(o))
        .unwrap();
    assert_eq!(result.request(), &query);
    assert_eq!(result.selection(), ChaseGateSelection::Straight);
    let los = result.line_of_sight().unwrap();
    let water = result.owner_water().unwrap();
    let target = result.target_water().unwrap();
    assert_eq!(
        observations,
        vec![
            Observation::LineOfSight(los),
            Observation::OwnerWater(water),
            Observation::TargetWater(false),
            Observation::Straight
        ]
    );
    assert_eq!(los.result, StaticRay::Miss);
    assert!(!los.request.ignore_m2);
    assert_eq!(los.request.ticket, 41);
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        unreachable!()
    };
    assert_ne!(
        route.request.goal,
        route.request.target.unwrap().pose.position
    );
    assert_eq!(
        los.request.from,
        [
            route.owner_position.x.bits(),
            route.owner_position.y.bits(),
            (route.owner_position.z.as_f32()
                + f32::from_bits(result.model().owner_height.height().bits()))
            .to_bits()
        ]
    );
    assert_eq!(
        los.request.to,
        [
            route.request.goal.x.bits(),
            route.request.goal.y.bits(),
            (route.request.goal.z.as_f32()
                + f32::from_bits(result.model().target_height.height().bits()))
            .to_bits()
        ]
    );
    assert_ne!(
        result.model().owner_height.height(),
        result.model().target_height.height()
    );
    assert_eq!(
        result.model().selection.combat_reach().bits(),
        world.profile().npcs[0].ai.combat_reach.bits()
    );
    assert_eq!(water.required_type, 15);
    assert_eq!(water.collision_height_bits, 0x4002_007e);
    assert_eq!(water.reply.request.ticket, 42);
    assert_eq!(water.reply.request.position, bits(route.owner_position));
    assert_eq!(water.reply.status, LiquidStatus::NoWater);
    assert_eq!(Some(target), route.target_player_water.as_ref());
    let straight = result.straight_dependency().unwrap();
    assert_eq!(straight.request().first_ticket, 43);
    assert_eq!(straight.tile_dependency().request().ticket, 43);
    assert!(straight.tile_dependency().result().both_present());
    assert_eq!(
        straight.tile_dependency().request().from,
        bits(route.request.origin)
    );
    assert_eq!(
        straight.tile_dependency().request().to,
        bits(route.request.goal)
    );
    assert_eq!(
        straight
            .filter_dependency()
            .liquid_observations()
            .map(|o| (o.mask.bits(), o.reply.request.ticket)),
        [(15, 44), (10, 45)]
    );
    assert_eq!(straight.straight_dependency().request().ticket, 46);
    assert_eq!(
        straight.straight_dependency().request().to,
        bits(route.request.goal)
    );
    assert_eq!(result, bound.query(query, deadline()).unwrap());
    assert_eq!(world.checkpoint().unwrap(), before);
}
#[test]
fn strict_numeric_short_circuit_leaves_geometry_and_cached_water_unobserved() {
    let (world, session, context, provider) = setup(point(-400., -430., 0.));
    for (goal, expected) in [
        (point(-198.5, -430., 0.), ChaseGateRejection::Distance),
        (point(-400., -430., 5.), ChaseGateRejection::Vertical),
    ] {
        let mut context = context.clone();
        route(&mut context).request.goal = goal;
        route(&mut context).target_player_water = None;
        let bound = provider.begin(&session, &context).unwrap();
        let mut observed = vec![];
        let result = bound
            .query_observed(request(&provider, &context), deadline(), |o| {
                observed.push(o)
            })
            .unwrap();
        assert_eq!(
            result.selection(),
            ChaseGateSelection::NormalRouteUnavailable(expected)
        );
        assert!(observed.is_empty());
        assert!(result.line_of_sight().is_none());
        assert!(result.owner_water().is_none());
        assert!(result.target_water().is_none());
        assert!(result.straight_dependency().is_none());
    }
    assert_eq!(world.state().tick, 2);
}
#[test]
fn actual_wall_and_m2_hits_choose_unavailable_normal_route_before_any_water() {
    for (owner, goal, m2) in [
        (point(-395., -400., 0.), point(-405., -400., 0.), false),
        (point(-420., -400., -1.), point(-420., -400., 1.), true),
    ] {
        let (_, session, mut context, provider) = setup(owner);
        route(&mut context).request.goal = goal;
        route(&mut context).target_player_water = None;
        let bound = provider.begin(&session, &context).unwrap();
        let mut observed = vec![];
        let result = bound
            .query_observed(request(&provider, &context), deadline(), |o| {
                observed.push(o)
            })
            .unwrap();
        let los = result.line_of_sight().unwrap();
        assert!(matches!(los.result, StaticRay::Hit { .. }));
        assert_eq!(
            result.selection(),
            ChaseGateSelection::NormalRouteUnavailable(ChaseGateRejection::LineOfSight)
        );
        assert_eq!(observed, vec![Observation::LineOfSight(los)]);
        assert!(result.owner_water().is_none());
        assert!(result.straight_dependency().is_none());
        if m2 {
            let spell_ray = provider
                .vmap
                .query(
                    RayRequest {
                        ignore_m2: true,
                        ..los.request
                    },
                    deadline(),
                )
                .unwrap();
            assert_eq!(
                spell_ray.result,
                StaticRay::Miss,
                "the spell/sight ignore-M2 policy cannot supply this gate"
            );
        }
    }
}
#[test]
fn missing_or_forged_owned_witness_and_non_chase_kind_cannot_begin() {
    let (_, session, context, provider) = setup(point(-400., -430., 0.));
    for edit in 0..11 {
        let mut changed = context.clone();
        let q = route(&mut changed);
        match edit {
            0 => q.collision_width = None,
            1 => q.collision_height = None,
            2 => q.target_player_height = None,
            3 => q.request.kind = RouteKind::Backpedal,
            4 => q.request.kind = RouteKind::HomeForceDestination,
            5 => q.request.prior_cache = None,
            6 => q.request.prior_cache.as_mut().unwrap().binding.source_epoch[0] ^= 1,
            7 => q.request.prior_cache.as_mut().unwrap().binding.inventory[0] ^= 1,
            8 => q.owner_position = point(-400., -429., 0.),
            9 => changed.binding.tick += 1,
            _ => {
                let mut json = serde_json::to_value(q.collision_width.as_ref().unwrap()).unwrap();
                json["source_binding"][0] = serde_json::json!(99);
                q.collision_width = Some(serde_json::from_value(json).unwrap());
            }
        }
        assert!(provider.begin(&session, &changed).is_err(), "edit {edit}");
    }
}
#[test]
fn target_water_is_required_only_after_real_owner_query_and_rejects_stale_witness() {
    let (_, session, context, provider) = setup(point(-400., -430., 0.));
    for edit in 0..5 {
        let mut context = context.clone();
        let q = route(&mut context);
        if edit == 0 {
            q.target_player_water = None;
        } else {
            let mut json = serde_json::to_value(q.target_player_water.as_ref().unwrap()).unwrap();
            match edit {
                1 => json["tick"] = serde_json::json!(77),
                2 => json["in_water"] = serde_json::json!(true),
                3 => json["snapshot"]["owner"]["source"][0] = serde_json::json!(99),
                _ => json["digest"][0] = serde_json::json!(99),
            }
            q.target_player_water = Some(serde_json::from_value(json).unwrap());
        }
        let bound = provider.begin(&session, &context).unwrap();
        let mut observed = vec![];
        assert!(bound
            .query_observed(request(&provider, &context), deadline(), |o| observed
                .push(o))
            .is_err());
        assert!(matches!(
            &observed[..],
            [Observation::LineOfSight(_), Observation::OwnerWater(_)]
        ));
    }
}
#[test]
fn complete_session_request_snapshot_and_deadline_are_bound() {
    let (_, session, context, provider) = setup(point(-400., -430., 0.));
    for edit in 0..5 {
        let mut session = session.clone();
        match edit {
            0 => session.player_environment = None,
            1 => session.world[0] ^= 1,
            2 => {
                let body = &mut session.player.as_mut().unwrap().body;
                body.height = ai::Real::new(body.height.as_f32() + 1.).unwrap();
            }
            3 => session.participants = None,
            _ => session.profile[0] ^= 1,
        }
        assert!(
            provider.begin(&session, &context).is_err(),
            "session edit {edit}"
        );
    }
    let bound = provider.begin(&session, &context).unwrap();
    for edit in 0..9 {
        let mut query = request(&provider, &context);
        match edit {
            0 => query.first_ticket = 0,
            1 => query.first_ticket = u64::MAX - 9,
            2 => query.snapshot.vmap.inventory[0] ^= 1,
            3 => query.snapshot.straight.liquid.source_epoch[0] ^= 1,
            4 => query.snapshot.straight.navigation.policy[0] ^= 1,
            5 => query.snapshot.configuration.line_of_sight_enabled = false,
            6 => route(&mut query.context).request.goal = point(-400., -420., 0.),
            7 => query.context.binding.world[0] ^= 1,
            _ => query.context.binding.npc.as_mut().unwrap().generation += 1,
        }
        assert!(bound.query(query, deadline()).is_err(), "edit {edit}");
    }
    assert_eq!(
        bound.query(request(&provider, &context), Instant::now()),
        Err(Failure::MissingGeometry)
    );
    let mut cfg = config();
    cfg.line_of_sight_enabled = false;
    let b = provider.binding();
    assert_eq!(
        geometry_revision(b.straight.liquid, b.straight.navigation, b.vmap, cfg),
        Err(Failure::UnsupportedQuery)
    );
    let mut vmap = b.vmap;
    vmap.inventory[0] ^= 1;
    assert_eq!(
        geometry_revision(b.straight.liquid, b.straight.navigation, vmap, config()),
        Err(Failure::Binding)
    );
}

#[test]
fn actual_unloaded_owner_liquid_and_post_ray_expiry_do_not_become_a_dry_gate() {
    let (_, session, mut context, provider) = setup(point(-400., -430., 0.));
    let q = route(&mut context);
    q.owner_position = point(-400., 0.1, 0.);
    q.request.origin = q.owner_position;
    q.request.launch_origin = q.owner_position;
    q.request.goal = point(-399., 0.1, 0.);
    let bound = provider.begin(&session, &context).unwrap();
    let mut observed = vec![];
    assert_eq!(
        bound.query_observed(request(&provider, &context), deadline(), |o| observed
            .push(o)),
        Err(Failure::MissingGeometry)
    );
    assert!(matches!(&observed[..], [Observation::LineOfSight(_)]));

    let (_, session, context, provider) = setup(point(-400., -430., 0.));
    let bound = provider.begin(&session, &context).unwrap();
    let expires = Instant::now() + Duration::from_millis(30);
    let mut observed = vec![];
    assert_eq!(
        bound.query_observed(request(&provider, &context), expires, |o| {
            observed.push(o);
            if matches!(o, Observation::LineOfSight(_)) {
                std::thread::sleep(
                    expires.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
                );
            }
        }),
        Err(Failure::MissingGeometry)
    );
    assert!(matches!(&observed[..], [Observation::LineOfSight(_)]));
}

#[test]
fn identical_inventory_does_not_transfer_a_lineage_to_a_reloaded_native_mesh() {
    let (world, session, context, original) = setup(point(-400., -430., 0.));
    let products = fixtures::products();
    let first = products.load_dry_route_projection().unwrap();
    let lineage = seed(&first);
    let reloaded = products.load_dry_route_projection().unwrap();
    assert_eq!(
        first.navigation().binding(),
        reloaded.navigation().binding()
    );
    assert!(matches!(
        SourceRouteChaseGateProvider::bind(
            &session,
            world.profile(),
            f::player(),
            reloaded,
            lineage,
            config(),
        ),
        Err(Failure::Binding)
    ));
    // The original, still-owned mesh is healthy; failure is not a global geometry veto.
    assert_eq!(
        original
            .begin(&session, &context)
            .unwrap()
            .query(request(&original, &context), deadline())
            .unwrap()
            .selection(),
        ChaseGateSelection::Straight
    );
}

#[test]
fn bind_rejects_actual_cross_inventory_or_epoch_and_missing_explicit_selections() {
    let (world, session, _, _) = setup(point(-400., -430., 0.));
    for edit in 0..5 {
        let (mut spec, files) = fixtures::fixture();
        let mut selected = session.clone();
        let mut player = f::player();
        let mut cfg = config();
        match edit {
            0 => spec.binding.effective_config[0] ^= 1,
            1 => spec.binding.source_epoch[0] ^= 1,
            2 => selected.player_environment = None,
            3 => player.character[0] ^= 1,
            _ => cfg.line_of_sight_enabled = false,
        }
        let products = fixtures::load(spec, &files);
        let projection = products.load_dry_route_projection().unwrap();
        let lineage = seed(&projection);
        let result = SourceRouteChaseGateProvider::bind(
            &selected,
            world.profile(),
            player,
            projection,
            lineage,
            cfg,
        );
        assert!(
            matches!(result, Err(Failure::Binding | Failure::UnsupportedQuery)),
            "edit {edit}"
        );
    }
}
