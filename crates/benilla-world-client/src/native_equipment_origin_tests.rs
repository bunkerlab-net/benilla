// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent legacy GUID recipe, actual durability/death and reopened descriptors.
use super::*;
fn legacy_guid(world: &n::NativeWorld, slot: u8, item: u32) -> u64 {
    let digest = crate::hash(
        b"benilla-native-equipment-guid-v2\0",
        &(world.state().world, world.state().player, slot, item),
    );
    let mut bytes = [0; 8];
    bytes[..6].copy_from_slice(&digest[..6]);
    0x4000_0000_0000_0000 | u64::from_le_bytes(bytes).max(1)
}
#[test]
fn immutable_origin_uses_exact_legacy_tuple_for_every_equipment_slot() {
    let world = n::synthetic_quest_fixture::world();
    for slot in 0..19 {
        for item in [1, 123, 900001, u32::MAX] {
            assert_eq!(
                equipment_guid(
                    &world,
                    n::equipment::EquipmentOrigin {
                        genesis_slot: slot,
                        item_id: item
                    }
                ),
                legacy_guid(&world, slot, item)
            );
        }
    }
    let projection = NativeProjection::new(
        &world,
        world.state().player,
        &super::quests::tests::Catalog::new(&world),
    )
    .unwrap();
    let events = projection.initial_events(&world).unwrap();
    for item in world.state().combat.as_ref().unwrap().equipment.items() {
        let expected = legacy_guid(&world, item.slot, item.item_id);
        assert!(events
            .iter()
            .any(|event| matches!(event,SessionEvent::ItemCreate{guid,..} if *guid==expected)));
    }
}
#[test]
fn actual_lethal_damage_and_durability_loss_keep_origin_guid_after_reopen() {
    use benilla_sim::player_death as d;
    use n::synthetic_fixture::*;
    let mut profile = n::synthetic_quest_fixture::profile();
    profile.npcs[0].max_health = 3000;
    let combat = profile.npcs[0].combat.as_mut().unwrap();
    combat.minimum_damage = float(10000.0);
    combat.maximum_damage = float(10000.0);
    profile
        .configure_player_death(
            d::DeathConfig {
                map_instanceable: false,
                pve_reclaim_delay_enabled: true,
            },
            d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|x| (x, Some(false)))).unwrap(),
        )
        .unwrap();
    let mut world =
        n::NativeWorld::synthetic_component_test(profile, [1; 16], player(), 7).unwrap();
    let initial = world.state().combat.as_ref().unwrap().equipment.snapshot();
    let expected = initial
        .items
        .iter()
        .map(|item| (item.slot, legacy_guid(&world, item.slot, item.item_id)))
        .collect::<BTreeMap<_, _>>();
    let mut projection = NativeProjection::new(
        &world,
        world.state().player,
        &super::quests::tests::Catalog::new(&world),
    )
    .unwrap();
    let mut authority = n::synthetic_quest_fixture::QuestAuthority::default();
    let mut orders = NativeMovementOrders::from_session_next_counter(37);
    let input = cast(&world, 60001);
    world
        .step(world.state().tick + 1, Some(input), &mut authority)
        .unwrap();
    projection
        .advance_with_movement_orders(&world, &mut orders)
        .unwrap();
    for _ in 0..100 {
        if !world.state().player_alive() {
            break;
        }
        world
            .step(world.state().tick + 1, None, &mut authority)
            .unwrap();
        projection
            .advance_with_movement_orders(&world, &mut orders)
            .unwrap();
    }
    assert!(!world.state().player_alive());
    world
        .step(world.state().tick + 1, None, &mut authority)
        .unwrap();
    projection
        .advance_with_movement_orders(&world, &mut orders)
        .unwrap();
    let current = world.state().combat.as_ref().unwrap().equipment.snapshot();
    assert!(initial
        .items
        .iter()
        .zip(&current.items)
        .any(|(a, b)| a.current_durability > b.current_durability));
    for (before, after) in initial.items.iter().zip(&current.items) {
        assert_eq!(before.origin, after.origin);
        assert_eq!(equipment_guid(&world, after.origin), expected[&after.slot]);
    }
    let reopened = n::NativeWorld::restore_synthetic_component_test(
        world.profile().clone(),
        world.checkpoint().unwrap(),
    )
    .unwrap();
    assert_eq!(
        reopened
            .state()
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .snapshot(),
        current
    );
    let projection = NativeProjection::new(
        &reopened,
        reopened.state().player,
        &super::quests::tests::Catalog::new(&reopened),
    )
    .unwrap();
    let events = projection
        .initial_events_with_movement_orders(&reopened, &mut orders)
        .unwrap();
    for item in &current.items {
        assert!(events.iter().any(|event|matches!(event,SessionEvent::ItemCreate{guid,fields,..} if *guid==expected[&item.slot] && fields.item_durability()==Some(item.current_durability))));
    }
}
