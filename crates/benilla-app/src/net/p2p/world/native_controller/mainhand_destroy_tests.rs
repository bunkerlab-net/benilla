// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual Lua discard and durable original mainhand tombstone publication.
use super::*;
fn lua_destroy(guid: u64) -> ClientCommand {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let mut ui = UiScript::new().unwrap();
    ui.keep_calls();
    ui.set_container(
        0,
        Some(ContainerState {
            num_slots: 16,
            slots: [(
                1,
                ContainerSlot {
                    guid,
                    item_id: 900002,
                    count: 1,
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        }),
    );
    ui.run("PickupContainerItem(0, 1); DeleteCursorItem()")
        .unwrap();
    let queued = ui.take_container_destroys();
    assert_eq!(queued, [(0, 1, 0)]);
    let (bag_index, slot) = crate::ui_items::wire_pos(queued[0].0, queued[0].1).unwrap();
    assert_eq!((bag_index, slot), (255, 23));
    ClientCommand::DestroyItem {
        bag_index,
        slot,
        count: 0,
    }
}
fn destructions(c: &NativeController) -> usize {
    progress(c)
        .derived_calls
        .iter()
        .filter(|call| matches!(call, n::DerivedCall::ItemDestruction { .. }))
        .count()
}
#[test]
fn actual_lua_staff_discard_is_withheld_then_published_once_and_reopens_retired() {
    let h = Harness::new();
    let mut c = enter_staff(&h, false);
    let initial = c.initial_events().unwrap();
    let guid = values(&initial, c.player_guid())
        .player_inv_slot(15)
        .unwrap();
    c.receive(ClientCommand::SwapInvItem {
        src_slot: 15,
        dst_slot: 23,
    })
    .unwrap();
    step(&mut c);
    let original = inventory(&c).slots[&23].clone();
    let frontier = inventory(&c).next_item;
    let combat = c.world.state().combat.as_ref().unwrap().clone();
    let recipe = serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]["combat"]
        ["recipe"]
        .clone();
    c.receive(lua_destroy(guid)).unwrap();
    inject(&h);
    let withheld = c.pump(c.tick_duration());
    paused(&withheld);
    assert!(withheld.events.is_empty());
    assert!(!inventory(&c)
        .slots
        .values()
        .any(|item| item.id == original.id));
    assert_eq!(inventory(&c).next_item, frontier);
    assert_eq!(destructions(&c), 1);
    assert_eq!(c.world.state().combat.as_ref().unwrap().stats, combat.stats);
    assert_eq!(
        c.world.state().combat.as_ref().unwrap().equipment,
        combat.equipment
    );
    assert_eq!(
        serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]["combat"]["recipe"],
        recipe
    );
    assert!(c.retry_checkpoint().is_err());
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert!(out.pause.is_none());
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e,SessionEvent::ObjectDestroyed(g) if *g==guid))
            .count(),
        1
    );
    assert_eq!(
        values(&out.events, c.player_guid()).player_pack_slot(0),
        Some(0)
    );
    assert!(!out.events.iter().any(|e| matches!(
        e,
        SessionEvent::ItemCreate { .. } | SessionEvent::ItemPushResult(_)
    )));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    c.receive(ClientCommand::DestroyItem {
        bag_index: 255,
        slot: 23,
        count: 0,
    })
    .unwrap();
    let out = step(&mut c);
    assert!(out.events.iter().any(|e| matches!(
        e,
        SessionEvent::InventoryFailure {
            reason: 23,
            item_guid: 0,
            item2_guid: 0,
            ..
        }
    )));
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::ObjectDestroyed(_))));
    assert_eq!(destructions(&c), 1);
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let mut c = enter_staff(&h, true);
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(inventory(&c).next_item, frontier);
    assert!(!inventory(&c)
        .slots
        .values()
        .any(|item| item.id == original.id));
    assert!(!progress(&c).inventory_receipts.contains_key(&original.id));
    assert!(c
        .world
        .profile()
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap()
        .equipment_origin(original.id)
        .is_some());
    assert!(c
        .world
        .state()
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(15)
        .is_none());
    assert_eq!(destructions(&c), 1);
    let initial = c.initial_events().unwrap();
    assert_eq!(
        values(&initial, c.player_guid()).player_pack_slot(0),
        Some(0)
    );
    assert!(!initial
        .iter()
        .any(|e| matches!(e,SessionEvent::ItemCreate {guid:g,..} if *g==guid)));
}
