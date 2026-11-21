// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual consumed-source dry replies and NativeWorld cache lifecycle over authored
//! geometry/model inputs. Wet replies remain an explicitly authored complement.
//! No install provenance, route conversion or gameplay admission.
use super::super::{
    core::{Failure, PlayerEnvironmentPolicy, Session},
    products::{
        source_height::{CompositionError, PreparedSourceHeight},
        source_query::SourceMapQuerySnapshot,
    },
    source_player_environment::resident::{
        ResidentEnvironmentError, ResidentPlayerEnvironmentProvider,
    },
    source_route_filter::resident::{
        self,
        chase_los::{
            water::ResidentChaseWaterDecision as Decision, ResidentChaseLosDecision as LosDecision,
        },
        ResidentFilterError, ResidentFilterProvider,
    },
};
use super::resident_los_tests::{self as los, AuthoredGoal};
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world_runtime::native::{
    environment::{BoundLiquidResult, LiquidCatalog, LiquidData, LiquidStatus},
    synthetic_fixture as f, synthetic_motion_fixture as motion_fixture, synthetic_slow_fixture,
    Authority, AuthorityReply, AuthorityRequest, Command, CreatureRouteRequest, Event, Facts,
    Input, MotionQuery, NativeError, NativeWorld, Orientation, OwnedInput, PlayerEnvironmentFacts,
    PlayerEnvironmentMoment, PlayerEnvironmentPhase, Profile, Query,
};
use std::{
    cell::Cell,
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

struct EnvironmentAuthority<F> {
    motion: motion_fixture::SyntheticMotionAuthority,
    seen: Vec<AuthorityRequest>,
    environment: F,
}
impl<F> EnvironmentAuthority<F> {
    fn new(environment: F) -> Self {
        Self {
            motion: motion_fixture::SyntheticMotionAuthority::default(),
            seen: Vec::new(),
            environment,
        }
    }
}
impl<F> Authority for EnvironmentAuthority<F>
where
    F: FnMut(&AuthorityRequest) -> Result<Facts, NativeError>,
{
    fn resolve(&mut self, context: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.seen.push(context.clone());
        if matches!(context.query, Query::PlayerEnvironment(_)) {
            return Ok(AuthorityReply {
                request: context.clone(),
                facts: (self.environment)(context)?,
            });
        }
        self.motion.resolve(context)
    }
}

fn environment_session(p: &Profile) -> Session {
    let mut s = los::session(p);
    s.player_environment = Some(PlayerEnvironmentPolicy::from_profile(p, f::player()).unwrap());
    s
}

fn authored_environment(
    context: &AuthorityRequest,
    wet_entry: Option<u32>,
) -> Result<Facts, NativeError> {
    let Query::PlayerEnvironment(query) = &context.query else {
        panic!("expected player environment query")
    };
    Ok(Facts::PlayerEnvironment(PlayerEnvironmentFacts::new(
        query,
        BoundLiquidResult {
            query: query.liquid_query(),
            status: if wet_entry.is_some() {
                LiquidStatus::InWater
            } else {
                LiquidStatus::NoWater
            },
            data: wet_entry.map(|entry| LiquidData {
                entry,
                type_flags: 8,
                level: (query.pose().position.z.as_f32() + 1.0).to_bits(),
                depth_level: (query.pose().position.z.as_f32() - 10.0).to_bits(),
            }),
        },
    )?))
}
fn pose(position: ai::Point, orientation: f32) -> Command {
    Command::MovementPose {
        destination: position,
        orientation: Orientation::new(orientation).unwrap(),
        movement_flags: motion::GroundMovementFlags::STOPPED,
    }
}
fn try_step(
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
fn step(world: &mut NativeWorld, commands: Vec<Command>, authority: &mut impl Authority) {
    try_step(world, commands, authority).unwrap();
}
fn environment_count<F>(authority: &EnvironmentAuthority<F>) -> usize {
    authority
        .seen
        .iter()
        .filter(|request| matches!(request.query, Query::PlayerEnvironment(_)))
        .count()
}

/// NativeWorld alone creates the cache, countdown receipts and route witness.
fn cached_dispatch(
    p: &Profile,
    before_countdown: bool,
    height: &PreparedSourceHeight<'_, '_, '_>,
) -> (NativeWorld, AuthorityRequest) {
    let session = environment_session(p);
    let provider =
        ResidentPlayerEnvironmentProvider::bind(&session, p, f::player(), height, until(), || {
            false
        })
        .unwrap();
    cached_dispatch_with(p, before_countdown, false, |request| {
        Ok(provider
            .resolve(&session, request, until(), || false)
            .unwrap())
    })
}

/// Explicitly authored wet complement and mismatched-binding consumer tests.
fn cached_dispatch_authored(
    p: &Profile,
    before_countdown: bool,
    wet_entry: Option<u32>,
) -> (NativeWorld, AuthorityRequest) {
    cached_dispatch_with(p, before_countdown, wet_entry.is_some(), |request| {
        authored_environment(request, wet_entry)
    })
}
fn cached_dispatch_with(
    p: &Profile,
    before_countdown: bool,
    wet: bool,
    environment: impl FnMut(&AuthorityRequest) -> Result<Facts, NativeError>,
) -> (NativeWorld, AuthorityRequest) {
    let mut world =
        NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), 1234).unwrap();
    let initial = world.checkpoint().unwrap();
    let mut authority = EnvironmentAuthority::new(environment);
    // Both poses really relocate. The first move schedules the notify callback;
    // the second must retain its older query position within the countdown.
    let first = ai::Point::new(
        p.player.start.x.as_f32(),
        p.player.start.y.as_f32() + 1.0,
        p.player.start.z.as_f32(),
    )
    .unwrap();
    let second =
        ai::Point::new(first.x.as_f32(), first.y.as_f32() + 1.0, first.z.as_f32()).unwrap();
    if before_countdown {
        authority.motion.visits = Some(false);
        for _ in 0..8 {
            step(&mut world, vec![], &mut authority);
        }
        step(&mut world, vec![pose(first, 0.0)], &mut authority);
        for _ in 0..3 {
            step(&mut world, vec![], &mut authority);
        }
        assert_eq!(
            world
                .state()
                .player_environment()
                .unwrap()
                .environment()
                .timer_ms(),
            0
        );
        authority.motion.visits = Some(true);
        step(&mut world, vec![pose(first, 0.0)], &mut authority);
        assert_eq!(environment_count(&authority), 2);
    } else {
        step(
            &mut world,
            vec![pose(first, 0.0), pose(second, 0.5)],
            &mut authority,
        );
        assert_eq!(environment_count(&authority), 1);
        assert_eq!(
            world
                .state()
                .player_environment()
                .unwrap()
                .environment()
                .timer_ms(),
            75
        );
        while !authority
            .seen
            .iter()
            .any(|request| matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))))
        {
            assert!(
                world.state().tick < 12,
                "missing genuine first Chase dispatch"
            );
            step(&mut world, vec![], &mut authority);
        }
        assert_eq!(
            environment_count(&authority),
            1,
            "countdown and Chase must not refresh target geometry"
        );
    }
    let context = authority
        .seen
        .into_iter()
        .find(|request| matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))))
        .unwrap();
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
    let target = route(&context)
        .require_target_water(p, [8; 16], f::player())
        .unwrap();
    assert_eq!(target.in_water(), wet);
    assert_eq!(
        target.moment(),
        if before_countdown {
            PlayerEnvironmentMoment::BeforeCountdown
        } else {
            PlayerEnvironmentMoment::AfterCountdown
        }
    );
    let record = world
        .records()
        .iter()
        .find(|record| record.tick == target.tick())
        .unwrap();
    let receipt = record
        .events
        .iter()
        .filter_map(|event| match event {
            Event::PlayerEnvironment(receipt) => Some(receipt),
            _ => None,
        })
        .find(|receipt| receipt.after == target.cache_digest())
        .unwrap();
    assert_eq!(receipt.sequence, target.phase_sequence());
    assert!(matches!(
        (&receipt.phase, before_countdown),
        (
            PlayerEnvironmentPhase::Position {
                refreshed: true,
                ..
            },
            true
        ) | (PlayerEnvironmentPhase::Update { diff_ms: 25 }, false)
    ));
    let saved = serde_json::to_value(target).unwrap();
    assert_eq!(
        saved["snapshot"]["state"]["cached_query"]["position"],
        serde_json::json!(bits(first))
    );
    assert_eq!(
        target.target().pose.position,
        if before_countdown { first } else { second }
    );
    if !before_countdown {
        assert_ne!(
            saved["snapshot"]["state"]["cached_query"]["position"],
            serde_json::json!(bits(target.target().pose.position))
        );
        assert_eq!(saved["snapshot"]["state"]["timer_ms"], 0);
    }
    let mut replay = NativeWorld::restore_synthetic_component_test(p.clone(), initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
    (world, context)
}

fn with_environment(
    mut p: Profile,
    query: &SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    catalog: &LiquidCatalog,
) -> Profile {
    p.geometry_revision = resident::geometry_revision(height.binding(), query.binding());
    p.tick_ms = 25;
    p.source_motion.as_mut().unwrap().relocation_aggro_delay_ms = 125;
    let binding = height
        .environment_binding(p.content_digest, catalog, || false)
        .unwrap();
    p.initialize_player_environment(binding, catalog.clone())
        .unwrap();
    p
}

pub(crate) fn assert_resident_chase_water(
    p: Profile,
    query: SourceMapQuerySnapshot,
    replacement: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    catalog: &LiquidCatalog,
    mut change_source: impl FnMut(bool),
) {
    let p = with_environment(p, &query, height, catalog);
    let s = los::session(&p);
    let mut provider = ResidentFilterProvider::bind(&s, &p, height, query).unwrap();
    let other = ResidentFilterProvider::bind(&s, &p, height, replacement).unwrap();
    let (world, mut context) = cached_dispatch(&p, false, height);
    route_mut(&mut context).owner_ignore_pathfinding = None;
    let before = world.checkpoint().unwrap();
    let prior = provider.observe(&context, 501, until(), || false).unwrap();
    let prefix = provider
        .observe_chase_los(&prior, &context, f::player(), 503, until(), || false)
        .unwrap();
    assert_eq!(
        provider
            .los_for(&prefix, &context, until(), || false)
            .unwrap()
            .decision,
        LosDecision::ClearPrefix
    );
    let mut phases = Vec::new();
    let water = provider
        .observe_chase_water_observed(
            &prefix,
            &context,
            504,
            until(),
            || false,
            |phase| phases.push(phase),
        )
        .unwrap();
    assert_eq!(phases, [0, 1, 2]);
    let observed = provider
        .water_for(&water, &context, until(), || false)
        .unwrap();
    assert_eq!(observed.decision, Decision::ClearConditional);
    let owner = observed.owner.unwrap().request();
    assert_eq!(owner.ticket, 504);
    assert_eq!(owner.mask, 15);
    assert_eq!(owner.position, bits(route(&context).owner_position));
    assert_eq!(owner.binding, height.binding());
    assert_eq!(
        observed.owner_collision_height_bits,
        Some(benilla_sim::chase_gate::LIQUID_COLLISION_HEIGHT_BITS)
    );
    assert_eq!(
        observed.target.as_ref(),
        route(&context).target_player_water.as_ref()
    );
    assert_eq!(
        prior.observations().map(|reply| reply.request().ticket),
        [501, 502]
    );
    assert!(prior
        .observations()
        .iter()
        .all(|reply| reply.request() != owner));
    assert_eq!(provider.retained_constructor_count(), 1);
    assert_eq!(world.checkpoint().unwrap(), before);

    let decoded: AuthorityRequest =
        serde_json::from_value(serde_json::to_value(&context).unwrap()).unwrap();
    let (_, mut independent) = cached_dispatch(&p, false, height);
    route_mut(&mut independent).owner_ignore_pathfinding = None;
    assert_eq!(independent, context);
    let mut invalid = vec![decoded, independent];
    for edit in 0..7 {
        let mut changed = context.clone();
        match edit {
            0 => changed.binding.world[0] ^= 1,
            1 => changed.binding.tick += 1,
            2 => changed.binding.npc.as_mut().unwrap().generation += 1,
            3 => route_mut(&mut changed).collision_height = None,
            4 => route_mut(&mut changed).target_player_height = None,
            5 => route_mut(&mut changed).target_player_water = None,
            _ => {
                route_mut(&mut changed).request.goal.x =
                    ai::Real::new(route(&context).request.goal.x.as_f32() + 1.0).unwrap()
            }
        }
        invalid.push(changed);
    }
    for invalid in invalid {
        let mut phases = Vec::new();
        assert!(provider
            .observe_chase_water_observed(
                &prefix,
                &invalid,
                505,
                until(),
                || false,
                |phase| phases.push(phase)
            )
            .is_err());
        assert!(phases.is_empty());
        assert!(provider
            .water_for(&water, &invalid, until(), || false)
            .is_err());
    }
    assert!(other
        .observe_chase_water(&prefix, &context, 505, until(), || false)
        .is_err());
    assert!(other
        .water_for(&water, &context, until(), || false)
        .is_err());
    for ticket in [0, 501, 502, 503] {
        let mut phases = Vec::new();
        assert!(provider
            .observe_chase_water_observed(
                &prefix,
                &context,
                ticket,
                until(),
                || false,
                |phase| phases.push(phase)
            )
            .is_err());
        assert_eq!(phases, [0]);
    }

    // Every altered target still has fresh filter and LOS evidence for that exact
    // context. The water witness itself fails only after fresh owner observation.
    for field in [
        "missing",
        "water",
        "sequence",
        "digest",
        "target",
        "source",
        "tick",
        "moment",
        "query_height",
        "query_position",
        "snapshot",
    ] {
        let mut changed = context.clone();
        if field == "missing" {
            route_mut(&mut changed).target_player_water = None;
        } else {
            let mut value =
                serde_json::to_value(route(&context).target_player_water.as_ref().unwrap())
                    .unwrap();
            match field {
                "water" => value["in_water"] = true.into(),
                "sequence" => value["snapshot"]["sequence"] = 0.into(),
                "digest" => value["digest"][0] = 99.into(),
                "target" => value["target"]["pose"]["position"]["x"] = 7.0_f32.to_bits().into(),
                "source" => value["snapshot"]["owner"]["source"][0] = 99.into(),
                "tick" => value["tick"] = 999.into(),
                "moment" => value["moment"] = "BeforeCountdown".into(),
                "query_height" => {
                    value["snapshot"]["state"]["cached_query"]["collision_height"] = 99.into()
                }
                "query_position" => {
                    value["snapshot"]["state"]["cached_query"]["position"][0] =
                        7.0_f32.to_bits().into()
                }
                "snapshot" => {
                    value["snapshot"]["state"]["cached_query"]["snapshot"]["source_epoch"][0] =
                        99.into()
                }
                _ => unreachable!(),
            }
            route_mut(&mut changed).target_player_water =
                Some(serde_json::from_value(value).unwrap());
        }
        let fresh = provider.observe(&changed, 511, until(), || false).unwrap();
        let fresh_prefix = provider
            .observe_chase_los(&fresh, &changed, f::player(), 513, until(), || false)
            .unwrap();
        let mut phases = Vec::new();
        assert!(
            provider
                .observe_chase_water_observed(
                    &fresh_prefix,
                    &changed,
                    514,
                    until(),
                    || false,
                    |phase| phases.push(phase)
                )
                .is_err(),
            "{field}"
        );
        assert_eq!(phases, [0, 1], "owner before cached target: {field}");
    }

    for stop in 0..=2 {
        let cancelled = Cell::new(false);
        let mut phases = Vec::new();
        let result = provider.observe_chase_water_observed(
            &prefix,
            &context,
            521,
            until(),
            || cancelled.get(),
            |phase| {
                phases.push(phase);
                if phase == stop {
                    cancelled.set(true);
                }
            },
        );
        assert!(matches!(
            result,
            Err(ResidentFilterError::Source(CompositionError::Cancelled))
        ));
        assert_eq!(phases.last(), Some(&stop));
        assert_eq!(
            provider
                .water_for(&water, &context, until(), || false)
                .unwrap(),
            observed
        );
        let mut reached = false;
        let result = provider.observe_chase_water_observed(
            &prefix,
            &context,
            522,
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
        assert!(result.is_err(), "source change at phase {stop}");
        assert!(provider
            .water_for(&water, &context, until(), || false)
            .is_err());
        change_source(false);
        let retry = provider
            .observe_chase_water(&prefix, &context, 523, until(), || false)
            .unwrap();
        assert_eq!(
            provider
                .water_for(&retry, &context, until(), || false)
                .unwrap()
                .decision,
            Decision::ClearConditional
        );
    }
    assert!(matches!(
        provider.observe_chase_water(&prefix, &context, 531, Instant::now(), || false),
        Err(ResidentFilterError::Context(Failure::Deadline))
    ));
    assert!(matches!(
        provider.water_for(&water, &context, Instant::now(), || false),
        Err(ResidentFilterError::Context(Failure::Deadline))
    ));
    assert!(matches!(
        provider.water_for(&water, &context, until(), || true),
        Err(ResidentFilterError::Source(CompositionError::Cancelled))
    ));
    let expires = Instant::now() + Duration::from_secs(1);
    let short_prior = provider.observe(&context, 541, expires, || false).unwrap();
    let short_prefix = provider
        .observe_chase_los(&short_prior, &context, f::player(), 543, until(), || false)
        .unwrap();
    let short_water = provider
        .observe_chase_water(&short_prefix, &context, 544, until(), || false)
        .unwrap();
    std::thread::sleep(expires.saturating_duration_since(Instant::now()));
    assert!(matches!(
        provider.water_for(&short_water, &context, until(), || false),
        Err(ResidentFilterError::Context(Failure::Deadline))
    ));
    let mut phases = Vec::new();
    assert!(provider
        .observe_chase_water_observed(
            &short_prefix,
            &context,
            545,
            until(),
            || false,
            |phase| phases.push(phase)
        )
        .is_err());
    assert!(phases.is_empty());
    assert_eq!(world.checkpoint().unwrap(), before);
}

pub(crate) fn assert_resident_water_before_countdown(
    p: Profile,
    query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    catalog: &LiquidCatalog,
) {
    let p = with_environment(p, &query, height, catalog);
    let mut provider = ResidentFilterProvider::bind(&los::session(&p), &p, height, query).unwrap();
    let (world, context) = cached_dispatch(&p, true, height);
    let before = world.checkpoint().unwrap();
    let prior = provider.observe(&context, 551, until(), || false).unwrap();
    let prefix = provider
        .observe_chase_los(&prior, &context, f::player(), 553, until(), || false)
        .unwrap();
    let water = provider
        .observe_chase_water(&prefix, &context, 554, until(), || false)
        .unwrap();
    let observed = provider
        .water_for(&water, &context, until(), || false)
        .unwrap();
    assert_eq!(observed.decision, Decision::ClearConditional);
    assert_eq!(
        observed.target.unwrap().moment(),
        PlayerEnvironmentMoment::BeforeCountdown
    );
    assert_eq!(world.checkpoint().unwrap(), before);
}

pub(crate) fn assert_resident_water_binding_rejected(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    catalog: &LiquidCatalog,
    field: &str,
) {
    p.geometry_revision = resident::geometry_revision(height.binding(), query.binding());
    p.tick_ms = 25;
    let mut binding = height
        .environment_binding(p.content_digest, catalog, || false)
        .unwrap();
    match field {
        "epoch" => binding.source_epoch[0] ^= 1,
        "inventory" => binding.inventory[0] ^= 1,
        "policy" => binding.policy[0] ^= 1,
        "missing" => {}
        _ => unreachable!(),
    }
    if field != "missing" {
        p.initialize_player_environment(binding, catalog.clone())
            .unwrap();
    }
    let mut environment_session = los::session(&p);
    if field != "missing" {
        environment_session.player_environment =
            Some(PlayerEnvironmentPolicy::from_profile(&p, f::player()).unwrap());
    }
    assert!(
        ResidentPlayerEnvironmentProvider::bind(
            &environment_session,
            &p,
            f::player(),
            height,
            until(),
            || false
        )
        .is_err(),
        "resident environment binding: {field}"
    );
    let mut provider = ResidentFilterProvider::bind(&los::session(&p), &p, height, query).unwrap();
    let (world, context) = if field == "missing" {
        los::first_dispatch(&p, None)
    } else {
        cached_dispatch_authored(&p, false, None)
    };
    let before = world.checkpoint().unwrap();
    let prior = provider.observe(&context, 561, until(), || false).unwrap();
    let prefix = provider
        .observe_chase_los(&prior, &context, f::player(), 563, until(), || false)
        .unwrap();
    let mut phases = Vec::new();
    assert!(
        provider
            .observe_chase_water_observed(
                &prefix,
                &context,
                564,
                until(),
                || false,
                |phase| phases.push(phase)
            )
            .is_err(),
        "{field}"
    );
    assert_eq!(phases, [0], "metadata before owner and target: {field}");
    assert_eq!(world.checkpoint().unwrap(), before);
}

pub(crate) fn assert_resident_water_prefix_rejected(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    goal: [f32; 3],
    expected: LosDecision,
) {
    p.geometry_revision = resident::geometry_revision(height.binding(), query.binding());
    assert!(p.player_environment.is_none());
    let mut provider = ResidentFilterProvider::bind(&los::session(&p), &p, height, query).unwrap();
    let goal = if matches!(expected, LosDecision::Distance | LosDecision::Vertical) {
        AuthoredGoal::OwnerOffset(goal)
    } else {
        AuthoredGoal::Absolute(ai::Point::new(goal[0], goal[1], goal[2]).unwrap())
    };
    let (world, mut context) = los::first_dispatch(&p, Some(goal));
    route_mut(&mut context).target_player_water = None;
    route_mut(&mut context).owner_ignore_pathfinding = None;
    let before = world.checkpoint().unwrap();
    let prior = provider.observe(&context, 571, until(), || false).unwrap();
    let prefix = provider
        .observe_chase_los(&prior, &context, f::player(), 573, until(), || false)
        .unwrap();
    assert_eq!(
        provider
            .los_for(&prefix, &context, until(), || false)
            .unwrap()
            .decision,
        expected
    );
    let mut phases = Vec::new();
    // Missing environment/cache and even an invalid unused water ticket must not
    // interfere with an earlier distance, vertical or blocked-LOS decision.
    let water = provider
        .observe_chase_water_observed(
            &prefix,
            &context,
            0,
            until(),
            || false,
            |phase| phases.push(phase),
        )
        .unwrap();
    assert_eq!(phases, [0]);
    let observed = provider
        .water_for(&water, &context, until(), || false)
        .unwrap();
    assert_eq!(observed.decision, Decision::PrefixRejected(expected));
    assert!(observed.owner.is_none());
    assert!(observed.owner_collision_height_bits.is_none());
    assert!(observed.target.is_none());
    assert_eq!(world.checkpoint().unwrap(), before);
}

pub(crate) fn assert_resident_water_cached_target_is_wet(
    p: Profile,
    query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    catalog: &LiquidCatalog,
    entry: u32,
) {
    let mut p = with_environment(p, &query, height, catalog);
    // Existing authored callback-source loaders establish the closed native surface
    // host. No slow effects are scheduled and no cache fields are assigned here.
    p.player_slow = synthetic_slow_fixture::with_effects(vec![]).player_slow;
    p.initialize_player_environment_surface_callbacks().unwrap();
    let row = catalog.get(entry).unwrap();
    assert_eq!((row.kind, row.spell_id), (3, 0));
    let mut provider = ResidentFilterProvider::bind(&los::session(&p), &p, height, query).unwrap();
    // The authority's wet target reply is explicitly authored. Resident geometry
    // still proves only the fresh dry owner point; it never samples target feet.
    let (world, context) = cached_dispatch_authored(&p, false, Some(entry));
    let before = world.checkpoint().unwrap();
    let prior = provider.observe(&context, 581, until(), || false).unwrap();
    let prefix = provider
        .observe_chase_los(&prior, &context, f::player(), 583, until(), || false)
        .unwrap();
    let mut phases = Vec::new();
    let water = provider
        .observe_chase_water_observed(
            &prefix,
            &context,
            584,
            until(),
            || false,
            |phase| phases.push(phase),
        )
        .unwrap();
    assert_eq!(phases, [0, 1, 2]);
    let observed = provider
        .water_for(&water, &context, until(), || false)
        .unwrap();
    assert_eq!(observed.decision, Decision::TargetWater);
    assert_eq!(observed.owner.unwrap().request().mask, 15);
    assert!(observed.target.unwrap().in_water());
    assert_eq!(world.checkpoint().unwrap(), before);
}

fn captured_environment(p: &Profile) -> (NativeWorld, AuthorityRequest) {
    let mut world =
        NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), 1234).unwrap();
    let before = world.checkpoint().unwrap();
    let mut authority = EnvironmentAuthority::new(|_: &AuthorityRequest| {
        Err(NativeError::AuthorityUnavailable(
            "capture exact environment query",
        ))
    });
    assert!(try_step(&mut world, vec![pose(p.player.start, 0.0)], &mut authority).is_err());
    assert_eq!(world.checkpoint().unwrap(), before);
    let request = authority
        .seen
        .into_iter()
        .find(|r| matches!(r.query, Query::PlayerEnvironment(_)))
        .unwrap();
    (world, request)
}

fn refused_environment(
    p: &Profile,
    mut resolve: impl FnMut(&AuthorityRequest) -> Result<Facts, ResidentEnvironmentError>,
) -> ResidentEnvironmentError {
    let mut world =
        NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), 1234).unwrap();
    let before = world.checkpoint().unwrap();
    let records = world.records().to_vec();
    let mut error = None;
    let mut authority = EnvironmentAuthority::new(|request: &AuthorityRequest| {
        error = Some(resolve(request).expect_err("refused source must not publish Facts"));
        Err(NativeError::AuthorityUnavailable(
            "resident environment refused",
        ))
    });
    assert!(try_step(&mut world, vec![pose(p.player.start, 0.0)], &mut authority).is_err());
    assert_eq!(
        world.checkpoint().unwrap(),
        before,
        "failed refresh must not mutate cache or movement"
    );
    assert_eq!(world.records(), records);
    error.unwrap()
}

pub(crate) fn assert_resident_environment_reply(
    p: Profile,
    query: SourceMapQuerySnapshot,
    height: &PreparedSourceHeight<'_, '_, '_>,
    catalog: &LiquidCatalog,
    mut change_source: impl FnMut(bool),
) {
    let p = with_environment(p, &query, height, catalog);
    let session = environment_session(&p);
    let provider =
        ResidentPlayerEnvironmentProvider::bind(&session, &p, f::player(), height, until(), || {
            false
        })
        .unwrap();
    let (mut world, request) = captured_environment(&p);
    let initial = world.checkpoint().unwrap();
    let mut phases = Vec::new();
    let facts = provider
        .resolve_observed(
            &session,
            &request,
            until(),
            || false,
            |phase| phases.push(phase),
        )
        .unwrap();
    assert_eq!(phases, [0, 1]);
    let Facts::PlayerEnvironment(environment) = &facts else {
        panic!("environment Facts")
    };
    let Query::PlayerEnvironment(exact) = &request.query else {
        panic!("environment query")
    };
    assert_eq!(environment.query(), exact.as_ref());
    assert_eq!(
        environment.result(),
        BoundLiquidResult {
            query: exact.liquid_query(),
            status: LiquidStatus::NoWater,
            data: None,
        }
    );
    assert_eq!(exact.liquid_query().required_type, 15);
    assert_eq!(exact.liquid_query().ticket, exact.phase_sequence());
    assert_eq!(exact.liquid_query().position, bits(exact.pose().position));
    assert_eq!(
        exact.liquid_query().collision_height,
        exact.height_witness().height().bits()
    );
    exact
        .height_witness()
        .validate_for(&p, session.world, f::player())
        .unwrap();
    assert_eq!(
        world.checkpoint().unwrap(),
        initial,
        "a reply itself never updates the runtime"
    );

    // Serialized historical queries remain answerable; no fabricated live-frame claim.
    let historical: AuthorityRequest =
        serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
    assert_eq!(
        provider
            .resolve(&session, &historical, until(), || false)
            .unwrap(),
        facts
    );
    for field in ["world", "profile", "policy", "geometry", "tick", "npc"] {
        let mut changed = request.clone();
        match field {
            "world" => changed.binding.world[0] ^= 1,
            "profile" => changed.binding.profile[0] ^= 1,
            "policy" => changed.binding.authority_policy[0] ^= 1,
            "geometry" => changed.binding.geometry[0] ^= 1,
            "tick" => changed.binding.tick = 0,
            "npc" => {
                let (_, context) = cached_dispatch(&p, false, height);
                changed.binding.npc = context.binding.npc;
            }
            _ => unreachable!(),
        }
        let mut phases = Vec::new();
        assert!(
            provider
                .resolve_observed(
                    &session,
                    &changed,
                    until(),
                    || false,
                    |phase| phases.push(phase)
                )
                .is_err(),
            "{field}"
        );
        assert!(
            phases.is_empty(),
            "invalid context must precede source sampling: {field}"
        );
    }
    for field in [
        "world",
        "player",
        "height",
        "height_source",
        "source",
        "epoch",
        "inventory",
        "policy",
        "catalog",
        "sequence",
        "ticket",
        "before",
        "pose",
        "position",
        "mask",
        "query_height",
    ] {
        let mut value = serde_json::to_value(exact).unwrap();
        match field {
            "world" => value["owner"]["height"]["world"][0] = 99.into(),
            "player" => value["owner"]["height"]["player"]["character"][0] = 99.into(),
            "height" => value["owner"]["height"]["height"] = 1.0_f32.to_bits().into(),
            "height_source" => value["owner"]["height"]["source_binding"][0] = 99.into(),
            "source" => value["owner"]["source"][0] = 99.into(),
            "epoch" => value["query"]["snapshot"]["source_epoch"][0] = 99.into(),
            "inventory" | "policy" => value["query"]["snapshot"][field][0] = 99.into(),
            "catalog" => value["query"]["snapshot"]["liquid_catalog"][0] = 99.into(),
            "sequence" => value["sequence"] = 0.into(),
            "ticket" => value["query"]["ticket"] = 99.into(),
            "before" => value["before"] = serde_json::to_value([0u8; 32]).unwrap(),
            "pose" => value["pose"]["position"]["x"] = 7.0_f32.to_bits().into(),
            "position" => value["query"]["position"][0] = 7.0_f32.to_bits().into(),
            "mask" => value["query"]["required_type"] = 10.into(),
            "query_height" => value["query"]["collision_height"] = 1.0_f32.to_bits().into(),
            _ => unreachable!(),
        }
        let mut changed = request.clone();
        changed.query = Query::PlayerEnvironment(Box::new(serde_json::from_value(value).unwrap()));
        let mut phases = Vec::new();
        assert!(
            provider
                .resolve_observed(
                    &session,
                    &changed,
                    until(),
                    || false,
                    |phase| phases.push(phase)
                )
                .is_err(),
            "{field}"
        );
        assert!(
            phases.is_empty(),
            "invalid query must precede source sampling: {field}"
        );
    }
    for field in [
        "world",
        "map",
        "player",
        "height",
        "participants",
        "selection",
    ] {
        let mut changed = session.clone();
        match field {
            "world" => changed.world[0] ^= 1,
            "map" => changed.map += 1,
            "player" => changed.player = None,
            "height" => changed.player.as_mut().unwrap().body.height = ai::Real::new(9.0).unwrap(),
            "participants" => changed.participants = None,
            "selection" => changed.player_environment = None,
            _ => unreachable!(),
        }
        assert!(
            provider
                .resolve(&changed, &request, until(), || false)
                .is_err(),
            "{field}"
        );
        if field != "world" {
            assert!(
                ResidentPlayerEnvironmentProvider::bind(
                    &changed,
                    &p,
                    f::player(),
                    height,
                    until(),
                    || false
                )
                .is_err(),
                "{field}"
            );
        }
    }
    let mut wrong_player = f::player();
    wrong_player.character[0] ^= 1;
    assert!(ResidentPlayerEnvironmentProvider::bind(
        &session,
        &p,
        wrong_player,
        height,
        until(),
        || false
    )
    .is_err());
    assert!(matches!(
        ResidentPlayerEnvironmentProvider::bind(
            &session,
            &p,
            f::player(),
            height,
            Instant::now(),
            || false
        ),
        Err(ResidentEnvironmentError::Context(Failure::Deadline))
    ));
    assert!(matches!(
        ResidentPlayerEnvironmentProvider::bind(&session, &p, f::player(), height, until(), || {
            true
        }),
        Err(ResidentEnvironmentError::Source(
            CompositionError::Cancelled
        ))
    ));
    assert!(matches!(
        refused_environment(&p, |current| provider.resolve(
            &session,
            current,
            Instant::now(),
            || false
        )),
        ResidentEnvironmentError::Context(Failure::Deadline)
    ));
    assert!(matches!(
        refused_environment(&p, |current| provider.resolve(
            &session,
            current,
            until(),
            || true
        )),
        ResidentEnvironmentError::Source(CompositionError::Cancelled)
    ));
    for stop in [0, 1] {
        let cancelled = Cell::new(false);
        let mut phases = Vec::new();
        assert!(matches!(
            refused_environment(&p, |current| provider.resolve_observed(
                &session,
                current,
                until(),
                || cancelled.get(),
                |phase| {
                    phases.push(phase);
                    if phase == stop {
                        cancelled.set(true);
                    }
                }
            )),
            ResidentEnvironmentError::Source(CompositionError::Cancelled)
        ));
        assert_eq!(phases.last(), Some(&stop));
        let mut reached = false;
        let error = refused_environment(&p, |current| {
            provider.resolve_observed(
                &session,
                current,
                until(),
                || false,
                |phase| {
                    if phase == stop {
                        reached = true;
                        change_source(true);
                    }
                },
            )
        });
        assert!(reached);
        assert!(matches!(error, ResidentEnvironmentError::Source(_)));
        assert!(ResidentPlayerEnvironmentProvider::bind(
            &session,
            &p,
            f::player(),
            height,
            until(),
            || false
        )
        .is_err());
        change_source(false);
        assert_eq!(
            provider
                .resolve(&session, &request, until(), || false)
                .unwrap(),
            facts
        );
    }
    let expires = Instant::now() + Duration::from_millis(250);
    let mut reached = false;
    assert!(matches!(
        refused_environment(&p, |current| provider.resolve_observed(
            &session,
            current,
            expires,
            || false,
            |phase| {
                if phase == 1 {
                    reached = true;
                    std::thread::sleep(expires.saturating_duration_since(Instant::now()));
                }
            }
        )),
        ResidentEnvironmentError::Context(Failure::Deadline)
    ));
    assert!(
        reached,
        "deadline expires after genuine point reply construction"
    );

    // Only the reducer knows which transcript belongs to its current cache/pose.
    let mut actual = EnvironmentAuthority::new(|current: &AuthorityRequest| {
        Ok(provider
            .resolve(&session, current, until(), || false)
            .unwrap())
    });
    step(&mut world, vec![pose(p.player.start, 0.0)], &mut actual);
    while world
        .state()
        .player_environment()
        .unwrap()
        .environment()
        .timer_ms()
        != 0
    {
        step(&mut world, vec![], &mut actual);
    }
    assert_eq!(
        provider
            .resolve(&session, &historical, until(), || false)
            .unwrap(),
        facts
    );
    let checkpoint = world.checkpoint().unwrap();
    let records = world.records().to_vec();
    let moved = ai::Point::new(
        p.player.start.x.as_f32(),
        p.player.start.y.as_f32() + 1.0,
        p.player.start.z.as_f32(),
    )
    .unwrap();
    let mut stale = EnvironmentAuthority::new(|current: &AuthorityRequest| {
        assert_ne!(*current, request);
        let Query::PlayerEnvironment(current) = &current.query else {
            panic!()
        };
        assert_ne!(current.phase_sequence(), exact.phase_sequence());
        assert_ne!(current.pose(), exact.pose());
        Ok(facts.clone())
    });
    assert!(try_step(&mut world, vec![pose(moved, 0.5)], &mut stale).is_err());
    assert_eq!(environment_count(&stale), 1);
    assert_eq!(world.checkpoint().unwrap(), checkpoint);
    assert_eq!(world.records(), records);
    step(&mut world, vec![pose(moved, 0.5)], &mut actual);
    assert_eq!(environment_count(&actual), 2);
    assert_eq!(world.state().position, moved);
    let mut replay = NativeWorld::restore_synthetic_component_test(p, initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), world.checkpoint().unwrap());
}

pub(crate) fn assert_resident_environment_unavailable(
    mut p: Profile,
    height: &PreparedSourceHeight<'_, '_, '_>,
    catalog: &LiquidCatalog,
    expected: super::super::products::Error,
) {
    let binding = height
        .environment_binding(p.content_digest, catalog, || false)
        .unwrap();
    p.initialize_player_environment(binding, catalog.clone())
        .unwrap();
    let session = environment_session(&p);
    let provider =
        ResidentPlayerEnvironmentProvider::bind(&session, &p, f::player(), height, until(), || {
            false
        })
        .unwrap();
    let mut phases = Vec::new();
    let error = refused_environment(&p, |request| {
        provider.resolve_observed(
            &session,
            request,
            until(),
            || false,
            |phase| phases.push(phase),
        )
    });
    assert!(
        matches!(error, ResidentEnvironmentError::Source(CompositionError::Validation(actual)) if actual == expected)
    );
    assert_eq!(
        phases,
        [0],
        "missing/wet products never construct NoWater Facts"
    );
}
