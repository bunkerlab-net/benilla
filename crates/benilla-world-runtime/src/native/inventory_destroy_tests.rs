// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn destroy(slot: u8, count: u8) -> Command {
    Command::InventoryDestroy(InventoryDestroyRequest { slot, count })
}
#[test]
fn destroy_later_duplicate_stack_preserves_earlier_instance_and_replays() {
    let mut w = world();
    grant(&mut w, 1, 70001, 7);
    let before = w.checkpoint().unwrap();
    let original = inventory(&w).slots[&23].clone();
    let allocator = inventory(&w).next_item;
    step(&mut w, destroy(24, 255));
    assert_eq!(inventory(&w).slots[&23], original);
    assert!(!inventory(&w).slots.contains_key(&24));
    assert_eq!(inventory(&w).next_item, allocator);
    let record = w.last_tick().unwrap();
    assert!(record.events.iter().any(|e|matches!(e,Event::QuestProgression(QuestProgressionEvent::InventoryDestroyed(r)) if r.slot==24&&r.removed==2)));
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), before).unwrap();
    replay.replay(std::slice::from_ref(record)).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    assert_eq!(restore(&w).state_digest(), w.state_digest());
}
#[test]
fn selected_destroy_receipt_tampering_and_later_bad_input_roll_back() {
    let mut w = world();
    grant(&mut w, 1, 70001, 7);
    let before = w.checkpoint().unwrap();
    let a = command(&w, destroy(24, 1));
    let mut b = command(&w, destroy(39, 0));
    b.input.sequence += 1;
    assert!(w
        .step_batch(
            w.state.tick + 1,
            vec![a, b],
            &mut fixture::LootAuthority::default()
        )
        .is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    step(&mut w, destroy(24, 1));
    for fault in 0..3 {
        let mut record = w.last_tick().unwrap().clone();
        let r = record
            .events
            .iter_mut()
            .find_map(|e| match e {
                Event::QuestProgression(QuestProgressionEvent::InventoryDestroyed(r)) => Some(r),
                _ => None,
            })
            .unwrap();
        match fault {
            0 => r.slot = 23,
            1 => r.removed += 1,
            _ => r.after.next_item += 1,
        }
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), before.clone())
                .unwrap();
        assert!(replay.replay(&[record]).is_err());
        assert_eq!(replay.checkpoint().unwrap(), before);
    }
}

// Authored rows exercise the pinned ItemRemovedQuestCheck integration; no stock
// data or native admission is inferred from this component fixture.
const ORDINARY_TRASH: u32 = 70004;

fn deliver_world(required: u32, with_reputation: bool) -> NativeWorld {
    use benilla_sim::{quest_lifecycle as q, reputation as rep};
    let (mut p, input, mut data) = fixture::item_grant_setup();
    let row = &mut data.quests[0];
    row.special_flags = q::DELIVER;
    row.required_creature = [0; 4];
    row.required_creature_count = [0; 4];
    row.required_item = [70001, 0, 0, 0];
    row.required_item_count = [required, 0, 0, 0];
    if with_reputation {
        assert!(data
            .quests
            .iter()
            .all(|q| !q.required_item.contains(&ORDINARY_TRASH)));
        data.reputation = Some(
            rep::Catalog::new_with_general_reputation_auras(
                vec![rep::Row {
                    faction: rep::Faction {
                        id: 72,
                        reputation_list_id: 3,
                        race_masks: [0; 4],
                        class_masks: [0; 4],
                        base_values: [0; 4],
                        flags: [0; 4],
                        team: 0,
                    },
                    reward_rates: rep::Resolved::Absent,
                    spillover: rep::Resolved::Absent,
                }],
                vec![],
                rep::Config {
                    gain_rate: benilla_sim::classic_spell::SpellFloat::ONE,
                    low_level_kill_rate: benilla_sim::classic_spell::SpellFloat::ONE,
                    low_level_quest_rate: benilla_sim::classic_spell::SpellFloat::ONE,
                },
                rep::Audit {
                    general_reputation_auras: rep::Evidence::Absent,
                    faction_reputation_auras: rep::Evidence::Absent,
                    forced_reactions: rep::Evidence::Absent,
                    reputation_scripts: rep::Evidence::Absent,
                },
                vec![],
            )
            .unwrap(),
        );
    }
    p.initialize_quest_progression(&input, data).unwrap();
    p.admin = Some(AdminPolicy {
        world: [1; 16],
        world_owner: synthetic_fixture::player().owner,
        allowlist: Default::default(),
        revision: 7,
    });
    NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42).unwrap()
}
fn deliver_progress(w: &NativeWorld) -> &benilla_sim::quest_lifecycle::Progress {
    w.state
        .quest_progression
        .as_ref()
        .unwrap()
        .quests
        .progress(70001)
        .unwrap()
}
fn accept_deliver(w: &mut NativeWorld) {
    let giver = w.state.npcs[&3].id;
    step(
        w,
        Command::AcceptQuest {
            quest: 70001,
            giver,
        },
    );
    assert_eq!(
        deliver_progress(w).status,
        benilla_sim::quest_lifecycle::Status::Incomplete
    );
}
fn replay_destroy(w: &NativeWorld, before: Checkpoint) {
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), before).unwrap();
    replay
        .replay(std::slice::from_ref(w.last_tick().unwrap()))
        .unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    assert_eq!(restore(w).state_digest(), w.state_digest());
}
#[test]
fn selected_destroy_deliver_complete_to_incomplete_precedes_inventory_publication() {
    use benilla_sim::quest_lifecycle as q;
    let mut w = deliver_world(7, false);
    accept_deliver(&mut w);
    grant(&mut w, 1, 70001, 7);
    assert_eq!(deliver_progress(&w).status, q::Status::Complete);
    assert_eq!(deliver_progress(&w).items[0], 7);
    let earlier = inventory(&w).slots[&23].clone();
    let before = w.checkpoint().unwrap();
    step(&mut w, destroy(24, 0));
    assert_eq!(inventory(&w).slots[&23], earlier);
    assert!(!inventory(&w).slots.contains_key(&24));
    assert_eq!(deliver_progress(&w).status, q::Status::Incomplete);
    assert_eq!(deliver_progress(&w).items[0], 5);
    let events = &w.last_tick().unwrap().events;
    let incomplete = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::Quest {
                    event: q::Event::LogComplete {
                        complete: false,
                        ..
                    },
                    ..
                })
            )
        })
        .expect("source quest callback before selected removal");
    let removed = events.iter().position(|e| matches!(e,
        Event::QuestProgression(QuestProgressionEvent::InventoryDestroyed(r)) if r.slot == 24 && r.removed == 2
    )).unwrap();
    assert!(incomplete < removed);
    replay_destroy(&w, before);
}
#[test]
fn selected_destroy_surplus_uses_aggregate_before_removal_not_selected_stack() {
    use benilla_sim::quest_lifecycle as q;
    let mut w = deliver_world(3, false);
    accept_deliver(&mut w);
    grant(&mut w, 1, 70001, 7);
    assert_eq!(deliver_progress(&w).status, q::Status::Complete);
    assert_eq!(deliver_progress(&w).items[0], 3);
    assert_eq!(inventory(&w).slots[&24].count, 2);
    let earlier = inventory(&w).slots[&23].clone();
    let before = w.checkpoint().unwrap();
    step(&mut w, destroy(24, 255));
    assert_eq!(inventory(&w).slots[&23], earlier);
    assert_eq!(inventory(&w).item_count(70001).unwrap(), 5);
    assert_eq!(deliver_progress(&w).status, q::Status::Complete);
    assert_eq!(deliver_progress(&w).items[0], 3);
    assert!(!w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::QuestProgression(QuestProgressionEvent::Quest {
            event: q::Event::LogComplete {
                complete: false,
                ..
            },
            ..
        })
    )));
    replay_destroy(&w, before);
}
#[test]
fn selected_ordinary_trash_preserves_enabled_reputation_owner_and_reopens() {
    let mut w = deliver_world(3, true);
    // The fixture asserts ORDINARY_TRASH is absent from every quest required-item
    // definition, not merely absent from accepted quests. This exercises the
    // all-catalog count map with an enabled reputation owner.
    grant(&mut w, 1, ORDINARY_TRASH, 7);
    let reputation = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation
        .clone();
    assert!(reputation.is_some());
    let before = w.checkpoint().unwrap();
    let earlier = inventory(&w).slots[&23].clone();
    step(&mut w, destroy(24, 1));
    assert_eq!(inventory(&w).slots[&23], earlier);
    assert_eq!(inventory(&w).slots[&24].count, 1);
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().reputation,
        reputation
    );
    replay_destroy(&w, before);
}
