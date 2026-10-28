// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::quest_lifecycle as q;
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
fn inventory(w: &NativeWorld) -> &benilla_sim::inventory::State {
    w.state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
}
fn prepared() -> NativeWorld {
    let mut w = NativeWorld::synthetic_component_test(
        synthetic_equipment_fixture::profile_with_duplicate_armor(),
        [1; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap();
    step(
        &mut w,
        Command::InventoryMove(InventoryMoveRequest {
            source: 23,
            destination: 38,
        }),
    );
    step(
        &mut w,
        Command::EquipmentMove(EquipmentMoveRequest {
            source: 4,
            destination: 23,
        }),
    );
    step(
        &mut w,
        Command::InventoryMove(InventoryMoveRequest {
            source: 38,
            destination: 24,
        }),
    );
    let g = w
        .profile
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap();
    assert!(g.equipment_origin(inventory(&w).slots[&23].id).is_some());
    assert!(g.equipment_origin(inventory(&w).slots[&24].id).is_none());
    assert_eq!(inventory(&w).slots[&23].item, inventory(&w).slots[&24].item);
    let giver = w.state.npcs[&3].id;
    step(
        &mut w,
        Command::AcceptQuest {
            quest: 70001,
            giver,
        },
    );
    w
}
fn destroy(slot: u8) -> Command {
    Command::InventoryDestroy(InventoryDestroyRequest { slot, count: 0 })
}
#[test]
fn selected_ordinary_duplicate_destroy_preserves_registered_original_and_unrelated_quest() {
    let mut w = prepared();
    let q = w.state.quest_progression.as_ref().unwrap();
    let progress = q.quests.progress(70001).unwrap().clone();
    let before = w.checkpoint().unwrap();
    let original = inventory(&w).slots[&23].clone();
    let frontier = inventory(&w).next_item;
    let recipe = w.state.combat.as_ref().unwrap().recipe.snapshot();
    let custody = q.derived_calls.clone();
    step(&mut w, destroy(24));
    assert_eq!(inventory(&w).slots[&23], original);
    assert!(!inventory(&w).slots.contains_key(&24));
    assert_eq!(inventory(&w).next_item, frontier);
    assert_eq!(w.state.combat.as_ref().unwrap().recipe.snapshot(), recipe);
    let q = w.state.quest_progression.as_ref().unwrap();
    assert_eq!(q.derived_calls, custody);
    assert_eq!(q.quests.progress(70001).unwrap(), &progress);
    let events = &w.last_tick().unwrap().events;
    let removal = events.iter().position(|e| matches!(e,Event::QuestProgression(QuestProgressionEvent::InventoryDestroyed(r)) if r.slot==24 && r.removed==1)).unwrap();
    assert!(
        matches!(&events[removal], Event::QuestProgression(QuestProgressionEvent::InventoryDestroyed(r)) if r.slot == 24 && r.removed == 1)
    );
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), before).unwrap();
    replay
        .replay(std::slice::from_ref(w.last_tick().unwrap()))
        .unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
        .unwrap();
}
#[test]
fn selected_duplicate_refinement_keeps_registered_and_unselected_consumers_closed() {
    let mut w = prepared();
    let before = w.checkpoint().unwrap();
    let g = w.profile.quest_progression.as_ref().unwrap();
    let q = w.state.quest_progression.as_ref().unwrap();
    let combat = w.state.combat.as_ref().unwrap();
    let item = inventory(&w).slots[&23].item;
    assert!(q
        .reduce(g, combat, q::Command::RemoveItems { item, count: 1 })
        .is_err());
    let registered = inventory(&w)
        .plan_destroy_backpack(&g.loot.as_ref().unwrap().inventory, 23, 0)
        .unwrap();
    assert!(q.destroy_selected(g, combat, registered).is_err());
    let mut forged = inventory(&w)
        .plan_destroy_backpack(&g.loot.as_ref().unwrap().inventory, 24, 0)
        .unwrap();
    forged.after.slots.remove(&23);
    assert!(q.destroy_selected(g, combat, forged).is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    let input = synthetic_quest_fixture::command(&w, destroy(23));
    assert!(w
        .step(
            w.state.tick + 1,
            Some(input),
            &mut EquipmentAuthority::default()
        )
        .is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
}
#[test]
fn selected_duplicate_destroy_record_tampering_rolls_back() {
    let mut w = prepared();
    let before = w.checkpoint().unwrap();
    step(&mut w, destroy(24));
    for fault in 0..3 {
        let mut record = w.last_tick().unwrap().clone();
        let r = record
            .events
            .iter_mut()
            .find_map(|e| match e {
                Event::QuestProgression(QuestProgressionEvent::InventoryDestroyed(r)) => Some(r),
                _ => None,
            })
            .unwrap();
        match fault {
            0 => r.slot = 23,
            1 => {
                r.after.slots.remove(&23);
            }
            _ => r.after.next_item += 1,
        }
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), before.clone())
                .unwrap();
        assert!(replay.replay(&[record]).is_err());
        assert_eq!(replay.checkpoint().unwrap(), before);
    }
}
