// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded filter/smoothing prerequisites across a test-only owner-thread
//! channel. The production mailbox still refuses Route. Other facts are synthetic.
use super::*;
use crate::net::p2p::native_geometry::{
    authored_route_smoothing, authored_route_smoothing_revision, RouteSmoothingRequest,
    SourceRouteSmoothingConfiguration, SourceRouteSmoothingProvider, SourceRouteSmoothingStage,
};
use std::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    context: n::AuthorityRequest,
    owner: [u32; 3],
    liquid_tickets: [u64; 2],
    raw_smoothing: benilla_native_nav::LoadedSourceSmooth,
}

struct SmoothingAuthority {
    send: mpsc::Sender<Option<n::AuthorityRequest>>,
    receive: mpsc::Receiver<Result<(), Failure>>,
    service: Option<std::thread::JoinHandle<()>>,
    other: source::SyntheticMotionAuthority,
    queries: Arc<Mutex<Vec<n::AuthorityRequest>>>,
}
impl n::Authority for SmoothingAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.queries.lock().unwrap().push(request.clone());
        if !matches!(
            request.query,
            n::Query::SourceMotion(n::MotionQuery::Route(_))
        ) {
            return self.other.resolve(request);
        }
        self.send.send(Some(request.clone())).unwrap();
        match self.receive.recv_timeout(Duration::from_secs(10)).unwrap() {
            Err(Failure::Binding) => return Err(n::NativeError::AuthorityBinding),
            Err(Failure::Invalid) => return Err(n::NativeError::InvalidInput),
            Err(_) => {
                return Err(n::NativeError::AuthorityUnavailable(
                    "loaded smoothing prerequisite",
                ));
            }
            Ok(()) => {}
        }
        // Source smoothing alone is not optimized/normalized OwnerPathFinder or
        // complete segment evidence. Never manufacture a Route fact from it.
        Err(n::NativeError::AuthorityUnavailable(
            "source owner route after smoothing unavailable",
        ))
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.other.observe_derived(reply)
    }
}
impl Drop for SmoothingAuthority {
    fn drop(&mut self) {
        let _ = self.send.send(None);
        if let Some(service) = self.service.take() {
            service.join().unwrap();
        }
    }
}

fn smoothing_setup() -> (
    n::Profile,
    SmoothingAuthority,
    Arc<AtomicU64>,
    Arc<Mutex<Vec<Observation>>>,
) {
    let (mut data, base) = super::setup(true);
    let products = authored_route_smoothing();
    let liquid = products.load_dry_liquid().unwrap();
    let navigation = products.load_navigation().unwrap();
    let config = SourceRouteSmoothingConfiguration {
        stage: SourceRouteSmoothingStage::PreOptimizePathPreNormalizeZ,
        point_limit: 74,
        budget: benilla_native_nav::SourceSmoothingBudget {
            corridor_expansions: 4096,
            smoothing_primitives: 256,
        },
    };
    let binding = liquid.binding();
    data.map_id = binding.map;
    data.content_digest = binding.content;
    data.geometry_revision =
        authored_route_smoothing_revision(binding, navigation.binding(), config).unwrap();
    data.npcs[0].ai.map_id = binding.map;
    data.npcs[0].ai.content_revision = ai::movement::Revision(binding.content);
    data.npcs[0].ai.spawn = ai::Point::new(-400., -400., 0.).unwrap();
    data.player.start = ai::Point::new(-400., -390., 0.).unwrap();
    data.initialize_creature_capabilities(source::creature_capability_sources(&data))
        .unwrap();
    let mut session = base.session;
    session.profile = data.digest();
    session.content = data.content_digest;
    session.geometry = data.geometry_revision;
    session.map = data.map_id;
    session.participants = Some(NonRenderParticipants::authored_fixture(
        binding.content,
        binding.map,
    ));
    // Mesh is deliberately non-Send. Construct and retain it on its query owner
    // thread; the discarded inspection snapshot above never crosses threads.
    drop(navigation);
    let profile = data.clone();
    let fault = Arc::new(AtomicU64::new(0));
    let mode = Arc::clone(&fault);
    let observations = Arc::new(Mutex::new(vec![]));
    let seen = Arc::clone(&observations);
    let (send, requests) = mpsc::channel::<Option<n::AuthorityRequest>>();
    let (answers, receive) = mpsc::channel();
    let parent_thread = std::thread::current().id();
    let service = std::thread::spawn(move || {
        assert_ne!(std::thread::current().id(), parent_thread);
        let products = authored_route_smoothing();
        let provider = SourceRouteSmoothingProvider::bind(
            &session,
            &profile,
            products.load_dry_liquid().unwrap(),
            products.load_navigation().unwrap(),
            config,
        )
        .unwrap();
        while let Some(context) = requests.recv().unwrap() {
            let mode = mode.load(Ordering::SeqCst);
            let mut actual_session = session.clone();
            if mode == 2 {
                actual_session.geometry[0] ^= 1;
            }
            let result = (|| {
                let bound = provider.begin(&actual_session, &context)?;
                let query = RouteSmoothingRequest {
                    snapshot: provider.binding(),
                    context: context.clone(),
                    first_ticket: 31,
                };
                let deadline = Instant::now()
                    + if mode == 1 {
                        Duration::ZERO
                    } else {
                        Duration::from_secs(5)
                    };
                let dependency = bound.query(query.clone(), deadline)?;
                assert_eq!(dependency.request(), &query);
                let filter = dependency.filter_dependency();
                assert_eq!(filter.request().context, context);
                assert_eq!(filter.filter().include_flags(), 1);
                assert_eq!(filter.filter().exclude_flags(), 0);
                let smoothing = dependency.smoothing_reply();
                let route = route_query(&context).unwrap();
                assert_eq!(smoothing.result.from, point_bits(route.request.origin));
                assert_eq!(smoothing.result.to, point_bits(route.request.goal));
                assert_eq!(smoothing.request.ticket, 33);
                assert_eq!(smoothing.result.status, 1 << 30);
                assert_eq!(
                    smoothing.result.stop,
                    benilla_native_nav::SourceSmoothStop::TargetReached
                );
                assert_eq!(
                    smoothing.result.points.first(),
                    Some(&smoothing.result.from)
                );
                assert_eq!(smoothing.result.points.last(), Some(&smoothing.result.to));
                assert!(smoothing.result.points.len() >= 3);
                assert!(
                    smoothing.result.points[1..smoothing.result.points.len() - 1]
                        .iter()
                        .any(|p| p[2] == 0.5f32.to_bits())
                );
                let observations = filter.liquid_observations();
                assert_eq!(observations.map(|o| o.mask.bits()), [0x0f, 0x0a]);
                seen.lock().unwrap().push(Observation {
                    context,
                    owner: filter.owner_position(),
                    liquid_tickets: observations.map(|o| o.reply.request.ticket),
                    raw_smoothing: dependency.smoothing_reply().result.clone(),
                });
                Ok(())
            })();
            answers.send(result).unwrap();
        }
    });
    (
        data,
        SmoothingAuthority {
            send,
            receive,
            service: Some(service),
            other: source::SyntheticMotionAuthority::default(),
            queries: Arc::new(Mutex::new(vec![])),
        },
        fault,
        observations,
    )
}

#[test]
fn actual_loaded_smoothing_dependency_never_publishes_a_route_and_retries_exactly() {
    let h = Harness::new();
    let (data, authority, fault, observations) = smoothing_setup();
    let requests = Arc::clone(&authority.queries);
    let mut c = create(&h, data.clone(), authority);
    c.initial_events().unwrap();
    pump_to(&mut c, 1);
    let before = c.world.checkpoint().unwrap();
    let records = c.world.records().to_vec();
    let files = durable_files(&h);
    let generation = c.store.generation();
    let position = c.position_xyz();
    let facing = c.facing();
    let npc = c.world.state().npcs[&2].id;
    let guid = c.projection.npc_guid(npc);
    let mut moved = position;
    moved[1] += 0.25;
    c.receive(move_to(moved)).unwrap();
    let mut pending = None;
    for (attempt, mode) in [0, 1, 0].into_iter().enumerate() {
        fault.store(mode, Ordering::SeqCst);
        if attempt != 0 {
            c.retry_tick().unwrap();
        }
        let out = c.pump(if attempt == 0 {
            c.tick_duration()
        } else {
            Duration::ZERO
        });
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.world.records(), records);
        assert_eq!(durable_files(&h), files);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(c.position_xyz(), position);
        assert_eq!(c.facing(), facing);
        assert_eq!(c.projection.npc_guid(npc), guid);
        let inputs = c.pending.as_ref().unwrap().inputs.clone();
        if let Some(expected) = &pending {
            assert_eq!(&inputs, expected);
        } else {
            pending = Some(inputs);
        }
    }
    let requests = requests.lock().unwrap();
    let routes: Vec<_> = requests
        .iter()
        .filter(|q| route_query(q).is_some())
        .collect();
    assert_eq!(routes.len(), 3);
    assert!(routes.iter().all(|q| *q == routes[0]));
    let observed = observations.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[0].context, *routes[0]);
    assert_eq!(observed[0].owner, [-400f32, -400., 0.].map(f32::to_bits));
    assert_eq!(observed[0].liquid_tickets, [31, 32]);
    drop(observed);
    drop(requests);
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let (_, authority, _, _) = smoothing_setup();
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, authority);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.tick(), 1);
    assert_eq!(reopened.world.state().next_sequence, 0);
}

#[test]
fn changed_smoothing_geometry_is_terminal_without_controller_or_store_mutation() {
    let h = Harness::new();
    let (data, authority, fault, observations) = smoothing_setup();
    let requests = Arc::clone(&authority.queries);
    let mut c = create(&h, data, authority);
    c.initial_events().unwrap();
    pump_to(&mut c, 1);
    let before = c.world.checkpoint().unwrap();
    let files = durable_files(&h);
    let generation = c.store.generation();
    let position = c.position_xyz();
    fault.store(2, Ordering::SeqCst);
    let mut moved = position;
    moved[1] += 0.25;
    c.receive(move_to(moved)).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(matches!(out.pause.unwrap().kind, PauseKind::Simulation));
    assert_eq!(out.ticks, 0);
    assert!(out.events.is_empty());
    assert_eq!(c.world.checkpoint().unwrap(), before);
    assert_eq!(durable_files(&h), files);
    assert_eq!(c.store.generation(), generation);
    assert_eq!(c.position_xyz(), position);
    assert!(observations.lock().unwrap().is_empty());
    assert!(matches!(c.retry_tick(), Err(Error::Paused)));
    let count = requests.lock().unwrap().len();
    c.pump(Duration::from_secs(60));
    assert_eq!(requests.lock().unwrap().len(), count);
}
