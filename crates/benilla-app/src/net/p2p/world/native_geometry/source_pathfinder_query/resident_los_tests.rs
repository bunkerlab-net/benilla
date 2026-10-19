// SPDX-License-Identifier: GPL-3.0-or-later
//! Generated resident geometry and actual NativeWorld constructor dispatch, with
//! model operands initialized through existing authored catalog loaders. No real
//! install catalog/starter provenance or movement admission is established.
use super::super::{
    core::{Body, Failure, NonRenderParticipants, PlayerMotion, Session},
    products::{
        source_height::{CompositionError, PreparedSourceHeight},
        source_query::SourceMapQuerySnapshot,
        vmap::StaticRay,
    },
    source_route_filter::resident::{
        self, chase_los::ResidentChaseLosDecision as Decision, ResidentFilterError,
        ResidentFilterProvider,
    },
};
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world_runtime::native::{
    synthetic_creature_model_fixture as models, synthetic_fixture as f,
    synthetic_motion_fixture as motion_fixture, synthetic_player_model_fixture as player_models,
    Authority, AuthorityReply, AuthorityRequest, CreatureCollisionWidthRows,
    CreatureCollisionWidthSource, CreatureRouteRequest, Facts, MotionFacts, MotionQuery,
    NativeError, NativeWorld, Profile, Query,
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    time::{Duration, Instant},
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
/// Values are raw authored model fields, never final body/witness assertions.
pub(crate) fn model_profile(
    start: [f32; 3],
    end: [f32; 3],
    player_raw: f32,
    owner_raw: f32,
) -> Profile {
    let mut p = super::tests::generated_profile(start, end);
    p.npcs[0].display_id = 34;
    let catalog = models::catalog_with_model_heights(player_raw, owner_raw);
    p.initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(vec![
        CreatureCollisionWidthSource::from_initial_model(2, models::selection(), &catalog).unwrap(),
    ]))
    .unwrap();
    p.initialize_creature_collision_heights().unwrap();
    p.initialize_player_collision_height(&player_models::catalog(), &catalog)
        .unwrap();
    p.validate().unwrap();
    p
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
/// Authored near-point facts let the real runtime create exact boundary dispatches.
/// No request receipt, model witness, or live dispatch seal is synthesized here.
struct GoalAuthority {
    base: motion_fixture::SyntheticMotionAuthority,
    goal: Option<AuthoredGoal>,
    resolved_goal: Option<ai::Point>,
    first_route_goal: Option<ai::Point>,
}
#[derive(Clone, Copy)]
enum AuthoredGoal {
    Absolute(ai::Point),
    OwnerOffset([f32; 3]),
}
impl Authority for GoalAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let mut reply = self.base.resolve(request)?;
        if matches!(request.query, Query::SourceMotion(MotionQuery::Route(_)))
            && self.first_route_goal.is_none()
        {
            // One frame may dispatch again after advancing its first spline.
            // Bind this assertion to the near-point fact preceding the first route.
            self.first_route_goal = self.resolved_goal;
        }
        if let (Some(goal), Query::SourceMotion(MotionQuery::NearPoint(near))) =
            (self.goal, &request.query)
        {
            let goal = match goal {
                AuthoredGoal::Absolute(goal) => goal,
                AuthoredGoal::OwnerOffset(offset) => {
                    let owner = near.request.owner.position;
                    ai::Point::new(
                        owner.x.as_f32() + offset[0],
                        owner.y.as_f32() + offset[1],
                        owner.z.as_f32() + offset[2],
                    )?
                }
            };
            let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &mut reply.facts else {
                panic!("expected authored near-point fact")
            };
            spatial.fact = motion::Fact::NearPoint(goal);
            self.resolved_goal = Some(goal);
        }
        Ok(reply)
    }
}
fn first_dispatch(p: &Profile, goal: Option<AuthoredGoal>) -> (NativeWorld, AuthorityRequest) {
    let mut world =
        NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), 1234).unwrap();
    let mut authority = GoalAuthority {
        base: Default::default(),
        goal,
        resolved_goal: None,
        first_route_goal: None,
    };
    for tick in 1..=8 {
        world.step(tick, None, &mut authority).unwrap();
        if let Some(context) = std::mem::take(&mut authority.base.queries)
            .into_iter()
            .find(|q| matches!(q.query, Query::SourceMotion(MotionQuery::Route(_))))
        {
            let dispatch = context.require_pathfinder_dispatch(p).unwrap();
            assert!(dispatch.first_construction());
            assert_eq!(
                dispatch.constructor_kind(),
                motion::PathfinderConstructorKind::ChaseLazy
            );
            assert_eq!(
                route(&context).request.kind,
                motion::RouteKind::ChaseInitial
            );
            assert!(route(&context).request.prior_cache.is_none());
            if let Some(goal) = authority.first_route_goal {
                assert_eq!(route(&context).request.goal, goal);
            }
            return (world, context);
        }
    }
    panic!("no genuine first Chase dispatch")
}

pub(crate) fn assert_resident_chase_los(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    replacement_query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    mut change_source: impl FnMut(bool),
) {
    p.geometry_revision = resident::geometry_revision(height.binding(), query.binding());
    let s = session(&p);
    let mut provider = ResidentFilterProvider::bind(&s, &p, height, query).unwrap();
    let other = ResidentFilterProvider::bind(&s, &p, height, replacement_query).unwrap();
    let (world, mut context) = first_dispatch(&p, None);
    // This prefix requires neither the target-water cache nor owner-ignore or
    // calculate receipts. Clearing these does not alter the actual route request.
    route_mut(&mut context).target_player_water = None;
    route_mut(&mut context).owner_ignore_pathfinding = None;
    let before = world.checkpoint().unwrap();
    let prior = provider.observe(&context, 101, until(), || false).unwrap();
    let mut phases = Vec::new();
    let dependency = provider
        .observe_chase_los_observed(
            &prior,
            &context,
            f::player(),
            103,
            until(),
            || false,
            |phase| phases.push(phase),
        )
        .unwrap();
    assert_eq!(phases, [0, 1, 2]);
    let observed = provider
        .los_for(&dependency, &context, until(), || false)
        .unwrap();
    assert_eq!(observed.decision, Decision::ClearPrefix);
    let ray = observed.ray.unwrap();
    assert_eq!(ray.result, StaticRay::Miss);
    assert_eq!(ray.request.ticket, 103);
    assert!(!ray.request.ignore_m2);
    let r = route(&context);
    let owner = r.require_collision_height(&p).unwrap();
    let target = r
        .require_target_collision_height(&p, s.world, f::player())
        .unwrap();
    let owner_bits = bits(r.owner_position);
    let goal_bits = bits(r.request.goal);
    assert_ne!(r.request.goal, r.request.target.unwrap().pose.position);
    assert_ne!(owner.height(), target.height());
    assert_eq!(
        ray.request.from,
        [
            owner_bits[0],
            owner_bits[1],
            (r.owner_position.z.as_f32() + f32::from_bits(owner.height().bits())).to_bits()
        ]
    );
    assert_eq!(
        ray.request.to,
        [
            goal_bits[0],
            goal_bits[1],
            (r.request.goal.z.as_f32() + f32::from_bits(target.height().bits())).to_bits()
        ]
    );
    assert_ne!(
        ray.request.to[..2],
        bits(r.request.target.unwrap().pose.position)[..2]
    );
    assert_eq!(prior.observations().map(|r| r.request().ticket), [101, 102]);
    assert_eq!(provider.retained_constructor_count(), 1);
    assert_eq!(world.checkpoint().unwrap(), before);

    let decoded: AuthorityRequest =
        serde_json::from_value(serde_json::to_value(&context).unwrap()).unwrap();
    assert_eq!(decoded, context);
    let (_, mut independent) = first_dispatch(&p, None);
    route_mut(&mut independent).target_player_water = None;
    route_mut(&mut independent).owner_ignore_pathfinding = None;
    assert_eq!(independent, context);
    let mut invalid = vec![decoded, independent];
    for edit in 0..10 {
        let mut changed = context.clone();
        match edit {
            0 => changed.binding.world[0] ^= 1,
            1 => changed.binding.tick += 1,
            2 => changed.binding.npc.as_mut().unwrap().generation += 1,
            3 => route_mut(&mut changed).collision_width = None,
            4 => route_mut(&mut changed).collision_height = None,
            5 => route_mut(&mut changed).target_player_height = None,
            6 => {
                route_mut(&mut changed).request.goal.x =
                    ai::Real::new(r.request.goal.x.as_f32() + 1.0).unwrap()
            }
            7 => {
                route_mut(&mut changed).request.owner_collision_height =
                    ai::Real::new(f32::from_bits(owner.height().bits()) + 1.0).unwrap()
            }
            8 => route_mut(&mut changed).request.kind = motion::RouteKind::Wander,
            _ => {
                let mut json =
                    serde_json::to_value(r.target_player_height.as_ref().unwrap()).unwrap();
                json["witness"]["source_binding"][0] = serde_json::json!(99);
                route_mut(&mut changed).target_player_height =
                    Some(serde_json::from_value(json).unwrap());
            }
        }
        invalid.push(changed);
    }
    for invalid in invalid {
        let mut phases = Vec::new();
        assert!(provider
            .observe_chase_los_observed(
                &prior,
                &invalid,
                f::player(),
                105,
                until(),
                || false,
                |phase| phases.push(phase)
            )
            .is_err());
        assert!(phases.is_empty());
        assert!(provider
            .los_for(&dependency, &invalid, until(), || false)
            .is_err());
    }
    assert!(other
        .observe_chase_los(&prior, &context, f::player(), 105, until(), || false)
        .is_err());
    assert!(other
        .los_for(&dependency, &context, until(), || false)
        .is_err());
    let mut wrong_player = f::player();
    wrong_player.character[0] ^= 1;
    assert!(provider
        .observe_chase_los(&prior, &context, wrong_player, 105, until(), || false)
        .is_err());
    for ticket in [0, 101, 102] {
        assert!(provider
            .observe_chase_los(&prior, &context, f::player(), ticket, until(), || false)
            .is_err());
    }

    // These exact contexts have fresh valid filter evidence; only model provenance
    // is missing/forged. The numeric/LOS stage must never infer it from body scalars.
    for edit in 0..4 {
        let mut changed = context.clone();
        match edit {
            0 => route_mut(&mut changed).collision_width = None,
            1 => route_mut(&mut changed).collision_height = None,
            2 => route_mut(&mut changed).target_player_height = None,
            _ => {
                let mut json = serde_json::to_value(r.collision_height.as_ref().unwrap()).unwrap();
                json["npc"]["generation"] = serde_json::json!(1);
                route_mut(&mut changed).collision_height =
                    Some(serde_json::from_value(json).unwrap());
            }
        }
        let fresh = provider
            .observe(&changed, 111 + edit * 4, until(), || false)
            .unwrap();
        let mut phases = Vec::new();
        assert!(provider
            .observe_chase_los_observed(
                &fresh,
                &changed,
                f::player(),
                113 + edit * 4,
                until(),
                || false,
                |phase| phases.push(phase)
            )
            .is_err());
        assert!(phases.is_empty());
    }

    assert!(matches!(
        provider.observe_chase_los(&prior, &context, f::player(), 131, Instant::now(), || false),
        Err(ResidentFilterError::Context(Failure::Deadline))
    ));
    assert!(matches!(
        provider.los_for(&dependency, &context, Instant::now(), || false),
        Err(ResidentFilterError::Context(Failure::Deadline))
    ));
    assert!(matches!(
        provider.los_for(&dependency, &context, until(), || true),
        Err(ResidentFilterError::Source(CompositionError::Cancelled))
    ));
    for stop in 0..=2 {
        let cancelled = Cell::new(false);
        let mut phases = Vec::new();
        let result = provider.observe_chase_los_observed(
            &prior,
            &context,
            f::player(),
            151 + u64::from(stop),
            until(),
            || cancelled.get(),
            |phase| {
                phases.push(phase);
                if phase == stop {
                    cancelled.set(true);
                }
            },
        );
        assert!(result.is_err(), "cancellation after phase {stop}");
        assert_eq!(phases.last(), Some(&stop));
        assert_eq!(
            provider
                .los_for(&dependency, &context, until(), || false)
                .unwrap(),
            observed
        );
        let retry = provider
            .observe_chase_los(
                &prior,
                &context,
                f::player(),
                161 + u64::from(stop),
                until(),
                || false,
            )
            .unwrap();
        assert_eq!(
            provider
                .los_for(&retry, &context, until(), || false)
                .unwrap()
                .decision,
            Decision::ClearPrefix
        );
    }
    for stop in 0..=2 {
        let mut reached = false;
        let result = provider.observe_chase_los_observed(
            &prior,
            &context,
            f::player(),
            171 + u64::from(stop),
            until(),
            || false,
            |phase| {
                if phase == stop {
                    reached = true;
                    change_source(true);
                }
            },
        );
        assert!(reached);
        assert!(result.is_err(), "source mutation after phase {stop}");
        assert!(provider
            .los_for(&dependency, &context, until(), || false)
            .is_err());
        change_source(false);
        let retry = provider
            .observe_chase_los(
                &prior,
                &context,
                f::player(),
                181 + u64::from(stop),
                until(),
                || false,
            )
            .unwrap();
        assert_eq!(
            provider
                .los_for(&retry, &context, until(), || false)
                .unwrap()
                .decision,
            Decision::ClearPrefix
        );
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut reached = false;
    let result = provider.observe_chase_los_observed(
        &prior,
        &context,
        f::player(),
        201,
        deadline,
        || false,
        |phase| {
            if phase == 2 {
                reached = true;
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            }
        },
    );
    assert!(reached);
    assert!(result.is_err());
    let mut expired = provider.observe(&context, 211, until(), || false).unwrap();
    expired.expire_for_test();
    let mut phases = Vec::new();
    assert!(provider
        .observe_chase_los_observed(
            &expired,
            &context,
            f::player(),
            213,
            until(),
            || false,
            |phase| phases.push(phase)
        )
        .is_err());
    assert!(phases.is_empty());
    // The longer requested LOS lifetime cannot outlive its exact constructor
    // evidence, even when a later lookup supplies another far-future deadline.
    let expires = Instant::now() + Duration::from_secs(1);
    let short_prior = provider.observe(&context, 221, expires, || false).unwrap();
    let capped = provider
        .observe_chase_los(&short_prior, &context, f::player(), 223, until(), || false)
        .unwrap();
    assert_eq!(
        provider
            .los_for(&capped, &context, until(), || false)
            .unwrap()
            .decision,
        Decision::ClearPrefix
    );
    std::thread::sleep(expires.saturating_duration_since(Instant::now()));
    assert!(matches!(
        provider.los_for(&capped, &context, until(), || false),
        Err(ResidentFilterError::Context(Failure::Deadline))
    ));
    let fresh = provider.observe(&context, 231, until(), || false).unwrap();
    let retry = provider
        .observe_chase_los(&fresh, &context, f::player(), 233, until(), || false)
        .unwrap();
    assert_eq!(
        provider
            .los_for(&retry, &context, until(), || false)
            .unwrap()
            .decision,
        Decision::ClearPrefix
    );
    assert_eq!(provider.retained_constructor_count(), 1);
    assert_eq!(world.checkpoint().unwrap(), before);
}

pub(crate) fn assert_resident_los_decision(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    goal: [f32; 3],
    expected: Decision,
) {
    p.geometry_revision = resident::geometry_revision(height.binding(), query.binding());
    let mut provider = ResidentFilterProvider::bind(&session(&p), &p, height, query).unwrap();
    let requested_offset = goal;
    let goal = if matches!(expected, Decision::Distance | Decision::Vertical) {
        AuthoredGoal::OwnerOffset(goal)
    } else {
        AuthoredGoal::Absolute(ai::Point::new(goal[0], goal[1], goal[2]).unwrap())
    };
    let (world, context) = first_dispatch(&p, Some(goal));
    let goal = route(&context).request.goal;
    if expected == Decision::Distance {
        let r = route(&context);
        let dx = goal.x.as_f32() - r.owner_position.x.as_f32();
        if requested_offset[2] == 0.0 {
            assert_eq!(
                dx.to_bits(),
                requested_offset[0].to_bits(),
                "actual runtime feet must preserve the exact distance boundary"
            );
        } else {
            let selection = r
                .require_collision_width(&p)
                .unwrap()
                .validated_model_selection(&p, r.owner.npc(), r.owner.initialization())
                .unwrap();
            assert!(dx > 200.0 + f32::from_bits(selection.combat_reach().bits()));
            assert!((goal.z.as_f32() - r.owner_position.z.as_f32()).abs() >= 5.0);
        }
    }
    if expected == Decision::Vertical {
        assert_eq!(
            (route(&context).owner_position.z.as_f32() - goal.z.as_f32()).to_bits(),
            5.0f32.to_bits(),
            "actual runtime feet must lie exactly on the strict vertical boundary"
        );
    }
    let before = world.checkpoint().unwrap();
    let prior = provider.observe(&context, 301, until(), || false).unwrap();
    let mut phases = Vec::new();
    let dep = provider
        .observe_chase_los_observed(
            &prior,
            &context,
            f::player(),
            303,
            until(),
            || false,
            |phase| phases.push(phase),
        )
        .unwrap();
    let observation = provider.los_for(&dep, &context, until(), || false).unwrap();
    assert_eq!(observation.decision, expected);
    if matches!(expected, Decision::Distance | Decision::Vertical) {
        assert_eq!(phases, [0]);
        assert!(observation.ray.is_none());
    } else {
        assert_eq!(phases, [0, 1, 2]);
        let ray = observation.ray.unwrap();
        assert!(!ray.request.ignore_m2);
        assert_eq!(ray.request.to[..2], bits(goal)[..2]);
        assert!(matches!(ray.result, StaticRay::Hit { .. }));
    }
    assert_eq!(prior.observations().map(|r| r.request().ticket), [301, 302]);
    assert_eq!(provider.retained_constructor_count(), 1);
    assert_eq!(world.checkpoint().unwrap(), before);
}

pub(crate) fn assert_resident_scalar_body_is_not_model(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
) {
    p.geometry_revision = resident::geometry_revision(height.binding(), query.binding());
    let mut provider = ResidentFilterProvider::bind(&session(&p), &p, height, query).unwrap();
    let (_, context) = first_dispatch(&p, None);
    assert!(route(&context).collision_width.is_none());
    assert!(route(&context).collision_height.is_none());
    assert!(route(&context).target_player_height.is_none());
    let prior = provider.observe(&context, 401, until(), || false).unwrap();
    let mut phases = Vec::new();
    assert!(provider
        .observe_chase_los_observed(
            &prior,
            &context,
            f::player(),
            403,
            until(),
            || false,
            |phase| phases.push(phase)
        )
        .is_err());
    assert!(phases.is_empty());
}
