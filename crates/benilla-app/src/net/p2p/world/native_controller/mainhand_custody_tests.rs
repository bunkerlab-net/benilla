// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual backpack Lua intents, same original staff, and durable custody publication.
use super::*;
fn lua(guid: u64, autostore: bool) -> ClientCommand {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let mut ui = UiScript::new().unwrap();
    ui.keep_calls();
    let source = if autostore { 2 } else { 1 };
    ui.set_container(
        0,
        Some(ContainerState {
            num_slots: 16,
            slots: [(
                source,
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
    if autostore {
        ui.run("PickupContainerItem(0, 2); PutItemInBackpack()")
            .unwrap();
        assert!(
            ui.take_bag_autostores().is_empty(),
            "own-backpack drop cancels"
        );
        assert!(ui.cursor_item().is_none());
        // This return is an explicit protocol test input, not output from the Lua drop.
        ClientCommand::AutoStoreBagItem {
            src_bag: 255,
            src_slot: 24,
            dst_bag: 255,
        }
    } else {
        ui.run("PickupContainerItem(0, 1); PickupContainerItem(0, 2)")
            .unwrap();
        let queued = ui.take_container_moves();
        assert_eq!(queued.len(), 1);
        let q = &queued[0];
        let (src_bag, src_slot) = crate::ui_items::wire_pos(q.src_bag, q.src_slot).unwrap();
        let (dst_bag, dst_slot) = crate::ui_items::wire_pos(q.dst_bag, q.dst_slot).unwrap();
        assert_eq!((src_bag, src_slot, dst_bag, dst_slot), (255, 23, 255, 24));
        ClientCommand::SwapInvItem { src_slot, dst_slot }
    }
}
fn no_acquisition(events: &[SessionEvent]) {
    assert!(!events.iter().any(|event| matches!(
        event,
        SessionEvent::ItemCreate { .. }
            | SessionEvent::ObjectDestroyed(_)
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::QuestObjectiveItem { .. }
    )));
}
#[test]
fn lua_staff_relocation_and_explicit_autostore_preserve_identity_across_save_retry_reopen() {
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
    let equipment = c.world.state().combat.as_ref().unwrap().equipment.clone();
    let stats = c.world.state().combat.as_ref().unwrap().stats.clone();
    let recipe = serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]["combat"]
        ["recipe"]
        .clone();
    c.receive(lua(guid, false)).unwrap();
    let moved = step(&mut c);
    no_acquisition(&moved.events);
    let fields = values(&moved.events, c.player_guid());
    assert_eq!(fields.player_pack_slot(0), Some(0));
    assert_eq!(fields.player_pack_slot(1), Some(guid));
    assert_eq!(inventory(&c).slots[&24], original);
    c.receive(lua(guid, true)).unwrap();
    inject(&h);
    let failed = c.pump(c.tick_duration());
    paused(&failed);
    assert!(failed.events.is_empty());
    assert_eq!(inventory(&c).slots[&23], original);
    assert!(!inventory(&c).slots.contains_key(&24));
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
    let digest = c.world.state_digest();
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    no_acquisition(&out.events);
    let fields = values(&out.events, c.player_guid());
    assert_eq!(fields.player_pack_slot(1), Some(0));
    assert_eq!(fields.player_pack_slot(0), Some(guid));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = enter_staff(&h, true);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(inventory(&c).slots[&23], original);
    assert_eq!(inventory(&c).next_item, frontier);
    let initial = c.initial_events().unwrap();
    assert_eq!(
        initial
            .iter()
            .filter(|e| matches!(e,SessionEvent::ItemCreate { guid:g,.. } if *g==guid))
            .count(),
        1
    );
    assert_eq!(
        values(&initial, c.player_guid()).player_pack_slot(0),
        Some(guid)
    );
}
