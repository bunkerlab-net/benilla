// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact selected deletion distinguishes ordinary duplicates from original registry custody.
use super::*;
use benilla_sim::quest_lifecycle as q;
use benilla_world_runtime::native::synthetic_equipment_fixture as armor;

fn enter_duplicate(h: &Harness, open: bool) -> NativeController {
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
        armor::profile_with_duplicate_armor(),
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
fn prepare(c: &mut NativeController) -> (u64, u64) {
    let initial = c.initial_events().unwrap();
    let fields = values(&initial, c.player_guid());
    let original_guid = fields.player_inv_slot(4).unwrap();
    let ordinary_guid = fields.player_pack_slot(0).unwrap();
    assert_ne!(original_guid, ordinary_guid);
    for (src_slot, dst_slot) in [(23, 38), (4, 23), (38, 24)] {
        c.receive(ClientCommand::SwapInvItem { src_slot, dst_slot })
            .unwrap();
        step(c);
    }
    assert_eq!(inventory(c).slots[&23].item, inventory(c).slots[&24].item);
    assert_ne!(inventory(c).slots[&23].id, inventory(c).slots[&24].id);
    let npc = c.projection.npc_guid(c.world.state().npcs[&3].id).unwrap();
    c.receive(ClientCommand::QuestgiverAccept { npc, quest: 70001 })
        .unwrap();
    step(c);
    let quest = progress(c).quests.progress(70001).unwrap();
    assert_eq!(quest.status, q::Status::Incomplete);
    (original_guid, ordinary_guid)
}
fn destroy(slot: u8) -> ClientCommand {
    ClientCommand::DestroyItem {
        bag_index: 255,
        slot,
        count: 0,
    }
}
fn lua_delete(guid: u64) -> ClientCommand {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let mut ui = UiScript::new().unwrap();
    ui.keep_calls();
    ui.set_container(
        0,
        Some(ContainerState {
            num_slots: 16,
            slots: [(
                2,
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
    ui.run("PickupContainerItem(0, 2); DeleteCursorItem()")
        .unwrap();
    let requests = ui.take_container_destroys();
    assert_eq!(requests, [(0, 2, 0)]);
    let (bag_index, slot) = crate::ui_items::wire_pos(requests[0].0, requests[0].1).unwrap();
    assert_eq!((bag_index, slot), (255, 24));
    ClientCommand::DestroyItem {
        bag_index,
        slot,
        count: requests[0].2 as u8,
    }
}
#[test]
fn lua_selected_ordinary_duplicate_preserves_original_and_unrelated_quest_on_retry_reopen() {
    let h = Harness::new();
    let mut c = enter_duplicate(&h, false);
    let (original_guid, ordinary_guid) = prepare(&mut c);
    let original = inventory(&c).slots[&23].clone();
    let ordinary = inventory(&c).slots[&24].clone();
    let quests = progress(&c).quests.view();
    let frontier = inventory(&c).next_item;
    let equipment = c.world.state().combat.as_ref().unwrap().equipment.clone();
    c.receive(lua_delete(ordinary_guid)).unwrap();
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
    assert_eq!(progress(&c).quests.view(), quests);
    let events = &c.world.last_tick().unwrap().events;
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event,
                n::Event::QuestProgression(n::QuestProgressionEvent::InventoryDestroyed(receipt))
                    if receipt.slot == 24 && receipt.removed == 1
                        && receipt.before.slots.get(&24) == Some(&ordinary)
                        && !receipt.after.slots.contains_key(&24)
                        && receipt.after.slots.get(&23) == Some(&original)
            ))
            .count(),
        1
    );
    let digest = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    assert!(c.retry_checkpoint().is_err());
    repair(&h);
    c.retry_checkpoint().unwrap();
    let published = c.pump(Duration::ZERO);
    assert_eq!(
        published
            .events
            .iter()
            .filter(|event| matches!(event,
                SessionEvent::ObjectDestroyed(guid) if *guid == ordinary_guid
            ))
            .count(),
        1
    );
    assert!(!published.events.iter().any(|event| matches!(event,
        SessionEvent::ObjectDestroyed(guid) if *guid == original_guid
    )));
    assert!(!published.events.iter().any(|event| matches!(
        event,
        SessionEvent::ItemPushResult(_) | SessionEvent::ItemCreate { .. }
    )));
    assert_eq!(
        values(&published.events, c.player_guid()).player_pack_slot(1),
        Some(0)
    );
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().next_sequence, sequence);
    drop(c);
    let mut reopened = enter_duplicate(&h, true);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(inventory(&reopened).slots[&23], original);
    assert!(!inventory(&reopened).slots.contains_key(&24));
    assert_eq!(inventory(&reopened).next_item, frontier);
    let initial = reopened.initial_events().unwrap();
    assert_eq!(
        values(&initial, reopened.player_guid()).player_pack_slot(0),
        Some(original_guid)
    );
    assert!(!initial.iter().any(|event| matches!(event,
        SessionEvent::ItemCreate { guid, .. } if *guid == ordinary_guid
    )));
    assert_eq!(progress(&reopened).quests.view(), quests);
}
#[test]
fn selecting_registered_original_retires_only_it_and_preserves_ordinary_duplicate() {
    let h = Harness::new();
    let mut c = enter_duplicate(&h, false);
    let (original_guid, ordinary_guid) = prepare(&mut c);
    let original = inventory(&c).slots[&23].clone();
    let ordinary = inventory(&c).slots[&24].clone();
    let frontier = inventory(&c).next_item;
    let quests = progress(&c).quests.view();
    c.receive(destroy(23)).unwrap();
    let out = step(&mut c);
    assert!(!inventory(&c).slots.contains_key(&23));
    assert_eq!(inventory(&c).slots[&24], ordinary);
    assert_eq!(inventory(&c).next_item, frontier);
    assert_eq!(progress(&c).quests.view(), quests);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ObjectDestroyed(g) if *g == original_guid))
            .count(),
        1
    );
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::ObjectDestroyed(g) if *g == ordinary_guid)));
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
    assert!(!progress(&c).inventory_receipts.contains_key(&original.id));
    let checkpoint = c.world.checkpoint().unwrap();
    drop(c);
    let mut reopened = enter_duplicate(&h, true);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert!(!inventory(&reopened).slots.contains_key(&23));
    assert_eq!(inventory(&reopened).slots[&24], ordinary);
    assert_eq!(inventory(&reopened).next_item, frontier);
    assert_eq!(progress(&reopened).quests.view(), quests);
    let initial = reopened.initial_events().unwrap();
    assert_eq!(
        values(&initial, reopened.player_guid()).player_pack_slot(1),
        Some(ordinary_guid)
    );
    assert!(!initial
        .iter()
        .any(|e| matches!(e, SessionEvent::ItemCreate {guid, ..} if *guid == original_guid)));
}

#[path = "registered_backpack_exchange_tests.rs"]
mod registered_backpack_exchange_tests;
