// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn split(source: u8, destination: u8, count: u8) -> Command {
    Command::InventorySplit(InventorySplitRequest {
        source,
        destination,
        count,
    })
}
#[test]
fn split_allocator_only_refusal_then_success_in_one_batch_replays_and_restores() {
    let mut w = world();
    grant(&mut w, 1, 70001, 7);
    let before = w.checkpoint().unwrap();
    let allocator = inventory(&w).next_item;
    let quests = w.state.quest_progression.as_ref().unwrap().quests.clone();
    let inputs = [split(24, 23, 1), split(23, 38, 2)]
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let mut input = command(&w, c);
            input.input.sequence += i as u64;
            input
        })
        .collect();
    w.step_batch(
        w.state.tick + 1,
        inputs,
        &mut fixture::LootAuthority::default(),
    )
    .unwrap();
    assert_eq!(inventory(&w).next_item, allocator + 2);
    assert_eq!(inventory(&w).slots[&38].id, allocator + 1);
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 7);
    assert_eq!(w.state.quest_progression.as_ref().unwrap().quests, quests);
    let record = w.last_tick().unwrap();
    let receipts = record
        .events
        .iter()
        .filter_map(|e| match e {
            Event::InventorySplit(r) => Some(r),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0].result, inv::InventoryResult::ItemCantStack);
    assert_eq!(receipts[0].after, receipts[1].before);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), before).unwrap();
    replay.replay(std::slice::from_ref(record)).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    assert_eq!(restore(&w).state_digest(), w.state_digest());
}
#[test]
fn later_invalid_input_rolls_back_split_clone_and_all_count_views() {
    let mut w = world();
    grant(&mut w, 1, 70001, 7);
    let before = w.checkpoint().unwrap();
    let a = command(&w, split(23, 38, 2));
    let mut b = command(&w, split(23, 39, 1));
    b.input.sequence += 1;
    assert!(w
        .step_batch(
            w.state.tick + 1,
            vec![a, b],
            &mut fixture::LootAuthority::default()
        )
        .is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
}
#[test]
fn split_receipt_allocator_and_intermediate_count_tamper_cannot_replay() {
    let mut w = world();
    grant(&mut w, 1, 70001, 7);
    let before = w.checkpoint().unwrap();
    step(&mut w, split(23, 24, 4));
    for fault in 0..3 {
        let mut record = w.last_tick().unwrap().clone();
        let r = record
            .events
            .iter_mut()
            .find_map(|e| match e {
                Event::InventorySplit(r) => Some(r),
                _ => None,
            })
            .unwrap();
        match fault {
            0 => r.after.next_item += 1,
            1 => r.views[0].slots.get_mut(&23).unwrap().count += 1,
            _ => r.clone_id = Some(1),
        };
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), before.clone())
                .unwrap();
        assert!(replay.replay(&[record]).is_err());
        assert_eq!(replay.checkpoint().unwrap(), before);
    }
}
