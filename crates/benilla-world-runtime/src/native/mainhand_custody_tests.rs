// SPDX-License-Identifier: GPL-3.0-or-later
//! Carried original-weapon custody without equipment callbacks or new identities.
use super::*;
use quest_progression::DerivedCall;
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
fn relocate(w: &mut NativeWorld, from: u8, to: u8) {
    step(
        w,
        Command::InventoryMove(InventoryMoveRequest {
            source: from,
            destination: to,
        }),
    );
}
fn auto(w: &mut NativeWorld, source: u8) {
    step(
        w,
        Command::InventoryAutoStore(InventoryAutoStoreRequest { source }),
    );
}
fn memberships(w: &NativeWorld) -> usize {
    w.state
        .quest_progression
        .as_ref()
        .unwrap()
        .derived_calls
        .iter()
        .filter(|call| matches!(call, DerivedCall::ItemMembership { .. }))
        .count()
}
fn verify_replay(w: &NativeWorld, initial: Checkpoint) {
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
fn custody(w: &mut NativeWorld, command: Command, from: u8, to: u8) {
    let combat = w.state.combat.as_ref().unwrap().clone();
    let inventory = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .clone();
    let count = memberships(w);
    step(w, command);
    let after = w.state.combat.as_ref().unwrap();
    assert_eq!(after.recipe.snapshot(), combat.recipe.snapshot());
    assert_eq!(after.equipment, combat.equipment);
    assert_eq!(after.stats, combat.stats);
    assert_eq!(
        after.melee_timers.remaining_ms(),
        combat
            .melee_timers
            .remaining_ms()
            .map(|value| value.saturating_sub(w.profile.tick_ms))
    );
    let inventory_after = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap();
    assert_eq!(inventory_after.next_item, inventory.next_item);
    assert_eq!(inventory_after.equipped, inventory.equipped);
    assert_eq!(inventory_after.slots[&to], inventory.slots[&from]);
    assert!(!inventory_after.slots.contains_key(&from));
    assert_eq!(memberships(w), count);
    let operands = quest_progression::MeleeExecutionBinding::capture(&w.profile, &w.state)
        .unwrap()
        .current_operands(&w.profile)
        .unwrap();
    assert_eq!((operands.skill_id, operands.weapon_item), (162, None));
    assert_eq!(operands.minimum_damage, operands.maximum_damage);
}
#[test]
fn carried_staff_relocates_autostores_and_autoequips_same_original_with_replay() {
    let mut w = world(false);
    let initial = w.checkpoint().unwrap();
    let original = *w.state.combat.as_ref().unwrap().equipment.item(15).unwrap();
    detach(&mut w);
    let timers = w.state.combat.as_ref().unwrap().melee_timers.remaining_ms();
    custody(
        &mut w,
        Command::InventoryMove(InventoryMoveRequest {
            source: 23,
            destination: 24,
        }),
        23,
        24,
    );
    assert_eq!(
        w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
        timers.map(|v| v.saturating_sub(w.profile.tick_ms))
    );
    custody(
        &mut w,
        Command::InventoryAutoStore(InventoryAutoStoreRequest { source: 24 }),
        24,
        23,
    );
    step(&mut w, Command::EquipmentAutoEquip { source: 23 });
    assert_eq!(
        w.state.combat.as_ref().unwrap().equipment.item(15),
        Some(&original)
    );
    verify_replay(&w, initial);
}
#[test]
fn casting_and_actual_combat_allow_carried_staff_custody_without_membership_callbacks() {
    for casting in [true, false] {
        let mut w = world(false);
        let initial = w.checkpoint().unwrap();
        detach(&mut w);
        let target = w.state.npcs[&2].id;
        if casting {
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
        } else {
            step(&mut w, Command::AttackStart { target });
            for _ in 0..100 {
                if w.state.combat.as_ref().unwrap().in_combat {
                    break;
                }
                w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
                    .unwrap();
            }
            assert!(w.state.combat.as_ref().unwrap().in_combat);
        }
        custody(
            &mut w,
            Command::InventoryMove(InventoryMoveRequest {
                source: 23,
                destination: 24,
            }),
            23,
            24,
        );
        custody(
            &mut w,
            Command::InventoryAutoStore(InventoryAutoStoreRequest { source: 24 }),
            24,
            23,
        );
        verify_replay(&w, initial);
    }
}
#[test]
fn actual_dead_player_cannot_swap_but_can_autostore_detached_original_staff() {
    let mut w = world(true);
    let initial = w.checkpoint().unwrap();
    detach(&mut w);
    relocate(&mut w, 23, 24);
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
    let before = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .clone();
    relocate(&mut w, 24, 25);
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e,Event::InventoryMoved(m) if m.result.protocol()==38)));
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().inventory,
        before
    );
    let count = memberships(&w);
    let recipe = w.state.combat.as_ref().unwrap().recipe.snapshot();
    auto(&mut w, 24);
    let inv = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap();
    assert_eq!(inv.slots[&23], before.as_ref().unwrap().slots[&24]);
    assert_eq!(inv.next_item, before.as_ref().unwrap().next_item);
    assert!(!inv.slots.contains_key(&24));
    assert_eq!(memberships(&w), count);
    assert_eq!(w.state.combat.as_ref().unwrap().recipe.snapshot(), recipe);
    assert!(!w.state.player_alive());
    verify_replay(&w, initial);
}
