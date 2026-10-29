// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use synthetic_equipment_fixture::{world, EquipmentAuthority};
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
            source: 4,
            destination: 23,
        }),
    );
}
fn relocate(w: &mut NativeWorld, source: u8, destination: u8) {
    step(
        w,
        Command::InventoryMove(InventoryMoveRequest {
            source,
            destination,
        }),
    );
}
fn reopen(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
#[test]
fn custody_only_relocation_keeps_recipe_and_replays_then_reequips() {
    let mut w = world();
    detach(&mut w);
    let initial = w.checkpoint().unwrap();
    let prefix = w.records().len();
    let combat = w.state.combat.as_ref().unwrap().clone();
    let q = w.state.quest_progression.as_ref().unwrap();
    let derived = q.derived.clone();
    let inventory = q.inventory.as_ref().unwrap().clone();
    for (from, to) in [(23, 24), (24, 38), (38, 23)] {
        relocate(&mut w, from, to);
        let current = w.state.combat.as_ref().unwrap();
        assert_eq!(current.recipe.snapshot(), combat.recipe.snapshot());
        assert_eq!(current.equipment.snapshot(), combat.equipment.snapshot());
        let q = w.state.quest_progression.as_ref().unwrap();
        assert_eq!(q.derived, derived);
        let current = q.inventory.as_ref().unwrap();
        assert_eq!(current.next_item, inventory.next_item);
        assert_eq!(current.equipped, inventory.equipped);
        assert_eq!(current.slots[&to], inventory.slots[&23]);
        assert!(!current.slots.contains_key(&from));
        reopen(&w);
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(&w.records()[prefix..]).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    step(
        &mut w,
        Command::EquipmentMove(EquipmentMoveRequest {
            source: 23,
            destination: 4,
        }),
    );
    assert!(w.state.combat.as_ref().unwrap().equipment.item(4).is_some());
    reopen(&w);
}
#[test]
fn casting_and_combat_allow_relocation_but_actual_death_refuses() {
    for lethal in [false, true] {
        let mut w = NativeWorld::synthetic_component_test(
            synthetic_equipment_fixture::profile_with_lethal(lethal),
            [1; 16],
            synthetic_fixture::player(),
            42,
        )
        .unwrap();
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
        relocate(&mut w, 23, 24);
        assert!(w
            .state
            .quest_progression
            .as_ref()
            .unwrap()
            .inventory
            .as_ref()
            .unwrap()
            .slots
            .contains_key(&24));
        reopen(&w);
        for _ in 0..100 {
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
        } else {
            assert!(w.state.combat.as_ref().unwrap().in_combat);
        }
        relocate(&mut w, 24, 25);
        let inventory = w
            .state
            .quest_progression
            .as_ref()
            .unwrap()
            .inventory
            .as_ref()
            .unwrap();
        assert!(inventory.slots.contains_key(&if lethal { 24 } else { 25 }));
        if lethal {
            assert!(w
                .last_tick()
                .unwrap()
                .events
                .iter()
                .any(|e| matches!(e, Event::InventoryMoved(m) if m.result.protocol()==38)));
        }
        reopen(&w);
    }
}
#[test]
fn relocation_noop_and_rehashed_custody_forgery() {
    let mut w = world();
    detach(&mut w);
    let count = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .derived_calls
        .len();
    relocate(&mut w, 23, 23);
    assert_eq!(
        w.state
            .quest_progression
            .as_ref()
            .unwrap()
            .derived_calls
            .len(),
        count
    );
    relocate(&mut w, 23, 24);
    for fault in 0..3 {
        let mut cp = w.checkpoint().unwrap();
        let mut state = serde_json::to_value(&cp.state).unwrap();
        let next_sequence = state["next_sequence"].clone();
        let movement = &mut state["quest_progression"]["derived_calls"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap()["ItemRelocation"]["movement"];
        match fault {
            0 => movement["sequence"] = next_sequence,
            1 => {
                let n = movement["movement"]["after"]["next_item"].as_u64().unwrap();
                movement["movement"]["after"]["next_item"] = serde_json::json!(n + 1);
            }
            _ => movement["origin"]["genesis_slot"] = serde_json::json!(5),
        }
        cp.state = serde_json::from_value(state).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v71\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v71\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err());
    }
}
