// SPDX-License-Identifier: GPL-3.0-or-later
//! Real service budgets for the authored loaded Home scene. Only geometry callbacks
//! cross the app mailbox; visitor membership and visibility remain authored inputs.
use super::{
    core::{self, JobLease, ServiceQuery, ServiceStep},
    route_commit::{RouteCommitBoundary, RouteCommitNotice, RouteCommitSink},
    source_route_same_polygon_local,
    wander_component_tests::{self as scene, Consumer},
    Failure, LocalCorridor,
};
use avian3d::prelude::SpatialQuery;
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world::{collision::WorldCollision, terrain_stream::NativeTerrainCoverage};
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_two_owner_fixture as visitors, *,
};
use bevy::ecs::system::SystemState;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const LATE_FAILURE: &str = "authored Home resolved-route rollback";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Commit,
    Cancel,
    Expire,
}

struct Driver {
    local: core::LocalAuthority,
    other: visitors::CompleteAuthority,
    home: Option<AuthorityReply>,
    fail_after_home: bool,
    rejected: Option<AuthorityReply>,
}
impl Authority for Driver {
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.other.observe_derived(reply)
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        match &request.query {
            Query::PlayerEnvironment(_)
            | Query::SourceMotion(MotionQuery::Route(_) | MotionQuery::NearPoint(_)) => {
                let started = Instant::now();
                let reply = self.local.resolve(request).inspect_err(|error| {
                    eprintln!(
                        "Home prerequisite failed: tick={} query={:?} elapsed={:?} error={error:?}",
                        request.binding.tick,
                        request.query,
                        started.elapsed()
                    );
                })?;
                if is_home(request) {
                    self.home = Some(reply.clone());
                    if self.fail_after_home {
                        // Home is selected after this tick's AfterMovement phase.
                        // Reject only after the real mailbox has delivered its
                        // complete spatial receipt, before outer commit/ACK.
                        self.rejected = Some(reply);
                        return Err(NativeError::AuthorityUnavailable(LATE_FAILURE));
                    }
                }
                Ok(reply)
            }
            _ => self.other.resolve(request),
        }
    }
}
fn is_home(request: &AuthorityRequest) -> bool {
    matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route))
        if route.request.kind == motion::RouteKind::HomeForceDestination)
}

fn drive(
    mut world: NativeWorld,
    local: core::LocalAuthority,
    mut sink: core::MailboxRouteCommitSink,
    mut boundary: RouteCommitBoundary,
    other: visitors::CompleteAuthority,
    stop: Stop,
) {
    sink.initialize(&boundary).unwrap();
    let mut driver = Driver {
        local,
        other,
        home: None,
        fail_after_home: stop == Stop::Commit,
        rejected: None,
    };
    for tick in 1..=50 {
        driver.other.visibility = Some(if tick >= 8 {
            ai::movement::Check::Passed
        } else {
            ai::movement::Check::Failed
        });
        let before = world.checkpoint().unwrap();
        let records = world.records().to_vec();
        match world.step(tick, None, &mut driver) {
            Ok(_) => {}
            Err(error) if stop != Stop::Commit => {
                assert!(
                    tick >= 8,
                    "cancellation must reach the real Home job: {error:?}"
                );
                assert_eq!(world.checkpoint().unwrap(), before);
                assert_eq!(world.records(), records);
                match stop {
                    Stop::Cancel => assert!(matches!(
                        error,
                        NativeError::AuthorityUnavailable(
                            "local geometry reply unavailable" | "local geometry snapshot replaced"
                        )
                    )),
                    Stop::Expire => assert_eq!(
                        error,
                        NativeError::AuthorityUnavailable(
                            "local geometry request deadline exceeded"
                        )
                    ),
                    Stop::Commit => unreachable!(),
                }
                assert!(driver.home.is_none(), "cancelled Home cannot publish facts");
                return;
            }
            Err(error) => {
                assert_eq!(error, NativeError::AuthorityUnavailable(LATE_FAILURE));
                assert_eq!(world.checkpoint().unwrap(), before);
                assert_eq!(world.records(), records);
                let home = driver
                    .home
                    .take()
                    .expect("real loaded Home completed before rollback");
                let rejected = driver
                    .rejected
                    .take()
                    .expect("actual fully resolved Home reply");
                driver.fail_after_home = false;
                let retried = world.step(tick, None, &mut driver).unwrap();
                assert!(retried.replies.contains(&rejected));
                assert_eq!(
                    driver.home.as_ref(),
                    Some(&home),
                    "same Home route after rollback"
                );
            }
        }
        let notice = RouteCommitNotice::capture(&world, &boundary).unwrap();
        sink.deliver(&notice).unwrap();
        // Duplicate delivery must ACK the same transition without rebuilding its native owner.
        sink.deliver(&notice).unwrap();
        boundary = notice.boundary().clone();
        if driver.home.is_some() {
            assert_eq!(stop, Stop::Commit);
            assert!(
                !driver.fail_after_home,
                "late rollback must actually execute"
            );
            assert!(matches!(
                scene::snapshot(&world).generator,
                motion::Generator::Home { .. }
            ));
            return;
        }
    }
    panic!("the real runtime did not request Home");
}

#[derive(Default)]
struct PreparationObserved {
    phases: Vec<&'static str>,
    pending: bool,
    completed: bool,
}

#[derive(Default)]
struct Observed {
    preparations: BTreeMap<u64, PreparationObserved>,
    jobs: BTreeMap<u64, (JobLease, usize)>,
    home_tickets: Vec<u64>,
    near_point_tickets: Vec<u64>,
    near_point_late_ticket: Option<u64>,
    near_point_recertified: bool,
    home_candidates: Vec<usize>,
    late_ticket: Option<u64>,
    recertified: bool,
    home_acked: bool,
    last_notice: Option<RouteCommitNotice>,
    stopped: bool,
}
impl Observed {
    fn job(&mut self, lease: JobLease, deadline: Instant) {
        let now = Instant::now();
        assert!(deadline <= now + Duration::from_millis(2));
        assert!(deadline <= lease.deadline());
        assert!(lease.deadline() <= now + Duration::from_millis(500));
        let ticket = lease.ticket();
        if is_home(lease.request()) && !self.home_tickets.contains(&ticket) {
            self.home_tickets.push(ticket);
        }
        let entry = self
            .jobs
            .entry(ticket)
            .or_insert_with(|| (lease.clone(), 0));
        assert!(
            entry.0.same_job(&lease),
            "a yield cannot replace the original Work"
        );
        assert_eq!(entry.0.request(), lease.request());
        assert_eq!(
            entry.0.deadline(),
            lease.deadline(),
            "the 500 ms lease is never renewed"
        );
        entry.1 += 1;
    }
}

fn run(stop: Stop) {
    let (world, consumer) = scene::setup_with_profile(Some(0), Some(800), |profile| {
        profile.player.start = ai::Point::new(12., 12., 0.).unwrap();
        profile.npcs[0].ai.pursuit_ms = 1;
        profile.npcs[0].ai.leash_radius = f::real(0.01);
    });
    let Consumer {
        mut geometry,
        session,
        mut providers,
        other,
        boundary,
        ..
    } = consumer;
    let (mut local, mailbox) = core::mailbox(session.clone()).unwrap();
    let sink = local.take_route_commit_sink().unwrap();
    let mut mailbox = Some(mailbox);
    let worker = std::thread::spawn(move || drive(world, local, sink, boundary, other, stop));
    let mut observed = Observed::default();
    let watchdog = Instant::now() + Duration::from_secs(30);
    while !worker.is_finished() && Instant::now() < watchdog {
        if observed.stopped {
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        // Physics runs outside each mailbox slice, just as the installed app's
        // systems establish the current geometry view before service dispatch.
        let physics_started = Instant::now();
        geometry.advance_physics();
        let physics_elapsed = physics_started.elapsed();
        if physics_elapsed >= Duration::from_millis(2) {
            eprintln!("Home geometry preparation: elapsed={physics_elapsed:?}");
        }
        providers.route.reap_jobs();
        let service = mailbox.as_ref().unwrap();
        let controls =
            service.serve_route_commits(core::PER_FRAME, |session, command| match command {
                core::RouteCommitCommand::Initialize(boundary) => {
                    providers.route.initialize_commit(session, boundary)
                }
                core::RouteCommitCommand::Commit(notice) => {
                    if observed.last_notice.as_ref() == Some(notice) {
                        let retained = providers.route.retained_candidate_count();
                        providers.route.acknowledge(session, notice)?;
                        assert_eq!(providers.route.retained_candidate_count(), retained);
                        return Ok(());
                    }
                    let home = notice.routes().iter().find(|reply| is_home(&reply.request));
                    if let Some(home) = home {
                        assert!(
                            observed.recertified,
                            "expired clearance cannot reach commit"
                        );
                        core::validate_facts(session, &home.request, &home.facts)?;
                        let pointer = providers.route.candidate_identity(home).unwrap();
                        assert!(
                            observed
                                .home_candidates
                                .iter()
                                .all(|saved| *saved == pointer),
                            "rollback and slice retry preserve the exact native proposal"
                        );
                        assert!(
                            providers
                                .route
                                .committed_candidate_for(home.request.binding.npc.unwrap())
                                .is_some(),
                            "Home stays speculative while the prior Chase owner is committed"
                        );
                    }
                    providers.route.acknowledge(session, notice)?;
                    for head in notice.boundary().heads() {
                        assert_eq!(
                            providers
                                .route
                                .committed_candidate_for(head.npc())
                                .map(|(_, cache)| cache),
                            head.cache()
                        );
                    }
                    if let Some(home) = home {
                        assert!(
                            providers.route.candidate_identity(home).is_none(),
                            "ACK retires the local Home PathFinder"
                        );
                        assert!(providers
                            .route
                            .committed_candidate_for(home.request.binding.npc.unwrap())
                            .is_none());
                        observed.home_acked = true;
                    }
                    observed.last_notice = Some(notice.clone());
                    Ok(())
                }
            });
        let mut state = SystemState::<(NativeTerrainCoverage, WorldCollision, SpatialQuery)>::new(
            geometry.app.world_mut(),
        );
        let (terrain, collision, spatial) = state.get(geometry.app.world());
        let mut cut = false;
        let queries =
            service.serve_service_queries_until(
                core::PER_FRAME - controls,
                true,
                |query, deadline| {
                    match query {
                        ServiceQuery::Route(job) => {
                            let lease = job.lease();
                            observed.job(lease.clone(), deadline);
                            let home = is_home(job.request());
                            let request = job.request().clone();
                            let prepare = matches!(&request.query,
                            Query::SourceMotion(MotionQuery::Route(route))
                                if route.request.kind == motion::RouteKind::WanderPrepare);
                            if prepare {
                                observed
                                    .preparations
                                    .entry(lease.ticket())
                                    .or_default()
                                    .phases
                                    .push(job.recorded_phase());
                            }
                            let mut corridor = LocalCorridor {
                                terrain: &terrain,
                                collision: &collision,
                                spatial: &spatial,
                                map: job.session().map,
                                revision: providers.route.raw_geometry(),
                            };
                            let result = source_route_same_polygon_local::resolve_job(
                                &mut providers.route,
                                job,
                                &mut corridor,
                                1,
                                deadline,
                            )
                            .map_err(|error| error.failure());
                            if prepare {
                                let preparation =
                                    observed.preparations.get_mut(&lease.ticket()).unwrap();
                                match &result {
                                    Ok(ServiceStep::Pending) => preparation.pending = true,
                                    Ok(ServiceStep::Complete(_)) => {
                                        assert!(preparation.pending,
                                        "WanderPrepare must advance through retained job stages");
                                        preparation.completed = true;
                                    }
                                    Err(_) => {}
                                }
                            }
                            if home {
                                match &result {
                                    Ok(ServiceStep::Pending) if stop != Stop::Commit => cut = true,
                                    Ok(ServiceStep::Complete(facts)) => {
                                        let reply = AuthorityReply {
                                            request,
                                            facts: *facts.clone(),
                                        };
                                        observed.home_candidates.push(
                                            providers.route.candidate_identity(&reply).unwrap(),
                                        );
                                        if observed.late_ticket == Some(lease.ticket()) {
                                            observed.recertified = true;
                                        } else if observed.late_ticket.is_none() {
                                            observed.late_ticket = Some(lease.ticket());
                                            // Deliberately return this completed attempt too late. The
                                            // service must yield under the SAME lease and recertify a
                                            // fresh concrete geometry view before publishing anything.
                                            std::thread::sleep(
                                                deadline.saturating_duration_since(Instant::now())
                                                    + Duration::from_micros(50),
                                            );
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            result
                        }
                        ServiceQuery::NearPoint(job) => {
                            let lease = job.lease();
                            observed.job(lease.clone(), deadline);
                            if !observed.near_point_tickets.contains(&lease.ticket()) {
                                observed.near_point_tickets.push(lease.ticket());
                            }
                            let facts = providers.near_point.resolve_job(job, deadline)?;
                            if stop == Stop::Commit {
                                if observed.near_point_late_ticket == Some(lease.ticket()) {
                                    observed.near_point_recertified = true;
                                } else if observed.near_point_late_ticket.is_none() {
                                    observed.near_point_late_ticket = Some(lease.ticket());
                                    // A completed but late NearPoint result must be discarded.
                                    // Retry runs real ground/LOS again under this exact Work.
                                    std::thread::sleep(
                                        deadline.saturating_duration_since(Instant::now())
                                            + Duration::from_micros(50),
                                    );
                                }
                            }
                            Ok(ServiceStep::Complete(Box::new(facts)))
                        }
                        ServiceQuery::Immediate(validated) => {
                            let facts = match &validated.request().query {
                                Query::PlayerEnvironment(_) => providers.environment.resolve(
                                    validated.session(),
                                    validated.request(),
                                    deadline,
                                ),
                                Query::SourceMotion(MotionQuery::NearPoint(_)) => providers
                                    .near_point
                                    .resolve(validated.session(), validated.request(), deadline),
                                _ => Err(Failure::UnsupportedQuery),
                            }?;
                            Ok(ServiceStep::Complete(Box::new(facts)))
                        }
                    }
                },
            );
        assert!(controls + queries <= core::PER_FRAME);
        if cut {
            assert!(providers.route.pending_job_count() > 0);
            observed.stopped = true;
            if stop == Stop::Cancel {
                drop(mailbox.take());
            }
        }
        std::thread::yield_now();
    }
    let expired = stop == Stop::Expire && observed.stopped;
    if expired {
        assert!(
            observed
                .home_tickets
                .iter()
                .all(|ticket| !observed.jobs[ticket].0.is_active()),
            "the original lease expires even while the mailbox stays attached"
        );
    }
    drop(mailbox);
    worker.join().unwrap();
    providers.route.reap_jobs();
    assert_eq!(providers.route.pending_job_count(), 0);
    assert!(observed.jobs.values().all(|(lease, _)| !lease.is_active()));
    assert!(!observed.preparations.is_empty());
    assert!(
        !observed.near_point_tickets.is_empty(),
        "the actual initialized NearPoint uses a retained Work proof"
    );
    for preparation in observed.preparations.values() {
        assert!(preparation.pending && preparation.completed);
        let ordered = ["adopt_validated_context", "prepare_constructor", "gate"].map(|phase| {
            preparation
                .phases
                .iter()
                .position(|seen| *seen == phase)
                .expect("WanderPrepare must use the actual staged job path")
        });
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
    }
    if stop == Stop::Commit {
        assert!(observed.near_point_recertified);
        assert!(observed.jobs[&observed.near_point_late_ticket.unwrap()].1 > 1);
        assert!(observed.home_acked);
        assert!(observed.recertified);
        assert_eq!(
            observed.home_tickets.len(),
            2,
            "outer rollback creates a distinct new Work"
        );
        assert!(observed
            .home_tickets
            .iter()
            .all(|ticket| observed.jobs[ticket].1 > 1));
    } else {
        assert!(observed.stopped);
        assert!(!observed.home_acked);
        assert_eq!(observed.home_tickets.len(), 1);
    }
}

#[test]
fn home_mailbox_preserves_real_slices_lease_rollback_and_commit_ack() {
    run(Stop::Commit);
}
#[test]
fn home_mailbox_cancellation_rolls_back_and_reaps_original_work() {
    run(Stop::Cancel);
}
#[test]
fn home_mailbox_original_500ms_lease_expires_without_renewal() {
    run(Stop::Expire);
}
