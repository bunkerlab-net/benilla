// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual controller/client/store with expressly authored two-owner visitors.
//! This is neither loaded map traversal nor a graphical/production admission test.
use super::*;
use benilla_sim::creature_ai as ai;
use n::synthetic_two_owner_fixture as two;

struct TwoOwnerCatalog;
impl ClientCatalog for TwoOwnerCatalog {
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
        matches!(id, 60001 | 96002)
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
fn enter(
    h: &Harness,
    entry: Entry,
    profile: n::Profile,
    authority: impl n::Authority + Send + 'static,
) -> NativeController {
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        entry,
        profile,
        &TwoOwnerCatalog,
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
fn created_guids(events: &[SessionEvent]) -> Vec<u64> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::ObjectCreate {
                guid,
                kind: benilla_protocol::EntityKind::Unit,
                ..
            } => Some(*guid),
            _ => None,
        })
        .collect()
}
fn owner_queues(world: &n::NativeWorld) -> Vec<(runtime::NpcId, n::NpcEvents)> {
    world
        .state()
        .npcs
        .values()
        .map(|npc| (npc.id, npc.events.clone()))
        .collect()
}

#[test]
fn two_owner_identity_motion_death_respawn_and_reopen_leave_neutral_owner_independent() {
    for order in [[two::HOSTILE, two::NEUTRAL], [two::NEUTRAL, two::HOSTILE]] {
        let h = Harness::new();
        let mut data = two::profile_with_order(order);
        data.npcs[0].max_health = 1;
        data.npcs[0].ai.corpse_decay_ms = 100;
        data.npcs[0].ai.respawn_delay_seconds = 1;
        let mut c = create(&h, data.clone(), two::CompleteAuthority::default());
        let original_a = c.world.state().npcs[&two::HOSTILE].id;
        let original_b = c.world.state().npcs[&two::NEUTRAL].id;
        let guid_a = c.projection.npc_guid(original_a).unwrap();
        let guid_b = c.projection.npc_guid(original_b).unwrap();
        assert_ne!(guid_a, guid_b);
        let handle_a = c.projection.capture_npc(guid_a).unwrap();
        let handle_b = c.projection.capture_npc(guid_b).unwrap();
        let mut initial_guids = created_guids(&c.initial_events().unwrap());
        initial_guids.sort_unstable();
        let mut expected_guids = vec![guid_a, guid_b];
        expected_guids.sort_unstable();
        assert_eq!(initial_guids, expected_guids);
        let initial_a_event = c.world.state().npcs[&two::HOSTILE]
            .events
            .notify_handle()
            .unwrap();
        let initial_b_event = c.world.state().npcs[&two::NEUTRAL]
            .events
            .notify_handle()
            .unwrap();
        assert_eq!(initial_a_event.event_id, initial_b_event.event_id);
        assert_ne!(initial_a_event.npc, initial_b_event.npc);
        let genesis = c.world.clone();
        let mut moved = false;
        let mut died = false;
        let mut removed = false;
        let mut respawned = false;
        while !respawned {
            assert!(
                c.tick() < 100,
                "authored NPC must respawn within the source timer window"
            );
            if c.tick() == 2 {
                c.receive(cast(&c)).unwrap();
            }
            let out = c.pump(c.tick_duration());
            assert!(out.pause.is_none(), "{:?}", out.pause);
            assert_eq!(out.ticks, 1);
            moved |= out.events.iter().any(|event| matches!(event, SessionEvent::MonsterMove { guid, stop: false, .. } if *guid == guid_a));
            died |= c.world.last_tick().unwrap().events.iter().any(|event| matches!(event, n::Event::Npc { id, effect: ai::Effect::Died } if *id == original_a));
            removed |= out.events.iter().any(
                |event| matches!(event, SessionEvent::ObjectDestroyed(guid) if *guid == guid_a),
            );
            assert!(!out.events.iter().any(|event| matches!(event,
                SessionEvent::ObjectDestroyed(guid) | SessionEvent::MonsterMove { guid, .. } if *guid == guid_b)));
            assert!(!created_guids(&out.events).contains(&guid_b));
            let b = &c.world.state().npcs[&two::NEUTRAL];
            assert_eq!(b.id, original_b);
            assert_eq!(b.ai.phase(), ai::Phase::Idle);
            assert_eq!(b.ai.next_sequence(), Some(c.tick() + 1));
            assert_eq!(b.events.clock_ms(), c.tick() * u64::from(data.tick_ms));
            assert_eq!(c.projection.npc_guid(original_b), Some(guid_b));
            let current_a = c.world.state().npcs[&two::HOSTILE].id;
            if current_a != original_a {
                assert_eq!(current_a.spawn, original_a.spawn);
                assert_eq!(current_a.generation, original_a.generation + 1);
                let next_guid = c.projection.npc_guid(current_a).unwrap();
                assert_ne!(next_guid, guid_a);
                assert!(created_guids(&out.events).contains(&next_guid));
                respawned = true;
            }
        }
        assert!(moved && died && removed && respawned);
        assert_eq!(c.projection.capture_npc(guid_b).unwrap(), handle_b);
        assert_eq!(handle_a.id, original_a);
        assert!(matches!(
            c.projection.capture_npc(guid_a),
            Err(benilla_world_client::Error::UnknownTarget)
        ));
        let sequence = c.world.state().next_sequence;
        assert!(matches!(
            c.receive(ClientCommand::CastSpell {
                spell_id: 60001,
                target: Some(guid_a)
            }),
            Err(Error::Client(benilla_world_client::Error::UnknownTarget))
        ));
        assert_eq!(c.world.state().next_sequence, sequence);
        assert!(c.actions.is_empty());

        let digest = c.world.state_digest();
        let queues = owner_queues(&c.world);
        let mut replay = genesis;
        replay.replay(c.world.records()).unwrap();
        assert_eq!(replay.state_digest(), digest);
        assert_eq!(owner_queues(&replay), queues);
        let next_a_guid = c
            .projection
            .npc_guid(c.world.state().npcs[&two::HOSTILE].id)
            .unwrap();
        c.shutdown().unwrap();
        drop(c);
        let mut reopened = enter(
            &h,
            Entry::Open { world: [8; 16] },
            data,
            two::CompleteAuthority::default(),
        );
        assert_eq!(reopened.world.state_digest(), digest);
        assert_eq!(owner_queues(&reopened.world), queues);
        assert_eq!(reopened.projection.npc_guid(original_b), Some(guid_b));
        let mut reopened_guids = created_guids(&reopened.initial_events().unwrap());
        reopened_guids.sort_unstable();
        let mut expected = vec![next_a_guid, guid_b];
        expected.sort_unstable();
        assert_eq!(reopened_guids, expected);
        let next = reopened.tick() + 1;
        pump_to(&mut reopened, next);
        replay
            .step(next, None, &mut two::CompleteAuthority::default())
            .unwrap();
        assert_eq!(reopened.world.state_digest(), replay.state_digest());
        reopened.shutdown().unwrap();
    }
}

struct FaultableComplete {
    fail: Arc<AtomicBool>,
    calls: Arc<AtomicU64>,
    derived: Arc<Mutex<Vec<n::AuthorityReply>>>,
    inner: two::CompleteAuthority,
}
impl n::Authority for FaultableComplete {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.resolve(request)
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.derived.lock().unwrap().push(reply.clone());
        self.inner.fail_derived_owner = self.fail.load(Ordering::SeqCst).then_some(two::NEUTRAL);
        self.inner.observe_derived(reply)
    }
}

#[test]
fn late_second_owner_failure_then_save_fault_retains_both_owners_and_publishes_once() {
    let h = Harness::new();
    let data = two::profile();
    let fail = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicU64::new(0));
    let derived = Arc::new(Mutex::new(vec![]));
    let mut c = create(
        &h,
        data.clone(),
        FaultableComplete {
            fail: fail.clone(),
            calls: calls.clone(),
            derived: derived.clone(),
            inner: Default::default(),
        },
    );
    c.initial_events().unwrap();
    let genesis = c.world.clone();
    pump_to(&mut c, 1);
    c.receive(move_to([0.0, 0.0, 0.0])).unwrap();
    let before = c.world.checkpoint().unwrap();
    let queues_before = owner_queues(&c.world);
    let mut expected = c.world.clone();
    let saved_generation = c.store.generation();
    fail.store(true, Ordering::SeqCst);
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 0);
    assert!(out.events.is_empty());
    assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
    assert_eq!(c.world.checkpoint().unwrap(), before);
    assert_eq!(owner_queues(&c.world), queues_before);
    assert_eq!(c.store.generation(), saved_generation);
    assert!(derived.lock().unwrap().iter().any(|reply| matches!(&reply.request.query,
        n::Query::SourceMotion(n::MotionQuery::VisitorSight(sight)) if matches!(sight.notification_owner, n::VisitorParticipant::Creature(id) if id.spawn == two::NEUTRAL))));
    let pending = c.pending.as_ref().unwrap().inputs.clone();
    expected
        .step_batch(2, pending, &mut two::CompleteAuthority::default())
        .unwrap();
    assert_ne!(
        expected.state().npcs[&two::HOSTILE].source_motion,
        c.world.state().npcs[&two::HOSTILE].source_motion
    );
    let mut replay = genesis;
    replay.replay(expected.records()).unwrap();
    assert_eq!(replay.state_digest(), expected.state_digest());

    fail.store(false, Ordering::SeqCst);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored two-owner save fault",
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
    assert_eq!(c.world.state_digest(), expected.state_digest());
    assert_eq!(owner_queues(&c.world), owner_queues(&expected));
    assert_eq!(c.world.state().next_sequence, 1);
    let committed = c.world.state_digest();
    let calls_at_commit = calls.load(Ordering::SeqCst);
    let queues_at_commit = owner_queues(&c.world);
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert!(!out.events.is_empty());
    assert!(out
        .events
        .iter()
        .any(|event| matches!(event, SessionEvent::MonsterMove { .. })));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        calls_at_commit,
        "publication retry must not redispatch either visitor"
    );
    assert_eq!(c.world.state_digest(), committed);
    assert_eq!(owner_queues(&c.world), queues_at_commit);
    c.shutdown().unwrap();
    drop(c);
    let mut reopened = enter(
        &h,
        Entry::Open { world: [8; 16] },
        data,
        two::CompleteAuthority::default(),
    );
    assert_eq!(reopened.world.state_digest(), committed);
    assert_eq!(owner_queues(&reopened.world), queues_at_commit);
    assert_eq!(created_guids(&reopened.initial_events().unwrap()).len(), 2);
    pump_to(&mut reopened, 3);
    expected
        .step(3, None, &mut two::CompleteAuthority::default())
        .unwrap();
    assert_eq!(reopened.world.state_digest(), expected.state_digest());
    reopened.shutdown().unwrap();
}
