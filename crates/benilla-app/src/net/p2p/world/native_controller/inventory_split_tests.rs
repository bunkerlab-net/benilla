// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual stock split commands, durable outbox, original authored item metadata only.
use super::*;
fn split(source: u8, destination: u8, count: u8) -> ClientCommand {
    ClientCommand::SplitItem {
        src_bag: 255,
        src_slot: source,
        dst_bag: 255,
        dst_slot: destination,
        count,
    }
}
fn no_credit(events: &[SessionEvent]) {
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::ItemPushResult(_)
            | SessionEvent::QuestObjectiveItem { .. }
            | SessionEvent::QuestObjectivesComplete { .. }
    )));
}
#[test]
fn stock_cursor_split_waits_for_checkpoint_then_reopens_both_exact_instances() {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let h = Harness::new();
    let (mut c, mut cache) = start(&h, 7);
    let original = inventory(&c).slots[&23].clone();
    let source = cache.guid(23);
    let allocator = inventory(&c).next_item;
    let quests = progress(&c).quests.clone();
    let mut ui = UiScript::new().unwrap();
    ui.set_container(
        0,
        Some(ContainerState {
            name: Some("Backpack".into()),
            num_slots: 16,
            slots: [(
                1,
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
    ui.run("SplitContainerItem(0, 1, 2); PickupContainerItem(0, 16)")
        .unwrap();
    let moves = ui.take_container_moves();
    assert_eq!(moves.len(), 1);
    let request = &moves[0];
    assert_eq!(
        (request.src_bag, request.dst_bag, request.count),
        (0, 0, Some(2))
    );
    c.receive(split(
        22 + request.src_slot as u8,
        22 + request.dst_slot as u8,
        request.count.unwrap() as u8,
    ))
    .unwrap();
    inject(&h);
    let out = c.pump(c.tick_duration());
    paused(&out);
    assert_eq!(cache.count(23), 5);
    assert_eq!(cache.guid(38), 0);
    assert_eq!(
        (
            inventory(&c).slots[&23].count,
            inventory(&c).slots[&38].count
        ),
        (3, 2)
    );
    assert_eq!(inventory(&c).slots[&38].id, allocator);
    assert_eq!(inventory(&c).next_item, allocator + 1);
    let digest = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    assert!(c.retry_checkpoint().is_err());
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    no_credit(&out.events);
    cache.apply(&out.events, c.player_guid());
    assert_eq!((cache.count(23), cache.count(38)), (3, 2));
    assert_eq!(cache.guid(23), source);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ItemCreate { .. }))
            .count(),
        1
    );
    let source_update=out.events.iter().position(|e|matches!(e,SessionEvent::ObjectValues{guid,fields} if *guid==source&&fields.item_stack_count()==Some(3))).unwrap();
    let created = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::ItemCreate { .. }))
        .unwrap();
    assert!(source_update < created);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().next_sequence, sequence);
    assert_eq!(progress(&c).quests, quests);
    let cloned = inventory(&c).slots[&38].clone();
    drop(c);
    let mut reopened = enter(&h, profile(7), true);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(inventory(&reopened).slots[&23].id, original.id);
    assert_eq!(inventory(&reopened).slots[&38], cloned);
    let mut restored = ItemCache::default();
    restored.apply(&reopened.initial_events().unwrap(), reopened.player_guid());
    assert_eq!((restored.count(23), restored.count(38)), (3, 2));
}
#[test]
fn split_partial_merge_publishes_reduced_source_before_refill_and_burns_clone() {
    let h = Harness::new();
    let (mut c, mut cache) = start(&h, 7);
    let source = cache.guid(23);
    let target = cache.guid(24);
    let allocator = inventory(&c).next_item;
    let quests = progress(&c).quests.clone();
    c.receive(split(23, 24, 4)).unwrap();
    let out = step(&mut c);
    no_credit(&out.events);
    let counts = out
        .events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::ObjectValues { guid, fields } if *guid == source => {
                fields.item_stack_count()
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(counts, vec![1, 2]);
    cache.apply(&out.events, c.player_guid());
    assert_eq!((cache.count(23), cache.count(24)), (2, 5));
    assert_eq!((cache.guid(23), cache.guid(24)), (source, target));
    assert_eq!(inventory(&c).next_item, allocator + 1);
    assert_eq!(progress(&c).quests, quests);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::ItemCreate { .. })));
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ObjectDestroyed(_)))
            .count(),
        1
    );
    let digest = c.world.state_digest();
    drop(c);
    assert_eq!(enter(&h, profile(7), true).world.state_digest(), digest);
}
#[test]
fn split_refusal_burns_only_post_clone_identity_and_packet_noops_precede_topology() {
    let h = Harness::new();
    let (mut c, _) = start(&h, 7);
    let allocator = inventory(&c).next_item;
    let slots = inventory(&c).slots.clone();
    for command in [
        ClientCommand::SplitItem {
            src_bag: 3,
            src_slot: 99,
            dst_bag: 3,
            dst_slot: 99,
            count: 255,
        },
        ClientCommand::SplitItem {
            src_bag: 3,
            src_slot: 99,
            dst_bag: 4,
            dst_slot: 99,
            count: 0,
        },
    ] {
        assert!(c.receive(command).unwrap().events.is_empty());
    }
    assert!(c.actions.is_empty());
    c.receive(split(24, 23, 2)).unwrap();
    c.receive(split(24, 23, 3)).unwrap();
    c.receive(split(24, 23, 1)).unwrap();
    c.receive(split(23, 38, 2)).unwrap();
    let out = step(&mut c);
    let reasons = out
        .events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::InventoryFailure {
                reason, item2_guid, ..
            } => {
                assert_eq!(*item2_guid, 0);
                Some(*reason)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(reasons, vec![27, 26, 19]);
    assert_eq!(inventory(&c).slots[&24], slots[&24]);
    assert_eq!(inventory(&c).slots[&23].count, 3);
    assert_eq!(inventory(&c).slots[&38].id, allocator + 1);
    assert_eq!(inventory(&c).next_item, allocator + 2);
    let digest = c.world.state_digest();
    drop(c);
    let reopened = enter(&h, profile(7), true);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(inventory(&reopened).next_item, allocator + 2);
}
#[test]
fn ordinary_backpack_split_has_no_swap_alive_guard() {
    use benilla_sim::player_death as d;
    let h = Harness::new();
    let mut data = profile(7);
    // Loot fixture normally has a ten-HP kill target. This authored enemy must
    // survive the opening spell so its real melee callback can kill the player.
    data.npcs[0].max_health = 1000;
    let npc = data.npcs[0].combat.as_mut().unwrap();
    npc.minimum_damage = fixture::float(10000.0);
    npc.maximum_damage = fixture::float(10000.0);
    data.configure_player_death(
        d::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|x| (x, Some(false)))).unwrap(),
    )
    .unwrap();
    let mut c = enter(&h, data, false);
    c.receive(cast(&c)).unwrap();
    for _ in 0..200 {
        step(&mut c);
        if !c.world.state().player_alive() {
            break;
        }
    }
    assert!(!c.world.state().player_alive());
    let allocator = inventory(&c).next_item;
    c.receive(split(23, 38, 2)).unwrap();
    let out = step(&mut c);
    no_credit(&out.events);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::InventoryFailure { .. })));
    assert_eq!(inventory(&c).slots[&38].count, 2);
    assert_eq!(inventory(&c).next_item, allocator + 1);
    assert_eq!(c.world.state().health, 0);
}
