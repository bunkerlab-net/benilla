// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use quest_progression::equipment_movement::EquipmentMoveOperation;
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
fn detach(w: &mut NativeWorld, slot: u8) {
    step(
        w,
        Command::EquipmentMove(EquipmentMoveRequest {
            source: slot,
            destination: 23,
        }),
    );
}
fn auto(w: &mut NativeWorld, source: u8) {
    step(w, Command::EquipmentAutoEquip { source });
}
fn reopen(w: &NativeWorld) {
    NativeWorld::restore_synthetic_component_test(w.profile.clone(), w.checkpoint().unwrap())
        .unwrap();
}
#[test]
fn source_auto_equip_relocated_original_replays_and_does_not_refill() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    let original = *w.state.combat.as_ref().unwrap().equipment.item(4).unwrap();
    detach(&mut w, 4);
    step(
        &mut w,
        Command::InventoryMove(InventoryMoveRequest {
            source: 23,
            destination: 24,
        }),
    );
    let mana = w.state.resources.caster.mana;
    let frontier = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .next_item;
    auto(&mut w, 24);
    assert_eq!(
        w.state.combat.as_ref().unwrap().equipment.item(4),
        Some(&original)
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
    let receipt = w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .find_map(|e| {
            if let Event::EquipmentMoved(r) = e {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(receipt.context.operation, EquipmentMoveOperation::AutoEquip);
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn auto_equip_cast_refusal_then_retained_traveling_slot_success() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    detach(&mut w, 4);
    let target = w.state.npcs[&2].id;
    step(
        &mut w,
        Command::Cast {
            ability: 133,
            target,
        },
    );
    auto(&mut w, 23);
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::EquipmentMoveRefused { reason: 39, .. })));
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
    auto(&mut w, 23);
    let receipt = w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .find_map(|e| {
            if let Event::EquipmentMoved(r) = e {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert!(receipt.context.generic_slot.is_some());
    assert_eq!(receipt.context.operation, EquipmentMoveOperation::AutoEquip);
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn auto_equip_empty_source_and_source_owned_scope_refusals() {
    let mut w = world();
    let calls = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .derived_calls
        .len();
    auto(&mut w, 38);
    assert_eq!(
        w.state
            .quest_progression
            .as_ref()
            .unwrap()
            .derived_calls
            .len(),
        calls
    );
    {
        let profile = synthetic_equipment_fixture::profile_with_armor_type(11);
        let slot = 10;
        let mut w = NativeWorld::synthetic_component_test(
            profile,
            [1; 16],
            synthetic_fixture::player(),
            42,
        )
        .unwrap();
        detach(&mut w, slot);
        let cp = w.checkpoint().unwrap();
        let command =
            synthetic_quest_fixture::command(&w, Command::EquipmentAutoEquip { source: 23 });
        assert!(matches!(
            w.step(
                w.state.tick + 1,
                Some(command),
                &mut EquipmentAuthority::default()
            ),
            Err(NativeError::UnsupportedMechanic(_))
        ));
        assert_eq!(w.checkpoint().unwrap(), cp);
    }
}
#[test]
fn operation_is_required_and_auto_cannot_relabel_an_unequip() {
    let mut w = world();
    detach(&mut w, 4);
    for missing in [false, true] {
        let mut cp = w.checkpoint().unwrap();
        let mut value = serde_json::to_value(&cp.state).unwrap();
        let context = &mut value["quest_progression"]["derived_calls"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap()["ItemMembership"]["movement"]["context"];
        if missing {
            context.as_object_mut().unwrap().remove("operation");
            assert!(serde_json::from_value::<persistence::StateCheckpoint>(value).is_err());
            continue;
        }
        context["operation"] = serde_json::json!("AutoEquip");
        cp.state = serde_json::from_value(value).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v76\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v76\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err());
    }
}

#[test]
fn auto_equip_offhand_dependency_is_closed_by_the_real_initializer() {
    use benilla_sim::starter_init as init;
    let (_, mut input, _) = synthetic_loot_fixture::setup();
    let row = input.items.iter_mut().find(|item| item.class == 4).unwrap();
    let id = row.id;
    row.inventory_type = 23;
    assert!(
        matches!(init::initialize_mage(&input), Err(init::InitError::Unsupported { domain: "initial item placement", id: rejected, detail: 23 }) if rejected == id)
    );
    // This is an unsupported source input, not a fabricated live offhand world.
}
#[test]
fn auto_equip_after_actual_lethal_damage_refuses_without_mutating_custody() {
    let mut w = NativeWorld::synthetic_component_test(
        synthetic_equipment_fixture::profile_with_lethal(true),
        [1; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap();
    detach(&mut w, 4);
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
    let before = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .clone();
    auto(&mut w, 23);
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::EquipmentMoveRefused { reason: 38, .. })));
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().inventory,
        before
    );
    reopen(&w);
}
