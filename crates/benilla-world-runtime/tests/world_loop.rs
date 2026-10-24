// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_world_data::{load_json, Limits, WorldData};
use benilla_world_runtime::*;
use sha2::{Digest as _, Sha256};
const FIXTURE: &[u8] = include_bytes!("../../benilla-world-data/tests/fixtures/original-camp.json");
const WORLD: WorldId = [7; 16];
const QUEST: u32 = 910001;
fn player(n: u8) -> PlayerId {
    PlayerId {
        owner: [n; 16],
        character: [n + 32; 16],
    }
}
fn data() -> WorldData {
    load_json(FIXTURE, Limits::default()).unwrap()
}
fn new_world() -> World {
    World::new(data(), Rules::original_camp(), WORLD, 42, vec![player(1)]).unwrap()
}
fn owned(w: &World, p: PlayerId, command: Command) -> OwnedInput {
    OwnedInput {
        authenticated_owner: p.owner,
        input: Input {
            world: WORLD,
            tick: w.state().tick + 1,
            player: p,
            sequence: w.player(p).unwrap().next_sequence,
            command,
        },
    }
}
fn send(w: &mut World, p: PlayerId, command: Command) -> TickResult {
    let input = owned(w, p, command);
    w.step(w.state().tick + 1, vec![input]).unwrap()
}
fn ok(w: &mut World, command: Command) -> TickResult {
    let result = send(w, player(1), command);
    assert_eq!(result.outcomes[0].rejection, None);
    result
}
fn reject(w: &mut World, command: Command, reason: Rejection) {
    assert_eq!(
        send(w, player(1), command).outcomes[0].rejection,
        Some(reason)
    );
}
fn wait(w: &mut World, tick: u64) {
    while w.state().tick < tick {
        w.step(w.state().tick + 1, vec![]).unwrap();
    }
}
fn npc(w: &World) -> NpcId {
    w.npc(2).unwrap().id
}
fn giver(w: &World) -> NpcId {
    w.npc(1).unwrap().id
}
fn accept(w: &mut World) {
    ok(
        w,
        Command::AcceptQuest {
            quest: QUEST,
            giver: giver(w),
        },
    );
}
fn approach(w: &mut World) {
    for _ in 0..2 {
        ok(
            w,
            Command::Move {
                delta: [1000, 0, 0],
            },
        );
    }
}
fn kill_with_melee_and_cast(w: &mut World) -> NpcId {
    let target = npc(w);
    ok(w, Command::Melee { target });
    let first_attack = w.state().tick;
    ok(
        w,
        Command::Cast {
            ability: 930001,
            target: Target::Npc { id: target },
        },
    );
    assert!(w.player(player(1)).unwrap().casting.is_some());
    wait(w, first_attack + 9);
    ok(w, Command::Melee { target });
    ok(
        w,
        Command::Cast {
            ability: 930001,
            target: Target::Npc { id: target },
        },
    );
    wait(w, first_attack + 16);
    assert_eq!(w.npc(2).unwrap().health, 0);
    target
}
#[test]
fn complete_solo_loop_replays_and_restores_exactly() {
    let mut world = new_world();
    let genesis = world.snapshot_bytes().unwrap();
    accept(&mut world);
    approach(&mut world);
    let first = kill_with_melee_and_cast(&mut world);
    assert_eq!(world.player(player(1)).unwrap().xp, 50);
    ok(&mut world, Command::Loot { target: first });
    reject(
        &mut world,
        Command::Loot { target: first },
        Rejection::AlreadyLooted,
    );
    reject(
        &mut world,
        Command::TurnInQuest {
            quest: QUEST,
            giver: NpcId {
                spawn: 1,
                generation: 0,
            },
        },
        Rejection::QuestIncomplete,
    );
    let respawn = world.npc(2).unwrap().respawn_at.unwrap();
    wait(&mut world, respawn);
    assert_eq!(world.npc(2).unwrap().id.generation, 1);
    reject(
        &mut world,
        Command::Melee { target: first },
        Rejection::StaleTarget,
    );
    let second = kill_with_melee_and_cast(&mut world);
    ok(&mut world, Command::Loot { target: second });
    let giver = giver(&world);
    ok(
        &mut world,
        Command::TurnInQuest {
            quest: QUEST,
            giver,
        },
    );
    reject(
        &mut world,
        Command::TurnInQuest {
            quest: QUEST,
            giver,
        },
        Rejection::AlreadyTurnedIn,
    );
    reject(
        &mut world,
        Command::AcceptQuest {
            quest: QUEST,
            giver,
        },
        Rejection::AlreadyTurnedIn,
    );
    let p = world.player(player(1)).unwrap();
    assert_eq!((p.xp, p.money, p.level), (125, 12, 1));
    assert_eq!(
        p.inventory,
        vec![InventoryItem {
            instance: 1,
            slot: 0,
            item_entry: 920001,
            count: 3
        }]
    );
    assert_eq!(p.quests[&QUEST].counts, vec![2]);
    assert!(p.quests[&QUEST].turned_in);
    let journal = world.journal_bytes().unwrap();
    let replayed = World::replay(data(), Rules::original_camp(), &genesis, &journal).unwrap();
    assert_eq!(replayed.state_digest(), world.state_digest());
    assert_eq!(
        replayed.snapshot_bytes().unwrap(),
        world.snapshot_bytes().unwrap()
    );
    let final_snapshot = world.snapshot_bytes().unwrap();
    let mut restored = World::restore(data(), Rules::original_camp(), &final_snapshot).unwrap();
    assert_eq!(restored.state(), world.state());
    let tick = world.state().tick + 1;
    assert_eq!(restored.step(tick, vec![]), world.step(tick, vec![]));
    assert_eq!(restored.state_digest(), world.state_digest());
}
#[test]
fn identical_complete_batches_ignore_arrival_order_and_bind_owners() {
    let mut forward = World::new(
        data(),
        Rules::original_camp(),
        WORLD,
        44,
        vec![player(2), player(1)],
    )
    .unwrap();
    let mut backward = World::new(
        data(),
        Rules::original_camp(),
        WORLD,
        44,
        vec![player(1), player(2)],
    )
    .unwrap();
    for tick in 1..=120 {
        let command = |w: &World, p| match tick {
            1 => Command::AcceptQuest {
                quest: QUEST,
                giver: giver(w),
            },
            2 | 3 => Command::Move {
                delta: [1000, 0, 0],
            },
            4 | 14 | 24 | 74 | 84 | 94 => Command::Melee { target: npc(w) },
            25 | 95 => Command::Loot { target: npc(w) },
            _ if p == player(1) => Command::Cast {
                ability: 930002,
                target: Target::SelfPlayer,
            },
            _ => Command::Move { delta: [0, 0, 0] },
        };
        let a = owned(&forward, player(1), command(&forward, player(1)));
        let b = owned(&forward, player(2), command(&forward, player(2)));
        let result1 = forward.step(tick, vec![a.clone(), b.clone()]).unwrap();
        let result2 = backward.step(tick, vec![b, a]).unwrap();
        assert_eq!(result1, result2);
        assert_eq!(forward.state_digest(), backward.state_digest());
    }
    assert_eq!(
        forward.snapshot_bytes().unwrap(),
        backward.snapshot_bytes().unwrap()
    );
    assert_eq!(
        forward.journal_bytes().unwrap(),
        backward.journal_bytes().unwrap()
    );
}
#[test]
fn bad_envelopes_leave_the_entire_tick_unchanged() {
    let mut w = new_world();
    let base = w.snapshot_bytes().unwrap();
    let good = owned(
        &w,
        player(1),
        Command::Move {
            delta: [1000, 0, 0],
        },
    );
    let mut bad = good.clone();
    bad.authenticated_owner = player(2).owner;
    assert_eq!(w.step(1, vec![bad]), Err(Error::OwnerMismatch));
    let mut bad = good.clone();
    bad.input.player.character = [99; 16];
    assert_eq!(w.step(1, vec![bad]), Err(Error::UnknownPlayer));
    let mut bad = good.clone();
    bad.input.world = [99; 16];
    assert_eq!(w.step(1, vec![bad]), Err(Error::WrongWorld));
    let mut bad = good.clone();
    bad.input.tick = 3;
    assert_eq!(w.step(1, vec![bad]), Err(Error::WrongTick));
    let mut bad = good.clone();
    bad.input.sequence = 1;
    assert_eq!(w.step(1, vec![bad]), Err(Error::InvalidSequence));
    assert_eq!(
        w.step(1, vec![good.clone(), good.clone()]),
        Err(Error::DuplicateInput)
    );
    assert_eq!(w.snapshot_bytes().unwrap(), base);
    w.step(1, vec![good.clone()]).unwrap();
    let base = w.snapshot_bytes().unwrap();
    let mut replay = good;
    replay.input.tick = 2;
    assert_eq!(w.step(2, vec![replay]), Err(Error::InvalidSequence));
    assert_eq!(w.snapshot_bytes().unwrap(), base);
}
#[test]
fn movements_are_euclidean_bounded_and_bad_actions_consume_sequence() {
    let mut w = new_world();
    reject(
        &mut w,
        Command::Move {
            delta: [i32::MIN, 0, 0],
        },
        Rejection::InvalidMove,
    );
    reject(
        &mut w,
        Command::Move {
            delta: [1000, 1000, 0],
        },
        Rejection::InvalidMove,
    );
    assert_eq!(w.player(player(1)).unwrap().position.xyz, [0, 0, 0]);
    assert_eq!(w.player(player(1)).unwrap().next_sequence, 2);
    ok(
        &mut w,
        Command::Move {
            delta: [600, 800, 0],
        },
    );
    assert_eq!(w.player(player(1)).unwrap().position.xyz, [600, 800, 0]);
    let target = npc(&w);
    reject(&mut w, Command::Melee { target }, Rejection::OutOfRange);
}
#[test]
fn cast_time_cooldowns_resources_and_heal_are_authoritative() {
    let mut w = new_world();
    approach(&mut w);
    let target = npc(&w);
    ok(&mut w, Command::Melee { target });
    assert_eq!(w.player(player(1)).unwrap().health, 58);
    reject(&mut w, Command::Melee { target }, Rejection::NotReady);
    ok(
        &mut w,
        Command::Cast {
            ability: 930002,
            target: Target::SelfPlayer,
        },
    );
    let start = w.state().tick;
    reject(
        &mut w,
        Command::Move { delta: [1, 0, 0] },
        Rejection::BusyCasting,
    );
    wait(&mut w, start + 4);
    assert_eq!(w.player(player(1)).unwrap().health, 58);
    let result = w.step(start + 5, vec![]).unwrap();
    assert!(result.events.contains(&Event::Healed {
        player: player(1),
        amount: 2
    }));
    assert_eq!(w.player(player(1)).unwrap().health, 60);
    reject(
        &mut w,
        Command::Cast {
            ability: 930002,
            target: Target::SelfPlayer,
        },
        Rejection::NotReady,
    );
    ok(
        &mut w,
        Command::Move {
            delta: [-1000, 0, 0],
        },
    );
    for _ in 0..14 {
        let ready = w.player(player(1)).unwrap().global_ready_at;
        wait(&mut w, ready);
        ok(
            &mut w,
            Command::Cast {
                ability: 930002,
                target: Target::SelfPlayer,
            },
        );
    }
    let ready = w.player(player(1)).unwrap().global_ready_at;
    wait(&mut w, ready);
    assert_eq!(w.player(player(1)).unwrap().mana, 0);
    reject(
        &mut w,
        Command::Cast {
            ability: 930002,
            target: Target::SelfPlayer,
        },
        Rejection::InsufficientMana,
    );
}
#[test]
fn snapshot_mid_cast_restores_pending_completion() {
    let mut w = new_world();
    let target = npc(&w);
    ok(
        &mut w,
        Command::Cast {
            ability: 930001,
            target: Target::Npc { id: target },
        },
    );
    wait(&mut w, 3);
    let mut restored =
        World::restore(data(), Rules::original_camp(), &w.snapshot_bytes().unwrap()).unwrap();
    for tick in 4..=12 {
        assert_eq!(w.step(tick, vec![]), restored.step(tick, vec![]));
        assert_eq!(w.state_digest(), restored.state_digest());
    }
    assert_eq!(w.npc(2).unwrap().health, 25);
    assert_eq!(w.player(player(1)).unwrap().mana, 28);
}
#[test]
fn dead_target_cast_fizzles_and_cannot_duplicate_kill_credit() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    let mut w = World::new(data(), rules, WORLD, 1, vec![player(1), player(2)]).unwrap();
    for tick in 1..=2 {
        let a = owned(
            &w,
            player(1),
            Command::Move {
                delta: [1000, 0, 0],
            },
        );
        let b = owned(
            &w,
            player(2),
            Command::Move {
                delta: [1000, 0, 0],
            },
        );
        w.step(tick, vec![a, b]).unwrap();
    }
    let target = npc(&w);
    let cast = owned(
        &w,
        player(1),
        Command::Cast {
            ability: 930001,
            target: Target::Npc { id: target },
        },
    );
    let hit = owned(&w, player(2), Command::Melee { target });
    w.step(3, vec![hit, cast]).unwrap();
    wait(&mut w, 7);
    let result = w.step(8, vec![]).unwrap();
    assert!(result.events.contains(&Event::CastFizzled {
        player: player(1),
        ability: 930001,
        reason: Rejection::TargetDead
    }));
    assert_eq!(w.player(player(1)).unwrap().xp, 0);
    assert_eq!(w.player(player(2)).unwrap().xp, 50);
    reject(&mut w, Command::Loot { target }, Rejection::LootNotOwned);
}
#[test]
fn player_death_blocks_actions_and_revive_obeys_delay() {
    let mut rules = Rules::original_camp();
    rules.player_health = 2;
    let mut w = World::new(data(), rules.clone(), WORLD, 1, vec![player(1)]).unwrap();
    approach(&mut w);
    let target = npc(&w);
    let result = ok(&mut w, Command::Melee { target });
    assert!(result
        .events
        .contains(&Event::PlayerDied { player: player(1) }));
    reject(&mut w, Command::Move { delta: [0, 0, 0] }, Rejection::Dead);
    reject(&mut w, Command::Revive, Rejection::NotReady);
    let revive_at = w.player(player(1)).unwrap().revive_at.unwrap();
    let saved = w.snapshot_bytes().unwrap();
    let mut restored = World::restore(data(), rules, &saved).unwrap();
    wait(&mut restored, revive_at);
    ok(&mut restored, Command::Revive);
    assert_eq!(restored.player(player(1)).unwrap().health, 2);
    assert_eq!(restored.player(player(1)).unwrap().position.xyz, [0, 0, 0]);
}
#[test]
fn bag_capacity_rejects_entire_reward_without_marking_quest_done() {
    let mut export = data().export().clone();
    export.quests[0].objectives[0].count = 1;
    let data = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let mut rules = Rules::original_camp();
    rules.inventory_slots = 1;
    rules.melee_damage = 30;
    rules.loot[0].items[0].count = 20;
    let mut w = World::new(data, rules, WORLD, 1, vec![player(1)]).unwrap();
    accept(&mut w);
    approach(&mut w);
    let target = npc(&w);
    ok(&mut w, Command::Melee { target });
    ok(&mut w, Command::Loot { target });
    let giver = giver(&w);
    for _ in 0..2 {
        reject(
            &mut w,
            Command::TurnInQuest {
                quest: QUEST,
                giver,
            },
            Rejection::InventoryFull,
        );
    }
    let p = w.player(player(1)).unwrap();
    assert_eq!((p.xp, p.money, p.inventory[0].count), (50, 1, 20));
    assert!(!p.quests[&QUEST].turned_in);
}
#[test]
fn quest_giver_level_and_distance_gates_are_enforced() {
    let mut export = data().export().clone();
    export.quests[0].min_level = 2;
    let custom = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let mut w = World::new(custom, Rules::original_camp(), WORLD, 1, vec![player(1)]).unwrap();
    let giver = giver(&w);
    reject(
        &mut w,
        Command::AcceptQuest {
            quest: QUEST,
            giver,
        },
        Rejection::QuestLevel,
    );
    let mut w = new_world();
    let wrong = npc(&w);
    reject(
        &mut w,
        Command::AcceptQuest {
            quest: QUEST,
            giver: wrong,
        },
        Rejection::WrongQuestGiver,
    );
    for _ in 0..5 {
        ok(
            &mut w,
            Command::Move {
                delta: [-1000, 0, 0],
            },
        );
    }
    reject(
        &mut w,
        Command::AcceptQuest {
            quest: QUEST,
            giver,
        },
        Rejection::OutOfRange,
    );
    reject(
        &mut w,
        Command::TurnInQuest {
            quest: QUEST,
            giver,
        },
        Rejection::OutOfRange,
    );
}
#[test]
fn xp_uses_ported_formula_and_explicit_integer_rate_and_original_level_table() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    rules.xp_rate_numerator = 3;
    rules.xp_rate_denominator = 2;
    rules.level_thresholds = vec![0, 75, 200];
    let mut w = World::new(data(), rules, WORLD, 1, vec![player(1)]).unwrap();
    approach(&mut w);
    let target = npc(&w);
    ok(&mut w, Command::Melee { target });
    assert_eq!(
        (
            w.player(player(1)).unwrap().xp,
            w.player(player(1)).unwrap().level
        ),
        (75, 2)
    );
}
#[test]
fn millisecond_durations_round_up_to_ticks() {
    let mut rules = Rules::original_camp();
    rules.tick_ms = 333;
    let mut w = World::new(data(), rules, WORLD, 1, vec![player(1)]).unwrap();
    let target = npc(&w);
    ok(
        &mut w,
        Command::Cast {
            ability: 930001,
            target: Target::Npc { id: target },
        },
    );
    wait(&mut w, 2);
    assert_eq!(w.npc(2).unwrap().health, 30);
    wait(&mut w, 3);
    assert_eq!(w.npc(2).unwrap().health, 25);
    assert_eq!(w.player(player(1)).unwrap().global_ready_at, 5);
}
#[test]
fn rejected_unrepresented_content_does_not_become_synthetic_classic() {
    let mut export = data().export().clone();
    export.creatures[1].faction_template_id = 1;
    let custom = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    assert!(matches!(
        World::new(custom, Rules::original_camp(), WORLD, 1, vec![player(1)]),
        Err(Error::UnsupportedContent)
    ));
    let mut export = data().export().clone();
    export.source = benilla_world_data::Source::LocalCmangosClassic {
        core_revision: benilla_world_data::REVIEWED_CORE_REVISION.into(),
        database_revision: benilla_world_data::REVIEWED_DATABASE_REVISION.into(),
        client_build: 5875,
        schema_sha256: "a".repeat(64),
    };
    let custom = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    assert!(matches!(
        World::new(custom, Rules::original_camp(), WORLD, 1, vec![player(1)]),
        Err(Error::UnsupportedContent)
    ));
}
fn mutate_snapshot(w: &World, change: impl FnOnce(&mut Snapshot)) -> Vec<u8> {
    let mut snapshot: Snapshot = serde_json::from_slice(&w.snapshot_bytes().unwrap()).unwrap();
    change(&mut snapshot);
    let mut hash = Sha256::new();
    hash.update(b"benilla-world-state-v2\0");
    hash.update(serde_json::to_vec(&snapshot.state).unwrap());
    snapshot.state_digest = hash.finalize().into();
    let mut hash = Sha256::new();
    hash.update(b"benilla-world-snapshot-v2\0");
    hash.update(
        serde_json::to_vec(&(
            snapshot.format_version,
            snapshot.content_digest,
            snapshot.rules_digest,
            snapshot.state_digest,
            snapshot.journal_tip,
            &snapshot.state,
        ))
        .unwrap(),
    );
    snapshot.checksum = hash.finalize().into();
    serde_json::to_vec(&snapshot).unwrap()
}
#[test]
fn snapshots_validate_references_bounds_and_state_even_with_recomputed_hash() {
    let w = new_world();
    type Corruption = Box<dyn FnOnce(&mut Snapshot)>;
    let corruptions: Vec<Corruption> = vec![
        Box::new(|s| s.state.players[0].health = 61),
        Box::new(|s| s.state.players[0].mana = 31),
        Box::new(|s| s.state.players[0].level = 60),
        Box::new(|s| s.state.players[0].position.xyz[0] = i32::MIN),
        Box::new(|s| {
            s.state.players[0]
                .cooldowns
                .insert(999, 1)
                .map(drop)
                .unwrap_or(())
        }),
        Box::new(|s| {
            s.state.players[0].inventory.push(InventoryItem {
                instance: 1,
                slot: 0,
                item_entry: 920001,
                count: 21,
            })
        }),
        Box::new(|s| s.state.players[0].health = 0),
        Box::new(|s| s.state.npcs.pop().map(drop).unwrap_or(())),
        Box::new(|s| s.state.npcs[1].tagged_by = Some(player(2))),
        Box::new(|s| s.state.npcs[1].looted = true),
        Box::new(|s| s.state.players[0].next_sequence = 1),
        Box::new(|s| s.state.revision = 1),
        Box::new(|s| {
            s.state.players[0]
                .quests
                .insert(
                    QUEST,
                    QuestProgress {
                        counts: vec![3],
                        turned_in: true,
                    },
                )
                .map(drop)
                .unwrap_or(())
        }),
    ];
    for change in corruptions {
        assert!(matches!(
            World::restore(data(), Rules::original_camp(), &mutate_snapshot(&w, change)),
            Err(Error::InvalidSnapshot)
        ));
    }
}
#[test]
fn snapshot_and_command_formats_fail_closed_and_are_bounded() {
    let w = new_world();
    let snapshot = w.snapshot_bytes().unwrap();
    let mut noncanonical = snapshot.clone();
    noncanonical.push(b'\n');
    assert!(matches!(
        World::restore(data(), Rules::original_camp(), &noncanonical),
        Err(Error::NonCanonical)
    ));
    assert!(matches!(
        World::restore(
            data(),
            Rules::original_camp(),
            &vec![b' '; MAX_SNAPSHOT_BYTES + 1]
        ),
        Err(Error::TooLarge)
    ));
    assert!(matches!(
        Input::from_json(&vec![b' '; MAX_INPUT_BYTES + 1]),
        Err(Error::TooLarge)
    ));
    assert!(matches!(
        Input::from_json(br#"{"world":[],"command":{"kind":"grant_xp","amount":999}}"#),
        Err(Error::Json)
    ));
    let mut value: serde_json::Value = serde_json::from_slice(&snapshot).unwrap();
    value["unknown"] = true.into();
    assert!(matches!(
        World::restore(
            data(),
            Rules::original_camp(),
            &serde_json::to_vec(&value).unwrap()
        ),
        Err(Error::Json)
    ));
    let mut rules = Rules::original_camp();
    rules.melee_damage += 1;
    assert!(matches!(
        World::restore(data(), rules, &snapshot),
        Err(Error::Incompatible)
    ));
    let mut s: Snapshot = serde_json::from_slice(&snapshot).unwrap();
    s.state.seed += 1;
    assert!(matches!(
        World::restore(
            data(),
            Rules::original_camp(),
            &serde_json::to_vec(&s).unwrap()
        ),
        Err(Error::InvalidSnapshot)
    ));
}
#[test]
fn modified_journal_results_commands_and_base_are_rejected() {
    let mut w = new_world();
    let base = w.snapshot_bytes().unwrap();
    approach(&mut w);
    let bytes = w.journal_bytes().unwrap();
    let original: Journal = serde_json::from_slice(&bytes).unwrap();
    let mut bad = original.clone();
    bad.records[0].result.outcomes[0].rejection = Some(Rejection::Dead);
    assert!(matches!(
        World::replay(
            data(),
            Rules::original_camp(),
            &base,
            &serde_json::to_vec(&bad).unwrap()
        ),
        Err(Error::ReplayMismatch)
    ));
    let mut bad = original.clone();
    bad.records[0].inputs[0].command = Command::Move { delta: [1, 0, 0] };
    assert!(matches!(
        World::replay(
            data(),
            Rules::original_camp(),
            &base,
            &serde_json::to_vec(&bad).unwrap()
        ),
        Err(Error::ReplayMismatch)
    ));
    let mut bad = original.clone();
    bad.records.remove(0);
    assert!(matches!(
        World::replay(
            data(),
            Rules::original_camp(),
            &base,
            &serde_json::to_vec(&bad).unwrap()
        ),
        Err(Error::ReplayMismatch)
    ));
    let mut bad = original.clone();
    bad.records.pop();
    assert!(matches!(
        World::replay(
            data(),
            Rules::original_camp(),
            &base,
            &serde_json::to_vec(&bad).unwrap()
        ),
        Err(Error::ReplayMismatch)
    ));
    let mut bad = original;
    bad.base.world = [8; 16];
    assert!(matches!(
        World::replay(
            data(),
            Rules::original_camp(),
            &base,
            &serde_json::to_vec(&bad).unwrap()
        ),
        Err(Error::ReplayMismatch)
    ));
}
#[test]
fn committed_checkpoint_starts_a_new_replay_boundary() {
    let mut w = new_world();
    approach(&mut w);
    let base = w.snapshot_bytes().unwrap();
    w.checkpoint_committed(&w.join_boundary()).unwrap();
    let target = npc(&w);
    ok(&mut w, Command::Melee { target });
    let replay = World::replay(
        data(),
        Rules::original_camp(),
        &base,
        &w.journal_bytes().unwrap(),
    )
    .unwrap();
    assert_eq!(
        replay.snapshot_bytes().unwrap(),
        w.snapshot_bytes().unwrap()
    );
}
#[test]
fn zero_id_duplicate_owner_and_excess_players_fail_at_genesis() {
    assert!(matches!(
        World::new(data(), Rules::original_camp(), [0; 16], 0, vec![player(1)]),
        Err(Error::InvalidWorld)
    ));
    assert!(matches!(
        World::new(data(), Rules::original_camp(), WORLD, 0, vec![]),
        Err(Error::InvalidPlayers)
    ));
    let mut other = player(1);
    other.character = [99; 16];
    assert!(matches!(
        World::new(
            data(),
            Rules::original_camp(),
            WORLD,
            0,
            vec![player(1), other]
        ),
        Err(Error::InvalidPlayers)
    ));
    assert!(matches!(
        World::new(
            data(),
            Rules::original_camp(),
            WORLD,
            0,
            (1..=17).map(player).collect()
        ),
        Err(Error::InvalidPlayers)
    ));
}

#[test]
fn journal_limit_is_atomic_and_checkpoint_releases_capacity() {
    let mut w = new_world();
    wait(&mut w, MAX_JOURNAL_TICKS as u64);
    let before = w.snapshot_bytes().unwrap();
    assert_eq!(w.step(w.state().tick + 1, vec![]), Err(Error::JournalFull));
    assert_eq!(before, w.snapshot_bytes().unwrap());
    w.checkpoint_committed(&w.join_boundary()).unwrap();
    w.step(w.state().tick + 1, vec![]).unwrap();
}
#[test]
fn reader_is_bounded_and_snapshot_checksum_covers_journal_tip() {
    let mut reader = std::io::Cursor::new(vec![b' '; MAX_SNAPSHOT_BYTES + 200]);
    assert!(matches!(
        World::restore_reader(data(), Rules::original_camp(), &mut reader),
        Err(Error::TooLarge)
    ));
    assert_eq!(reader.position(), (MAX_SNAPSHOT_BYTES + 1) as u64);
    let w = new_world();
    let mut snapshot: Snapshot = serde_json::from_slice(&w.snapshot_bytes().unwrap()).unwrap();
    snapshot.journal_tip[0] ^= 1;
    assert!(matches!(
        World::restore(
            data(),
            Rules::original_camp(),
            &serde_json::to_vec(&snapshot).unwrap()
        ),
        Err(Error::InvalidSnapshot)
    ));
}

#[test]
fn leveling_past_acceptance_maximum_does_not_strand_completed_quest() {
    let mut export = data().export().clone();
    export.quests[0].max_level = 1;
    export.quests[0].objectives[0].count = 1;
    let custom = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    rules.level_thresholds = vec![0, 50, 200];
    let mut w = World::new(custom, rules, WORLD, 1, vec![player(1)]).unwrap();
    accept(&mut w);
    approach(&mut w);
    let target = npc(&w);
    ok(&mut w, Command::Melee { target });
    assert_eq!(w.player(player(1)).unwrap().level, 2);
    let giver = giver(&w);
    ok(
        &mut w,
        Command::TurnInQuest {
            quest: QUEST,
            giver,
        },
    );
    assert!(w.player(player(1)).unwrap().quests[&QUEST].turned_in);
}

#[test]
fn review_randomized_commands_preserve_roundtrip_and_replay() {
    let mut w = World::new(
        data(),
        Rules::original_camp(),
        WORLD,
        1,
        vec![player(1), player(2)],
    )
    .unwrap();
    let mut seed = 42u64;
    for tick in 1..=1200 {
        let mut inputs = Vec::new();
        for p in [player(1), player(2)] {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let target = npc(&w);
            let giver = giver(&w);
            let cmd = match (seed >> 32) % 14 {
                0 => Command::AcceptQuest {
                    quest: QUEST,
                    giver,
                },
                1 => Command::TurnInQuest {
                    quest: QUEST,
                    giver,
                },
                2 => Command::Loot { target },
                3 => Command::Revive,
                4 => Command::Cast {
                    ability: 930001,
                    target: Target::Npc { id: target },
                },
                5 => Command::Cast {
                    ability: 930002,
                    target: Target::SelfPlayer,
                },
                6 => Command::Move {
                    delta: [1000, 0, 0],
                },
                7 => Command::Move {
                    delta: [-1000, 0, 0],
                },
                8 => Command::Melee { target },
                9 => Command::OpenLoot { target },
                10 => Command::TakeLootItem { target, slot: 0 },
                11 => Command::TakeLootMoney { target },
                12 => Command::ReleaseLoot { target },
                _ => Command::Move {
                    delta: [i32::MIN, 0, 0],
                },
            };
            inputs.push(owned(&w, p, cmd));
        }
        let base = w.snapshot_bytes().unwrap();
        w.checkpoint_committed(&w.join_boundary()).unwrap();
        w.step(tick, inputs).unwrap();
        let snapshot = w.snapshot_bytes().unwrap();
        let restored = World::restore(data(), Rules::original_camp(), &snapshot).unwrap();
        assert_eq!(snapshot, restored.snapshot_bytes().unwrap());
        let replayed = World::replay(
            data(),
            Rules::original_camp(),
            &base,
            &w.journal_bytes().unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot, replayed.snapshot_bytes().unwrap());
    }
}

#[test]
fn review_tick_limit_is_atomic_and_extreme_ranges_are_safe() {
    let w = new_world();
    let bytes = mutate_snapshot(&w, |s| {
        s.state.tick = MAX_TICK - 1;
        s.state.revision = MAX_TICK - 1;
        s.state.players[0].position.xyz = [MAX_COORD; 3];
    });
    let mut w = World::restore(data(), Rules::original_camp(), &bytes).unwrap();
    let target = npc(&w);
    reject(
        &mut w,
        Command::Cast {
            ability: 930001,
            target: Target::Npc { id: target },
        },
        Rejection::OutOfRange,
    );
    let snapshot = w.snapshot_bytes().unwrap();
    assert!(matches!(
        w.step(MAX_TICK + 1, vec![]),
        Err(Error::LimitReached)
    ));
    assert_eq!(snapshot, w.snapshot_bytes().unwrap());
    World::restore(data(), Rules::original_camp(), &snapshot).unwrap();
}

#[test]
fn stale_durable_checkpoint_receipt_cannot_discard_newer_ticks() {
    let mut w = new_world();
    let persisted = w.join_boundary();
    w.step(1, vec![]).unwrap();
    let journal = w.journal_bytes().unwrap();
    assert_eq!(
        w.checkpoint_committed(&persisted),
        Err(Error::StaleCheckpoint)
    );
    assert_eq!(w.journal_bytes().unwrap(), journal);
    assert!(!w.checkpoint_due());
    wait(&mut w, (MAX_JOURNAL_TICKS / 2) as u64);
    assert!(w.checkpoint_due());
}

#[test]
fn exact_config_milli_xp_rates_are_representable_without_float_conversion() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    rules.xp_rate_numerator = 99_999;
    rules.xp_rate_denominator = 1000;
    let mut w = World::new(data(), rules, WORLD, 1, vec![player(1)]).unwrap();
    approach(&mut w);
    let target = npc(&w);
    ok(&mut w, Command::Melee { target });
    assert_eq!(w.player(player(1)).unwrap().xp, 4_999);
}

#[test]
fn physical_melee_calls_integer_armor_port_without_reclassifying_direct_abilities() {
    let mut export = data().export().clone();
    export.creatures[1].armor = 485;
    let armored = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let rules = Rules::original_camp();
    assert_eq!(
        rules.armor_numeric_policy,
        benilla_sim::classic::NUMERIC_POLICY_VERSION
    );
    let mut w = World::new(armored.clone(), rules.clone(), WORLD, 1, vec![player(1)]).unwrap();
    let base = w.snapshot_bytes().unwrap();
    approach(&mut w);
    let target = npc(&w);
    let result = ok(&mut w, Command::Melee { target });
    assert!(result.events.contains(&Event::DamageNpc {
        id: target,
        player: player(1),
        amount: 5
    }));
    assert_eq!(w.npc(2).unwrap().health, 25);
    ok(
        &mut w,
        Command::Cast {
            ability: 930001,
            target: Target::Npc { id: target },
        },
    );
    let completes = w
        .player(player(1))
        .unwrap()
        .casting
        .as_ref()
        .unwrap()
        .completes_at;
    wait(&mut w, completes);
    assert_eq!(w.npc(2).unwrap().health, 20);
    let replayed = World::replay(
        armored.clone(),
        rules.clone(),
        &base,
        &w.journal_bytes().unwrap(),
    )
    .unwrap();
    assert_eq!(
        replayed.snapshot_bytes().unwrap(),
        w.snapshot_bytes().unwrap()
    );
    let mut incompatible = rules;
    incompatible.armor_numeric_policy += 1;
    assert!(matches!(
        World::new(armored, incompatible, WORLD, 1, vec![player(1)]),
        Err(Error::InvalidRules)
    ));
}

fn quick_world(mut rules: Rules) -> World {
    rules.melee_damage = 30;
    World::new(data(), rules, WORLD, 42, vec![player(1)]).unwrap()
}
fn quick_kill(w: &mut World) -> NpcId {
    let target = npc(w);
    ok(w, Command::Melee { target });
    assert_eq!(w.npc(2).unwrap().health, 0);
    target
}
fn loot_effects(w: &World) -> (u64, Vec<InventoryItem>, u64, Npc) {
    let p = w.player(player(1)).unwrap();
    (
        p.money,
        p.inventory.clone(),
        p.next_item_instance,
        w.npc(2).unwrap().clone(),
    )
}

#[test]
fn explicit_loot_is_exact_once_and_partial_slots_survive_reopen_restore_replay() {
    let mut export = data().export().clone();
    let mut second = export.items[0].clone();
    second.entry = 920002;
    export.items.push(second);
    let custom = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    rules.loot[0].items.push(benilla_world_data::ItemStack {
        item_entry: 920002,
        count: 2,
    });
    let mut w = World::new(custom.clone(), rules.clone(), WORLD, 42, vec![player(1)]).unwrap();
    let genesis = w.snapshot_bytes().unwrap();
    approach(&mut w);
    let target = quick_kill(&mut w);
    let initial = loot_effects(&w);
    for _ in 0..2 {
        let result = ok(&mut w, Command::OpenLoot { target });
        assert_eq!(
            result.events,
            vec![Event::LootOpened {
                player: player(1),
                id: target
            }]
        );
        assert_eq!(loot_effects(&w), initial);
    }
    let result = ok(&mut w, Command::TakeLootItem { target, slot: 1 });
    assert_eq!(
        result.events,
        vec![Event::LootItemTaken {
            player: player(1),
            id: target,
            slot: 1
        }]
    );
    let n = w.npc(2).unwrap();
    assert_eq!(n.loot_items.len(), 2);
    assert!(n.loot_items[0].is_some());
    assert!(n.loot_items[1].is_none());
    assert!(!n.looted);
    assert_eq!(
        w.player(player(1)).unwrap().inventory,
        vec![InventoryItem {
            instance: 1,
            slot: 0,
            item_entry: 920002,
            count: 2,
        }]
    );
    let before = loot_effects(&w);
    reject(
        &mut w,
        Command::TakeLootItem { target, slot: 1 },
        Rejection::LootSlotEmpty,
    );
    reject(
        &mut w,
        Command::TakeLootItem {
            target,
            slot: u8::MAX,
        },
        Rejection::InvalidLootSlot,
    );
    assert_eq!(loot_effects(&w), before);
    let active_snapshot = w.snapshot_bytes().unwrap();
    let restored = World::restore(custom.clone(), rules.clone(), &active_snapshot).unwrap();
    assert_eq!(
        restored.player(player(1)).unwrap().active_loot,
        Some(target)
    );
    ok(&mut w, Command::ReleaseLoot { target });
    let before = loot_effects(&w);
    reject(
        &mut w,
        Command::TakeLootMoney { target },
        Rejection::LootNotOpen,
    );
    assert_eq!(loot_effects(&w), before);
    let partial_snapshot = w.snapshot_bytes().unwrap();
    let mut restored = World::restore(custom.clone(), rules.clone(), &partial_snapshot).unwrap();
    for command in [
        Command::OpenLoot { target },
        Command::TakeLootMoney { target },
        Command::TakeLootMoney { target },
        Command::TakeLootItem { target, slot: 0 },
        Command::OpenLoot { target },
        Command::ReleaseLoot { target },
        Command::ReleaseLoot { target },
    ] {
        assert_eq!(
            send(&mut w, player(1), command.clone()),
            send(&mut restored, player(1), command)
        );
        assert_eq!(w.state_digest(), restored.state_digest());
    }
    let p = w.player(player(1)).unwrap();
    assert_eq!(p.money, 1);
    assert_eq!(p.next_item_instance, 3);
    assert_eq!(
        p.inventory[1],
        InventoryItem {
            instance: 2,
            slot: 1,
            item_entry: 920001,
            count: 1
        }
    );
    assert!(w.npc(2).unwrap().looted);
    assert_eq!(w.npc(2).unwrap().loot_items, vec![None, None]);
    let replayed = World::replay(custom, rules, &genesis, &w.journal_bytes().unwrap()).unwrap();
    assert_eq!(
        replayed.snapshot_bytes().unwrap(),
        w.snapshot_bytes().unwrap()
    );
}

#[test]
fn explicit_loot_requires_owner_dead_target_and_open_generation() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    let mut w = World::new(data(), rules, WORLD, 42, vec![player(1), player(2)]).unwrap();
    let target = npc(&w);
    reject(&mut w, Command::OpenLoot { target }, Rejection::Alive);
    approach(&mut w);
    quick_kill(&mut w);
    let before = loot_effects(&w);
    for command in [
        Command::OpenLoot { target },
        Command::TakeLootMoney { target },
        Command::TakeLootItem { target, slot: 0 },
    ] {
        assert_eq!(
            send(&mut w, player(2), command).outcomes[0].rejection,
            Some(Rejection::LootNotOwned)
        );
    }
    reject(
        &mut w,
        Command::TakeLootItem { target, slot: 0 },
        Rejection::LootNotOpen,
    );
    reject(
        &mut w,
        Command::TakeLootMoney { target },
        Rejection::LootNotOpen,
    );
    assert_eq!(loot_effects(&w), before);
    ok(&mut w, Command::OpenLoot { target });
    ok(&mut w, Command::TakeLootMoney { target });
    let before = loot_effects(&w);
    reject(
        &mut w,
        Command::TakeLootMoney { target },
        Rejection::LootMoneyEmpty,
    );
    assert_eq!(loot_effects(&w), before);
    let duplicate = owned(&w, player(1), Command::TakeLootItem { target, slot: 0 });
    w.step(w.state().tick + 1, vec![duplicate.clone()]).unwrap();
    let snapshot = w.snapshot_bytes().unwrap();
    let mut duplicate = duplicate;
    duplicate.input.tick = w.state().tick + 1;
    assert_eq!(
        w.step(w.state().tick + 1, vec![duplicate]),
        Err(Error::InvalidSequence)
    );
    assert_eq!(w.snapshot_bytes().unwrap(), snapshot);
}

#[test]
fn full_bag_take_and_legacy_loot_are_atomic_after_prospective_stack_merge() {
    let mut export = data().export().clone();
    export.quests[0].objectives[0].count = 1;
    export.quests[0].reward_items[0].count = 19;
    let custom = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    rules.inventory_slots = 1;
    rules.loot[0].items[0].count = 2;
    let mut w = World::new(custom, rules, WORLD, 42, vec![player(1)]).unwrap();
    accept(&mut w);
    approach(&mut w);
    let target = quick_kill(&mut w);
    let giver = giver(&w);
    ok(
        &mut w,
        Command::TurnInQuest {
            quest: QUEST,
            giver,
        },
    );
    ok(&mut w, Command::OpenLoot { target });
    let before = loot_effects(&w);
    for command in [
        Command::TakeLootItem { target, slot: 0 },
        Command::Loot { target },
    ] {
        reject(&mut w, command, Rejection::InventoryFull);
        assert_eq!(loot_effects(&w), before);
        assert_eq!(w.player(player(1)).unwrap().active_loot, Some(target));
    }
    ok(&mut w, Command::TakeLootMoney { target });
    assert_eq!(w.player(player(1)).unwrap().inventory[0].count, 19);
    assert_eq!(w.player(player(1)).unwrap().next_item_instance, 2);
    assert!(!w.npc(2).unwrap().looted);
}

#[test]
fn stable_inventory_instances_merge_then_allocate_in_first_free_slots() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    rules.loot[0].items[0].count = 25;
    let mut w = quick_world(rules.clone());
    let genesis = w.snapshot_bytes().unwrap();
    approach(&mut w);
    let first = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target: first });
    ok(
        &mut w,
        Command::TakeLootItem {
            target: first,
            slot: 0,
        },
    );
    assert_eq!(
        w.player(player(1)).unwrap().inventory,
        vec![
            InventoryItem {
                instance: 1,
                slot: 0,
                item_entry: 920001,
                count: 20
            },
            InventoryItem {
                instance: 2,
                slot: 1,
                item_entry: 920001,
                count: 5
            },
        ]
    );
    let respawn = w.npc(2).unwrap().respawn_at.unwrap();
    wait(&mut w, respawn);
    let second = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target: second });
    ok(
        &mut w,
        Command::TakeLootItem {
            target: second,
            slot: 0,
        },
    );
    let p = w.player(player(1)).unwrap();
    assert_eq!(
        p.inventory,
        vec![
            InventoryItem {
                instance: 1,
                slot: 0,
                item_entry: 920001,
                count: 20
            },
            InventoryItem {
                instance: 2,
                slot: 1,
                item_entry: 920001,
                count: 20
            },
            InventoryItem {
                instance: 3,
                slot: 2,
                item_entry: 920001,
                count: 10
            },
        ]
    );
    assert_eq!(p.next_item_instance, 4);
    let replayed = World::replay(data(), rules, &genesis, &w.journal_bytes().unwrap()).unwrap();
    assert_eq!(
        replayed.snapshot_bytes().unwrap(),
        w.snapshot_bytes().unwrap()
    );
}

#[test]
fn instance_counter_exhaustion_is_atomic_but_existing_stack_can_still_merge() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    rules.loot[0].items[0].count = 2;
    let mut w = quick_world(rules.clone());
    approach(&mut w);
    let target = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target });
    let make = |count| {
        mutate_snapshot(&w, |s| {
            let p = &mut s.state.players[0];
            p.next_item_instance = u64::MAX;
            p.inventory = vec![InventoryItem {
                instance: 1,
                slot: 0,
                item_entry: 920001,
                count,
            }];
        })
    };
    let mut exhausted = World::restore(data(), rules.clone(), &make(19)).unwrap();
    let before = loot_effects(&exhausted);
    reject(
        &mut exhausted,
        Command::TakeLootItem { target, slot: 0 },
        Rejection::ProgressionLimit,
    );
    assert_eq!(loot_effects(&exhausted), before);
    reject(
        &mut exhausted,
        Command::Loot { target },
        Rejection::ProgressionLimit,
    );
    assert_eq!(loot_effects(&exhausted), before);
    let mut merge = World::restore(data(), rules, &make(18)).unwrap();
    ok(&mut merge, Command::TakeLootItem { target, slot: 0 });
    assert_eq!(merge.player(player(1)).unwrap().inventory[0].count, 20);
    assert_eq!(merge.player(player(1)).unwrap().inventory[0].instance, 1);
    assert_eq!(
        merge.player(player(1)).unwrap().next_item_instance,
        u64::MAX
    );
}

#[test]
fn leaving_loot_range_releases_once_and_explicit_release_is_safe_out_of_range() {
    let mut w = quick_world(Rules::original_camp());
    approach(&mut w);
    let target = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target });
    for _ in 0..3 {
        let result = ok(
            &mut w,
            Command::Move {
                delta: [-1000, 0, 0],
            },
        );
        assert!(result.events.is_empty());
        assert_eq!(w.player(player(1)).unwrap().active_loot, Some(target));
    }
    let result = ok(
        &mut w,
        Command::Move {
            delta: [-1000, 0, 0],
        },
    );
    assert_eq!(
        result.events,
        vec![Event::LootReleased {
            player: player(1),
            id: target
        }]
    );
    assert_eq!(w.player(player(1)).unwrap().active_loot, None);
    let before = loot_effects(&w);
    reject(
        &mut w,
        Command::TakeLootItem { target, slot: 0 },
        Rejection::OutOfRange,
    );
    reject(
        &mut w,
        Command::TakeLootMoney { target },
        Rejection::OutOfRange,
    );
    reject(&mut w, Command::OpenLoot { target }, Rejection::OutOfRange);
    assert!(ok(&mut w, Command::ReleaseLoot { target })
        .events
        .is_empty());
    assert!(ok(&mut w, Command::ReleaseLoot { target })
        .events
        .is_empty());
    assert_eq!(loot_effects(&w), before);
}

#[test]
fn respawn_releases_old_generation_and_stale_inputs_never_target_new_loot() {
    let mut w = quick_world(Rules::original_camp());
    approach(&mut w);
    let old = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target: old });
    let respawn = w.npc(2).unwrap().respawn_at.unwrap();
    wait(&mut w, respawn - 1);
    let result = send(&mut w, player(1), Command::TakeLootMoney { target: old });
    assert_eq!(result.outcomes[0].rejection, Some(Rejection::StaleTarget));
    assert_eq!(
        result.events[0],
        Event::LootReleased {
            player: player(1),
            id: old
        }
    );
    assert_eq!(w.player(player(1)).unwrap().active_loot, None);
    assert_eq!(w.npc(2).unwrap().loot_money, 0);
    assert!(w.npc(2).unwrap().loot_items.is_empty());
    let new = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target: new });
    let before = loot_effects(&w);
    assert!(ok(&mut w, Command::ReleaseLoot { target: old })
        .events
        .is_empty());
    reject(
        &mut w,
        Command::TakeLootItem {
            target: old,
            slot: 0,
        },
        Rejection::StaleTarget,
    );
    reject(
        &mut w,
        Command::OpenLoot { target: old },
        Rejection::StaleTarget,
    );
    assert_eq!(w.player(player(1)).unwrap().active_loot, Some(new));
    assert_eq!(loot_effects(&w), before);
}

#[test]
fn player_death_closes_loot_window_and_release_remains_idempotent() {
    let mut export = data().export().clone();
    let mut third = export.spawns[1].clone();
    third.spawn_id = 3;
    export.spawns.push(third);
    let custom = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    rules.player_health = 2;
    let mut w = World::new(custom.clone(), rules.clone(), WORLD, 42, vec![player(1)]).unwrap();
    approach(&mut w);
    let target = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target });
    let enemy = w.npc(3).unwrap().id;
    ok(
        &mut w,
        Command::Cast {
            ability: 930001,
            target: Target::Npc { id: enemy },
        },
    );
    let completion = w
        .player(player(1))
        .unwrap()
        .casting
        .as_ref()
        .unwrap()
        .completes_at;
    wait(&mut w, completion - 1);
    let result = w.step(completion, vec![]).unwrap();
    assert!(result
        .events
        .contains(&Event::PlayerDied { player: player(1) }));
    assert!(result.events.contains(&Event::LootReleased {
        player: player(1),
        id: target
    }));
    assert_eq!(w.player(player(1)).unwrap().active_loot, None);
    reject(&mut w, Command::TakeLootMoney { target }, Rejection::Dead);
    assert!(ok(&mut w, Command::ReleaseLoot { target })
        .events
        .is_empty());
    World::restore(custom, rules, &w.snapshot_bytes().unwrap()).unwrap();
}

#[test]
fn loot_and_inventory_snapshots_reject_impossible_state_with_recomputed_hashes() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    let mut w = quick_world(rules.clone());
    approach(&mut w);
    let target = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target });
    type Corruption = Box<dyn FnOnce(&mut Snapshot)>;
    let corruptions: Vec<Corruption> = vec![
        Box::new(|s| s.state.npcs[1].loot_money = 2),
        Box::new(|s| s.state.npcs[1].loot_items[0].as_mut().unwrap().count = 2),
        Box::new(|s| s.state.npcs[1].loot_items[0].as_mut().unwrap().item_entry = 1),
        Box::new(|s| s.state.npcs[1].loot_items.clear()),
        Box::new(|s| s.state.npcs[1].loot_items.push(None)),
        Box::new(|s| s.state.npcs[1].looted = true),
        Box::new(|s| s.state.players[0].active_loot.as_mut().unwrap().generation += 1),
        Box::new(|s| s.state.players[0].active_loot.as_mut().unwrap().spawn = 1),
        Box::new(|s| s.state.players[0].position.xyz = [-2000, 0, 0]),
        Box::new(|s| {
            s.state.players[0].health = 0;
            s.state.players[0].revive_at = Some(s.state.tick + 30);
        }),
        Box::new(|s| s.state.players[0].next_item_instance = 0),
    ];
    for change in corruptions {
        assert!(matches!(
            World::restore(data(), rules.clone(), &mutate_snapshot(&w, change)),
            Err(Error::InvalidSnapshot)
        ));
    }
    ok(&mut w, Command::TakeLootItem { target, slot: 0 });
    let corruptions: Vec<Corruption> = vec![
        Box::new(|s| s.state.players[0].inventory[0].instance = 0),
        Box::new(|s| s.state.players[0].inventory[0].instance = 2),
        Box::new(|s| s.state.players[0].inventory[0].slot = 16),
        Box::new(|s| {
            let item = s.state.players[0].inventory[0].clone();
            s.state.players[0].inventory.push(item);
        }),
        Box::new(|s| {
            let mut item = s.state.players[0].inventory[0].clone();
            item.slot = 1;
            s.state.players[0].inventory.push(item);
        }),
        Box::new(|s| {
            let mut item = s.state.players[0].inventory[0].clone();
            item.instance = 2;
            s.state.players[0].next_item_instance = 3;
            s.state.players[0].inventory.push(item);
        }),
    ];
    for change in corruptions {
        assert!(matches!(
            World::restore(data(), rules.clone(), &mutate_snapshot(&w, change)),
            Err(Error::InvalidSnapshot)
        ));
    }
    let alive = quick_world(rules.clone());
    let corrupt = mutate_snapshot(&alive, |s| s.state.npcs[1].loot_money = 1);
    assert!(matches!(
        World::restore(data(), rules.clone(), &corrupt),
        Err(Error::InvalidSnapshot)
    ));
    let corrupt = mutate_snapshot(&alive, |s| s.state.npcs[1].loot_items.push(None));
    assert!(matches!(
        World::restore(data(), rules.clone(), &corrupt),
        Err(Error::InvalidSnapshot)
    ));
}

#[test]
fn snapshot_format_two_rejects_version_one_and_legacy_loot_takes_only_remainder() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    let mut w = quick_world(rules.clone());
    assert_eq!(FORMAT_VERSION, 2);
    let old = mutate_snapshot(&w, |s| s.format_version = 1);
    assert!(matches!(
        World::restore(data(), rules, &old),
        Err(Error::Incompatible)
    ));
    approach(&mut w);
    let target = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target });
    ok(&mut w, Command::TakeLootMoney { target });
    let result = ok(&mut w, Command::Loot { target });
    assert_eq!(
        result.events,
        vec![Event::Looted {
            player: player(1),
            id: target
        }]
    );
    assert_eq!(w.player(player(1)).unwrap().money, 1);
    assert_eq!(w.player(player(1)).unwrap().inventory[0].count, 1);
    assert_eq!(w.player(player(1)).unwrap().next_item_instance, 2);
    let before = loot_effects(&w);
    reject(&mut w, Command::Loot { target }, Rejection::AlreadyLooted);
    assert_eq!(loot_effects(&w), before);
}

#[test]
fn switching_loot_windows_releases_old_and_failed_open_preserves_current() {
    let mut export = data().export().clone();
    let mut third = export.spawns[1].clone();
    third.spawn_id = 3;
    export.spawns.push(third);
    let custom = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    let mut w = World::new(custom.clone(), rules.clone(), WORLD, 42, vec![player(1)]).unwrap();
    let genesis = w.snapshot_bytes().unwrap();
    approach(&mut w);
    let first = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target: first });
    let second = w.npc(3).unwrap().id;
    reject(
        &mut w,
        Command::OpenLoot { target: second },
        Rejection::Alive,
    );
    assert_eq!(w.player(player(1)).unwrap().active_loot, Some(first));
    let ready = w.player(player(1)).unwrap().melee_ready_at;
    wait(&mut w, ready - 1);
    ok(&mut w, Command::Melee { target: second });
    let result = ok(&mut w, Command::OpenLoot { target: second });
    assert_eq!(
        result.events,
        vec![
            Event::LootReleased {
                player: player(1),
                id: first
            },
            Event::LootOpened {
                player: player(1),
                id: second
            },
        ]
    );
    reject(
        &mut w,
        Command::TakeLootItem {
            target: first,
            slot: 0,
        },
        Rejection::LootNotOpen,
    );
    assert!(ok(&mut w, Command::ReleaseLoot { target: first })
        .events
        .is_empty());
    assert_eq!(w.player(player(1)).unwrap().active_loot, Some(second));
    ok(
        &mut w,
        Command::TakeLootItem {
            target: second,
            slot: 0,
        },
    );
    let replayed = World::replay(custom, rules, &genesis, &w.journal_bytes().unwrap()).unwrap();
    assert_eq!(
        w.snapshot_bytes().unwrap(),
        replayed.snapshot_bytes().unwrap()
    );
}

#[test]
fn money_limit_failures_preserve_all_remaining_rewards_and_item_allocator() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    let mut w = quick_world(rules.clone());
    approach(&mut w);
    let target = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target });
    let saved = mutate_snapshot(&w, |s| s.state.players[0].money = MAX_PROGRESSION);
    let mut w = World::restore(data(), rules, &saved).unwrap();
    let before = loot_effects(&w);
    reject(
        &mut w,
        Command::TakeLootMoney { target },
        Rejection::ProgressionLimit,
    );
    assert_eq!(loot_effects(&w), before);
    reject(
        &mut w,
        Command::Loot { target },
        Rejection::ProgressionLimit,
    );
    assert_eq!(loot_effects(&w), before);
    ok(&mut w, Command::TakeLootItem { target, slot: 0 });
    assert_eq!(w.player(player(1)).unwrap().money, MAX_PROGRESSION);
    assert_eq!(w.npc(2).unwrap().loot_money, 1);
    assert!(!w.npc(2).unwrap().looted);
}

#[test]
fn bag_allocation_fills_first_free_slot_without_reusing_instances() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    let mut w = quick_world(rules.clone());
    approach(&mut w);
    let target = quick_kill(&mut w);
    ok(&mut w, Command::OpenLoot { target });
    let saved = mutate_snapshot(&w, |s| {
        let p = &mut s.state.players[0];
        p.inventory = vec![InventoryItem {
            instance: 7,
            slot: 3,
            item_entry: 920001,
            count: 20,
        }];
        p.next_item_instance = 8;
    });
    let mut w = World::restore(data(), rules, &saved).unwrap();
    ok(&mut w, Command::TakeLootItem { target, slot: 0 });
    assert_eq!(
        w.player(player(1)).unwrap().inventory,
        vec![
            InventoryItem {
                instance: 8,
                slot: 0,
                item_entry: 920001,
                count: 1
            },
            InventoryItem {
                instance: 7,
                slot: 3,
                item_entry: 920001,
                count: 20
            },
        ]
    );
    assert_eq!(w.player(player(1)).unwrap().next_item_instance, 9);
}

#[test]
fn no_drop_death_has_exhausted_loot_without_creating_rewards() {
    let mut rules = Rules::original_camp();
    rules.melee_damage = 30;
    rules.loot.clear();
    let mut w = quick_world(rules.clone());
    approach(&mut w);
    let target = quick_kill(&mut w);
    assert!(w.npc(2).unwrap().looted);
    ok(&mut w, Command::OpenLoot { target });
    reject(
        &mut w,
        Command::TakeLootMoney { target },
        Rejection::LootMoneyEmpty,
    );
    reject(
        &mut w,
        Command::TakeLootItem { target, slot: 0 },
        Rejection::InvalidLootSlot,
    );
    reject(&mut w, Command::Loot { target }, Rejection::AlreadyLooted);
    assert_eq!(w.player(player(1)).unwrap().next_item_instance, 1);
    World::restore(data(), rules, &w.snapshot_bytes().unwrap()).unwrap();
}
