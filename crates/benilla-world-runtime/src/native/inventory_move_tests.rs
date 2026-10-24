// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored component inventory; these tests admit no production content.
use super::synthetic_loot_fixture as fixture;
use super::synthetic_quest_fixture::command;
use super::*;
use benilla_sim::inventory as inv;
fn world() -> NativeWorld {
    let mut p = fixture::item_grant_profile();
    p.admin = Some(AdminPolicy {
        world: [1; 16],
        world_owner: synthetic_fixture::player().owner,
        allowlist: Default::default(),
        revision: 7,
    });
    NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42).unwrap()
}
fn step(w: &mut NativeWorld, c: Command) {
    let input = command(w, c);
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut fixture::LootAuthority::default(),
    )
    .unwrap();
}
fn grant(w: &mut NativeWorld, key: u8, item: u32, count: u32) {
    step(
        w,
        Command::Admin(AdminRequest {
            key: [key; 16],
            world: w.state.world,
            player: w.state.player,
            policy_revision: 7,
            operation: AdminOperation::GrantItem { item, count },
        }),
    );
}
fn movement(source: u8, destination: u8) -> Command {
    Command::InventoryMove(InventoryMoveRequest {
        source,
        destination,
    })
}
fn inventory(w: &NativeWorld) -> &inv::State {
    w.state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
}
fn restore(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
#[test]
fn moves_swaps_partial_merges_preserve_identity_counts_and_replay() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    grant(&mut w, 1, 70001, 7);
    let first = inventory(&w).slots[&23].id;
    let second = inventory(&w).slots[&24].id;
    let allocator = inventory(&w).next_item;
    step(&mut w, movement(23, 24));
    assert_eq!(
        (
            inventory(&w).slots[&23].count,
            inventory(&w).slots[&24].count
        ),
        (2, 5)
    );
    assert_eq!(
        (inventory(&w).slots[&23].id, inventory(&w).slots[&24].id),
        (first, second)
    );
    step(&mut w, movement(23, 38));
    assert!(!inventory(&w).slots.contains_key(&23));
    assert_eq!(inventory(&w).slots[&38].id, first);
    // Full destination takes the source swap branch, even for equal entries.
    step(&mut w, movement(38, 24));
    assert_eq!(
        (inventory(&w).slots[&38].id, inventory(&w).slots[&24].id),
        (second, first)
    );
    assert_eq!(inventory(&w).next_item, allocator);
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 7);
    assert!(!w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::InventoryStored(_) | Event::ItemAcquired(_) | Event::QuestProgression(_)
    )));
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn queued_moves_resolve_fifo_current_slots_and_noops_leave_inventory_unchanged() {
    let mut w = world();
    grant(&mut w, 1, 70001, 1);
    let original = inventory(&w).clone();
    let mut inputs = Vec::new();
    for (index, (source, destination)) in [(23, 38), (38, 24), (24, 23)].into_iter().enumerate() {
        let mut input = command(&w, movement(source, destination));
        input.input.sequence += index as u64;
        inputs.push(input);
    }
    w.step_batch(
        w.state.tick + 1,
        inputs,
        &mut fixture::LootAuthority::default(),
    )
    .unwrap();
    assert_eq!(inventory(&w), &original);
    for (source, destination) in [(23, 23), (37, 38)] {
        step(&mut w, movement(source, destination));
        assert_eq!(inventory(&w), &original);
    }
    assert_eq!(restore(&w).state_digest(), w.state_digest());
}
#[test]
fn unauthorized_inputs_and_unsupported_positions_roll_back_whole_frame() {
    let mut w = world();
    grant(&mut w, 1, 70001, 1);
    for fault in 0..4 {
        let mut input = command(&w, movement(23, 38));
        match fault {
            0 => input.authenticated_owner = [9; 16],
            1 => input.input.world = [9; 16],
            2 => input.input.player.character = [9; 16],
            _ => input.input.sequence += 1,
        }
        let before = w.checkpoint().unwrap();
        assert!(w
            .step(
                w.state.tick + 1,
                Some(input),
                &mut fixture::LootAuthority::default()
            )
            .is_err());
        assert_eq!(w.checkpoint().unwrap(), before);
    }
    for slot in [0, 19, 22, 39, 255] {
        let before = w.checkpoint().unwrap();
        let input = command(&w, movement(23, slot));
        assert!(w
            .step(
                w.state.tick + 1,
                Some(input),
                &mut fixture::LootAuthority::default()
            )
            .is_err());
        assert_eq!(w.checkpoint().unwrap(), before);
    }
}
#[test]
fn corrupted_movement_receipts_are_recomputed_during_replay() {
    let mut w = world();
    grant(&mut w, 1, 70001, 1);
    let before = w.checkpoint().unwrap();
    step(&mut w, movement(23, 38));
    let mut record = w.last_tick().unwrap().clone();
    let receipt = record
        .events
        .iter_mut()
        .find_map(|e| match e {
            Event::InventoryMoved(r) => Some(r),
            _ => None,
        })
        .unwrap();
    receipt.after.slots.get_mut(&38).unwrap().count += 1;
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), before).unwrap();
    let snapshot = replay.checkpoint().unwrap();
    assert!(replay.replay(&[record]).is_err());
    assert_eq!(replay.checkpoint().unwrap(), snapshot);
}
#[test]
fn previous_native_schema_is_not_silently_migrated() {
    let mut checkpoint = serde_json::to_value(world().checkpoint().unwrap()).unwrap();
    checkpoint["version"] = serde_json::json!(18);
    assert!(Checkpoint::decode(&serde_json::to_vec(&checkpoint).unwrap()).is_err());
}

#[test]
fn maximum_move_batch_stays_bounded_and_failed_later_input_rolls_back_prior_move() {
    let mut w = world();
    grant(&mut w, 1, 70003, 16);
    let snapshot = w.checkpoint().unwrap();
    let original = inventory(&w).clone();
    let inputs = (0..MAX_INPUTS_PER_TICK)
        .map(|index| {
            let mut input = command(&w, movement(23, 38));
            input.input.sequence += index as u64;
            input
        })
        .collect();
    w.step_batch(
        w.state.tick + 1,
        inputs,
        &mut fixture::LootAuthority::default(),
    )
    .unwrap();
    assert_eq!(inventory(&w).slots.len(), 16);
    assert_eq!(inventory(&w).next_item, original.next_item);
    assert!(serde_json::to_vec(w.last_tick().unwrap()).unwrap().len() < MAX_BYTES);
    assert!(w.checkpoint().unwrap().encode().unwrap().len() < MAX_BYTES);
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), snapshot).unwrap();
    replay.replay(&[w.last_tick().unwrap().clone()]).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let snapshot = w.checkpoint().unwrap();
    let first = command(&w, movement(23, 38));
    let mut second = command(&w, movement(23, 19));
    second.input.sequence += 1;
    assert!(w
        .step_batch(
            w.state.tick + 1,
            vec![first, second],
            &mut fixture::LootAuthority::default()
        )
        .is_err());
    assert_eq!(w.checkpoint().unwrap(), snapshot);
}
