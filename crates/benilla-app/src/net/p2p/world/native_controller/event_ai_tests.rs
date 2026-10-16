// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact app controller/client/store publication with authored EventAI sources.
//! No production capability, graphical assets, or crash-proof UI delivery claim.
use super::*;
use benilla_protocol::messages::ChatMessage;
use n::synthetic_event_ai_fixture as event_ai;

struct EventAiCatalog;
impl ClientCatalog for EventAiCatalog {
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

struct CountingAuthority {
    inner: event_ai::Authority,
    audience_calls: Arc<AtomicU64>,
}
impl n::Authority for CountingAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        if matches!(request.query, n::Query::EventAiAudience(_)) {
            self.audience_calls.fetch_add(1, Ordering::Relaxed);
        }
        self.inner.resolve(request)
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.inner.observe_derived(reply)
    }
}

fn enter(harness: &Harness, entry: Entry, calls: Arc<AtomicU64>) -> NativeController {
    NativeController::enter_component_test(
        &harness.root,
        &harness.profile,
        &harness.character,
        entry,
        event_ai::profile(),
        &EventAiCatalog,
        CountingAuthority {
            inner: event_ai::Authority::default(),
            audience_calls: calls,
        },
    )
    .unwrap()
}
fn chats(events: &[SessionEvent]) -> Vec<ChatMessage> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::Chat(chat) => Some(chat.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn event_ai_controller_checkpoint_failure_withholds_speech_and_retry_publishes_once() {
    let harness = Harness::new();
    let calls = Arc::new(AtomicU64::new(0));
    let mut controller = enter(
        &harness,
        Entry::Create {
            world: [8; 16],
            seed: 1234,
        },
        calls.clone(),
    );
    assert!(chats(&controller.initial_events().unwrap()).is_empty());
    let first = controller.pump(controller.tick_duration());
    assert_eq!(first.ticks, 1);
    assert!(first.pause.is_none(), "{:?}", first.pause);
    assert!(chats(&first.events).is_empty());
    assert_eq!(first.saved.unwrap().tick, 1);
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    let saved_tick = controller.saved_tick;
    let generation = controller.store.generation();
    let disk = fs::read(harness.path().join("world.checkpoint")).unwrap();
    let published_position = controller.position_xyz();
    let published_facing = controller.facing();
    let fault = harness.path().join(".world-checkpoint.pending");
    fs::write(&fault, b"authored prepublication EventAI fault").unwrap();

    // The fixture's 200ms source notification reaches EnterCombat on tick two.
    // Exercise the actual controller outbox and checkpoint branch, without an
    // alternate projection transaction or hand-built delivery queue.
    let blocked = controller.pump(controller.tick_duration());
    assert_eq!(blocked.ticks, 1);
    assert!(blocked.events.is_empty());
    assert!(blocked.saved.is_none());
    assert!(matches!(
        blocked.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert_eq!(controller.tick(), 2);
    assert_eq!(controller.saved_tick, saved_tick);
    assert_eq!(controller.store.generation(), generation);
    assert_eq!(
        fs::read(harness.path().join("world.checkpoint")).unwrap(),
        disk
    );
    assert_eq!(controller.position_xyz(), published_position);
    assert_eq!(controller.facing(), published_facing);
    assert!(controller.pending.is_none());
    let pending = controller.outbox.as_ref().unwrap();
    assert_eq!(pending.tick, 2);
    assert!(pending.saved.is_none());
    let expected_chat = chats(&pending.events);
    assert_eq!(expected_chat.len(), 1);
    assert_eq!(expected_chat[0].chat_type, 0x0b);
    assert_eq!(expected_chat[0].target_guid, controller.player_guid());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let checkpoint = controller.world.checkpoint().unwrap();
    let records = controller.world.records().to_vec();
    let rng = controller.world.state().rng_counter;
    let sequence = controller.world.state().next_sequence;
    assert_eq!(
        controller
            .world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .filter(|event| matches!(event, n::Event::EventAiSpeech(_)))
            .count(),
        1
    );

    // Paused time cannot rerun the callback, change the committed simulation,
    // publish the pending line, or accumulate debt for a later burst.
    let debt = controller.debt;
    let paused = controller.pump(Duration::from_secs(30));
    assert_eq!(paused.ticks, 0);
    assert!(paused.events.is_empty());
    assert_eq!(controller.debt, debt);
    assert_eq!(controller.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(controller.world.records(), records);
    assert_eq!(calls.load(Ordering::Relaxed), 1);

    fs::remove_file(fault).unwrap();
    let saved = controller.retry_checkpoint().unwrap();
    assert_eq!(saved.tick, 2);
    assert_eq!(saved.generation, generation + 1);
    assert_eq!(controller.world.checkpoint().unwrap(), checkpoint);
    assert!(controller.world.records().is_empty());
    assert!(controller.outbox.as_ref().unwrap().saved.is_some());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let delivered = controller.pump(Duration::ZERO);
    assert_eq!(delivered.ticks, 0);
    assert!(delivered.pause.is_none());
    assert_eq!(chats(&delivered.events), expected_chat);
    assert!(controller.outbox.is_none());
    assert_eq!(controller.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(controller.world.state().rng_counter, rng);
    assert_eq!(controller.world.state().next_sequence, sequence);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(controller.pump(Duration::ZERO).events.is_empty());
    assert!(controller.retry_checkpoint().is_err());

    let (_, cancelled, shutdown) = controller.shutdown().unwrap();
    assert_eq!(cancelled, 0);
    assert!(chats(&shutdown).is_empty());
    drop(controller);
    let mut reopened = enter(&harness, Entry::Open { world: [8; 16] }, calls.clone());
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert!(chats(&reopened.initial_events().unwrap()).is_empty());
    assert!(reopened.pump(Duration::ZERO).events.is_empty());
    let next = reopened.pump(reopened.tick_duration());
    assert!(next.pause.is_none(), "{:?}", next.pause);
    assert!(chats(&next.events).is_empty());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}
