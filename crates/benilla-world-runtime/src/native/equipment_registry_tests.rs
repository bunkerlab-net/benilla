// SPDX-License-Identifier: GPL-3.0-or-later
//! Local genesis registration only: registered equipment cannot become carried.
use super::synthetic_loot_fixture as fixture;
use super::*;
use benilla_sim::{inventory as inv, starter_init as init};

fn world() -> NativeWorld {
    let (mut p, mut input, q) = fixture::item_grant_setup();
    let init::Rows::Complete(grants) = &mut input.item_grants else {
        unreachable!()
    };
    // One worn robe and one identical carried robe, both actually initialized.
    grants[0].count = 2;
    p.initialize_player(&input, synthetic_fixture::float(100.0), 1)
        .unwrap();
    p.initialize_quest_progression(&input, q).unwrap();
    p.admin = Some(AdminPolicy {
        world: [1; 16],
        world_owner: synthetic_fixture::player().owner,
        allowlist: Default::default(),
        revision: 7,
    });
    NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42).unwrap()
}
fn inventory(w: &NativeWorld) -> &inv::State {
    w.state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
}
fn genesis(w: &NativeWorld) -> &NativeLootGenesis {
    w.profile
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap()
}
fn reopen(w: &NativeWorld) {
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
}
fn rehash(c: &mut Checkpoint) {
    c.digest = crate::digest(b"benilla-native-state-v62\0", &c.state);
    c.checksum = crate::digest(
        b"benilla-native-checkpoint-v62\0",
        &(&c.state, c.digest, c.history_tip),
    );
}
#[test]
fn real_initial_carried_ids_precede_contiguous_bijective_equipment_handles() {
    let w = world();
    let g = genesis(&w);
    let i = inventory(&w);
    assert_eq!(i.slots.len(), 1);
    assert_eq!(i.slots[&23].id, 1);
    assert_eq!(g.equipment_origin(1), None);
    let registered: Vec<_> = g.equipment_instances().collect();
    let equipment = w.state.combat.as_ref().unwrap().equipment.snapshot();
    assert_eq!(registered.len(), equipment.items.len());
    assert!(!registered.is_empty());
    for (index, (id, origin)) in registered.iter().enumerate() {
        assert_eq!(*id, 2 + index as u64);
        assert_eq!(g.equipment_origin(*id), Some(*origin));
        assert_eq!(g.equipment_instance(*origin), Some(*id));
        assert!(equipment.items.iter().any(|item| item.origin == *origin));
        assert!(!i.slots.values().any(|item| item.id == *id));
    }
    assert_eq!(i.next_item, 2 + registered.len() as u64);
    assert_eq!(g.equipment_origin(i.next_item), None);
    reopen(&w);
}
#[test]
fn actual_future_grants_skip_registered_objects_and_replay_exactly() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    let frontier = inventory(&w).next_item;
    let registry: Vec<_> = genesis(&w).equipment_instances().collect();
    let request = Command::Admin(AdminRequest {
        key: [31; 16],
        world: w.state.world,
        player: w.state.player,
        policy_revision: 7,
        operation: AdminOperation::GrantItem {
            item: 70001,
            count: 7,
        },
    });
    let command = synthetic_quest_fixture::command(&w, request);
    w.step(1, Some(command), &mut fixture::LootAuthority::default())
        .unwrap();
    assert_eq!(inventory(&w).slots[&23].id, 1);
    // StoreNewItem allocates the original first; the nonfinal destination
    // gets a clone, while the last destination keeps that original handle.
    assert_eq!(inventory(&w).slots[&24].id, frontier + 1);
    assert_eq!(inventory(&w).slots[&25].id, frontier);
    assert_eq!(inventory(&w).next_item, frontier + 2);
    assert_eq!(
        genesis(&w).equipment_instances().collect::<Vec<_>>(),
        registry
    );
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn reserved_handle_in_pristine_carried_item_with_valid_self_hash_is_rejected() {
    let w = world();
    let id = genesis(&w).equipment_instances().next().unwrap().0;
    let mut forged = w.checkpoint().unwrap();
    let mut item = inventory(&w).slots[&23].clone();
    let old = item.id;
    item.id = id;
    let mut receipt = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory_receipts[&old]
        .clone();
    receipt.digest = crate::digest(
        b"benilla-native-item-chain-v2\0",
        &(receipt.previous, receipt.reason, 23u8, &item),
    );
    assert!(receipt.validates(23, &item));
    let mut pristine = inventory(&w).clone();
    pristine.slots.insert(23, item);
    // Ordinary pristine validation alone accepts this ID below the allocator.
    pristine.validate(&genesis(&w).inventory).unwrap();
    let mut encoded = serde_json::to_value(&forged.state).unwrap();
    encoded["quest_progression"]["inventory"] = serde_json::to_value(pristine).unwrap();
    let receipts = encoded["quest_progression"]["inventory_receipts"]
        .as_object_mut()
        .unwrap();
    receipts.remove(&old.to_string());
    receipts.insert(id.to_string(), serde_json::to_value(receipt).unwrap());
    forged.state = serde_json::from_value(encoded).unwrap();
    rehash(&mut forged);
    assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), forged).is_err());
}
#[test]
fn registration_does_not_authorize_equipment_moves() {
    let mut w = world();
    let slot = genesis(&w)
        .equipment_instances()
        .next()
        .unwrap()
        .1
        .genesis_slot;
    for (source, destination) in [(slot, 24), (23, slot)] {
        let before = w.checkpoint().unwrap();
        let command = synthetic_quest_fixture::command(
            &w,
            Command::InventoryMove(InventoryMoveRequest {
                source,
                destination,
            }),
        );
        assert!(w
            .step(
                w.state.tick + 1,
                Some(command),
                &mut fixture::LootAuthority::default()
            )
            .is_err());
        assert_eq!(w.checkpoint().unwrap(), before);
    }
}
