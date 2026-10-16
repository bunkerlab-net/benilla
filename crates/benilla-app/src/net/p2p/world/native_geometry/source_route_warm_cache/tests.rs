// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded warm geometry, with explicit authored prior-cache unit inputs.
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
fn configuration() -> WarmCacheConfiguration {
    WarmCacheConfiguration {
        budget: SourceWarmCacheBudget::default(),
    }
}
fn origin() -> ai::Point {
    ai::Point::new(-400., -400., 0.).unwrap()
}
fn setup(
    products: &InventoriedProducts,
    config: WarmCacheConfiguration,
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
    config: WarmCacheConfiguration,
) -> SourceRouteWarmCacheProvider {
    SourceRouteWarmCacheProvider::bind(
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
    provider: &SourceRouteWarmCacheProvider,
    context: &AuthorityRequest,
) -> WarmCacheRequest {
    WarmCacheRequest {
        snapshot: provider.binding(),
        context: context.clone(),
        first_ticket: 41,
    }
}
#[test]
fn loaded_warm_candidate_is_repeatable_and_never_changes_prior_or_runtime() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context, navigation, lineage) = setup(&products, config, origin());
    let before = world.checkpoint().unwrap();
    let stored = lineage.cache().storage().to_vec();
    let p = provider(&products, &world, &session, navigation, lineage, config);
    let q = request(&p, &context);
    let bound = p.begin(&session, &context).unwrap();
    let a = bound.query(q.clone(), deadline()).unwrap();
    let b = bound.query(q.clone(), deadline()).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.request(), &q);
    assert_eq!(
        a.filter_dependency().liquid_observations()[0]
            .reply
            .request
            .ticket,
        41
    );
    assert_eq!(
        a.filter_dependency().liquid_observations()[1]
            .reply
            .request
            .ticket,
        42
    );
    assert_eq!(a.polygon_dependency().request().ticket, 43);
    assert_eq!(a.polygon_dependency().result().after().active_len(), 1);
    assert_eq!(a.polygon_dependency().result().after().storage(), stored);
    assert_eq!(p.lineage.cache().active_len(), 2);
    assert_eq!(world.checkpoint().unwrap(), before);
}
#[test]
fn frozen_prior_and_request_identity_reject_every_changed_cache_input() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context, navigation, lineage) = setup(&products, config, origin());
    let p = provider(&products, &world, &session, navigation, lineage, config);
    for field in 0..8 {
        let mut changed = context.clone();
        let Query::SourceMotion(MotionQuery::Route(r)) = &mut changed.query else {
            unreachable!()
        };
        let prior = r.request.prior_cache.as_mut().unwrap();
        match field {
            0 => prior.binding.owner_generation += 1,
            1 => prior.binding.inventory[0] ^= 1,
            2 => prior.binding.source_epoch[0] ^= 1,
            3 => prior.binding.navigation_policy[0] ^= 1,
            4 => prior.poly_refs[0] ^= 1,
            5 => prior.active_len = 0,
            6 => prior.point_limit = 73,
            _ => prior.binding.identity.geometry.0[0] ^= 1,
        }
        assert!(
            matches!(p.begin(&session, &changed), Err(Failure::Binding)),
            "field {field}"
        );
    }
    let bound = p.begin(&session, &context).unwrap();
    for field in 0..5 {
        let mut q = request(&p, &context);
        match field {
            0 => q.first_ticket = 0,
            1 => q.snapshot.navigation.inventory[0] ^= 1,
            2 => q.context.binding.geometry[0] ^= 1,
            3 => {
                let Query::SourceMotion(MotionQuery::Route(r)) = &mut q.context.query else {
                    unreachable!()
                };
                r.request.prior_cache.as_mut().unwrap().revision += 1;
            }
            _ => q.snapshot.configuration.budget.lookup_primitives -= 1,
        }
        assert!(matches!(bound.query(q, deadline()), Err(Failure::Binding)));
    }
    let mut no_prior = context.clone();
    let Query::SourceMotion(MotionQuery::Route(r)) = &mut no_prior.query else {
        unreachable!()
    };
    r.request.prior_cache = None;
    assert!(matches!(
        p.begin(&session, &no_prior),
        Err(Failure::MissingGeometry)
    ));
}
#[test]
fn identical_reloaded_bytes_cannot_rehydrate_an_opaque_cache_lineage() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    let config = configuration();
    let (world, session, context, _original_navigation, lineage) =
        setup(&products, config, origin());
    let reloaded = products.load_navigation().unwrap();
    assert_eq!(lineage.binding(), reloaded.binding());
    let p = provider(&products, &world, &session, reloaded, lineage, config);
    assert!(matches!(
        p.begin(&session, &context)
            .unwrap()
            .query(request(&p, &context), deadline()),
        Err(Failure::MissingGeometry)
    ));
}
#[test]
fn deadline_budget_tickets_and_participant_guards_never_publish_a_partial_candidate() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let products = products::authored_route_smoothing();
    for zero_budget in [false, true] {
        let mut config = configuration();
        if zero_budget {
            config.budget.lookup_primitives = 0;
        }
        let (world, session, context, navigation, lineage) = setup(&products, config, origin());
        let p = provider(&products, &world, &session, navigation, lineage, config);
        let bound = p.begin(&session, &context).unwrap();
        let before = p.lineage.clone();
        assert!(matches!(
            bound.query(
                request(&p, &context),
                if zero_budget {
                    deadline()
                } else {
                    Instant::now()
                }
            ),
            Err(Failure::MissingGeometry)
        ));
        assert_eq!(p.lineage, before);
        let mut q = request(&p, &context);
        q.first_ticket = u64::MAX;
        assert!(matches!(bound.query(q, deadline()), Err(Failure::Invalid)));
        let mut invalid = session.clone();
        invalid.participants = None;
        assert!(p.begin(&invalid, &context).is_err());
    }
}
