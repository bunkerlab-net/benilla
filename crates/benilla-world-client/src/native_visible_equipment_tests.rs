// SPDX-License-Identifier: GPL-3.0-or-later
//! Bound worn entries only: no manufactured creators, enchants or properties.
use super::*;
fn assert_entries(fields: &ObjectFields, w: &n::NativeWorld) {
    let raw: BTreeMap<_, _> = fields.raw_fields().collect();
    let equipment = &w.state().combat.as_ref().unwrap().equipment;
    for slot in 0..19u8 {
        let expected = equipment.item(slot).map_or(0, |i| i.item_id);
        let creator = 258 + 12 * u16::from(slot);
        assert_eq!(raw.get(&(creator + 2)), Some(&expected), "slot {slot}");
        assert_eq!(
            fields.player_visible_item_entry(slot),
            (expected != 0).then_some(expected)
        );
        for offset in [0, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11] {
            assert!(
                !raw.contains_key(&(creator + offset)),
                "unowned visible word slot{slot} offset{offset}"
            );
        }
    }
}
#[test]
fn bound_initial_and_frame_entries_resolve_via_actual_item_query_metadata() {
    let w = build(&[2], 0);
    let mut projection = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let events = projection.initial_events(&w).unwrap();
    let fields = player_fields(&events, projection.player_guid());
    assert_entries(&fields, &w);
    let entry = fields.player_visible_item_entry(4).unwrap();
    assert_eq!(entry, 900001);
    let SessionEvent::ItemTemplate {
        entry: returned,
        info: Some(info),
    } = projection.item_query(&w, entry).unwrap()
    else {
        panic!("bound worn query")
    };
    assert_eq!(returned, entry);
    assert_eq!(info.display_info_id, 900002);
    assert_eq!(info.inventory_type, 5);
    let mut w = w;
    w.step(
        w.state().tick + 1,
        None,
        &mut fixture::LootAuthority::default(),
    )
    .unwrap();
    let fields = player_fields(&projection.advance(&w).unwrap(), projection.player_guid());
    assert_entries(&fields, &w);
}
#[test]
fn worn_metadata_missing_mismatched_or_display_unavailable_refuses_projection() {
    let w = build(&[2], 0);
    for fault in 0..4 {
        let mut c = Catalog::new(&w);
        match fault {
            0 => {
                c.items.remove(&900001);
            }
            1 => c.items.get_mut(&900001).unwrap().display_info_id += 1,
            2 => c.items.get_mut(&900001).unwrap().inventory_type += 1,
            _ => c.display = false,
        }
        assert!(
            NativeProjection::new(&w, w.state().player, &c).is_err(),
            "fault{fault}"
        );
    }
}
#[test]
fn equipment_only_world_without_bound_metadata_does_not_fabricate_visible_entries() {
    let w = n::synthetic_fixture::initialized_world(9);
    assert!(w
        .state()
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(4)
        .is_some());
    assert!(w.profile().quest_progression.is_none());
    let projection =
        NativeProjection::new(&w, w.state().player, &crate::native::tests::Catalog).unwrap();
    let fields = player_fields(
        &projection.initial_events(&w).unwrap(),
        projection.player_guid(),
    );
    let raw: BTreeMap<_, _> = fields.raw_fields().collect();
    for slot in 0..19u8 {
        assert!(!raw.contains_key(&(260 + 12 * u16::from(slot))));
        assert_eq!(fields.player_visible_item_entry(slot), None);
    }
}
#[test]
fn genuine_lethal_damage_and_broken_equipment_keep_visible_entry_after_reopen() {
    use benilla_sim::player_death as d;
    let mut profile = build(&[2], 0).profile().clone();
    profile.npcs[0].max_health = 3000;
    let combat = profile.npcs[0].combat.as_mut().unwrap();
    combat.minimum_damage = n::synthetic_fixture::float(10000.0);
    combat.maximum_damage = n::synthetic_fixture::float(10000.0);
    profile
        .configure_player_death(
            d::DeathConfig {
                map_instanceable: false,
                pve_reclaim_delay_enabled: true,
            },
            d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|x| (x, Some(false)))).unwrap(),
        )
        .unwrap();
    let mut w = n::NativeWorld::synthetic_component_test(
        profile,
        [8; 16],
        n::synthetic_fixture::player(),
        7,
    )
    .unwrap();
    let mut projection = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let mut orders = NativeMovementOrders::from_session_next_counter(37);
    let mut authority = fixture::LootAuthority::default();
    let input = n::synthetic_fixture::cast(&w, 60001);
    w.step(w.state().tick + 1, Some(input), &mut authority)
        .unwrap();
    projection
        .advance_with_movement_orders(&w, &mut orders)
        .unwrap();
    for _ in 0..100 {
        if w.state().health == 0 {
            break;
        }
        w.step(w.state().tick + 1, None, &mut authority).unwrap();
        let fields = player_fields(
            &projection
                .advance_with_movement_orders(&w, &mut orders)
                .unwrap(),
            projection.player_guid(),
        );
        assert_entries(&fields, &w);
    }
    assert_eq!(w.state().health, 0);
    assert_eq!(
        w.state()
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(4)
            .unwrap()
            .current_durability,
        0
    );
    w.step(w.state().tick + 1, None, &mut authority).unwrap();
    let fields = player_fields(
        &projection
            .advance_with_movement_orders(&w, &mut orders)
            .unwrap(),
        projection.player_guid(),
    );
    assert_entries(&fields, &w);
    assert_eq!(fields.player_visible_item_entry(4), Some(900001));
    let restored = n::NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        n::Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    let fresh = NativeProjection::new(&restored, restored.state().player, &Catalog::new(&restored))
        .unwrap();
    let fields = player_fields(
        &fresh
            .initial_events_with_movement_orders(&restored, &mut orders)
            .unwrap(),
        fresh.player_guid(),
    );
    assert_entries(&fields, &restored);
    assert_eq!(fields.player_visible_item_entry(4), Some(900001));
}

#[test]
fn registered_equipment_handles_resolve_to_legacy_objects_without_backpack_creation() {
    let world = build(&[2], 0);
    let genesis = world
        .profile()
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap();
    let inventory = world
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap();
    let projection =
        NativeProjection::new(&world, world.state().player, &Catalog::new(&world)).unwrap();
    let events = projection.initial_events(&world).unwrap();
    assert!(genesis.equipment_instances().count() > 0);
    for (handle, origin) in genesis.equipment_instances() {
        let guid = equipment_guid(&world, origin);
        assert_eq!(loot::guid(&world, handle), guid);
        assert_eq!(genesis.equipment_instance(origin), Some(handle));
        assert!(!inventory.slots.values().any(|item| item.id == handle));
        assert_eq!(events.iter().filter(|event| matches!(event, SessionEvent::ItemCreate {guid: actual, ..} if *actual == guid)).count(), 1);
    }
    for item in inventory.slots.values() {
        let digest = crate::hash(
            b"benilla-native-inventory-guid-v1\0",
            &(world.state().world, world.state().player, item.id),
        );
        let mut bytes = [0; 8];
        bytes[..6].copy_from_slice(&digest[..6]);
        assert_eq!(
            loot::guid(&world, item.id),
            0x4000_0000_0000_0000 | u64::from_le_bytes(bytes).max(1)
        );
    }
}
