// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-owned original mainhand auto-equip; no generic weapon admission.
use super::*;
use quest_progression::equipment_movement::EquipmentMoveOperation;
use synthetic_equipment_fixture::{weapon_profile, EquipmentAuthority};
fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(
        weapon_profile(),
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
fn auto(w: &mut NativeWorld) {
    step(w, Command::EquipmentAutoEquip { source: 23 });
}
fn reopen_and_replay(w: &NativeWorld, initial: Checkpoint) {
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn original_staff_auto_equip_preserves_identity_frontier_resources_and_source_caches() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    let original = *w.state.combat.as_ref().unwrap().equipment.item(15).unwrap();
    let before = quest_progression::MeleeExecutionBinding::capture(&w.profile, &w.state)
        .unwrap()
        .current_operands(&w.profile)
        .unwrap();
    detach(&mut w);
    let carried = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .slots[&23]
        .clone();
    let frontier = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .next_item;
    let mana = w.state.resources.caster.mana;
    let health = w.state.health;
    let timers = w.state.combat.as_ref().unwrap().melee_timers.remaining_ms();
    auto(&mut w);
    assert_eq!(
        w.state.combat.as_ref().unwrap().equipment.item(15),
        Some(&original)
    );
    let inv = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap();
    assert!(!inv.slots.contains_key(&23));
    assert_eq!(inv.next_item, frontier);
    assert_eq!(
        w.profile
            .quest_progression
            .as_ref()
            .unwrap()
            .loot
            .as_ref()
            .unwrap()
            .equipment_instance(original.origin),
        Some(carried.id)
    );
    assert_eq!(w.state.resources.caster.mana, mana);
    assert_eq!(w.state.health, health);
    assert_eq!(
        w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
        timers.map(|value| value.saturating_sub(w.profile.tick_ms))
    );
    let after = quest_progression::MeleeExecutionBinding::capture(&w.profile, &w.state)
        .unwrap()
        .current_operands(&w.profile)
        .unwrap();
    assert_eq!(
        (after.skill_id, after.weapon_item, after.attack_times_ms),
        (136, Some(900002), [2500, 2000])
    );
    assert_eq!(
        (after.minimum_damage, after.maximum_damage),
        (before.minimum_damage, before.maximum_damage)
    );
    let receipt = w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .find_map(|event| match event {
            Event::EquipmentMoved(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(receipt.context.operation, EquipmentMoveOperation::AutoEquip);
    reopen_and_replay(&w, initial);
}
#[test]
fn casting_refuses39_then_traveling_staff_auto_equip_uses_retained_prefix() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    detach(&mut w);
    let target = w.state.npcs[&2].id;
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
    let inventory = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .clone();
    auto(&mut w);
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|event| matches!(event, Event::EquipmentMoveRefused { reason: 39, .. })));
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().inventory,
        inventory
    );
    for _ in 0..40 {
        if w.state
            .casts
            .values()
            .any(|c| matches!(c.execution.state(), spell::CastState::Traveling { .. }))
        {
            break;
        }
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
    }
    assert!(w
        .state
        .casts
        .values()
        .any(|c| matches!(c.execution.state(), spell::CastState::Traveling { .. })));
    assert!(!w.state.combat.as_ref().unwrap().in_combat);
    let remaining = w.state.combat.as_ref().unwrap().melee_timers.remaining_ms();
    assert_eq!(remaining[0], 2000 - w.profile.tick_ms);
    auto(&mut w);
    let receipt = w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .find_map(|event| match event {
            Event::EquipmentMoved(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert!(receipt.context.generic_slot.is_some());
    assert_eq!(receipt.context.operation, EquipmentMoveOperation::AutoEquip);
    assert_eq!(
        w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
        remaining.map(|value| value.saturating_sub(w.profile.tick_ms))
    );
    reopen_and_replay(&w, initial);
}
#[test]
fn real_combat_auto_equip_is_host_unsupported_with_exact_rollback() {
    let mut w = world();
    detach(&mut w);
    let target = w.state.npcs[&2].id;
    step(&mut w, Command::AttackStart { target });
    for _ in 0..100 {
        if w.state.combat.as_ref().unwrap().in_combat {
            break;
        }
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
    }
    assert!(w.state.combat.as_ref().unwrap().in_combat);
    let checkpoint = w.checkpoint().unwrap();
    let record = w.last_tick().unwrap().clone();
    let command = synthetic_quest_fixture::command(&w, Command::EquipmentAutoEquip { source: 23 });
    assert!(matches!(
        w.step(
            w.state.tick + 1,
            Some(command),
            &mut EquipmentAuthority::default()
        ),
        Err(NativeError::UnsupportedMechanic(_))
    ));
    assert_eq!(w.checkpoint().unwrap(), checkpoint);
    assert_eq!(w.last_tick().unwrap(), &record);
}
#[test]
fn empty_source_auto_equip_is_silent_without_custody_mutation() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    let inventory = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .clone();
    let calls = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .derived_calls
        .clone();
    step(&mut w, Command::EquipmentAutoEquip { source: 38 });
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().inventory,
        inventory
    );
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().derived_calls,
        calls
    );
    assert!(!w.last_tick().unwrap().events.iter().any(|event| matches!(
        event,
        Event::EquipmentMoved(_) | Event::EquipmentMoveRefused { .. }
    )));
    reopen_and_replay(&w, initial);
}
#[test]
fn singleton_mainhand21_succeeds_but_ambiguous_weapon13_remains_unsupported() {
    for kind in [21, 13] {
        let mut w = NativeWorld::synthetic_component_test(
            synthetic_equipment_fixture::weapon_profile_with_type(kind),
            [1; 16],
            synthetic_fixture::player(),
            42,
        )
        .unwrap();
        let initial = w.checkpoint().unwrap();
        let original = *w.state.combat.as_ref().unwrap().equipment.item(15).unwrap();
        detach(&mut w);
        let before = w.checkpoint().unwrap();
        if kind == 21 {
            auto(&mut w);
            assert_eq!(
                w.state.combat.as_ref().unwrap().equipment.item(15),
                Some(&original)
            );
            reopen_and_replay(&w, initial);
        } else {
            let input =
                synthetic_quest_fixture::command(&w, Command::EquipmentAutoEquip { source: 23 });
            assert!(matches!(
                w.step(
                    w.state.tick + 1,
                    Some(input),
                    &mut EquipmentAuthority::default()
                ),
                Err(NativeError::UnsupportedMechanic(_))
            ));
            assert_eq!(w.checkpoint().unwrap(), before);
            reopen_and_replay(&w, initial);
        }
    }
}
