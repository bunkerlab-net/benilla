// SPDX-License-Identifier: GPL-3.0-or-later
//! Registered armor relocation through the existing semantic inventory projection.
use super::*;

fn relocate(
    world: &mut n::NativeWorld,
    authority: &mut n::synthetic_equipment_fixture::EquipmentAuthority,
    source: u8,
    destination: u8,
) -> inv::MovementReceipt {
    let command = n::synthetic_quest_fixture::command(
        world,
        n::Command::InventoryMove(n::InventoryMoveRequest {
            source,
            destination,
        }),
    );
    world
        .step(world.state().tick + 1, Some(command), authority)
        .unwrap();
    world
        .last_tick()
        .unwrap()
        .events
        .iter()
        .find_map(|event| {
            if let n::Event::InventoryMoved(receipt) = event {
                Some((**receipt).clone())
            } else {
                None
            }
        })
        .expect("actual registered backpack relocation")
}

#[test]
fn registered_armor_relocation_preserves_identity_and_only_changes_backpack_pointers() {
    let mut world = n::synthetic_equipment_fixture::world();
    let mut authority = n::synthetic_equipment_fixture::EquipmentAuthority::default();
    let receipt = armor_move(&mut world, &mut authority, 4, 23);
    let instance = receipt.instance;
    let item_guid = guid(&world, instance);
    let projection =
        NativeProjection::new(&world, world.state().player, &Catalog::new(&world)).unwrap();
    let frontier = receipt.after.next_item;
    let recipe = serde_json::to_value(world.checkpoint().unwrap()).unwrap()["state"]["combat"]
        ["recipe"]
        .clone();
    assert!(recipe.is_object(), "serialized source recipe is present");
    for (source, destination) in [(23, 24), (24, 38), (38, 23)] {
        let mut frame = Frame::new(world.state());
        let moved = relocate(&mut world, &mut authority, source, destination);
        assert_eq!(moved.before.slots[&source], moved.after.slots[&destination]);
        assert_eq!(moved.after.next_item, frontier);
        assert_eq!(moved.before.equipped, moved.after.equipped);
        assert_eq!(
            serde_json::to_value(world.checkpoint().unwrap()).unwrap()["state"]["combat"]["recipe"]
                .clone(),
            recipe
        );
        let events = projection
            .inventory_moved(&world, &moved, &mut frame)
            .unwrap();
        assert_eq!(
            events.len(),
            1,
            "semantic final pointers, not upstream transient packet order"
        );
        assert_eq!(
            value_pairs(&events[0], projection.player_guid),
            BTreeMap::from([
                (486 + 2 * u16::from(source), 0),
                (487 + 2 * u16::from(source), 0),
                (486 + 2 * u16::from(destination), item_guid as u32),
                (487 + 2 * u16::from(destination), (item_guid >> 32) as u32),
            ])
        );
        frame.validate_final(&world).unwrap();
        let restored = n::NativeWorld::restore_synthetic_component_test(
            world.profile().clone(),
            world.checkpoint().unwrap(),
        )
        .unwrap();
        let fresh =
            NativeProjection::new(&restored, restored.state().player, &Catalog::new(&restored))
                .unwrap();
        let mut orders = NativeMovementOrders::from_session_next_counter(17);
        let events = fresh
            .initial_events_with_movement_orders(&restored, &mut orders)
            .unwrap();
        assert_eq!(events.iter().filter(|event| matches!(event, SessionEvent::ItemCreate { guid, .. } if *guid == item_guid)).count(), 1);
        world = restored;
    }
    let equipped = armor_move(&mut world, &mut authority, 23, 4);
    assert_eq!(equipped.instance, instance);
    assert_eq!(guid(&world, equipped.instance), item_guid);
    assert_eq!(equipped.after.next_item, frontier);
}

#[test]
fn registered_relocation_projection_rejects_stale_and_forged_receipts_atomically() {
    let mut world = n::synthetic_equipment_fixture::world();
    let mut authority = n::synthetic_equipment_fixture::EquipmentAuthority::default();
    armor_move(&mut world, &mut authority, 4, 23);
    let projection =
        NativeProjection::new(&world, world.state().player, &Catalog::new(&world)).unwrap();
    let mut frame = Frame::new(world.state());
    let receipt = relocate(&mut world, &mut authority, 23, 24);
    for mutate in [
        |r: &mut inv::MovementReceipt| r.after.next_item += 1,
        |r: &mut inv::MovementReceipt| r.after.slots.get_mut(&24).unwrap().id += 1,
        |r: &mut inv::MovementReceipt| {
            r.after.equipped.insert(900001, 1);
        },
    ] {
        let mut bad = receipt.clone();
        mutate(&mut bad);
        assert!(projection
            .inventory_moved(&world, &bad, &mut frame)
            .is_err());
        assert_eq!(frame.slots, receipt.before.slots);
        assert_eq!(frame.equipped, receipt.before.equipped);
        assert_eq!(frame.next_item, Some(receipt.before.next_item));
    }
    // A locally valid reducer witness from another allocator frontier is not
    // the projection's actual preceding owner state.
    let mut alternate = receipt.clone();
    alternate.before.next_item += 1;
    alternate.after.next_item += 1;
    let catalog = &world
        .profile()
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap()
        .inventory;
    alternate.validate(catalog).unwrap();
    assert!(projection
        .inventory_moved(&world, &alternate, &mut frame)
        .is_err());
    assert_eq!(frame.slots, receipt.before.slots);
    assert_eq!(frame.next_item, Some(receipt.before.next_item));
    projection
        .inventory_moved(&world, &receipt, &mut frame)
        .unwrap();
    assert!(projection
        .inventory_moved(&world, &receipt, &mut frame)
        .is_err());
    assert_eq!(frame.slots, receipt.after.slots);
    frame.validate_final(&world).unwrap();
}
