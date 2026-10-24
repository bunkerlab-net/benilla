// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual staged owner requests over one decoded terrain/VMap/MMap inventory.
//! Synthetic surrounding queries supply neither the filter nor smoothing answers.
use super::super::{
    core::{Body, NonRenderParticipants, PlayerMotion},
    products::{self, InventoriedProducts},
};
use super::*;
use benilla_native_nav::SourceSmoothStop;
use benilla_sim::{creature_ai as ai, creature_motion as motion, route_filter};
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture as fixture, NativeWorld,
};
use std::{collections::BTreeMap, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn configuration() -> SourceRouteSmoothingConfiguration {
    SourceRouteSmoothingConfiguration {
        stage: SourceRouteSmoothingStage::PreOptimizePathPreNormalizeZ,
        point_limit: 74,
        budget: SourceSmoothingBudget {
            corridor_expansions: 4096,
            smoothing_primitives: 256,
        },
    }
}
fn origin() -> ai::Point {
    ai::Point::new(-400., -400., 0.).unwrap()
}
fn setup(
    products: &InventoriedProducts,
    config: SourceRouteSmoothingConfiguration,
    position: ai::Point,
) -> (NativeWorld, Session, AuthorityRequest) {
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
    let session = session(&profile, binding);
    let mut world =
        NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234).unwrap();
    let mut authority = fixture::SyntheticMotionAuthority::default();
    for tick in 1..=2 {
        world.step(tick, None, &mut authority).unwrap();
    }
    let context = authority
        .queries
        .into_iter()
        .find(|q| matches!(q.query, Query::SourceMotion(MotionQuery::Route(_))))
        .unwrap();
    (world, session, context)
}
fn session(profile: &Profile, binding: super::super::products::liquid::SnapshotBinding) -> Session {
    let motion = profile.source_motion.as_ref().unwrap();
    Session {
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
    }
}

fn bind_provider(
    products: &InventoriedProducts,
    session: &Session,
    profile: &Profile,
    config: SourceRouteSmoothingConfiguration,
) -> SourceRouteSmoothingProvider {
    SourceRouteSmoothingProvider::bind(
        session,
        profile,
        products.load_dry_liquid().unwrap(),
        products.load_navigation().unwrap(),
        config,
    )
    .unwrap()
}
fn request(
    provider: &SourceRouteSmoothingProvider,
    context: &AuthorityRequest,
) -> RouteSmoothingRequest {
    RouteSmoothingRequest {
        snapshot: provider.binding(),
        context: context.clone(),
        first_ticket: 41,
    }
}

#[test]
fn actual_same_inventory_filter_then_smoothing_retains_all_inputs_without_publishing_route() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context) = setup(&products, config, origin());
    let before = world.checkpoint().unwrap();
    let provider = bind_provider(&products, &session, world.profile(), config);
    let bound = provider.begin(&session, &context).unwrap();
    let query = request(&provider, &context);
    let mut stages = Vec::new();
    let result = bound
        .query_observed(query.clone(), deadline(), |observation| match observation {
            QueryObservation::Filter(filter) => {
                assert_eq!(filter.request().context, context);
                stages.push("liquid-filter");
            }
            QueryObservation::Smoothing(smoothing) => {
                assert_eq!(smoothing.request.ticket, 43);
                stages.push("loaded-smoothing");
            }
        })
        .unwrap();
    assert_eq!(stages, ["liquid-filter", "loaded-smoothing"]);
    assert_eq!(result.request(), &query);
    let filter = result.filter_dependency();
    assert_eq!(filter.request().first_ticket, 41);
    let masks = [
        route_filter::LiquidMask::AllLiquids,
        route_filter::LiquidMask::WaterOrOcean,
    ];
    for (i, observation) in filter.liquid_observations().iter().enumerate() {
        assert_eq!(observation.mask, masks[i]);
        assert_eq!(observation.reply.request.snapshot, query.snapshot.liquid);
        assert_eq!(observation.reply.request.ticket, 41 + i as u64);
        assert_eq!(observation.reply.request.position, filter.owner_position());
    }
    assert_eq!(filter.filter().include_flags(), 1);
    assert_eq!(filter.filter().exclude_flags(), 0);
    let raw = result.smoothing_reply();
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        unreachable!()
    };
    let bits = |p: ai::Point| [p.x, p.y, p.z].map(|v| v.as_f32().to_bits());
    assert_eq!(raw.request.from, bits(route.request.origin));
    assert_eq!(raw.request.to, bits(route.request.goal));
    assert_eq!(
        raw.request.filter.include_flags,
        filter.filter().include_flags()
    );
    assert_eq!(
        raw.request.filter.exclude_flags,
        filter.filter().exclude_flags()
    );
    assert_eq!(raw.request.point_limit, config.point_limit);
    assert_eq!(raw.request.budget, config.budget);
    assert_eq!(raw.result.stop, SourceSmoothStop::TargetReached);
    assert!(!raw.result.polygons.is_empty());
    assert!(raw.result.points.len() >= 2);
    let standalone = products
        .load_navigation()
        .unwrap()
        .source_smooth(raw.request, deadline())
        .unwrap();
    assert_eq!(&standalone, raw);
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn composite_policy_requires_same_map_content_epoch_inventory_and_explicit_configuration() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, _) = setup(&products, config, origin());
    let liquid = products.load_dry_liquid().unwrap().binding();
    let navigation = products.load_navigation().unwrap().binding();
    for field in 0..4 {
        let mut changed = navigation;
        match field {
            0 => changed.map += 1,
            1 => changed.content[0] ^= 1,
            2 => changed.source_epoch[0] ^= 1,
            _ => changed.inventory[0] ^= 1,
        }
        assert_eq!(
            geometry_revision(liquid, changed, config),
            Err(Failure::Binding)
        );
    }
    for field in 0..3 {
        let other = products::authored_route_smoothing_with_source(|binding| match field {
            0 => binding.content[0] ^= 1,
            1 => binding.source_epoch[0] ^= 1,
            _ => binding.effective_config[0] ^= 1,
        });
        assert!(matches!(
            SourceRouteSmoothingProvider::bind(
                &session,
                world.profile(),
                products.load_dry_liquid().unwrap(),
                other.load_navigation().unwrap(),
                config,
            ),
            Err(Failure::Binding)
        ));
        // Matching replacement snapshots still cannot reuse the selected old
        // profile/session revision, even when the decoded geometry is identical.
        assert!(matches!(
            SourceRouteSmoothingProvider::bind(
                &session,
                world.profile(),
                other.load_dry_liquid().unwrap(),
                other.load_navigation().unwrap(),
                config,
            ),
            Err(Failure::Binding)
        ));
    }
    for field in 0..5 {
        let mut a = liquid;
        let mut b = navigation;
        let mut c = config;
        match field {
            0 => a.policy[0] ^= 1,
            1 => b.policy[0] ^= 1,
            2 => c.point_limit -= 1,
            3 => c.budget.corridor_expansions -= 1,
            _ => c.budget.smoothing_primitives -= 1,
        }
        assert_ne!(geometry_revision(a, b, c).unwrap(), session.geometry);
    }
    assert!(matches!(
        SourceRouteFilterProvider::bind(
            &session,
            world.profile(),
            products.load_dry_liquid().unwrap(),
        ),
        Err(Failure::Binding)
    ));
    assert_ne!(
        source_route_filter::geometry_revision(liquid),
        session.geometry
    );
    for point_limit in [0, 1, 75, usize::MAX] {
        assert_eq!(
            geometry_revision(
                liquid,
                navigation,
                SourceRouteSmoothingConfiguration {
                    point_limit,
                    ..config
                },
            ),
            Err(Failure::Invalid)
        );
    }
    for field in 0..2 {
        let mut invalid = config;
        match field {
            0 => invalid.budget.corridor_expansions += 1,
            _ => invalid.budget.smoothing_primitives += 1,
        }
        assert_eq!(
            geometry_revision(liquid, navigation, invalid),
            Err(Failure::Invalid)
        );
    }
}

#[test]
fn frozen_full_request_rejects_pose_ticket_snapshot_and_stage_capacity_changes() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context) = setup(&products, config, origin());
    let provider = bind_provider(&products, &session, world.profile(), config);
    let bound = provider.begin(&session, &context).unwrap();
    let query = request(&provider, &context);
    for field in 0..27 {
        let mut changed = query.clone();
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut changed.context.query else {
            unreachable!()
        };
        match field {
            0 => changed.snapshot.liquid.map += 1,
            1 => changed.snapshot.liquid.content[0] ^= 1,
            2 => changed.snapshot.liquid.source_epoch[0] ^= 1,
            3 => changed.snapshot.liquid.inventory[0] ^= 1,
            4 => changed.snapshot.liquid.policy[0] ^= 1,
            5 => changed.snapshot.navigation.map += 1,
            6 => changed.snapshot.navigation.content[0] ^= 1,
            7 => changed.snapshot.navigation.source_epoch[0] ^= 1,
            8 => changed.snapshot.navigation.inventory[0] ^= 1,
            9 => changed.snapshot.navigation.policy[0] ^= 1,
            10 => changed.snapshot.configuration.point_limit -= 1,
            11 => changed.snapshot.configuration.budget.corridor_expansions -= 1,
            12 => changed.snapshot.configuration.budget.smoothing_primitives -= 1,
            13 => changed.first_ticket = 0,
            14 => route.owner_position.x = ai::Real::new(-401.).unwrap(),
            15 => route.request.origin.x = ai::Real::new(-401.).unwrap(),
            16 => route.request.launch_origin.x = ai::Real::new(-401.).unwrap(),
            17 => route.request.goal.x = ai::Real::new(-401.).unwrap(),
            18 => route.request.point_limit -= 1,
            19 => route.request.binding.query_id += 1,
            20 => route.request.binding.now_ms += 1,
            21 => changed.context.binding.npc.as_mut().unwrap().generation += 1,
            22 => changed.context.binding.tick += 1,
            23 => route.request.owner_collision_height = ai::Real::new(3.).unwrap(),
            24 => route.request.target = None,
            25 => route.request.kind = motion::RouteKind::HomeForceDestination,
            _ => changed.context.binding.geometry[0] ^= 1,
        }
        let mut observations = 0;
        assert_eq!(
            bound.query_observed(changed, deadline(), |_| observations += 1),
            Err(Failure::Binding),
            "field {field}"
        );
        assert_eq!(observations, 0);
    }
    for first_ticket in [u64::MAX - 1, u64::MAX] {
        let mut changed = query.clone();
        changed.first_ticket = first_ticket;
        let mut observations = 0;
        assert_eq!(
            bound.query_observed(changed, deadline(), |_| observations += 1),
            Err(Failure::Invalid)
        );
        assert_eq!(observations, 0);
    }
    let mut changed = context.clone();
    let Query::SourceMotion(MotionQuery::Route(route)) = &mut changed.query else {
        unreachable!()
    };
    route.request.point_limit = 73;
    assert!(matches!(
        provider.begin(&session, &changed),
        Err(Failure::Invalid)
    ));
}

#[test]
fn all_existing_owner_participant_session_and_generation_guards_remain_required() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context) = setup(&products, config, origin());
    let provider = bind_provider(&products, &session, world.profile(), config);
    for field in 0..10 {
        let mut changed = session.clone();
        match field {
            0 => changed.world[0] ^= 1,
            1 => changed.profile[0] ^= 1,
            2 => changed.policy[0] ^= 1,
            3 => changed.content[0] ^= 1,
            4 => changed.geometry[0] ^= 1,
            5 => changed.map += 1,
            6 => changed.bodies.get_mut(&2).unwrap().height = ai::Real::new(3.).unwrap(),
            7 => changed.player = None,
            8 => changed.participants = None,
            _ => {
                changed.participants = Some(NonRenderParticipants::authored_fixture(
                    [99; 32],
                    session.map,
                ))
            }
        }
        assert!(
            provider.begin(&changed, &context).is_err(),
            "session field {field}"
        );
    }
    for field in 0..6 {
        let mut changed = context.clone();
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut changed.query else {
            unreachable!()
        };
        match field {
            0 => changed.binding.npc.as_mut().unwrap().generation += 1,
            1 => changed.binding.tick += 1,
            2 => route.owner_position.x = ai::Real::new(-401.).unwrap(),
            3 => route.request.binding.identity.content.0[0] ^= 1,
            4 => route.request.owner_collision_height = ai::Real::new(3.).unwrap(),
            _ => route.request.target.as_mut().unwrap().id += 1,
        }
        assert!(
            provider.begin(&session, &changed).is_err(),
            "context field {field}"
        );
    }
    let mut no_owner = context;
    let Query::SourceMotion(MotionQuery::Route(route)) = &no_owner.query else {
        unreachable!()
    };
    no_owner.query = Query::SourceMotion(MotionQuery::Spatial(motion::Query::Route(
        route.request.clone(),
    )));
    assert!(matches!(
        provider.begin(&session, &no_owner),
        Err(Failure::UnsupportedQuery)
    ));
}

#[test]
fn shared_absolute_deadline_never_leaks_a_partial_filter_or_smoothing_dependency() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context) = setup(&products, config, origin());
    let provider = bind_provider(&products, &session, world.profile(), config);
    let bound = provider.begin(&session, &context).unwrap();
    let query = request(&provider, &context);
    let mut observations = 0;
    assert_eq!(
        bound.query_observed(query.clone(), Instant::now(), |_| observations += 1),
        Err(Failure::MissingGeometry)
    );
    assert_eq!(observations, 0);
    for expire_after in [1, 2] {
        let until = Instant::now() + Duration::from_millis(100);
        let mut observations = 0;
        assert_eq!(
            bound.query_observed(query.clone(), until, |_| {
                observations += 1;
                if observations == expire_after {
                    std::thread::sleep(until.saturating_duration_since(Instant::now()));
                }
            }),
            Err(Failure::MissingGeometry)
        );
        assert_eq!(observations, expire_after);
    }
    assert!(bound.query(query, deadline()).is_ok());
}

#[test]
fn uncovered_owner_and_missing_navigation_never_supply_a_partial_dependency() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    let config = configuration();
    for missing_owner in [true, false] {
        let position = if missing_owner {
            ai::Point::new(-600., -400., 0.).unwrap()
        } else {
            origin()
        };
        let (world, session, mut context) = setup(&products, config, position);
        let provider = bind_provider(&products, &session, world.profile(), config);
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut context.query else {
            unreachable!()
        };
        // Authored Home-shaped variants expose which position each loaded stage
        // consumes; these altered transcripts do not assert staged liveness.
        route.request.kind = motion::RouteKind::HomeForceDestination;
        route.request.target = None;
        route.request.launch_origin = origin();
        route.request.goal = if missing_owner {
            origin()
        } else {
            ai::Point::new(-400., 800., 0.).unwrap()
        };
        let bound = provider.begin(&session, &context).unwrap();
        let mut observations = 0;
        assert_eq!(
            bound.query_observed(request(&provider, &context), deadline(), |_| {
                observations += 1
            }),
            Err(Failure::MissingGeometry)
        );
        assert_eq!(observations, usize::from(!missing_owner));
    }
}

#[test]
fn source_point_limit_failure_is_retained_raw_but_operational_budget_is_unavailable() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    let config = SourceRouteSmoothingConfiguration {
        point_limit: 2,
        ..configuration()
    };
    let (world, session, context) = setup(&products, config, origin());
    let provider = bind_provider(&products, &session, world.profile(), config);
    let result = provider
        .begin(&session, &context)
        .unwrap()
        .query(request(&provider, &context), deadline())
        .unwrap();
    assert_eq!(
        result.smoothing_reply().result.stop,
        SourceSmoothStop::PointLimit
    );
    assert_eq!(result.smoothing_reply().result.points.len(), 2);
    assert_eq!(result.smoothing_reply().result.point_limit, 2);
    let mut exhausted = configuration();
    exhausted.budget.smoothing_primitives = 0;
    let (world, session, context) = setup(&products, exhausted, origin());
    let provider = bind_provider(&products, &session, world.profile(), exhausted);
    assert_eq!(
        provider
            .begin(&session, &context)
            .unwrap()
            .query(request(&provider, &context), deadline()),
        Err(Failure::MissingGeometry)
    );
}

#[test]
fn previous_sliced_search_policy_cannot_bind_the_direct_search_provider() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    assert_eq!(SOURCE_SMOOTHING_POLICY, 2);
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, mut session, _) = setup(&products, config, origin());
    let checkpoint = world.checkpoint().unwrap();
    let liquid = products.load_dry_liquid().unwrap().binding();
    let navigation = products.load_navigation().unwrap().binding();
    // Reproduce the previously published policy-1 digest, without accepting a
    // caller-selected policy in the production binder.
    let mut old = Sha256::new();
    old.update(b"benilla-source-ground-owner-loaded-smoothing-dependency-v1");
    old.update(source_route_filter::geometry_revision(liquid));
    old.update(navigation.map.to_le_bytes());
    for value in [
        navigation.content,
        navigation.source_epoch,
        navigation.inventory,
        navigation.policy,
    ] {
        old.update(value);
    }
    old.update(1u32.to_le_bytes());
    old.update(SOURCE_PATHFINDER_CPP_SHA256.as_bytes());
    old.update(SOURCE_PATHFINDER_H_SHA256.as_bytes());
    old.update(b"pre-OptimizePath-pre-NormalizeZ");
    for value in [
        config.point_limit,
        config.budget.corridor_expansions,
        config.budget.smoothing_primitives,
    ] {
        old.update(u64::try_from(value).unwrap().to_le_bytes());
    }
    let old_revision: [u8; 32] = old.finalize().into();
    assert_ne!(old_revision, world.profile().geometry_revision);
    let mut profile = world.profile().clone();
    profile.geometry_revision = old_revision;
    session.geometry = old_revision;
    session.profile = profile.digest();
    assert!(matches!(
        SourceRouteSmoothingProvider::bind(
            &session,
            &profile,
            products.load_dry_liquid().unwrap(),
            products.load_navigation().unwrap(),
            config,
        ),
        Err(Failure::Binding)
    ));
    assert_eq!(world.checkpoint().unwrap(), checkpoint);
}
