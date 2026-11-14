// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored native33 owners consume loaded environment, NearPoint and Route facts
//! through the installed Service and concrete LocalCorridor. Every other callback
//! stays an explicit CompleteAuthority fixture, including player pose acceptance.
use super::super::{
    core,
    real_corridor_fixture::Fixture,
    route_commit::{authored_conflicts, RouteCommitBoundary, RouteCommitNotice, RouteCommitSink},
    source_route_same_polygon::two_owner_tests::{
        composed_inputs, lifecycle_tests::configure_lifecycle_profile, source_session,
    },
    Service,
};
use super::*;
use benilla_sim::creature_ai as ai;
use benilla_world_runtime::{
    native::{
        synthetic_fixture as f, synthetic_two_owner_fixture as owners, Authority, AuthorityReply,
        CancelCastTarget, Command, Event, Input, NativeError, NativeWorld, OwnedInput, Profile,
    },
    NpcId,
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::mpsc::{self, Receiver, SyncSender},
    time::Duration,
};

fn calculation(reply: &AuthorityReply) -> &motion::RouteCalculation {
    let Facts::SourceMotion(MotionFacts::Spatial(inner)) = &reply.facts else {
        panic!("actual Route reply")
    };
    let motion::Fact::RouteCalculation(calculation) = &inner.fact else {
        panic!("actual Route calculation")
    };
    calculation
}
fn point(x: f32, y: f32) -> ai::Point {
    ai::Point::new(x, y, 0.).unwrap()
}

struct Transport {
    local: core::LocalAuthority,
    other: owners::CompleteAuthority,
    routes: Vec<AuthorityReply>,
    near_points: Vec<AuthorityReply>,
    fail_after_pair: Option<(u64, u64)>,
    failed_route: Option<AuthorityReply>,
}
impl Transport {
    fn after_route(&self) -> Result<(), NativeError> {
        if self.failed_route.is_some() {
            Err(NativeError::AuthorityUnavailable(
                "authored late B callback after actual Route",
            ))
        } else {
            Ok(())
        }
    }
}
impl Authority for Transport {
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.after_route()?;
        self.other.observe_derived(reply)
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.after_route()?;
        if !matches!(
            request.query,
            Query::PlayerEnvironment(_)
                | Query::SourceMotion(MotionQuery::NearPoint(_) | MotionQuery::Route(_))
        ) {
            return self.other.resolve(request);
        }
        let reply = self.local.resolve(request)?;
        match &request.query {
            Query::SourceMotion(MotionQuery::NearPoint(_)) => self.near_points.push(reply.clone()),
            Query::SourceMotion(MotionQuery::Route(_)) => {
                assert_eq!(
                    calculation(&reply).source,
                    motion::RouteSource::OwnerPathFinder
                );
                self.routes.push(reply.clone());
                if self.fail_after_pair.is_some_and(|(earlier, later)| {
                    request.binding.npc.map(|id| id.spawn) == Some(later)
                        && self
                            .routes
                            .iter()
                            .rev()
                            .skip(1)
                            .take_while(|prior| prior.request.binding.tick == request.binding.tick)
                            .any(|prior| {
                                prior.request.binding.npc.map(|id| id.spawn) == Some(earlier)
                            })
                }) {
                    self.failed_route = Some(reply.clone());
                }
            }
            _ => (),
        }
        Ok(reply)
    }
}

/// The runtime pauses here while the app thread inspects its actual non-Send
/// native owners. A callback cannot promote itself before the commit sink runs.
struct Inspection {
    boundary: RouteCommitBoundary,
    routes: Vec<AuthorityReply>,
    acknowledged: bool,
    reply: SyncSender<NativeHeads>,
}
#[derive(Debug, PartialEq, Eq)]
struct NativeHeads {
    committed: BTreeMap<NpcId, (usize, motion::PathFinderCache)>,
    candidates: Vec<usize>,
}
struct Run {
    world: NativeWorld,
    authority: Transport,
    session: Session,
    sink: core::MailboxRouteCommitSink,
    boundary: RouteCommitBoundary,
    inspect: SyncSender<Inspection>,
    accepted: BTreeMap<NpcId, motion::RouteCalculation>,
    notices: Vec<RouteCommitNotice>,
    crossed_commit_checked: bool,
}
impl Run {
    fn inspect(&self, routes: Vec<AuthorityReply>, acknowledged: bool) -> NativeHeads {
        let (reply, receive) = mpsc::sync_channel(0);
        self.inspect
            .send(Inspection {
                boundary: self.boundary.clone(),
                routes,
                acknowledged,
                reply,
            })
            .unwrap();
        receive.recv_timeout(Duration::from_secs(20)).unwrap()
    }
    fn acknowledge(&mut self) -> RouteCommitNotice {
        let notice = RouteCommitNotice::capture(&self.world, &self.boundary).unwrap();
        let before = self.inspect(notice.routes().to_vec(), false);
        for reply in notice.routes() {
            core::validate_facts(&self.session, &reply.request, &reply.facts).unwrap();
            let npc = reply.request.binding.npc.unwrap();
            let current = calculation(reply);
            if let Some(previous) = self.accepted.get(&npc) {
                assert_eq!(current.prior_cache.as_deref(), Some(&previous.after_cache));
                assert_eq!(
                    current.after_cache.revision,
                    previous.after_cache.revision + 1
                );
                assert_eq!(current.after_cache.binding, previous.after_cache.binding);
                assert_eq!(
                    current.after_cache.poly_refs.len(),
                    previous.after_cache.poly_refs.len()
                );
            } else {
                assert!(current.prior_cache.is_none());
                assert_eq!(current.after_cache.revision, 1);
                assert_eq!(current.after_cache.poly_refs.len(), 75);
            }
            assert_eq!(current.after_cache.binding.identity.creature, npc.spawn);
            assert_eq!(current.after_cache.binding.owner_generation, npc.generation);
            self.accepted.insert(npc, current.clone());
        }
        if !self.crossed_commit_checked
            && notice
                .boundary()
                .heads()
                .iter()
                .filter(|head| head.cache().is_some())
                .count()
                == 2
        {
            let crossed = authored_conflicts::crossed_owner_heads(&notice);
            assert!(self.sink.deliver(&crossed).is_err());
            assert_eq!(self.inspect(notice.routes().to_vec(), false), before);
            self.crossed_commit_checked = true;
        }
        self.sink.deliver(&notice).unwrap();
        self.boundary = notice.boundary().clone();
        let after = self.inspect(notice.routes().to_vec(), true);
        for (reply, pointer) in notice.routes().iter().zip(before.candidates) {
            let npc = reply.request.binding.npc.unwrap();
            // Death can clear the generator later in the same accepted frame.
            if let Some((committed, cache)) = after.committed.get(&npc) {
                assert_eq!(*committed, pointer);
                assert_eq!(*cache, calculation(reply).after_cache);
            }
        }
        self.sink.deliver(&notice).unwrap();
        assert_eq!(self.inspect(vec![], true).committed, after.committed);
        self.notices.push(notice.clone());
        notice
    }
    fn step(&mut self, commands: Vec<Command>) -> RouteCommitNotice {
        let inputs = self.inputs(commands);
        self.world
            .step_batch(self.world.state().tick + 1, inputs, &mut self.authority)
            .unwrap();
        self.acknowledge()
    }
    fn inputs(&self, commands: Vec<Command>) -> Vec<OwnedInput> {
        commands
            .into_iter()
            .enumerate()
            .map(|(index, command)| OwnedInput {
                authenticated_owner: self.world.state().player.owner,
                input: Input {
                    world: self.world.state().world,
                    tick: self.world.state().tick + 1,
                    player: self.world.state().player,
                    sequence: self.world.state().next_sequence + index as u64,
                    command,
                },
            })
            .collect()
    }
    fn cast(&self, owner: u64) -> Command {
        Command::Cast {
            ability: 60001,
            target: self.world.state().npcs[&owner].id,
        }
    }
    fn engage_both(&mut self, hostile: u64, neutral: u64) {
        self.step(vec![self.cast(hostile)]);
        while self.world.state().now_ms < 500 {
            self.step(vec![]);
        }
        assert_eq!(
            self.world.state().npcs[&neutral].ai.phase(),
            ai::Phase::Idle
        );
        assert!(!self
            .authority
            .routes
            .iter()
            .any(|r| r.request.binding.npc.unwrap().spawn == neutral));
        self.step(vec![self.cast(neutral)]);
        while self
            .world
            .state()
            .npcs
            .values()
            .any(|npc| npc.auras.is_empty())
        {
            assert!(self.world.state().tick < 45, "both actual casts must land");
            self.step(vec![]);
        }
        assert_eq!(
            self.world.state().npcs[&neutral].ai.phase(),
            ai::Phase::Combat
        );
        let neutral_profile = self
            .world
            .profile()
            .npcs
            .iter()
            .find(|npc| npc.ai.creature_id == neutral)
            .unwrap();
        assert_eq!(neutral_profile.ai.reaction, ai::Reaction::Aggressive);
        assert!(neutral_profile.ai.neutral_to_all);
        for owner in [hostile, neutral] {
            let first = self
                .authority
                .routes
                .iter()
                .find(|reply| reply.request.binding.npc.unwrap().spawn == owner)
                .unwrap();
            assert!(calculation(first).prior_cache.is_none());
            assert!(self.authority.near_points.iter().any(|reply| reply
                .request
                .binding
                .npc
                .unwrap()
                .spawn
                == owner));
            let first_spline = self
                .world
                .records()
                .iter()
                .flat_map(|record| &record.events)
                .find_map(|event| match event {
                    Event::NpcMotion {
                        id,
                        event: motion::Event::SplineLaunched { spline_id, .. },
                        ..
                    } if id.spawn == owner => Some(*spline_id),
                    _ => None,
                })
                .unwrap();
            assert_eq!(first_spline, 1, "independent owner-local spline counter");
        }
        let heads = self.inspect(vec![], true);
        assert_eq!(heads.committed.len(), 2);
        let identities: Vec<_> = heads
            .committed
            .values()
            .map(|(pointer, _)| *pointer)
            .collect();
        assert_ne!(identities[0], identities[1]);
        let caches: Vec<_> = heads.committed.values().map(|(_, cache)| cache).collect();
        assert_ne!(caches[0].binding.identity, caches[1].binding.identity);
        assert_ne!(caches[0].requested_end, caches[1].requested_end);
    }
}

fn inspect_native(geometry: &Fixture, receive: &Receiver<Inspection>) {
    while let Ok(inspection) = receive.try_recv() {
        let service = geometry.app.world().non_send_resource::<Service>();
        let provider = service.same_polygon_route.as_ref().unwrap();
        let mut committed = BTreeMap::new();
        for head in inspection.boundary.heads() {
            let actual = provider.committed_candidate_for(head.npc());
            assert_eq!(actual.map(|(_, cache)| cache), head.cache());
            if let Some((pointer, cache)) = actual {
                committed.insert(head.npc(), (pointer, cache.clone()));
            }
        }
        let candidates = inspection
            .routes
            .iter()
            .map(|reply| {
                provider
                    .candidate_identity(reply)
                    .expect("actual native proposal")
            })
            .collect();
        if inspection.acknowledged {
            assert_eq!(provider.retained_candidate_count(), committed.len());
            assert_eq!(provider.certified_candidate_count(), committed.len());
        } else {
            assert!(
                provider.retained_candidate_count() <= 4,
                "two committed heads plus two staged descendants"
            );
        }
        inspection
            .reply
            .send(NativeHeads {
                committed,
                candidates,
            })
            .unwrap();
    }
}

fn installed(
    hostile: u64,
    neutral: u64,
    order: [u64; 2],
    configure: impl FnOnce(&mut Profile),
    exercise: impl FnOnce(&mut Run) + Send + 'static,
) {
    let mut geometry = Fixture::new();
    let (mut profile, _, products) = composed_inputs(
        geometry.products(),
        geometry.raw_geometry_revision(),
        &geometry
            .app
            .world()
            .resource::<benilla_assets::MapCatalogRes>()
            .0,
        hostile,
        neutral,
        order,
    );
    for npc in &mut profile.npcs {
        npc.combat.as_mut().unwrap().minimum_damage = f::float(0.);
        npc.combat.as_mut().unwrap().maximum_damage = f::float(0.);
    }
    configure(&mut profile);
    profile.validate().unwrap();
    let mut session = source_session(&profile);
    session.primary_near_point =
        Some(core::PrimaryNearPointPolicy::from_profile(&profile).unwrap());
    session.same_polygon_route = Some(
        core::SamePolygonRoutePolicy::from_profile(
            &profile,
            f::player(),
            products.binding().unwrap(),
        )
        .unwrap(),
    );
    super::super::plugin(&mut geometry.app);
    let (local, sink) = super::super::install_with_same_polygon(
        geometry.app.world_mut(),
        session.clone(),
        &profile,
        f::player(),
        products,
    )
    .unwrap();
    let (inspect, receive) = mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        let world =
            NativeWorld::synthetic_component_test(profile, session.world, f::player(), 1234)
                .unwrap();
        let boundary = RouteCommitBoundary::capture_initial(&world).unwrap();
        let mut other = owners::CompleteAuthority::default();
        other.base.base.critical_chance = Some(0);
        other.base.base.resist_chance = Some(0);
        let mut run = Run {
            world,
            authority: Transport {
                local,
                other,
                routes: vec![],
                near_points: vec![],
                fail_after_pair: None,
                failed_route: None,
            },
            session,
            sink,
            boundary,
            inspect,
            accepted: BTreeMap::new(),
            notices: vec![],
            crossed_commit_checked: false,
        };
        run.sink.initialize(&run.boundary).unwrap();
        exercise(&mut run);
        assert!(run.authority.other.queries.iter().all(|request| !matches!(
            request.query,
            Query::PlayerEnvironment(_)
                | Query::SourceMotion(MotionQuery::NearPoint(_) | MotionQuery::Route(_))
        )));
        assert!(run.world.checkpoint().unwrap().encode().is_ok());
    });
    let deadline = Instant::now() + Duration::from_secs(90);
    while !worker.is_finished() && Instant::now() < deadline {
        geometry.advance_physics();
        inspect_native(&geometry, &receive);
        std::thread::sleep(Duration::from_millis(1));
    }
    if !worker.is_finished() {
        super::super::uninstall(geometry.app.world_mut());
    }
    drop(receive);
    worker.join().unwrap();
    super::super::uninstall(geometry.app.world_mut());
}

fn segment_distance_squared(from: ai::Point, to: ai::Point, owner: ai::Point) -> f32 {
    let dx = to.x.as_f32() - from.x.as_f32();
    let dy = to.y.as_f32() - from.y.as_f32();
    let length = dx * dx + dy * dy;
    let fraction = if length == 0. {
        0.
    } else {
        (((owner.x.as_f32() - from.x.as_f32()) * dx + (owner.y.as_f32() - from.y.as_f32()) * dy)
            / length)
            .clamp(0., 1.)
    };
    (from.x.as_f32() + fraction * dx - owner.x.as_f32()).powi(2)
        + (from.y.as_f32() + fraction * dy - owner.y.as_f32()).powi(2)
}

/// This selects authored player inputs only. It does not produce NearPoint,
/// navigation, collision, or Route facts. Both owners are stationary here.
fn next_target_leg(world: &NativeWorld) -> VecDeque<ai::Point> {
    let position = world.state().position;
    let obstacles: Vec<_> = world
        .profile()
        .npcs
        .iter()
        .map(|npc| {
            let motion = &world.state().npcs[&npc.ai.creature_id]
                .source_motion
                .as_ref()
                .unwrap()
                .motion;
            assert!(!motion.is_moving());
            let distance = (npc.ai.combat_reach.as_f32()
                + world.profile().player.combat_reach.as_f32())
            .min(3.75);
            let reach = ai::combat_reach(npc.ai.combat_reach, world.profile().player.combat_reach)
                .unwrap()
                .as_f32();
            (
                motion.pose().position,
                distance * distance * 0.33,
                reach * reach,
            )
        })
        .collect();
    let mut xs = vec![0.75, 2.5, 5., 7.5, 9.25, position.x.as_f32()];
    let mut ys = vec![0.75, 2.5, 5., 7.5, 9.25, position.y.as_f32()];
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    let points: Vec<_> = xs
        .iter()
        .flat_map(|&x| ys.iter().map(move |&y| point(x, y)))
        .collect();
    let start = points.iter().position(|p| *p == position).unwrap();
    let mut parents = vec![None; points.len()];
    parents[start] = Some(start);
    let mut queue = VecDeque::from([start]);
    while let Some(index) = queue.pop_front() {
        for (next, &destination) in points.iter().enumerate() {
            if parents[next].is_some()
                || (points[index].x != destination.x && points[index].y != destination.y)
                || obstacles.iter().any(|(owner, threshold, _)| {
                    segment_distance_squared(points[index], destination, *owner) <= threshold + 0.01
                })
            {
                continue;
            }
            parents[next] = Some(index);
            queue.push_back(next);
        }
    }
    let mut farthest = None;
    for (index, &destination) in points.iter().enumerate() {
        if parents[index].is_none()
            || !obstacles.iter().all(|(owner, _, reach)| {
                segment_distance_squared(destination, destination, *owner) > *reach
            })
        {
            continue;
        }
        let separation = obstacles
            .iter()
            .map(|(owner, _, _)| segment_distance_squared(destination, destination, *owner))
            .fold(f32::INFINITY, f32::min);
        if farthest.is_none_or(|(_, previous)| separation > previous) {
            farthest = Some((index, separation));
        }
    }
    let (mut index, _) =
        farthest.expect("a bounded clear player-input leg beyond both melee reaches");
    let mut path = VecDeque::new();
    while index != start {
        path.push_front(points[index]);
        index = parents[index].unwrap();
    }
    assert!(!path.is_empty());
    path
}
fn movement(world: &NativeWorld, waypoints: &mut VecDeque<ai::Point>) -> Option<Command> {
    if world
        .state()
        .npcs
        .values()
        .any(|npc| npc.source_motion.as_ref().unwrap().motion.is_moving())
    {
        return None;
    }
    // Hold once a real recheck is due rather than move again before its 250ms timer.
    if world.profile().npcs.iter().any(|npc| {
        let owner = world.state().npcs[&npc.ai.creature_id]
            .source_motion
            .as_ref()
            .unwrap()
            .motion
            .pose()
            .position;
        let reach = ai::combat_reach(npc.ai.combat_reach, world.profile().player.combat_reach)
            .unwrap()
            .as_f32();
        segment_distance_squared(world.state().position, world.state().position, owner)
            > reach * reach
    }) {
        return None;
    }
    if waypoints.is_empty() {
        *waypoints = next_target_leg(world);
    }
    let current = world.state().position;
    while waypoints.front() == Some(&current) {
        waypoints.pop_front();
    }
    let goal = waypoints.front()?;
    let destination = point(
        current.x.as_f32() + (goal.x.as_f32() - current.x.as_f32()).clamp(-0.5, 0.5),
        current.y.as_f32() + (goal.y.as_f32() - current.y.as_f32()).clamp(-0.5, 0.5),
    );
    Some(Command::MovementPose {
        destination,
        orientation: world.state().orientation,
        movement_flags: motion::GroundMovementFlags::STOPPED,
    })
}

#[test]
fn installed_two_owner_service_acknowledges_interleaved_native_replans_and_late_b_retry() {
    installed(
        2,
        3,
        [2, 3],
        |profile| {
            // Authored map frames align both real source 250ms rechecks without
            // modifying either owner's actual timer or constructor state.
            profile.tick_ms = 250;
            profile.player.start = point(5., 6.5);
        },
        |run| {
            run.engage_both(2, 3);
            // The symmetric actual endpoints cross the next >5-unit chase
            // threshold together at y=9.25 after both first splines settle.
            let mut waypoints = VecDeque::from([point(5., 9.25)]);
            let mut retried_b = false;
            let mut interleaved = Vec::new();
            // Each iteration admits at most one .5-unit player input. A diagonal
            // spline, an axis-grid detour and a 250ms source recheck bound each leg.
            let tick_ms = run.world.profile().tick_ms;
            let spline_ticks = ((200_f32.sqrt() / 2.5) * 1000. / tick_ms as f32).ceil() as u64;
            let leg_ticks = (4 * 17) + spline_ticks + u64::from(250_u32.div_ceil(tick_ms)) + 2;
            let finish_by = run.world.state().tick + 16 * leg_ticks;
            while !retried_b
                || run
                    .accepted
                    .values()
                    .any(|route| route.after_cache.revision < 6)
            {
                assert!(
                    run.world.state().tick < finish_by,
                    "both owners must retain at least six actual calculations"
                );
                let commands = movement(&run.world, &mut waypoints).into_iter().collect();
                let inputs = run.inputs(commands);
                if !retried_b
                    && run.accepted[&NpcId {
                        spawn: 3,
                        generation: 0,
                    }]
                        .after_cache
                        .revision
                        >= 1
                {
                    run.authority.fail_after_pair = Some((2, 3));
                }
                let checkpoint = run.world.checkpoint().unwrap();
                let heads = run.inspect(vec![], true);
                let first = run.authority.routes.len();
                let result = run.world.step_batch(
                    run.world.state().tick + 1,
                    inputs.clone(),
                    &mut run.authority,
                );
                if let Some(failed_b) = run.authority.failed_route.clone() {
                    assert_eq!(
                        result,
                        Err(NativeError::AuthorityUnavailable(
                            "authored late B callback after actual Route"
                        ))
                    );
                    assert_eq!(run.world.checkpoint().unwrap(), checkpoint);
                    assert_eq!(failed_b.request.binding.npc.unwrap().spawn, 3);
                    let attempted = run.authority.routes[first..].to_vec();
                    assert_eq!(
                        attempted
                            .iter()
                            .map(|reply| reply.request.binding.npc.unwrap().spawn)
                            .collect::<Vec<_>>(),
                        vec![2, 3]
                    );
                    assert!(attempted.iter().all(|reply| {
                        calculation(reply).prior_cache.is_some()
                            && calculation(reply).after_cache.revision == 2
                    }));
                    let failed_heads = run.inspect(attempted.clone(), false);
                    assert_eq!(failed_heads.committed, heads.committed);
                    run.authority.failed_route = None;
                    run.authority.fail_after_pair = None;
                    let retry_start = run.authority.routes.len();
                    run.world
                        .step_batch(run.world.state().tick + 1, inputs, &mut run.authority)
                        .unwrap();
                    let retried = run.authority.routes[retry_start..].to_vec();
                    assert_eq!(retried, attempted);
                    let retry_heads = run.inspect(retried.clone(), false);
                    assert_eq!(
                        retry_heads, failed_heads,
                        "retry must reuse each owner-local native candidate without promotion"
                    );
                    for (prior, fresh) in attempted.iter().zip(&retried) {
                        assert!(!prior
                            .request
                            .require_pathfinder_dispatch(run.world.profile())
                            .unwrap()
                            .same_live_dispatch(
                                fresh
                                    .request
                                    .require_pathfinder_dispatch(run.world.profile())
                                    .unwrap()
                            ));
                    }
                    retried_b = true;
                } else {
                    result.unwrap();
                }
                let notice = run.acknowledge();
                if !notice.routes().is_empty() {
                    waypoints.clear();
                }
                interleaved.extend(
                    notice
                        .routes()
                        .iter()
                        .map(|reply| reply.request.binding.npc.unwrap().spawn),
                );
            }
            assert!(retried_b);
            assert!(run.crossed_commit_checked);
            assert!(interleaved.windows(2).any(|owners| owners == [2, 3]));
            assert!(interleaved.windows(2).any(|owners| owners == [3, 2]));
            let heads = run.inspect(vec![], true);
            assert_eq!(heads.committed.len(), 2);
            assert!(heads
                .committed
                .values()
                .all(|(_, cache)| cache.revision >= 6));
            let old_notice = run.notices[0].clone();
            assert!(run.sink.deliver(&old_notice).is_err());
            assert_eq!(run.inspect(vec![], true), heads);
            let mut replay = NativeWorld::synthetic_component_test(
                run.world.profile().clone(),
                run.world.state().world,
                run.world.state().player,
                1234,
            )
            .unwrap();
            replay.replay(run.world.records()).unwrap();
            assert_eq!(
                replay.checkpoint().unwrap(),
                run.world.checkpoint().unwrap()
            );
            for world in [&run.world, &replay] {
                assert_eq!(
                    world.validate_saved_session_reopen(),
                    Err(NativeError::UnsupportedMechanic(
                        "saved native PathFinder cache rehydration",
                    ))
                );
            }
        },
    );
}

#[test]
fn installed_two_owner_cancellation_death_and_respawn_preserve_peer_native_head() {
    installed(
        2,
        3,
        [2, 3],
        |profile| {
            configure_lifecycle_profile(profile);
            assert_eq!(profile.tick_ms, 100);
        },
        |run| {
            let neutral_health = run.world.state().npcs[&3].health;
            let sequence = run.world.state().next_sequence;
            run.step(vec![
                run.cast(3),
                Command::CancelCast {
                    spell_id: 60001,
                    target: CancelCastTarget::PriorInput(sequence),
                },
            ]);
            assert!(run
                .world
                .last_tick()
                .unwrap()
                .events
                .iter()
                .any(|event| matches!(
                    event,
                    Event::Spell {
                        event: benilla_sim::classic_spell::SpellEvent::Failed(
                            benilla_sim::classic_spell::CastFailure::Interrupted
                        ),
                        ..
                    }
                )));
            while run.authority.routes.is_empty() {
                assert!(run.world.state().tick < 8);
                run.step(vec![]);
            }
            let first_route = run.authority.routes[0].clone();
            assert_eq!(
                first_route.request.binding.npc,
                Some(NpcId {
                    spawn: 2,
                    generation: 0
                })
            );
            assert_eq!(run.inspect(vec![], true).committed.len(), 1);
            while run.world.state().tick < 6 {
                run.step(vec![]);
            }
            assert_eq!(run.world.state().npcs[&3].ai.phase(), ai::Phase::Idle);
            assert_eq!(run.world.state().npcs[&3].health, neutral_health);
            assert!(run.authority.routes.iter().all(|reply| reply
                .request
                .binding
                .npc
                .unwrap()
                .spawn
                == 2));
            // Only this second, uncancelled cast may make the neutral owner retaliate.
            run.step(vec![run.cast(3)]);
            let a = NpcId {
                spawn: 2,
                generation: 0,
            };
            let b = NpcId {
                spawn: 3,
                generation: 0,
            };
            while !run.accepted.contains_key(&b)
                || run
                    .world
                    .state()
                    .npcs
                    .values()
                    .any(|npc| npc.source_motion.as_ref().unwrap().motion.is_moving())
            {
                assert!(
                    run.world.state().tick < 60,
                    "both genuine source splines must settle"
                );
                run.step(vec![]);
            }
            let both = run.inspect(vec![], true);
            assert_eq!(both.committed.len(), 2);
            assert_ne!(both.committed[&a].0, both.committed[&b].0);
            let peer = both.committed[&b].clone();
            assert_eq!(run.world.state().npcs[&3].ai.phase(), ai::Phase::Combat);
            assert!(
                run.world
                    .state()
                    .position
                    .distance_squared(run.world.state().npcs[&2].ai.position())
                    .unwrap()
                    .as_f32()
                    <= 25.
            );
            // Source bound at 100ms/tick: settled by 60; command 1 tick,
            // 200ms cast + at most two 200ms pushbacks + event alignment 7,
            // max(5yd, distance)/4 projectile 13, respawn <=20. Total <=101.
            // The authored 2000ms swings cannot repeat during the cast window.
            run.step(vec![run.cast(2)]);
            let mut phases = vec![];
            while run.world.state().npcs[&2].id.generation == 0 {
                assert!(
                    run.world.state().tick < 110,
                    "projectile, corpse decay and respawn are bounded: state={:?}, last_tick={:?}",
                    run.world.state(),
                    run.world.last_tick()
                );
                run.step(vec![]);
                let state = run.world.state();
                phases.push(state.npcs[&2].ai.phase());
                assert_eq!(state.npcs[&3].id, b);
                assert_eq!(state.npcs[&3].ai.phase(), ai::Phase::Combat);
                assert_eq!(state.npcs[&3].events.clock_ms(), state.now_ms);
                let heads = run.inspect(vec![], true);
                assert_eq!(
                    heads.committed[&b], peer,
                    "A's lifecycle must not prune or rebind B's native owner"
                );
                if matches!(
                    state.npcs[&2].ai.phase(),
                    ai::Phase::Corpse | ai::Phase::Despawned
                ) {
                    assert_eq!(heads.committed.len(), 1);
                    assert!(!heads.committed.contains_key(&a));
                }
            }
            assert!(phases.contains(&ai::Phase::Corpse));
            assert!(phases.contains(&ai::Phase::Despawned));
            let respawned = &run.world.state().npcs[&2];
            assert_eq!(respawned.id.generation, 1);
            assert_eq!(respawned.ai.phase(), ai::Phase::Idle);
            assert!(matches!(
                respawned.source_motion.as_ref().unwrap().motion.generator(),
                motion::Generator::Idle
            ));
            let new_head = run
                .boundary
                .heads()
                .iter()
                .find(|head| head.npc() == respawned.id)
                .unwrap();
            assert!(new_head.cache().is_none());
            assert!(new_head.construction().is_none());
            let heads = run.inspect(vec![], true);
            assert_eq!(heads.committed, BTreeMap::from([(b, peer)]));
            assert_eq!(
                run.world.validate_saved_session_reopen(),
                Err(NativeError::UnsupportedMechanic(
                    "saved native PathFinder cache rehydration",
                )),
                "B alone must keep native cache reopening closed"
            );
            let checkpoint = run.world.checkpoint().unwrap();
            assert!(run.authority.local.resolve(&first_route.request).is_err());
            assert_eq!(run.world.checkpoint().unwrap(), checkpoint);
            assert_eq!(run.inspect(vec![], true), heads);
        },
    );
}

#[test]
fn installed_two_owner_reversed_roles_reject_crossed_actual_route_facts() {
    installed(
        3,
        2,
        [3, 2],
        |_| {},
        |run| {
            run.engage_both(3, 2);
            assert!(run.crossed_commit_checked);
            let routes: Vec<_> = [2, 3]
                .into_iter()
                .map(|owner| {
                    run.authority
                        .routes
                        .iter()
                        .find(|reply| reply.request.binding.npc.unwrap().spawn == owner)
                        .unwrap()
                        .clone()
                })
                .collect();
            let before = run.world.checkpoint().unwrap();
            let heads = run.inspect(vec![], true);
            for (own, foreign) in [(&routes[0], &routes[1]), (&routes[1], &routes[0])] {
                core::validate_facts(&run.session, &own.request, &own.facts).unwrap();
                assert!(core::validate_facts(&run.session, &own.request, &foreign.facts).is_err());
                for mutation in 0..4 {
                    let mut crossed = own.clone();
                    let Query::SourceMotion(MotionQuery::Route(request)) =
                        &mut crossed.request.query
                    else {
                        unreachable!()
                    };
                    let Query::SourceMotion(MotionQuery::Route(other)) = &foreign.request.query
                    else {
                        unreachable!()
                    };
                    match mutation {
                        0 => crossed.request.binding.npc = foreign.request.binding.npc,
                        1 => {
                            request.request.prior_cache =
                                Some(Box::new(calculation(foreign).after_cache.clone()))
                        }
                        2 => request.pathfinder_dispatch = other.pathfinder_dispatch.clone(),
                        3 => request.owner = other.owner.clone(),
                        _ => unreachable!(),
                    }
                    assert!(
                        core::validate_facts(&run.session, &crossed.request, &crossed.facts)
                            .is_err(),
                        "owner crossing {mutation}"
                    );
                }
            }
            assert_eq!(run.world.checkpoint().unwrap(), before);
            assert_eq!(run.inspect(vec![], true), heads);
        },
    );
}
