// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual controller/store tests with real StarterInput initialization and authored
//! MPQ-loaded ChrRaces/CDI/CMD. Spatial successes remain SyntheticFixture only.
use super::*;
use benilla_sim::creature_motion as motion;
use n::{
    synthetic_creature_model_fixture as models, synthetic_motion_fixture as source,
    synthetic_player_model_fixture as player,
};

fn profile() -> n::Profile {
    profile_with_fields(1.75, 2.5, 2.875)
}
fn profile_with_fields(display_scale: f32, model_scale: f32, height: f32) -> n::Profile {
    let mut p = player::unowned_profile();
    p.npcs[0].display_id = 34;
    p.initialize_creature_collision_widths(n::CreatureCollisionWidthRows::Complete(vec![
        n::CreatureCollisionWidthSource::from_initial_model(
            2,
            models::selection(),
            &models::catalog(),
        )
        .unwrap(),
    ]))
    .unwrap();
    p.initialize_creature_collision_heights().unwrap();
    p.initialize_player_collision_height(
        &player::catalog(),
        &models::catalog_with_height_fields(1.375, height, model_scale, display_scale, false),
    )
    .unwrap();
    p
}
struct ModelCatalog;
impl ClientCatalog for ModelCatalog {
    fn build(&self) -> u32 {
        Catalog.build()
    }
    fn manifest_digest(&self) -> runtime::Digest {
        Catalog.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        Catalog.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        id == 34
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

fn movement(flags: u32, x: f32) -> ClientCommand {
    ClientCommand::Move {
        kind: MoveKind::Heartbeat,
        flags,
        pos: [x, 0.0, 0.0],
        orientation: 0.75,
        pitch: 0.0,
        fall_time: 0,
        jump: None,
        transport: None,
    }
}

#[derive(Default)]
struct ObservedAuthority {
    // Zero succeeds; one refuses the selected query; 2+ corrupt its echoed field.
    fault: Arc<AtomicU64>,
    near: bool,
    observed: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    profile: Option<n::Profile>,
    player: Option<PlayerId>,
    inner: source::SyntheticMotionAuthority,
}
impl n::Authority for ObservedAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.observed.lock().unwrap().push(request.clone());
        let profile = self.profile.as_ref().unwrap();
        let selected = match &request.query {
            n::Query::SourceMotion(n::MotionQuery::NearPoint(q)) => {
                q.require_target_collision_height(
                    profile,
                    request.binding.world,
                    self.player.unwrap(),
                )?;
                self.near
            }
            n::Query::SourceMotion(n::MotionQuery::Route(q)) => {
                q.require_target_collision_height(
                    profile,
                    request.binding.world,
                    self.player.unwrap(),
                )?;
                q.require_collision_height(profile)?;
                q.require_collision_width(profile)?;
                !self.near
            }
            _ => false,
        };
        let fault = self.fault.load(Ordering::SeqCst);
        if selected && fault == 1 {
            return Err(n::NativeError::AuthorityUnavailable(
                "authored player-height query fault",
            ));
        }
        let mut reply = self.inner.resolve(request)?;
        if selected && fault >= 2 {
            forge_query(&mut reply.request.query, (fault - 2) as u8);
        }
        Ok(reply)
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.inner.observe_derived(reply)
    }
}
fn enter(
    h: &Harness,
    entry: Entry,
    data: n::Profile,
    mut authority: ObservedAuthority,
) -> NativeController {
    authority.profile = Some(data.clone());
    authority.player = Some(
        Selection::from_profile(&h.profile, &h.character)
            .unwrap()
            .player,
    );
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        entry,
        data,
        &ModelCatalog,
        authority,
    )
    .unwrap()
}
fn create(h: &Harness, data: n::Profile, authority: ObservedAuthority) -> NativeController {
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
fn route_query(request: &n::AuthorityRequest) -> Option<&n::CreatureRouteRequest> {
    match &request.query {
        n::Query::SourceMotion(n::MotionQuery::Route(q)) => Some(q),
        _ => None,
    }
}
fn routes(requests: &[n::AuthorityRequest]) -> Vec<&n::CreatureRouteRequest> {
    requests.iter().filter_map(route_query).collect()
}
fn durable_files(h: &Harness) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(h.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap() != "world.lock")
        .map(|p| {
            let bytes = fs::read(&p).unwrap();
            (p, bytes)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}
fn forge_target(target: &mut motion::Target, field: u8) {
    match field {
        15 => target.collision_height = fixture::real(17.0),
        16 => target.id += 1,
        17 => target.pose.position = fixture::point(0.125),
        18 => target.pose.orientation = fixture::real(1.25),
        19 => {
            target.movement_flags =
                motion::GroundMovementFlags::from_bits(target.movement_flags.bits() ^ 1).unwrap()
        }
        20 => target.combat_reach = fixture::real(7.0),
        21 => target.bounding_radius = fixture::real(9.0),
        _ => unreachable!(),
    }
}
fn forge_owned(target: &mut Option<n::PlayerCollisionHeightTarget>, field: u8) {
    if field == 14 {
        *target = None;
        return;
    }
    let mut value = serde_json::to_value(target.as_ref().unwrap()).unwrap();
    let witness = &mut value["witness"];
    match field {
        0 => witness["world"][0] = 9.into(),
        1 => {
            witness["player"]["owner"][0] =
                (witness["player"]["owner"][0].as_u64().unwrap() ^ 1).into()
        }
        2 => {
            witness["player"]["character"][0] =
                (witness["player"]["character"][0].as_u64().unwrap() ^ 1).into()
        }
        3..=6 => {
            let key = ["content", "profile", "geometry", "source_binding"][usize::from(field - 3)];
            witness[key][0] = (witness[key][0].as_u64().unwrap() ^ 1).into();
        }
        7 => witness["map"] = 9.into(),
        8 => witness["native_display_id"] = 34.into(),
        9 => witness["model_id"] = 9.into(),
        10..=13 => {
            let key = [
                "stored_object_scale",
                "effective_object_scale",
                "raw_height",
                "height",
            ][usize::from(field - 10)];
            witness[key] = 17.0_f32.to_bits().into();
        }
        22 => value["target"]["pose"]["orientation"] = 1.25_f32.to_bits().into(),
        _ => unreachable!(),
    }
    *target = Some(serde_json::from_value(value).unwrap());
}
fn forge_query(query: &mut n::Query, field: u8) {
    match query {
        n::Query::SourceMotion(n::MotionQuery::NearPoint(q)) => {
            if (15..=21).contains(&field) {
                forge_target(&mut q.request.target, field);
            } else {
                forge_owned(&mut q.target_player_height, field);
            }
        }
        n::Query::SourceMotion(n::MotionQuery::Route(q)) => match field {
            15..=21 => forge_target(q.request.target.as_mut().unwrap(), field),
            23 => q.collision_width = None,
            24 => {
                let mut value = serde_json::to_value(q.collision_width.as_ref().unwrap()).unwrap();
                value["source_binding"][0] =
                    (value["source_binding"][0].as_u64().unwrap() ^ 1).into();
                q.collision_width = Some(serde_json::from_value(value).unwrap());
            }
            _ => forge_owned(&mut q.target_player_height, field),
        },
        _ => unreachable!(),
    }
}
fn require_target<'a>(
    q: &'a n::AuthorityRequest,
    p: &n::Profile,
    player: PlayerId,
) -> Result<&'a n::PlayerCollisionHeightWitness, n::NativeError> {
    match &q.query {
        n::Query::SourceMotion(n::MotionQuery::NearPoint(q2)) => {
            q2.require_target_collision_height(p, q.binding.world, player)
        }
        n::Query::SourceMotion(n::MotionQuery::Route(q2)) => {
            q2.require_target_collision_height(p, q.binding.world, player)
        }
        _ => unreachable!(),
    }
}

#[test]
fn initial_player_height_binds_actual_chase_moved_target_replay_and_reopen() {
    let h = Harness::new();
    let data = profile();
    let observed = Arc::new(Mutex::new(vec![]));
    let mut c = create(
        &h,
        data.clone(),
        ObservedAuthority {
            observed: observed.clone(),
            ..Default::default()
        },
    );
    c.initial_events().unwrap();
    let genesis = c.world.clone();
    let initial = c.world.state().player_collision_height().unwrap().clone();
    let player_id = c.world.state().player;
    let events = pump_to(&mut c, 2);
    let records = n::decode_records(&n::encode_records(c.world.records()).unwrap()).unwrap();
    assert!(records
        .iter()
        .all(|r| r.version == n::NATIVE_FORMAT_VERSION));
    let replies: Vec<_> = records
        .iter()
        .flat_map(|r| &r.replies)
        .filter(|r| {
            matches!(
                r.request.query,
                n::Query::SourceMotion(n::MotionQuery::NearPoint(_) | n::MotionQuery::Route(_))
            )
        })
        .collect();
    assert!(replies.iter().any(|r| matches!(
        r.request.query,
        n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
    )));
    let first = replies
        .iter()
        .find_map(|r| route_query(&r.request))
        .unwrap()
        .clone();
    let mut missing = serde_json::to_value(&first).unwrap();
    missing
        .as_object_mut()
        .unwrap()
        .remove("target_player_height");
    assert!(serde_json::from_value::<n::CreatureRouteRequest>(missing).is_err());
    let mut absent = first.clone();
    absent.target_player_height = None;
    assert!(absent
        .require_target_collision_height(&data, [8; 16], player_id)
        .is_err());
    let height = initial.witness();
    assert_eq!(height.player(), player_id);
    assert_eq!(height.world(), [8; 16]);
    assert_eq!(height.native_display_id(), 31);
    assert_eq!(height.model_id(), 7);
    assert_eq!(height.stored_object_scale().bits(), 4.375_f32.to_bits());
    assert_eq!(height.effective_object_scale().bits(), 4.375_f32.to_bits());
    assert_eq!(height.raw_height().bits(), 2.875_f32.to_bits());
    assert_eq!(height.height().bits(), 12.578125_f32.to_bits());
    let selected = data.player_collision_height.as_ref().unwrap();
    assert_eq!(
        (selected.race(), selected.class(), selected.gender()),
        (1, 8, 0)
    );
    assert_eq!(height.source_binding(), selected.binding());
    assert_eq!(height.height(), selected.initial_height().height());
    assert_eq!(
        data.source_motion
            .as_ref()
            .unwrap()
            .player_collision_height
            .bits(),
        height.height().bits()
    );
    assert_ne!(
        first.request.owner_collision_height.bits(),
        height.height().bits()
    );
    assert_eq!(
        first.require_collision_width(&data).unwrap().width().bits(),
        1.71875_f32.to_bits()
    );
    for reply in replies {
        assert_eq!(
            require_target(&reply.request, &data, player_id).unwrap(),
            height
        );
        assert_eq!(reply.request.binding.profile, data.digest());
        assert_eq!(reply.request.binding.geometry, data.geometry_revision);
        if let Some(route) = route_query(&reply.request) {
            assert_eq!(
                route.target_player_height.as_ref().unwrap().target(),
                route.request.target.as_ref().unwrap()
            );
            let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &reply.facts else {
                unreachable!()
            };
            let motion::Fact::Route(motion::RouteOutcome::Complete(path)) = &spatial.fact else {
                unreachable!()
            };
            assert_eq!(path.source, motion::RouteSource::SyntheticFixture);
        }
    }
    let npc_guid = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
    assert_eq!(events.iter().filter(|e| matches!(e, SessionEvent::MonsterMove { guid, stop:false, .. } if *guid == npc_guid)).count(), 1);
    let mut replay = genesis.clone();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
    let mut all_records = records;
    let mut replanned = false;
    for _ in 0..12 {
        let x = c.world.state().position.x.as_f32() - 0.7;
        c.receive(movement(1, x)).unwrap();
        let pending = c.next_tick().unwrap();
        let mut expected = c.world.clone();
        expected
            .step_batch(
                expected.state().tick + 1,
                pending.inputs.clone(),
                &mut source::SyntheticMotionAuthority::default(),
            )
            .unwrap();
        all_records.push(expected.last_tick().unwrap().clone());
        c.pending = Some(pending);
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
        assert_eq!(out.ticks, 1);
        assert_eq!(
            expected.checkpoint().unwrap(),
            c.world.checkpoint().unwrap()
        );
        let requests = observed.lock().unwrap();
        if let Some(route) = routes(&requests).last() {
            if route.request.binding.now_ms != first.request.binding.now_ms {
                assert_eq!(
                    route
                        .require_target_collision_height(&data, [8; 16], player_id)
                        .unwrap(),
                    height
                );
                let target = route.request.target.as_ref().unwrap();
                assert_eq!(target.pose.position, c.world.state().position);
                assert_eq!(
                    target.pose.orientation.bits(),
                    c.world.state().orientation.bits()
                );
                assert_eq!(target.movement_flags.bits(), 1);
                assert_ne!(target.pose, first.request.target.unwrap().pose);
                assert_ne!(route.owner_position, first.owner_position);
                assert_ne!(route.request.goal, first.request.goal);
                replanned = true;
                break;
            }
        }
    }
    assert!(
        replanned,
        "moving target never reached a player-height-bound replan"
    );
    let mut replay = genesis;
    replay
        .replay(&n::decode_records(&n::encode_records(&all_records).unwrap()).unwrap())
        .unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
    assert_eq!(c.world.state().player_collision_height(), Some(&initial));
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(
        reopened.world.state().player_collision_height(),
        Some(&initial)
    );
}

#[test]
fn forged_target_source_pose_identity_or_width_echo_is_atomic_in_controller_and_replay() {
    for near in [true, false] {
        for field in 0..=if near { 22 } else { 24 } {
            let h = Harness::new();
            let data = profile();
            let observed = Arc::new(Mutex::new(vec![]));
            let mut c = create(
                &h,
                data.clone(),
                ObservedAuthority {
                    near,
                    observed: observed.clone(),
                    fault: Arc::new(AtomicU64::new(u64::from(field) + 2)),
                    ..Default::default()
                },
            );
            c.initial_events().unwrap();
            pump_to(&mut c, 1);
            let before = c.world.checkpoint().unwrap();
            let persisted = durable_files(&h);
            let generation = c.store.generation();
            let position = c.position_xyz();
            let facing = c.facing();
            c.receive(movement(1, -0.25)).unwrap();
            let out = c.pump(c.tick_duration());
            assert_eq!(out.ticks, 0, "near={near} field={field}");
            assert!(out.events.is_empty());
            assert!(matches!(out.pause.unwrap().kind, PauseKind::Simulation));
            assert_eq!(c.world.checkpoint().unwrap(), before);
            assert_eq!(c.store.generation(), generation);
            assert_eq!(durable_files(&h), persisted);
            assert_eq!(c.position_xyz(), position);
            assert_eq!(c.facing(), facing);
            let inputs = c.pending.as_ref().unwrap().inputs.clone();
            assert!(matches!(c.retry_tick(), Err(Error::Paused)));
            assert_eq!(c.pending.as_ref().unwrap().inputs, inputs);
            let mut good = c.world.clone();
            good.step_batch(2, inputs, &mut source::SyntheticMotionAuthority::default())
                .unwrap();
            let mut record = good.last_tick().unwrap().clone();
            let reply = record
                .replies
                .iter_mut()
                .find(|r| {
                    if near {
                        matches!(
                            r.request.query,
                            n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
                        )
                    } else {
                        route_query(&r.request).is_some()
                    }
                })
                .unwrap();
            forge_query(&mut reply.request.query, field);
            if field < 23 {
                assert!(require_target(&reply.request, &data, c.world.state().player).is_err());
            }
            let mut rejected = c.world.clone();
            assert!(
                rejected.replay(&[record]).is_err(),
                "near={near} field={field}"
            );
            assert_eq!(rejected.checkpoint().unwrap(), before);
            let digest = c.world.state_digest();
            c.shutdown().unwrap();
            drop(c);
            let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
            assert_eq!(reopened.world.state_digest(), digest);
            assert_eq!(reopened.world.state().next_sequence, 0);
        }
    }
}

#[test]
fn player_height_bound_route_retry_and_store_failure_publish_the_original_batch_once() {
    let h = Harness::new();
    let data = profile();
    let fault = Arc::new(AtomicU64::new(1));
    let observed = Arc::new(Mutex::new(vec![]));
    let mut c = create(
        &h,
        data.clone(),
        ObservedAuthority {
            fault: fault.clone(),
            observed: observed.clone(),
            ..Default::default()
        },
    );
    c.initial_events().unwrap();
    pump_to(&mut c, 1);
    let before = c.world.checkpoint().unwrap();
    let persisted = durable_files(&h);
    let generation = c.store.generation();
    let mut expected = c.world.clone();
    let mut replay = c.world.clone();
    c.receive(movement(1, -0.25)).unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 0);
    assert!(out.events.is_empty());
    assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
    assert_eq!(c.world.checkpoint().unwrap(), before);
    assert_eq!(c.store.generation(), generation);
    assert_eq!(durable_files(&h), persisted);
    let inputs = c.pending.as_ref().unwrap().inputs.clone();
    expected
        .step_batch(
            2,
            inputs.clone(),
            &mut source::SyntheticMotionAuthority::default(),
        )
        .unwrap();
    let attempted = (*routes(&observed.lock().unwrap())[0]).clone();
    assert_eq!(
        attempted
            .require_target_collision_height(&data, [8; 16], c.world.state().player)
            .unwrap(),
        c.world.state().player_collision_height().unwrap().witness()
    );
    let idle = c.pump(Duration::from_secs(60));
    assert!(idle.events.is_empty());
    assert_eq!(c.pending.as_ref().unwrap().inputs, inputs);
    assert_eq!(c.world.checkpoint().unwrap(), before);
    fault.store(0, Ordering::SeqCst);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored model-height store fault",
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
    assert_eq!(
        c.world.checkpoint().unwrap(),
        expected.checkpoint().unwrap()
    );
    assert_eq!(c.world.state().next_sequence, 1);
    assert_eq!(c.position_xyz(), [0.0; 3]);
    assert_eq!(c.store.generation(), generation);
    let requests = observed.lock().unwrap();
    let attempts = routes(&requests);
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0], &attempted);
    assert_eq!(attempts[1], &attempted);
    drop(requests);
    replay
        .replay(std::slice::from_ref(c.world.last_tick().unwrap()))
        .unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
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
                SessionEvent::MonsterMove { guid, stop: false, .. } if *guid == npc_guid
            ))
            .count(),
        1
    );
    assert_eq!(c.position_xyz(), [-0.25, 0.0, 0.0]);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert!(c.world.records().is_empty());
    c.shutdown().unwrap();
    drop(c);
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.world.state().next_sequence, 1);
}

#[test]
fn signed_zero_object_scale_getter_and_raw_height_fallback_survive_route_and_reopen() {
    for (display_scale, model_scale, raw_height, stored_scale, getter, height) in [
        (0.0, 2.5, 2.875, 0.0_f32, 1.0_f32, 2.875_f32),
        (-0.0, 2.5, 2.875, 0.0, 1.0, 2.875),
        (1.75, -0.0, 2.875, 0.0, 1.0, 2.875),
        (1.75, 2.5, 0.0, 4.375, 4.375, 2.03128),
        (1.75, 2.5, -0.0, 4.375, 4.375, 2.03128),
    ] {
        let h = Harness::new();
        let data = profile_with_fields(display_scale, model_scale, raw_height);
        let observed = Arc::new(Mutex::new(vec![]));
        let mut c = create(
            &h,
            data.clone(),
            ObservedAuthority {
                observed: observed.clone(),
                ..Default::default()
            },
        );
        c.initial_events().unwrap();
        let mut replay = c.world.clone();
        pump_to(&mut c, 2);
        let owned = c.world.state().player_collision_height().unwrap().clone();
        let witness = owned.witness();
        assert_eq!(witness.stored_object_scale().bits(), stored_scale.to_bits());
        assert_eq!(witness.effective_object_scale().bits(), getter.to_bits());
        assert_eq!(witness.raw_height().bits(), raw_height.to_bits());
        assert_eq!(witness.height().bits(), height.to_bits());
        let requests = observed.lock().unwrap();
        let route = *routes(&requests).last().unwrap();
        assert_eq!(
            route
                .require_target_collision_height(&data, [8; 16], c.world.state().player)
                .unwrap(),
            witness
        );
        assert_eq!(
            route.request.target.unwrap().collision_height.bits(),
            witness.height().bits()
        );
        // Player's scale getter and height fallback leave the independent NPC size alone.
        assert_eq!(
            route.require_collision_width(&data).unwrap().width().bits(),
            1.71875_f32.to_bits()
        );
        assert_eq!(
            route.request.owner_collision_height.bits(),
            3.59375_f32.to_bits()
        );
        drop(requests);
        replay.replay(c.world.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
        let digest = c.world.state_digest();
        c.shutdown().unwrap();
        drop(c);
        let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
        assert_eq!(reopened.world.state_digest(), digest);
        assert_eq!(
            reopened.world.state().player_collision_height(),
            Some(&owned)
        );
    }
}

#[test]
fn stale_player_sources_edited_scalar_and_reinitializers_cannot_rewrite_owned_profile_or_save() {
    let h = Harness::new();
    let data = profile();
    let mut c = create(&h, data.clone(), Default::default());
    c.initial_events().unwrap();
    pump_to(&mut c, 2);
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let persisted = durable_files(&h);
    let changed_height = profile_with_fields(1.75, 2.5, 3.0);
    // Same effective scale/height, different actual CDI/CMD operands and source binding.
    let same_height_other_source = profile_with_fields(2.5, 1.75, 2.875);
    assert_eq!(
        same_height_other_source
            .source_motion
            .as_ref()
            .unwrap()
            .player_collision_height,
        data.source_motion.as_ref().unwrap().player_collision_height
    );
    assert_ne!(
        same_height_other_source
            .player_collision_height
            .as_ref()
            .unwrap()
            .binding(),
        data.player_collision_height.as_ref().unwrap().binding()
    );
    let mut removed = data.clone();
    removed.player_collision_height = None;
    let mut scalar = data.clone();
    scalar
        .source_motion
        .as_mut()
        .unwrap()
        .player_collision_height = fixture::real(17.0);
    assert!(scalar.validate().is_err());
    let mut stale = data.clone();
    stale.player_collision_height = changed_height.player_collision_height.clone();
    assert!(stale.validate().is_err());
    let mut content = data.clone();
    content.content_digest[0] ^= 1;
    for changed in [
        removed,
        scalar,
        stale,
        changed_height,
        same_height_other_source,
        content,
    ] {
        assert_ne!(changed.digest(), data.digest());
        let observed = Arc::new(Mutex::new(vec![]));
        let result = NativeController::enter_component_test(
            &h.root,
            &h.profile,
            &h.character,
            Entry::Open { world: [8; 16] },
            changed.clone(),
            &ModelCatalog,
            ObservedAuthority {
                observed: observed.clone(),
                profile: Some(changed),
                player: Some(
                    Selection::from_profile(&h.profile, &h.character)
                        .unwrap()
                        .player,
                ),
                ..Default::default()
            },
        );
        assert!(result.is_err());
        assert!(observed.lock().unwrap().is_empty());
        assert_eq!(durable_files(&h), persisted);
    }
    for fields in [(1.75, 2.5, 3.0), (2.5, 1.75, 2.875)] {
        let mut changed = data.clone();
        let before = changed.clone();
        assert!(changed
            .initialize_player_collision_height(
                &player::catalog(),
                &models::catalog_with_height_fields(1.375, fields.2, fields.1, fields.0, false)
            )
            .is_err());
        assert_eq!(changed, before);
    }
    let mut scalar = data.clone();
    scalar
        .source_motion
        .as_mut()
        .unwrap()
        .player_collision_height = fixture::real(17.0);
    let before = scalar.clone();
    assert!(scalar
        .initialize_player_collision_height(&player::catalog(), &models::catalog())
        .is_err());
    assert_eq!(scalar, before);
    assert!(scalar
        .initialize_player(
            &fixture::initialized_starter_input(),
            fixture::float(100.0),
            1
        )
        .is_err());
    assert_eq!(scalar, before);
    let mut other_identity = fixture::initialized_starter_input();
    other_identity.gender = 1;
    let mut changed = data.clone();
    let before = changed.clone();
    assert!(changed
        .initialize_player(&other_identity, fixture::float(100.0), 1)
        .is_err());
    assert_eq!(changed, before);
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
    assert_eq!(reopened.world.state_digest(), digest);
}

// Mutate untrusted JSON, restore its typed field ordering, then recompute the
// public integrity hashes. This deliberately supplies no authentication proof.
fn rehashed_checkpoint(
    c: &NativeController,
    edit: impl FnOnce(&mut serde_json::Value),
) -> n::Checkpoint {
    use sha2::{Digest, Sha256};
    fn hash(domain: &[u8], body: &[u8]) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(domain);
        hash.update(body);
        hash.finalize().into()
    }
    let mut value = serde_json::to_value(c.world.checkpoint().unwrap()).unwrap();
    edit(&mut value["state"]);
    let typed: n::Checkpoint = serde_json::from_value(value.clone()).unwrap();
    let canonical = serde_json::to_string(&typed).unwrap();
    let start = canonical.find("\"state\":").unwrap() + "\"state\":".len();
    let end = canonical.rfind(",\"digest\":").unwrap();
    let state = &canonical[start..end];
    let digest = hash(b"benilla-native-state-v50\0", state.as_bytes());
    let tuple = format!(
        "[{state},{},{}]",
        serde_json::to_string(&digest).unwrap(),
        serde_json::to_string(&value["history_tip"]).unwrap()
    );
    value["digest"] = serde_json::to_value(digest).unwrap();
    value["checksum"] =
        serde_json::to_value(hash(b"benilla-native-checkpoint-v50\0", tuple.as_bytes())).unwrap();
    let typed: n::Checkpoint = serde_json::from_value(value).unwrap();
    // Decode passes integrity checks, so restore must reject the semantic forgery.
    n::Checkpoint::decode(&serde_json::to_vec(&typed).unwrap()).unwrap()
}

#[test]
fn rehashed_retained_spline_target_forgery_is_refused_without_rejecting_historical_pose() {
    let h = Harness::new();
    let data = profile();
    let mut c = create(&h, data.clone(), Default::default());
    c.initial_events().unwrap();
    pump_to(&mut c, 2);
    // A retained route may legitimately precede the player's present pose.
    c.receive(movement(1, -0.25)).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    let checkpoint = c.world.checkpoint().unwrap();
    let restored =
        n::NativeWorld::restore_synthetic_component_test(data.clone(), checkpoint.clone()).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
    let value = serde_json::to_value(&checkpoint).unwrap();
    let retained: motion::Target = serde_json::from_value(
        value["state"]["npcs"]["2"]["source_motion"]["motion"]["spline"]["route_request"]["target"]
            .clone(),
    )
    .unwrap();
    assert_ne!(retained.pose.position, c.world.state().position);
    for field in [
        "id",
        "collision_height",
        "bounding_radius",
        "combat_reach",
        "missing",
    ] {
        let forged = rehashed_checkpoint(&c, |state| {
            let target = &mut state["npcs"]["2"]["source_motion"]["motion"]["spline"]
                ["route_request"]["target"];
            assert!(target.is_object());
            if field == "missing" {
                *target = serde_json::Value::Null;
            } else {
                target[field] = if field == "id" {
                    999_u64.into()
                } else {
                    17.0_f32.to_bits().into()
                };
            }
        });
        assert!(
            n::NativeWorld::restore_synthetic_component_test(data.clone(), forged).is_err(),
            "{field}"
        );
    }
    for field in ["height", "source_binding", "player"] {
        let forged = rehashed_checkpoint(&c, |state| {
            let witness = &mut state["source_player_collision_height"]["witness"];
            match field {
                "source_binding" => {
                    witness[field][0] = (witness[field][0].as_u64().unwrap() ^ 1).into()
                }
                "player" => {
                    witness[field]["character"][0] =
                        (witness[field]["character"][0].as_u64().unwrap() ^ 1).into()
                }
                _ => witness[field] = 17.0_f32.to_bits().into(),
            }
        });
        assert!(
            n::NativeWorld::restore_synthetic_component_test(data.clone(), forged).is_err(),
            "{field}"
        );
    }
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
    assert_eq!(reopened.world.state_digest(), digest);
}
