// SPDX-License-Identifier: GPL-3.0-or-later
//! Required immutable equipped origins; no movement or loadout relaxation.
use super::*;
use equipment::{EquipmentOrigin, EquipmentSnapshot, EquipmentState, EquippedItem};
fn snapshot(w: &NativeWorld) -> EquipmentSnapshot {
    w.state.combat.as_ref().unwrap().equipment.snapshot()
}
fn reopen(w: &NativeWorld) {
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
}
fn twin_world() -> NativeWorld {
    use benilla_sim::starter_init as init;
    let mut input = synthetic_fixture::initialized_starter_input();
    input.items[0].inventory_type = 11; // Source FindEquipSlot chooses finger slots10 then11.
    let init::Rows::Complete(grants) = &mut input.item_grants else {
        unreachable!()
    };
    grants[0].count = 2;
    // Keep the source-shaped NPC damage owner and rank1 spell metadata required
    // by Profile::validate; only replace authored starter item placement.
    let mut profile = synthetic_fixture::initialized_profile();
    profile
        .initialize_player(&input, synthetic_fixture::float(100.0), 1)
        .unwrap();
    NativeWorld::synthetic_component_test(profile, [1; 16], synthetic_fixture::player(), 42)
        .unwrap()
}
#[test]
fn source_initializer_origin_survives_actual_updates_checkpoint_and_replay() {
    let mut w = synthetic_fixture::initialized_world(9);
    let initial = w.checkpoint().unwrap();
    let origins: Vec<_> = snapshot(&w)
        .items
        .iter()
        .map(|i| (i.slot, i.item_id, i.origin))
        .collect();
    assert!(!origins.is_empty());
    assert!(origins
        .iter()
        .all(|(slot, item, origin)| origin.genesis_slot == *slot && origin.item_id == *item));
    let cast = synthetic_fixture::cast(&w, 60001);
    w.step(
        1,
        Some(cast),
        &mut synthetic_fixture::KernelAuthority::default(),
    )
    .unwrap();
    for _ in 0..30 {
        w.step(
            w.state.tick + 1,
            None,
            &mut synthetic_fixture::KernelAuthority::default(),
        )
        .unwrap();
    }
    assert_eq!(
        snapshot(&w)
            .items
            .iter()
            .map(|i| (i.slot, i.item_id, i.origin))
            .collect::<Vec<_>>(),
        origins
    );
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn two_source_placed_same_template_items_have_distinct_genesis_origins() {
    let w = twin_world();
    let s = snapshot(&w);
    assert_eq!(s.version, 2);
    assert_eq!(s.items.len(), 2);
    assert_eq!((s.items[0].slot, s.items[1].slot), (10, 11));
    assert_eq!(s.items[0].item_id, s.items[1].item_id);
    assert_ne!(s.items[0].origin, s.items[1].origin);
    reopen(&w);
}
#[test]
fn missing_forged_swapped_duplicate_origins_and_old_format_fail_closed() {
    let w = twin_world();
    for fault in ["missing", "slot", "item", "swap", "duplicate", "oldformat"] {
        let mut forged = w.checkpoint().unwrap();
        let mut encoded = serde_json::to_value(&forged.state).unwrap();
        let equipment = &mut encoded["combat"]["equipment"];
        match fault {
            "missing" => {
                equipment["items"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("origin");
            }
            "slot" => equipment["items"][0]["origin"]["genesis_slot"] = serde_json::json!(4),
            "item" => equipment["items"][0]["origin"]["item_id"] = serde_json::json!(12345),
            "swap" => {
                let a = equipment["items"][0]["origin"].clone();
                equipment["items"][0]["origin"] = equipment["items"][1]["origin"].clone();
                equipment["items"][1]["origin"] = a;
            }
            "duplicate" => {
                equipment["items"][1]["origin"] = equipment["items"][0]["origin"].clone()
            }
            "oldformat" => equipment["version"] = serde_json::json!(1),
            _ => unreachable!(),
        }
        let decoded = serde_json::from_value(encoded);
        if fault == "missing" {
            assert!(decoded.is_err());
            continue;
        }
        forged.state = decoded.unwrap();
        forged.digest = crate::digest(b"benilla-native-state-v67\0", &forged.state);
        forged.checksum = crate::digest(
            b"benilla-native-checkpoint-v67\0",
            &(&forged.state, forged.digest, forged.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), forged).is_err(),
            "{fault}"
        );
    }
}
#[test]
fn structurally_consistent_new_identity_cannot_replace_authoritative_loadout() {
    let item = EquippedItem {
        origin: EquipmentOrigin {
            genesis_slot: 4,
            item_id: 900001,
        },
        slot: 4,
        item_id: 900001,
        max_durability: 10,
        current_durability: 10,
    };
    let fresh = EquipmentState::from_snapshot(EquipmentSnapshot {
        version: 2,
        items: vec![item],
    })
    .unwrap();
    let mut damaged = fresh.snapshot();
    damaged.items[0].current_durability = 0;
    let restored = EquipmentState::restore(damaged, &fresh).unwrap();
    assert_eq!(restored.item(4).unwrap().origin, item.origin);
    for relocate in [false, true] {
        let mut forged = fresh.snapshot();
        if relocate {
            forged.items[0].slot = 5;
            forged.items[0].origin.genesis_slot = 5;
        } else {
            forged.items[0].item_id += 1;
            forged.items[0].origin.item_id += 1;
        }
        assert!(EquipmentState::from_snapshot(forged.clone()).is_ok());
        assert!(EquipmentState::restore(forged, &fresh).is_err());
    }
    let mut old = fresh.snapshot();
    old.version = 1;
    assert!(EquipmentState::from_snapshot(old).is_err());
}
