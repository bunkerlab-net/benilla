// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored placeholders only. These tests compile the exact app controller and command enum;
//! they neither mint a production capability nor claim game-asset/collision coverage.
use super::*;
use crate::net::MoveKind;
use benilla_world_runtime::native::{
    synthetic_fixture as fixture, synthetic_quest_fixture as quests,
};
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

struct Catalog;
impl ClientCatalog for Catalog {
    fn build(&self) -> u32 {
        5875
    }
    fn manifest_digest(&self) -> runtime::Digest {
        [9; 32]
    }
    fn contains_map(&self, id: u32) -> bool {
        id == 7
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        id == 60001
    }
    fn contains_item_display(&self, _: u32) -> bool {
        false
    }
    fn contains_spell(&self, id: u32) -> bool {
        matches!(
            id,
            81 | 133 | 3050 | 60001 | 60002 | 60004 | 60005 | 61004 | 61005
        )
    }
    fn player_power_type(&self, _: PlayerId) -> Option<u8> {
        Some(0)
    }
    fn player_bytes_2(&self, _: PlayerId) -> Option<u32> {
        Some(0x00abcdef)
    }
    fn item_metadata(&self, _: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        None
    }
    fn quest_metadata(&self, id: u32) -> Option<benilla_world_client::QuestMetadata> {
        if id != 70001 {
            return None;
        }
        use benilla_protocol::messages::{QuestObjective, QuestTemplate};
        Some(benilla_world_client::QuestMetadata {
            template: QuestTemplate {
                quest_id: id,
                method: 2,
                level: 1,
                zone_or_sort: 0,
                quest_type: 0,
                rep_objective_faction: 0,
                rep_objective_value: 0,
                next_quest_in_chain: 0,
                money: 7,
                money_max_level: 60,
                reward_spell: 0,
                src_item_id: 0,
                flags: 0,
                rewards: [(0, 0); 4],
                choices: [(0, 0); 6],
                point_map_id: 7,
                point_x: 0.0,
                point_y: 0.0,
                point_opt: 0,
                title: "Authored quest".into(),
                objectives_text: "Authored objective".into(),
                details: "Authored details".into(),
                end_text: "Authored end".into(),
                objectives: std::array::from_fn(|i| QuestObjective {
                    creature_or_go: if i == 0 { 60001 } else { 0 },
                    required_count: u32::from(i == 0),
                    item_id: 0,
                    item_count: 0,
                    text: String::new(),
                }),
            },
            request_text: "Authored request".into(),
            offer_text: "Authored offer".into(),
        })
    }
    fn quest_greeting(&self, entry: u32) -> Option<String> {
        (entry == 60003).then(|| "Authored greeting".into())
    }
    fn native_quest_greeting(
        &self,
        entry: u32,
    ) -> Option<benilla_world_client::NativeQuestGreeting> {
        (entry == 60003).then(|| benilla_world_client::NativeQuestGreeting {
            has_greeting_row: true,
            text: "Authored greeting".into(),
            emote_delay: 0,
            emote: 0,
        })
    }
    fn native_quest_presentation(
        &self,
        quest: u32,
    ) -> Option<benilla_world_client::NativeQuestPresentation> {
        (quest == 70001).then_some(benilla_world_client::NativeQuestPresentation {
            details_emotes: [(0, 0); 4],
            offer_emotes: [(0, 0); 4],
            complete_emote: 0,
            complete_emote_delay: 0,
            incomplete_emote: 0,
            incomplete_emote_delay: 0,
        })
    }
}
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Harness {
    root: PathBuf,
    profile: ProfileStore,
    character: Character,
}
impl Harness {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "benilla-native-controller-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let mut profile = ProfileStore::create(root.join("p2p-profile")).unwrap();
        let id = profile
            .create_character(benilla_profile::NewCharacter {
                name: "Alice".into(),
                race: 1,
                class: 8,
                gender: 0,
                appearance: Default::default(),
            })
            .unwrap();
        let character = profile.character(id).unwrap().clone();
        Self {
            root,
            profile,
            character,
        }
    }
    fn path(&self) -> PathBuf {
        world_path(&self.root, [8; 16]).unwrap()
    }
    fn enter(
        &self,
        entry: Entry,
        data: n::Profile,
        authority: impl n::Authority + Send + 'static,
    ) -> Result<NativeController, Error> {
        NativeController::enter_component_test(
            &self.root,
            &self.profile,
            &self.character,
            entry,
            data,
            &Catalog,
            authority,
        )
    }
    fn start(&self, data: n::Profile) -> NativeController {
        self.enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            data,
            fixture::KernelAuthority::default(),
        )
        .unwrap()
    }
    fn reopen(&self, data: n::Profile) -> NativeController {
        self.enter(
            Entry::Open { world: [8; 16] },
            data,
            fixture::KernelAuthority::default(),
        )
        .unwrap()
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn stationary() -> n::Profile {
    let mut data = fixture::profile();
    data.npcs.clear();
    data
}
fn move_to(pos: [f32; 3]) -> ClientCommand {
    ClientCommand::Move {
        kind: MoveKind::Heartbeat,
        flags: 0,
        pos,
        orientation: 0.75,
        pitch: 0.0,
        fall_time: 0,
        jump: None,
        transport: None,
    }
}
fn cast(c: &NativeController) -> ClientCommand {
    ClientCommand::CastSpell {
        spell_id: 60001,
        target: Some(c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap()),
    }
}
fn pump_to(c: &mut NativeController, tick: u64) -> Vec<SessionEvent> {
    let mut events = vec![];
    while c.tick() < tick {
        let output = c.pump(c.tick_duration());
        assert!(output.pause.is_none(), "{:?}", output.pause);
        events.extend(output.events);
    }
    events
}

#[test]
fn native51_controller_create_and_reopen_retain_genesis_without_initial_cast_packets() {
    let h = Harness::new();
    let profile = n::synthetic_initial_cast_fixture::profile();
    let mut controller = h.start(profile.clone());
    assert_eq!(controller.tick(), 0);
    assert_eq!(controller.world.state().next_sequence, 0);
    assert_eq!(controller.world.state().next_cast_id, 1);
    assert!(controller.world.state().rng_counter > 0);
    let genesis = controller.world.state().initial_cast_genesis.clone();
    assert_eq!(controller.world.state().rng_counter, genesis.end_counter());
    let initial = controller.initial_events().unwrap();
    assert!(!initial.iter().any(|event| matches!(
        event,
        SessionEvent::SpellStart { .. } | SessionEvent::SpellGo { .. }
    )));
    pump_to(&mut controller, 1);
    let expected = controller.world.checkpoint().unwrap();
    controller.shutdown().unwrap();
    drop(controller);
    let mut restored = h.reopen(profile);
    assert_eq!(restored.world.checkpoint().unwrap(), expected);
    assert_eq!(restored.world.state().initial_cast_genesis, genesis);
    assert_eq!(restored.world.state().next_sequence, 0);
    assert_eq!(restored.world.state().next_cast_id, 1);
    pump_to(&mut restored, 2);
    assert_eq!(restored.world.state().initial_cast_genesis, genesis);
}

#[test]
fn previous_native_checkpoints_inside_fresh_store_checksum_cannot_reopen_or_replace_controller_world(
) {
    for (version, policy) in [
        (36, "benilla-native-source-combat-timers-v36"),
        (37, "benilla-native-initial-cast-genesis-v37"),
        (38, "benilla-native-creature-generation-v38"),
        (39, "benilla-native-creature-equipment-v39"),
        (40, "benilla-native-reference-loot-v40"),
        (41, "benilla-native-creature-addons-v41"),
        (42, "benilla-native-addon-capabilities-v42"),
        (43, "benilla-native-dry-habitat-v43"),
        (44, "benilla-native-combat-assistance-v44"),
        (45, "benilla-native-assistance-neighbors-v45"),
        (46, "benilla-native-peer-help-v46"),
        (47, "benilla-native-positive-peer-help-v47"),
        (48, "benilla-native-wander-leg-v48"),
        (52, "benilla-native-player-mainhand-v52"),
        (53, "benilla-native-player-mainhand-v53"),
        (54, "benilla-native-backpack-split-v54"),
        (55, "benilla-native-backpack-destroy-v55"),
        (56, "benilla-native-quest-abandon-v56"),
        (57, "benilla-native-reputation-controls-v57"),
        (58, "benilla-native-watched-faction-v58"),
    ] {
        use sha2::{Digest as _, Sha256};
        let h = Harness::new();
        let mut controller = h.start(stationary());
        controller.shutdown().unwrap();
        drop(controller);
        let path = h.path().join("world.checkpoint");
        let original = fs::read(&path).unwrap();
        let length = u32::from_le_bytes(original[52..56].try_into().unwrap()) as usize;
        let mut checkpoint: serde_json::Value =
            serde_json::from_slice(&original[60..60 + length]).unwrap();
        checkpoint["version"] = version.into();
        checkpoint["state"]["version"] = version.into();
        checkpoint["state"]["policy"] = policy.into();
        let typed: n::Checkpoint = serde_json::from_value(checkpoint.clone()).unwrap();
        let encoded = String::from_utf8(typed.encode().unwrap()).unwrap();
        let start = encoded.find("\"state\":").unwrap() + "\"state\":".len();
        let end = encoded.rfind(",\"digest\":").unwrap();
        let state = &encoded[start..end];
        let hash = |domain: &[u8], bytes: &[u8]| -> runtime::Digest {
            let mut hasher = Sha256::new();
            hasher.update(domain);
            hasher.update(bytes);
            hasher.finalize().into()
        };
        checkpoint["digest"] =
            serde_json::to_value(hash(b"benilla-native-state-v59\0", state.as_bytes())).unwrap();
        let body = format!(
            "[{state},{},{}]",
            checkpoint["digest"], checkpoint["history_tip"]
        );
        checkpoint["checksum"] =
            serde_json::to_value(hash(b"benilla-native-checkpoint-v59\0", body.as_bytes()))
                .unwrap();
        let typed: n::Checkpoint = serde_json::from_value(checkpoint).unwrap();
        let snapshot = typed.encode().unwrap();
        let mut forged = original[..60].to_vec();
        forged[52..56].copy_from_slice(&(snapshot.len() as u32).to_le_bytes());
        forged.extend(snapshot);
        forged.extend(&original[60 + length..original.len() - 32]);
        forged.extend(hash(b"benilla-native-store-v1\0", &forged));
        fs::write(&path, &forged).unwrap();
        assert!(h
            .enter(
                Entry::Open { world: [8; 16] },
                stationary(),
                fixture::KernelAuthority::default()
            )
            .is_err());
        assert!(h
            .enter(
                Entry::Create {
                    world: [8; 16],
                    seed: 7
                },
                stationary(),
                fixture::KernelAuthority::default()
            )
            .is_err());
        assert_eq!(fs::read(path).unwrap(), forged);
    }
}

#[test]
fn selected_identity_native_clock_and_saves_never_touch_profile() {
    let h = Harness::new();
    let before = fs::read(h.root.join("p2p-profile/profile.json")).unwrap();
    let mut c = h.start(stationary());
    let driven: &dyn Driven = &c;
    assert_eq!(driven.map_id(), 7);
    assert_eq!(driven.facing(), stationary().player.orientation.as_f32());
    assert_eq!(
        driven.initial_player_fields().unwrap().unit_health(),
        Some(90)
    );
    assert_eq!(
        c.selected_player(),
        Selection::from_profile(&h.profile, &h.character)
            .unwrap()
            .player
    );
    assert_eq!(c.pump(Duration::from_millis(99)).ticks, 0);
    for _ in 0..1000 {
        c.receive(ClientCommand::SetSelection { guid: 0 }).unwrap();
    }
    assert_eq!(c.tick(), 0);
    let output = c.pump(Duration::from_secs(3));
    assert_eq!(output.ticks, 30);
    let output = c.pump(Duration::from_secs(100));
    assert_eq!(output.ticks, MAX_TICKS_PER_PUMP);
    assert!(output.pending_ticks > 900);
    while c.debt >= c.tick_duration() {
        assert!(c.pump(Duration::ZERO).pause.is_none());
    }
    assert!(c.store.generation() > 1);
    let digest = c.world.state_digest();
    let tick = c.tick();
    c.shutdown().unwrap();
    drop(c);
    let c = h.reopen(stationary());
    assert_eq!(c.tick(), tick);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(
        fs::read(h.root.join("p2p-profile/profile.json")).unwrap(),
        before
    );
}
#[test]
fn journal_warning_saves_before_hard_limit_with_one_ms_ticks() {
    let h = Harness::new();
    let mut data = stationary();
    data.tick_ms = 1;
    let mut c = h.start(data);
    for _ in 0..(n::MAX_RECORDS * 3 / MAX_TICKS_PER_PUMP) {
        let out = c.pump(Duration::from_millis(MAX_TICKS_PER_PUMP as u64));
        assert!(out.pause.is_none());
        assert!(c.world.records().len() < n::MAX_RECORDS);
    }
    assert!(c.tick() > n::MAX_RECORDS as u64 * 2);
    assert!(c.store.generation() > 3);
}
#[test]
fn movement_preserves_binary32_and_native_cast_interruption() {
    let h = Harness::new();
    let mut c = h.start(fixture::profile());
    c.receive(cast(&c)).unwrap();
    c.pump(c.tick_duration());
    let x = f32::from_bits(0x3dcccccd);
    c.receive(move_to([x, 0.0, 0.0])).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert_eq!(c.position_xyz()[0].to_bits(), x.to_bits());
    assert!(out.events.iter().any(|e| matches!(e, SessionEvent::UnitMove { guid, position, .. } if *guid == c.player_guid() && position[0].to_bits() == x.to_bits())));
    assert!(out.events.iter().any(|e| matches!(
        e,
        SessionEvent::SpellFailedOther {
            spell_id: 60001,
            ..
        }
    )));
    assert!(c.world.state().casts.is_empty());
    assert!(out.saved.is_some());
}
struct ToggleAuthority {
    unavailable: Arc<AtomicBool>,
    observed: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    inner: fixture::KernelAuthority,
}
impl n::Authority for ToggleAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.observed.lock().unwrap().push(request.clone());
        if self.unavailable.load(Ordering::SeqCst) {
            Err(n::NativeError::AuthorityUnavailable(
                "authored unavailable geometry",
            ))
        } else {
            self.inner.resolve(request)
        }
    }
}
#[test]
fn unavailable_authority_retains_exact_input_tick_and_explicit_retry() {
    let h = Harness::new();
    let unavailable = Arc::new(AtomicBool::new(true));
    let observed = Arc::new(Mutex::new(vec![]));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            stationary(),
            ToggleAuthority {
                unavailable: unavailable.clone(),
                observed: observed.clone(),
                inner: Default::default(),
            },
        )
        .unwrap();
    c.receive(move_to([0.25, 0.0, 0.0])).unwrap();
    let digest = c.world.state_digest();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 0);
    assert_eq!(c.world.state_digest(), digest);
    assert!(matches!(
        c.pause().map(|p| &p.kind),
        Some(PauseKind::Authority)
    ));
    let inputs = c.pending.as_ref().unwrap().inputs.clone();
    let mut expected = c.world.clone();
    expected
        .step_batch(1, inputs, &mut fixture::KernelAuthority::default())
        .unwrap();
    c.pump(Duration::from_secs(60));
    assert_eq!(c.debt, c.tick_duration());
    assert!(matches!(
        c.receive(move_to([1.0, 0.0, 0.0])),
        Err(Error::Paused)
    ));
    assert!(matches!(c.retry_checkpoint(), Err(Error::Paused)));
    unavailable.store(false, Ordering::SeqCst);
    c.retry_tick().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 1);
    assert!(out.pause.is_none());
    assert_eq!(
        c.world.checkpoint().unwrap(),
        expected.checkpoint().unwrap()
    );
    assert!(out.saved.is_some());
    assert!(c.world.records().is_empty());
    let observed = observed.lock().unwrap();
    assert_eq!(observed[0], observed[1]);
    assert!(c.pending.is_none());
    assert_eq!(c.world.state().next_sequence, 1);
}
#[test]
fn checkpoint_failure_pauses_and_retry_does_not_reapply_input() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    pump_to(&mut c, 99);
    c.receive(move_to([0.125, 0.0, 0.0])).unwrap();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored fault",
    )
    .unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 1);
    assert!(c.pending.is_none());
    let digest = c.world.state_digest();
    assert!(!c.world.records().is_empty());
    assert!(matches!(
        c.pause().map(|p| &p.kind),
        Some(PauseKind::Checkpoint {
            requires_reopen: false
        })
    ));
    c.pump(Duration::from_secs(500));
    assert_eq!(c.tick(), 100);
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    assert_eq!(c.world.state_digest(), digest);
    assert!(c.world.records().is_empty());
    assert_eq!(c.pump(Duration::ZERO).ticks, 0);
    assert_eq!(c.world.state().next_sequence, 1);
}
#[test]
fn projection_failure_retains_committed_world_without_replaying_action() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    c.receive(move_to([0.125, 0.0, 0.0])).unwrap();
    // Advancing independently is intentionally malformed controller state: its exact
    // projection must refuse this gap, never pretend the committed world rolled back.
    c.world
        .step(1, None, &mut fixture::KernelAuthority::default())
        .unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 1);
    assert_eq!(c.tick(), 2);
    assert!(matches!(
        c.pause().map(|p| &p.kind),
        Some(PauseKind::Projection)
    ));
    assert!(c.pending.is_none());
    assert_eq!(c.world.state().next_sequence, 1);
    assert!(matches!(c.retry_tick(), Err(Error::Paused)));
    c.shutdown().unwrap();
    drop(c);
    let c = h.reopen(stationary());
    assert_eq!(c.tick(), 2);
    assert_eq!(c.world.state().next_sequence, 1);
}
fn death_profile() -> n::Profile {
    let mut p = fixture::initialized_profile();
    let c = p.npcs[0].combat.as_mut().unwrap();
    c.minimum_damage = fixture::float(10000.0);
    c.maximum_damage = fixture::float(10000.0);
    p.configure_player_death(
        benilla_sim::player_death::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        benilla_sim::player_death::DeathSupport::checked(
            &benilla_sim::player_death::REQUIRED_ABSENCES.map(|d| (d, Some(false))),
        )
        .unwrap(),
    )
    .unwrap();
    p
}
fn root_ack(c: &NativeController, counter: u32, flags: u32) -> ClientCommand {
    ClientCommand::MoveModeAck {
        guid: c.player_guid(),
        counter,
        mode: MoveMode::Root,
        apply: true,
        flags,
        pos: [13.0, 0.0, 0.0],
        orientation: 1.0,
    }
}
#[test]
fn death_root_uses_session_counter_real_ack_and_restart_emits_fresh_root() {
    let h = Harness::new();
    let data = death_profile();
    let mut c = h.start(data.clone());
    // Same real namespace may have prior forced orders; never derive the root from tick.
    c.orders.counter = NativeMovementOrders::from_session_next_counter(41);
    c.receive(cast(&c)).unwrap();
    let mut events = vec![];
    while c.orders.pending.is_none() && c.tick() < 200 {
        events.extend(c.pump(c.tick_duration()).events);
        assert!(c.pause.is_none());
    }
    let root = c.orders.pending.unwrap();
    assert_eq!(root.counter, 41);
    assert_ne!(c.tick(), 41);
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::MoveMode {
            counter: 41,
            mode: MoveMode::Root,
            apply: true,
            ..
        }
    )));
    let before = c.world.state_digest();
    assert!(matches!(
        c.receive(root_ack(&c, 40, move_flags::ROOT)),
        Err(Error::InvalidMovementAck)
    ));
    assert!(matches!(
        c.receive(root_ack(&c, 41, move_flags::ROOT | move_flags::FORWARD)),
        Err(Error::InvalidMovementAck)
    ));
    c.receive(root_ack(
        &c,
        41,
        move_flags::ROOT | move_flags::TURN_LEFT | move_flags::WALK_MODE,
    ))
    .unwrap();
    c.receive(root_ack(&c, 41, move_flags::ROOT)).unwrap();
    assert!(c.orders.pending.is_none());
    assert_eq!(c.world.state_digest(), before);
    let received = c.receive(move_to([20.0, 0.0, 0.0])).unwrap();
    assert!(c.actions.is_empty());
    assert!(received.events.iter().any(
        |e| matches!(e, SessionEvent::UnitMove { flags, .. } if *flags & move_flags::ROOT != 0)
    ));
    c.shutdown().unwrap();
    drop(c);
    let mut c = h.reopen(data);
    assert_eq!(c.orders.pending.unwrap().counter, 0);
    assert!(matches!(
        c.receive(root_ack(&c, 41, move_flags::ROOT)),
        Err(Error::InvalidMovementAck)
    ));
    let initial = c.initial_events().unwrap();
    assert!(initial.iter().any(|e| matches!(
        e,
        SessionEvent::MoveMode {
            counter: 0,
            mode: MoveMode::Root,
            apply: true,
            ..
        }
    )));
    assert!(matches!(c.initial_events(), Err(Error::Closed)));
}
#[test]
fn owner_catalog_and_open_failures_do_not_create_or_replace_worlds() {
    let h = Harness::new();
    let foreign = Harness::new();
    assert!(matches!(
        NativeController::enter_component_test(
            &h.root,
            &h.profile,
            &foreign.character,
            Entry::Create {
                world: [8; 16],
                seed: 7
            },
            stationary(),
            &Catalog,
            fixture::KernelAuthority::default()
        ),
        Err(Error::WrongOwner)
    ));
    assert!(!h.root.join("p2p-worlds").exists());
    assert!(h
        .enter(
            Entry::Open { world: [8; 16] },
            stationary(),
            fixture::KernelAuthority::default()
        )
        .is_err());
    assert!(!h.root.join("p2p-worlds").exists());
    let mut bad = stationary();
    bad.map_id = 9;
    assert!(h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7
            },
            bad,
            fixture::KernelAuthority::default()
        )
        .is_err());
    assert!(!h.root.join("p2p-worlds").exists());
    let mut c = h.start(stationary());
    c.shutdown().unwrap();
    drop(c);
    let checkpoint = fs::read(h.path().join("world.checkpoint")).unwrap();
    assert!(h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7
            },
            stationary(),
            fixture::KernelAuthority::default()
        )
        .is_err());
    assert_eq!(
        fs::read(h.path().join("world.checkpoint")).unwrap(),
        checkpoint
    );
}
#[test]
fn actual_shared_driver_emits_initial_native_state_and_saves_logout() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    let (et, er) = crossbeam_channel::bounded(1000);
    let (ct, cr) = crossbeam_channel::bounded(10);
    let (_rt, rr) = crossbeam_channel::bounded(1);
    ct.send(move_to([0.1, 0.0, 0.0])).unwrap();
    ct.send(ClientCommand::Logout).unwrap();
    let exit = run(&mut c, &et, &cr, &rr, &AtomicBool::new(false)).unwrap();
    assert_eq!(exit.saved.tick, 0);
    assert_eq!(exit.cancelled_inputs, 1);
    assert!(c.closed());
    assert!(er
        .try_iter()
        .any(|e| matches!(e, SessionEvent::SpellBook { .. })));
}

#[test]
fn native_logout_failure_retains_lock_until_explicit_checkpoint_retry() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    pump_to(&mut c, 3);
    let digest = c.world.state_digest();
    fs::write(h.path().join(".world-checkpoint.pending"), b"repair needed").unwrap();
    let (et, er) = crossbeam_channel::bounded(1000);
    let (ct, cr) = crossbeam_channel::bounded(10);
    let (rt, rr) = crossbeam_channel::bounded(1);
    ct.send(ClientCommand::Logout).unwrap();
    let worker = std::thread::spawn(move || run(&mut c, &et, &cr, &rr, &AtomicBool::new(false)));
    loop {
        if matches!(er.recv_timeout(Duration::from_secs(5)).unwrap(), SessionEvent::Notification { text } if text.contains("Logout could not save"))
        {
            break;
        }
    }
    assert!(!worker.is_finished());
    assert!(matches!(
        h.enter(
            Entry::Open { world: [8; 16] },
            stationary(),
            fixture::KernelAuthority::default()
        ),
        Err(Error::NativeStore(
            benilla_world_store::native::Error::Store(benilla_world_store::Error::Locked)
        ))
    ));
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    rt.send(Control::RetryCheckpoint).unwrap();
    let exit = worker.join().unwrap().unwrap();
    assert_eq!(exit.saved.tick, 3);
    assert_eq!(h.reopen(stationary()).world.state_digest(), digest);
}
#[test]
fn native_input_capacity_unsupported_forms_and_exact_owner_are_bounded() {
    let h = Harness::new();
    let mut c = h.start(fixture::profile());
    for _ in 0..MAX_ACTIONS {
        c.receive(cast(&c)).unwrap();
    }
    assert!(matches!(c.receive(cast(&c)), Err(Error::Backpressure)));
    assert_eq!(c.tick(), 0);
    let mut movement = move_to([0.1, 0.0, 0.0]);
    if let ClientCommand::Move { flags, .. } = &mut movement {
        *flags = move_flags::ON_TRANSPORT;
    }
    assert!(matches!(c.receive(movement), Err(Error::InvalidMovement)));
    assert!(matches!(
        c.receive(ClientCommand::CastSpell {
            spell_id: 60001,
            target: Some(c.player_guid())
        }),
        Err(Error::Client(_))
    ));
    assert!(matches!(
        c.receive(ClientCommand::AttackSwing {
            guid: c.player_guid()
        }),
        Err(Error::UnsupportedCommand)
    ));
    assert!(matches!(
        c.receive(ClientCommand::AttackStop),
        Err(Error::UnsupportedCommand)
    ));
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    let input = &c.world.last_tick().unwrap().inputs[0];
    assert_eq!(input.player, c.selected.player);
    assert_eq!(input.sequence, 0);
    assert_eq!(input.tick, 1);
    let (saved, cancelled, _) = c.shutdown().unwrap();
    assert_eq!(saved.tick, 1);
    assert_eq!(cancelled, 0);
    assert_eq!(c.world.state().next_sequence, MAX_ACTIONS as u64);
}

#[test]
fn cancel_cast_checks_requested_spell_and_captured_preparing_instance() {
    let h = Harness::new();
    let mut c = h.start(fixture::profile());
    c.receive(cast(&c)).unwrap();
    c.pump(c.tick_duration());
    c.receive(ClientCommand::CancelCast { spell_id: 60002 })
        .unwrap();
    assert!(c.actions.is_empty());
    c.receive(ClientCommand::CancelCast { spell_id: 60001 })
        .unwrap();
    let old = c.actions.pop_front().unwrap();
    c.pump(c.tick_duration());
    c.pump(c.tick_duration());
    assert!(c.world.state().casts.values().all(|cast| !matches!(
        cast.execution.state(),
        benilla_sim::classic_spell::CastState::Casting { .. }
    )));
    c.receive(cast(&c)).unwrap();
    c.pump(c.tick_duration());
    assert!(c.world.state().casts.values().any(|cast| matches!(
        cast.execution.state(),
        benilla_sim::classic_spell::CastState::Casting { .. }
    )));
    c.actions.push_front(old);
    let sequence = c.world.state().next_sequence;
    c.pump(c.tick_duration());
    assert_eq!(c.world.state().next_sequence, sequence + 1);
    assert!(c.world.state().casts.values().any(|cast| matches!(
        cast.execution.state(),
        benilla_sim::classic_spell::CastState::Casting { .. }
    )));
    c.receive(ClientCommand::CancelCast { spell_id: 0 })
        .unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert!(c
        .world
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|event| matches!(
            event,
            n::Event::Spell {
                event: benilla_sim::classic_spell::SpellEvent::Failed(
                    benilla_sim::classic_spell::CastFailure::Interrupted
                ),
                ..
            }
        )));
}

#[test]
fn actual_native_driver_explicit_tick_retry_uses_retained_input() {
    let h = Harness::new();
    let unavailable = Arc::new(AtomicBool::new(true));
    let observed = Arc::new(Mutex::new(vec![]));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            stationary(),
            ToggleAuthority {
                unavailable: unavailable.clone(),
                observed: observed.clone(),
                inner: Default::default(),
            },
        )
        .unwrap();
    c.receive(move_to([0.25, 0.0, 0.0])).unwrap();
    assert!(c.pump(c.tick_duration()).pause.is_some());
    let (et, er) = crossbeam_channel::bounded(1000);
    let (ct, cr) = crossbeam_channel::bounded(10);
    let (rt, rr) = crossbeam_channel::bounded(1);
    let worker = std::thread::spawn(move || run(&mut c, &et, &cr, &rr, &AtomicBool::new(false)));
    loop {
        if matches!(er.recv_timeout(Duration::from_secs(5)).unwrap(), SessionEvent::Notification { text } if text.contains("World paused"))
        {
            break;
        }
    }
    unavailable.store(false, Ordering::SeqCst);
    rt.send(Control::RetryTick).unwrap();
    loop {
        if matches!(
            er.recv_timeout(Duration::from_secs(5)).unwrap(),
            SessionEvent::UnitMove {
                position: [0.25, 0.0, 0.0],
                ..
            }
        ) {
            break;
        }
    }
    ct.send(ClientCommand::Logout).unwrap();
    let exit = worker.join().unwrap().unwrap();
    assert!(exit.saved.tick >= 1);
    let c = h.reopen(stationary());
    assert_eq!(c.world.state().next_sequence, 1);
    assert_eq!(c.position_xyz(), [0.25, 0.0, 0.0]);
    let observed = observed.lock().unwrap();
    assert_eq!(observed[0], observed[1]);
}

#[test]
fn cancel_before_first_pump_preserves_cast_then_cancels_its_instance() {
    let h = Harness::new();
    let mut c = h.start(fixture::profile());
    c.receive(cast(&c)).unwrap();
    c.receive(ClientCommand::CancelCast { spell_id: 60001 })
        .unwrap();
    let first = c.pump(c.tick_duration());
    assert!(first.pause.is_none());
    assert!(first.events.iter().any(|e| matches!(
        e,
        SessionEvent::SpellStart {
            spell_id: 60001,
            ..
        }
    )));
    assert!(
        c.world.last_tick().unwrap().events.iter().any(|e| matches!(
            e,
            n::Event::Spell {
                event: benilla_sim::classic_spell::SpellEvent::Failed(
                    benilla_sim::classic_spell::CastFailure::Interrupted
                ),
                ..
            }
        )),
        "the client's pre-SpellStart cancellation was dropped"
    );
    assert!(c.world.state().casts.is_empty());
    assert_eq!(c.world.state().resources.caster.global_ready_at(9), None);
    assert_eq!(c.world.state().resources.caster.mana, 90);
    assert_eq!(c.world.state().next_sequence, 2);
}

#[test]
fn short_cast_cancelled_before_first_pump_must_not_launch() {
    let h = Harness::new();
    let mut data = fixture::profile();
    let n::AbilityAdmission::Available(spec) = &mut data.abilities[0].admission else {
        unreachable!()
    };
    spec.cast_ms = 50;
    let mut c = h.start(data);
    c.receive(cast(&c)).unwrap();
    c.receive(ClientCommand::CancelCast { spell_id: 60001 })
        .unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert!(
        !out.events.iter().any(|e| matches!(
            e,
            SessionEvent::SpellGo {
                spell_id: 60001,
                ..
            }
        )),
        "cast and cancel were both received before the update, but the short cast launched"
    );
    assert!(c.world.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        n::Event::Spell {
            event: benilla_sim::classic_spell::SpellEvent::Failed(
                benilla_sim::classic_spell::CastFailure::Interrupted
            ),
            ..
        }
    )));
}

#[test]
fn batched_cast_results_and_cancellations_project_in_input_order() {
    let h = Harness::new();
    let mut c = h.start(fixture::profile());
    c.receive(cast(&c)).unwrap();
    c.receive(cast(&c)).unwrap(); // Refused by the still-active first cast.
    c.receive(ClientCommand::CancelCast { spell_id: 60001 })
        .unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    let results = out
        .events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::CastResult { success, .. } => Some(*success),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(results, vec![true, false]);
    assert!(matches!(
        c.world.state().casts[&1].execution.state(),
        benilla_sim::classic_spell::CastState::Casting { .. }
    ));
    assert_eq!(c.world.state().next_sequence, 3);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellFailedOther { .. })));
    // A separate cancellation now captures the actual current instance, then recast/cancel
    // in this same batch must refer to the new request rather than the old instance.
    c.receive(ClientCommand::CancelCast { spell_id: 0 })
        .unwrap();
    c.receive(cast(&c)).unwrap();
    c.receive(ClientCommand::CancelCast { spell_id: 60001 })
        .unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    let order = out
        .events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::SpellFailedOther { .. } => Some("cancel"),
            SessionEvent::CastResult { success: true, .. } => Some("accepted"),
            SessionEvent::SpellStart { .. } => Some("start"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(order, vec!["cancel", "accepted", "start", "cancel"]);
    assert!(c.world.state().casts.is_empty());
}

#[test]
fn full_batch_checkpoint_retry_and_reopen_never_repeat_any_input() {
    let h = Harness::new();
    let mut c = h.start(stationary());
    pump_to(&mut c, 99);
    c.receive(move_to([0.125, 0.0, 0.0])).unwrap();
    c.receive(move_to([0.25, 0.0, 0.0])).unwrap();
    fs::write(h.path().join(".world-checkpoint.pending"), b"fault").unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 1);
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(c.pending.is_none());
    assert_eq!(c.world.state().next_sequence, 2);
    assert_eq!(c.world.last_tick().unwrap().inputs.len(), 2);
    assert!(out.events.is_empty());
    assert_eq!(c.position_xyz(), [0.0; 3]);
    assert_eq!(c.facing(), 0.0);
    let state = c.world.state_digest();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ObjectMove { .. }))
            .count(),
        2
    );
    assert_eq!(c.position_xyz(), [0.25, 0.0, 0.0]);
    assert_eq!(c.facing(), 0.75);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state_digest(), state);
    c.shutdown().unwrap();
    drop(c);
    let c = h.reopen(stationary());
    assert_eq!(c.world.state_digest(), state);
    assert_eq!(c.world.state().next_sequence, 2);
}

#[path = "quest_tests.rs"]
mod quest_tests;

#[path = "loot_tests.rs"]
mod loot_tests;

#[path = "pose_tests.rs"]
mod pose_tests;

#[path = "reputation_tests.rs"]
mod reputation_tests;

#[path = "interaction_tests.rs"]
mod interaction_tests;
#[path = "motion_tests.rs"]
mod motion_tests;

#[path = "creature_capability_tests.rs"]
mod creature_capability_tests;

#[path = "creature_model_width_tests.rs"]
mod creature_model_width_tests;

#[path = "creature_model_height_tests.rs"]
mod creature_model_height_tests;

#[path = "player_model_height_tests.rs"]
mod player_model_height_tests;

#[path = "near_point_primary_tests.rs"]
mod near_point_primary_tests;

#[path = "route_filter_tests.rs"]
mod route_filter_tests;

#[path = "event_scheduler_tests.rs"]
mod event_scheduler_tests;

#[path = "combat_authority_tests.rs"]
mod combat_authority_tests;

#[path = "two_owner_visitor_tests.rs"]
mod two_owner_visitor_tests;

#[path = "spell_authority_tests.rs"]
mod spell_authority_tests;

#[path = "spell_range_tests.rs"]
mod spell_range_tests;

#[path = "player_slow_tests.rs"]
mod player_slow_tests;

#[path = "admin_tests.rs"]
mod admin_tests;
#[path = "npc_spell_tests.rs"]
mod npc_spell_tests;

#[path = "level_sixty_tests.rs"]
mod level_sixty_tests;

#[path = "player_environment_tests.rs"]
mod player_environment_tests;

#[path = "route_commit_tests.rs"]
mod route_commit_tests;

#[path = "distinct_quest_loop_tests.rs"]
pub(crate) mod distinct_quest_loop_tests;

#[path = "event_ai_tests.rs"]
mod event_ai_tests;

#[cfg(not(benilla_headless_geometry))]
#[path = "resident_fixture.rs"]
pub(crate) mod resident_fixture;

#[path = "player_melee_tests.rs"]
mod player_melee_tests;

#[test]
fn dead_body_liveness_blocks_stock_cast_and_move_and_release_stays_unavailable_after_reopen() {
    let h = Harness::new();
    let data = death_profile();
    let mut c = h.start(data.clone());
    c.receive(cast(&c)).unwrap();
    for _ in 0..200 {
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
        if !c.world.state().player_alive() {
            break;
        }
    }
    assert!(!c.world.state().player_alive());
    assert_eq!(c.world.state().health, 0);
    // Acknowledge only the genuine pending root order; rejection must not consume it.
    if let Some(order) = c.orders.pending {
        c.receive(root_ack(&c, order.counter, move_flags::ROOT))
            .unwrap();
    }
    let position = c.world.state().position;
    let tick = c.tick();
    c.receive(ClientCommand::RepopRequest).unwrap();
    c.receive(cast(&c)).unwrap();
    c.receive(move_to([0.1, 0.0, 0.0])).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(c.tick(), tick + 1);
    assert!(out.events.iter().any(
        |event| matches!(event, SessionEvent::Notification { text } if text.contains("SpiritReleaseUnavailable"))
    ));
    assert!(c.world.last_tick().unwrap().events.iter().any(|event| {
        matches!(
            event,
            n::Event::Rejected(n::Rejection::SpiritReleaseUnavailable { .. })
        )
    }));
    assert!(!out
        .events
        .iter()
        .any(|event| matches!(event, SessionEvent::SpellStart { .. })));
    assert!(!c.world.state().player_alive());
    assert_eq!(c.world.state().health, 0);
    assert_eq!(c.world.state().position, position);
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let mut c = h.reopen(data);
    assert_eq!(c.world.state_digest(), digest);
    assert!(!c.world.state().player_alive());
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(c.world.state().health, 0);
    assert!(!c.world.state().player_alive());
}
