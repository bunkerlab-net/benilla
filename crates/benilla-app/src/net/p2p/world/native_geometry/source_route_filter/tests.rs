// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual staged runtime requests over authored, decoded terrain/VMap products.
//! Synthetic authority generates surrounding motion only; it supplies neither
//! liquid observations nor the filter being tested, and enables no native entry.
use super::super::{
    core::{Body, NonRenderParticipants, PlayerMotion},
    products,
};
use super::*;
use benilla_sim::creature_ai as ai;
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture as fixture, NativeWorld,
};
use std::{collections::BTreeMap, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

fn owner_position() -> ai::Point {
    let mid = 0.5f32 * (64.0 * 533.333_3);
    ai::Point::new(mid - 21., mid - 1., 10.).unwrap()
}

fn setup(
    binding: SnapshotBinding,
    position: ai::Point,
) -> (NativeWorld, Session, AuthorityRequest) {
    let mut profile = fixture::profile();
    profile.content_digest = binding.content;
    profile.map_id = binding.map;
    profile.geometry_revision = geometry_revision(binding);
    profile.npcs[0].ai.map_id = binding.map;
    profile.npcs[0].ai.content_revision = ai::movement::Revision(binding.content);
    profile.npcs[0].ai.spawn = position;
    profile.player.start = ai::Point::new(
        position.x.as_f32() + 16.,
        position.y.as_f32(),
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
    let context = authority
        .queries
        .into_iter()
        .find(|q| matches!(q.query, Query::SourceMotion(MotionQuery::Route(_))))
        .unwrap();
    (world, session, context)
}

fn request(provider: &SourceRouteFilterProvider, context: &AuthorityRequest) -> RouteFilterRequest {
    RouteFilterRequest {
        snapshot: provider.binding(),
        context: context.clone(),
        first_ticket: 41,
    }
}

#[test]
fn actual_owned_route_reads_loaded_liquids_in_source_order_without_publishing_a_route() {
    let (products, _) = products::authored_ground_height();
    let snapshot = products.load_dry_liquid().unwrap();
    let (world, session, context) = setup(snapshot.binding(), owner_position());
    let before = world.checkpoint().unwrap();
    let provider = SourceRouteFilterProvider::bind(&session, world.profile(), snapshot).unwrap();
    let bound = provider.begin(&session, &context).unwrap();
    let query = request(&provider, &context);
    let mut observed = vec![];
    let result = bound
        .query_observed(query.clone(), deadline(), |observation| {
            observed.push(*observation)
        })
        .unwrap();
    assert_eq!(result.request(), &query);
    let position = owner_position();
    let position = [position.x, position.y, position.z].map(|v| v.as_f32().to_bits());
    assert_eq!(result.owner_position(), position);
    assert_eq!(observed.as_slice(), result.liquid_observations());
    for (i, mask) in [
        route_filter::LiquidMask::AllLiquids,
        route_filter::LiquidMask::WaterOrOcean,
    ]
    .into_iter()
    .enumerate()
    {
        let observation = result.liquid_observations()[i];
        assert_eq!(observation.mask, mask);
        assert_eq!(observation.reply.status, LiquidStatus::NoWater);
        assert_eq!(observation.reply.request.snapshot, provider.binding());
        assert_eq!(observation.reply.request.ticket, 41 + i as u64);
        assert_eq!(observation.reply.request.position, position);
    }
    assert_eq!(result.filter().include_flags(), 1);
    assert_eq!(result.filter().exclude_flags(), 0);
    assert_eq!(result.filter().branch(), route_filter::Branch::GroundDry);
    assert_eq!(world.checkpoint().unwrap(), before);
}

#[test]
fn independent_participants_and_selected_profile_are_required_at_bind_and_begin() {
    let (products, _) = products::authored_ground_height();
    let snapshot = products.load_dry_liquid().unwrap();
    let (world, session, context) = setup(snapshot.binding(), owner_position());
    let provider = SourceRouteFilterProvider::bind(&session, world.profile(), snapshot).unwrap();
    for participants in [
        None,
        Some(NonRenderParticipants::authored_fixture(
            [99; 32],
            session.map,
        )),
        Some(NonRenderParticipants::authored_fixture(
            session.content,
            session.map + 1,
        )),
    ] {
        let mut changed = session.clone();
        changed.participants = participants;
        assert!(matches!(
            provider.begin(&changed, &context),
            Err(Failure::UnprovenParticipants)
        ));
        assert!(matches!(
            SourceRouteFilterProvider::bind(
                &changed,
                world.profile(),
                products.load_dry_liquid().unwrap(),
            ),
            Err(Failure::UnprovenParticipants)
        ));
    }
    for field in 0..8 {
        let mut changed = session.clone();
        match field {
            0 => changed.world = [0; 16],
            1 => changed.profile[0] ^= 1,
            2 => changed.policy[0] ^= 1,
            3 => changed.content[0] ^= 1,
            4 => changed.geometry[0] ^= 1,
            5 => changed.map += 1,
            6 => changed.bodies.get_mut(&2).unwrap().height = ai::Real::new(3.).unwrap(),
            _ => changed.player = None,
        }
        assert!(provider.begin(&changed, &context).is_err(), "field {field}");
    }
    let mut changed = world.profile().clone();
    changed.npcs[0].entry += 1;
    assert!(SourceRouteFilterProvider::bind(
        &session,
        &changed,
        products.load_dry_liquid().unwrap(),
    )
    .is_err());
}

#[test]
fn full_outer_generation_clock_source_identity_and_body_are_checked_before_queries() {
    let (products, _) = products::authored_ground_height();
    let snapshot = products.load_dry_liquid().unwrap();
    let (world, session, context) = setup(snapshot.binding(), owner_position());
    let provider = SourceRouteFilterProvider::bind(&session, world.profile(), snapshot).unwrap();
    for field in 0..18 {
        let mut changed = context.clone();
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut changed.query else {
            unreachable!()
        };
        match field {
            0 => changed.binding.world[0] ^= 1,
            1 => changed.binding.profile[0] ^= 1,
            2 => changed.binding.authority_policy[0] ^= 1,
            3 => changed.binding.geometry[0] ^= 1,
            4 => changed.binding.tick = 0,
            5 => changed.binding.tick += 1,
            6 => changed.binding.npc.as_mut().unwrap().generation += 1,
            7 => changed.binding.npc = None,
            8 => route.request.binding.identity.creature += 1,
            9 => route.request.binding.identity.map += 1,
            10 => route.request.binding.identity.content.0[0] ^= 1,
            11 => route.request.binding.identity.geometry.0[0] ^= 1,
            12 => route.request.binding.query_id = 0,
            13 => route.request.binding.now_ms += 1,
            14 => route.owner_position.x = ai::Real::new(0.).unwrap(),
            15 => route.request.owner_collision_height = ai::Real::new(3.).unwrap(),
            16 => route.request.point_limit = 1,
            _ => route.request.target.as_mut().unwrap().id += 1,
        }
        assert!(provider.begin(&session, &changed).is_err(), "field {field}");
    }
    for field in 0..5 {
        let mut wire = serde_json::to_value(&context).unwrap();
        let witness = &mut wire["query"]["SourceMotion"]["Route"]["owner"];
        match field {
            0 => {
                witness["source_binding"][0] =
                    (witness["source_binding"][0].as_u64().unwrap() ^ 1).into()
            }
            1 => witness["inhabit_type"] = 3.into(),
            2 => witness["capability_movement_flags"] = 0x400_u32.into(),
            3 => witness["npc"]["generation"] = 99.into(),
            _ => witness["npc"]["spawn"] = 3.into(),
        }
        let changed: AuthorityRequest = serde_json::from_value(wire).unwrap();
        assert!(
            provider.begin(&session, &changed).is_err(),
            "capability field {field}"
        );
    }
}

#[test]
fn frozen_request_rejects_every_liquid_snapshot_field_and_route_context_change() {
    let (products, _) = products::authored_ground_height();
    let snapshot = products.load_dry_liquid().unwrap();
    let (world, session, context) = setup(snapshot.binding(), owner_position());
    let provider = SourceRouteFilterProvider::bind(&session, world.profile(), snapshot).unwrap();
    let bound = provider.begin(&session, &context).unwrap();
    let query = request(&provider, &context);
    for field in 0..18 {
        let mut changed = query.clone();
        let Query::SourceMotion(MotionQuery::Route(route)) = &mut changed.context.query else {
            unreachable!()
        };
        match field {
            0 => changed.snapshot.map += 1,
            1 => changed.snapshot.content[0] ^= 1,
            2 => changed.snapshot.source_epoch[0] ^= 1,
            3 => changed.snapshot.inventory[0] ^= 1,
            4 => changed.snapshot.policy[0] ^= 1,
            5 => changed.first_ticket = 0,
            6 => route.request.origin.x = ai::Real::new(0.).unwrap(),
            7 => route.request.launch_origin.x = ai::Real::new(0.).unwrap(),
            8 => route.request.goal.x = ai::Real::new(0.).unwrap(),
            9 => route.owner_position.x = ai::Real::new(0.).unwrap(),
            10 => route.request.binding.query_id += 1,
            11 => route.request.binding.now_ms += 1,
            12 => route.request.point_limit -= 1,
            13 => route.request.owner_collision_height = ai::Real::new(3.).unwrap(),
            14 => route.request.kind = motion::RouteKind::HomeForceDestination,
            15 => route.request.target = None,
            16 => changed.context.binding.npc.as_mut().unwrap().generation += 1,
            _ => changed.context.binding.tick += 1,
        }
        let mut observations = 0;
        assert_eq!(
            bound.query_observed(changed, deadline(), |_| observations += 1),
            Err(Failure::Binding),
            "field {field}"
        );
        assert_eq!(observations, 0);
    }
    let mut overflow = query;
    overflow.first_ticket = u64::MAX;
    let mut observations = 0;
    assert_eq!(
        bound.query_observed(overflow, deadline(), |_| observations += 1),
        Err(Failure::Invalid)
    );
    assert_eq!(observations, 0);
}

#[test]
fn covered_goal_or_launch_origin_cannot_substitute_for_uncovered_owner_position() {
    let (products, _) = products::authored_ground_height();
    let snapshot = products.load_dry_liquid().unwrap();
    let (world, session, mut context) =
        setup(snapshot.binding(), ai::Point::new(0., 0., 10.).unwrap());
    let provider = SourceRouteFilterProvider::bind(&session, world.profile(), snapshot).unwrap();
    let Query::SourceMotion(MotionQuery::Route(route)) = &mut context.query else {
        unreachable!()
    };
    // An explicitly authored Home-shaped variant tests which retained coordinate
    // is consumed; it is not an assertion that this altered transcript is live.
    route.request.kind = motion::RouteKind::HomeForceDestination;
    route.request.target = None;
    route.request.goal = owner_position();
    route.request.launch_origin = owner_position();
    let bound = provider.begin(&session, &context).unwrap();
    assert_eq!(
        bound.query(request(&provider, &context), deadline()),
        Err(Failure::MissingGeometry)
    );

    let (world, session, mut context) = setup(provider.binding(), owner_position());
    let provider = SourceRouteFilterProvider::bind(
        &session,
        world.profile(),
        products.load_dry_liquid().unwrap(),
    )
    .unwrap();
    let Query::SourceMotion(MotionQuery::Route(route)) = &mut context.query else {
        unreachable!()
    };
    route.request.kind = motion::RouteKind::HomeForceDestination;
    route.request.target = None;
    route.request.goal = ai::Point::new(0., 0., 10.).unwrap();
    route.request.launch_origin = route.request.goal;
    let bound = provider.begin(&session, &context).unwrap();
    let result = bound
        .query(request(&provider, &context), deadline())
        .unwrap();
    assert_eq!(
        result.liquid_observations()[0].reply.request.position,
        [owner_position().x, owner_position().y, owner_position().z].map(|v| v.as_f32().to_bits())
    );
}

#[test]
fn both_observations_share_one_absolute_deadline_and_expiration_discards_the_dependency() {
    let (products, _) = products::authored_ground_height();
    let snapshot = products.load_dry_liquid().unwrap();
    let (world, session, context) = setup(snapshot.binding(), owner_position());
    let provider = SourceRouteFilterProvider::bind(&session, world.profile(), snapshot).unwrap();
    let bound = provider.begin(&session, &context).unwrap();
    let query = request(&provider, &context);
    let mut observations = 0;
    assert_eq!(
        bound.query_observed(query.clone(), Instant::now(), |_| observations += 1),
        Err(Failure::Deadline)
    );
    assert_eq!(observations, 0);
    let until = Instant::now() + Duration::from_millis(100);
    let result = bound.query_observed(query, until, |_| {
        observations += 1;
        std::thread::sleep(until.saturating_duration_since(Instant::now()));
    });
    assert_eq!(result, Err(Failure::Deadline));
    assert_eq!(observations, 1);
}

#[test]
fn loaded_snapshot_is_owned_and_new_product_inventory_cannot_reuse_its_revision() {
    fn assert_send_static<T: Send + 'static>() {}
    assert_send_static::<SourceRouteFilterProvider>();
    let (products, _) = products::authored_ground_height();
    let snapshot = products.load_dry_liquid().unwrap();
    let (world, session, context) = setup(snapshot.binding(), owner_position());
    let provider = SourceRouteFilterProvider::bind(&session, world.profile(), snapshot).unwrap();
    drop(products);
    let bound = provider.begin(&session, &context).unwrap();
    assert!(bound
        .query(request(&provider, &context), deadline())
        .is_ok());

    let (changed, _) = products::authored_hole_ground_height();
    let snapshot = changed.load_dry_liquid().unwrap();
    assert_ne!(provider.binding().inventory, snapshot.binding().inventory);
    assert_ne!(session.geometry, geometry_revision(snapshot.binding()));
    assert!(matches!(
        SourceRouteFilterProvider::bind(&session, world.profile(), snapshot),
        Err(Failure::Binding)
    ));
    for field in 0..5 {
        let mut b = provider.binding();
        match field {
            0 => b.map += 1,
            1 => b.content[0] ^= 1,
            2 => b.source_epoch[0] ^= 1,
            3 => b.inventory[0] ^= 1,
            _ => b.policy[0] ^= 1,
        }
        assert_ne!(geometry_revision(b), session.geometry, "field {field}");
    }
}

#[test]
fn bare_spatial_route_does_not_acquire_owner_capabilities_or_a_filter() {
    let (products, _) = products::authored_ground_height();
    let snapshot = products.load_dry_liquid().unwrap();
    let (world, session, mut context) = setup(snapshot.binding(), owner_position());
    let provider = SourceRouteFilterProvider::bind(&session, world.profile(), snapshot).unwrap();
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        unreachable!()
    };
    context.query = Query::SourceMotion(MotionQuery::Spatial(motion::Query::Route(
        route.request.clone(),
    )));
    assert!(matches!(
        provider.begin(&session, &context),
        Err(Failure::UnsupportedQuery)
    ));
}
