// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-owned initial-model selection and authored MPQ-loaded DBC width through
//! the actual controller/store. Successful spatial answers remain synthetic.
use super::*;
use benilla_sim::{classic_spell as spell, creature_ai as ai, creature_motion as motion};
use n::{synthetic_creature_model_fixture as model, synthetic_motion_fixture as source};

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
    // 1 refuses Route; 2 changes its width echo; 3 removes its width echo.
    fault: Arc<AtomicU64>,
    observed: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    profile: Option<n::Profile>,
    inner: source::SyntheticMotionAuthority,
}
impl n::Authority for ObservedAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.observed.lock().unwrap().push(request.clone());
        let is_route = if let Some(route) = route_query(request) {
            route.require_collision_width(self.profile.as_ref().unwrap())?;
            true
        } else {
            false
        };
        let fault = self.fault.load(Ordering::SeqCst);
        if is_route && fault == 1 {
            return Err(n::NativeError::AuthorityUnavailable(
                "authored model-width route fault",
            ));
        }
        let mut reply = self.inner.resolve(request)?;
        if is_route && matches!(fault, 2 | 3) {
            let n::Query::SourceMotion(n::MotionQuery::Route(route)) = &mut reply.request.query
            else {
                unreachable!()
            };
            if fault == 2 {
                forge_width(route.collision_width.as_mut().unwrap(), 8);
            } else {
                route.collision_width = None;
            }
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
        n::Query::SourceMotion(n::MotionQuery::Route(route)) => Some(route),
        _ => None,
    }
}
fn routes(requests: &[n::AuthorityRequest]) -> Vec<&n::CreatureRouteRequest> {
    requests.iter().filter_map(route_query).collect()
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
fn forge_width(width: &mut n::CreatureCollisionWidthWitness, field: u8) {
    // Deserialization represents untrusted transcript input, never authority to
    // change the source-owned selection or live NPC state.
    let mut value = serde_json::to_value(&width).unwrap();
    match field {
        0 => value["npc"]["spawn"] = 3.into(),
        1 => value["npc"]["generation"] = 1.into(),
        2 | 3 => {
            let key = if field == 2 {
                "content"
            } else {
                "source_binding"
            };
            value[key][0] = (value[key][0].as_u64().unwrap() ^ 1).into();
        }
        4 => value["native_display_id"] = 60002.into(),
        5 => value["model_id"] = 99999.into(),
        6 => value["object_scale"] = 7.0_f32.to_bits().into(),
        7 => value["raw_width"] = 11.0_f32.to_bits().into(),
        _ => value["width"] = 13.0_f32.to_bits().into(),
    }
    *width = serde_json::from_value(value).unwrap();
}

#[test]
fn initial_model_width_binds_controller_chase_replan_replay_and_reopen() {
    let h = Harness::new();
    // The helper resolves Plan's real selection receipt and loads original
    // authored DBC bytes through MPQ -> Chain -> CreatureCatalog, including field14.
    let data = model::profile();
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
    let initial = c.world.state().npcs[&2]
        .creature_collision_width()
        .unwrap()
        .clone();
    let events = pump_to(&mut c, 2);
    let records = n::decode_records(&n::encode_records(c.world.records()).unwrap()).unwrap();
    assert_eq!(records, c.world.records());
    assert!(records.iter().all(|record| record.version == 25));
    let reply = records
        .iter()
        .flat_map(|record| &record.replies)
        .find(|reply| route_query(&reply.request).is_some())
        .unwrap();
    let first = route_query(&reply.request).unwrap();
    let width = first.require_collision_width(&data).unwrap();
    let selected = data
        .creature_collision_widths
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap();
    let expected = selected.initial_width();
    assert_eq!(width, initial.witness());
    assert_eq!(width.native_display_id(), 34);
    assert_eq!(width.model_id(), 9);
    assert_eq!(width.object_scale().bits(), 1.25_f32.to_bits());
    assert_eq!(width.raw_width().bits(), 1.375_f32.to_bits());
    assert_eq!(width.width().bits(), 1.71875_f32.to_bits());
    // Display/render scales 1.75 and 2.5 are retained source dependencies, not
    // multipliers in GetCollisionWidth's OBJECT_FIELD_SCALE_X * field14.
    assert_ne!(
        width.width().bits(),
        (1.25_f32 * 1.375 * 1.75 * 2.5).to_bits()
    );
    assert_eq!(width.npc(), c.world.state().npcs[&2].id);
    assert_eq!(width.npc().generation, 0);
    assert_eq!(width.native_display_id(), data.npcs[0].display_id);
    assert_eq!(width.native_display_id(), expected.native_display_id());
    assert_eq!(width.model_id(), expected.model_id());
    assert_eq!(width.object_scale(), expected.object_scale());
    assert_eq!(width.raw_width(), expected.raw_width());
    assert_eq!(width.width(), expected.width());
    assert_ne!(width.source_binding(), [0; 32]);
    assert_eq!(first.owner.npc(), width.npc());
    assert_eq!(first.owner_position, data.npcs[0].ai.spawn);
    assert_eq!(first.request.origin, first.owner_position);
    assert_eq!(first.request.binding.now_ms, 200);
    assert_eq!(reply.request.binding.world, [8; 16]);
    assert_eq!(reply.request.binding.tick, 2);
    assert_eq!(reply.request.binding.profile, data.digest());
    assert_eq!(
        first.request.binding.identity.content.0,
        data.content_digest
    );
    assert_eq!(reply.request.binding.geometry, data.geometry_revision);
    assert_eq!(reply.request.binding.npc, Some(width.npc()));
    assert!(observed.lock().unwrap().contains(&reply.request));
    let n::Facts::SourceMotion(n::MotionFacts::Spatial(spatial)) = &reply.facts else {
        panic!("expected the explicitly synthetic route receipt")
    };
    assert_eq!(spatial.request, motion::Query::Route(first.request.clone()));
    let motion::Fact::Route(motion::RouteOutcome::Complete(path)) = &spatial.fact else {
        panic!("expected a complete synthetic route")
    };
    assert_eq!(path.source, motion::RouteSource::SyntheticFixture);
    let npc_guid = c.projection.npc_guid(width.npc()).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event,
                SessionEvent::MonsterMove { guid, stop: false, .. } if *guid == npc_guid
            ))
            .count(),
        1
    );
    assert_eq!(c.world.state().npcs[&2].ai.phase(), ai::Phase::Combat);
    assert!(c.world.state().npcs[&2].ai.position().x.as_f32() < 16.0);
    let mut replay = genesis.clone();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());

    for field in 0..11 {
        let mut forged = records.clone();
        let reply = forged
            .iter_mut()
            .flat_map(|record| &mut record.replies)
            .find(|reply| route_query(&reply.request).is_some())
            .unwrap();
        let n::Query::SourceMotion(n::MotionQuery::Route(route)) = &mut reply.request.query else {
            unreachable!()
        };
        if field < 9 {
            forge_width(route.collision_width.as_mut().unwrap(), field);
            assert!(route.require_collision_width(&data).is_err());
        } else if field == 9 {
            route.collision_width = None;
            assert!(route.require_collision_width(&data).is_err());
        } else {
            reply.request.query = n::Query::SourceMotion(n::MotionQuery::Spatial(
                motion::Query::Route(route.request.clone()),
            ));
        }
        let mut rejected = genesis.clone();
        assert!(
            rejected.replay(&forged).is_err(),
            "forged width field {field}"
        );
        assert_eq!(
            rejected.checkpoint().unwrap(),
            genesis.checkpoint().unwrap()
        );
    }

    let mut all_records = records.clone();
    let mut replanned = false;
    for _ in 0..12 {
        let x = c.world.state().position.x.as_f32() - 0.7;
        c.receive(movement(1, x)).unwrap();
        // Movement ticks checkpoint and drain the live journal before returning.
        // Execute the exact staged batch in an independent fixture world, then
        // compare checkpoint/history-tip equality with the actual controller.
        let pending = c.next_tick().unwrap();
        let mut tick_witness = c.world.clone();
        tick_witness
            .step_batch(
                tick_witness.state().tick + 1,
                pending.inputs.clone(),
                &mut source::SyntheticMotionAuthority::default(),
            )
            .unwrap();
        all_records.push(tick_witness.last_tick().unwrap().clone());
        c.pending = Some(pending);
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
        assert_eq!(out.ticks, 1);
        assert_eq!(
            tick_witness.checkpoint().unwrap(),
            c.world.checkpoint().unwrap()
        );
        let observed = observed.lock().unwrap();
        if let Some(route) = routes(&observed).last() {
            if route.request.binding.now_ms != first.request.binding.now_ms {
                assert_eq!(route.require_collision_width(&data).unwrap(), width);
                assert_ne!(route.owner_position, first.owner_position);
                assert_ne!(route.request.goal, first.request.goal);
                assert_eq!(
                    c.world.state().npcs[&2].creature_collision_width(),
                    Some(&initial)
                );
                replanned = true;
                break;
            }
        }
    }
    assert!(
        replanned,
        "moving target did not reach a width-bound route replan"
    );
    let records = n::decode_records(&n::encode_records(&all_records).unwrap()).unwrap();
    let mut replay = genesis;
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(
        reopened.world.state().npcs[&2].creature_collision_width(),
        Some(&initial)
    );
}

#[test]
fn width_bound_route_retry_and_store_failure_publish_the_original_batch_once() {
    let h = Harness::new();
    let data = model::profile();
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
        attempted.require_collision_width(&data).unwrap(),
        c.world.state().npcs[&2]
            .creature_collision_width()
            .unwrap()
            .witness()
    );
    let idle = c.pump(Duration::from_secs(60));
    assert!(idle.events.is_empty());
    assert_eq!(c.pending.as_ref().unwrap().inputs, inputs);
    assert_eq!(c.world.checkpoint().unwrap(), before);
    fault.store(0, Ordering::SeqCst);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored model-width store fault",
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
fn forged_or_removed_width_reply_never_mutates_persists_or_publishes_the_tick() {
    for fault_kind in [2, 3] {
        let h = Harness::new();
        let data = model::profile();
        let mut c = create(
            &h,
            data.clone(),
            ObservedAuthority {
                fault: Arc::new(AtomicU64::new(fault_kind)),
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
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Simulation));
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(durable_files(&h), persisted);
        assert_eq!(c.position_xyz(), position);
        assert_eq!(c.facing(), facing);
        let inputs = c.pending.as_ref().unwrap().inputs.clone();
        assert_eq!(inputs.len(), 1);
        assert!(matches!(c.retry_tick(), Err(Error::Paused)));
        assert_eq!(c.pending.as_ref().unwrap().inputs, inputs);
        let digest = c.world.state_digest();
        c.shutdown().unwrap();
        drop(c);
        let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
        assert_eq!(reopened.world.state_digest(), digest);
        assert_eq!(reopened.world.state().next_sequence, 0);
    }
}

#[test]
fn absent_duplicate_or_changed_model_catalog_cannot_rewrite_a_saved_selection() {
    let data = model::profile();
    let selected = data
        .creature_collision_widths
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .clone();
    for rows in [
        n::CreatureCollisionWidthRows::Missing,
        n::CreatureCollisionWidthRows::Complete(vec![]),
        n::CreatureCollisionWidthRows::Complete(vec![selected.clone(), selected]),
    ] {
        let mut changed = data.clone();
        assert!(changed.initialize_creature_collision_widths(rows).is_err());
        assert_eq!(changed, data);
        assert_eq!(changed.digest(), data.digest());
    }
    let h = Harness::new();
    let mut c = create(&h, data.clone(), Default::default());
    c.initial_events().unwrap();
    pump_to(&mut c, 2);
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let persisted = durable_files(&h);
    let mut removed = data.clone();
    removed.creature_collision_widths = None;
    let mut changed_display = data.clone();
    changed_display.npcs[0].display_id += 1;
    let mut changed_content = data.clone();
    changed_content.content_digest[0] ^= 1;
    let mut changed_width = data.clone();
    changed_width
        .initialize_creature_collision_widths(n::CreatureCollisionWidthRows::Complete(vec![
            n::CreatureCollisionWidthSource::from_initial_model(
                2,
                model::selection(),
                &model::catalog_with_width(2.25),
            )
            .unwrap(),
        ]))
        .unwrap();
    for changed in [removed, changed_display, changed_content, changed_width] {
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
                ..Default::default()
            },
        );
        assert!(result.is_err());
        assert_eq!(durable_files(&h), persisted);
        assert!(observed.lock().unwrap().is_empty());
    }
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
    assert_eq!(reopened.world.state_digest(), digest);
}

#[test]
fn respawn_clears_initial_width_and_the_next_width_required_route_fails_closed() {
    let h = Harness::new();
    let mut data = model::profile();
    data.npcs[0].max_health = 1;
    data.npcs[0].ai.reaction = ai::Reaction::Defensive;
    data.npcs[0].ai.corpse_decay_ms = 500;
    let n::AbilityAdmission::Available(spec) = &mut data.abilities[0].admission else {
        unreachable!()
    };
    let spell::MageEffect::SchoolDamage(mut damage) = spec.effects[0] else {
        unreachable!()
    };
    damage.base_points = 99;
    damage.base_dice = 1;
    damage.die_sides = 1;
    spec.effects = [
        spell::MageEffect::SchoolDamage(damage),
        spell::MageEffect::Empty,
        spell::MageEffect::Empty,
    ];
    let observed = Arc::new(Mutex::new(vec![]));
    let mut c = create(
        &h,
        data.clone(),
        ObservedAuthority {
            observed: observed.clone(),
            inner: source::SyntheticMotionAuthority {
                critical_chance: Some(0),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    c.initial_events().unwrap();
    pump_to(&mut c, 2);
    let initial_id = c.world.state().npcs[&2].id;
    let initial = c.world.state().npcs[&2]
        .creature_collision_width()
        .unwrap()
        .clone();
    c.receive(cast(&c)).unwrap();
    while c.world.state().npcs[&2].health != 0 {
        assert!(
            c.tick() < 100,
            "source spell never killed the authored target"
        );
        let next = c.tick() + 1;
        pump_to(&mut c, next);
    }
    assert_eq!(c.world.state().npcs[&2].ai.phase(), ai::Phase::Corpse);
    assert_eq!(
        c.world.state().npcs[&2].creature_collision_width(),
        Some(&initial)
    );
    assert!(routes(&observed.lock().unwrap())
        .iter()
        .any(|route| route.collision_width.as_ref() == Some(initial.witness())));
    let corpse_digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let mut c = enter(
        &h,
        Entry::Open { world: [8; 16] },
        data.clone(),
        Default::default(),
    );
    assert_eq!(c.world.state_digest(), corpse_digest);
    assert_eq!(
        c.world.state().npcs[&2].creature_collision_width(),
        Some(&initial)
    );
    c.initial_events().unwrap();
    let mut saw_despawn = false;
    while c.world.state().npcs[&2].id == initial_id {
        assert!(c.tick() < 150, "authored source respawn did not complete");
        let next = c.tick() + 1;
        pump_to(&mut c, next);
        saw_despawn |= c.world.state().npcs[&2].ai.phase() == ai::Phase::Despawned;
    }
    assert!(saw_despawn);
    let current_id = c.world.state().npcs[&2].id;
    assert_eq!(current_id.spawn, initial_id.spawn);
    assert_eq!(current_id.generation, initial_id.generation + 1);
    assert!(c.world.state().npcs[&2]
        .creature_collision_width()
        .is_none());
    assert!(initial.witness().validate_for(&data, current_id).is_err());
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let observed = Arc::new(Mutex::new(vec![]));
    let mut c = enter(
        &h,
        Entry::Open { world: [8; 16] },
        data.clone(),
        ObservedAuthority {
            observed: observed.clone(),
            ..Default::default()
        },
    );
    assert_eq!(c.world.state_digest(), digest);
    assert!(c.world.state().npcs[&2]
        .creature_collision_width()
        .is_none());
    c.initial_events().unwrap();
    c.receive(cast(&c)).unwrap();
    loop {
        assert!(
            c.tick() < 200,
            "respawned target never requested a width-dependent route"
        );
        let before = c.world.checkpoint().unwrap();
        let persisted = durable_files(&h);
        let generation = c.store.generation();
        let out = c.pump(c.tick_duration());
        if let Some(pause) = out.pause {
            assert_eq!(out.ticks, 0);
            assert!(out.events.is_empty());
            assert!(matches!(pause.kind, PauseKind::Authority));
            assert!(pause
                .message
                .contains("current creature model collision width"));
            assert_eq!(c.world.checkpoint().unwrap(), before);
            assert_eq!(c.store.generation(), generation);
            assert_eq!(durable_files(&h), persisted);
            let requests = observed.lock().unwrap();
            let route = *routes(&requests).last().unwrap();
            assert_eq!(route.owner.npc(), current_id);
            assert!(route.collision_width.is_none());
            assert!(matches!(
                route.require_collision_width(&data),
                Err(n::NativeError::AuthorityUnavailable(
                    "current creature model collision width"
                ))
            ));
            break;
        }
    }
}
