// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use synthetic_equipment_fixture::{world, EquipmentAuthority};
fn step(w: &mut NativeWorld, c: Command) {
    let input = synthetic_quest_fixture::command(w, c);
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
fn reopen(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
#[test]
fn original_armor_roundtrip_preserves_handle_and_replays_checkpoint_suffix() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    let original = *w.state.combat.as_ref().unwrap().equipment.item(4).unwrap();
    let g = w
        .profile
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap();
    let id = g.equipment_instance(original.origin).unwrap();
    let frontier = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .next_item;
    let max_mana = w.state.combat.as_ref().unwrap().stats.max_mana;
    step(&mut w, movement(4, 23));
    assert!(w.state.combat.as_ref().unwrap().equipment.item(4).is_none());
    assert!(w.state.combat.as_ref().unwrap().stats.max_mana < max_mana);
    let carried = &w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .slots[&23];
    assert_eq!((carried.id, carried.item), (id, original.item_id));
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::EquipmentMoved(_))));
    let clamped_mana = w.state.resources.caster.mana;
    let checkpoint = w.checkpoint().unwrap();
    let prefix = w.records().len();
    let mut restored = reopen(&w);
    step(&mut w, movement(23, 4));
    assert_eq!(
        w.state.combat.as_ref().unwrap().equipment.item(4),
        Some(&original)
    );
    assert_eq!(w.state.combat.as_ref().unwrap().stats.max_mana, max_mana);
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
    assert_eq!(w.state.resources.caster.mana, clamped_mana);
    restored.replay(&w.records()[prefix..]).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    assert_ne!(checkpoint, w.checkpoint().unwrap());
}
#[test]
fn casting_allows_unequip_but_refuses_equip_without_reinterpreting_cast_history() {
    let mut w = world();
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
        .any(|cast| matches!(cast.execution.state(), spell::CastState::Casting { .. })));
    assert!(!w.state.combat.as_ref().unwrap().in_combat);
    step(&mut w, movement(4, 23));
    assert!(w.state.combat.as_ref().unwrap().equipment.item(4).is_none());
    reopen(&w);
    step(&mut w, movement(23, 4));
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::EquipmentMoveRefused { reason: 39, .. })));
    assert!(w.state.combat.as_ref().unwrap().equipment.item(4).is_none());
    reopen(&w);
    let mut impact = false;
    for _ in 0..80 {
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
        impact |= w.last_tick().unwrap().events.iter().any(|event| {
            matches!(
                event,
                Event::Spell {
                    event: spell::SpellEvent::DirectDamage { .. },
                    ..
                }
            )
        });
        if impact {
            break;
        }
    }
    assert!(impact, "prepared cast must release and hit after unequip");
    reopen(&w);
}
#[test]
fn rehashed_custody_and_membership_snapshot_forgery_is_rejected() {
    let mut w = world();
    step(&mut w, movement(4, 23));
    for fault in 0..3 {
        let mut cp = w.checkpoint().unwrap();
        let mut json = serde_json::to_value(&cp.state).unwrap();
        match fault {
            0 => json["combat"]["recipe"]["inactive_slots"] = serde_json::json!([]),
            1 => {
                json["quest_progression"]["inventory"]["slots"]["23"]["id"] = serde_json::json!(999)
            }
            _ => {
                json["quest_progression"]["inventory"]["slots"]
                    .as_object_mut()
                    .unwrap()
                    .remove("23");
            }
        }
        cp.state = serde_json::from_value(json).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v71\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v71\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err());
    }
}

#[test]
fn same_tick_roundtrip_empty_source_and_cross_consumer_holds() {
    let mut w = world();
    let before = w.state.combat.as_ref().unwrap().equipment.snapshot();
    let a = synthetic_quest_fixture::command(&w, movement(4, 23));
    let mut b = synthetic_quest_fixture::command(&w, movement(23, 4));
    b.input.sequence += 1;
    w.step_batch(1, vec![a, b], &mut EquipmentAuthority::default())
        .unwrap();
    assert_eq!(
        w.state.combat.as_ref().unwrap().equipment.snapshot(),
        before
    );
    assert_eq!(
        w.last_tick()
            .unwrap()
            .events
            .iter()
            .filter(|e| matches!(e, Event::EquipmentMoved(_)))
            .count(),
        2
    );
    reopen(&w);
    step(&mut w, movement(5, 24));
    assert!(!w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::EquipmentMoved(_))));
    step(&mut w, movement(4, 23));
    // AutoStore now has its own bounded custody owner; same-position is a
    // source no-op, while unrelated registered-item consumers stay closed.
    let prior_inventory = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .clone();
    let prior_calls = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .derived_calls
        .clone();
    step(
        &mut w,
        Command::InventoryAutoStore(InventoryAutoStoreRequest { source: 23 }),
    );
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().inventory,
        prior_inventory
    );
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().derived_calls,
        prior_calls
    );
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(e,
        Event::InventoryAutoStored(r) if r.outcome == benilla_sim::inventory::AutoStoreOutcome::Unchanged && r.views.is_empty()
    )));
    reopen(&w);
    for c in [
        Command::InventorySplit(InventorySplitRequest {
            source: 23,
            destination: 24,
            count: 1,
        }),
        Command::InventoryDestroy(InventoryDestroyRequest { slot: 23, count: 0 }),
    ] {
        let before = w.checkpoint().unwrap();
        let c = synthetic_quest_fixture::command(&w, c);
        assert!(w
            .step(
                w.state.tick + 1,
                Some(c),
                &mut EquipmentAuthority::default()
            )
            .is_err());
        assert_eq!(w.checkpoint().unwrap(), before);
    }
    let mut unsupported = synthetic_loot_fixture::world();
    let before = unsupported.checkpoint().unwrap();
    let c = synthetic_quest_fixture::command(&unsupported, movement(4, 23));
    assert!(unsupported
        .step(
            1,
            Some(c),
            &mut synthetic_loot_fixture::LootAuthority::default()
        )
        .is_err());
    assert_eq!(unsupported.checkpoint().unwrap(), before);
}
#[test]
fn actual_combat_and_lethal_death_refuse_armor_movement() {
    for lethal in [false, true] {
        let mut w = NativeWorld::synthetic_component_test(
            synthetic_equipment_fixture::profile_with_lethal(lethal),
            [1; 16],
            synthetic_fixture::player(),
            42,
        )
        .unwrap();
        let target = w.state.npcs[&2].id;
        step(
            &mut w,
            Command::Cast {
                ability: 133,
                target,
            },
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
        let before = w.state.combat.as_ref().unwrap().equipment.snapshot();
        step(&mut w, movement(4, 23));
        let expected = if lethal { 38 } else { 60 };
        assert!(w
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::EquipmentMoveRefused { reason,.. } if *reason==expected)));
        assert_eq!(
            w.state.combat.as_ref().unwrap().equipment.snapshot(),
            before
        );
        reopen(&w);
    }
}

#[test]
fn actual_unarmed_mainhand_uses_inactive_armor_history_and_reopens() {
    let p = synthetic_equipment_fixture::mainhand_profile();
    let mut w =
        NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42).unwrap();
    let initial = w.checkpoint().unwrap();
    step(&mut w, movement(4, 23));
    let target = w.state.npcs[&2].id;
    let health = w.state.npcs[&2].health;
    step(&mut w, Command::AttackStart { target });
    let mut hit = w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::PlayerMelee { .. }));
    for _ in 0..60 {
        if hit && w.state.npcs[&2].health < health {
            break;
        }
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
        hit |= w
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::PlayerMelee { .. }));
    }
    assert!(hit && w.state.npcs[&2].health < health);
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
    assert!(w
        .state
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(15)
        .is_none());
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}

#[test]
fn oversized_aggregate_membership_history_fails_before_semantic_replay() {
    let mut w = world();
    step(&mut w, movement(4, 23));
    let mut checkpoint = w.checkpoint().unwrap();
    let mut encoded = serde_json::to_value(&checkpoint.state).unwrap();
    let q = &mut encoded["quest_progression"];
    let call = q["derived_calls"].as_array().unwrap()[0].clone();
    assert!(call.get("ItemMembership").is_some());
    let call_bytes = serde_json::to_vec(&call).unwrap().len();
    let count = (MAX_BYTES / 2) / call_bytes + 1;
    assert!(
        count <= 4096,
        "fixture cannot exercise bytes before call-count limit"
    );
    // Deliberately invalid duplicated sequences, not reachable gameplay. The
    // aggregate byte preflight must win before those semantic replay failures.
    q["derived_calls"] = serde_json::Value::Array(vec![call; count]);
    let aggregate = serde_json::to_vec(&(&q["training"]["calls"], &q["derived_calls"]))
        .unwrap()
        .len();
    assert!(aggregate > MAX_BYTES / 2);
    assert!(serde_json::to_vec(&encoded).unwrap().len() < MAX_BYTES);
    checkpoint.state = serde_json::from_value(encoded).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v71\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v71\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    assert!(matches!(
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint),
        Err(NativeError::TooLarge)
    ));
}

#[test]
fn traveling_cast_allows_reequip_and_retains_release_history_through_impact() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
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
    step(&mut w, movement(4, 23));
    let mut traveling = false;
    for _ in 0..40 {
        traveling = w
            .state
            .casts
            .values()
            .any(|c| matches!(c.execution.state(), spell::CastState::Traveling { .. }));
        if traveling {
            break;
        }
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
    }
    assert!(
        traveling,
        "fixture must expose a real pre-impact traveling phase"
    );
    assert!(!w.state.combat.as_ref().unwrap().in_combat);
    let mut impact = false;
    step(&mut w, movement(23, 4));
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::EquipmentMoved(_))));
    let receipt = w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .find_map(|event| {
            if let Event::EquipmentMoved(receipt) = event {
                Some(receipt)
            } else {
                None
            }
        })
        .unwrap();
    assert!(
        receipt.context.generic_slot.is_some(),
        "exercise retained traveling generic-slot witness"
    );
    assert!(w.state.combat.as_ref().unwrap().equipment.item(4).is_some());
    for _ in 0..40 {
        impact |= w.last_tick().unwrap().events.iter().any(|e| {
            matches!(
                e,
                Event::Spell {
                    event: spell::SpellEvent::DirectDamage { .. },
                    ..
                }
            )
        });
        if impact {
            break;
        }
        w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
            .unwrap();
    }
    assert!(impact);
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
