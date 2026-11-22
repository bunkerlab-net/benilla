// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual resident primary NearPoint facts consumed by NativeWorld, followed by
//! its dry target cache and conditional Chase gate. Model/starter and unrelated
//! motion inputs remain explicitly authored; no normal-world admission is proven.
use super::super::{
    core::{
        EmptySelectorNearPointPolicy, Failure, PlayerEnvironmentPolicy, PrimaryNearPointPolicy,
        Session,
    },
    products::{
        ground_height::{allowed_ground_z, map_height},
        source_height::{CompositionError, PreparedSourceHeight},
        source_query::SourceMapQuerySnapshot,
        vmap::StaticRay,
    },
    source_route_filter::resident::{
        self,
        chase_los::{water::ResidentChaseWaterDecision, ResidentChaseLosDecision},
        near_point::NearPointObservation,
        ResidentFilterError, ResidentFilterProvider,
    },
};
use super::resident_los_tests as los;
use benilla_sim::{creature_ai as ai, creature_motion as motion, facing, near_point};
use benilla_world_runtime::native::{
    environment::LiquidCatalog, synthetic_fixture as f, synthetic_motion_fixture as authored,
    Authority, AuthorityReply, AuthorityRequest, Command, CreatureRouteRequest, Facts, Input,
    MotionFacts, MotionQuery, NativeError, NativeWorld, Orientation, OwnedInput, Profile, Query,
};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

fn until() -> Instant {
    Instant::now() + Duration::from_secs(60)
}
fn bits(p: ai::Point) -> [u32; 3] {
    [p.x.bits(), p.y.bits(), p.z.bits()]
}
fn near(context: &AuthorityRequest) -> &motion::NearPointRequest {
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &context.query else {
        panic!("NearPoint")
    };
    &near.request
}
fn near_mut(context: &mut AuthorityRequest) -> &mut motion::NearPointRequest {
    let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut context.query else {
        panic!("NearPoint")
    };
    &mut near.request
}
fn route(context: &AuthorityRequest) -> &CreatureRouteRequest {
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        panic!("Route")
    };
    route
}
fn point(facts: &Facts) -> ai::Point {
    let Facts::SourceMotion(MotionFacts::Spatial(reply)) = facts else {
        panic!("spatial facts")
    };
    let motion::Fact::NearPoint(point) = reply.fact else {
        panic!("NearPoint fact")
    };
    point
}
fn session(p: &Profile) -> Session {
    let mut session = los::session(p);
    session.primary_near_point = Some(PrimaryNearPointPolicy::from_profile(p).unwrap());
    if p.player_environment.is_some() {
        session.player_environment =
            Some(PlayerEnvironmentPolicy::from_profile(p, f::player()).unwrap());
    }
    session
}
fn pose(p: ai::Point) -> Command {
    Command::MovementPose {
        destination: p,
        orientation: Orientation::new(0.5).unwrap(),
        movement_flags: motion::GroundMovementFlags::STOPPED,
    }
}
fn step(
    world: &mut NativeWorld,
    commands: Vec<Command>,
    authority: &mut impl Authority,
) -> Result<(), NativeError> {
    let inputs = commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| OwnedInput {
            authenticated_owner: world.state().player.owner,
            input: Input {
                world: world.state().world,
                player: world.state().player,
                tick: world.state().tick + 1,
                sequence: world.state().next_sequence + index as u64,
                command,
            },
        })
        .collect();
    world
        .step_batch(world.state().tick + 1, inputs, authority)
        .map(|_| ())
}
/// The authored authority supplies initialization/combat and a labeled synthetic
/// Route only after the real source NearPoint has reached the runtime reducer.
struct RuntimeAuthority<N, E> {
    authored: authored::SyntheticMotionAuthority,
    near: N,
    environment: E,
    seen: Vec<AuthorityRequest>,
    accepted: Vec<Facts>,
}
impl<N, E> RuntimeAuthority<N, E> {
    fn new(near: N, environment: E) -> Self {
        Self {
            authored: Default::default(),
            near,
            environment,
            seen: Vec::new(),
            accepted: Vec::new(),
        }
    }
}
impl<N, E> Authority for RuntimeAuthority<N, E>
where
    N: FnMut(&AuthorityRequest) -> Result<Facts, NativeError>,
    E: FnMut(&AuthorityRequest) -> Result<Facts, NativeError>,
{
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.seen.push(request.clone());
        let facts = match &request.query {
            Query::SourceMotion(MotionQuery::NearPoint(_)) => {
                let facts = (self.near)(request)?;
                self.accepted.push(facts.clone());
                facts
            }
            Query::PlayerEnvironment(_) => (self.environment)(request)?,
            _ => return self.authored.resolve(request),
        };
        Ok(AuthorityReply {
            request: request.clone(),
            facts,
        })
    }
}
fn moved(p: &Profile, offset: f32) -> ai::Point {
    ai::Point::new(
        p.player.start.x.as_f32(),
        p.player.start.y.as_f32() + offset,
        p.player.start.z.as_f32(),
    )
    .unwrap()
}

pub(crate) fn assert_resident_primary_near_point(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    old_query: SourceMapQuerySnapshot,
    replaced_query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    catalog: &LiquidCatalog,
    mut change_source: impl FnMut(bool),
) {
    let navigation = query.binding();
    p.geometry_revision = resident::geometry_revision(height.binding(), navigation);
    p.tick_ms = 25;
    p.source_motion.as_mut().unwrap().relocation_aggro_delay_ms = 125;
    p.initialize_player_environment(
        height
            .environment_binding(p.content_digest, catalog, || false)
            .unwrap(),
        catalog.clone(),
    )
    .unwrap();
    let s = session(&p);
    let mut provider = ResidentFilterProvider::bind(&s, &p, height, query).unwrap();
    // Reconstruct the previous policy exactly; merely copying any requested
    // profile revision cannot replace the provider's actual loaded composition.
    let mut legacy = Sha256::new();
    legacy.update(b"benilla-resident-point-no-water-owner-filter-v1");
    legacy.update(height.binding().map.to_le_bytes());
    legacy.update(height.binding().revision);
    legacy.update(super::geometry_revision(navigation));
    let old: [u8; 32] = legacy.finalize().into();
    assert_ne!(old, p.geometry_revision);
    for (revision, query) in [(old, old_query), ([0xa5; 32], replaced_query)] {
        let mut changed = p.clone();
        changed.geometry_revision = revision;
        assert!(ResidentFilterProvider::bind(&session(&changed), &changed, height, query).is_err());
    }

    let environment = provider
        .player_environment(f::player(), until(), || false)
        .unwrap();
    let mut observations = Vec::new();
    let mut authority = RuntimeAuthority::new(
        |request: &AuthorityRequest| {
            Ok(provider
                .resolve_primary_near_point_observed(
                    &s,
                    request,
                    until(),
                    || false,
                    |observation| observations.push(observation.clone()),
                )
                .unwrap())
        },
        |request: &AuthorityRequest| {
            Ok(environment.resolve(&s, request, until(), || false).unwrap())
        },
    );
    let mut world =
        NativeWorld::synthetic_component_test(p.clone(), s.world, f::player(), 1234).unwrap();
    let initial = world.checkpoint().unwrap();
    step(
        &mut world,
        vec![pose(moved(&p, 1.0)), pose(moved(&p, 2.0))],
        &mut authority,
    )
    .unwrap();
    while !authority
        .seen
        .iter()
        .any(|r| matches!(r.query, Query::SourceMotion(MotionQuery::Route(_))))
    {
        assert!(
            world.state().tick < 12,
            "genuine source NearPoint never produced Chase dispatch"
        );
        step(&mut world, vec![], &mut authority).unwrap();
    }
    let context = authority
        .seen
        .iter()
        .find(|r| matches!(r.query, Query::SourceMotion(MotionQuery::NearPoint(_))))
        .unwrap()
        .clone();
    let route_context = authority
        .seen
        .iter()
        .find(|r| matches!(r.query, Query::SourceMotion(MotionQuery::Route(_))))
        .unwrap()
        .clone();
    let accepted = authority.accepted[0].clone();
    assert_eq!(
        authority
            .seen
            .iter()
            .filter(|r| matches!(r.query, Query::PlayerEnvironment(_)))
            .count(),
        1,
        "runtime countdown retains original dry observation"
    );
    drop(authority);
    let q = near(&context);
    let goal = point(&accepted);
    assert_eq!(route(&route_context).request.goal, goal);
    assert_ne!(goal, q.target.pose.position);
    assert_eq!(q.target.pose.position, moved(&p, 2.0));
    assert_eq!(
        route(&route_context).request.kind,
        motion::RouteKind::ChaseInitial
    );
    assert!(route_context
        .require_pathfinder_dispatch(&p)
        .unwrap()
        .first_construction());
    let cached = route(&route_context)
        .require_target_water(&p, s.world, f::player())
        .unwrap();
    assert!(!cached.in_water());
    let wire = serde_json::to_value(cached).unwrap();
    assert_eq!(
        wire["snapshot"]["state"]["cached_query"]["position"],
        serde_json::json!(bits(moved(&p, 1.0)))
    );
    assert_eq!(cached.target().pose.position, moved(&p, 2.0));

    assert!(observations.len() >= 2);
    let NearPointObservation::Ground {
        angle,
        request,
        height: ground,
        allowed_z,
    } = &observations[0]
    else {
        panic!("ground must precede LOS")
    };
    assert_eq!(
        *angle,
        facing::bearing_bits(
            [
                q.target.pose.position.x.bits(),
                q.target.pose.position.y.bits()
            ],
            [q.owner.position.x.bits(), q.owner.position.y.bits()]
        )
        .unwrap()
    );
    assert_eq!(*angle, 0x4096_cbe4, "pinned source negative-Y bearing");
    let xy = near_point::candidate_bits(
        [
            q.target.pose.position.x.bits(),
            q.target.pose.position.y.bits(),
        ],
        q.distance.bits(),
        *angle,
    )
    .unwrap();
    assert_eq!(
        request.position,
        [xy[0], xy[1], q.target.pose.position.z.bits()]
    );
    assert_eq!(request.binding, height.binding());
    assert_eq!(request.ticket, 1);
    assert_eq!(request.max_search, 10.0f32.to_bits());
    let raw_height = height.query(*request, until()).unwrap().height;
    let expected_ground = map_height(
        request.position.map(f32::from_bits),
        false,
        |position, enabled, search| {
            assert_eq!(position.map(f32::to_bits), request.position);
            assert!(enabled);
            assert_eq!(search.to_bits(), 10.0f32.to_bits());
            Ok(f32::from_bits(raw_height))
        },
        |_, _| Ok(f32::NEG_INFINITY),
    )
    .unwrap();
    assert_eq!(*ground, expected_ground.to_bits());
    assert_eq!(
        *allowed_z,
        allowed_ground_z(q.target.pose.position.z.as_f32(), expected_ground).to_bits()
    );
    assert_eq!(bits(goal), [xy[0], xy[1], *allowed_z]);
    let NearPointObservation::Ray(ray) = &observations[1] else {
        panic!("one initial ray follows ground")
    };
    assert_eq!(ray.request.ticket, 2);
    assert_eq!(
        ray.request.from,
        near_point::primary::target_eye_bits(
            bits(q.target.pose.position),
            q.target.collision_height.bits()
        )
        .unwrap()
    );
    assert_eq!(ray.request.to, bits(goal));
    assert!(!ray.request.ignore_m2);
    assert_eq!(ray.result, StaticRay::Miss);
    assert_ne!(
        ray.request.to[2],
        (goal.z.as_f32() + q.target.collision_height.as_f32()).to_bits()
    );
    assert_ne!(ray.request.from, bits(q.owner.position));

    // The source point has now passed the runtime reducer. Fresh point filter,
    // model-derived Chase eyes and the actual runtime cache authorize only this
    // conditional prefix; no source Route or startup certificate is created.
    let before = world.checkpoint().unwrap();
    let prior = provider
        .observe(&route_context, 601, until(), || false)
        .unwrap();
    let los = provider
        .observe_chase_los(&prior, &route_context, f::player(), 603, until(), || false)
        .unwrap();
    assert_eq!(
        provider
            .los_for(&los, &route_context, until(), || false)
            .unwrap()
            .decision,
        ResidentChaseLosDecision::ClearPrefix
    );
    let water = provider
        .observe_chase_water(&los, &route_context, 604, until(), || false)
        .unwrap();
    assert_eq!(
        provider
            .water_for(&water, &route_context, until(), || false)
            .unwrap()
            .decision,
        ResidentChaseWaterDecision::ClearConditional
    );
    assert_eq!(world.checkpoint().unwrap(), before);
    let mut replay = NativeWorld::restore_synthetic_component_test(p.clone(), initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), before);

    assert_rejected_contexts(&provider, &s, &p, &context);
    for stop in 0..2 {
        let cancelled = Cell::new(false);
        let mut count = 0;
        let result = provider.resolve_primary_near_point_observed(
            &s,
            &context,
            until(),
            || cancelled.get(),
            |_| {
                if count == stop {
                    cancelled.set(true);
                }
                count += 1;
            },
        );
        assert!(matches!(
            result,
            Err(ResidentFilterError::Source(CompositionError::Cancelled))
        ));
        assert_eq!(count, stop + 1);
        assert_eq!(
            provider
                .resolve_primary_near_point(&s, &context, until(), || false)
                .unwrap(),
            accepted
        );
    }
    assert!(matches!(
        provider.resolve_primary_near_point(&s, &context, Instant::now(), || false),
        Err(ResidentFilterError::Context(Failure::Deadline))
    ));
    assert!(matches!(
        provider.resolve_primary_near_point(&s, &context, until(), || true),
        Err(ResidentFilterError::Source(CompositionError::Cancelled))
    ));
    for stop in 0..2 {
        // Expire after a real observation, rather than merely refusing a call
        // whose deadline had already passed before geometry began.
        let expires = Instant::now() + Duration::from_secs(1);
        let mut count = 0;
        let result = provider.resolve_primary_near_point_observed(
            &s,
            &context,
            expires,
            || false,
            |_| {
                if count == stop {
                    std::thread::sleep(expires.saturating_duration_since(Instant::now()));
                }
                count += 1;
            },
        );
        assert!(matches!(
            result,
            Err(ResidentFilterError::Context(Failure::Deadline))
        ));
        assert_eq!(count, stop + 1);
    }
    for stop in 0..2 {
        let mut count = 0;
        let result = provider.resolve_primary_near_point_observed(
            &s,
            &context,
            until(),
            || false,
            |_| {
                if count == stop {
                    change_source(true);
                }
                count += 1;
            },
        );
        assert!(result.is_err(), "source mutation after observation {stop}");
        change_source(false);
        assert_eq!(
            provider
                .resolve_primary_near_point(&s, &context, until(), || false)
                .unwrap(),
            accepted
        );
    }
    change_source(true);
    let mut observed = false;
    assert!(provider
        .resolve_primary_near_point_observed(&s, &context, until(), || false, |_| observed = true)
        .is_err());
    assert!(!observed);
    change_source(false);

    // Equal serialized transcripts can be geometrically queried; that is not a
    // claim of current-frame ownership. Only NativeWorld may consume the fact.
    let decoded: AuthorityRequest =
        serde_json::from_value(serde_json::to_value(&context).unwrap()).unwrap();
    assert_eq!(
        provider
            .resolve_primary_near_point(&s, &decoded, until(), || false)
            .unwrap(),
        accepted
    );
    let mut stale_world =
        NativeWorld::synthetic_component_test(p.clone(), s.world, f::player(), 1234).unwrap();
    let mut stale = RuntimeAuthority::new(
        |request: &AuthorityRequest| {
            assert_ne!(near(request), q);
            Ok(accepted.clone())
        },
        |request: &AuthorityRequest| {
            Ok(environment.resolve(&s, request, until(), || false).unwrap())
        },
    );
    let mut failed = false;
    for tick in 0..12 {
        let checkpoint = stale_world.checkpoint().unwrap();
        let commands = if tick == 0 {
            vec![pose(moved(&p, 1.0)), pose(moved(&p, 3.0))]
        } else {
            vec![]
        };
        if step(&mut stale_world, commands, &mut stale).is_err() {
            assert_eq!(stale_world.checkpoint().unwrap(), checkpoint);
            failed = true;
            break;
        }
    }
    assert!(failed, "stale source Facts must not become a current goal");
    assert!(!stale
        .seen
        .iter()
        .any(|r| matches!(r.query, Query::SourceMotion(MotionQuery::Route(_)))));
    let failed_context = stale
        .seen
        .iter()
        .find(|r| matches!(r.query, Query::SourceMotion(MotionQuery::NearPoint(_))))
        .unwrap()
        .clone();
    drop(stale);
    let mut first_retry = true;
    let mut fresh = RuntimeAuthority::new(
        |request: &AuthorityRequest| {
            if first_retry {
                assert_eq!(
                    request, &failed_context,
                    "retry must consume the same pending runtime request"
                );
                first_retry = false;
            }
            Ok(provider
                .resolve_primary_near_point(&s, request, until(), || false)
                .unwrap())
        },
        |request: &AuthorityRequest| {
            Ok(environment.resolve(&s, request, until(), || false).unwrap())
        },
    );
    step(&mut stale_world, vec![], &mut fresh).unwrap();
    let retried_route = fresh
        .seen
        .iter()
        .find(|r| matches!(r.query, Query::SourceMotion(MotionQuery::Route(_))))
        .expect("fresh source facts must allow retry to reach its route");
    assert_eq!(route(retried_route).request.goal, point(&fresh.accepted[0]));
    assert_ne!(point(&fresh.accepted[0]), point(&accepted));
    assert_eq!(world.checkpoint().unwrap(), before);
}

fn assert_rejected_contexts(
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    s: &Session,
    p: &Profile,
    context: &AuthorityRequest,
) {
    for field in 0..8 {
        let mut wrong = s.clone();
        match field {
            0 => wrong.primary_near_point = None,
            1 => {
                wrong.empty_selector_near_point =
                    Some(EmptySelectorNearPointPolicy::from_profile(p).unwrap())
            }
            2 => {
                let mut other = p.clone();
                other.geometry_revision[0] ^= 1;
                wrong.primary_near_point =
                    Some(PrimaryNearPointPolicy::from_profile(&other).unwrap());
            }
            3 => wrong.participants = None,
            4 => wrong.world[0] ^= 1,
            5 => wrong.geometry[0] ^= 1,
            6 => wrong.bodies.get_mut(&2).unwrap().height = ai::Real::new(99.0).unwrap(),
            _ => wrong.player_environment = None,
        }
        let mut seen = false;
        assert!(
            provider
                .resolve_primary_near_point_observed(
                    &wrong,
                    context,
                    until(),
                    || false,
                    |_| seen = true
                )
                .is_err(),
            "session {field}"
        );
        assert!(!seen);
    }
    for field in 0..10 {
        let mut changed = context.clone();
        match field {
            0 => changed.binding.world[0] ^= 1,
            1 => changed.binding.geometry[0] ^= 1,
            2 => changed.binding.npc.as_mut().unwrap().generation += 1,
            3 => near_mut(&mut changed).binding.now_ms += 1,
            4 => near_mut(&mut changed).binding.query_id = 0,
            5 => near_mut(&mut changed).binding.identity.creature += 1,
            6 => near_mut(&mut changed).owner_bounding_radius = ai::Real::new(99.0).unwrap(),
            7 => near_mut(&mut changed).target.collision_height = ai::Real::new(99.0).unwrap(),
            8 => near_mut(&mut changed).target.id += 1,
            _ => near_mut(&mut changed).binding.identity.content.0[0] ^= 1,
        }
        let mut seen = false;
        assert!(
            provider
                .resolve_primary_near_point_observed(
                    s,
                    &changed,
                    until(),
                    || false,
                    |_| seen = true
                )
                .is_err(),
            "context {field}"
        );
        assert!(!seen);
    }
    for field in 0..5 {
        let mut wire = serde_json::to_value(context).unwrap();
        let searcher = &mut wire["query"]["SourceMotion"]["NearPoint"]["searcher"];
        match field {
            0 => {
                searcher["source_binding"][0] =
                    (searcher["source_binding"][0].as_u64().unwrap() ^ 1).into()
            }
            1 => searcher["inhabit_type"] = 3.into(),
            2 => searcher["capability_movement_flags"] = 0x400u32.into(),
            3 => {
                searcher["npc"]["generation"] = (context.binding.npc.unwrap().generation + 1).into()
            }
            _ => searcher["npc"]["spawn"] = 3.into(),
        }
        let changed: AuthorityRequest = serde_json::from_value(wire).unwrap();
        let mut seen = false;
        assert!(
            provider
                .resolve_primary_near_point_observed(
                    s,
                    &changed,
                    until(),
                    || false,
                    |_| seen = true
                )
                .is_err(),
            "searcher {field}"
        );
        assert!(!seen);
    }
}

/// Negative runtime path: unavailable source geometry/fallback cannot publish a
/// NearPoint fact or any subsequent Route, and the entire tick remains unchanged.
pub(crate) fn assert_resident_primary_unavailable(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    expected: Failure,
    expected_observations: usize,
) {
    p.geometry_revision = resident::geometry_revision(height.binding(), query.binding());
    let s = session(&p);
    let provider = ResidentFilterProvider::bind(&s, &p, height, query).unwrap();
    let mut observed = Vec::new();
    let mut errors = Vec::new();
    let mut authority = RuntimeAuthority::new(
        |request: &AuthorityRequest| match provider.resolve_primary_near_point_observed(
            &s,
            request,
            until(),
            || false,
            |o| observed.push(o.clone()),
        ) {
            Ok(_) => panic!("unavailable source unexpectedly published a point"),
            Err(error) => {
                errors.push(error);
                Err(NativeError::AuthorityUnavailable(
                    "resident initial point unavailable",
                ))
            }
        },
        |_: &AuthorityRequest| panic!("no environment selected"),
    );
    let mut world = NativeWorld::synthetic_component_test(p, s.world, f::player(), 1234).unwrap();
    let mut failed = false;
    for _ in 0..8 {
        let before = world.checkpoint().unwrap();
        if step(&mut world, vec![], &mut authority).is_err() {
            assert_eq!(world.checkpoint().unwrap(), before);
            failed = true;
            break;
        }
    }
    assert!(failed);
    assert!(authority.accepted.is_empty());
    assert!(!authority
        .seen
        .iter()
        .any(|r| matches!(r.query, Query::SourceMotion(MotionQuery::Route(_)))));
    drop(authority);
    assert_eq!(observed.len(), expected_observations);
    assert!(
        matches!(&errors[..], [ResidentFilterError::Context(error)] if *error == expected),
        "{errors:?}"
    );
    if expected_observations == 2 {
        assert!(
            matches!(&observed[1], NearPointObservation::Ray(ray) if matches!(ray.result, StaticRay::Hit { .. }))
        );
    }
}
