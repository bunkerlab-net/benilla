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
fn detach_late(w: &mut NativeWorld) {
    step(
        w,
        Command::EquipmentMove(EquipmentMoveRequest {
            source: 4,
            destination: 23,
        }),
    );
    step(
        w,
        Command::InventoryMove(InventoryMoveRequest {
            source: 23,
            destination: 38,
        }),
    );
}
fn autostore(w: &mut NativeWorld, source: u8) {
    step(
        w,
        Command::InventoryAutoStore(InventoryAutoStoreRequest { source }),
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
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
}
#[test]
fn original_armor_autostore_first_empty_no_stats_and_replay() {
    let mut w = world();
    detach_late(&mut w);
    let initial = w.checkpoint().unwrap();
    let prefix = w.records().len();
    let before = inventory(&w).clone();
    let combat = w.state.combat.as_ref().unwrap().clone();
    let q = w.state.quest_progression.as_ref().unwrap();
    let derived = q.derived.clone();
    let quests = q.quests.clone();
    autostore(&mut w, 38);
    assert_eq!(inventory(&w).slots[&23], before.slots[&38]);
    assert!(!inventory(&w).slots.contains_key(&38));
    assert_eq!(inventory(&w).next_item, before.next_item);
    assert_eq!(inventory(&w).equipped, before.equipped);
    assert_eq!(
        w.state.combat.as_ref().unwrap().recipe.snapshot(),
        combat.recipe.snapshot()
    );
    assert_eq!(
        w.state.combat.as_ref().unwrap().equipment.snapshot(),
        combat.equipment.snapshot()
    );
    let q = w.state.quest_progression.as_ref().unwrap();
    assert_eq!(q.derived, derived);
    assert_eq!(q.quests, quests);
    assert!(matches!(
        q.derived_calls.last(),
        Some(DerivedCall::ItemAutoStore { .. })
    ));
    assert!(!w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::EquipmentMoved(_)
            | Event::ItemAcquired(_)
            | Event::InventoryStored(_)
            | Event::QuestProgression(_)
    )));
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(&w.records()[prefix..]).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    step(&mut w, Command::EquipmentAutoEquip { source: 23 });
    assert!(w.state.combat.as_ref().unwrap().equipment.item(4).is_some());
    reopen(&w);
}
#[test]
fn original_armor_autostore_while_casting_combat_and_dead() {
    for lethal in [false, true] {
        let mut w = NativeWorld::synthetic_component_test(
            synthetic_equipment_fixture::profile_with_lethal(lethal),
            [1; 16],
            synthetic_fixture::player(),
            42,
        )
        .unwrap();
        detach_late(&mut w);
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
        autostore(&mut w, 38);
        assert!(inventory(&w).slots.contains_key(&23));
        step(
            &mut w,
            Command::InventoryMove(InventoryMoveRequest {
                source: 23,
                destination: 38,
            }),
        );
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
        let before = inventory(&w).clone();
        autostore(&mut w, 38);
        assert_eq!(inventory(&w).slots[&23], before.slots[&38]);
        assert_eq!(inventory(&w).next_item, before.next_item);
        if lethal {
            assert!(!w.state.player_alive());
        }
        reopen(&w);
        let calls = w
            .state
            .quest_progression
            .as_ref()
            .unwrap()
            .derived_calls
            .iter()
            .filter(|call| {
                matches!(
                    call,
                    DerivedCall::ItemAutoStore { .. }
                        | DerivedCall::ItemMembership { .. }
                        | DerivedCall::ItemRelocation { .. }
                )
            })
            .count();
        let before_noop = inventory(&w).clone();
        autostore(&mut w, 23);
        assert_eq!(inventory(&w), &before_noop);
        assert_eq!(
            w.state
                .quest_progression
                .as_ref()
                .unwrap()
                .derived_calls
                .iter()
                .filter(|call| matches!(
                    call,
                    DerivedCall::ItemAutoStore { .. }
                        | DerivedCall::ItemMembership { .. }
                        | DerivedCall::ItemRelocation { .. }
                ))
                .count(),
            calls
        );
        assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(e, Event::InventoryAutoStored(r) if r.outcome == benilla_sim::inventory::AutoStoreOutcome::Unchanged && r.views.is_empty())));
        reopen(&w);
    }
}
#[test]
fn original_armor_autostore_rehashed_custody_forgery_is_rejected() {
    let mut w = world();
    detach_late(&mut w);
    autostore(&mut w, 38);
    for fault in 0..4 {
        let mut cp = w.checkpoint().unwrap();
        let mut state = serde_json::to_value(&cp.state).unwrap();
        let next_sequence = state["next_sequence"].clone();
        let m = &mut state["quest_progression"]["derived_calls"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap()["ItemAutoStore"]["movement"];
        match fault {
            0 => m["sequence"] = next_sequence,
            1 => {
                let n = m["movement"]["after"]["next_item"].as_u64().unwrap();
                m["movement"]["after"]["next_item"] = serde_json::json!(n + 1);
            }
            2 => m["origin"]["genesis_slot"] = serde_json::json!(5),
            _ => m["movement"]["destinations"][0]["slot"] = serde_json::json!(24),
        }
        cp.state = serde_json::from_value(state).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v76\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v76\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err());
    }
}
