// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use quest_progression::equipment_movement::{placements, EquipmentPlacement};
use synthetic_equipment_fixture::EquipmentAuthority;
fn step(w: &mut NativeWorld, command: Command) {
    let input = synthetic_quest_fixture::command(w, command);
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut EquipmentAuthority::default(),
    )
    .unwrap();
}
fn prepared(lethal: bool) -> NativeWorld {
    let mut w = NativeWorld::synthetic_component_test(
        synthetic_equipment_fixture::profile_with_lethal(lethal),
        [1; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap();
    step(
        &mut w,
        Command::EquipmentMove(EquipmentMoveRequest {
            source: 4,
            destination: 23,
        }),
    );
    w
}
fn inventory(w: &NativeWorld) -> &benilla_sim::inventory::State {
    w.state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
}
fn destroy(w: &mut NativeWorld, count: u8) {
    step(
        w,
        Command::InventoryDestroy(InventoryDestroyRequest { slot: 23, count }),
    );
}
fn reopen(w: &NativeWorld) {
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn selected_original_destroy_retires_exact_identity_without_stat_or_allocator_change() {
    for count in [0, 1, 255] {
        let mut w = prepared(false);
        let initial = w.checkpoint().unwrap();
        let prefix = w.records().len();
        let before = inventory(&w).clone();
        let original = before.slots[&23].clone();
        let recipe = w.state.combat.as_ref().unwrap().recipe.snapshot();
        let equipment = w.state.combat.as_ref().unwrap().equipment.snapshot();
        let stats = w.state.combat.as_ref().unwrap().stats.clone();
        destroy(&mut w, count);
        assert!(!inventory(&w).slots.contains_key(&23));
        assert_eq!(inventory(&w).next_item, before.next_item);
        assert_eq!(inventory(&w).equipped, before.equipped);
        assert_eq!(w.state.combat.as_ref().unwrap().recipe.snapshot(), recipe);
        assert_eq!(
            w.state.combat.as_ref().unwrap().equipment.snapshot(),
            equipment
        );
        assert_eq!(w.state.combat.as_ref().unwrap().stats, stats);
        let q = w.state.quest_progression.as_ref().unwrap();
        let g = w.profile.quest_progression.as_ref().unwrap();
        assert!(g
            .loot
            .as_ref()
            .unwrap()
            .equipment_origin(original.id)
            .is_some());
        assert_eq!(
            placements(g, &q.derived_calls).unwrap()[&original.id],
            EquipmentPlacement::Retired
        );
        assert!(!q.inventory_receipts.contains_key(&original.id));
        let events = &w.last_tick().unwrap().events;
        let selected=events.iter().position(|e|matches!(e,Event::QuestProgression(QuestProgressionEvent::InventoryDestroyed(r)) if r.slot==23&&r.removed==1)).unwrap();
        assert!(matches!(
            &events[selected + 1],
            Event::QuestProgression(QuestProgressionEvent::Quest {
                event: benilla_sim::quest_lifecycle::Event::Host(
                    benilla_sim::quest_lifecycle::HostEffect::DestroyCarriedStack { count: 1, .. }
                ),
                ..
            })
        ));
        assert!(!events.iter().any(|e| matches!(
            e,
            Event::EquipmentMoved(_) | Event::ItemAcquired(_) | Event::InventoryStored(_)
        )));
        reopen(&w);
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
        replay.replay(&w.records()[prefix..]).unwrap();
        assert_eq!(replay.state_digest(), w.state_digest());
        let calls = w
            .state
            .quest_progression
            .as_ref()
            .unwrap()
            .derived_calls
            .clone();
        destroy(&mut w, 0);
        assert_eq!(
            w.state.quest_progression.as_ref().unwrap().derived_calls,
            calls
        );
        assert!(w
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::Rejected(Rejection::Inventory { reason: 23, .. }))));
        reopen(&w);
    }
}
#[test]
fn actual_dead_player_can_destroy_detached_original_without_resurrection() {
    let mut w = prepared(true);
    let target = w.state.npcs[&2].id;
    step(
        &mut w,
        Command::Cast {
            ability: 133,
            target,
        },
    );
    for _ in 0..100 {
        if !w.state.player_alive() {
            break;
        }
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
    }
    assert!(!w.state.player_alive());
    let before = w.checkpoint().unwrap();
    let original = inventory(&w).slots[&23].clone();
    destroy(&mut w, 0);
    assert!(!w.state.player_alive());
    assert_eq!(w.state.health, 0);
    assert!(!inventory(&w).slots.values().any(|i| i.id == original.id));
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), before).unwrap();
    replay
        .replay(std::slice::from_ref(w.last_tick().unwrap()))
        .unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn retired_custody_cannot_be_removed_or_relabelled_in_rehashed_checkpoint() {
    let mut w = prepared(false);
    destroy(&mut w, 0);
    for fault in 0..3 {
        let mut cp = w.checkpoint().unwrap();
        let mut value = serde_json::to_value(&cp.state).unwrap();
        let calls = value["quest_progression"]["derived_calls"]
            .as_array_mut()
            .unwrap();
        let index = calls
            .iter()
            .position(|c| c.get("ItemDestruction").is_some())
            .unwrap();
        match fault {
            0 => {
                calls.remove(index);
            }
            1 => calls[index]["ItemDestruction"]["movement"]["instance"] = serde_json::json!(0),
            _ => {
                let m = &mut calls[index]["ItemDestruction"]["movement"];
                let id = m["instance"].as_u64().unwrap().to_string();
                m["before_placements"][id] = serde_json::json!("Retired");
            }
        }
        cp.state = serde_json::from_value(value).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v72\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v72\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err());
    }
}

#[test]
fn retiring_one_original_preserves_other_original_movement_receipt_custody_and_replay() {
    let mut w = NativeWorld::synthetic_component_test(
        synthetic_equipment_fixture::profile_with_two_originals(),
        [1; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap();
    let mut replay = w.clone();
    let a = *w.state.combat.as_ref().unwrap().equipment.item(4).unwrap();
    let b = *w.state.combat.as_ref().unwrap().equipment.item(5).unwrap();
    assert_eq!((a.item_id, b.item_id), (900001, 900003));
    let registry = w
        .profile
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap();
    let a_id = registry.equipment_instance(a.origin).unwrap();
    let b_id = registry.equipment_instance(b.origin).unwrap();
    assert_ne!(a_id, b_id);
    let frontier = inventory(&w).next_item;
    step(
        &mut w,
        Command::EquipmentMove(EquipmentMoveRequest {
            source: 4,
            destination: 23,
        }),
    );
    assert_eq!(inventory(&w).slots[&23].id, a_id);
    destroy(&mut w, 0);
    for (source, destination) in [(5, 23), (23, 5)] {
        step(
            &mut w,
            Command::EquipmentMove(EquipmentMoveRequest {
                source,
                destination,
            }),
        );
        let movement = w
            .last_tick()
            .unwrap()
            .events
            .iter()
            .find_map(|event| {
                if let Event::EquipmentMoved(receipt) = event {
                    Some(receipt.as_ref())
                } else {
                    None
                }
            })
            .unwrap();
        let g = w.profile.quest_progression.as_ref().unwrap();
        assert_eq!(movement.instance, b_id);
        assert_eq!(movement.origin, b.origin);
        assert_eq!(
            movement.before_placements[&a_id],
            EquipmentPlacement::Retired
        );
        // This standalone validation must use retained tombstones, not infer all
        // absent originals are still worn from the carried-inventory snapshot.
        movement.validate(g).unwrap();
        let mut invented_worn_a = movement.clone();
        invented_worn_a
            .before_placements
            .insert(a_id, EquipmentPlacement::Slot(4));
        assert!(invented_worn_a.validate(g).is_err());
        let q = w.state.quest_progression.as_ref().unwrap();
        let custody = placements(g, &q.derived_calls).unwrap();
        assert_eq!(custody[&a_id], EquipmentPlacement::Retired);
        assert_eq!(custody[&b_id], EquipmentPlacement::Slot(destination));
        assert!(!q.inventory_receipts.contains_key(&a_id));
        assert_eq!(inventory(&w).next_item, frontier);
        assert!(g.loot.as_ref().unwrap().equipment_origin(a_id).is_some());
        if destination == 23 {
            assert_eq!(inventory(&w).slots[&23].id, b_id);
        }
        reopen(&w);
    }
    assert!(w.state.combat.as_ref().unwrap().equipment.item(4).is_none());
    assert_eq!(w.state.combat.as_ref().unwrap().equipment.item(5), Some(&b));
    assert_eq!(
        w.state
            .combat
            .as_ref()
            .unwrap()
            .recipe
            .snapshot()
            .inactive_slots,
        vec![4]
    );
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
