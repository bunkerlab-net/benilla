// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual Lua intent and non-ammunition sender, not the Bevy metadata dispatch.
use super::*;
fn lua_autoequip(guid: u64) -> ClientCommand {
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
                    item_class: Some(2),
                    inventory_type: 17,
                    equip_slots: vec![16],
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        }),
    );
    ui.set_item_template(
        900002,
        benilla_ui::script::ItemTemplateView {
            class: 2,
            inventory_type: 17,
            ..Default::default()
        },
    );
    ui.run("PickupContainerItem(0, 1); AutoEquipCursorItem()")
        .unwrap();
    assert!(ui.cursor_item().is_none());
    assert!(ui.item_locked(guid));
    let queued = ui.take_container_autoequips();
    assert_eq!(queued, [(0, 1)]);
    let (bag_index, slot) = crate::ui_items::wire_pos(queued[0].0, queued[0].1).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    crate::ui_items::send_equipment_command(&tx, bag_index, slot);
    let sent = rx.try_recv().unwrap();
    assert!(matches!(
        sent,
        ClientCommand::AutoEquipItem {
            bag_index: 255,
            slot: 23
        }
    ));
    assert!(rx.try_recv().is_err());
    sent
}
#[test]
fn lua_mainhand_auto_equip_waits_for_durable_save_then_reopens_exact_original() {
    let h = Harness::new();
    let mut c = enter_staff(&h, false);
    let original = c.world.state().combat.as_ref().unwrap().equipment.clone();
    let initial = c.initial_events().unwrap();
    let guid = values(&initial, c.player_guid())
        .player_inv_slot(15)
        .unwrap();
    c.receive(ClientCommand::SwapInvItem {
        src_slot: 15,
        dst_slot: 23,
    })
    .unwrap();
    let detached = step(&mut c);
    assert_eq!(
        values(&detached.events, c.player_guid()).player_pack_slot(0),
        Some(guid)
    );
    let frontier = inventory(&c).next_item;
    let mana = c.world.state().resources.caster.mana;
    let health = c.world.state().health;
    let timer = c
        .world
        .state()
        .combat
        .as_ref()
        .unwrap()
        .melee_timers
        .remaining_ms();
    c.receive(lua_autoequip(guid)).unwrap();
    inject(&h);
    let failed = c.pump(c.tick_duration());
    paused(&failed);
    assert!(failed.events.is_empty());
    assert_eq!(c.world.state().combat.as_ref().unwrap().equipment, original);
    assert_eq!(inventory(&c).next_item, frontier);
    assert!(!inventory(&c).slots.contains_key(&23));
    assert_eq!(c.world.state().resources.caster.mana, mana);
    assert_eq!(c.world.state().health, health);
    assert_eq!(
        c.world
            .state()
            .combat
            .as_ref()
            .unwrap()
            .melee_timers
            .remaining_ms(),
        timer.map(|value| value.saturating_sub(c.world.profile().tick_ms))
    );
    let digest = c.world.state_digest();
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert!(!out.events.iter().any(|event| matches!(
        event,
        SessionEvent::ItemCreate { .. }
            | SessionEvent::ObjectDestroyed(_)
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::QuestObjectiveItem { .. }
    )));
    let fields = values(&out.events, c.player_guid());
    assert_eq!(fields.player_pack_slot(0), Some(0));
    assert_eq!(fields.player_inv_slot(15), Some(guid));
    assert_eq!(fields.player_visible_item_entry(15), Some(900002));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = enter_staff(&h, true);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.world.state().combat.as_ref().unwrap().equipment, original);
    assert_eq!(inventory(&c).next_item, frontier);
    assert_eq!(c.world.state().resources.caster.mana, mana);
    let initial = c.initial_events().unwrap();
    assert_eq!(
        initial
            .iter()
            .filter(|event| matches!(event, SessionEvent::ItemCreate { guid: g, .. } if *g == guid))
            .count(),
        1
    );
    assert_eq!(
        values(&initial, c.player_guid()).player_inv_slot(15),
        Some(guid)
    );
}
