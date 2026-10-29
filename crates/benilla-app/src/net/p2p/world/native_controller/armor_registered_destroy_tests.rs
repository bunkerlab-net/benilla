// SPDX-License-Identifier: GPL-3.0-or-later
//! Real Lua-selected original armor retirement, durable publication, no general consumption.
use super::*;
use benilla_world_runtime::native::synthetic_equipment_fixture as armor;
fn enter(h: &Harness, lethal: bool, open: bool) -> NativeController {
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        armor::profile_with_lethal(lethal),
        &LootCatalog,
        armor::EquipmentAuthority::default(),
    )
    .unwrap()
}
fn values(events: &[SessionEvent], player: u64) -> benilla_protocol::ObjectFields {
    let mut fields = benilla_protocol::ObjectFields::default();
    for event in events {
        if let SessionEvent::ObjectValues {
            guid,
            fields: delta,
        } = event
        {
            if *guid == player {
                fields.merge(delta.clone());
            }
        }
    }
    fields
}
fn lua_detach(c: &mut NativeController) -> u64 {
    use benilla_ui::script::{ContainerState, InvSlotView, InventorySlots, UiScript};
    let initial = c.initial_events().unwrap();
    let guid = values(&initial, c.player_guid())
        .player_inv_slot(4)
        .unwrap();
    let mut ui = UiScript::new().unwrap();
    let mut doll: InventorySlots = Default::default();
    doll[5] = Some(InvSlotView {
        item_id: 900001,
        guid,
        count: 1,
        equip_slots: vec![5],
        ..Default::default()
    });
    ui.set_inventory_slots(doll);
    ui.set_container(
        0,
        Some(ContainerState {
            num_slots: 16,
            ..Default::default()
        }),
    );
    ui.run("PickupInventoryItem(5); PickupContainerItem(0, 1)")
        .unwrap();
    let moves = ui.take_container_moves();
    assert_eq!(moves.len(), 1);
    let m = &moves[0];
    let src = crate::ui_items::wire_pos(m.src_bag, m.src_slot).unwrap();
    let dst = crate::ui_items::wire_pos(m.dst_bag, m.dst_slot).unwrap();
    assert_eq!((src, dst), ((255, 4), (255, 23)));
    c.receive(ClientCommand::SwapInvItem {
        src_slot: src.1,
        dst_slot: dst.1,
    })
    .unwrap();
    let out = step(c);
    assert_eq!(
        values(&out.events, c.player_guid()).player_pack_slot(0),
        Some(guid)
    );
    guid
}
fn lua_destroy(guid: u64) -> ClientCommand {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let mut ui = UiScript::new().unwrap();
    ui.set_container(
        0,
        Some(ContainerState {
            num_slots: 16,
            slots: [(
                1,
                ContainerSlot {
                    guid,
                    item_id: 900001,
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
    let requests = ui.take_container_destroys();
    assert_eq!(requests, [(0, 1, 0)]);
    let (bag_index, slot) = crate::ui_items::wire_pos(requests[0].0, requests[0].1).unwrap();
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
// Inspect the durable recipe through its public checkpoint serialization, without
// granting app code mutation access to the runtime's private recipe owner.
fn recipe(c: &NativeController) -> serde_json::Value {
    serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]["combat"]["recipe"]
        .clone()
}
fn assert_retired(c: &NativeController, id: u64, frontier: u64) {
    assert!(!inventory(c).slots.values().any(|item| item.id == id));
    assert!(!progress(c).inventory_receipts.contains_key(&id));
    assert_eq!(inventory(c).next_item, frontier);
    assert!(c
        .world
        .profile()
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap()
        .equipment_origin(id)
        .is_some());
    assert!(c
        .world
        .state()
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(4)
        .is_none());
    assert!(recipe(c)["inactive_slots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|slot| slot.as_u64() == Some(4)));
}
#[test]
fn lua_original_destroy_is_withheld_then_published_once_and_retirement_reopens() {
    let h = Harness::new();
    let mut c = enter(&h, false, false);
    let guid = lua_detach(&mut c);
    let original = inventory(&c).slots[&23].clone();
    let frontier = inventory(&c).next_item;
    let before_recipe = recipe(&c);
    c.receive(lua_destroy(guid)).unwrap();
    inject(&h);
    let withheld = c.pump(c.tick_duration());
    paused(&withheld);
    assert!(withheld.events.is_empty());
    assert_retired(&c, original.id, frontier);
    assert_eq!(destructions(&c), 1);
    assert_eq!(recipe(&c), before_recipe);
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
    // Source empty-slot retry is ItemNotFound, not a second tombstone or deletion.
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
    assert_retired(&c, original.id, frontier);
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let mut reopened = enter(&h, false, true);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert_retired(&reopened, original.id, frontier);
    assert_eq!(destructions(&reopened), 1);
    let initial = reopened.initial_events().unwrap();
    assert_eq!(
        values(&initial, reopened.player_guid()).player_pack_slot(0),
        Some(0)
    );
    assert!(!initial
        .iter()
        .any(|e| matches!(e,SessionEvent::ItemCreate {guid:g,..} if *g==guid)));
}
#[test]
fn actual_dead_player_can_destroy_detached_original_without_equip_or_resurrection() {
    let h = Harness::new();
    let mut c = enter(&h, true, false);
    let guid = lua_detach(&mut c);
    let original = inventory(&c).slots[&23].clone();
    let frontier = inventory(&c).next_item;
    let target = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 133,
        target: Some(target),
    })
    .unwrap();
    for _ in 0..100 {
        if !c.world.state().player_alive() {
            break;
        }
        step(&mut c);
    }
    assert!(!c.world.state().player_alive());
    assert_eq!(inventory(&c).slots[&23], original);
    c.receive(lua_destroy(guid)).unwrap();
    let out = step(&mut c);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e,SessionEvent::ObjectDestroyed(g) if *g==guid))
            .count(),
        1
    );
    assert_eq!(c.world.state().health, 0);
    assert!(!c.world.state().player_alive());
    assert_retired(&c, original.id, frontier);
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let reopened = enter(&h, true, true);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert!(!reopened.world.state().player_alive());
    assert_retired(&reopened, original.id, frontier);
}
