// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual Lua auto-store intent and durable registered-original custody, without admission.
use super::*;
use benilla_world_runtime::native::synthetic_equipment_fixture as armor;

fn enter_armor(h: &Harness, open: bool, lethal: bool) -> NativeController {
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
fn no_acquisition(events: &[SessionEvent]) {
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::ItemCreate { .. }
            | SessionEvent::ObjectDestroyed(_)
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::QuestObjectiveItem { .. }
    )));
}
fn detach_and_relocate(c: &mut NativeController) -> u64 {
    let initial = c.initial_events().unwrap();
    let guid = values(&initial, c.player_guid())
        .player_inv_slot(4)
        .unwrap();
    c.receive(ClientCommand::SwapInvItem {
        src_slot: 4,
        dst_slot: 23,
    })
    .unwrap();
    step(c);
    c.receive(ClientCommand::SwapInvItem {
        src_slot: 23,
        dst_slot: 38,
    })
    .unwrap();
    let moved = step(c);
    assert_eq!(
        values(&moved.events, c.player_guid()).player_pack_slot(15),
        Some(guid)
    );
    guid
}
fn autostore(source: u8) -> ClientCommand {
    ClientCommand::AutoStoreBagItem {
        src_bag: 255,
        src_slot: source,
        dst_bag: 255,
    }
}
fn lua_autostore(guid: u64) -> ClientCommand {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let mut ui = UiScript::new().unwrap();
    ui.set_container(
        0,
        Some(ContainerState {
            num_slots: 16,
            slots: [(
                16,
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
    ui.run("PickupContainerItem(0, 16); PutItemInBackpack()")
        .unwrap();
    let requests = ui.take_bag_autostores();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(
        (
            request.src_bag,
            request.src_slot,
            request.dst_bag,
            request.count
        ),
        (0, 16, 0, None)
    );
    let (bag, slot) = crate::ui_items::wire_pos(request.src_bag, request.src_slot).unwrap();
    assert_eq!((bag, slot), (255, 38));
    autostore(slot)
}
#[test]
fn registered_armor_lua_autostore_reuses_original_at_first_empty_after_save_retry_and_reopen() {
    let h = Harness::new();
    let mut c = enter_armor(&h, false, false);
    let guid = detach_and_relocate(&mut c);
    let original = inventory(&c).slots[&38].clone();
    let frontier = inventory(&c).next_item;
    let stats = c.world.state().combat.as_ref().unwrap().stats.clone();
    let equipment = c.world.state().combat.as_ref().unwrap().equipment.clone();
    let recipe = serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]["combat"]
        ["recipe"]
        .clone();
    assert!(recipe.is_object());
    let calls = progress(&c).derived_calls.len();
    let quests = progress(&c).quests.clone();
    c.receive(lua_autostore(guid)).unwrap();
    inject(&h);
    let failed = c.pump(c.tick_duration());
    paused(&failed);
    assert!(failed.events.is_empty());
    assert_eq!(inventory(&c).slots[&23], original);
    assert!(!inventory(&c).slots.contains_key(&38));
    assert_eq!(inventory(&c).next_item, frontier);
    assert_eq!(c.world.state().combat.as_ref().unwrap().stats, stats);
    assert_eq!(
        c.world.state().combat.as_ref().unwrap().equipment,
        equipment
    );
    assert_eq!(
        serde_json::to_value(c.world.checkpoint().unwrap()).unwrap()["state"]["combat"]["recipe"],
        recipe
    );
    assert_eq!(progress(&c).quests, quests);
    assert_eq!(progress(&c).derived_calls.len(), calls + 1);
    let digest = c.world.state_digest();
    repair(&h);
    c.retry_checkpoint().unwrap();
    let published = c.pump(Duration::ZERO);
    no_acquisition(&published.events);
    let fields = values(&published.events, c.player_guid());
    assert_eq!(fields.player_pack_slot(15), Some(0));
    assert_eq!(fields.player_pack_slot(0), Some(guid));
    assert!(published.events.iter().any(|e| matches!(e, SessionEvent::ObjectValues { guid:g, fields } if *g==guid && fields.item_contained()==Some(0))));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = enter_armor(&h, true, false);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(inventory(&c).slots[&23], original);
    let initial = c.initial_events().unwrap();
    assert_eq!(
        initial
            .iter()
            .filter(|e| matches!(e, SessionEvent::ItemCreate { guid:g, .. } if *g==guid))
            .count(),
        1
    );
    assert_eq!(
        values(&initial, c.player_guid()).player_pack_slot(0),
        Some(guid)
    );
}
#[test]
fn actual_dead_player_can_autostore_original_and_same_slot_only_sends_zero_error() {
    let h = Harness::new();
    let mut c = enter_armor(&h, false, true);
    let guid = detach_and_relocate(&mut c);
    let original = inventory(&c).slots[&38].clone();
    let frontier = inventory(&c).next_item;
    let target = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 133,
        target: Some(target),
    })
    .unwrap();
    for _ in 0..200 {
        step(&mut c);
        if !c.world.state().player_alive() {
            break;
        }
    }
    assert!(
        !c.world.state().player_alive(),
        "actual lethal NPC combat required"
    );
    assert_eq!(inventory(&c).slots[&38], original);
    c.receive(autostore(38)).unwrap();
    let stored = step(&mut c);
    no_acquisition(&stored.events);
    assert_eq!(inventory(&c).slots[&23], original);
    assert_eq!(inventory(&c).next_item, frontier);
    assert_eq!(
        values(&stored.events, c.player_guid()).player_pack_slot(0),
        Some(guid)
    );
    assert!(!c.world.state().player_alive());
    let before = inventory(&c).clone();
    let calls = progress(&c).derived_calls.clone();
    c.receive(autostore(23)).unwrap();
    let unchanged = step(&mut c);
    no_acquisition(&unchanged.events);
    let errors: Vec<_> = unchanged
        .events
        .iter()
        .filter_map(|e| {
            if let SessionEvent::InventoryFailure {
                reason,
                item_guid,
                item2_guid,
                required_level,
                bag_slot,
            } = e
            {
                Some((*reason, *item_guid, *item2_guid, *required_level, *bag_slot))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(errors, [(0, 0, 0, None, 0)]);
    assert_eq!(inventory(&c), &before);
    assert_eq!(progress(&c).derived_calls, calls);
    c.shutdown().unwrap();
    let digest = c.world.state_digest();
    drop(c);
    let reopened = enter_armor(&h, true, true);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(inventory(&reopened), &before);
    assert!(!reopened.world.state().player_alive());
}
