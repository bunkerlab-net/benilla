// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual Lua backpack relocation with same-instance durable publication.
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
fn no_new_object_or_acquisition(events: &[SessionEvent]) {
    assert!(!events.iter().any(|event| matches!(
        event,
        SessionEvent::ItemCreate { .. }
            | SessionEvent::ObjectDestroyed(_)
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::QuestObjectiveItem { .. }
            | SessionEvent::QuestObjectivesComplete { .. }
    )));
}
#[test]
fn original_armor_lua_backpack_relocation_withholds_retry_reopens_and_reequips_same_object() {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let h = Harness::new();
    let mut c = enter_armor(&h, false);
    let initial = c.initial_events().unwrap();
    let guid = values(&initial, c.player_guid())
        .player_inv_slot(4)
        .unwrap();
    c.receive(ClientCommand::SwapInvItem {
        src_slot: 4,
        dst_slot: 23,
    })
    .unwrap();
    let detached = step(&mut c);
    no_new_object_or_acquisition(&detached.events);
    assert_eq!(
        values(&detached.events, c.player_guid()).player_pack_slot(0),
        Some(guid)
    );
    let instance = inventory(&c).slots[&23].clone();
    let frontier = inventory(&c).next_item;
    let stats = c.world.state().combat.as_ref().unwrap().stats.clone();
    let equipment = c.world.state().combat.as_ref().unwrap().equipment.clone();
    let quests = progress(&c).quests.clone();
    let mut ui = UiScript::new().unwrap();
    ui.keep_calls();
    ui.set_container(
        0,
        Some(ContainerState {
            name: Some("Backpack".into()),
            num_slots: 16,
            slots: [(
                1,
                ContainerSlot {
                    guid,
                    item_id: instance.item,
                    count: 1,
                    ..Default::default()
                },
            )]
            .into(),
        }),
    );
    ui.run("PickupContainerItem(0, 1); PickupContainerItem(0, 2)")
        .unwrap();
    assert!(ui.cursor_item().is_none());
    let moves = ui.take_container_moves();
    assert_eq!(moves.len(), 1);
    let request = &moves[0];
    let source = crate::ui_items::wire_pos(request.src_bag, request.src_slot).unwrap();
    let destination = crate::ui_items::wire_pos(request.dst_bag, request.dst_slot).unwrap();
    assert_eq!((source, destination), ((255, 23), (255, 24)));
    c.receive(ClientCommand::SwapInvItem {
        src_slot: source.1,
        dst_slot: destination.1,
    })
    .unwrap();
    let generation = c.store.generation();
    inject(&h);
    let failed = c.pump(c.tick_duration());
    paused(&failed);
    assert!(failed.events.is_empty());
    assert!(ui.item_locked(guid));
    assert_eq!(c.store.generation(), generation);
    assert_eq!(inventory(&c).slots[&24], instance);
    assert_eq!(inventory(&c).next_item, frontier);
    assert_eq!(c.world.state().combat.as_ref().unwrap().stats, stats);
    assert_eq!(
        c.world.state().combat.as_ref().unwrap().equipment,
        equipment
    );
    assert_eq!(progress(&c).quests, quests);
    let digest = c.world.state_digest();
    repair(&h);
    c.retry_checkpoint().unwrap();
    let committed = c.pump(Duration::ZERO);
    no_new_object_or_acquisition(&committed.events);
    let fields = values(&committed.events, c.player_guid());
    assert_eq!(fields.player_pack_slot(0), Some(0));
    assert_eq!(fields.player_pack_slot(1), Some(guid));
    assert_eq!(fields.player_visible_item_entry(4), None);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state_digest(), digest);
    drop(c);
    let mut c = enter_armor(&h, true);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(inventory(&c).slots[&24], instance);
    let initial = c.initial_events().unwrap();
    assert_eq!(
        initial
            .iter()
            .filter(|e| matches!(e, SessionEvent::ItemCreate { guid:g, .. } if *g == guid))
            .count(),
        1
    );
    assert_eq!(
        values(&initial, c.player_guid()).player_pack_slot(1),
        Some(guid)
    );
    c.receive(ClientCommand::SwapInvItem {
        src_slot: 24,
        dst_slot: 4,
    })
    .unwrap();
    let equipped = step(&mut c);
    no_new_object_or_acquisition(&equipped.events);
    assert_eq!(
        values(&equipped.events, c.player_guid()).player_inv_slot(4),
        Some(guid)
    );
    assert_eq!(inventory(&c).next_item, frontier);
    c.shutdown().unwrap();
    let digest = c.world.state_digest();
    drop(c);
    assert_eq!(enter_armor(&h, true).world.state_digest(), digest);
}
