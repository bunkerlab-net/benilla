// SPDX-License-Identifier: GPL-3.0-or-later
//! Original authored test bindings, never user-local DBCs or a runnable Classic profile.
use super::*;
use crate::net::{ClientCommand, MoveKind};
use benilla_protocol::{messages as protocol, SessionEvent};
use benilla_world_client::QuestMetadata;
use benilla_world_data::{load_json, Limits};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
const FIXTURE: &[u8] =
    include_bytes!("../../../../../benilla-world-data/tests/fixtures/original-camp.json");
struct TestCatalog {
    data: WorldData,
    missing_spell: bool,
}
impl ClientCatalog for TestCatalog {
    fn item_metadata(&self, entry: u32) -> Option<protocol::ItemInfo> {
        let item = self.data.item(entry)?;
        Some(protocol::ItemInfo {
            class: 15,
            subclass: 0,
            name: item.name.clone(),
            display_info_id: item.display_id,
            quality: 1,
            flags: 0,
            buy_price: 0,
            sell_price: 0,
            inventory_type: 0,
            allowable_class: -1,
            allowable_race: -1,
            item_level: 0,
            required_level: 0,
            required_skill: 0,
            required_skill_rank: 0,
            required_spell: 0,
            required_honor_rank: 0,
            required_city_rank: 0,
            required_rep_faction: 0,
            required_rep_rank: 0,
            max_count: 0,
            stackable: u32::from(item.max_stack),
            container_slots: 0,
            stats: Vec::new(),
            damages: Vec::new(),
            dmg_min: 0.0,
            dmg_max: 0.0,
            dmg_type: 0,
            armor: 0,
            resistances: [0; 6],
            delay_ms: 0,
            ammo_type: 0,
            ranged_mod_range: 0.0,
            spells: Vec::new(),
            spell_charges_0: 0,
            use_spell: None,
            bonding: 0,
            description: "An original test token.".into(),
            page_text: 0,
            language_id: 0,
            page_material: 0,
            start_quest: 0,
            lock_id: 0,
            material: 0,
            sheath: 0,
            random_property: 0,
            block: 0,
            item_set: 0,
            max_durability: 0,
            area: 0,
            map: 0,
            bag_family: 0,
        })
    }
    fn quest_metadata(&self, entry: u32) -> Option<QuestMetadata> {
        let q = self.data.quest(entry)?;
        let mut rewards = [(0, 0); 4];
        for (i, item) in q.reward_items.iter().enumerate() {
            rewards[i] = (item.item_entry, u32::from(item.count));
        }
        Some(QuestMetadata {
            template: protocol::QuestTemplate {
                quest_id: entry,
                method: 2,
                level: 1,
                zone_or_sort: 0,
                quest_type: 0,
                rep_objective_faction: 0,
                rep_objective_value: 0,
                next_quest_in_chain: 0,
                money: q.reward_money as i32,
                money_max_level: 0,
                reward_spell: 0,
                src_item_id: 0,
                flags: 0,
                rewards,
                choices: [(0, 0); 6],
                point_map_id: 0,
                point_x: 0.0,
                point_y: 0.0,
                point_opt: 0,
                title: q.title.clone(),
                objectives_text: "Practice on two constructs.".into(),
                details: q.details.clone(),
                end_text: "Return to the steward.".into(),
                objectives: std::array::from_fn(|i| {
                    let (creature_or_go, required_count) = q
                        .objectives
                        .get(i)
                        .map_or((0, 0), |o| (o.creature_entry, u32::from(o.count)));
                    protocol::QuestObjective {
                        creature_or_go,
                        required_count,
                        item_id: 0,
                        item_count: 0,
                        text: String::new(),
                    }
                }),
            },
            request_text: "Have you finished the practice?".into(),
            offer_text: "Take this practice token.".into(),
        })
    }
    fn quest_greeting(&self, _: u32) -> Option<String> {
        Some("Welcome to the original practice camp.".into())
    }

    fn build(&self) -> u32 {
        5875
    }
    fn manifest_digest(&self) -> runtime::Digest {
        [5; 32]
    }
    fn contains_map(&self, id: u32) -> bool {
        id == 0
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        matches!(id, 770001 | 770002)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        id == 770003
    }
    fn player_power_type(&self, _: PlayerId) -> Option<u8> {
        Some(0)
    }
    fn contains_spell(&self, id: u32) -> bool {
        !self.missing_spell && matches!(id, 53001 | 53002)
    }
}
fn raw_data() -> WorldData {
    load_json(FIXTURE, Limits::default()).unwrap()
}
fn bound_data() -> WorldData {
    let mut export = raw_data().export().clone();
    export.creatures[0].display_id = 770001;
    export.creatures[1].display_id = 770002;
    export.items[0].display_id = 770003;
    export.abilities[0].entry = 53001;
    export.abilities[1].entry = 53002;
    load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap()
}
fn rules() -> Rules {
    let mut rules = Rules::original_camp();
    rules.starting_abilities = vec![53001, 53002];
    rules
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
            "benilla-world-controller-{}-{}",
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
    fn selected(&self) -> Selection {
        Selection::from_profile(&self.profile, &self.character).unwrap()
    }
    fn start(&self) -> Controller {
        self.with_rules(rules())
    }
    fn with_rules(&self, rules: Rules) -> Controller {
        self.enter(
            Entry::Create {
                world: [8; 16],
                seed: 42,
            },
            rules,
        )
        .unwrap()
    }
    fn enter(&self, entry: Entry, rules: Rules) -> Result<Controller, Error> {
        let data = bound_data();
        let catalog = TestCatalog {
            data: data.clone(),
            missing_spell: false,
        };
        Controller::enter_validated(&self.root, self.selected(), entry, data, rules, &catalog)
    }
    fn path(&self) -> PathBuf {
        world_path(&self.root, [8; 16]).unwrap()
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn ticks(c: &mut Controller, count: usize) -> Vec<SessionEvent> {
    let mut events = Vec::new();
    for _ in 0..count {
        let output = c.pump(c.tick_duration());
        assert!(output.pause.is_none(), "{:?}", output.pause);
        assert_eq!(output.ticks, 1);
        events.extend(output.events);
    }
    events
}
fn npc(c: &Controller, spawn: u64) -> u64 {
    c.projection
        .npc_guid(c.world.npc(spawn).unwrap().id)
        .unwrap()
}
fn send(c: &mut Controller, command: ClientCommand) {
    assert!(!c.receive(command).unwrap().logout);
}
fn move_to(pos: [f32; 3]) -> ClientCommand {
    ClientCommand::Move {
        kind: MoveKind::Heartbeat,
        pos,
        orientation: 0.5,
        flags: 0,
        pitch: 0.0,
        fall_time: 0,
        jump: None,
        transport: None,
    }
}
fn journal(c: &Controller) -> runtime::Journal {
    serde_json::from_slice(&c.world.journal_bytes().unwrap()).unwrap()
}
fn profile_bytes(h: &Harness) -> Vec<(String, Vec<u8>)> {
    let mut files = fs::read_dir(h.root.join("p2p-profile"))
        .unwrap()
        .map(|f| {
            let f = f.unwrap();
            (
                f.file_name().to_string_lossy().into_owned(),
                fs::read(f.path()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

#[test]
fn selected_owner_and_persisted_character_checked_before_world_files() {
    let h = Harness::new();
    let before = profile_bytes(&h);
    let mut foreign = h.character.clone();
    foreign.owner_id = [9; 16];
    assert!(matches!(
        Selection::from_profile(&h.profile, &foreign),
        Err(Error::WrongOwner)
    ));
    let mut absent = h.character.clone();
    absent.id = [9; 16];
    assert!(matches!(
        Selection::from_profile(&h.profile, &absent),
        Err(Error::WrongCharacter)
    ));
    let mut edited = h.character.clone();
    edited.name = "Alicechanged".into();
    assert!(matches!(
        Selection::from_profile(&h.profile, &edited),
        Err(Error::WrongCharacter)
    ));
    assert!(!h.root.join("p2p-worlds").exists());
    assert_eq!(before, profile_bytes(&h));
}
#[test]
fn production_entry_refuses_fixture_and_native_preparation_without_side_effects() {
    let h = Harness::new();
    let data = bound_data();
    let catalog = TestCatalog {
        data: data.clone(),
        missing_spell: false,
    };
    for entry in [
        Entry::Create {
            world: [8; 16],
            seed: 42,
        },
        Entry::Open { world: [8; 16] },
    ] {
        assert!(matches!(
            Controller::enter(
                &h.root,
                &h.profile,
                &h.character,
                entry,
                data.clone(),
                rules(),
                &catalog
            ),
            Err(Error::FixtureNotPlayable)
        ));
        let mut native = data.export().clone();
        native.source = Source::LocalCmangosClassic {
            core_revision: benilla_world_data::REVIEWED_CORE_REVISION.into(),
            database_revision: benilla_world_data::REVIEWED_DATABASE_REVISION.into(),
            client_build: 5875,
            schema_sha256: "1".repeat(64),
        };
        let native = load_json(&serde_json::to_vec(&native).unwrap(), Limits::default()).unwrap();
        assert!(matches!(
            Controller::enter(
                &h.root,
                &h.profile,
                &h.character,
                entry,
                native,
                rules(),
                &catalog
            ),
            Err(Error::NativeMechanicsUnavailable)
        ));
    }
    assert!(!h.root.join("p2p-worlds").exists());
}
#[test]
fn invalid_catalog_cannot_create_even_a_world_directory() {
    let h = Harness::new();
    let data = bound_data();
    let catalog = TestCatalog {
        data: data.clone(),
        missing_spell: true,
    };
    assert!(matches!(
        Controller::enter_validated(
            &h.root,
            h.selected(),
            Entry::Create {
                world: [8; 16],
                seed: 42
            },
            data,
            rules(),
            &catalog
        ),
        Err(Error::Client(_))
    ));
    assert!(!h.root.join("p2p-worlds").exists());
}
#[test]
fn open_is_not_create_and_reopen_requires_the_entire_solo_roster() {
    let h = Harness::new();
    assert!(h.enter(Entry::Open { world: [8; 16] }, rules()).is_err());
    assert!(!h.root.join("p2p-worlds").exists());
    fs::create_dir(h.root.join("p2p-worlds")).unwrap();
    let p = h.selected().player;
    let other = PlayerId {
        owner: [4; 16],
        character: [5; 16],
    };
    let w = World::new(bound_data(), rules(), [8; 16], 42, vec![p, other]).unwrap();
    let (store, _) = WorldStore::create(h.path(), &w).unwrap();
    drop(store);
    let before = fs::read(h.path().join("world.checkpoint")).unwrap();
    assert!(matches!(
        h.enter(Entry::Open { world: [8; 16] }, rules()),
        Err(Error::WrongRoster)
    ));
    assert_eq!(fs::read(h.path().join("world.checkpoint")).unwrap(), before);
}
#[test]
fn missing_corrupt_incompatible_and_duplicate_create_do_not_replace_worlds() {
    let h = Harness::new();
    let mut c = h.start();
    ticks(&mut c, 3);
    c.shutdown().unwrap();
    drop(c);
    let before = fs::read(h.path().join("world.checkpoint")).unwrap();
    assert!(matches!(
        h.enter(
            Entry::Create {
                world: [8; 16],
                seed: 42
            },
            rules()
        ),
        Err(Error::Store(benilla_world_store::Error::AlreadyExists))
    ));
    let mut changed = rules();
    changed.melee_damage += 1;
    assert!(h.enter(Entry::Open { world: [8; 16] }, changed).is_err());
    assert_eq!(before, fs::read(h.path().join("world.checkpoint")).unwrap());
    fs::write(h.path().join("world.checkpoint"), b"corrupt").unwrap();
    assert!(matches!(
        h.enter(Entry::Open { world: [8; 16] }, rules()),
        Err(Error::Store(benilla_world_store::Error::Corrupt))
    ));
    assert_eq!(
        b"corrupt",
        fs::read(h.path().join("world.checkpoint"))
            .unwrap()
            .as_slice()
    );
}
#[test]
fn ten_second_simulation_checkpoints_survive_more_than_journal_hard_cap() {
    let h = Harness::new();
    let before = profile_bytes(&h);
    let mut c = h.start();
    ticks(&mut c, 99);
    assert_eq!(c.store.generation(), 1);
    assert_eq!(journal(&c).records.len(), 99);
    ticks(&mut c, 1);
    assert_eq!(c.store.generation(), 2);
    assert_eq!(journal(&c).records.len(), 0);
    ticks(&mut c, 10_005);
    assert_eq!(c.world.state().tick, 10_105);
    assert_eq!(c.store.generation(), 102);
    let snapshot = c.world.snapshot_bytes().unwrap();
    let (saved, _, _) = c.shutdown().unwrap();
    assert_eq!(saved.tick, 10_105);
    assert_eq!(saved.generation, 103);
    assert_eq!(saved.durability, c.store.durability());
    drop(c);
    let restored = h.enter(Entry::Open { world: [8; 16] }, rules()).unwrap();
    assert_eq!(snapshot, restored.world.snapshot_bytes().unwrap());
    assert_eq!(before, profile_bytes(&h));
}
#[test]
fn journal_warning_checkpoints_before_ten_seconds_at_one_ms_ticks() {
    let h = Harness::new();
    let mut r = rules();
    r.tick_ms = 1;
    let mut c = h.with_rules(r);
    ticks(&mut c, runtime::MAX_JOURNAL_TICKS / 2 - 1);
    assert_eq!(c.store.generation(), 1);
    ticks(&mut c, 1);
    assert_eq!(c.store.generation(), 2);
    assert!(journal(&c).records.is_empty());
}
#[test]
fn fixed_clock_retains_catchup_debt_and_packet_bursts_never_create_ticks() {
    let h = Harness::new();
    let mut c = h.start();
    for _ in 0..1000 {
        send(&mut c, move_to([2.0, 0.0, 0.0]));
    }
    assert_eq!(c.world.state().tick, 0);
    assert!(c.actions.is_empty());
    assert_eq!(c.pump(Duration::from_millis(99)).ticks, 0);
    assert_eq!(c.pump(Duration::from_millis(1)).ticks, 1);
    let output = c.pump(Duration::from_secs(10));
    assert_eq!(output.ticks, MAX_TICKS_PER_PUMP);
    assert_eq!(output.pending_ticks, 100 - MAX_TICKS_PER_PUMP as u128);
    let mut total = output.ticks;
    loop {
        let output = c.pump(Duration::ZERO);
        total += output.ticks;
        if output.pending_ticks == 0 {
            break;
        }
    }
    assert_eq!(total, 100);
    assert_eq!(c.world.state().tick, 101);
}
#[test]
fn input_backpressure_and_invalid_commands_are_isolated_from_solo_clock() {
    let h = Harness::new();
    let mut c = h.start();
    assert!(matches!(
        c.receive(move_to([f32::NAN, 0.0, 0.0])),
        Err(Error::InvalidMovement)
    ));
    assert!(matches!(
        c.receive(ClientCommand::CastSpellAtDest {
            spell_id: 53001,
            dest: [0.0; 3]
        }),
        Err(Error::UnsupportedCommand)
    ));
    assert!(matches!(
        c.receive(ClientCommand::RepopRequest),
        Err(Error::UnsupportedCommand)
    ));
    assert!(matches!(
        c.receive(ClientCommand::CastSpell {
            spell_id: 53001,
            target: Some(u64::MAX)
        }),
        Err(Error::Client(benilla_world_client::Error::UnknownTarget))
    ));
    for _ in 0..MAX_ACTIONS {
        send(
            &mut c,
            ClientCommand::CastSpell {
                spell_id: 1,
                target: None,
            },
        );
    }
    assert!(matches!(
        c.receive(ClientCommand::CastSpell {
            spell_id: 53002,
            target: None
        }),
        Err(Error::Backpressure)
    ));
    let events = ticks(&mut c, 1);
    assert_eq!(c.actions.len(), MAX_ACTIONS - 1);
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::CastResult { success: false, .. })));
    assert_eq!(c.world.player(c.selected.player).unwrap().next_sequence, 1);
    assert_eq!(c.world.state().tick, 1);
}
#[test]
fn movement_is_coalesced_sampled_bounded_and_corrected_through_self_move_pipeline() {
    let h = Harness::new();
    let mut c = h.start();
    send(&mut c, move_to([100.0, 0.0, 0.0]));
    send(&mut c, move_to([3.0, 4.0, 0.0]));
    let events = ticks(&mut c, 1);
    assert_eq!(c.position().xyz, [600, 800, 0]);
    assert!(events.iter().any(|e| matches!(e, SessionEvent::UnitMove { guid, position, orientation, .. } if *guid == c.player_guid() && *position == [0.6,0.8,0.0] && *orientation == 0.5)));
    ticks(&mut c, 4);
    assert_eq!(c.position().xyz, [3000, 4000, 0]);
    for delta in [[1, 1, 1], [200000000, -200000000, 200000000], [-1, 0, 0]] {
        for limit in [1, 2, 1000, 100000] {
            let bounded = bounded_delta(delta, limit);
            assert!(
                bounded
                    .iter()
                    .map(|v| i64::from(*v).pow(2) as u64)
                    .sum::<u64>()
                    <= u64::from(limit).pow(2)
            );
            assert_ne!(bounded, [0; 3]);
        }
    }
}
#[test]
fn empty_ticks_finish_cast_and_autoattack_is_scheduled_until_death_not_packet_count() {
    let h = Harness::new();
    let mut c = h.start();
    assert_eq!(c.selected_player(), h.selected().player);
    assert_eq!(c.facing(), h.character.position.orientation);
    let target = npc(&c, 2);
    send(
        &mut c,
        ClientCommand::CastSpell {
            spell_id: 53001,
            target: Some(target),
        },
    );
    let events = ticks(&mut c, 1);
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::SpellStart {
            spell_id: 53001,
            ..
        }
    )));
    let events = ticks(&mut c, 5);
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::SpellGo {
            spell_id: 53001,
            ..
        }
    )));
    send(&mut c, move_to([1.0, 0.0, 0.0]));
    ticks(&mut c, 1);
    for _ in 0..100 {
        send(&mut c, ClientCommand::AttackSwing { guid: target });
    }
    assert_eq!(c.world.state().tick, 7);
    let events = ticks(&mut c, 21);
    assert_eq!(c.world.npc(2).unwrap().health, 0);
    assert!(c.autoattack.is_none());
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::AttackStart { .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::AttackStop { .. }))
            .count(),
        1
    );
}
#[test]
fn attack_stop_refusal_and_target_generation_do_not_keep_swinging() {
    let h = Harness::new();
    let mut c = h.start();
    let target = npc(&c, 2);
    send(&mut c, ClientCommand::AttackSwing { guid: target });
    ticks(&mut c, 1);
    assert!(c.autoattack.is_none()); // out of range
    send(&mut c, move_to([1.0, 0.0, 0.0]));
    ticks(&mut c, 1);
    send(&mut c, ClientCommand::AttackSwing { guid: target });
    ticks(&mut c, 1);
    let events = c.receive(ClientCommand::AttackStop).unwrap().events;
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::AttackStop { .. })));
    let hp = c.world.npc(2).unwrap().health;
    ticks(&mut c, 15);
    assert_eq!(c.world.npc(2).unwrap().health, hp);
}
#[test]
fn failed_checkpoint_pauses_retains_journal_and_explicit_retry_resumes_same_world() {
    let h = Harness::new();
    let mut c = h.start();
    ticks(&mut c, 99);
    let checkpoint = fs::read(h.path().join("world.checkpoint")).unwrap();
    // An existing staging file is a safe, definitely pre-publication failure. The store must
    // neither overwrite it nor acknowledge current history. Removal simulates user repair.
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"external pending",
    )
    .unwrap();
    let output = c.pump(c.tick_duration());
    assert_eq!(output.ticks, 1);
    assert!(matches!(
        output.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(!output.events.is_empty());
    assert_eq!(c.saved_tick, 0);
    assert_eq!(journal(&c).records.len(), 100);
    let state = c.world.snapshot_bytes().unwrap();
    assert_eq!(c.pump(Duration::from_secs(100)).ticks, 0);
    assert!(matches!(
        c.receive(move_to([1.0, 0.0, 0.0])),
        Err(Error::Paused)
    ));
    assert_eq!(state, c.world.snapshot_bytes().unwrap());
    assert_eq!(
        checkpoint,
        fs::read(h.path().join("world.checkpoint")).unwrap()
    );
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    assert_eq!(c.retry_checkpoint().unwrap().tick, 100);
    assert!(c.pause().is_none());
    assert!(journal(&c).records.is_empty());
    ticks(&mut c, 1);
    assert_eq!(c.world.state().tick, 101);
}
#[test]
fn external_file_change_latches_reopen_requirement_without_replacing_memory_or_disk() {
    let h = Harness::new();
    let mut c = h.start();
    ticks(&mut c, 99);
    fs::write(h.path().join("world.checkpoint"), b"external edit").unwrap();
    let output = c.pump(c.tick_duration());
    assert!(matches!(
        output.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: true
        }
    ));
    let bytes = c.world.snapshot_bytes().unwrap();
    assert!(matches!(c.retry_checkpoint(), Err(Error::Paused)));
    assert_eq!(c.pump(Duration::from_secs(10)).ticks, 0);
    assert_eq!(bytes, c.world.snapshot_bytes().unwrap());
    assert_eq!(journal(&c).records.len(), 100);
    assert_eq!(
        fs::read(h.path().join("world.checkpoint")).unwrap(),
        b"external edit"
    );
}
#[test]
fn orderly_shutdown_commits_only_admitted_ticks_and_failure_keeps_controller_for_retry() {
    let h = Harness::new();
    let mut c = h.start();
    ticks(&mut c, 3);
    send(&mut c, move_to([10.0, 0.0, 0.0]));
    send(
        &mut c,
        ClientCommand::CastSpell {
            spell_id: 53002,
            target: None,
        },
    );
    fs::write(h.path().join(".world-checkpoint.pending"), b"repair needed").unwrap();
    assert!(c.shutdown().is_err());
    assert!(!c.is_closed());
    assert_eq!(journal(&c).records.len(), 3);
    assert_eq!(c.actions.len(), 1);
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    assert_eq!(c.pump(Duration::from_secs(10)).ticks, 0); // a closing session cannot restart
    let (saved, cancelled, _) = c.shutdown().unwrap();
    assert_eq!(saved.tick, 3);
    assert_eq!(cancelled, 2);
    assert!(c.is_closed());
    assert!(c.actions.is_empty());
    assert!(journal(&c).records.is_empty());
}

fn kill(c: &mut Controller, spawn: u64) {
    let target = npc(c, spawn);
    send(c, ClientCommand::AttackSwing { guid: target });
    for _ in 0..30 {
        ticks(c, 1);
        if c.world.npc(spawn).unwrap().health == 0 {
            return;
        }
    }
    panic!("test encounter did not finish");
}
#[test]
fn partially_looted_corpse_reopens_with_remaining_stable_slots_and_no_duplicate_reward() {
    let h = Harness::new();
    let before = profile_bytes(&h);
    let mut c = h.start();
    send(&mut c, move_to([1.0, 0.0, 0.0]));
    ticks(&mut c, 1);
    kill(&mut c, 2);
    let target = npc(&c, 2);
    send(&mut c, ClientCommand::Loot { guid: target });
    ticks(&mut c, 1);
    send(&mut c, ClientCommand::LootMoney);
    let events = ticks(&mut c, 1);
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootClearMoney)));
    assert_eq!(c.world.player(c.selected.player).unwrap().money, 1);
    assert!(c.world.npc(2).unwrap().loot_items[0].is_some());
    c.shutdown().unwrap();
    drop(c);
    let mut c = h.enter(Entry::Open { world: [8; 16] }, rules()).unwrap();
    let restored = c.initial_events().unwrap();
    assert!(restored.iter().any(|e|matches!(e,SessionEvent::LootResponse{guid,gold:0,items,..} if *guid==target && items.len()==1 && items[0].slot==0)));
    assert!(!restored
        .iter()
        .any(|e| matches!(e, SessionEvent::ItemPushResult(_))));
    send(&mut c, ClientCommand::AutostoreLootItem { slot: 0 });
    ticks(&mut c, 1);
    let p = c.world.player(c.selected.player).unwrap();
    assert_eq!(p.inventory.len(), 1);
    assert_eq!(p.inventory[0].count, 1);
    assert_eq!(p.money, 1);
    send(&mut c, ClientCommand::AutostoreLootItem { slot: 0 });
    ticks(&mut c, 1);
    assert_eq!(
        c.world.player(c.selected.player).unwrap().inventory[0].count,
        1
    );
    let snapshot = c.world.snapshot_bytes().unwrap();
    c.shutdown().unwrap();
    drop(c);
    let c = h.enter(Entry::Open { world: [8; 16] }, rules()).unwrap();
    assert_eq!(c.world.snapshot_bytes().unwrap(), snapshot);
    assert_eq!(profile_bytes(&h), before);
}
#[test]
fn queued_loot_captures_original_window_and_stale_release_cannot_close_new_window() {
    let h = Harness::new();
    let mut export = bound_data().export().clone();
    let mut extra = export.spawns[1].clone();
    extra.spawn_id = 3;
    export.spawns.push(extra);
    let data = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let catalog = TestCatalog {
        data: data.clone(),
        missing_spell: false,
    };
    let mut r = rules();
    r.melee_damage = 30;
    let mut c = Controller::enter_validated(
        &h.root,
        h.selected(),
        Entry::Create {
            world: [8; 16],
            seed: 42,
        },
        data,
        r,
        &catalog,
    )
    .unwrap();
    send(&mut c, move_to([1.0, 0.0, 0.0]));
    ticks(&mut c, 1);
    kill(&mut c, 2);
    kill(&mut c, 3);
    let a = npc(&c, 2);
    let b = npc(&c, 3);
    send(&mut c, ClientCommand::Loot { guid: a });
    ticks(&mut c, 1);
    send(&mut c, ClientCommand::Loot { guid: b });
    send(&mut c, ClientCommand::AutostoreLootItem { slot: 0 }); // captured A at receipt, before B opens
    ticks(&mut c, 1);
    let events = ticks(&mut c, 1);
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::Notification{text} if text.contains("LootNotOpen"))));
    assert!(c.world.npc(2).unwrap().loot_items[0].is_some());
    assert!(c.world.npc(3).unwrap().loot_items[0].is_some());
    assert_eq!(
        c.world.player(c.selected.player).unwrap().active_loot,
        Some(c.world.npc(3).unwrap().id)
    );
    send(&mut c, ClientCommand::LootRelease { guid: a });
    ticks(&mut c, 1);
    assert_eq!(
        c.world.player(c.selected.player).unwrap().active_loot,
        Some(c.world.npc(3).unwrap().id)
    );
}
#[test]
fn quest_queries_only_project_panels_and_exact_reward_intent_survives_reload() {
    let h = Harness::new();
    let mut r = rules();
    r.melee_damage = 30;
    let mut c = h.with_rules(r.clone());
    let giver = npc(&c, 1);
    for command in [
        ClientCommand::QuestgiverStatusQuery { npc: giver },
        ClientCommand::QuestgiverHello { npc: giver },
        ClientCommand::QuestgiverQuery {
            npc: giver,
            quest: 910001,
        },
        ClientCommand::QuestQuery { quest: 910001 },
        ClientCommand::ItemQuery {
            entry: 920001,
            guid: 0,
        },
    ] {
        assert!(!c.receive(command).unwrap().events.is_empty());
    }
    assert_eq!(c.world.state().tick, 0);
    assert!(c.world.player(c.selected.player).unwrap().quests.is_empty());
    send(
        &mut c,
        ClientCommand::QuestgiverAccept {
            npc: giver,
            quest: 910001,
        },
    );
    ticks(&mut c, 1);
    send(&mut c, move_to([1.0, 0.0, 0.0]));
    ticks(&mut c, 1);
    kill(&mut c, 2);
    ticks(&mut c, 50);
    kill(&mut c, 2);
    let p = c.world.player(c.selected.player).unwrap();
    assert_eq!(p.quests[&910001].counts, [2]);
    assert!(!p.quests[&910001].turned_in);
    let before = c.world.snapshot_bytes().unwrap();
    let progress = c
        .receive(ClientCommand::QuestgiverComplete {
            npc: giver,
            quest: 910001,
        })
        .unwrap();
    assert!(matches!(progress.events[0], SessionEvent::QuestProgress(_)));
    let offer = c
        .receive(ClientCommand::QuestgiverRequestReward {
            npc: giver,
            quest: 910001,
        })
        .unwrap();
    assert!(matches!(offer.events[0], SessionEvent::QuestOffer(_)));
    assert!(matches!(
        c.receive(ClientCommand::QuestgiverChooseReward {
            npc: giver,
            quest: 910001,
            choice: 1
        }),
        Err(Error::UnsupportedCommand)
    ));
    assert_eq!(before, c.world.snapshot_bytes().unwrap());
    send(
        &mut c,
        ClientCommand::QuestgiverChooseReward {
            npc: giver,
            quest: 910001,
            choice: 0,
        },
    );
    let events = ticks(&mut c, 1);
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestComplete(_))));
    let p = c.world.player(c.selected.player).unwrap();
    assert!(p.quests[&910001].turned_in);
    assert_eq!(p.money, 10);
    assert_eq!(p.inventory[0].count, 1);
    send(
        &mut c,
        ClientCommand::QuestgiverChooseReward {
            npc: giver,
            quest: 910001,
            choice: 0,
        },
    );
    ticks(&mut c, 1);
    assert_eq!(c.world.player(c.selected.player).unwrap().money, 10);
    let snapshot = c.world.snapshot_bytes().unwrap();
    c.shutdown().unwrap();
    drop(c);
    let c = h.enter(Entry::Open { world: [8; 16] }, r).unwrap();
    assert_eq!(snapshot, c.world.snapshot_bytes().unwrap());
}
#[test]
fn driver_projects_actual_events_rejects_bad_input_and_returns_only_after_durable_logout() {
    let h = Harness::new();
    let mut c = h.start();
    ticks(&mut c, 3);
    let (event_tx, event_rx) = crossbeam_channel::bounded(256);
    let (command_tx, command_rx) = crossbeam_channel::bounded(8);
    let (_control_tx, control_rx) = crossbeam_channel::bounded(1);
    command_tx
        .send(ClientCommand::CastSpellAtDest {
            spell_id: 53001,
            dest: [0.0; 3],
        })
        .unwrap();
    command_tx.send(ClientCommand::Logout).unwrap();
    let exit: Exit = run(
        &mut c,
        &event_tx,
        &command_rx,
        &control_rx,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(exit.saved.tick, 3);
    assert_eq!(exit.cancelled_inputs, 0);
    assert!(c.is_closed());
    let events: Vec<_> = event_rx.try_iter().collect();
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellBook { .. })));
    assert!(events.iter().any(
        |e| matches!(e,SessionEvent::Notification{text} if text.contains("UnsupportedCommand"))
    ));
    assert_eq!(c.store.boundary().tick, 3);
}
#[test]
fn driver_keeps_failed_checkpoint_alive_until_explicit_repair_and_retry() {
    let h = Harness::new();
    let mut c = h.start();
    ticks(&mut c, 99);
    fs::write(h.path().join(".world-checkpoint.pending"), b"repair needed").unwrap();
    let (event_tx, event_rx) = crossbeam_channel::bounded(256);
    let (command_tx, command_rx) = crossbeam_channel::bounded(8);
    let (control_tx, control_rx) = crossbeam_channel::bounded(1);
    let worker = std::thread::spawn(move || {
        let result = run(
            &mut c,
            &event_tx,
            &command_rx,
            &control_rx,
            &std::sync::atomic::AtomicBool::new(false),
        );
        (c, result)
    });
    loop {
        let event = event_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if matches!(event,SessionEvent::Notification{text} if text.contains("World paused at tick 100"))
        {
            break;
        }
    }
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    control_tx.send(Control::RetryCheckpoint).unwrap();
    loop {
        let event = event_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if matches!(event,SessionEvent::Notification{text} if text.contains("gameplay resumed")) {
            break;
        }
    }
    command_tx.send(ClientCommand::Logout).unwrap();
    let (c, result) = worker.join().unwrap();
    assert!(result.is_ok());
    assert!(c.is_closed());
    assert!(c.saved_tick >= 100);
}
#[test]
fn output_channel_failure_stops_clock_but_checkpoints_committed_world() {
    let h = Harness::new();
    let mut c = h.start();
    ticks(&mut c, 3);
    let (event_tx, event_rx) = crossbeam_channel::bounded(1);
    drop(event_rx);
    let (_command_tx, command_rx) = crossbeam_channel::bounded(1);
    let (_control_tx, control_rx) = crossbeam_channel::bounded(1);
    assert!(run(
        &mut c,
        &event_tx,
        &command_rx,
        &control_rx,
        &std::sync::atomic::AtomicBool::new(false)
    )
    .is_err());
    assert!(c.is_closed());
    assert_eq!(c.saved_tick, 3);
    assert!(journal(&c).records.is_empty());
}
#[test]
fn actual_app_dispatch_never_falls_back_after_explicit_gameplay_selection_fails() {
    let h = Harness::new();
    let data = bound_data();
    let catalog = TestCatalog {
        data: data.clone(),
        missing_spell: false,
    };
    let (request, _) = Request::new(Entry::Open { world: [8; 16] }, data, rules(), catalog);
    let explored = std::cell::Cell::new(false);
    let result = dispatch(
        Some(request),
        (),
        |(), request| {
            request
                .enter(
                    &h.root,
                    &h.profile,
                    &h.character,
                    benilla_config::Ruleset::default(),
                )
                .map(|_| ())
        },
        |()| {
            explored.set(true);
            Ok(())
        },
    );
    assert!(matches!(result, Err(Error::FixtureNotPlayable)));
    assert!(!explored.get());
    assert!(!h.root.join("p2p-worlds").exists());
    let result: Result<(), Error> = dispatch(
        None,
        (),
        |(), _| panic!("no gameplay requested"),
        |()| {
            explored.set(true);
            Ok(())
        },
    );
    assert!(result.is_ok());
    assert!(explored.get());
    // Keep the source-level harness connected to the real selected-character entry point.
    let source = include_str!("../../p2p.rs");
    let entry = &source[source.find("fn run_character(").unwrap()
        ..source.find("fn run_exploration_character(").unwrap()];
    assert!(entry.contains("world::dispatch("));
    assert!(entry.contains("request.enter(&root, store, character, settings.1)?"));
    assert!(entry.contains("run_gameplay_character("));
}
#[test]
fn rates_use_integer_milli_and_unsupported_drop_changes_fail_closed() {
    let h = Harness::new();
    for (rate, numerator) in [("0.001", 1), ("1.125", 1125), ("1000", 1_000_000)] {
        let config =
            benilla_config::AppConfig::parse(&format!("[ruleset]\nxp_rate={rate}"), &h.root)
                .unwrap();
        let applied = apply_rates(rules(), config.ruleset).unwrap();
        assert_eq!(applied.xp_rate_numerator, numerator);
        assert_eq!(applied.xp_rate_denominator, 1000);
    }
    for rate in ["0.5", "2"] {
        let config =
            benilla_config::AppConfig::parse(&format!("[ruleset]\ndrop_rate={rate}"), &h.root)
                .unwrap();
        assert!(matches!(
            apply_rates(rules(), config.ruleset),
            Err(Error::UnsupportedDropRate)
        ));
    }
}
#[test]
fn owning_session_does_not_return_or_drop_world_on_recoverable_logout_save_failure() {
    let h = Harness::new();
    let mut c = h.start();
    ticks(&mut c, 3);
    fs::write(h.path().join(".world-checkpoint.pending"), b"repair needed").unwrap();
    let expected = c.world.snapshot_bytes().unwrap();
    let (event_tx, event_rx) = crossbeam_channel::bounded(256);
    let (command_tx, command_rx) = crossbeam_channel::bounded(8);
    let (control_tx, control_rx) = crossbeam_channel::bounded(1);
    command_tx.send(ClientCommand::Logout).unwrap();
    // Match the production closure's ownership: the controller is dropped immediately when
    // run returns. Thus a premature return really releases its exclusive lock in this test.
    let worker = std::thread::spawn(move || {
        run(
            &mut c,
            &event_tx,
            &command_rx,
            &control_rx,
            &std::sync::atomic::AtomicBool::new(false),
        )
    });
    loop {
        let event = event_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if matches!(event,SessionEvent::Notification{text} if text.contains("Logout could not save"))
        {
            break;
        }
    }
    assert!(!worker.is_finished());
    assert!(matches!(
        h.enter(Entry::Open { world: [8; 16] }, rules()),
        Err(Error::Store(benilla_world_store::Error::Locked))
    ));
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    control_tx.send(Control::RetryCheckpoint).unwrap();
    let exit = worker.join().unwrap().unwrap();
    assert_eq!(exit.saved.tick, 3);
    let restored = h.enter(Entry::Open { world: [8; 16] }, rules()).unwrap();
    assert_eq!(restored.world.snapshot_bytes().unwrap(), expected);
}
#[test]
fn blocked_predicted_movement_is_corrected_and_never_replayed_after_cast() {
    let h = Harness::new();
    let mut c = h.start();
    let target = npc(&c, 2);
    send(
        &mut c,
        ClientCommand::CastSpell {
            spell_id: 53001,
            target: Some(target),
        },
    );
    ticks(&mut c, 1);
    assert!(c.world.player(c.selected.player).unwrap().casting.is_some());
    let received = c.receive(move_to([3.0, 0.0, 0.0])).unwrap();
    assert!(received.events.iter().any(|e|matches!(e,SessionEvent::UnitMove{guid,position,..} if *guid==c.player_guid() && *position==[0.0;3])));
    assert!(c.desired_pose.is_none());
    ticks(&mut c, 10);
    assert_eq!(c.position().xyz, [0; 3]);
    // A move coalesced before a cast must also be corrected when the admitted cast blocks it.
    send(&mut c, move_to([3.0, 0.0, 0.0]));
    c.next_lane = 0;
    send(
        &mut c,
        ClientCommand::CastSpell {
            spell_id: 53001,
            target: Some(target),
        },
    );
    let events = ticks(&mut c, 1);
    assert!(c.world.player(c.selected.player).unwrap().casting.is_some());
    assert!(events.iter().any(|e|matches!(e,SessionEvent::UnitMove{guid,position,..} if *guid==c.player_guid() && *position==[0.0;3])));
    assert!(c.desired_pose.is_none());
    ticks(&mut c, 10);
    assert_eq!(c.position().xyz, [0; 3]);
}
#[test]
fn dead_player_predicted_movement_is_corrected_and_does_not_queue_revive_motion() {
    let h = Harness::new();
    let mut r = rules();
    r.player_health = 2;
    let mut c = h.with_rules(r);
    send(&mut c, move_to([2.0, 0.0, 0.0]));
    ticks(&mut c, 2);
    let target = npc(&c, 2);
    send(&mut c, ClientCommand::AttackSwing { guid: target });
    ticks(&mut c, 1);
    assert_eq!(c.world.player(c.selected.player).unwrap().health, 0);
    let received = c.receive(move_to([4.0, 0.0, 0.0])).unwrap();
    assert!(received.events.iter().any(|e|matches!(e,SessionEvent::UnitMove{guid,position,..} if *guid==c.player_guid() && *position==[2.0,0.0,0.0])));
    assert!(c.desired_pose.is_none());
    ticks(&mut c, 10);
    assert_eq!(c.position().xyz, [2000, 0, 0]);
}
#[test]
fn driver_delivers_blocked_move_correction_to_self_mover_and_never_drifts_after_cast() {
    let h = Harness::new();
    let mut c = h.start();
    let target = npc(&c, 2);
    send(
        &mut c,
        ClientCommand::CastSpell {
            spell_id: 53001,
            target: Some(target),
        },
    );
    ticks(&mut c, 1);
    let selected_guid = c.player_guid();
    let (event_tx, event_rx) = crossbeam_channel::bounded(256);
    let (command_tx, command_rx) = crossbeam_channel::bounded(8);
    let (_control_tx, control_rx) = crossbeam_channel::bounded(1);
    command_tx.send(move_to([3.0, 0.0, 0.0])).unwrap();
    let worker = std::thread::spawn(move || {
        let result = run(
            &mut c,
            &event_tx,
            &command_rx,
            &control_rx,
            &std::sync::atomic::AtomicBool::new(false),
        );
        (c, result)
    });
    let mut corrected = false;
    loop {
        let event = event_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if matches!(event,SessionEvent::UnitMove{guid,position,..} if guid==selected_guid && position==[0.0;3])
        {
            corrected = true;
        }
        if matches!(
            event,
            SessionEvent::SpellGo {
                spell_id: 53001,
                ..
            }
        ) {
            assert!(corrected, "correction must precede cast completion");
            break;
        }
    }
    // Allow further real driver ticks to expose deferred-goal drift after cast completion.
    let mut later_ticks = 0;
    while later_ticks < 3 {
        let event = event_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if matches!(event,SessionEvent::ObjectValues{guid,..} if guid==selected_guid) {
            later_ticks += 1;
        }
        if let SessionEvent::UnitMove { guid, position, .. } = event {
            if guid == selected_guid {
                assert_eq!(position, [0.0; 3]);
            }
        }
    }
    command_tx.send(ClientCommand::Logout).unwrap();
    let (c, result) = worker.join().unwrap();
    assert!(result.is_ok());
    assert_eq!(c.position().xyz, [0; 3]);
    assert!(c.desired_pose.is_none());
}
#[test]
fn first_avatar_fields_are_world_progression_with_profile_appearance_preserved() {
    let h = Harness::new();
    let mut r = rules();
    r.level_thresholds = vec![0, 1];
    r.melee_damage = 30;
    let mut c = h.with_rules(r);
    send(&mut c, move_to([1.0, 0.0, 0.0]));
    ticks(&mut c, 1);
    kill(&mut c, 2);
    assert_eq!(c.world.player(c.selected.player).unwrap().level, 2);
    let mut appearance =
        benilla_protocol::ObjectFields::from_pairs(&[(22, 999), (34, 1), (193, 12345)])
            .into_created(protocol::ObjectType::Player);
    appearance.merge(c.initial_player_fields().unwrap());
    let fields: std::collections::BTreeMap<_, _> = appearance.raw_fields().collect();
    assert_eq!(
        fields.get(&22).copied(),
        Some(c.world.player(c.selected.player).unwrap().health)
    );
    assert_eq!(fields.get(&34).copied(), Some(2));
    assert_eq!(fields.get(&193).copied(), Some(12345));
    assert_eq!(h.character.level, 1); // world progress did not rewrite the exploration profile
    let source = include_str!("../../p2p.rs");
    let start = source.find("fn run_gameplay_character(").unwrap();
    let end = source.find("fn run_exploration_character(").unwrap();
    let gameplay = &source[start..end];
    assert!(gameplay.contains("fields.merge(controller.initial_player_fields()?)"));
    assert!(gameplay.contains("ticket.clear()"));
    assert!(!gameplay.contains("Network::"));
}
