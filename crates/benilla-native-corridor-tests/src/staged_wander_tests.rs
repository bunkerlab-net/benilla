// SPDX-License-Identifier: GPL-3.0-or-later
//! Concurrent original jobs for one actual initialized Wander preparation.
use super::{
    core::{self, ServiceQuery, ServiceStep},
    source_route_same_polygon_local,
    wander_component_tests::{self as scene, Consumer},
};
use benilla_sim::creature_motion as motion;
use benilla_world_runtime::native::{
    Authority, AuthorityReply, AuthorityRequest, Facts, MotionFacts, MotionQuery, NativeError,
    Query,
};
use crossbeam_channel::TryRecvError;
use std::time::{Duration, Instant};

struct CapturePreparation {
    consumer: Consumer,
    request: Option<AuthorityRequest>,
}
impl Authority for CapturePreparation {
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.consumer.observe_derived(reply)
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(&request.query, Query::SourceMotion(MotionQuery::Route(route))
            if route.request.kind == motion::RouteKind::WanderPrepare)
        {
            self.request = Some(request.clone());
            return Err(NativeError::AuthorityUnavailable(
                "capture live preparation",
            ));
        }
        self.consumer.resolve(request)
    }
}

#[test]
fn home_wander_prepare_equal_live_jobs_preserve_both_original_leases() {
    let (mut world, consumer) = scene::setup_with_schedule(None, None);
    let mut capture = CapturePreparation {
        consumer,
        request: None,
    };
    for tick in 1..=10 {
        match world.step(tick, None, &mut capture) {
            Ok(_) => capture.consumer.acknowledge(&world),
            Err(error) => {
                assert_eq!(
                    error,
                    NativeError::AuthorityUnavailable("capture live preparation")
                );
                break;
            }
        }
    }
    let request = capture
        .request
        .take()
        .expect("actual initialized WanderPrepare");
    let Consumer {
        session,
        mut geometry,
        mut providers,
        ..
    } = capture.consumer;
    let (_authority, mailbox) = core::mailbox(session).unwrap();
    let jobs = [
        mailbox
            .enqueue_service_test_request(1, request.clone())
            .unwrap(),
        mailbox.enqueue_service_test_request(2, request).unwrap(),
    ];
    assert!(!jobs[0].0.same_job(&jobs[1].0));
    let expiries = [jobs[0].0.deadline(), jobs[1].0.deadline()];
    let mut completed = [false; 2];
    let mut phases = vec![];
    for _ in 0..24 {
        geometry.with_local_corridor(|corridor| {
            assert_eq!(
                mailbox.serve_service_test_slice(
                    1,
                    true,
                    Instant::now() + Duration::from_secs(10),
                    |query, deadline| {
                        let ServiceQuery::Route(job) = query else {
                            panic!("Route job")
                        };
                        let ticket = job.lease().ticket();
                        let index = usize::try_from(ticket - 1).unwrap();
                        assert!(job.lease().same_job(&jobs[index].0));
                        assert_eq!(job.lease().deadline(), expiries[index]);
                        phases.push((ticket, job.recorded_phase()));
                        let result = source_route_same_polygon_local::resolve_job(
                            &mut providers.route,
                            job,
                            corridor,
                            1,
                            deadline,
                        )
                        .map_err(|error| error.failure())?;
                        if matches!(result, ServiceStep::Complete(_)) {
                            assert_eq!(providers.route.pending_job_count(), 2 - index);
                        }
                        Ok(result)
                    },
                ),
                1
            );
        });
        for (index, (_, reply)) in jobs.iter().enumerate() {
            if completed[index] {
                continue;
            }
            match reply.try_result() {
                Err(TryRecvError::Empty) => assert!(jobs[index].0.is_active()),
                Ok(Ok(Facts::SourceMotion(MotionFacts::Spatial(reply)))) => {
                    assert_eq!(reply.fact, motion::Fact::WanderPrepared);
                    completed[index] = true;
                }
                other => panic!("unexpected preparation reply: {other:?}"),
            }
        }
        if completed == [true; 2] {
            break;
        }
    }
    assert_eq!(completed, [true; 2]);
    // FIFO puts both jobs through the gate before either finishes its prefix.
    let prefix_and_final: Vec<_> = phases
        .into_iter()
        .filter(|(_, phase)| matches!(*phase, "gate" | "native_terminal"))
        .collect();
    assert_eq!(
        prefix_and_final,
        vec![
            (1, "gate"),
            (2, "gate"),
            (1, "native_terminal"),
            (2, "native_terminal"),
        ]
    );
    for (index, (lease, _)) in jobs.iter().enumerate() {
        assert_eq!(lease.deadline(), expiries[index]);
        assert!(!lease.is_active());
    }
    providers.route.reap_jobs();
    assert_eq!(providers.route.pending_job_count(), 0);
}
