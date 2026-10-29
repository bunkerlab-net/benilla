// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual Lua occupied-slot exchange, with the ordinary item as the source.
use super::*;
fn lua_exchange(original_guid: u64, ordinary_guid: u64) -> ClientCommand {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let mut ui = UiScript::new().unwrap();
    ui.set_container(
        0,
        Some(ContainerState {
            num_slots: 16,
            slots: [
                (
                    1,
                    ContainerSlot {
                        guid: original_guid,
                        item_id: 900001,
                        count: 1,
                        ..Default::default()
                    },
                ),
                (
                    2,
                    ContainerSlot {
                        guid: ordinary_guid,
                        item_id: 900001,
                        count: 1,
                        ..Default::default()
                    },
                ),
            ]
            .into(),
            ..Default::default()
        }),
    );
    ui.run("PickupContainerItem(0, 2); PickupContainerItem(0, 1)")
        .unwrap();
    let requests = ui.take_container_moves();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    let (src_bag, src_slot) = crate::ui_items::wire_pos(request.src_bag, request.src_slot).unwrap();
    let (dst_bag, dst_slot) = crate::ui_items::wire_pos(request.dst_bag, request.dst_slot).unwrap();
    assert_eq!((src_bag, src_slot, dst_bag, dst_slot), (255, 24, 255, 23));
    ClientCommand::SwapInvItem { src_slot, dst_slot }
}
#[test]
fn ordinary_source_lua_exchange_with_registered_destination_is_atomic_and_reopens_same_objects() {
    let h = Harness::new();
    let mut c = enter_duplicate(&h, false);
    let (original_guid, ordinary_guid) = prepare(&mut c);
    let original = inventory(&c).slots[&23].clone();
    let ordinary = inventory(&c).slots[&24].clone();
    assert_ne!(original.id, ordinary.id);
    let registry = c
        .world
        .profile()
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap();
    assert!(registry.equipment_origin(original.id).is_some());
    assert!(registry.equipment_origin(ordinary.id).is_none());
    assert_eq!(original.item, ordinary.item);
    let frontier = inventory(&c).next_item;
    let equipment = c.world.state().combat.as_ref().unwrap().equipment.clone();
    let stats = c.world.state().combat.as_ref().unwrap().stats.clone();
    let recipe = serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]["combat"]
        ["recipe"]
        .clone();
    let quests = progress(&c).quests.clone();
    let memberships = progress(&c)
        .derived_calls
        .iter()
        .filter(|call| matches!(call, n::DerivedCall::ItemMembership { .. }))
        .count();
    c.receive(lua_exchange(original_guid, ordinary_guid))
        .unwrap();
    inject(&h);
    let withheld = c.pump(c.tick_duration());
    paused(&withheld);
    assert!(withheld.events.is_empty());
    assert_eq!(inventory(&c).slots[&23], ordinary);
    assert_eq!(inventory(&c).slots[&24], original);
    assert_eq!(inventory(&c).next_item, frontier);
    assert_eq!(
        c.world.state().combat.as_ref().unwrap().equipment,
        equipment
    );
    assert_eq!(c.world.state().combat.as_ref().unwrap().stats, stats);
    assert_eq!(
        serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]["combat"]["recipe"],
        recipe
    );
    assert_eq!(progress(&c).quests, quests);
    assert_eq!(
        progress(&c)
            .derived_calls
            .iter()
            .filter(|call| matches!(call, n::DerivedCall::ItemMembership { .. }))
            .count(),
        memberships
    );
    assert!(c.retry_checkpoint().is_err());
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert!(out.pause.is_none());
    let fields = values(&out.events, c.player_guid());
    assert_eq!(fields.player_pack_slot(0), Some(ordinary_guid));
    assert_eq!(fields.player_pack_slot(1), Some(original_guid));
    assert!(!out.events.iter().any(|e| matches!(
        e,
        SessionEvent::ItemCreate { .. }
            | SessionEvent::ObjectDestroyed(_)
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::QuestObjectiveItem { .. }
    )));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let mut c = enter_duplicate(&h, true);
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(inventory(&c).slots[&23], ordinary);
    assert_eq!(inventory(&c).slots[&24], original);
    assert_eq!(inventory(&c).next_item, frontier);
    let initial = c.initial_events().unwrap();
    let fields = values(&initial, c.player_guid());
    assert_eq!(fields.player_pack_slot(0), Some(ordinary_guid));
    assert_eq!(fields.player_pack_slot(1), Some(original_guid));
    for guid in [ordinary_guid, original_guid] {
        assert_eq!(
            initial
                .iter()
                .filter(|e| matches!(e,SessionEvent::ItemCreate {guid:g,..} if *g==guid))
                .count(),
            1
        );
    }
}
