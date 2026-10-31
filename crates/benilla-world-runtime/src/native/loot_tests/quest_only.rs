// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored negative body leaves use existing quest callbacks and inventory transactions.
use super::*;

fn setup_quest() -> (
    Profile,
    benilla_sim::starter_init::StarterInput,
    QuestProgressionInput,
) {
    let (p, input, mut rows) = fixed_setup();
    let quest = &mut rows.quests[0];
    quest.special_flags = q::DELIVER;
    quest.required_creature = [0; 4];
    quest.required_creature_count = [0; 4];
    quest.required_item = [70001, 0, 0, 0];
    quest.required_item_count = [1, 0, 0, 0];
    let loot = rows.loot.as_mut().unwrap();
    loot.captures[0].rows[0].chance_or_quest_chance_bits = (-80f32).to_bits();
    loot.rules.quest_drop_rate = benilla_sim::loot::QuestDropRate {
        multiplier_bits: 1.25f32.to_bits(),
    };
    let mut items: Vec<_> = loot.inventory.items().cloned().collect();
    let item = items.iter_mut().find(|item| item.entry == 70001).unwrap();
    item.class = 12;
    item.bonding = 4;
    loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
    (p, input, rows)
}
fn active(w: &mut NativeWorld, a: &mut impl Authority) {
    let giver = w.state.npcs[&3].id;
    step(
        w,
        a,
        Command::AcceptQuest {
            quest: 70001,
            giver,
        },
    );
}

#[test]
fn quest_only_visibility_rechecks_need_and_rejects_an_extra_preexisting_slot() {
    let (p, input, mut rows) = setup_quest();
    let capture = &mut rows.loot.as_mut().unwrap().captures[0];
    capture.rows.push(capture.rows[0].clone());
    capture.loader_order.result_row_count = 2;
    capture.loader_order.selected_row_positions = vec![0, 1];
    let mut w = world_from(p, input, rows);
    let mut a = LootAuthority::default();
    assert!(!w.loot_quest_items().unwrap().contains(&70001));
    active(&mut w, &mut a);
    assert!(w.loot_quest_items().unwrap().contains(&70001));
    w = roundtrip(&w);
    kill(&mut w, &mut a);
    let corpse = w.state.npcs[&2].id;
    assert_eq!(
        w.state.npcs[&2]
            .corpse_loot
            .as_ref()
            .unwrap()
            .generated
            .items
            .len(),
        2
    );
    w = roundtrip(&w);
    step(&mut w, &mut a, Command::OpenLoot { corpse });
    step(&mut w, &mut a, Command::LootItem { corpse, slot: 0 });
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|event| matches!(event, Event::Loot(LootEvent::ItemRemoved { slot: 1, .. }))));
    assert_eq!(
        progress(&w).quests.progress(70001).unwrap().status,
        q::Status::Complete
    );
    assert!(!w.loot_quest_items().unwrap().contains(&70001));
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 1);
    let before = progress(&w).quests.clone();
    step(&mut w, &mut a, Command::LootItem { corpse, slot: 1 });
    assert_eq!(progress(&w).quests, before);
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 1);
    assert!(!w.state.npcs[&2]
        .corpse_loot
        .as_ref()
        .unwrap()
        .collected
        .contains(&1));
    step(&mut w, &mut a, Command::OpenLoot { corpse });
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|event| matches!(event,
        Event::Loot(LootEvent::Opened(window)) if window.items.is_empty())));
    let giver = w.state.npcs[&3].id;
    step(
        &mut w,
        &mut a,
        Command::TurnInQuest {
            quest: 70001,
            giver,
        },
    );
    assert!(progress(&w).quests.progress(70001).unwrap().rewarded);
    assert!(!w.loot_quest_items().unwrap().contains(&70001));
    w = roundtrip(&w);
    let before = progress(&w).clone();
    step(
        &mut w,
        &mut a,
        Command::TurnInQuest {
            quest: 70001,
            giver,
        },
    );
    assert_eq!(progress(&w).quests, before.quests);
    assert_eq!(progress(&w).inventory, before.inventory);
    assert_eq!(progress(&w).money, before.money);
}

#[test]
fn quest_only_failed_storage_and_callbacks_preserve_quest_counts_and_corpse_ownership() {
    for fault in 0..3 {
        let (p, input, mut rows) = setup_quest();
        let loot = rows.loot.as_mut().unwrap();
        let mut items: Vec<_> = loot.inventory.items().cloned().collect();
        let mut filler = synthetic_loot_fixture::item(70003);
        filler.stackable = 1;
        items.push(filler);
        loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
        let mut w = world_from(p, input, rows.clone());
        let mut a = LootAuthority::default();
        active(&mut w, &mut a);
        if fault == 0 {
            let catalog = w
                .profile
                .quest_progression
                .as_ref()
                .unwrap()
                .loot
                .as_ref()
                .unwrap()
                .inventory
                .clone();
            let current = w.state.quest_progression.as_mut().unwrap();
            let inventory = current.inventory.as_mut().unwrap();
            let count = 16 - inventory.slots.len() as u32;
            inventory
                .store_corpse_item(&catalog, 70003, count, |_, _, _| Ok(()))
                .unwrap();
            super::super::loot::update_inventory_receipts(current, [0x71; 32]).unwrap();
        }
        kill(&mut w, &mut a);
        let corpse = w.state.npcs[&2].id;
        step(&mut w, &mut a, Command::OpenLoot { corpse });
        let current = w.state.quest_progression.as_mut().unwrap();
        if fault == 1 {
            current.inventory.as_mut().unwrap().next_item = u64::MAX;
        } else if fault == 2 {
            let mut snapshot = current.quests.snapshot();
            snapshot.next_sequence = u64::MAX;
            let catalog = q::Catalog::new(rows.quests, rows.relations, rows.quest_audit).unwrap();
            current.quests = q::State::restore(&catalog, snapshot).unwrap();
        }
        let before = w.checkpoint().unwrap();
        let quests = progress(&w).quests.clone();
        let inventory = inventory(&w).clone();
        let saved_corpse = w.state.npcs[&2].corpse_loot.clone();
        let input = command(&w, Command::LootItem { corpse, slot: 0 });
        let result = w.step(w.state.tick + 1, Some(input), &mut a);
        if fault == 0 {
            result.unwrap();
            assert!(w.last_tick().unwrap().events.iter().any(|event| matches!(
                event,
                Event::Loot(LootEvent::StorageFailed {
                    result: inv::InventoryResult::InventoryFull,
                    ..
                })
            )));
        } else {
            assert!(result.is_err());
            assert_eq!(w.checkpoint().unwrap(), before);
        }
        assert_eq!(progress(&w).quests, quests);
        assert_eq!(super::inventory(&w), &inventory);
        assert_eq!(w.state.npcs[&2].corpse_loot, saved_corpse);
    }
}
