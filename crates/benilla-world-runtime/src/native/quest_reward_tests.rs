// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored native transaction witnesses for pinned Player.cpp quest reward order.
//! Synthetic rows establish no Classic content admission or live client parity.
use super::synthetic_loot_fixture::{inventory_catalog, item, setup, LootAuthority};
use super::synthetic_quest_fixture::command;
use super::*;
use benilla_sim::{
    inventory as inv, player_progression as pg, quest_lifecycle as q, starter_init as init,
};

const QUEST: u32 = 70001;
const FILL: u32 = 70999;
const REQUIRED: u32 = 70001;
const CHOSEN: u32 = 70002;
const FIXED: u32 = 70003;
const LAST: u32 = 70004;
const FILLER: u32 = 70010;

fn ordinary(row: &mut q::QuestRow) {
    row.special_flags = 0;
    row.required_creature = [0; 4];
    row.required_creature_count = [0; 4];
    row.required_item = [0; 4];
    row.required_item_count = [0; 4];
    row.excluded.reward_choice_item = [0; 6];
    row.excluded.reward_choice_count = [0; 6];
    row.excluded.reward_item = [0; 4];
    row.excluded.reward_item_count = [0; 4];
}
fn catalog_items(data: &mut QuestProgressionInput, replacements: Vec<inv::Item>) {
    let loot = data.loot.as_mut().unwrap();
    let mut rows: BTreeMap<_, _> = loot
        .inventory
        .items()
        .map(|r| (r.entry, r.clone()))
        .collect();
    for row in replacements {
        rows.insert(row.entry, row);
    }
    loot.inventory = inventory_catalog(rows.into_values().collect());
}
fn fixture() -> (Profile, init::StarterInput, QuestProgressionInput) {
    let (p, i, mut data) = setup();
    ordinary(&mut data.quests[0]);
    let mut filler = item(FILLER);
    filler.stackable = 1;
    catalog_items(
        &mut data,
        vec![item(CHOSEN), item(FIXED), item(LAST), filler],
    );
    let loot = data.loot.as_mut().unwrap();
    loot.captures[0].rows[0].chance_or_quest_chance_bits = 100f32.to_bits();
    loot.captures[0].rows[0].min_count_or_reference = 1;
    loot.captures[0].rows[0].max_count = 1;
    (p, i, data)
}
fn append_quest(data: &mut QuestProgressionInput, row: q::QuestRow) {
    let relation = q::CreatureQuestRelation {
        creature: 60003,
        quest: row.id,
    };
    data.relations.starters.push(relation.clone());
    data.relations.finishers.push(relation);
    data.quests.push(row);
    data.quest_relationships = synthetic_quest_fixture::relationship_capture(&data.quests);
}
fn fill_quest(data: &mut QuestProgressionInput, rewards: &[(u32, u32)]) {
    let mut row = data.quests[0].clone();
    ordinary(&mut row);
    row.id = FILL;
    row.reward.xp.reward_money_max_level = 0;
    row.reward.reward_or_required_money = 0;
    for (index, &(entry, count)) in rewards.iter().enumerate() {
        row.excluded.reward_item[index] = entry;
        row.excluded.reward_item_count[index] = count;
    }
    append_quest(data, row);
}
fn world_from(
    mut profile: Profile,
    input: init::StarterInput,
    data: QuestProgressionInput,
) -> NativeWorld {
    profile.initialize_quest_progression(&input, data).unwrap();
    NativeWorld::synthetic_component_test(profile, [1; 16], synthetic_fixture::player(), 42)
        .unwrap()
}
fn progress(w: &NativeWorld) -> &QuestProgressionState {
    w.state.quest_progression.as_ref().unwrap()
}
fn inventory(w: &NativeWorld) -> &inv::State {
    progress(w).inventory.as_ref().unwrap()
}
fn step(w: &mut NativeWorld, authority: &mut LootAuthority, c: Command) {
    let input = command(w, c);
    w.step(w.state.tick + 1, Some(input), authority).unwrap();
}
fn accept(w: &mut NativeWorld, authority: &mut LootAuthority, quest: u32) {
    let giver = w.state.npcs[&3].id;
    step(w, authority, Command::AcceptQuest { quest, giver });
    assert!(progress(w).quests.progress(quest).is_some());
}
fn turn_in(w: &mut NativeWorld, authority: &mut LootAuthority, quest: u32) {
    let giver = w.state.npcs[&3].id;
    step(w, authority, Command::TurnInQuest { quest, giver });
}
fn choose(w: &mut NativeWorld, authority: &mut LootAuthority, choice: u32) {
    let giver = w.state.npcs[&3].id;
    step(
        w,
        authority,
        Command::ChooseQuestReward {
            quest: QUEST,
            giver,
            choice,
        },
    );
}
fn fill(w: &mut NativeWorld, authority: &mut LootAuthority) {
    accept(w, authority, FILL);
    turn_in(w, authority, FILL);
    assert!(progress(w).quests.progress(FILL).unwrap().rewarded);
}
fn events(w: &NativeWorld) -> &[Event] {
    &w.last_tick().unwrap().events
}
fn stores(w: &NativeWorld) -> Vec<&inv::Stored> {
    events(w)
        .iter()
        .filter_map(|e| match e {
            Event::QuestProgression(QuestProgressionEvent::RewardItem(stored)) => {
                Some(stored.as_ref())
            }
            _ => None,
        })
        .collect()
}
fn restore(w: &NativeWorld) -> NativeWorld {
    let checkpoint = Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap();
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint).unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
    restored
}
fn reward_position(events: &[Event], item: u32) -> usize {
    events.iter().position(|e| matches!(e, Event::QuestProgression(QuestProgressionEvent::RewardItem(stored)) if stored.item == item)).unwrap()
}
fn quest_position(events: &[Event], expected: &q::Event) -> usize {
    events.iter().position(|e| matches!(e, Event::QuestProgression(QuestProgressionEvent::Quest {event, ..}) if event == expected)).unwrap()
}
fn assert_refused(w: &NativeWorld, item: u32, result: inv::InventoryResult) {
    assert!(events(w).iter().any(|e| matches!(e, Event::Rejected(Rejection::Quest(q::Gate::RewardStorage {item: actual, result: actual_result})) if *actual == item && *actual_result == result)));
    assert!(events(w).iter().any(|e| matches!(e, Event::QuestProgression(QuestProgressionEvent::RewardRefused {quest: QUEST, result: actual, ..}) if *actual == Some(result))));
    assert!(stores(w).is_empty());
}

#[test]
fn chosen_then_fixed_items_precede_xp_and_restore_replay_awards_exactly_once() {
    let (p, i, mut data) = fixture();
    let row = &mut data.quests[0];
    row.excluded.reward_choice_item = [LAST, CHOSEN, 0, 0, 0, 0];
    row.excluded.reward_choice_count = [1, 2, 0, 0, 0, 0];
    row.excluded.reward_item = [FIXED, LAST, 0, 0];
    row.excluded.reward_item_count = [3, 1, 0, 0];
    let mut w = world_from(p, i, data);
    let start = w.checkpoint().unwrap();
    let mut authority = LootAuthority::default();
    accept(&mut w, &mut authority, QUEST);
    let accepted = w.checkpoint().unwrap();
    w.checkpoint_committed(&accepted).unwrap();
    choose(&mut w, &mut authority, 1);
    assert_eq!(
        stores(&w)
            .iter()
            .map(|s| (s.item, s.count))
            .collect::<Vec<_>>(),
        vec![(CHOSEN, 2), (FIXED, 3), (LAST, 1)]
    );
    let events = events(&w);
    let xp = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::Progression(_))
            )
        })
        .unwrap();
    assert!(reward_position(events, CHOSEN) < reward_position(events, FIXED));
    assert!(reward_position(events, FIXED) < reward_position(events, LAST));
    assert!(reward_position(events, LAST) < xp);
    assert!(progress(&w).quests.progress(QUEST).unwrap().rewarded);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), accepted).unwrap();
    replay
        .replay(&decode_records(&encode_records(w.records()).unwrap()).unwrap())
        .unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    assert_ne!(w.checkpoint().unwrap().identity(), start.identity());
    let expected_inventory = inventory(&w).clone();
    let expected_receipts = progress(&w).inventory_receipts.clone();
    let expected_money = progress(&w).money;
    let expected_level_xp = (w.player_level(), progress(&w).player.view().xp);
    w = restore(&w);
    choose(&mut w, &mut authority, 1);
    turn_in(&mut w, &mut authority, QUEST);
    assert_eq!(inventory(&w), &expected_inventory);
    assert_eq!(progress(&w).inventory_receipts, expected_receipts);
    assert_eq!(progress(&w).money, expected_money);
    assert_eq!(
        (w.player_level(), progress(&w).player.view().xp),
        expected_level_xp
    );
    assert!(w
        .records()
        .iter()
        .flat_map(|r| &r.events)
        .all(|e| !matches!(
            e,
            Event::QuestProgression(
                QuestProgressionEvent::RewardItem(_)
                    | QuestProgressionEvent::Quest {
                        event: q::Event::RewardAnnouncement { .. },
                        ..
                    }
            )
        )));
    restore(&w);
}

#[test]
fn full_bag_refuses_before_required_item_destruction_even_if_removal_would_free_space() {
    let (p, i, mut data) = fixture();
    data.quests[0].special_flags = q::DELIVER;
    data.quests[0].required_item = [REQUIRED, 0, 0, 0];
    data.quests[0].required_item_count = [1, 0, 0, 0];
    data.quests[0].excluded.reward_item = [FIXED, 0, 0, 0];
    data.quests[0].excluded.reward_item_count = [1, 0, 0, 0];
    fill_quest(&mut data, &[(FILLER, 15), (REQUIRED, 1)]);
    let mut w = world_from(p, i, data);
    let mut authority = LootAuthority::default();
    fill(&mut w, &mut authority);
    accept(&mut w, &mut authority, QUEST);
    assert_eq!(inventory(&w).slots.len(), 16);
    let before = progress(&w).clone();
    turn_in(&mut w, &mut authority, QUEST);
    assert_refused(&w, FIXED, inv::InventoryResult::InventoryFull);
    assert_eq!(inventory(&w), before.inventory.as_ref().unwrap());
    assert_eq!(progress(&w).inventory_receipts, before.inventory_receipts);
    assert_eq!(progress(&w).quests, before.quests);
    assert_eq!(progress(&w).money, before.money);
    assert_eq!(progress(&w).player.view().xp, before.player.view().xp);
    assert!(events(&w).iter().all(|e| !matches!(
        e,
        Event::QuestProgression(QuestProgressionEvent::Quest {
            event: q::Event::Host(_) | q::Event::RemoveTimedQuest(_),
            ..
        })
    )));
    restore(&w);
}

#[test]
fn shared_preflight_reserves_whole_positions_instead_of_aggregate_stack_capacity() {
    let (p, i, mut data) = fixture();
    data.quests[0].excluded.reward_item = [FIXED, FIXED, 0, 0];
    data.quests[0].excluded.reward_item_count = [2, 3, 0, 0];
    fill_quest(&mut data, &[(FILLER, 15)]);
    let mut w = world_from(p, i, data);
    let mut authority = LootAuthority::default();
    fill(&mut w, &mut authority);
    accept(&mut w, &mut authority, QUEST);
    let catalog = &w
        .profile
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap()
        .inventory;
    assert_eq!(
        inventory(&w).can_store(catalog, FIXED, 5).unwrap().result,
        inv::InventoryResult::Ok
    );
    let before = inventory(&w).clone();
    turn_in(&mut w, &mut authority, QUEST);
    assert_refused(&w, FIXED, inv::InventoryResult::InventoryFull);
    assert_eq!(inventory(&w), &before);
    assert!(!progress(&w).quests.progress(QUEST).unwrap().rewarded);
    restore(&w);
}

#[test]
fn later_actual_unique_refusal_skips_only_that_grant_and_still_rewards_quest_and_xp() {
    let (p, i, mut data) = fixture();
    let mut unique = item(FIXED);
    unique.maximum_count = 1;
    catalog_items(&mut data, vec![unique]);
    data.quests[0].excluded.reward_item = [FIXED, FIXED, 0, 0];
    data.quests[0].excluded.reward_item_count = [1, 1, 0, 0];
    fill_quest(&mut data, &[(FILLER, 14)]);
    let mut w = world_from(p, i, data);
    let mut authority = LootAuthority::default();
    fill(&mut w, &mut authority);
    accept(&mut w, &mut authority, QUEST);
    let before = (w.player_level(), progress(&w).player.view().xp);
    turn_in(&mut w, &mut authority, QUEST);
    assert_eq!(
        stores(&w)
            .iter()
            .map(|s| (s.item, s.count))
            .collect::<Vec<_>>(),
        vec![(FIXED, 1)]
    );
    assert_eq!(inventory(&w).item_count(FIXED).unwrap(), 1);
    assert_eq!(inventory(&w).slots.len(), 15);
    assert!(progress(&w).quests.progress(QUEST).unwrap().rewarded);
    assert_ne!((w.player_level(), progress(&w).player.view().xp), before);
    assert!(events(&w).iter().any(|e| matches!(
        e,
        Event::QuestProgression(QuestProgressionEvent::Quest {
            event: q::Event::RewardAnnouncement { quest: QUEST, .. },
            ..
        })
    )));
    assert!(events(&w).iter().all(|e| !matches!(
        e,
        Event::QuestProgression(QuestProgressionEvent::RewardRefused { .. })
    )));
    restore(&w);
}

#[test]
fn empty_in_range_choice_is_legal_but_six_and_larger_are_rejected() {
    let (p, i, mut data) = fixture();
    data.quests[0].excluded.reward_choice_item = [CHOSEN, 0, 0, 0, 0, 0];
    data.quests[0].excluded.reward_choice_count = [1, 0, 0, 0, 0, 0];
    data.quests[0].excluded.reward_item = [FIXED, 0, 0, 0];
    data.quests[0].excluded.reward_item_count = [2, 0, 0, 0];
    let mut w = world_from(p, i, data);
    let mut authority = LootAuthority::default();
    accept(&mut w, &mut authority, QUEST);
    let before = inventory(&w).clone();
    for choice in [6, u32::MAX] {
        choose(&mut w, &mut authority, choice);
        assert_eq!(inventory(&w), &before);
        assert!(events(&w)
            .iter()
            .any(|e| matches!(e, Event::Rejected(Rejection::Quest(q::Gate::RewardChoice)))));
        assert!(!progress(&w).quests.progress(QUEST).unwrap().rewarded);
    }
    choose(&mut w, &mut authority, 5);
    assert_eq!(inventory(&w).item_count(CHOSEN).unwrap(), 0);
    assert_eq!(inventory(&w).item_count(FIXED).unwrap(), 2);
    assert!(progress(&w).quests.progress(QUEST).unwrap().rewarded);
    assert!(events(&w).iter().any(|e| matches!(
        e,
        Event::QuestProgression(QuestProgressionEvent::Progression(_))
    )));
    restore(&w);
}

#[test]
fn required_item_removal_callbacks_and_reward_recompletion_precede_store_and_xp() {
    let (p, i, mut data) = fixture();
    let row = &mut data.quests[0];
    row.special_flags = q::DELIVER;
    row.required_item = [REQUIRED, 0, 0, 0];
    row.required_item_count = [1, 0, 0, 0];
    row.excluded.reward_item = [REQUIRED, 0, 0, 0];
    row.excluded.reward_item_count = [1, 0, 0, 0];
    let mut observer = row.clone();
    observer.id = 70011;
    observer.excluded.reward_item = [0; 4];
    observer.excluded.reward_item_count = [0; 4];
    append_quest(&mut data, observer);
    fill_quest(&mut data, &[(REQUIRED, 1)]);
    let mut w = world_from(p, i, data);
    let mut authority = LootAuthority::default();
    fill(&mut w, &mut authority);
    accept(&mut w, &mut authority, QUEST);
    accept(&mut w, &mut authority, 70011);
    let removed_id = inventory(&w).slots[&inv::FIRST_SLOT].id;
    turn_in(&mut w, &mut authority, QUEST);
    let e = events(&w);
    let incomplete = quest_position(
        e,
        &q::Event::LogComplete {
            slot: 0,
            complete: false,
        },
    );
    let destroy = quest_position(
        e,
        &q::Event::Host(q::HostEffect::DestroyCarriedStack {
            item: REQUIRED,
            count: 1,
        }),
    );
    let timed = quest_position(e, &q::Event::RemoveTimedQuest(QUEST));
    let credit = quest_position(
        e,
        &q::Event::ItemCredit {
            quest: QUEST,
            objective: 0,
            previous: 0,
            added: 1,
        },
    );
    let complete = quest_position(
        e,
        &q::Event::LogComplete {
            slot: 0,
            complete: true,
        },
    );
    let stored = reward_position(e, REQUIRED);
    let clear = quest_position(
        e,
        &q::Event::LogSet {
            slot: 0,
            quest: None,
        },
    );
    let xp = e
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::QuestProgression(QuestProgressionEvent::Progression(_))
            )
        })
        .unwrap();
    assert!(
        incomplete < destroy
            && destroy < timed
            && timed < credit
            && credit < complete
            && complete < stored
            && stored < clear
            && clear < xp
    );
    assert_eq!(
        progress(&w).quests.progress(70011).unwrap().status,
        q::Status::Complete
    );
    assert_eq!(progress(&w).quests.progress(70011).unwrap().items[0], 1);
    assert!(!progress(&w).quests.progress(70011).unwrap().rewarded);
    assert_eq!(inventory(&w).item_count(REQUIRED).unwrap(), 1);
    assert_ne!(inventory(&w).slots[&inv::FIRST_SLOT].id, removed_id);
    assert!(!progress(&w).inventory_receipts.contains_key(&removed_id));
    assert_eq!(progress(&w).inventory_receipts.len(), 1);
    restore(&w);
}

#[test]
fn admission_requires_inventory_templates_dense_rewards_and_supported_effects() {
    let (mut p, i, mut data) = fixture();
    data.quests[0].excluded.reward_item = [FIXED, 0, 0, 0];
    data.quests[0].excluded.reward_item_count = [1, 0, 0, 0];
    data.loot = None;
    data.kill.audit.native_loot_or_money = pg::Evidence::Absent;
    assert_eq!(
        p.initialize_quest_progression(&i, data),
        Err(NativeError::UnsupportedMechanic(
            "quest items require inventory host"
        ))
    );
    for choice in [false, true] {
        let (mut p, i, mut data) = fixture();
        if choice {
            data.quests[0].excluded.reward_choice_item[0] = 79999;
            data.quests[0].excluded.reward_choice_count[0] = 1;
        } else {
            data.quests[0].excluded.reward_item[0] = 79999;
            data.quests[0].excluded.reward_item_count[0] = 1;
        }
        assert!(matches!(
            p.initialize_quest_progression(&i, data),
            Err(NativeError::Inventory(inv::Error::Invalid("unknown_item")))
        ));
    }
    // Deliberately malformed/unsupported genesis rows, never live-state mutation.
    for case in 0..12 {
        let (mut p, i, mut data) = fixture();
        let e = &mut data.quests[0].excluded;
        match case {
            0 => {
                e.reward_item[1] = FIXED;
                e.reward_item_count[1] = 1;
            }
            1 => {
                e.reward_choice_item[1] = CHOSEN;
                e.reward_choice_count[1] = 1;
            }
            2 => e.reward_item[0] = FIXED,
            3 => e.reward_choice_count[0] = 1,
            4 => {
                e.reward_item[0] = FIXED;
                e.reward_item_count[0] = 10001;
            }
            5 => e.source_item = FIXED,
            6 => e.reward_spell = 60001,
            7 => e.reward_spell_cast = 60001,
            8 => e.reward_mail_template = 1,
            9 => e.reward_rep_faction[0] = 1,
            10 => e.complete_script = 1,
            11 => e.start_script = 1,
            _ => unreachable!(),
        }
        assert!(
            p.initialize_quest_progression(&i, data).is_err(),
            "admitted unsupported case {case}"
        );
    }
}

#[test]
fn stackable_255_splits_award_256_while_stackable_256_is_not_admitted() {
    let (p, i, mut data) = fixture();
    let mut row = item(FIXED);
    row.stackable = 255;
    catalog_items(&mut data, vec![row]);
    data.quests[0].excluded.reward_item[0] = FIXED;
    data.quests[0].excluded.reward_item_count[0] = 256;
    let mut w = world_from(p, i, data);
    let mut authority = LootAuthority::default();
    accept(&mut w, &mut authority, QUEST);
    turn_in(&mut w, &mut authority, QUEST);
    let stores = stores(&w);
    assert_eq!(stores.len(), 1);
    assert_eq!(stores[0].count, 256);
    assert_eq!(
        stores[0]
            .destinations
            .iter()
            .map(|d| d.count)
            .collect::<Vec<_>>(),
        vec![255, 1]
    );
    assert_eq!(stores[0].last_count, 1);
    assert_eq!(stores[0].last_slot, inv::FIRST_SLOT + 1);
    assert_eq!(inventory(&w).item_count(FIXED).unwrap(), 256);
    restore(&w);
    let (mut p, i, mut data) = fixture();
    // Malformed catalog is deserialized only to test genesis validation.
    let mut encoded = serde_json::to_value(&data.loot.as_ref().unwrap().inventory).unwrap();
    encoded["rows"][FIXED.to_string()]["stackable"] = 256.into();
    data.loot.as_mut().unwrap().inventory = serde_json::from_value(encoded).unwrap();
    assert!(matches!(
        p.initialize_quest_progression(&i, data),
        Err(NativeError::Inventory(inv::Error::Invalid("item_template")))
    ));
}

#[test]
fn allocator_exhaustion_after_first_grant_rolls_back_entire_frame_and_owner_sequence() {
    let (p, i, mut data) = fixture();
    data.quests[0].excluded.reward_choice_item[0] = CHOSEN;
    data.quests[0].excluded.reward_choice_count[0] = 1;
    data.quests[0].excluded.reward_item[0] = FIXED;
    data.quests[0].excluded.reward_item_count[0] = 1;
    let mut w = world_from(p, i, data);
    let mut authority = LootAuthority::default();
    // Explicit fault injection: first allocation succeeds, second exhausts the allocator.
    w.state
        .quest_progression
        .as_mut()
        .unwrap()
        .inventory
        .as_mut()
        .unwrap()
        .next_item = u64::MAX - 1;
    let before = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    let bytes = w.journal_bytes;
    let giver = w.state.npcs[&3].id;
    let first = command(
        &w,
        Command::AcceptQuest {
            quest: QUEST,
            giver,
        },
    );
    let mut second = command(
        &w,
        Command::ChooseQuestReward {
            quest: QUEST,
            giver,
            choice: 0,
        },
    );
    second.input.sequence += 1;
    assert!(w
        .step_batch(w.state.tick + 1, vec![first, second], &mut authority)
        .is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(w.records(), records);
    assert_eq!(w.journal_bytes, bytes);
    assert!(progress(&w).quests.progress(QUEST).is_none());
    assert!(inventory(&w).slots.is_empty());
    assert!(progress(&w).inventory_receipts.is_empty());
}

fn kill(w: &mut NativeWorld, authority: &mut LootAuthority) {
    let target = w.state.npcs[&2].id;
    step(
        w,
        authority,
        Command::Cast {
            ability: 60001,
            target,
        },
    );
    for _ in 0..30 {
        if w.state.npcs[&2].health == 0 {
            return;
        }
        w.step(w.state.tick + 1, None, authority).unwrap();
    }
    panic!("authored target did not die");
}
fn quest_history_bytes(w: &NativeWorld) -> usize {
    serde_json::to_vec(&progress(w).quests.snapshot().records)
        .unwrap()
        .len()
}

#[test]
fn checkpoint_anchor_stays_bounded_across_4100_legal_respawn_collect_save_cycles() {
    let (mut p, mut input, mut data) = fixture();
    let mut stack = item(REQUIRED);
    stack.stackable = 255;
    catalog_items(&mut data, vec![stack]);
    // Authored zero-cost projectile and zero-damage creature keep the long witness
    // focused on legal loot/quest lifecycles, without resetting mana or health.
    input
        .spells
        .iter_mut()
        .find(|s| s.id == 60001)
        .unwrap()
        .words[32] = 0;
    if let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission {
        spec.mana_cost = 0;
    }
    if let AbilityAdmission::Available(spec) = &mut data.abilities[0].ability.admission {
        spec.mana_cost = 0;
    }
    let combat = p.npcs[0].combat.as_mut().unwrap();
    combat.minimum_damage = spell::SpellFloat::ZERO;
    combat.maximum_damage = spell::SpellFloat::ZERO;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    p.initialize_player(&input, synthetic_fixture::float(100.0), 1)
        .unwrap();
    for row in &mut data.kill.creatures {
        row.is_no_xp = true;
    }
    for row in &mut data.loot.as_mut().unwrap().creatures {
        row.minimum_money = 0;
        row.maximum_money = 0;
        row.corpse_acceleration_decay_ms = 300;
    }
    let mut template = data.quests[0].clone();
    ordinary(&mut template);
    template.special_flags = q::DELIVER;
    template.required_item = [REQUIRED, 0, 0, 0];
    template.required_item_count = [63, 0, 0, 0];
    template.reward.xp.reward_money_max_level = 0;
    template.reward.reward_or_required_money = 0;
    data.quests.clear();
    data.relations.starters.clear();
    data.relations.finishers.clear();
    for id in 71001..=71065 {
        let mut row = template.clone();
        row.id = id;
        append_quest(&mut data, row);
    }
    let mut w = world_from(p, input, data);
    let mut authority = LootAuthority::default();
    let mut anchor = w.checkpoint().unwrap();
    let initial_size = anchor.encode().unwrap().len();
    let initial_history_size = quest_history_bytes(&w);
    for cycle in 0..4100 {
        for _ in 0..40 {
            if w.state.npcs[&2].health > 0 {
                break;
            }
            w.step(w.state.tick + 1, None, &mut authority).unwrap();
        }
        assert!(w.state.npcs[&2].health > 0, "respawn at cycle {cycle}");
        kill(&mut w, &mut authority);
        let corpse = w.state.npcs[&2].id;
        step(&mut w, &mut authority, Command::OpenLoot { corpse });
        step(
            &mut w,
            &mut authority,
            Command::LootItem { corpse, slot: 0 },
        );
        if (cycle + 1) % 63 == 0 {
            let quest = 71000 + (cycle + 1) / 63;
            accept(&mut w, &mut authority, quest);
            turn_in(&mut w, &mut authority, quest);
            assert!(progress(&w).quests.progress(quest).unwrap().rewarded);
        }
        let residual = (cycle + 1) % 63;
        assert_eq!(inventory(&w).item_count(REQUIRED).unwrap(), residual);
        assert_eq!(
            progress(&w).inventory_receipts.len(),
            usize::from(residual != 0)
        );
        assert_eq!(inventory(&w).slots.len(), usize::from(residual != 0));
        assert_eq!(progress(&w).credited_generations.len(), 1);
        assert!(w.state.npcs[&2].ai.corpse_timer_calls().len() <= 1);
        let checkpoint = w.checkpoint().unwrap();
        let bytes = checkpoint.encode().unwrap();
        let history_growth = quest_history_bytes(&w) - initial_history_size;
        assert!(
            bytes.len() <= initial_size + history_growth + 2048,
            "checkpoint grew at cycle {cycle}: {}",
            bytes.len()
        );
        if [0, 4096, 4099].contains(&cycle) {
            let mut replay =
                NativeWorld::restore_synthetic_component_test(w.profile.clone(), anchor.clone())
                    .unwrap();
            replay.replay(w.records()).unwrap();
            assert_eq!(replay.checkpoint().unwrap(), checkpoint);
        }
        let restored = NativeWorld::restore_synthetic_component_test(
            w.profile.clone(),
            Checkpoint::decode(&bytes).unwrap(),
        )
        .unwrap();
        assert_eq!(restored.state_digest(), w.state_digest());
        assert_eq!(
            w.checkpoint_committed(&anchor),
            Err(NativeError::InvalidCheckpoint)
        );
        w.checkpoint_committed(&checkpoint).unwrap();
        assert!(w.records().is_empty());
        assert_eq!(w.journal_bytes, 2);
        assert_eq!(w.checkpoint().unwrap(), checkpoint);
        anchor = checkpoint;
        w = restored;
    }
    assert_eq!(inventory(&w).item_count(REQUIRED).unwrap(), 5);
    assert_eq!(progress(&w).quests.snapshot().records.len(), 65);
    assert!(progress(&w)
        .quests
        .snapshot()
        .records
        .values()
        .all(|q| q.rewarded));
}

#[test]
fn repeated_4200_opens_keep_one_timer_witness_and_restore_without_item_or_receipt_growth() {
    let (p, i, mut data) = fixture();
    for row in &mut data.loot.as_mut().unwrap().creatures {
        row.corpse_acceleration_decay_ms = 1000;
    }
    let mut w = world_from(p, i, data);
    let mut authority = LootAuthority::default();
    kill(&mut w, &mut authority);
    let corpse = w.state.npcs[&2].id;
    let mut anchor = w.checkpoint().unwrap();
    w.checkpoint_committed(&anchor).unwrap();
    let initial_size = anchor.encode().unwrap().len();
    let initial_inventory = inventory(&w).clone();
    for cycle in 0..4200 {
        step(&mut w, &mut authority, Command::OpenLoot { corpse });
        assert_eq!(w.state.npcs[&2].id, corpse);
        assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Corpse);
        assert_eq!(w.state.npcs[&2].ai.corpse_timer_calls().len(), 1);
        assert_eq!(inventory(&w), &initial_inventory);
        assert!(progress(&w).inventory_receipts.is_empty());
        let checkpoint = w.checkpoint().unwrap();
        assert!(checkpoint.encode().unwrap().len() <= initial_size + 2048);
        if [0, 4096, 4199].contains(&cycle) {
            let mut replay =
                NativeWorld::restore_synthetic_component_test(w.profile.clone(), anchor.clone())
                    .unwrap();
            replay.replay(w.records()).unwrap();
            assert_eq!(replay.checkpoint().unwrap(), checkpoint);
        }
        w.checkpoint_committed(&checkpoint).unwrap();
        assert!(w.records().is_empty());
        w = restore(&w);
        anchor = checkpoint;
    }
}
