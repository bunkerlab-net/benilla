// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored reference execution through the real controller/store publication path.
use super::*;

fn reference_profile() -> n::Profile {
    let (mut p, input, mut data) = setup(0);
    let loot = data.loot.as_mut().unwrap();
    let mut reference = loot.captures[0].clone();
    reference.source_fingerprint = [0xa1; 32];
    reference.loader_order.ordered_result_fingerprint = [0xb2; 32];
    let body = &mut loot.captures[0];
    body.rows[0].item = 0;
    body.rows[0].min_count_or_reference = -(reference.loot_id as i32);
    body.rows[0].max_count = 2;
    body.items.clear();
    loot.reference_captures = vec![reference];
    p.initialize_quest_progression(&input, data).unwrap();
    p
}

#[test]
fn reference_items_wait_for_durable_retry_and_reopen_without_reroll() {
    let h = Harness::new();
    let mut c = enter(&h, reference_profile(), false);
    c.initial_events().unwrap();
    kill(&mut c);
    let generated = c.world.state().npcs[&2].corpse_loot.clone().unwrap();
    assert_eq!(generated.generated.items.len(), 2);
    assert!(generated
        .generated
        .items
        .iter()
        .all(|item| item.item == 70001 && item.count == 7));
    open(&mut c);
    for slot in 0..2 {
        c.receive(ClientCommand::AutostoreLootItem { slot })
            .unwrap();
    }
    inject(&h);
    let generation = c.store.generation();
    paused(&c.pump(c.tick_duration()));
    assert_eq!(c.store.generation(), generation);
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 14);
    let digest = c.world.state_digest();
    let counter = c.world.state().rng_counter;
    let tick = c.tick();
    for _ in 0..2 {
        assert!(c.retry_checkpoint().is_err());
        assert!(c.pump(Duration::from_secs(60)).events.is_empty());
        assert_eq!(c.tick(), tick);
        assert_eq!(c.world.state_digest(), digest);
        assert_eq!(c.world.state().rng_counter, counter);
        assert_eq!(
            c.world.state().npcs[&2].corpse_loot.as_ref().unwrap().draws,
            generated.draws
        );
    }
    repair(&h);
    c.retry_checkpoint().unwrap();
    let delivered = c.pump(Duration::ZERO);
    assert_eq!(delivered.ticks, 0);
    assert_eq!(
        delivered
            .events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ItemPushResult(_)))
            .count(),
        2
    );
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state_digest(), digest);
    drop(c);
    let mut reopened = enter(&h, reference_profile(), true);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.world.state().rng_counter, counter);
    assert_eq!(inventory(&reopened).item_count(70001).unwrap(), 14);
    assert!(!reopened
        .initial_events()
        .unwrap()
        .iter()
        .any(|event| matches!(event, SessionEvent::ItemPushResult(_))));
    assert_eq!(
        reopened.world.state().npcs[&2]
            .corpse_loot
            .as_ref()
            .unwrap()
            .draws,
        generated.draws
    );
}
