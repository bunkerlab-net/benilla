// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual stock backpack auto-store, original instance identity and durable publication.
use super::*;
fn autostore(source: u8) -> ClientCommand {
    ClientCommand::AutoStoreBagItem {
        src_bag: 255,
        src_slot: source,
        dst_bag: 255,
    }
}
#[test]
fn stock_backpack_autostore_detaches_merges_then_reuses_original_after_saved_retry() {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let h = Harness::new();
    let (mut c, mut cache) = start(&h, 7);
    c.receive(command(23, 38)).unwrap();
    let out = step(&mut c);
    cache.apply(&out.events, c.player_guid());
    let source = cache.guid(38);
    let target = cache.guid(24);
    let allocator = inventory(&c).next_item;
    let quests = progress(&c).quests.clone();
    let mut ui = UiScript::new().unwrap();
    ui.set_container(
        0,
        Some(ContainerState {
            name: Some("Backpack".into()),
            num_slots: 16,
            slots: [(
                16,
                ContainerSlot {
                    guid: source,
                    item_id: 70001,
                    count: 5,
                    texture: Some("Authored icon".into()),
                    ..Default::default()
                },
            )]
            .into(),
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
    c.receive(autostore(22 + request.src_slot as u8)).unwrap();
    inject(&h);
    let out = c.pump(c.tick_duration());
    paused(&out);
    assert_eq!(cache.count(38), 5);
    assert_eq!(inventory(&c).slots[&23].count, 2);
    assert_eq!(inventory(&c).slots[&24].count, 5);
    let digest = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    no_acquisition(&out.events);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e,SessionEvent::ObjectDestroyed(g) if *g==source)));
    let detached=out.events.iter().position(|e|matches!(e,SessionEvent::ObjectValues{guid,fields} if *guid==source && fields.item_contained()==Some(0))).unwrap();
    let merged=out.events.iter().position(|e|matches!(e,SessionEvent::ObjectValues{guid,fields} if *guid==target && fields.item_stack_count()==Some(5))).unwrap();
    let restored=out.events.iter().rposition(|e|matches!(e,SessionEvent::ObjectValues{guid,fields} if *guid==source && fields.item_stack_count()==Some(2))).unwrap();
    assert!(detached < merged && merged < restored);
    cache.apply(&out.events, c.player_guid());
    assert_eq!(
        (cache.guid(23), cache.guid(24), cache.guid(38)),
        (source, target, 0)
    );
    assert_eq!((cache.count(23), cache.count(24)), (2, 5));
    assert_eq!(inventory(&c).next_item, allocator);
    assert_eq!(progress(&c).quests, quests);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().next_sequence, sequence);
    drop(c);
    let mut c = enter(&h, profile(7), true);
    assert_eq!(c.world.state_digest(), digest);
    let mut cache = ItemCache::default();
    cache.apply(&c.initial_events().unwrap(), c.player_guid());
    assert_eq!((cache.guid(23), cache.count(23)), (source, 2));
}
#[test]
fn backpack_autostore_full_merge_destroys_only_original_and_same_slot_clears_cursor() {
    let h = Harness::new();
    let (mut c, mut cache) = start(&h, 3);
    let original = cache.guid(23);
    let snapshot = inventory(&c).clone();
    c.receive(autostore(38)).unwrap();
    c.receive(autostore(23)).unwrap();
    let out = step(&mut c);
    assert_eq!(*inventory(&c), snapshot);
    no_acquisition(&out.events);
    let failures = out
        .events
        .iter()
        .filter_map(|e| {
            if let SessionEvent::InventoryFailure {
                reason,
                item_guid,
                item2_guid,
                ..
            } = e
            {
                Some((*reason, *item_guid, *item2_guid))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(failures, vec![(0, 0, 0)]);
    c.receive(ClientCommand::SplitItem {
        src_bag: 255,
        src_slot: 23,
        dst_bag: 255,
        dst_slot: 38,
        count: 1,
    })
    .unwrap();
    let out = step(&mut c);
    cache.apply(&out.events, c.player_guid());
    let target = cache.guid(38);
    let allocator = inventory(&c).next_item;
    c.receive(autostore(23)).unwrap();
    let out = step(&mut c);
    no_acquisition(&out.events);
    let merged=out.events.iter().position(|e|matches!(e,SessionEvent::ObjectValues{guid,fields} if *guid==target && fields.item_stack_count()==Some(3))).unwrap();
    let destroyed = out
        .events
        .iter()
        .position(|e| matches!(e,SessionEvent::ObjectDestroyed(g) if *g==original))
        .unwrap();
    assert!(merged < destroyed);
    cache.apply(&out.events, c.player_guid());
    assert_eq!(
        (cache.guid(23), cache.guid(38), cache.count(38)),
        (0, target, 3)
    );
    assert_eq!(inventory(&c).next_item, allocator);
    assert!(c
        .receive(ClientCommand::AutoStoreBagItem {
            src_bag: 255,
            src_slot: 38,
            dst_bag: 19
        })
        .is_err());
    let digest = c.world.state_digest();
    drop(c);
    assert_eq!(enter(&h, profile(3), true).world.state_digest(), digest);
}
