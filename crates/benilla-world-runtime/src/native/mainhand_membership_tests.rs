// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual original-staff custody, source operands and durable historical replay.
use super::*;
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
fn movement(source: u8, destination: u8) -> Command {
    Command::EquipmentMove(EquipmentMoveRequest {
        source,
        destination,
    })
}
fn reopen(w: &NativeWorld) {
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
}
struct Observed {
    skill_id: u16,
    weapon_item: Option<u32>,
    attack_times_ms: [u32; 2],
    minimum_damage: spell::SpellFloat,
    maximum_damage: spell::SpellFloat,
}
fn operands(w: &NativeWorld) -> Observed {
    let actual = quest_progression::MeleeExecutionBinding::capture(&w.profile, &w.state)
        .unwrap()
        .current_operands(&w.profile)
        .unwrap();
    Observed {
        skill_id: actual.skill_id,
        weapon_item: actual.weapon_item,
        attack_times_ms: actual.attack_times_ms,
        minimum_damage: actual.minimum_damage,
        maximum_damage: actual.maximum_damage,
    }
}
#[test]
fn original_staff_roundtrip_preserves_custody_and_uses_cleared_damage_not_initial_unarmed_defaults()
{
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    let original = *w.state.combat.as_ref().unwrap().equipment.item(15).unwrap();
    let original_id = w
        .profile
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap()
        .equipment_instance(original.origin)
        .unwrap();
    let frontier = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .next_item;
    let before = operands(&w);
    assert_eq!(
        (before.skill_id, before.weapon_item, before.attack_times_ms),
        (136, Some(900002), [2500, 2000])
    );
    let before_timers = w.state.combat.as_ref().unwrap().melee_timers.remaining_ms();
    step(&mut w, movement(15, 23));
    assert!(w
        .state
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(15)
        .is_none());
    let q = w.state.quest_progression.as_ref().unwrap();
    assert_eq!(
        (
            q.inventory.as_ref().unwrap().slots[&23].id,
            q.inventory.as_ref().unwrap().slots[&23].item
        ),
        (original_id, 900002)
    );
    assert_eq!(q.inventory.as_ref().unwrap().next_item, frontier);
    let detached = operands(&w);
    assert_eq!(
        (
            detached.skill_id,
            detached.weapon_item,
            detached.attack_times_ms
        ),
        (162, None, [2000, 2000])
    );
    // Source _ApplyItemBonuses(false) cleared both weapon damage rows. The
    // never-equipped initializer's unarmed 1..2 must not be substituted here.
    assert_eq!(detached.minimum_damage, detached.maximum_damage);
    assert!(
        f32::from_bits(detached.minimum_damage.bits())
            < f32::from_bits(before.minimum_damage.bits())
    );
    assert_eq!(
        w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
        before_timers.map(|n| n.saturating_sub(w.profile.tick_ms))
    );
    reopen(&w);
    let mana = w.state.resources.caster.mana;
    step(&mut w, movement(23, 15));
    assert_eq!(
        w.state.combat.as_ref().unwrap().equipment.item(15),
        Some(&original)
    );
    let equipped = operands(&w);
    assert_eq!(
        (
            equipped.skill_id,
            equipped.weapon_item,
            equipped.attack_times_ms
        ),
        (136, Some(900002), [2500, 2000])
    );
    assert_eq!(
        (equipped.minimum_damage, equipped.maximum_damage),
        (before.minimum_damage, before.maximum_damage)
    );
    assert_eq!(w.state.resources.caster.mana, mana);
    assert_eq!(
        w.state
            .quest_progression
            .as_ref()
            .unwrap()
            .inventory
            .as_ref()
            .unwrap()
            .next_item,
        frontier
    );
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn actual_melee_after_detach_and_after_reequip_uses_unarmed_or_staff_and_replays() {
    for equipped in [false, true] {
        let mut w = world();
        let initial = w.checkpoint().unwrap();
        step(&mut w, movement(15, 23));
        if equipped {
            step(&mut w, movement(23, 15));
        }
        let selected = operands(&w);
        assert_eq!(selected.skill_id, if equipped { 136 } else { 162 });
        let target = w.state.npcs[&2].id;
        let health = w.state.npcs[&2].health;
        step(&mut w, Command::AttackStart { target });
        for _ in 0..100 {
            if w.records().iter().any(|r| {
                r.events.iter().any(
                    |event| matches!(event, Event::PlayerMelee { effective, .. } if *effective > 0),
                )
            }) {
                break;
            }
            w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
                .unwrap();
        }
        assert!(w.records().iter().any(|r| r.events.iter().any(|event| matches!(event, Event::PlayerMelee { target: actual, effective, .. } if *actual == target && *effective > 0))));
        assert!(w.state.npcs[&2].health < health);
        assert_eq!(operands(&w).skill_id, if equipped { 136 } else { 162 });
        reopen(&w);
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    }
}
#[test]
fn actual_combat_mainhand_switch_is_explicitly_unsupported_and_rolls_back_without_armor_error() {
    let mut w = world();
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
    let before = w.checkpoint().unwrap();
    let last = w.last_tick().unwrap().clone();
    let command = synthetic_quest_fixture::command(&w, movement(15, 23));
    assert!(matches!(
        w.step(
            w.state.tick + 1,
            Some(command),
            &mut EquipmentAuthority::default()
        ),
        Err(NativeError::UnsupportedMechanic(_))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(w.last_tick().unwrap(), &last);
    reopen(&w);
}
#[test]
fn actual_dead_player_swap_keeps_source_error38_not_combat_host_error() {
    let mut w = NativeWorld::synthetic_component_test(
        synthetic_equipment_fixture::lethal_weapon_profile(),
        [1; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap();
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
    let inventory = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .clone();
    let equipment = w.state.combat.as_ref().unwrap().equipment.snapshot();
    step(&mut w, movement(15, 23));
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|event| matches!(event, Event::EquipmentMoveRefused { reason: 38, .. })));
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().inventory,
        inventory
    );
    assert_eq!(
        w.state.combat.as_ref().unwrap().equipment.snapshot(),
        equipment
    );
    assert!(!w.state.player_alive());
    reopen(&w);
}
#[test]
fn rehashed_mainhand_history_cannot_substitute_equipped_or_damage_cache_observations() {
    let mut w = world();
    step(&mut w, movement(15, 23));
    for fault in 0..3 {
        let mut cp = w.checkpoint().unwrap();
        let mut value = serde_json::to_value(&cp.state).unwrap();
        match fault {
            0 => value["combat"]["recipe"]["inactive_slots"] = serde_json::json!([]),
            1 => {
                let calls = value["quest_progression"]["derived_calls"]
                    .as_array_mut()
                    .unwrap();
                let movement = calls
                    .iter_mut()
                    .find_map(|call| call.get_mut("ItemMembership"))
                    .unwrap();
                movement["after_inputs"]["weapons"][0]["inactive"] = serde_json::json!(false);
            }
            2 => {
                let calls = value["quest_progression"]["derived_calls"]
                    .as_array_mut()
                    .unwrap();
                let movement = calls
                    .iter_mut()
                    .find_map(|call| call.get_mut("ItemMembership"))
                    .unwrap();
                movement["after_recipe"]["inactive_slots"] = serde_json::json!([]);
            }
            _ => unreachable!(),
        }
        assert_ne!(value, serde_json::to_value(&cp.state).unwrap());
        cp.state = serde_json::from_value(value).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v77\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v77\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err());
    }
}
fn cast_membership_history(detach_during_casting: bool) {
    let mut w = world();
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
        .any(|cast| matches!(cast.execution.state(), spell::CastState::Casting { .. })));
    if detach_during_casting {
        step(&mut w, movement(15, 23));
        assert!(w
            .state
            .casts
            .values()
            .any(|cast| matches!(cast.execution.state(), spell::CastState::Casting { .. })));
        reopen(&w);
    }
    for _ in 0..40 {
        if w.state
            .casts
            .values()
            .any(|cast| matches!(cast.execution.state(), spell::CastState::Traveling { .. }))
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
        .any(|cast| matches!(cast.execution.state(), spell::CastState::Traveling { .. })));
    assert!(!w.state.combat.as_ref().unwrap().in_combat);
    let before_timer = w.state.combat.as_ref().unwrap().melee_timers.remaining_ms();
    let before_interval = operands(&w).attack_times_ms[0];
    // The actual source cast release, not a fabricated timer write, reset BASE
    // before this frame's elapsed countdown. Release used that moment's weapon.
    assert_eq!(before_timer[0], before_interval - w.profile.tick_ms);
    reopen(&w);
    if detach_during_casting {
        step(&mut w, movement(23, 15));
        let receipt = w
            .last_tick()
            .unwrap()
            .events
            .iter()
            .find_map(|event| match event {
                Event::EquipmentMoved(receipt) => Some(receipt),
                _ => None,
            })
            .unwrap();
        assert!(receipt.context.generic_slot.is_some());
        assert!(w
            .state
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(15)
            .is_some());
    } else {
        assert!(before_timer[0] > 2000);
        step(&mut w, movement(15, 23));
        assert_eq!(operands(&w).attack_times_ms[0], 2000);
        assert!(w
            .state
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(15)
            .is_none());
    }
    assert_eq!(
        w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
        before_timer.map(|value| value.saturating_sub(w.profile.tick_ms))
    );
    if !detach_during_casting {
        assert!(
            w.state.combat.as_ref().unwrap().melee_timers.remaining_ms()[0] > 2000,
            "removing a weapon changes attack-time field, not the running countdown"
        );
    }
    reopen(&w);
    for _ in 0..40 {
        if w.state.npcs[&2].health < health {
            break;
        }
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
    }
    assert!(w.state.npcs[&2].health < health);
    assert!(w
        .records()
        .iter()
        .any(|record| record.events.iter().any(|event| matches!(
            event,
            Event::Spell {
                event: spell::SpellEvent::DirectDamage { .. },
                ..
            }
        ))));
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn casting_staff_detach_then_traveling_reequip_replays_each_source_prefix() {
    cast_membership_history(true);
}
#[test]
fn traveling_staff_detach_preserves_source_release_countdown_above_new_attack_interval() {
    cast_membership_history(false);
}
