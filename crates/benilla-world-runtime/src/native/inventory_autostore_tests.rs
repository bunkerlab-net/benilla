// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual native inventory transactions; authored content only.
use super::*;
fn autostore(source: u8) -> Command {
    Command::InventoryAutoStore(InventoryAutoStoreRequest { source })
}
fn prepare(w: &mut NativeWorld) {
    grant(w, 1, 70001, 7);
    step(
        w,
        Command::InventorySplit(InventorySplitRequest {
            source: 23,
            destination: 38,
            count: 2,
        }),
    );
}
fn deliver_world() -> NativeWorld {
    let (mut p, input, mut data) = fixture::item_grant_setup();
    let row = &mut data.quests[0];
    row.special_flags = benilla_sim::quest_lifecycle::DELIVER;
    row.required_creature = [0; 4];
    row.required_creature_count = [0; 4];
    row.required_item = [70001, 0, 0, 0];
    row.required_item_count = [7, 0, 0, 0];
    p.initialize_quest_progression(&input, data).unwrap();
    p.admin = world().profile().admin.clone();
    NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42).unwrap()
}
#[test]
fn real_grant_split_autostore_preserves_quest_state_allocator_and_replays() {
    let mut w = deliver_world();
    let initial = w.checkpoint().unwrap();
    let giver = w.state.npcs[&3].id;
    step(
        &mut w,
        Command::AcceptQuest {
            quest: 70001,
            giver,
        },
    );
    prepare(&mut w);
    assert_eq!(
        w.state
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .progress(70001)
            .unwrap()
            .status,
        benilla_sim::quest_lifecycle::Status::Complete
    );
    let quests = w.state.quest_progression.as_ref().unwrap().quests.clone();
    let allocator = inventory(&w).next_item;
    let target = inventory(&w).slots[&23].id;
    let source = inventory(&w).slots[&38].id;
    step(&mut w, autostore(38));
    assert_eq!(inventory(&w).slots[&23].id, target);
    assert_eq!(inventory(&w).slots[&23].count, 5);
    assert!(inventory(&w).slots.values().all(|i| i.id != source));
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 7);
    assert_eq!(inventory(&w).next_item, allocator);
    assert_eq!(w.state.quest_progression.as_ref().unwrap().quests, quests);
    assert!(!w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::QuestProgression(_) | Event::ItemAcquired(_) | Event::InventoryStored(_)
    )));
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::InventoryAutoStored(_))));
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn empty_and_unchanged_autostore_keep_inventory_and_allocator() {
    let mut w = world();
    grant(&mut w, 1, 70001, 3);
    for source in [38, 23] {
        let before = inventory(&w).clone();
        step(&mut w, autostore(source));
        assert_eq!(inventory(&w), &before);
        assert_eq!(restore(&w).state_digest(), w.state_digest());
    }
}
#[test]
fn later_invalid_input_and_receipt_tamper_cannot_commit_autostore() {
    let mut w = world();
    prepare(&mut w);
    let before = w.checkpoint().unwrap();
    let a = command(&w, autostore(38));
    let mut b = command(&w, autostore(39));
    b.input.sequence += 1;
    assert!(w
        .step_batch(
            w.state.tick + 1,
            vec![a, b],
            &mut fixture::LootAuthority::default()
        )
        .is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    step(&mut w, autostore(38));
    for fault in 0..4 {
        let mut record = w.last_tick().unwrap().clone();
        let r = record
            .events
            .iter_mut()
            .find_map(|e| match e {
                Event::InventoryAutoStored(r) => Some(r),
                _ => None,
            })
            .unwrap();
        match fault {
            0 => r.source = 24,
            1 => r.after.next_item += 1,
            2 => {
                r.views[0].slots.insert(38, r.before.slots[&38].clone());
            }
            _ => r.destinations[0].count += 1,
        }
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), before.clone())
                .unwrap();
        assert!(replay.replay(&[record]).is_err());
        assert_eq!(replay.checkpoint().unwrap(), before);
    }
}
#[test]
fn real_lethal_damage_does_not_add_an_alive_guard_to_backpack_autostore() {
    use benilla_sim::player_death as d;
    let mut p = world().profile().clone();
    p.npcs[0].max_health = 3000;
    p.npcs[0].combat.as_mut().unwrap().minimum_damage = synthetic_fixture::float(10000.0);
    p.npcs[0].combat.as_mut().unwrap().maximum_damage = synthetic_fixture::float(10000.0);
    p.configure_player_death(
        d::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|x| (x, Some(false)))).unwrap(),
    )
    .unwrap();
    let mut w =
        NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 7).unwrap();
    let initial = w.checkpoint().unwrap();
    prepare(&mut w);
    let target = w.state.npcs[&2].id;
    step(
        &mut w,
        Command::Cast {
            ability: 60001,
            target,
        },
    );
    for _ in 0..100 {
        if !w.state.player_alive() {
            break;
        }
        w.step(
            w.state.tick + 1,
            None,
            &mut fixture::LootAuthority::default(),
        )
        .unwrap();
    }
    assert_eq!(w.state.health, 0);
    assert!(!w
        .state
        .combat
        .as_ref()
        .unwrap()
        .death
        .as_ref()
        .unwrap()
        .is_alive());
    let allocator = inventory(&w).next_item;
    step(&mut w, autostore(38));
    assert_eq!(inventory(&w).slots[&23].count, 5);
    assert!(!inventory(&w).slots.contains_key(&38));
    assert_eq!(inventory(&w).next_item, allocator);
    assert!(!w.state.player_alive());
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
