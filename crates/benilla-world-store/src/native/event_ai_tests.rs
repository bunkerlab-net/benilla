// SPDX-License-Identifier: GPL-3.0-or-later
//! Durable authored EventAI receipts with a separately staged presentation queue.
//! These tests do not assert crash-proof exactly-once delivery to an external UI.
use super::*;
use benilla_protocol::{messages as p, SessionEvent};
use benilla_world_client::{native::NativeProjection, ClientCatalog, QuestMetadata};
use benilla_world_runtime::native::{self as n, synthetic_event_ai_fixture as fixture};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SPEAKING_SEED: u64 = 1234;
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "benilla-event-ai-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("world")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Catalog;
impl ClientCatalog for Catalog {
    fn build(&self) -> u32 {
        5875
    }
    fn manifest_digest(&self) -> Digest {
        [9; 32]
    }
    fn contains_map(&self, id: u32) -> bool {
        id == 7
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        matches!(id, 60001 | 96002)
    }
    fn contains_item_display(&self, _: u32) -> bool {
        false
    }
    fn contains_spell(&self, _: u32) -> bool {
        true
    }
    fn player_power_type(&self, _: PlayerId) -> Option<u8> {
        Some(0)
    }
    fn item_metadata(&self, _: u32) -> Option<p::ItemInfo> {
        None
    }
    fn quest_metadata(&self, _: u32) -> Option<QuestMetadata> {
        None
    }
    fn quest_greeting(&self, _: u32) -> Option<String> {
        None
    }
}
fn chat_count(events: &[SessionEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, SessionEvent::Chat(_)))
        .count()
}
fn speech_tick(world: &NativeWorld) -> bool {
    world.last_tick().is_some_and(|record| {
        record
            .events
            .iter()
            .any(|event| matches!(event, n::Event::EventAiSpeech(_)))
    })
}
fn open(path: &Path, identity: Identity) -> Result<(NativeStore, NativeWorld), Error> {
    NativeStore::open(path, identity, |checkpoint| {
        NativeWorld::restore_synthetic_component_test(fixture::profile(), checkpoint)
    })
}

#[test]
fn event_ai_prepublication_failure_withholds_staged_chat_retry_publishes_once_and_reopen_is_silent()
{
    let temp = Temp::new();
    let mut world = fixture::world(SPEAKING_SEED);
    let identity = Identity::of(&world.checkpoint().unwrap());
    let (mut store, receipt) = NativeStore::create(temp.path(), &world).unwrap();
    receipt.acknowledge(&mut world).unwrap();
    let mut projection = NativeProjection::new(&world, world.state().player, &Catalog).unwrap();
    let mut authority = fixture::Authority::default();
    let mut queue = Vec::new();
    let before = loop {
        assert!(world.state().tick < 10, "authored seed must produce speech");
        let before = world.clone();
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
        if speech_tick(&world) {
            break before;
        }
        queue.extend(projection.advance(&world).unwrap());
    };
    assert_eq!(chat_count(&queue), 0);
    let expected = world.checkpoint().unwrap();
    let journal = world.records().to_vec();
    let generation = store.generation();
    // A disposable presentation transaction can be prepared before publication.
    // Its packets and cursor are not installed until the durable boundary succeeds.
    let mut staged = NativeProjection::new(&before, before.state().player, &Catalog).unwrap();
    let pending = staged.advance(&world).unwrap();
    assert_eq!(chat_count(&pending), 1);
    let prepared = store.prepare(&world).unwrap();
    assert_eq!(prepared.final_world.checkpoint().unwrap(), expected);
    store
        .storage
        .inject_fault(super::super::storage::SaveFault::BeforeRename);
    assert!(store.commit(prepared).is_err());
    assert!(!store.requires_reopen());
    assert_eq!(store.generation(), generation);
    assert_eq!(world.checkpoint().unwrap(), expected);
    assert_eq!(world.records(), journal);
    assert_eq!(chat_count(&queue), 0);
    drop(pending);
    drop(staged);
    store.storage.clear_fault();
    let receipt = store.commit(store.prepare(&world).unwrap()).unwrap();
    // The original cursor still admits exactly this frame after the failed save.
    queue.extend(projection.advance(&world).unwrap());
    assert_eq!(chat_count(&queue), 1);
    assert!(projection.advance(&world).is_err());
    receipt.acknowledge(&mut world).unwrap();
    assert!(world.records().is_empty());
    drop(store);
    let (_store, mut restored) = open(&temp.path(), identity).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), expected);
    assert!(restored.records().is_empty());
    let mut projection =
        NativeProjection::new(&restored, restored.state().player, &Catalog).unwrap();
    assert_eq!(
        chat_count(&projection.initial_events(&restored).unwrap()),
        0
    );
    assert!(projection.advance(&restored).is_err());
    restored
        .step(
            restored.state().tick + 1,
            None,
            &mut fixture::Authority::default(),
        )
        .unwrap();
    assert_eq!(chat_count(&projection.advance(&restored).unwrap()), 0);
}

#[test]
fn event_ai_resigned_envelope_receipt_tampering_fails_semantic_replay() {
    let temp = Temp::new();
    let mut world = fixture::world(SPEAKING_SEED);
    let identity = Identity::of(&world.checkpoint().unwrap());
    let (mut store, _) = NativeStore::create(temp.path(), &world).unwrap();
    let mut authority = fixture::Authority::default();
    while !speech_tick(&world) {
        assert!(world.state().tick < 10, "authored seed must produce speech");
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
    }
    store.checkpoint(&mut world).unwrap();
    drop(store);
    let path = temp.path().join("world.checkpoint");
    let original = fs::read(&path).unwrap();
    for case in 0..9 {
        let mut envelope = Envelope::decode(&original).unwrap();
        let speech = envelope
            .records
            .iter_mut()
            .flat_map(|record| &mut record.events)
            .find_map(|event| match event {
                n::Event::EventAiSpeech(speech) => Some(speech),
                _ => None,
            })
            .unwrap();
        match case {
            0 => speech.callback.catalog[0] ^= 1,
            1 => speech.callback.npc.generation += 1,
            2 => speech.callback.tick += 1,
            3 => speech.callback.invoker.owner[0] ^= 1,
            4 => speech.broadcast_id += 1,
            5 => speech.text.push('!'),
            6 => speech.speaker_name.push('!'),
            7 => speech.language = 33,
            8 => speech.occurrence += 1,
            _ => unreachable!(),
        }
        // Envelope and journal JSON are re-encoded with valid outer checksums;
        // regenerated callback events must still disagree during exact replay.
        fs::write(&path, envelope.encode().unwrap()).unwrap();
        assert!(matches!(
            open(&temp.path(), identity.clone()),
            Err(Error::Runtime(_))
        ));
    }
    fs::write(&path, original).unwrap();
    assert!(open(&temp.path(), identity).is_ok());
}

#[test]
fn event_ai_uncertain_publication_reopens_without_replaying_historical_chat() {
    let temp = Temp::new();
    let mut world = fixture::world(SPEAKING_SEED);
    let identity = Identity::of(&world.checkpoint().unwrap());
    let (mut store, _) = NativeStore::create(temp.path(), &world).unwrap();
    let mut authority = fixture::Authority::default();
    while !speech_tick(&world) {
        assert!(world.state().tick < 10, "authored seed must produce speech");
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
    }
    let expected = world.checkpoint().unwrap();
    store
        .storage
        .inject_fault(super::super::storage::SaveFault::AfterRename);
    assert!(store.checkpoint(&mut world).is_err());
    assert!(store.requires_reopen());
    assert!(speech_tick(&world));
    drop(store);
    let (_, restored) = open(&temp.path(), identity).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), expected);
    let projection = NativeProjection::new(&restored, restored.state().player, &Catalog).unwrap();
    assert_eq!(
        chat_count(&projection.initial_events(&restored).unwrap()),
        0
    );
}

#[test]
fn event_ai_reopen_in_combat_emits_only_the_new_respawn_generations_speech() {
    let temp = Temp::new();
    let mut profile = fixture::profile();
    profile.npcs[0].max_health = 1;
    profile.npcs[0].ai.corpse_decay_ms = 0;
    let damage = profile.npcs[0].combat.as_mut().unwrap();
    damage.minimum_damage = n::synthetic_fixture::float(0.0);
    damage.maximum_damage = n::synthetic_fixture::float(0.0);
    let mut world = NativeWorld::synthetic_component_test(
        profile.clone(),
        [8; 16],
        n::synthetic_fixture::player(),
        SPEAKING_SEED,
    )
    .unwrap();
    let identity = Identity::of(&world.checkpoint().unwrap());
    let (mut store, _) = NativeStore::create(temp.path(), &world).unwrap();
    let mut authority = fixture::Authority::default();
    while !speech_tick(&world) {
        assert!(world.state().tick < 10);
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
    }
    let old = world.state().npcs[&2].id;
    store.checkpoint(&mut world).unwrap();
    drop(store);
    let (_, mut restored) = NativeStore::open(temp.path(), identity, |checkpoint| {
        NativeWorld::restore_synthetic_component_test(profile.clone(), checkpoint)
    })
    .unwrap();
    let mut projection =
        NativeProjection::new(&restored, restored.state().player, &Catalog).unwrap();
    let old_guid = projection.npc_guid(old).unwrap();
    let mut delivered = projection.initial_events(&restored).unwrap();
    assert_eq!(chat_count(&delivered), 0);
    authority.base.base.base.critical_chance = Some(0);
    authority.base.base.base.resist_chance = Some(0);
    let mut cast = Some(n::synthetic_fixture::cast(&restored, 60001));
    loop {
        assert!(
            restored.state().tick < 100,
            "respawn must reach a new combat entry"
        );
        restored
            .step(restored.state().tick + 1, cast.take(), &mut authority)
            .unwrap();
        let packets = projection.advance(&restored).unwrap();
        if let Some(chat) = packets.iter().find_map(|event| match event {
            SessionEvent::Chat(chat) => Some(chat),
            _ => None,
        }) {
            assert_ne!(chat.sender_guid, old_guid);
            assert!(restored.state().npcs[&2].id.generation > old.generation);
            delivered.extend(packets);
            break;
        }
        delivered.extend(packets);
    }
    assert_eq!(chat_count(&delivered), 1);
}
