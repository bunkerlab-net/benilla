// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::{encounter::*, xp::Level};

// Invented test content. These IDs/stats are not game database records.
const NPC: EntityId = 900_000_001;
const ITEM: u32 = 900_000_002;
fn player(peer: PeerId) -> PlayerSpec {
    PlayerSpec {
        peer,
        level: Level::new(10).unwrap(),
        max_health: 100,
        damage: 6,
        swing_ticks: 1,
        respawn_ticks: 2,
    }
}
fn creature() -> CreatureSpec {
    CreatureSpec {
        entity: NPC,
        level: Level::new(10).unwrap(),
        max_health: 10,
        damage: 1,
        swing_ticks: 1,
        respawn_ticks: 2,
        loot: Some(LootSpec {
            item: ITEM,
            quantity: 1,
        }),
    }
}
fn rules() -> Rules {
    Rules {
        tick_ms: 50,
        xp_milli: 1000,
    }
}
fn world() -> World {
    World::new(
        rules(),
        Content {
            creatures: vec![creature()],
        },
        vec![player(1), player(2)],
    )
    .unwrap()
}
fn frame(tick: u64, peer: PeerId, attack: bool) -> InputFrame {
    InputFrame {
        tick,
        peer,
        commands: if attack {
            vec![Command::Attack(NPC)]
        } else {
            vec![]
        },
    }
}
fn advance(world: &mut World, attack: bool, reverse: bool) -> CommittedTick {
    let tick = world.tick();
    for peer in if reverse { [2, 1] } else { [1, 2] } {
        world.submit(frame(tick, peer, attack)).unwrap();
    }
    match world.step().unwrap() {
        StepOutcome::Advanced(result) => result,
        _ => panic!("All peers supplied input"),
    }
}

#[test]
fn roster_barrier_never_advances_on_missing_or_duplicate_input() {
    let mut world = world();
    let initial = world.state_hash();
    assert_eq!(world.submit(frame(0, 1, true)), Ok(Submission::Accepted));
    assert_eq!(world.submit(frame(0, 1, true)), Ok(Submission::Duplicate));
    for _ in 0..10 {
        assert_eq!(
            world.step().unwrap(),
            StepOutcome::Waiting { missing: vec![2] }
        );
        assert_eq!(world.tick(), 0);
        assert_eq!(world.state_hash(), initial);
    }
    assert_eq!(world.snapshot(), Err(Error::PendingInputs));
    assert_eq!(
        world.submit(frame(0, 1, false)),
        Err(Error::ConflictingInput)
    );
    world.submit(frame(0, 2, true)).unwrap();
    assert!(matches!(world.step().unwrap(), StepOutcome::Advanced(_)));
    assert_eq!(
        world.submit(frame(0, 2, true)),
        Err(Error::WrongTick {
            expected: 1,
            received: 0
        })
    );
}

#[test]
fn two_peers_converge_despite_opposite_arrival_and_roster_order() {
    let mut a = world();
    let mut b = World::new(
        rules(),
        Content {
            creatures: vec![creature()],
        },
        vec![player(2), player(1)],
    )
    .unwrap();
    let initial = a.snapshot().unwrap();
    assert_eq!(initial, b.snapshot().unwrap());
    let mut log = Vec::new();
    for tick in 0..50 {
        let left = advance(&mut a, tick % 2 == 0, false);
        let right = advance(&mut b, tick % 2 == 0, true);
        assert_eq!(left, right);
        assert_eq!(
            a.snapshot().unwrap().canonical_bytes(),
            b.snapshot().unwrap().canonical_bytes()
        );
        log.push(left);
    }
    let replayed = World::replay(initial, &log).unwrap();
    assert_eq!(replayed.snapshot().unwrap(), a.snapshot().unwrap());
}

#[test]
fn death_loot_and_xp_commit_once_per_spawn_generation() {
    let mut world = world();
    let killed = advance(&mut world, true, true);
    assert_eq!(world.creatures()[&NPC].health, 0);
    assert_eq!(world.players()[&1].xp, 0);
    assert_eq!(world.players()[&2].xp, 95); // Stable peer-order winner, not arrival-order winner.
    assert_eq!(world.players()[&2].inventory[&ITEM], 1);
    assert_eq!(
        killed
            .events
            .iter()
            .filter(|e| matches!(e, Event::Reward { generation: 0, .. }))
            .count(),
        1
    );
    let dead = advance(&mut world, true, false);
    assert!(!dead
        .events
        .iter()
        .any(|e| matches!(e, Event::Reward { .. })));
    assert_eq!(world.players()[&2].xp, 95);
    let alive = advance(&mut world, false, false);
    assert!(alive.events.contains(&Event::CreatureRespawned {
        entity: NPC,
        generation: 1
    }));
    assert_eq!(world.creatures()[&NPC].health, 10);
    let killed_again = advance(&mut world, true, false);
    assert_eq!(
        killed_again
            .events
            .iter()
            .filter(|e| matches!(e, Event::Reward { generation: 1, .. }))
            .count(),
        1
    );
    assert_eq!(world.players()[&2].xp, 190);
    assert_eq!(world.players()[&2].inventory[&ITEM], 2);
}

#[test]
fn player_death_and_respawn_are_logical_tick_events() {
    let mut spec = creature();
    spec.max_health = 1000;
    spec.damage = 100;
    let mut world = World::new(
        rules(),
        Content {
            creatures: vec![spec],
        },
        vec![player(1), player(2)],
    )
    .unwrap();
    let killed = advance(&mut world, true, false);
    assert!(killed.events.contains(&Event::PlayerDied { peer: 1 }));
    assert_eq!(world.players()[&1].health, 0);
    advance(&mut world, false, false);
    let respawned = advance(&mut world, false, false);
    assert!(respawned
        .events
        .contains(&Event::PlayerRespawned { peer: 1 }));
    assert_eq!(world.players()[&1].health, 100);
    assert_eq!(world.players()[&1].target, None);
}

#[test]
fn snapshot_resume_does_not_repeat_rewards() {
    let mut original = world();
    advance(&mut original, true, false);
    let mut restored = World::from_snapshot(original.snapshot().unwrap());
    for attack in [false, false, true, false] {
        assert_eq!(
            advance(&mut original, attack, false),
            advance(&mut restored, attack, true)
        );
    }
    assert_eq!(original.players()[&2].xp, 190);
}

#[test]
fn replay_detects_missing_input_hash_and_event_tampering() {
    let mut world = world();
    let snapshot = world.snapshot().unwrap();
    let valid = advance(&mut world, true, false);
    let mut bad = valid.clone();
    bad.state_hash[0] ^= 1;
    assert!(matches!(
        World::replay(snapshot.clone(), &[bad]),
        Err(Error::ReplayMismatch { tick: 0 })
    ));
    let mut bad = valid.clone();
    bad.events.clear();
    assert!(matches!(
        World::replay(snapshot.clone(), &[bad]),
        Err(Error::ReplayMismatch { tick: 0 })
    ));
    let mut bad = valid;
    bad.inputs.pop();
    assert!(matches!(
        World::replay(snapshot, &[bad]),
        Err(Error::IncompleteReplay { tick: 0 })
    ));
}

#[test]
fn invalid_input_does_not_mutate_committed_state() {
    let mut world = world();
    let hash = world.state_hash();
    assert_eq!(world.submit(frame(0, 99, false)), Err(Error::UnknownPeer));
    assert_eq!(
        world.submit(frame(1, 1, false)),
        Err(Error::WrongTick {
            expected: 0,
            received: 1
        })
    );
    assert_eq!(
        world.submit(InputFrame {
            tick: 0,
            peer: 1,
            commands: vec![Command::Attack(7)]
        }),
        Err(Error::UnknownTarget)
    );
    assert_eq!(
        world.submit(InputFrame {
            tick: 0,
            peer: 1,
            commands: vec![Command::StopAttack; MAX_COMMANDS_PER_FRAME + 1]
        }),
        Err(Error::TooManyCommands)
    );
    assert_eq!(world.state_hash(), hash);
    assert!(world.snapshot().is_ok());
}

#[test]
fn rules_and_content_are_hashed_and_validated() {
    let default = world();
    let mut changed_rules = rules();
    changed_rules.xp_milli = 2000;
    let changed = World::new(
        changed_rules,
        Content {
            creatures: vec![creature()],
        },
        vec![player(1), player(2)],
    )
    .unwrap();
    assert_ne!(default.state_hash(), changed.state_hash());
    let mut changed_content = creature();
    changed_content.damage = 2;
    let changed = World::new(
        rules(),
        Content {
            creatures: vec![changed_content],
        },
        vec![player(1), player(2)],
    )
    .unwrap();
    assert_ne!(default.state_hash(), changed.state_hash());
    assert!(matches!(
        World::new(rules(), Content { creatures: vec![] }, vec![]),
        Err(Error::InvalidRoster)
    ));
    assert!(matches!(
        World::new(
            rules(),
            Content { creatures: vec![] },
            vec![player(1), player(1)]
        ),
        Err(Error::InvalidRoster)
    ));
    assert!(matches!(
        World::new(
            rules(),
            Content {
                creatures: vec![creature(), creature()]
            },
            vec![player(1)]
        ),
        Err(Error::InvalidContent)
    ));
    let mut invalid = rules();
    invalid.tick_ms = 0;
    assert!(matches!(
        World::new(invalid, Content { creatures: vec![] }, vec![player(1)]),
        Err(Error::InvalidRules)
    ));
}

#[test]
fn stop_attack_and_cooldown_cannot_be_reset_by_command_spam() {
    let mut spec = creature();
    spec.max_health = 1000;
    let mut p = player(1);
    p.swing_ticks = 3;
    let mut world = World::new(
        rules(),
        Content {
            creatures: vec![spec],
        },
        vec![p, player(2)],
    )
    .unwrap();
    for tick in 0..3 {
        world.submit(frame(tick, 2, false)).unwrap();
        world
            .submit(InputFrame {
                tick,
                peer: 1,
                commands: vec![Command::Attack(NPC); 16],
            })
            .unwrap();
        world.step().unwrap();
    }
    assert_eq!(world.creatures()[&NPC].health, 994);
    world
        .submit(InputFrame {
            tick: 3,
            peer: 1,
            commands: vec![Command::Attack(NPC), Command::StopAttack],
        })
        .unwrap();
    world.submit(frame(3, 2, false)).unwrap();
    world.step().unwrap();
    assert_eq!(world.creatures()[&NPC].health, 994);
}

#[test]
fn xp_rate_is_applied_before_final_rounding() {
    let mut r = rules();
    r.xp_milli = 1500;
    let mut world = World::new(
        r,
        Content {
            creatures: vec![creature()],
        },
        vec![player(1), player(2)],
    )
    .unwrap();
    advance(&mut world, true, false);
    assert_eq!(world.players()[&2].xp, 142); // 95 * 1.5 = 142.5, ties to even.
}

#[test]
fn dead_spawn_clears_long_cooldown_attackers() {
    let mut p1 = player(1);
    p1.swing_ticks = 100;
    let mut world = World::new(
        rules(),
        Content {
            creatures: vec![creature()],
        },
        vec![p1, player(2)],
    )
    .unwrap();
    advance(&mut world, true, false);
    assert!(world.players().values().all(|p| p.target.is_none()));
    advance(&mut world, false, false);
    advance(&mut world, false, false);
    assert_eq!(world.creatures()[&NPC].health, 10);
}

#[test]
fn canonical_digest_golden() {
    let mut world = world();
    let first = advance(&mut world, true, false);
    let hex: String = first
        .state_hash
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        hex,
        "cbb0ec969c06f3aca84c2cf90b55d439f52b31817d7501085ae109154a4416ee"
    );
}
