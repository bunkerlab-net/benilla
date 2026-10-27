// SPDX-License-Identifier: GPL-3.0-or-later
//! Fresh reconstruction tests using authored rows, never an admitted Classic zone.
use super::synthetic_loot_fixture::{setup, LootAuthority};
use super::synthetic_quest_fixture::command;
use super::*;
use benilla_sim::{inventory as inv, quest_lifecycle as q};
fn world_from(
    p: Profile,
    input: benilla_sim::starter_init::StarterInput,
    data: QuestProgressionInput,
) -> NativeWorld {
    let mut p = p;
    p.initialize_quest_progression(&input, data).unwrap();
    NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42).unwrap()
}
fn fixed_setup() -> (
    Profile,
    benilla_sim::starter_init::StarterInput,
    QuestProgressionInput,
) {
    let (p, i, mut q) = setup();
    let l = q.loot.as_mut().unwrap();
    l.captures[0].rows[0].chance_or_quest_chance_bits = 100f32.to_bits();
    l.captures[0].rows[0].min_count_or_reference = 1;
    l.captures[0].rows[0].max_count = 1;
    (p, i, q)
}
fn world() -> NativeWorld {
    let (p, i, q) = fixed_setup();
    world_from(p, i, q)
}
fn step(w: &mut NativeWorld, a: &mut impl Authority, c: Command) {
    let i = command(w, c);
    w.step(w.state.tick + 1, Some(i), a).unwrap();
}
fn kill(w: &mut NativeWorld, a: &mut impl Authority) {
    let target = w.state.npcs[&2].id;
    step(
        w,
        a,
        Command::Cast {
            ability: 60001,
            target,
        },
    );
    for _ in 0..30 {
        if w.state.npcs[&2].health == 0 {
            return;
        }
        w.step(w.state.tick + 1, None, a).unwrap();
    }
    panic!("authored target did not die")
}
fn roundtrip(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
fn progress(w: &NativeWorld) -> &QuestProgressionState {
    w.state.quest_progression.as_ref().unwrap()
}
fn inventory(w: &NativeWorld) -> &inv::State {
    progress(w).inventory.as_ref().unwrap()
}
#[test]
fn corpse_items_are_not_kill_rewards_collect_once_checkpoint_and_replay() {
    let mut w = world();
    let start = w.checkpoint().unwrap();
    let mut a = LootAuthority::default();
    let money = progress(&w).money;
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 0);
    assert_eq!(progress(&w).money, money);
    let c = w.state.npcs[&2].corpse_loot.as_ref().unwrap();
    assert_eq!(c.owner, w.state.player);
    assert_eq!(c.items().count(), 1);
    let gold = c.money();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), start).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    step(
        &mut w,
        &mut a,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 1);
    step(&mut w, &mut a, Command::LootMoney { corpse: id });
    assert_eq!(progress(&w).money, money + gold);
    assert!(progress(&w).loot_target.is_none());
    w = roundtrip(&w);
    let after = inventory(&w).clone();
    step(
        &mut w,
        &mut a,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    step(&mut w, &mut a, Command::LootMoney { corpse: id });
    assert_eq!(&after, inventory(&w));
    assert_eq!(progress(&w).money, money + gold);
}
#[test]
fn source_release_distinguishes_explicit_missing_slot_and_automatic_response() {
    let (p, i, mut q) = fixed_setup();
    for r in &mut q.loot.as_mut().unwrap().creatures {
        r.minimum_money = 0;
        r.maximum_money = 0;
    }
    let mut w = world_from(p, i, q);
    let mut a = LootAuthority::default();
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    step(&mut w, &mut a, Command::ReleaseLoot { corpse: id });
    assert!(matches!(
        w.last_tick().unwrap().events[1],
        Event::Loot(LootEvent::Released {
            send_response: false,
            ..
        })
    ));
    step(&mut w, &mut a, Command::ReleaseLoot { corpse: id });
    assert!(!w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::Loot(_))));
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    step(
        &mut w,
        &mut a,
        Command::LootItem {
            corpse: id,
            slot: 99,
        },
    );
    let events = &w.last_tick().unwrap().events;
    let error = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::Loot(LootEvent::StorageFailed {
                    result: inv::InventoryResult::ItemNotFound,
                    ..
                })
            )
        })
        .unwrap();
    assert!(matches!(
        events[error + 1],
        Event::Loot(LootEvent::Released {
            send_response: false,
            ..
        })
    ));
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    step(
        &mut w,
        &mut a,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::Loot(LootEvent::Released {
            send_response: true,
            ..
        })
    )));
    assert!(!w.state.npcs[&2].corpse_loot.as_ref().unwrap().looted);
    roundtrip(&w);
}
#[test]
fn source_empty_looted_release_does_not_reapply_early_timer_shortening() {
    let (mut p, i, mut q) = fixed_setup();
    p.npcs[0].ai.corpse_decay_ms = 20_000;
    let r = &mut q.loot.as_mut().unwrap().creatures[0];
    r.loot_id = 0;
    r.minimum_money = 0;
    r.maximum_money = 0;
    r.corpse_acceleration_decay_ms = 300;
    let mut w = world_from(p, i, q);
    let mut a = LootAuthority::default();
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    let expiry = w.state.npcs[&2].ai.corpse_expiration_ms();
    assert!(w.state.npcs[&2].corpse_loot.as_ref().unwrap().looted);
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    step(&mut w, &mut a, Command::ReleaseLoot { corpse: id });
    assert_eq!(expiry, w.state.npcs[&2].ai.corpse_expiration_ms());
    assert!(w.state.npcs[&2].ai.corpse_timer_calls().is_empty());
    roundtrip(&w);
}
#[test]
fn collection_at_exact_expiry_precedes_corpse_removal_and_restores() {
    let (mut p, i, mut q) = fixed_setup();
    p.npcs[0].ai.corpse_decay_ms = 500;
    q.loot.as_mut().unwrap().creatures[0].corpse_acceleration_decay_ms = 500;
    let mut w = world_from(p, i, q);
    let mut a = LootAuthority::default();
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    step(
        &mut w,
        &mut a,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    let expiry = w.state.npcs[&2].ai.corpse_expiration_ms();
    while w.state.now_ms + u64::from(w.profile.tick_ms) < expiry {
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    step(&mut w, &mut a, Command::LootMoney { corpse: id });
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Despawned);
    roundtrip(&w);
}
#[test]
fn item_callbacks_precede_store_and_requirement_destruction_precedes_rewards() {
    let (p, i, mut q) = fixed_setup();
    let quest = &mut q.quests[0];
    quest.required_creature = [0; 4];
    quest.required_creature_count = [0; 4];
    quest.required_item = [70001, 0, 0, 0];
    quest.required_item_count = [1, 0, 0, 0];
    quest.special_flags = q::DELIVER;
    quest.excluded.reward_item = [70001, 0, 0, 0];
    quest.excluded.reward_item_count = [1, 0, 0, 0];
    let mut w = world_from(p, i, q);
    let mut a = LootAuthority::default();
    let giver = w.state.npcs[&3].id;
    step(
        &mut w,
        &mut a,
        Command::AcceptQuest {
            quest: 70001,
            giver,
        },
    );
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    step(
        &mut w,
        &mut a,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    let events = &w.last_tick().unwrap().events;
    let credit = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::Quest {
                    event: q::Event::ItemCredit { .. },
                    ..
                })
            )
        })
        .unwrap();
    let stored = events
        .iter()
        .position(|e| matches!(e, Event::InventoryStored(_)))
        .unwrap();
    assert!(credit < stored);
    step(
        &mut w,
        &mut a,
        Command::TurnInQuest {
            quest: 70001,
            giver,
        },
    );
    let events = &w.last_tick().unwrap().events;
    let removed = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::Quest {
                    event: q::Event::Host(q::HostEffect::DestroyCarriedStack { .. }),
                    ..
                })
            )
        })
        .unwrap();
    let reward = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::RewardItem(_))
            )
        })
        .unwrap();
    let xp = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::Progression(_))
            )
        })
        .unwrap();
    assert!(removed < reward && reward < xp);
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 1);
    assert!(progress(&w).quests.progress(70001).unwrap().rewarded);
    roundtrip(&w);
}
#[test]
fn wrong_owner_generation_or_authority_cannot_change_inventory() {
    let mut w = world();
    let mut a = LootAuthority::default();
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    let before = w.checkpoint().unwrap();
    a.wrong = true;
    let i = command(&w, Command::OpenLoot { corpse: id });
    assert_eq!(
        w.step(w.state.tick + 1, Some(i), &mut a),
        Err(NativeError::AuthorityBinding)
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    a.wrong = false;
    let mut i = command(&w, Command::OpenLoot { corpse: id });
    i.authenticated_owner = [9; 16];
    assert_eq!(
        w.step(w.state.tick + 1, Some(i), &mut a),
        Err(NativeError::InvalidInput)
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    let old = crate::NpcId {
        spawn: id.spawn,
        generation: id.generation + 1,
    };
    step(&mut w, &mut a, Command::OpenLoot { corpse: old });
    assert!(progress(&w).loot_target.is_none());
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 0);
}
#[test]
fn allocation_failure_rolls_back_entire_collection_frame_and_sequence() {
    let mut w = world();
    let mut a = LootAuthority::default();
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    w.state
        .quest_progression
        .as_mut()
        .unwrap()
        .inventory
        .as_mut()
        .unwrap()
        .next_item = u64::MAX;
    let before = w.checkpoint().unwrap();
    let i = command(
        &w,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    assert!(w.step(w.state.tick + 1, Some(i), &mut a).is_err());
    assert_eq!(before, w.checkpoint().unwrap());
}

#[test]
fn exhausted_nonempty_reopen_releases_without_inspection_or_timer_extension() {
    let (p, i, mut q) = fixed_setup();
    for row in &mut q.loot.as_mut().unwrap().creatures {
        row.minimum_money = 0;
        row.maximum_money = 0;
    }
    let mut w = world_from(p, i, q);
    let mut a = LootAuthority::default();
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    step(
        &mut w,
        &mut a,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    let deadline = w.state.npcs[&2].ai.corpse_expiration_ms();
    let timer = w.state.npcs[&2].ai.corpse_timer_calls().to_vec();
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    assert_eq!(w.state.npcs[&2].ai.corpse_expiration_ms(), deadline);
    assert_eq!(w.state.npcs[&2].ai.corpse_timer_calls(), timer);
    assert!(progress(&w).loot_target.is_none());
    assert_eq!(
        w.last_tick()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Loot(e) => Some(e.clone()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![LootEvent::Released {
            corpse: id,
            send_response: true
        }]
    );
    roundtrip(&w);
}
#[test]
fn current_item_receipts_reject_resigned_same_id_count_entry_and_slot_mutations() {
    let (p, i, mut data) = fixed_setup();
    let loot = data.loot.as_mut().unwrap();
    let mut rows = loot.inventory.items().cloned().collect::<Vec<_>>();
    rows.push(synthetic_loot_fixture::item(70003));
    loot.inventory = synthetic_loot_fixture::inventory_catalog(rows);
    let mut w = world_from(p, i, data);
    let mut a = LootAuthority::default();
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    step(
        &mut w,
        &mut a,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    let checkpoint = w.checkpoint().unwrap();
    let slot = inventory(&w)
        .slots
        .iter()
        .find(|(_, item)| item.item == 70001)
        .unwrap()
        .0
        .to_string();
    for mutation in 0..3 {
        let mut forged = serde_json::to_value(&checkpoint).unwrap();
        let slots = forged["state"]["quest_progression"]["inventory"]["slots"]
            .as_object_mut()
            .unwrap();
        match mutation {
            0 => slots.get_mut(&slot).unwrap()["count"] = serde_json::json!(2),
            1 => slots.get_mut(&slot).unwrap()["item"] = serde_json::json!(70003),
            _ => {
                let item = slots.remove(&slot).unwrap();
                slots.insert("38".into(), item);
            }
        }
        let mut forged: Checkpoint = serde_json::from_value(forged).unwrap();
        forged.digest = crate::digest(b"benilla-native-state-v52\0", &forged.state);
        forged.checksum = crate::digest(
            b"benilla-native-checkpoint-v52\0",
            &(&forged.state, forged.digest, forged.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), forged).is_err(),
            "mutation {mutation}"
        );
    }
    w.checkpoint_committed(&checkpoint).unwrap();
    assert!(w.records().is_empty());
    assert_eq!(
        progress(&roundtrip(&w)).inventory_receipts,
        progress(&w).inventory_receipts
    );
}

#[test]
fn queued_explicit_release_after_item_last_still_transitions_looted_once() {
    let (mut p, i, mut q) = fixed_setup();
    p.npcs[0].ai.corpse_decay_ms = 20_000;
    for row in &mut q.loot.as_mut().unwrap().creatures {
        row.minimum_money = 0;
        row.maximum_money = 0;
        row.corpse_acceleration_decay_ms = 500;
    }
    let mut w = world_from(p, i, q);
    let mut a = LootAuthority::default();
    kill(&mut w, &mut a);
    let id = w.state.npcs[&2].id;
    step(&mut w, &mut a, Command::OpenLoot { corpse: id });
    let first = command(
        &w,
        Command::LootItem {
            corpse: id,
            slot: 0,
        },
    );
    let mut second = command(&w, Command::ReleaseLoot { corpse: id });
    second.input.sequence += 1;
    w.step_batch(w.state.tick + 1, vec![first, second], &mut a)
        .unwrap();
    assert!(w.state.npcs[&2].corpse_loot.as_ref().unwrap().looted);
    let deadline = w.state.npcs[&2].ai.corpse_expiration_ms();
    assert_eq!(deadline, w.state.now_ms + 500);
    assert_eq!(
        w.last_tick()
            .unwrap()
            .events
            .iter()
            .filter(|e| matches!(e, Event::Loot(LootEvent::Released { .. })))
            .count(),
        1
    );
    step(&mut w, &mut a, Command::ReleaseLoot { corpse: id });
    assert_eq!(deadline, w.state.npcs[&2].ai.corpse_expiration_ms());
    assert!(!w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::Loot(_))));
    roundtrip(&w);
}

#[path = "loot_tests/corpse_carried.rs"]
mod corpse_carried;

#[path = "loot_tests/reference_loot.rs"]
mod reference_loot;
