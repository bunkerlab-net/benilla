// SPDX-License-Identifier: GPL-3.0-or-later
//! Real Lua cursor command, durable publication and same-instance reopen.
use super::*;
use benilla_world_runtime::native::synthetic_equipment_fixture as armor;
fn enter_armor(h: &Harness, open: bool) -> NativeController {
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
        armor::profile(),
        &LootCatalog,
        armor::EquipmentAuthority::default(),
    )
    .unwrap()
}
fn values(events: &[SessionEvent], player: u64) -> benilla_protocol::ObjectFields {
    let mut result = benilla_protocol::ObjectFields::default();
    for e in events {
        if let SessionEvent::ObjectValues { guid, fields } = e {
            if *guid == player {
                result.merge(fields.clone());
            }
        }
    }
    result
}
#[test]
fn armor_cursor_move_withholds_same_object_until_save_then_reopens_and_reequips() {
    use benilla_ui::script::{ContainerState, InvSlotView, InventorySlots, UiScript};
    let h = Harness::new();
    let mut c = enter_armor(&h, false);
    let initial = c.initial_events().unwrap();
    let mut fields = values(&initial, c.player_guid());
    let guid = fields.player_inv_slot(4).unwrap();
    assert_ne!(guid, 0);
    let frontier = inventory(&c).next_item;
    let mana = c.world.state().resources.caster.mana;
    let maximum = c.world.state().combat.as_ref().unwrap().stats.max_mana;
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
    let (src_bag, src_slot) = crate::ui_items::wire_pos(m.src_bag, m.src_slot).unwrap();
    let (dst_bag, dst_slot) = crate::ui_items::wire_pos(m.dst_bag, m.dst_slot).unwrap();
    assert_eq!((src_bag, src_slot, dst_bag, dst_slot), (255, 4, 255, 23));
    c.receive(ClientCommand::SwapInvItem { src_slot, dst_slot })
        .unwrap();
    inject(&h);
    let failed = c.pump(c.tick_duration());
    paused(&failed);
    assert!(failed.events.is_empty());
    assert_eq!(fields.player_inv_slot(4), Some(guid));
    let digest = c.world.state_digest();
    repair(&h);
    c.retry_checkpoint().unwrap();
    let committed = c.pump(Duration::ZERO);
    assert!(!committed.events.iter().any(|e| matches!(
        e,
        SessionEvent::ItemCreate { .. }
            | SessionEvent::ObjectDestroyed(_)
            | SessionEvent::ItemPushResult(_)
    )));
    fields.merge(values(&committed.events, c.player_guid()));
    assert_eq!(fields.player_inv_slot(4).unwrap_or(0), 0);
    assert_eq!(fields.player_pack_slot(0), Some(guid));
    assert_eq!(fields.player_visible_item_entry(4), None);
    assert_eq!(inventory(&c).next_item, frontier);
    assert!(c.world.state().combat.as_ref().unwrap().stats.max_mana < maximum);
    assert!(c.world.state().resources.caster.mana <= mana);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = enter_armor(&h, true);
    assert_eq!(c.world.state_digest(), digest);
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
    let before_mana = c.world.state().resources.caster.mana;
    c.receive(ClientCommand::SwapInvItem {
        src_slot: 23,
        dst_slot: 4,
    })
    .unwrap();
    let equipped = step(&mut c);
    let fields = values(&equipped.events, c.player_guid());
    assert_eq!(fields.player_inv_slot(4), Some(guid));
    assert_eq!(fields.player_visible_item_entry(4), Some(900001));
    assert_eq!(
        c.world.state().combat.as_ref().unwrap().stats.max_mana,
        maximum
    );
    assert_eq!(c.world.state().resources.caster.mana, before_mana);
    assert_eq!(inventory(&c).next_item, frontier);
    assert!(!equipped.events.iter().any(|e| matches!(
        e,
        SessionEvent::ItemCreate { .. } | SessionEvent::ObjectDestroyed(_)
    )));
    c.shutdown().unwrap();
    let final_digest = c.world.state_digest();
    drop(c);
    assert_eq!(enter_armor(&h, true).world.state_digest(), final_digest);
}
