// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded dry-product filter prerequisites at runtime-owned route poses.
//! Other authorities and the separate successful-route replay are synthetic.
use super::*;
use crate::net::p2p::native_geometry::{
    authored_ground_height, authored_route_filter_revision as geometry_revision,
    core::{Body, Failure, NonRenderParticipants, PlayerMotion, Session},
    RouteFilterRequest, SourceRouteFilterProvider,
};
use benilla_sim::{creature_ai as ai, creature_motion as motion, route_filter};
use n::synthetic_motion_fixture as source;
use std::{collections::BTreeMap, time::Instant};

const MID: f32 = 0.5f32 * (64.0 * 533.333_3);

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct FilterObservation {
    request: RouteFilterRequest,
    owner_position: [u32; 3],
    masks: [route_filter::LiquidMask; 2],
    tickets: [u64; 2],
    filter: route_filter::Filter,
}

struct ProductAuthority {
    provider: SourceRouteFilterProvider,
    session: Session,
    // 1 expires the query; 2 changes session geometry; 3 forges the frozen
    // context; 4 changes snapshot identity; 5 removes the observation ticket.
    fault: Arc<AtomicU64>,
    queries: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    observations: Arc<Mutex<Vec<FilterObservation>>>,
    failures: Arc<Mutex<Vec<Failure>>>,
    other: source::SyntheticMotionAuthority,
}
impl n::Authority for ProductAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.queries.lock().unwrap().push(request.clone());
        let n::Query::SourceMotion(n::MotionQuery::Route(route)) = &request.query else {
            return self.other.resolve(request);
        };
        let fault = self.fault.load(Ordering::SeqCst);
        let result = (|| {
            let mut session = self.session.clone();
            if fault == 2 {
                session.geometry[0] ^= 1;
            }
            let bound = self.provider.begin(&session, request)?;
            let mut query = RouteFilterRequest {
                snapshot: self.provider.binding(),
                context: request.clone(),
                first_ticket: 41,
            };
            match fault {
                3 => {
                    let n::Query::SourceMotion(n::MotionQuery::Route(route)) =
                        &mut query.context.query
                    else {
                        unreachable!()
                    };
                    route.owner_position.x = fixture::real(route.owner_position.x.as_f32() + 1.);
                }
                4 => query.snapshot.inventory[0] ^= 1,
                5 => query.first_ticket = 0,
                _ => {}
            }
            let deadline = Instant::now()
                + if fault == 1 {
                    Duration::ZERO
                } else {
                    Duration::from_secs(5)
                };
            let dependency = bound.query(query.clone(), deadline)?;
            assert_eq!(dependency.request(), &query);
            assert_eq!(
                dependency.owner_position(),
                point_bits(route.owner_position)
            );
            let liquid = dependency.liquid_observations();
            for observation in liquid {
                assert_eq!(observation.reply.request.snapshot, query.snapshot);
                assert_eq!(
                    observation.reply.request.position,
                    dependency.owner_position()
                );
            }
            self.observations.lock().unwrap().push(FilterObservation {
                request: query,
                owner_position: dependency.owner_position(),
                masks: liquid.map(|observation| observation.mask),
                tickets: liquid.map(|observation| observation.reply.request.ticket),
                filter: dependency.filter(),
            });
            Ok(())
        })();
        if let Err(failure) = result {
            self.failures.lock().unwrap().push(failure);
            return Err(match failure {
                Failure::Binding => n::NativeError::AuthorityBinding,
                Failure::Invalid => n::NativeError::InvalidInput,
                _ => n::NativeError::AuthorityUnavailable(
                    "loaded route filter prerequisite unavailable",
                ),
            });
        }
        // Even a real include/exclude filter is not a complete source route.
        // No Fact::Route or successful movement receipt is manufactured here.
        Err(n::NativeError::AuthorityUnavailable(
            "source owner route remains unavailable after dry filter prerequisite",
        ))
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.other.observe_derived(reply)
    }
}

fn setup(covered: bool) -> (n::Profile, ProductAuthority) {
    let (products, _) = authored_ground_height();
    let snapshot = products.load_dry_liquid().unwrap();
    let binding = snapshot.binding();
    let mut data = source::profile();
    data.map_id = binding.map;
    data.content_digest = binding.content;
    data.geometry_revision = geometry_revision(binding);
    data.npcs[0].ai.map_id = binding.map;
    data.npcs[0].ai.content_revision = ai::movement::Revision(binding.content);
    let [x, y] = if covered {
        [MID - 21., MID - 1.]
    } else {
        [0., 0.]
    };
    data.npcs[0].ai.spawn = ai::Point::new(x, y, 10.).unwrap();
    data.player.start = ai::Point::new(x + 16., y, 10.).unwrap();
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
                height: motion.npc_policy.settings.collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: data.player.bounding_radius,
                height: motion.player_collision_height,
            },
            speed: fixture::real(7.),
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
    let provider = SourceRouteFilterProvider::bind(&session, &data, snapshot).unwrap();
    (
        data,
        ProductAuthority {
            provider,
            session,
            fault: Arc::new(AtomicU64::new(0)),
            queries: Arc::new(Mutex::new(vec![])),
            observations: Arc::new(Mutex::new(vec![])),
            failures: Arc::new(Mutex::new(vec![])),
            other: source::SyntheticMotionAuthority::default(),
        },
    )
}

fn enter(
    h: &Harness,
    entry: Entry,
    data: n::Profile,
    authority: impl n::Authority + Send + 'static,
) -> NativeController {
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        entry,
        data,
        &GroundCatalog,
        authority,
    )
    .unwrap()
}
fn create(
    h: &Harness,
    data: n::Profile,
    authority: impl n::Authority + Send + 'static,
) -> NativeController {
    enter(
        h,
        Entry::Create {
            world: [8; 16],
            seed: 7,
        },
        data,
        authority,
    )
}
fn point_bits(point: ai::Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}
fn durable_files(h: &Harness) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(h.path())
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
fn route_query(request: &n::AuthorityRequest) -> Option<&n::CreatureRouteRequest> {
    match &request.query {
        n::Query::SourceMotion(n::MotionQuery::Route(route)) => Some(route),
        _ => None,
    }
}
fn route_reply(records: &[n::TickRecord]) -> &n::AuthorityReply {
    records
        .iter()
        .flat_map(|record| &record.replies)
        .find(|reply| route_query(&reply.request).is_some())
        .unwrap()
}

#[test]
fn loaded_filter_and_deadline_never_publish_a_route_or_consume_the_staged_tick() {
    let h = Harness::new();
    let (data, authority) = setup(true);
    let fault = Arc::clone(&authority.fault);
    let queries = Arc::clone(&authority.queries);
    let observations = Arc::clone(&authority.observations);
    let failures = Arc::clone(&authority.failures);
    let mut c = create(&h, data.clone(), authority);
    c.initial_events().unwrap();
    pump_to(&mut c, 1);
    let before = c.world.checkpoint().unwrap();
    let records = c.world.records().to_vec();
    let persisted = durable_files(&h);
    let generation = c.store.generation();
    let position = c.position_xyz();
    let facing = c.facing();
    let npc = c.world.state().npcs[&2].id;
    let npc_guid = c.projection.npc_guid(npc).unwrap();
    let mut moved = position;
    moved[0] += 0.25;
    c.receive(move_to(moved)).unwrap();
    let mut inputs = None;
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
        assert_eq!(out.ticks, 0, "mode {mode}");
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.world.records(), records);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(durable_files(&h), persisted);
        assert_eq!(c.position_xyz(), position);
        assert_eq!(c.facing(), facing);
        assert_eq!(c.projection.npc_guid(npc), Some(npc_guid));
        let pending = &c.pending.as_ref().unwrap().inputs;
        if let Some(inputs) = &inputs {
            assert_eq!(pending, inputs);
        } else {
            assert_eq!(pending.len(), 1);
            inputs = Some(pending.clone());
        }
        let query_count = queries.lock().unwrap().len();
        let waiting = c.pump(Duration::from_secs(60));
        assert_eq!(waiting.ticks, 0);
        assert!(waiting.events.is_empty());
        assert_eq!(queries.lock().unwrap().len(), query_count);
    }
    assert_eq!(*failures.lock().unwrap(), [Failure::MissingGeometry]);
    let queries = queries.lock().unwrap();
    let routes: Vec<_> = queries
        .iter()
        .filter(|q| route_query(q).is_some())
        .collect();
    assert_eq!(routes.len(), 3);
    assert!(routes.iter().all(|request| *request == routes[0]));
    let first_route = queries
        .iter()
        .position(|q| route_query(q).is_some())
        .unwrap();
    assert!(queries[..first_route].iter().any(|q| matches!(
        q.query,
        n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
    )));
    let request = routes[0];
    let route = route_query(request).unwrap();
    assert_eq!(request.binding.tick, 2);
    assert_eq!(request.binding.world, [8; 16]);
    assert_eq!(request.binding.profile, data.digest());
    assert_eq!(request.binding.geometry, data.geometry_revision);
    assert_eq!(request.binding.npc, Some(npc));
    assert_eq!(route.owner_position, data.npcs[0].ai.spawn);
    assert_eq!(route.owner_position, route.request.origin);
    assert_ne!(route.owner_position, route.request.goal);
    assert_eq!(route.request.binding.now_ms, 200);
    assert_eq!(route.request.binding.identity.map, data.map_id);
    assert_eq!(
        route.request.binding.identity.content.0,
        data.content_digest
    );
    assert_eq!(
        route.request.binding.identity.geometry.0,
        data.geometry_revision
    );
    assert_eq!(
        &route.owner,
        c.world.state().npcs[&2]
            .creature_capabilities()
            .unwrap()
            .witness()
    );
    let observed = observations.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert_eq!(observed[0], observed[1]);
    let filter = &observed[0];
    assert_eq!(filter.request.context, *request);
    assert_eq!(filter.owner_position, point_bits(route.owner_position));
    assert_eq!(
        filter.masks,
        [
            route_filter::LiquidMask::AllLiquids,
            route_filter::LiquidMask::WaterOrOcean
        ]
    );
    assert_eq!(
        filter.masks.map(route_filter::LiquidMask::bits),
        [0x0f, 0x0a]
    );
    assert_eq!(filter.tickets, [41, 42]);
    assert_eq!(filter.filter.include_flags(), 1);
    assert_eq!(filter.filter.exclude_flags(), 0);
    assert_eq!(filter.filter.branch(), route_filter::Branch::GroundDry);
    drop(observed);
    drop(queries);
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let (_, authority) = setup(true);
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, authority);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.world.state().next_sequence, 0);
    assert_eq!(reopened.tick(), 1);
}

#[test]
fn changed_geometry_and_forged_filter_context_are_terminal_without_publication() {
    for mode in 2..=5 {
        let h = Harness::new();
        let (data, authority) = setup(true);
        authority.fault.store(mode, Ordering::SeqCst);
        let queries = Arc::clone(&authority.queries);
        let observations = Arc::clone(&authority.observations);
        let failures = Arc::clone(&authority.failures);
        let mut c = create(&h, data.clone(), authority);
        c.initial_events().unwrap();
        pump_to(&mut c, 1);
        let before = c.world.checkpoint().unwrap();
        let records = c.world.records().to_vec();
        let generation = c.store.generation();
        let persisted = durable_files(&h);
        let position = c.position_xyz();
        let facing = c.facing();
        let mut moved = position;
        moved[0] += 0.25;
        c.receive(move_to(moved)).unwrap();
        let out = c.pump(c.tick_duration());
        assert_eq!(out.ticks, 0, "mode {mode}");
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Simulation));
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.world.records(), records);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(durable_files(&h), persisted);
        assert_eq!(c.position_xyz(), position);
        assert_eq!(c.facing(), facing);
        let inputs = c.pending.as_ref().unwrap().inputs.clone();
        assert_eq!(inputs.len(), 1);
        assert!(matches!(c.retry_tick(), Err(Error::Paused)));
        let query_count = queries.lock().unwrap().len();
        let waiting = c.pump(Duration::from_secs(60));
        assert_eq!(waiting.ticks, 0);
        assert!(waiting.events.is_empty());
        assert_eq!(queries.lock().unwrap().len(), query_count);
        assert_eq!(c.pending.as_ref().unwrap().inputs, inputs);
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(durable_files(&h), persisted);
        assert!(observations.lock().unwrap().is_empty());
        assert_eq!(*failures.lock().unwrap(), [Failure::Binding]);
        assert_eq!(
            queries
                .lock()
                .unwrap()
                .iter()
                .filter(|q| route_query(q).is_some())
                .count(),
            1
        );
        let digest = c.world.state_digest();
        c.shutdown().unwrap();
        drop(c);
        let (_, authority) = setup(true);
        let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, authority);
        assert_eq!(reopened.world.state_digest(), digest);
        assert_eq!(reopened.world.state().next_sequence, 0);
        assert_eq!(reopened.tick(), 1);
    }
}

#[test]
fn missing_owner_terrain_never_becomes_a_dry_filter_or_committed_route() {
    let h = Harness::new();
    let (data, authority) = setup(false);
    let queries = Arc::clone(&authority.queries);
    let observations = Arc::clone(&authority.observations);
    let failures = Arc::clone(&authority.failures);
    let mut c = create(&h, data, authority);
    c.initial_events().unwrap();
    pump_to(&mut c, 1);
    let before = c.world.checkpoint().unwrap();
    let generation = c.store.generation();
    let persisted = durable_files(&h);
    let position = c.position_xyz();
    let mut moved = position;
    moved[0] += 0.25;
    c.receive(move_to(moved)).unwrap();
    let mut inputs = None;
    for retry in [false, true] {
        if retry {
            c.retry_tick().unwrap();
        }
        let out = c.pump(if retry {
            Duration::ZERO
        } else {
            c.tick_duration()
        });
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(durable_files(&h), persisted);
        assert_eq!(c.position_xyz(), position);
        let pending = &c.pending.as_ref().unwrap().inputs;
        if let Some(inputs) = &inputs {
            assert_eq!(pending, inputs);
        } else {
            inputs = Some(pending.clone());
        }
    }
    assert!(observations.lock().unwrap().is_empty());
    assert_eq!(*failures.lock().unwrap(), [Failure::MissingGeometry; 2]);
    let queries = queries.lock().unwrap();
    let routes: Vec<_> = queries
        .iter()
        .filter(|q| route_query(q).is_some())
        .collect();
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0], routes[1]);
    assert_eq!(
        point_bits(route_query(routes[0]).unwrap().owner_position),
        [0.0f32, 0., 10.].map(f32::to_bits)
    );
}

#[test]
fn explicitly_synthetic_successful_routes_roundtrip_replay_and_reopen_typed_owner_requests() {
    let h = Harness::new();
    let (data, _) = setup(true);
    // This test establishes journal/store binding only. It does not pass a dry
    // filter dependency off as OwnerPathFinder or real segment evidence.
    let mut c = create(
        &h,
        data.clone(),
        source::SyntheticMotionAuthority::default(),
    );
    c.initial_events().unwrap();
    let genesis = c.world.clone();
    let events = pump_to(&mut c, 2);
    let records = n::decode_records(&n::encode_records(c.world.records()).unwrap()).unwrap();
    assert_eq!(records, c.world.records());
    assert!(records
        .iter()
        .all(|record| record.version == n::NATIVE_FORMAT_VERSION));
    let reply = route_reply(&records);
    let route = route_query(&reply.request).unwrap();
    assert_eq!(route.owner_position, data.npcs[0].ai.spawn);
    assert_eq!(route.owner_position, route.request.origin);
    assert_eq!(route.owner.npc(), c.world.state().npcs[&2].id);
    let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &reply.facts else {
        panic!("synthetic source motion must return a spatial receipt")
    };
    assert_eq!(spatial.request, motion::Query::Route(route.request.clone()));
    let motion::Fact::Route(motion::RouteOutcome::Complete(path)) = &spatial.fact else {
        panic!("this separate replay fixture must have a complete synthetic route")
    };
    assert_eq!(path.source, motion::RouteSource::SyntheticFixture);
    let npc_guid = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event,
                SessionEvent::MonsterMove { guid, stop: false, .. } if *guid == npc_guid
            ))
            .count(),
        1
    );
    let mut replay = genesis.clone();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
    for field in 0..12 {
        let mut changed = records.clone();
        let reply = changed
            .iter_mut()
            .flat_map(|record| &mut record.replies)
            .find(|reply| route_query(&reply.request).is_some())
            .unwrap();
        let n::Query::SourceMotion(n::MotionQuery::Route(route)) = &mut reply.request.query else {
            unreachable!()
        };
        match field {
            0 => route.owner_position.x = fixture::real(MID - 22.),
            1..=4 => {
                let mut owner = serde_json::to_value(&route.owner).unwrap();
                match field {
                    1 => owner["npc"]["generation"] = 1.into(),
                    2 => {
                        owner["source_binding"][0] =
                            (owner["source_binding"][0].as_u64().unwrap() ^ 1).into();
                    }
                    3 => owner["inhabit_type"] = 3.into(),
                    _ => owner["capability_movement_flags"] = 0x400_u32.into(),
                }
                route.owner = serde_json::from_value(owner).unwrap();
            }
            5 => route.request.binding.query_id += 1,
            6 => route.request.origin.x = fixture::real(MID - 22.),
            7 => route.request.launch_origin.y = fixture::real(MID - 2.),
            8 => route.request.goal.z = fixture::real(11.),
            9 => reply.request.binding.geometry[0] ^= 1,
            10 => reply.request.binding.tick += 1,
            _ => {
                reply.request.query = n::Query::SourceMotion(n::MotionQuery::Spatial(
                    motion::Query::Route(route.request.clone()),
                ));
            }
        }
        let mut rejected = genesis.clone();
        assert!(rejected.replay(&changed).is_err(), "field {field}");
        assert_eq!(
            rejected.checkpoint().unwrap(),
            genesis.checkpoint().unwrap()
        );
    }
    let mut old_format = records.clone();
    old_format[0].version = 22;
    assert!(n::decode_records(&n::encode_records(&old_format).unwrap()).is_err());
    let digest = c.world.state_digest();
    let owner = route.owner.clone();
    c.shutdown().unwrap();
    drop(c);
    let persisted = durable_files(&h);
    let mut changed_geometry = data.clone();
    changed_geometry.geometry_revision[0] ^= 1;
    assert!(NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        Entry::Open { world: [8; 16] },
        changed_geometry,
        &GroundCatalog,
        source::SyntheticMotionAuthority::default(),
    )
    .is_err());
    assert_eq!(durable_files(&h), persisted);
    let reopened = enter(
        &h,
        Entry::Open { world: [8; 16] },
        data,
        source::SyntheticMotionAuthority::default(),
    );
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(
        reopened.world.state().npcs[&2]
            .creature_capabilities()
            .unwrap()
            .witness(),
        &owner
    );
}

#[path = "route_smoothing_tests.rs"]
mod route_smoothing_tests;

#[path = "route_warm_cache_tests.rs"]
mod route_warm_cache_tests;

#[path = "route_point_path_tests.rs"]
mod route_point_path_tests;

#[path = "route_straight_path_tests.rs"]
mod route_straight_path_tests;
