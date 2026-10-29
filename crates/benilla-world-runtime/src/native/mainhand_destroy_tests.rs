// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected original-staff retirement without weapon callbacks or identity reuse.
use super::*;
use quest_progression::equipment_movement::{placements, EquipmentPlacement};
use synthetic_equipment_fixture::EquipmentAuthority;
fn world(lethal: bool) -> NativeWorld {
    NativeWorld::synthetic_component_test(
        if lethal {
            synthetic_equipment_fixture::lethal_weapon_profile()
        } else {
            synthetic_equipment_fixture::weapon_profile()
        },
        [1; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap()
}
fn step(w: &mut NativeWorld, command: Command) {
    let input = synthetic_quest_fixture::command(w, command);
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut EquipmentAuthority::default(),
    )
    .unwrap();
}
fn detach(w: &mut NativeWorld) {
    step(
        w,
        Command::EquipmentMove(EquipmentMoveRequest {
            source: 15,
            destination: 23,
        }),
    );
}
fn destroy(w: &mut NativeWorld, count: u8) {
    step(
        w,
        Command::InventoryDestroy(InventoryDestroyRequest { slot: 23, count }),
    );
}
fn replay(w: &NativeWorld, initial: Checkpoint) {
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
fn assert_retired(w: &NativeWorld, id: u64, frontier: u64) {
    let q = w.state.quest_progression.as_ref().unwrap();
    let g = w.profile.quest_progression.as_ref().unwrap();
    let inv = q.inventory.as_ref().unwrap();
    assert!(!inv.slots.values().any(|item| item.id == id));
    assert_eq!(inv.next_item, frontier);
    assert!(!q.inventory_receipts.contains_key(&id));
    assert!(g.loot.as_ref().unwrap().equipment_origin(id).is_some());
    assert_eq!(
        placements(g, &q.derived_calls).unwrap()[&id],
        EquipmentPlacement::Retired
    );
    assert!(w
        .state
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(15)
        .is_none());
    assert!(w
        .state
        .combat
        .as_ref()
        .unwrap()
        .recipe
        .snapshot()
        .inactive_slots
        .contains(&15));
}
#[test]
fn selected_staff_retirement_preserves_inactive_cache_then_actual_unarmed_hit_and_replay() {
    for count in [0, 1, 255] {
        let mut w = world(false);
        let initial = w.checkpoint().unwrap();
        detach(&mut w);
        let inv = w
            .state
            .quest_progression
            .as_ref()
            .unwrap()
            .inventory
            .as_ref()
            .unwrap();
        let id = inv.slots[&23].id;
        let frontier = inv.next_item;
        let combat = w.state.combat.as_ref().unwrap().clone();
        destroy(&mut w, count);
        assert_retired(&w, id, frontier);
        assert_eq!(
            w.state.combat.as_ref().unwrap().recipe.snapshot(),
            combat.recipe.snapshot()
        );
        assert_eq!(w.state.combat.as_ref().unwrap().stats, combat.stats);
        assert_eq!(w.state.combat.as_ref().unwrap().equipment, combat.equipment);
        let operands = quest_progression::MeleeExecutionBinding::capture(&w.profile, &w.state)
            .unwrap()
            .current_operands(&w.profile)
            .unwrap();
        assert_eq!(
            (
                operands.skill_id,
                operands.weapon_item,
                operands.attack_times_ms
            ),
            (162, None, [2000, 2000])
        );
        assert_eq!(operands.minimum_damage, operands.maximum_damage);
        let events = &w.last_tick().unwrap().events;
        let index=events.iter().position(|e|matches!(e,Event::QuestProgression(QuestProgressionEvent::InventoryDestroyed(r)) if r.slot==23 && r.removed==1)).unwrap();
        assert!(matches!(
            &events[index + 1],
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
        let target = w.state.npcs[&2].id;
        let health = w.state.npcs[&2].health;
        step(&mut w, Command::AttackStart { target });
        for _ in 0..100 {
            if w.state.npcs[&2].health < health {
                break;
            }
            w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
                .unwrap();
        }
        assert!(w.state.npcs[&2].health < health);
        assert_retired(&w, id, frontier);
        replay(&w, initial);
    }
}
#[test]
fn casting_with_old_staff_prefix_can_detach_retire_and_finish_actual_projectile() {
    let mut w = world(false);
    let initial = w.checkpoint().unwrap();
    let target = w.state.npcs[&2].id;
    let health = w.state.npcs[&2].health;
    step(
        &mut w,
        Command::Cast {
            ability: 133,
            target,
        },
    );
    assert!(w
        .state
        .casts
        .values()
        .any(|c| matches!(c.execution.state(), spell::CastState::Casting { .. })));
    detach(&mut w);
    let inv = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap();
    let id = inv.slots[&23].id;
    let frontier = inv.next_item;
    destroy(&mut w, 0);
    assert!(w
        .state
        .casts
        .values()
        .any(|c| matches!(c.execution.state(), spell::CastState::Casting { .. })));
    assert_retired(&w, id, frontier);
    replay(&w, initial.clone());
    let mut traveling = false;
    for _ in 0..100 {
        traveling |= w
            .state
            .casts
            .values()
            .any(|c| matches!(c.execution.state(), spell::CastState::Traveling { .. }));
        if w.state.npcs[&2].health < health {
            break;
        }
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
    }
    assert!(traveling);
    assert!(w.state.npcs[&2].health < health);
    assert_retired(&w, id, frontier);
    replay(&w, initial);
}
#[test]
fn actual_dead_player_can_retire_detached_staff_without_resurrection() {
    let mut w = world(true);
    let initial = w.checkpoint().unwrap();
    detach(&mut w);
    let target = w.state.npcs[&2].id;
    step(&mut w, Command::AttackStart { target });
    for _ in 0..100 {
        if !w.state.player_alive() {
            break;
        }
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
    }
    assert!(!w.state.player_alive());
    let inv = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap();
    let id = inv.slots[&23].id;
    let frontier = inv.next_item;
    let recipe = w.state.combat.as_ref().unwrap().recipe.snapshot();
    destroy(&mut w, 0);
    assert_retired(&w, id, frontier);
    assert_eq!(w.state.health, 0);
    assert!(!w.state.player_alive());
    assert_eq!(w.state.combat.as_ref().unwrap().recipe.snapshot(), recipe);
    replay(&w, initial);
}
#[test]
fn rehashed_retired_staff_cannot_resurrect_reserved_identity_or_restore_active_cache() {
    let mut w = world(false);
    detach(&mut w);
    let original = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .slots[&23]
        .clone();
    let original_receipt = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory_receipts[&original.id]
        .clone();
    destroy(&mut w, 0);
    for fault in 0..3 {
        let mut cp = w.checkpoint().unwrap();
        let mut value = serde_json::to_value(&cp.state).unwrap();
        match fault {
            0 => {
                value["quest_progression"]["inventory"]["slots"]["23"] =
                    serde_json::to_value(&original).unwrap();
                value["quest_progression"]["inventory_receipts"][original.id.to_string()] =
                    serde_json::to_value(&original_receipt).unwrap();
            }
            1 => value["combat"]["recipe"]["inactive_slots"] = serde_json::json!([]),
            _ => {
                let calls = value["quest_progression"]["derived_calls"]
                    .as_array_mut()
                    .unwrap();
                let index = calls
                    .iter()
                    .position(|c| c.get("ItemDestruction").is_some())
                    .unwrap();
                calls.remove(index);
            }
        }
        assert_ne!(value, serde_json::to_value(&cp.state).unwrap());
        if fault == 0 {
            let inventory: benilla_sim::inventory::State =
                serde_json::from_value(value["quest_progression"]["inventory"].clone()).unwrap();
            inventory
                .validate(
                    &w.profile
                        .quest_progression
                        .as_ref()
                        .unwrap()
                        .loot
                        .as_ref()
                        .unwrap()
                        .inventory,
                )
                .unwrap();
            let receipt: loot::ItemReceipt = serde_json::from_value(
                value["quest_progression"]["inventory_receipts"][original.id.to_string()].clone(),
            )
            .unwrap();
            assert!(receipt.validates(23, &original));
        }
        cp.state = serde_json::from_value(value).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v79\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v79\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err());
    }
}
