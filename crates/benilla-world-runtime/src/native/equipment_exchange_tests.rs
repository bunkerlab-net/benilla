// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual occupied-backpack commands; source-authored origins, no new identities.
use super::*;
use synthetic_equipment_fixture::EquipmentAuthority;
fn world(profile: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile, [1; 16], synthetic_fixture::player(), 42)
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
fn move_item(w: &mut NativeWorld, source: u8, destination: u8) {
    step(
        w,
        Command::InventoryMove(InventoryMoveRequest {
            source,
            destination,
        }),
    );
}
fn detach(w: &mut NativeWorld, source: u8, destination: u8) {
    step(
        w,
        Command::EquipmentMove(EquipmentMoveRequest {
            source,
            destination,
        }),
    );
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
fn reopen(w: &NativeWorld) {
    let cp = w.checkpoint().unwrap();
    let reopened =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp.clone()).unwrap();
    assert_eq!(reopened.checkpoint().unwrap(), cp);
}
fn exchange(w: &mut NativeWorld, source: u8, destination: u8) {
    let before = inventory(w).clone();
    let combat = w.state.combat.as_ref().unwrap();
    let idle_timer =
        (!combat.in_combat && w.state.casts.is_empty()).then(|| combat.melee_timers.remaining_ms());
    let recipe = combat.recipe.snapshot();
    let equipment = combat.equipment.snapshot();
    let stats = combat.stats.clone();
    let derived = w.state.quest_progression.as_ref().unwrap().derived.clone();
    move_item(w, source, destination);
    let after = inventory(w);
    assert_eq!(after.slots[&source], before.slots[&destination]);
    assert_eq!(after.slots[&destination], before.slots[&source]);
    assert_eq!(after.next_item, before.next_item);
    assert_eq!(after.equipped, before.equipped);
    assert_eq!(after.slots.len(), before.slots.len());
    assert_eq!(w.state.combat.as_ref().unwrap().recipe.snapshot(), recipe);
    assert_eq!(
        w.state.combat.as_ref().unwrap().equipment.snapshot(),
        equipment
    );
    assert_eq!(w.state.combat.as_ref().unwrap().stats, stats);
    assert_eq!(w.state.quest_progression.as_ref().unwrap().derived, derived);
    if let Some(timer) = idle_timer {
        assert_eq!(
            w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
            timer.map(|remaining| remaining.saturating_sub(w.profile.tick_ms))
        );
    }
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::InventoryMoved(r) if r.result.protocol()==0)));
    reopen(w);
}
fn replay_suffix(w: &NativeWorld, checkpoint: Checkpoint, prefix: usize) {
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint).unwrap();
    replay.replay(&w.records()[prefix..]).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn registered_and_ordinary_same_entry_exchange_in_both_command_directions() {
    let mut w = world(synthetic_equipment_fixture::profile_with_duplicate_armor());
    move_item(&mut w, 23, 24);
    detach(&mut w, 4, 23);
    let registry = w
        .profile
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap();
    assert!(registry
        .equipment_origin(inventory(&w).slots[&23].id)
        .is_some());
    assert!(registry
        .equipment_origin(inventory(&w).slots[&24].id)
        .is_none());
    assert_eq!(inventory(&w).slots[&23].item, inventory(&w).slots[&24].item);
    let initial = w.checkpoint().unwrap();
    let prefix = w.records().len();
    exchange(&mut w, 23, 24); // registered source
    exchange(&mut w, 23, 24); // ordinary source, registered destination
    replay_suffix(&w, initial, prefix);
}
#[test]
fn two_originals_exchange_and_inactive_mainhand_cache_remains_source_unchanged() {
    for (profile, second_slot) in [
        (synthetic_equipment_fixture::profile_with_two_originals(), 5),
        (synthetic_equipment_fixture::weapon_profile(), 15),
    ] {
        let mut w = world(profile);
        detach(&mut w, 4, 23);
        detach(&mut w, second_slot, 24);
        let initial = w.checkpoint().unwrap();
        let prefix = w.records().len();
        exchange(&mut w, 23, 24);
        exchange(&mut w, 24, 23);
        replay_suffix(&w, initial, prefix);
        assert!(w
            .state
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(second_slot)
            .is_none());
    }
}
#[test]
fn occupied_exchange_allows_actual_casting_and_combat_but_refuses_actual_dead_player() {
    for lethal in [false, true] {
        let profile = if lethal {
            synthetic_equipment_fixture::lethal_weapon_profile()
        } else {
            synthetic_equipment_fixture::weapon_profile()
        };
        let mut w = world(profile);
        detach(&mut w, 4, 23);
        detach(&mut w, 15, 24);
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
        exchange(&mut w, 23, 24);
        for _ in 0..150 {
            if if lethal {
                !w.state.player_alive()
            } else {
                w.state.combat.as_ref().unwrap().in_combat
            } {
                break;
            }
            w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
                .unwrap();
        }
        if lethal {
            assert!(!w.state.player_alive());
            let before = inventory(&w).clone();
            let custody_before = w
                .state
                .quest_progression
                .as_ref()
                .unwrap()
                .derived_calls
                .iter()
                .filter(|c| matches!(c, quest_progression::DerivedCall::ItemRelocation { .. }))
                .count();
            move_item(&mut w, 23, 24);
            assert_eq!(inventory(&w), &before);
            assert_eq!(w.state.health, 0);
            assert_eq!(
                w.state
                    .quest_progression
                    .as_ref()
                    .unwrap()
                    .derived_calls
                    .iter()
                    .filter(|c| matches!(c, quest_progression::DerivedCall::ItemRelocation { .. }))
                    .count(),
                custody_before
            );
            assert!(w
                .last_tick()
                .unwrap()
                .events
                .iter()
                .any(|e| matches!(e, Event::InventoryMoved(r) if r.result.protocol()==38)));
            reopen(&w);
        } else {
            assert!(w.state.combat.as_ref().unwrap().in_combat);
            // Compare stable custody/caches, not elapsed timers or genuine defense skill callbacks.
            let before = inventory(&w).clone();
            move_item(&mut w, 24, 23);
            assert_eq!(inventory(&w).slots[&23], before.slots[&24]);
            assert_eq!(inventory(&w).slots[&24], before.slots[&23]);
            assert_eq!(inventory(&w).next_item, before.next_item);
            reopen(&w);
        }
    }
}

#[test]
fn rehashed_checkpoint_cannot_omit_registered_counterpart_or_keep_its_old_placement() {
    let mut w = world(synthetic_equipment_fixture::profile_with_two_originals());
    detach(&mut w, 4, 23);
    detach(&mut w, 5, 24);
    let counterpart = inventory(&w).slots[&24].id;
    exchange(&mut w, 23, 24);
    for fault in ["omit", "origin", "old_placement"] {
        let mut cp = w.checkpoint().unwrap();
        let mut json = serde_json::to_value(&cp.state).unwrap();
        if fault == "old_placement" {
            // Keep valid whole item facts but move the counterpart away from the
            // placement established by the last source custody transition.
            let slots = json["quest_progression"]["inventory"]["slots"]
                .as_object_mut()
                .unwrap();
            let counterpart_item = slots.remove("23").unwrap();
            slots.insert("25".into(), counterpart_item);
        } else {
            let calls = json["quest_progression"]["derived_calls"]
                .as_array_mut()
                .unwrap();
            let movement = calls
                .iter_mut()
                .rev()
                .find_map(|c| c.get_mut("ItemRelocation"))
                .unwrap();
            let registered = movement["movement"]["registered"].as_object_mut().unwrap();
            assert_eq!(registered.len(), 2);
            let key = counterpart.to_string();
            assert!(registered.contains_key(&key));
            if fault == "omit" {
                registered.remove(&key);
            } else {
                registered.get_mut(&key).unwrap()["genesis_slot"] = serde_json::json!(4);
            }
        }
        cp.state = serde_json::from_value(json).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v79\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v79\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err(),
            "{fault}"
        );
    }
}
