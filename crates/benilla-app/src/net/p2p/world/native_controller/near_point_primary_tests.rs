// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual first-candidate NearPoint facts over authored loaded products, through
//! the controller, durable store and replay. Other authorities remain synthetic.
use super::*;
use crate::net::p2p::native_geometry::{
    authored_ground_height, authored_near_point_revision as geometry_revision,
    core::{self, Body, Failure, NonRenderParticipants, PlayerMotion, Session},
    SourceNearPointConfiguration, SourceNearPointProvider,
};
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use n::synthetic_motion_fixture as source;
use std::{collections::BTreeMap, time::Instant};

const MID: f32 = 0.5f32 * (64.0 * 533.333_3);

// The loaded authored terrain is map0. Renderability remains an explicit fixture.
struct GroundCatalog;
impl ClientCatalog for GroundCatalog {
    fn build(&self) -> u32 {
        Catalog.build()
    }
    fn manifest_digest(&self) -> runtime::Digest {
        Catalog.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        id == 0
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        Catalog.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        Catalog.contains_item_display(id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        Catalog.contains_spell(id)
    }
    fn player_power_type(&self, player: PlayerId) -> Option<u8> {
        Catalog.player_power_type(player)
    }
    fn player_bytes_2(&self, player: PlayerId) -> Option<u32> {
        Catalog.player_bytes_2(player)
    }
    fn item_metadata(&self, entry: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        Catalog.item_metadata(entry)
    }
    fn quest_metadata(&self, entry: u32) -> Option<benilla_world_client::QuestMetadata> {
        Catalog.quest_metadata(entry)
    }
    fn quest_greeting(&self, entry: u32) -> Option<String> {
        Catalog.quest_greeting(entry)
    }
}

type Rebind =
    dyn Fn(SourceNearPointConfiguration) -> Result<SourceNearPointProvider, Failure> + Send;

struct ProductAuthority {
    provider: SourceNearPointProvider,
    session: Session,
    config: SourceNearPointConfiguration,
    rebind: Box<Rebind>,
    // 1 gates the reader; 2..4 disable required configuration; 5 changes geometry;
    // 6 removes independent participant proof; 7 expires the whole query deadline.
    fault: Arc<AtomicU64>,
    observed: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    failures: Arc<Mutex<Vec<Failure>>>,
    other: source::SyntheticMotionAuthority,
}
impl n::Authority for ProductAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        if !matches!(
            request.query,
            n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
        ) {
            return self.other.resolve(request);
        }
        self.observed.lock().unwrap().push(request.clone());
        let fault = self.fault.load(Ordering::SeqCst);
        if fault == 1 {
            return Err(n::NativeError::AuthorityUnavailable(
                "authored first-candidate reader gate",
            ));
        }
        let facts = (|| {
            if (2..=4).contains(&fault) {
                let mut config = self.config;
                match fault {
                    2 => config.detect_position_collision = false,
                    3 => config.vmap_line_of_sight_enabled = false,
                    _ => config.height.vmap_height_enabled = false,
                }
                return (self.rebind)(config)?.resolve(
                    &self.session,
                    request,
                    Instant::now() + Duration::from_secs(5),
                );
            }
            let mut session = self.session.clone();
            if fault == 5 {
                session.geometry[0] ^= 1;
            } else if fault == 6 {
                session.participants = None;
            }
            let deadline = Instant::now()
                + if fault == 7 {
                    Duration::ZERO
                } else {
                    Duration::from_secs(5)
                };
            self.provider.resolve(&session, request, deadline)
        })()
        .map_err(|failure| {
            self.failures.lock().unwrap().push(failure);
            n::NativeError::AuthorityUnavailable("bound source NearPoint unavailable")
        })?;
        Ok(n::AuthorityReply {
            request: request.clone(),
            facts,
        })
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.other.observe_derived(reply)
    }
}

// A stand-in for the main-thread product service. Only NearPoint crosses this
// actual bounded mailbox; the other component authorities remain synthetic.
struct MailboxAuthority {
    local: core::LocalAuthority,
    other: source::SyntheticMotionAuthority,
    stop: Arc<AtomicBool>,
    service: Option<std::thread::JoinHandle<()>>,
}
impl n::Authority for MailboxAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        if matches!(
            request.query,
            n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
        ) {
            self.local.resolve(request)
        } else {
            self.other.resolve(request)
        }
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.other.observe_derived(reply)
    }
}
impl Drop for MailboxAuthority {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let result = self.service.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}
fn mailbox_authority(authority: ProductAuthority) -> MailboxAuthority {
    let ProductAuthority {
        provider,
        session,
        fault,
        observed,
        failures,
        other,
        ..
    } = authority;
    let (local, mailbox) = core::mailbox(session).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let service_stop = Arc::clone(&stop);
    let controller_thread = std::thread::current().id();
    let service = std::thread::spawn(move || {
        assert_ne!(std::thread::current().id(), controller_thread);
        while !service_stop.load(Ordering::Acquire) {
            let count = mailbox.serve_queries_until(1, |session, request, _, deadline| {
                observed.lock().unwrap().push(request.clone());
                // The retained provider is loaded/bound before this 2ms service window.
                let facts = if fault.load(Ordering::SeqCst) == 1 {
                    Err(Failure::MissingGeometry)
                } else {
                    provider.resolve(session, request, deadline)
                };
                let mut facts = facts.inspect_err(|&failure| {
                    failures.lock().unwrap().push(failure);
                })?;
                let n::Facts::SourceMotion(n::MotionFacts::Spatial(reply)) = &mut facts else {
                    unreachable!()
                };
                match fault.load(Ordering::SeqCst) {
                    2 => {
                        let motion::Query::NearPoint(query) = &mut reply.request else {
                            unreachable!()
                        };
                        query.binding.query_id += 1;
                    }
                    3 => reply.fact = motion::Fact::Missing,
                    _ => {}
                }
                Ok(facts)
            });
            assert!(count <= 1);
            if count == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });
    MailboxAuthority {
        local,
        other,
        stop,
        service: Some(service),
    }
}

fn setup(z: f32) -> (n::Profile, ProductAuthority) {
    let (products, height) = authored_ground_height();
    let products = Arc::new(products);
    let proof = NonRenderParticipants::authored_fixture(height.content, height.map);
    let config = SourceNearPointConfiguration {
        height,
        detect_position_collision: true,
        vmap_line_of_sight_enabled: true,
    };
    let snapshot = Arc::clone(&products)
        .load_map_height_owned(height, Some(&proof))
        .unwrap();
    let mut data = source::profile();
    data.map_id = height.map;
    data.content_digest = height.content;
    data.geometry_revision = geometry_revision(&snapshot, config).unwrap();
    data.npcs[0].ai.map_id = height.map;
    data.npcs[0].ai.content_revision = ai::movement::Revision(height.content);
    data.npcs[0].ai.spawn = ai::Point::new(MID - 66., MID - 1., z).unwrap();
    data.player.start = ai::Point::new(MID - 50., MID - 1., z).unwrap();
    data.initialize_creature_capabilities(source::creature_capability_sources(&data))
        .unwrap();
    let motion = data.source_motion.as_ref().unwrap();
    let session = Session {
        world: [8; 16],
        profile: data.digest(),
        policy: data.authority_policy,
        content: data.content_digest,
        geometry: data.geometry_revision,
        map: data.map_id,
        bodies: BTreeMap::from([(
            2,
            Body {
                radius: data.npcs[0].ai.bounding_radius,
                height: motion.npc(2).unwrap().npc_policy.settings.collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: data.player.bounding_radius,
                height: motion.player_collision_height,
            },
            speed: fixture::real(7.),
        }),
        participants: Some(proof.clone()),
        static_los: None,
        primary_near_point: Some(core::PrimaryNearPointPolicy::from_profile(&data).unwrap()),
        empty_selector_near_point: None,
        source_location: None,
        player_environment: None,
        same_polygon_route: None,
    };
    let provider = SourceNearPointProvider::bind(&session, &data, snapshot, config).unwrap();
    let bound_session = session.clone();
    let bound_profile = data.clone();
    let rebind = move |config: SourceNearPointConfiguration| {
        let snapshot = Arc::clone(&products)
            .load_map_height_owned(config.height, Some(&proof))
            .map_err(|_| Failure::MissingGeometry)?;
        SourceNearPointProvider::bind(&bound_session, &bound_profile, snapshot, config)
    };
    (
        data,
        ProductAuthority {
            provider,
            session,
            config,
            rebind: Box::new(rebind),
            fault: Arc::new(AtomicU64::new(0)),
            observed: Arc::new(Mutex::new(vec![])),
            failures: Arc::new(Mutex::new(vec![])),
            other: source::SyntheticMotionAuthority::default(),
        },
    )
}

fn enter(
    harness: &Harness,
    entry: Entry,
    profile: n::Profile,
    authority: impl n::Authority + Send + 'static,
) -> NativeController {
    NativeController::enter_component_test(
        &harness.root,
        &harness.profile,
        &harness.character,
        entry,
        profile,
        &GroundCatalog,
        authority,
    )
    .unwrap()
}
fn create(
    h: &Harness,
    profile: n::Profile,
    authority: impl n::Authority + Send + 'static,
) -> NativeController {
    enter(
        h,
        Entry::Create {
            world: [8; 16],
            seed: 7,
        },
        profile,
        authority,
    )
}
fn durable_files(harness: &Harness) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(harness.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.file_name().unwrap() != "world.lock")
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}
fn point_bits(point: ai::Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}
fn near_reply(records: &[n::TickRecord]) -> &n::AuthorityReply {
    records
        .iter()
        .flat_map(|record| &record.replies)
        .find(|reply| {
            matches!(
                reply.request.query,
                n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
            )
        })
        .unwrap()
}

#[test]
fn loaded_primary_candidate_advances_projects_persists_and_replays_exactly() {
    let h = Harness::new();
    let (data, authority) = setup(10.);
    let observed = Arc::clone(&authority.observed);
    let mut c = create(&h, data.clone(), mailbox_authority(authority));
    c.initial_events().unwrap();
    let genesis = c.world.clone();
    let events = pump_to(&mut c, 2);
    let records = n::decode_records(&n::encode_records(c.world.records()).unwrap()).unwrap();
    assert_eq!(records, c.world.records());
    let reply = near_reply(&records);
    let n::Query::SourceMotion(n::MotionQuery::NearPoint(query)) = &reply.request.query else {
        unreachable!()
    };
    assert_eq!(reply.request.binding.world, [8; 16]);
    assert_eq!(reply.request.binding.profile, data.digest());
    assert_eq!(reply.request.binding.geometry, data.geometry_revision);
    assert_eq!(reply.request.binding.tick, 2);
    assert_eq!(reply.request.binding.npc, Some(c.world.state().npcs[&2].id));
    assert_eq!(query.request.owner.position, data.npcs[0].ai.spawn);
    assert_eq!(query.request.target.pose.position, data.player.start);
    assert_eq!(query.request.binding.now_ms, 200);
    assert_eq!(query.request.binding.identity.map, data.map_id);
    assert_eq!(
        query.request.binding.identity.content.0,
        data.content_digest
    );
    assert_eq!(
        query.request.binding.identity.geometry.0,
        data.geometry_revision
    );
    assert_eq!(
        &query.searcher,
        c.world.state().npcs[&2]
            .creature_capabilities()
            .unwrap()
            .witness()
    );
    let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &reply.facts else {
        panic!("real provider must return a source spatial fact")
    };
    assert_eq!(
        spatial.request,
        motion::Query::NearPoint(query.request.clone())
    );
    let motion::Fact::NearPoint(point) = spatial.fact else {
        panic!("first-candidate success must return its exact point")
    };
    let expected = [MID - 52.5, MID - 1., 7.].map(f32::to_bits);
    assert_eq!(point_bits(point), expected);
    assert_ne!(point.z, query.request.target.pose.position.z);
    assert_eq!(*observed.lock().unwrap(), vec![reply.request.clone()]);
    assert_eq!(c.world.state().npcs[&2].ai.phase(), ai::Phase::Combat);
    assert!(c.world.state().npcs[&2].ai.position().x.as_f32() > data.npcs[0].ai.spawn.x.as_f32());
    assert!(c.world.state().npcs[&2].ai.position().z.as_f32() < 10.);
    let npc_guid = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
    let launches: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::MonsterMove {
                guid,
                stop: false,
                path,
                ..
            } if *guid == npc_guid => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(launches.len(), 1);
    assert_eq!(launches[0].last().unwrap().map(f32::to_bits), expected);
    let mut replay = genesis.clone();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());

    for field in 0..9 {
        let mut changed = records.clone();
        let reply = changed
            .iter_mut()
            .flat_map(|record| &mut record.replies)
            .find(|reply| {
                matches!(
                    reply.request.query,
                    n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
                )
            })
            .unwrap();
        let n::Query::SourceMotion(n::MotionQuery::NearPoint(query)) = &mut reply.request.query
        else {
            unreachable!()
        };
        match field {
            0 => query.request.owner.position.x = fixture::real(MID - 67.),
            1 => query.request.target.pose.position.y = fixture::real(MID - 2.),
            2 => query.request.binding.query_id += 1,
            3 => reply.request.binding.npc.as_mut().unwrap().generation += 1,
            4..=7 => {
                let mut witness = serde_json::to_value(&query.searcher).unwrap();
                match field {
                    4 => {
                        witness["source_binding"][0] =
                            (witness["source_binding"][0].as_u64().unwrap() ^ 1).into();
                    }
                    5 => witness["inhabit_type"] = 3.into(),
                    6 => witness["capability_movement_flags"] = 0x400_u32.into(),
                    _ => witness["npc"]["generation"] = 1.into(),
                }
                query.searcher = serde_json::from_value(witness).unwrap();
            }
            _ => {
                let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &mut reply.facts
                else {
                    unreachable!()
                };
                spatial.fact = motion::Fact::NearPoint(data.player.start);
            }
        }
        let mut rejected = genesis.clone();
        assert!(rejected.replay(&changed).is_err(), "field {field}");
        assert_eq!(
            rejected.checkpoint().unwrap(),
            genesis.checkpoint().unwrap()
        );
    }
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let persisted = durable_files(&h);
    let mut changed_geometry = data.clone();
    changed_geometry.geometry_revision[0] ^= 1;
    let (_, rejected_authority) = setup(10.);
    let rejected_requests = Arc::clone(&rejected_authority.observed);
    assert!(NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        Entry::Open { world: [8; 16] },
        changed_geometry,
        &GroundCatalog,
        mailbox_authority(rejected_authority),
    )
    .is_err());
    assert_eq!(durable_files(&h), persisted);
    assert!(rejected_requests.lock().unwrap().is_empty());
    let (reopened_profile, authority) = setup(10.);
    assert_eq!(reopened_profile, data);
    let reopened = enter(
        &h,
        Entry::Open { world: [8; 16] },
        data,
        mailbox_authority(authority),
    );
    assert_eq!(reopened.world.state_digest(), digest);
}

#[test]
fn primary_faults_retain_exact_input_and_store_failure_publishes_repaired_tick_once() {
    let h = Harness::new();
    let (data, authority) = setup(10.);
    let fault = Arc::clone(&authority.fault);
    let observed = Arc::clone(&authority.observed);
    let failures = Arc::clone(&authority.failures);
    let mut c = create(&h, data.clone(), authority);
    c.initial_events().unwrap();
    pump_to(&mut c, 1);
    let before = c.world.checkpoint().unwrap();
    let persisted = durable_files(&h);
    let generation = c.store.generation();
    let mut replay = c.world.clone();
    let mut moved = [
        data.player.start.x.as_f32(),
        data.player.start.y.as_f32(),
        10.,
    ];
    moved[0] += 0.25;
    c.receive(move_to(moved)).unwrap();
    let mut inputs = None;
    for mode in 1..=7 {
        fault.store(mode, Ordering::SeqCst);
        if mode > 1 {
            c.retry_tick().unwrap();
        }
        let out = c.pump(if mode == 1 {
            c.tick_duration()
        } else {
            Duration::ZERO
        });
        assert_eq!(out.ticks, 0, "mode {mode}");
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(durable_files(&h), persisted);
        let pending = &c.pending.as_ref().unwrap().inputs;
        if let Some(inputs) = &inputs {
            assert_eq!(pending, inputs);
        } else {
            inputs = Some(pending.clone());
        }
        assert_eq!(
            c.position_xyz(),
            [
                data.player.start.x.as_f32(),
                data.player.start.y.as_f32(),
                10.
            ]
        );
        let waiting = c.pump(Duration::from_secs(60));
        assert_eq!(waiting.ticks, 0);
        assert!(waiting.events.is_empty());
    }
    assert_eq!(failures.lock().unwrap().len(), 6);
    assert!(failures.lock().unwrap().contains(&Failure::Binding));
    assert!(failures
        .lock()
        .unwrap()
        .contains(&Failure::UnprovenParticipants));
    fault.store(0, Ordering::SeqCst);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored primary store fault",
    )
    .unwrap();
    c.retry_tick().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(c.pending.is_none());
    assert_eq!(c.world.state().next_sequence, 1);
    assert_eq!(c.store.generation(), generation);
    assert_eq!(
        c.position_xyz(),
        [
            data.player.start.x.as_f32(),
            data.player.start.y.as_f32(),
            10.
        ]
    );
    let recorded = c.world.last_tick().unwrap().clone();
    assert_eq!(
        recorded.inputs,
        inputs
            .unwrap()
            .into_iter()
            .map(|owned| owned.input)
            .collect::<Vec<_>>()
    );
    replay.replay(std::slice::from_ref(&recorded)).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
    let attempts = observed.lock().unwrap();
    assert_eq!(attempts.len(), 8);
    assert!(attempts.iter().all(|attempt| attempt == &attempts[0]));
    assert_eq!(
        near_reply(std::slice::from_ref(&recorded)).request,
        attempts[0]
    );
    drop(attempts);
    let digest = c.world.state_digest();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    assert_eq!(c.world.state_digest(), digest);
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    let npc_guid = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
    assert_eq!(
        out.events
            .iter()
            .filter(|event| matches!(event,
            SessionEvent::MonsterMove { guid, stop: false, .. } if *guid == npc_guid))
            .count(),
        1
    );
    assert_eq!(c.position_xyz(), moved);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().next_sequence, 1);
    assert!(c.world.records().is_empty());
    c.shutdown().unwrap();
    drop(c);
    let (_, authority) = setup(10.);
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, authority);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.world.state().next_sequence, 1);
}

#[test]
fn actual_primary_vertical_fallback_remains_unavailable_without_any_publication() {
    let h = Harness::new();
    // Ground is 7; abs(7-11) >= (distance2.5 + owner radius.5) + target radius.5.
    let (data, authority) = setup(11.);
    let failures = Arc::clone(&authority.failures);
    let observed = Arc::clone(&authority.observed);
    let mut c = create(&h, data.clone(), mailbox_authority(authority));
    c.initial_events().unwrap();
    pump_to(&mut c, 1);
    let before = c.world.checkpoint().unwrap();
    let persisted = durable_files(&h);
    let generation = c.store.generation();
    c.receive(move_to([
        data.player.start.x.as_f32(),
        data.player.start.y.as_f32(),
        11.,
    ]))
    .unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 0);
    assert!(out.events.is_empty());
    assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
    let inputs = c.pending.as_ref().unwrap().inputs.clone();
    assert_eq!(c.world.checkpoint().unwrap(), before);
    assert_eq!(c.store.generation(), generation);
    assert_eq!(durable_files(&h), persisted);
    assert_eq!(
        *failures.lock().unwrap(),
        vec![Failure::UnsupportedNearPointFallback]
    );
    c.retry_tick().unwrap();
    let retry = c.pump(Duration::ZERO);
    assert_eq!(retry.ticks, 0);
    assert!(retry.events.is_empty());
    assert!(matches!(retry.pause.unwrap().kind, PauseKind::Authority));
    assert_eq!(c.pending.as_ref().unwrap().inputs, inputs);
    assert_eq!(c.world.checkpoint().unwrap(), before);
    assert_eq!(c.store.generation(), generation);
    assert_eq!(durable_files(&h), persisted);
    let attempts = observed.lock().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0], attempts[1]);
    assert!(!c
        .world
        .records()
        .iter()
        .flat_map(|record| &record.replies)
        .any(|reply| {
            matches!(
                reply.request.query,
                n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
            )
        }));
}

#[test]
fn mailbox_primary_retry_saves_then_publishes_exact_staged_tick_once() {
    let h = Harness::new();
    let (data, product) = setup(10.);
    let fault = Arc::clone(&product.fault);
    let observed = Arc::clone(&product.observed);
    let failures = Arc::clone(&product.failures);
    let mut c = create(&h, data.clone(), mailbox_authority(product));
    c.initial_events().unwrap();
    pump_to(&mut c, 1);
    let before = c.world.checkpoint().unwrap();
    let persisted = durable_files(&h);
    let generation = c.store.generation();
    let mut replay = c.world.clone();
    let moved = [MID - 49.75, MID - 1., 10.];
    c.receive(move_to(moved)).unwrap();
    fault.store(1, Ordering::SeqCst);
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 0);
    assert!(out.events.is_empty());
    assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
    let pending = c.pending.as_ref().unwrap().inputs.clone();
    assert_eq!(c.world.checkpoint().unwrap(), before);
    assert_eq!(c.store.generation(), generation);
    assert_eq!(durable_files(&h), persisted);
    assert_eq!(*failures.lock().unwrap(), vec![Failure::MissingGeometry]);
    assert_eq!(observed.lock().unwrap().len(), 1);
    assert_eq!(c.pump(Duration::from_secs(60)).ticks, 0);
    assert_eq!(observed.lock().unwrap().len(), 1);

    fault.store(0, Ordering::SeqCst);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored mailbox primary save fault",
    )
    .unwrap();
    c.retry_tick().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert_eq!(c.store.generation(), generation);
    assert_eq!(c.position_xyz(), [MID - 50., MID - 1., 10.]);
    assert!(c.pending.is_none());
    assert_eq!(c.world.state().next_sequence, 1);
    let record = c.world.last_tick().unwrap().clone();
    assert_eq!(
        record.inputs,
        pending
            .into_iter()
            .map(|owned| owned.input)
            .collect::<Vec<_>>()
    );
    replay.replay(std::slice::from_ref(&record)).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
    let attempts = observed.lock().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0], attempts[1]);
    assert_eq!(
        near_reply(std::slice::from_ref(&record)).request,
        attempts[0]
    );
    drop(attempts);
    let digest = c.world.state_digest();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.position_xyz(), moved);
    let npc = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
    assert_eq!(
        out.events
            .iter()
            .filter(|event| matches!(event,
                SessionEvent::MonsterMove { guid, stop: false, .. } if *guid == npc))
            .count(),
        1
    );
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(observed.lock().unwrap().len(), 2);
    c.shutdown().unwrap();
    drop(c);
    let (_, product) = setup(10.);
    let reopened = enter(
        &h,
        Entry::Open { world: [8; 16] },
        data,
        mailbox_authority(product),
    );
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.world.state().next_sequence, 1);
}

#[test]
fn mailbox_primary_malformed_facts_do_not_commit_project_or_rewrite_store() {
    for fault_kind in [2, 3] {
        let h = Harness::new();
        let (data, product) = setup(10.);
        let fault = Arc::clone(&product.fault);
        let observed = Arc::clone(&product.observed);
        let mut c = create(&h, data, mailbox_authority(product));
        c.initial_events().unwrap();
        pump_to(&mut c, 1);
        let before = c.world.checkpoint().unwrap();
        let persisted = durable_files(&h);
        let generation = c.store.generation();
        c.receive(move_to([MID - 49.75, MID - 1., 10.])).unwrap();
        fault.store(fault_kind, Ordering::SeqCst);
        let out = c.pump(c.tick_duration());
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Simulation));
        let pending = c.pending.as_ref().unwrap().inputs.clone();
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(durable_files(&h), persisted);
        assert_eq!(c.position_xyz(), [MID - 50., MID - 1., 10.]);
        assert_eq!(observed.lock().unwrap().len(), 1);
        assert!(c.retry_tick().is_err());
        let waiting = c.pump(Duration::from_secs(60));
        assert_eq!(waiting.ticks, 0);
        assert!(waiting.events.is_empty());
        assert_eq!(c.pending.as_ref().unwrap().inputs, pending);
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(durable_files(&h), persisted);
        assert_eq!(observed.lock().unwrap().len(), 1);
    }
}

#[path = "near_point_empty_tests.rs"]
mod near_point_empty_tests;
