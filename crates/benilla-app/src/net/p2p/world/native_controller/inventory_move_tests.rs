// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual backpack commands and durable item descriptors; original authored fixtures only.
use super::*;
use benilla_protocol::ObjectFields;
use std::collections::BTreeMap;

#[derive(Default)]
struct ItemCache {
    player: ObjectFields,
    items: BTreeMap<u64, ObjectFields>,
}
impl ItemCache {
    fn apply(&mut self, events: &[SessionEvent], player: u64) {
        for event in events {
            match event {
                SessionEvent::ItemCreate { guid, fields, .. } => {
                    assert!(self.items.insert(*guid, fields.clone()).is_none());
                }
                SessionEvent::ObjectValues { guid, fields } if *guid == player => {
                    self.player.merge(fields.clone());
                }
                SessionEvent::ObjectValues { guid, fields } => {
                    if let Some(item) = self.items.get_mut(guid) {
                        item.merge(fields.clone());
                    }
                }
                SessionEvent::ObjectDestroyed(guid) => {
                    self.items.remove(guid);
                }
                _ => {}
            }
        }
    }
    fn guid(&self, slot: u8) -> u64 {
        self.player
            .player_pack_slot(slot - inv::FIRST_SLOT)
            .unwrap_or(0)
    }
    fn count(&self, slot: u8) -> u32 {
        self.items[&self.guid(slot)].item_stack_count().unwrap()
    }
}
fn command(source: u8, destination: u8) -> ClientCommand {
    ClientCommand::SwapInvItem {
        src_slot: source,
        dst_slot: destination,
    }
}
fn start(h: &Harness, carried: u32) -> (NativeController, ItemCache) {
    let mut c = enter(h, profile(carried), false);
    let mut cache = ItemCache::default();
    cache.apply(&c.initial_events().unwrap(), c.player_guid());
    (c, cache)
}
fn no_acquisition(events: &[SessionEvent]) {
    assert!(!events.iter().any(|event| matches!(
        event,
        SessionEvent::ItemPushResult(_)
            | SessionEvent::ItemCreate { .. }
            | SessionEvent::QuestObjectiveItem { .. }
            | SessionEvent::QuestObjectivesComplete { .. }
    )));
}

#[test]
fn backpack_move_partial_merge_and_full_target_swap_retain_guids_in_fifo_order() {
    let h = Harness::new();
    let (mut c, mut cache) = start(&h, 7);
    let a = cache.guid(23);
    let b = cache.guid(24);
    let allocator = inventory(&c).next_item;
    let quests = progress(&c).quests.clone();
    assert_eq!((cache.count(23), cache.count(24)), (5, 2));
    c.receive(command(23, 25)).unwrap();
    c.receive(command(25, 24)).unwrap();
    let out = step(&mut c);
    no_acquisition(&out.events);
    cache.apply(&out.events, c.player_guid());
    assert_eq!((cache.guid(23), cache.guid(24), cache.guid(25)), (0, b, a));
    assert_eq!((cache.count(24), cache.count(25)), (5, 2));
    assert_eq!(inventory(&c).next_item, allocator);
    assert_eq!(progress(&c).quests, quests);
    // A full target takes the source swap branch, rather than discarding or merging.
    c.receive(command(25, 24)).unwrap();
    let out = step(&mut c);
    no_acquisition(&out.events);
    cache.apply(&out.events, c.player_guid());
    assert_eq!((cache.guid(24), cache.guid(25)), (a, b));
    assert_eq!((cache.count(24), cache.count(25)), (2, 5));
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 7);
    assert_eq!(inventory(&c).next_item, allocator);
}

#[test]
fn stock_cursor_move_waits_for_checkpoint_retry_then_reopens_exact_instance() {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let h = Harness::new();
    let (mut c, mut cache) = start(&h, 7);
    let guid = cache.guid(23);
    let instance = inventory(&c).slots[&23].clone();
    let mut ui = UiScript::new().unwrap();
    ui.set_container(
        0,
        Some(ContainerState {
            name: Some("Backpack".into()),
            num_slots: 16,
            slots: [(
                1,
                ContainerSlot {
                    guid,
                    item_id: 70001,
                    count: 5,
                    texture: Some("Authored icon".into()),
                    ..Default::default()
                },
            )]
            .into(),
        }),
    );
    ui.run("PickupContainerItem(0, 1); PickupContainerItem(0, 16)")
        .unwrap();
    assert!(ui.cursor_item().is_none());
    assert!(ui.item_locked(guid));
    let moves = ui.take_container_moves();
    assert_eq!(moves.len(), 1);
    let request = &moves[0];
    assert_eq!(
        (request.src_bag, request.dst_bag, request.count),
        (0, 0, None)
    );
    // The stock sender's backpack mapping is Lua 1..16 -> protocol 23..38.
    c.receive(command(
        22 + request.src_slot as u8,
        22 + request.dst_slot as u8,
    ))
    .unwrap();
    let generation = c.store.generation();
    inject(&h);
    let out = c.pump(c.tick_duration());
    paused(&out);
    cache.apply(&out.events, c.player_guid());
    assert_eq!((cache.guid(23), cache.guid(38)), (guid, 0));
    assert!(
        ui.item_locked(guid),
        "no successful slot update was delivered"
    );
    assert_eq!(inventory(&c).slots[&38], instance);
    assert_eq!(c.store.generation(), generation);
    let state = c.world.state_digest();
    let allocator = inventory(&c).next_item;
    let sequence = c.world.state().next_sequence;
    assert!(matches!(c.receive(command(38, 23)), Err(Error::Paused)));
    assert!(c.retry_checkpoint().is_err());
    assert!(c.pump(Duration::from_secs(600)).events.is_empty());
    repair(&h);
    c.retry_checkpoint().unwrap();
    assert_eq!(cache.guid(23), guid, "save receipt alone does not publish");
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    no_acquisition(&out.events);
    cache.apply(&out.events, c.player_guid());
    assert_eq!((cache.guid(23), cache.guid(38)), (0, guid));
    assert_eq!(cache.count(38), 5);
    // The actual app seat watcher unlocks the departed GUID from this pointer change.
    assert!(out.events.iter().any(|event| matches!(event,
        SessionEvent::ObjectValues { guid: player, fields }
        if *player == c.player_guid() && fields.player_pack_slot(0) == Some(0)
            && fields.player_pack_slot(15) == Some(guid))));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state_digest(), state);
    assert_eq!(c.world.state().next_sequence, sequence);
    assert_eq!(inventory(&c).next_item, allocator);
    drop(c);
    let mut reopened = enter(&h, profile(7), true);
    assert_eq!(reopened.world.state_digest(), state);
    assert_eq!(inventory(&reopened).slots[&38], instance);
    let initial = reopened.initial_events().unwrap();
    assert!(!initial
        .iter()
        .any(|e| matches!(e, SessionEvent::ItemPushResult(_))));
    let mut restored = ItemCache::default();
    restored.apply(&initial, reopened.player_guid());
    assert_eq!((restored.guid(23), restored.guid(38)), (0, guid));
    assert_eq!(restored.count(38), 5);
}

#[test]
fn newly_looted_stack_can_move_later_in_same_batch_without_repeating_acquisition() {
    let h = Harness::new();
    let mut c = ready(&h, 0);
    open(&mut c);
    c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .unwrap();
    c.receive(command(23, 38)).unwrap();
    let out = step(&mut c);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ItemPushResult(_)))
            .count(),
        1
    );
    assert_eq!(inventory(&c).slots[&38].count, 5);
    assert_eq!(inventory(&c).slots[&24].count, 2);
    assert!(!inventory(&c).slots.contains_key(&23));
    let push = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::ItemPushResult(_)))
        .unwrap();
    let moved = out
        .events
        .iter()
        .position(|e| {
            matches!(e,
        SessionEvent::ObjectValues { guid, fields } if *guid == c.player_guid()
            && fields.player_pack_slot(15).is_some_and(|g| g != 0))
        })
        .unwrap();
    assert!(push < moved);
}

#[test]
fn unsupported_topology_never_queues_and_empty_or_same_slot_inputs_are_silent() {
    let h = Harness::new();
    let (mut c, _) = start(&h, 7);
    for (source, destination) in [(19, 23), (22, 23), (23, 39), (255, 23)] {
        assert!(matches!(
            c.receive(command(source, destination)),
            Err(Error::UnsupportedCommand)
        ));
    }
    assert!(c.actions.is_empty());
    let before = inventory(&c).clone();
    c.receive(command(23, 23)).unwrap();
    c.receive(command(38, 24)).unwrap();
    let out = step(&mut c);
    assert_eq!(inventory(&c), &before);
    no_acquisition(&out.events);
    assert!(!out.events.iter().any(|e| matches!(
        e,
        SessionEvent::InventoryFailure { .. }
            | SessionEvent::Notification { .. }
            | SessionEvent::ObjectDestroyed(_)
    )));
}

#[test]
fn equipment_topology_queues_but_missing_source_authority_rejects_atomically() {
    let h = Harness::new();
    let (mut c, _) = start(&h, 7);
    let before = inventory(&c).clone();
    let digest = c.world.state_digest();
    let checkpoint = c.world.checkpoint().unwrap();
    c.receive(command(0, 23)).unwrap();
    assert_eq!(c.actions.len(), 1);
    let mut out = Pump::default();
    let result = c.advance_one(&mut out);
    assert!(
        matches!(
            result,
            Err(Error::NativeRuntime(n::NativeError::UnsupportedMechanic(
                "armor source interaction owner"
            )))
        ),
        "{result:?}"
    );
    assert_eq!(inventory(&c), &before);
    assert_eq!(c.world.state_digest(), digest);
    assert!(out.events.is_empty());
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    drop(c);
    let reopened = enter(&h, profile(7), true);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(inventory(&reopened), &before);
}

#[test]
fn dead_backpack_error_names_both_existing_items_once_and_noops_stay_silent() {
    let h = Harness::new();
    let (mut p, input, q) = setup(7);
    p.npcs[1].ai.reaction = benilla_sim::creature_ai::Reaction::Defensive;
    let combat = p.npcs[1].combat.as_mut().unwrap();
    combat.minimum_damage = fixture::float(10000.0);
    combat.maximum_damage = fixture::float(10000.0);
    p.configure_player_death(
        benilla_sim::player_death::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        benilla_sim::player_death::DeathSupport::checked(
            &benilla_sim::player_death::REQUIRED_ABSENCES.map(|d| (d, Some(false))),
        )
        .unwrap(),
    )
    .unwrap();
    p.initialize_quest_progression(&input, q).unwrap();
    let mut c = enter(&h, p, false);
    let mut cache = ItemCache::default();
    cache.apply(&c.initial_events().unwrap(), c.player_guid());
    let target = c.projection.npc_guid(c.world.state().npcs[&3].id).unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 60001,
        target: Some(target),
    })
    .unwrap();
    for _ in 0..100 {
        let out = step(&mut c);
        cache.apply(&out.events, c.player_guid());
        if c.world.state().health == 0 {
            break;
        }
    }
    assert_eq!(c.world.state().health, 0);
    let before = inventory(&c).clone();
    c.receive(command(23, 24)).unwrap();
    c.receive(command(23, 23)).unwrap();
    c.receive(command(38, 24)).unwrap();
    let out = step(&mut c);
    assert_eq!(inventory(&c), &before);
    let errors: Vec<_> = out
        .events
        .iter()
        .filter(|e| matches!(e, SessionEvent::InventoryFailure { .. }))
        .collect();
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0], SessionEvent::InventoryFailure {
        reason: 38, required_level: None, item_guid, item2_guid, bag_slot: 0,
    } if *item_guid == cache.guid(23) && *item2_guid == cache.guid(24)));
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::Notification { .. })));
    no_acquisition(&out.events);
}

#[test]
fn moving_admin_unbound_empty_bag_rebinds_same_guid_without_another_acquisition() {
    use benilla_ui::script::admin::AdminBackend;
    let h = Harness::new();
    let mut c = enter_grants(&h, false);
    let mut cache = ItemCache::default();
    cache.apply(&c.initial_events().unwrap(), c.player_guid());
    let backend = c.admin_backend();
    backend.submit(item_request(&backend, 1, 70003, 1)).unwrap();
    cache.apply(&step(&mut c).events, c.player_guid());
    let guid = cache.guid(23);
    let id = inventory(&c).slots[&23].id;
    assert!(!inventory(&c).slots[&23].bound);
    assert_eq!(cache.items[&guid].item_flags(), Some(0));
    c.receive(command(23, 38)).unwrap();
    let out = step(&mut c);
    no_acquisition(&out.events);
    cache.apply(&out.events, c.player_guid());
    assert_eq!(inventory(&c).slots[&38].id, id);
    assert!(inventory(&c).slots[&38].bound);
    assert_eq!((cache.guid(23), cache.guid(38)), (0, guid));
    assert_eq!(cache.items[&guid].item_flags(), Some(1));
    assert!(out.events.iter().any(|e| matches!(e,
        SessionEvent::ObjectValues { guid: changed, fields }
        if *changed == guid && fields.item_flags() == Some(1))));
    let digest = c.world.state_digest();
    drop(c);
    let mut reopened = enter_grants(&h, true);
    assert_eq!(reopened.world.state_digest(), digest);
    assert!(reopened
        .initial_events()
        .unwrap()
        .iter()
        .any(|e| matches!(e,
        SessionEvent::ItemCreate { guid: restored, fields, .. }
        if *restored == guid && fields.item_flags() == Some(1))));
}

#[path = "inventory_split_tests.rs"]
mod inventory_split_tests;

#[path = "inventory_destroy_tests.rs"]
mod inventory_destroy_tests;

#[path = "inventory_autostore_tests.rs"]
mod inventory_autostore_tests;
