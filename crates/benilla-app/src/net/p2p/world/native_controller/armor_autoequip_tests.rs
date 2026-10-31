// SPDX-License-Identifier: GPL-3.0-or-later
//! Real Lua auto-equip intent and the app's non-projectile sender, without Bevy dispatch.
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
    for event in events {
        if let SessionEvent::ObjectValues { guid, fields } = event {
            if *guid == player {
                result.merge(fields.clone());
            }
        }
    }
    result
}
fn no_new_object(events: &[SessionEvent]) {
    assert!(!events.iter().any(|event| matches!(
        event,
        SessionEvent::ItemCreate { .. }
            | SessionEvent::ObjectDestroyed(_)
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::QuestObjectiveItem { .. }
    )));
}
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
                    item_id: 900001,
                    count: 1,
                    item_class: Some(4),
                    inventory_type: 5,
                    equip_slots: vec![5],
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        }),
    );
    ui.set_item_template(
        900001,
        benilla_ui::script::ItemTemplateView {
            class: 4,
            inventory_type: 5,
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
    // The real sender's non-projectile branch; ammo/template dispatch is not emulated.
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
fn detach(c: &mut NativeController) -> u64 {
    let initial = c.initial_events().unwrap();
    let guid = values(&initial, c.player_guid())
        .player_inv_slot(4)
        .unwrap();
    c.receive(ClientCommand::SwapInvItem {
        src_slot: 4,
        dst_slot: 23,
    })
    .unwrap();
    let out = step(c);
    assert_eq!(
        values(&out.events, c.player_guid()).player_pack_slot(0),
        Some(guid)
    );
    guid
}

#[test]
fn lua_autoequip_real_sender_waits_for_checkpoint_then_restores_same_original_without_refill() {
    let h = Harness::new();
    let mut c = enter_armor(&h, false);
    let original = c.world.state().combat.as_ref().unwrap().equipment.clone();
    let maximum = c.world.state().combat.as_ref().unwrap().stats.max_mana;
    let guid = detach(&mut c);
    let carried = inventory(&c).slots[&23].clone();
    let frontier = inventory(&c).next_item;
    let mana = c.world.state().resources.caster.mana;
    assert!(c.world.state().combat.as_ref().unwrap().stats.max_mana < maximum);
    c.receive(lua_autoequip(guid)).unwrap();
    inject(&h);
    let failed = c.pump(c.tick_duration());
    paused(&failed);
    assert!(failed.events.is_empty());
    assert_eq!(c.world.state().combat.as_ref().unwrap().equipment, original);
    assert!(!inventory(&c).slots.contains_key(&23));
    assert_eq!(inventory(&c).next_item, frontier);
    assert_eq!(c.world.state().resources.caster.mana, mana);
    assert_eq!(
        c.world.state().combat.as_ref().unwrap().stats.max_mana,
        maximum
    );
    let digest = c.world.state_digest();
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    no_new_object(&out.events);
    let fields = values(&out.events, c.player_guid());
    assert_eq!(fields.player_pack_slot(0), Some(0));
    assert_eq!(fields.player_inv_slot(4), Some(guid));
    assert_eq!(fields.player_visible_item_entry(4), Some(carried.item));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = enter_armor(&h, true);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.world.state().combat.as_ref().unwrap().equipment, original);
    assert_eq!(inventory(&c).next_item, frontier);
    assert_eq!(c.world.state().resources.caster.mana, mana);
    let initial = c.initial_events().unwrap();
    assert_eq!(
        initial
            .iter()
            .filter(|event| matches!(event, SessionEvent::ItemCreate { guid:g, .. } if *g == guid))
            .count(),
        1
    );
    assert_eq!(
        values(&initial, c.player_guid()).player_inv_slot(4),
        Some(guid)
    );
}

#[test]
fn autoequip_refuses_actual_cast_and_prioritizes_actual_combat_over_casting() {
    let h = Harness::new();
    let mut c = enter_armor(&h, false);
    let guid = detach(&mut c);
    let before = inventory(&c).clone();
    let target = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 133,
        target: Some(target),
    })
    .unwrap();
    let cast = step(&mut c);
    assert!(cast
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellStart { spell_id: 133, .. })));
    assert!(!c.world.state().combat.as_ref().unwrap().in_combat);
    c.receive(lua_autoequip(guid)).unwrap();
    let refused = step(&mut c);
    assert!(refused.events.iter().any(|e| matches!(e, SessionEvent::InventoryFailure { reason: 39, item_guid, .. } if *item_guid == guid)));
    assert_eq!(inventory(&c), &before);
    for _ in 0..100 {
        step(&mut c);
        if c.world.state().combat.as_ref().unwrap().in_combat {
            break;
        }
    }
    assert!(c.world.state().combat.as_ref().unwrap().in_combat);
    c.receive(ClientCommand::CastSpell {
        spell_id: 133,
        target: Some(target),
    })
    .unwrap();
    c.receive(lua_autoequip(guid)).unwrap();
    let refused = step(&mut c);
    assert!(refused
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellStart { spell_id: 133, .. })));
    assert!(refused.events.iter().any(|e| matches!(e, SessionEvent::InventoryFailure { reason: 60, item_guid, .. } if *item_guid == guid)));
    assert!(!refused
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::InventoryFailure { reason: 39, .. })));
    assert_eq!(inventory(&c), &before);
    assert!(c
        .world
        .state()
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(4)
        .is_none());
    c.shutdown().unwrap();
    let digest = c.world.state_digest();
    drop(c);
    assert_eq!(enter_armor(&h, true).world.state_digest(), digest);
}
