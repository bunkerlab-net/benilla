// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored end-to-end source reputation transactions; no game content admission.
use super::synthetic_reputation_fixture as fixture;
use super::*;
use benilla_sim::{quest_lifecycle as q, reputation as rep};
fn progress(w: &NativeWorld) -> &QuestProgressionState {
    w.state().quest_progression.as_ref().unwrap()
}
fn step(w: &mut NativeWorld, a: &mut synthetic_quest_fixture::QuestAuthority, command: Command) {
    let input = synthetic_quest_fixture::command(w, command);
    w.step(w.state().tick + 1, Some(input), a).unwrap();
}
fn accept(w: &mut NativeWorld, a: &mut synthetic_quest_fixture::QuestAuthority, quest: u32) {
    step(
        w,
        a,
        Command::AcceptQuest {
            quest,
            giver: w.state().npcs[&3].id,
        },
    );
}
fn reward(w: &mut NativeWorld, a: &mut synthetic_quest_fixture::QuestAuthority, quest: u32) {
    step(
        w,
        a,
        Command::TurnInQuest {
            quest,
            giver: w.state().npcs[&3].id,
        },
    );
}
fn restore(w: &NativeWorld) -> NativeWorld {
    let result = NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(w.checkpoint().unwrap(), result.checkpoint().unwrap());
    result
}
#[test]
fn source_spillover_callbacks_toggle_objective_before_log_clear_xp_and_level() {
    source_spillover_callbacks(None);
}
#[test]
fn retained_general_modifier_callbacks_toggle_objective_before_log_clear_xp_and_level() {
    source_spillover_callbacks(Some(10));
}
fn source_spillover_callbacks(modifier: Option<i32>) {
    let mut w = modifier.map_or_else(fixture::world, fixture::world_with_modifier);
    let gained = if modifier.is_some() { 240 } else { 200 };
    let lost = if modifier.is_some() { 450 } else { 500 };
    let initial = w.checkpoint().unwrap();
    let mut a = synthetic_quest_fixture::QuestAuthority::default();
    accept(&mut w, &mut a, fixture::OBJECTIVE);
    assert_eq!(
        progress(&w)
            .quests
            .progress(fixture::OBJECTIVE)
            .unwrap()
            .status,
        q::Status::Incomplete
    );
    let events = &w.last_tick().unwrap().events;
    let visible = events.iter().position(|e| matches!(e, Event::QuestProgression(QuestProgressionEvent::Reputation(event)) if matches!(**event, rep::Event::Visible{list_id:7}))).unwrap();
    let log = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::Quest {
                    event: q::Event::LogSet { .. },
                    ..
                })
            )
        })
        .unwrap();
    assert!(visible < log);
    assert!(w.reputation_initial_ack_needed());
    restore(&w);
    accept(&mut w, &mut a, fixture::GATED);
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .contains(&Event::Rejected(Rejection::Quest(q::Gate::Reputation))));
    accept(&mut w, &mut a, fixture::GRANT);
    reward(&mut w, &mut a, fixture::GRANT);
    let p = progress(&w);
    let g = w.profile().quest_progression.as_ref().unwrap();
    assert_eq!(p.reputation(g, fixture::PRIMARY).unwrap(), gained - 100);
    assert_eq!(p.reputation(g, fixture::SPILLOVER).unwrap(), gained);
    let reputation = p.reputation.as_ref().unwrap().view();
    assert_eq!(
        reputation
            .rank(g.reputation_catalog().unwrap(), fixture::PRIMARY)
            .unwrap(),
        rep::Rank::Neutral
    );
    assert!(!reputation
        .at_war(g.reputation_catalog().unwrap(), fixture::PRIMARY)
        .unwrap());
    assert_eq!(
        p.quests.progress(fixture::OBJECTIVE).unwrap().status,
        q::Status::Complete
    );
    assert_eq!(w.player_level(), 2);
    let events = &w.last_tick().unwrap().events;
    let changes: Vec<_> = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            Event::QuestProgression(QuestProgressionEvent::Quest {
                event: q::Event::LogComplete { slot: 0, complete },
                level,
                ..
            }) => {
                assert_eq!(*level, 1);
                Some((i, *complete))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        changes
            .iter()
            .map(|(_, complete)| *complete)
            .collect::<Vec<_>>(),
        [true, false, true]
    );
    let clear = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::Quest {
                    event: q::Event::LogSet {
                        slot: 1,
                        quest: None
                    },
                    ..
                })
            )
        })
        .unwrap();
    let xp = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::QuestProgression(QuestProgressionEvent::Progression(_))
            )
        })
        .unwrap();
    assert!(changes.last().unwrap().0 < clear && clear < xp);
    assert_eq!(events.iter().filter(|e| matches!(e,Event::QuestProgression(QuestProgressionEvent::Reputation(event)) if matches!(**event,rep::Event::StateSent{..}))).count(),3);
    restore(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(w.checkpoint().unwrap(), replay.checkpoint().unwrap());
    accept(&mut w, &mut a, fixture::GATED);
    assert!(progress(&w).quests.progress(fixture::GATED).is_some());
    accept(&mut w, &mut a, fixture::LOSS);
    reward(&mut w, &mut a, fixture::LOSS);
    assert_eq!(
        progress(&w)
            .quests
            .progress(fixture::OBJECTIVE)
            .unwrap()
            .status,
        q::Status::Incomplete
    );
    assert_eq!(
        progress(&w)
            .reputation(
                w.profile().quest_progression.as_ref().unwrap(),
                fixture::SPILLOVER
            )
            .unwrap(),
        gained - lost
    );
    restore(&w);
}
#[test]
fn saved_reputation_and_callback_witness_cannot_be_reblessed_by_outer_hashes() {
    check_saved_reputation_and_callback(false);
    check_saved_reputation_and_callback(true);
}
fn check_saved_reputation_and_callback(modifier: bool) {
    let mut w = if modifier {
        fixture::world_with_modifier(10)
    } else {
        fixture::world()
    };
    let mut a = synthetic_quest_fixture::QuestAuthority::default();
    accept(&mut w, &mut a, fixture::OBJECTIVE);
    accept(&mut w, &mut a, fixture::GRANT);
    reward(&mut w, &mut a, fixture::GRANT);
    for field in [
        "standing",
        "flags",
        "need_send",
        "need_save",
        "history",
        "callback",
        "final_status",
        "pre_xp_level",
    ] {
        let mut forged = w.checkpoint().unwrap();
        let mut state = serde_json::to_value(&forged.state).unwrap();
        let rep = &mut state["quest_progression"]["reputation"];
        match field {
            "standing" => rep["state"]["factions"]["3"]["standing"] = serde_json::json!(9000),
            "flags" => rep["state"]["factions"]["3"]["flags"] = serde_json::json!(0),
            "need_send" => rep["state"]["factions"]["3"]["need_send"] = serde_json::json!(true),
            "need_save" => rep["state"]["factions"]["3"]["need_save"] = serde_json::json!(false),
            "history" => {
                rep["history"].as_array_mut().unwrap().pop();
            }
            "final_status" => {
                state["quest_progression"]["quests"]["records"][fixture::OBJECTIVE.to_string()]
                    ["status"] = serde_json::json!("Incomplete");
            }
            "pre_xp_level" => rep["history"][1]["call"]["Reward"]["level"] = serde_json::json!(2),
            "callback" => {
                rep["history"][1]["call"]["Reward"]["after"]["records"]
                    [fixture::OBJECTIVE.to_string()]["status"] = serde_json::json!("Incomplete")
            }
            _ => unreachable!(),
        }
        forged.state = serde_json::from_value(state).unwrap();
        forged.digest = crate::digest(b"benilla-native-state-v50\0", &forged.state);
        forged.checksum = crate::digest(
            b"benilla-native-checkpoint-v50\0",
            &(&forged.state, forged.digest, forged.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err(),
            "{field}"
        );
    }
}
#[test]
fn reward_level_witness_rejects_an_unfinished_level_hook_prefix() {
    let mut w = fixture::world();
    let mut a = synthetic_quest_fixture::QuestAuthority::default();
    accept(&mut w, &mut a, fixture::GRANT);
    reward(&mut w, &mut a, fixture::GRANT);
    assert_eq!(w.player_level(), 2);
    accept(&mut w, &mut a, fixture::LOSS);
    reward(&mut w, &mut a, fixture::LOSS);
    restore(&w);
    let mut forged = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&forged.state).unwrap();
    let history = state["quest_progression"]["reputation"]["history"]
        .as_array_mut()
        .unwrap();
    let receipt = history
        .iter_mut()
        .find(|r| r["call"]["Reward"]["quest"] == fixture::LOSS)
        .unwrap();
    assert_eq!(receipt["call"]["Reward"]["level"], 2);
    assert!(
        receipt["call"]["Reward"]["progression_prefix"]
            .as_u64()
            .unwrap()
            > 1
    );
    receipt["call"]["Reward"]["progression_prefix"] = serde_json::json!(1);
    forged.state = serde_json::from_value(state).unwrap();
    forged.digest = crate::digest(b"benilla-native-state-v50\0", &forged.state);
    forged.checksum = crate::digest(
        b"benilla-native-checkpoint-v50\0",
        &(&forged.state, forged.digest, forged.history_tip),
    );
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err());
}
#[test]
fn closure_is_explicit_and_bound_and_initial_ack_has_no_duplicate_packets() {
    let (mut p, input, mut rows) = fixture::setup();
    rows.reputation = None;
    assert!(p.initialize_quest_progression(&input, rows).is_err());
    let mut w = fixture::world();
    let mut a = synthetic_quest_fixture::QuestAuthority::default();
    accept(&mut w, &mut a, fixture::OBJECTIVE);
    let before = w.state().now_ms;
    step(&mut w, &mut a, Command::AcknowledgeInitialReputations);
    assert_eq!(w.state().now_ms, before + u64::from(w.profile().tick_ms));
    assert!(!w.reputation_initial_ack_needed());
    assert!(!w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::QuestProgression(QuestProgressionEvent::Reputation(_))
    )));
    let history = serde_json::to_value(w.checkpoint().unwrap()).unwrap()["state"]
        ["quest_progression"]["reputation"]["history"]
        .clone();
    for _ in 0..20 {
        step(&mut w, &mut a, Command::AcknowledgeInitialReputations);
    }
    assert_eq!(
        serde_json::to_value(w.checkpoint().unwrap()).unwrap()["state"]["quest_progression"]
            ["reputation"]["history"],
        history
    );
    restore(&w);
    let mut changed = w.profile().clone();
    let (_, input, mut rows) = fixture::setup();
    let mut encoded = serde_json::to_value(rows.reputation.unwrap()).unwrap();
    encoded["config"]["gain_rate"] = serde_json::json!(0x40000000u32);
    rows.reputation = Some(serde_json::from_value(encoded).unwrap());
    changed.initialize_quest_progression(&input, rows).unwrap();
    assert_ne!(changed.digest(), w.profile_digest());
    assert!(
        NativeWorld::restore_synthetic_component_test(changed, w.checkpoint().unwrap()).is_err()
    );
}

#[test]
fn later_reputation_slot_failure_rolls_back_objectives_standing_and_outer_tick() {
    check_reputation_slot_rollback(false);
    check_reputation_slot_rollback(true);
}
fn check_reputation_slot_rollback(modifier: bool) {
    let (mut profile, input, mut rows) = if modifier {
        fixture::setup_with_modifier(10)
    } else {
        fixture::setup()
    };
    rows.quests[0].excluded.reward_rep_value[1] = i32::MAX;
    profile.initialize_quest_progression(&input, rows).unwrap();
    let mut w =
        NativeWorld::synthetic_component_test(profile, [1; 16], synthetic_fixture::player(), 42)
            .unwrap();
    let mut a = synthetic_quest_fixture::QuestAuthority::default();
    accept(&mut w, &mut a, fixture::OBJECTIVE);
    accept(&mut w, &mut a, fixture::GRANT);
    let before = w.checkpoint().unwrap();
    let command = synthetic_quest_fixture::command(
        &w,
        Command::TurnInQuest {
            quest: fixture::GRANT,
            giver: w.state().npcs[&3].id,
        },
    );
    assert!(matches!(
        w.step(w.state().tick + 1, Some(command), &mut a),
        Err(NativeError::Quest(q::Error::Host(
            "source reputation transaction"
        )))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(
        progress(&w)
            .quests
            .progress(fixture::OBJECTIVE)
            .unwrap()
            .status,
        q::Status::Incomplete
    );
    assert_eq!(
        progress(&w)
            .reputation(
                w.profile().quest_progression.as_ref().unwrap(),
                fixture::SPILLOVER
            )
            .unwrap(),
        0
    );
    restore(&w);
}

#[test]
fn source_item_removal_overcount_keeps_incomplete_reputation_objective_reopenable() {
    let (mut profile, input, mut data) = synthetic_loot_fixture::setup();
    let (_, _, reputation_rows) = fixture::setup();
    data.reputation = reputation_rows.reputation;
    let mut donor = data.quests[0].clone();
    donor.special_flags = q::DELIVER;
    donor.required_creature = [0; 4];
    donor.required_creature_count = [0; 4];
    donor.required_item = [70001, 0, 0, 0];
    donor.required_item_count = [3, 0, 0, 0];
    donor.reward.xp.reward_money_max_level = 0;
    let mut objective = donor.clone();
    objective.id = fixture::OBJECTIVE;
    objective.required_item_count[0] = 2;
    objective.excluded.rep_objective_faction = fixture::SPILLOVER;
    objective.excluded.rep_objective_value = 0;
    let mut fill = donor.clone();
    fill.id = 70006;
    fill.special_flags = 0;
    fill.required_item = [0; 4];
    fill.required_item_count = [0; 4];
    fill.excluded.reward_item[0] = 70001;
    fill.excluded.reward_item_count[0] = 4;
    let mut second_donor = donor.clone();
    second_donor.id = 70007;
    second_donor.required_item_count[0] = 1;
    let mut refill = fill.clone();
    refill.id = 70008;
    refill.excluded.reward_item_count[0] = 2;
    let mut unrelated = fill.clone();
    unrelated.id = 70009;
    unrelated.excluded.reward_item = [0; 4];
    unrelated.excluded.reward_item_count = [0; 4];
    data.quests = vec![donor, objective, fill, second_donor, refill, unrelated];
    data.relations.starters.clear();
    data.relations.finishers.clear();
    for row in &data.quests {
        let r = q::CreatureQuestRelation {
            creature: 60003,
            quest: row.id,
        };
        data.relations.starters.push(r.clone());
        data.relations.finishers.push(r);
    }
    profile.initialize_quest_progression(&input, data).unwrap();
    let mut w =
        NativeWorld::synthetic_component_test(profile, [1; 16], synthetic_fixture::player(), 42)
            .unwrap();
    let mut a = synthetic_loot_fixture::LootAuthority::default();
    let run =
        |w: &mut NativeWorld, a: &mut synthetic_loot_fixture::LootAuthority, quest, reward| {
            let giver = w.state().npcs[&3].id;
            let cmd = if reward {
                Command::TurnInQuest { quest, giver }
            } else {
                Command::AcceptQuest { quest, giver }
            };
            let input = synthetic_quest_fixture::command(w, cmd);
            w.step(w.state().tick + 1, Some(input), a).unwrap();
        };
    run(&mut w, &mut a, 70006, false);
    run(&mut w, &mut a, 70006, true);
    run(&mut w, &mut a, fixture::OBJECTIVE, false);
    run(&mut w, &mut a, fixture::GRANT, false);
    run(&mut w, &mut a, fixture::GRANT, true);
    let p = progress(&w);
    assert_eq!(p.inventory.as_ref().unwrap().item_count(70001).unwrap(), 1);
    assert_eq!(p.quests.progress(fixture::OBJECTIVE).unwrap().items[0], 3);
    assert_eq!(
        p.quests.progress(fixture::OBJECTIVE).unwrap().status,
        q::Status::Incomplete
    );
    assert_eq!(
        p.reputation(
            w.profile().quest_progression.as_ref().unwrap(),
            fixture::SPILLOVER
        )
        .unwrap(),
        0
    );
    restore(&w);
    let mut forged = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&forged.state).unwrap();
    state["quest_progression"]["reputation"]["item_incomplete"] = serde_json::json!({});
    forged.state = serde_json::from_value(state).unwrap();
    forged.digest = crate::digest(b"benilla-native-state-v50\0", &forged.state);
    forged.checksum = crate::digest(
        b"benilla-native-checkpoint-v50\0",
        &(&forged.state, forged.digest, forged.history_tip),
    );
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err());
    let witness = |w: &NativeWorld| {
        serde_json::to_value(w.checkpoint().unwrap()).unwrap()["state"]["quest_progression"]
            ["reputation"]["item_incomplete"]
            .clone()
    };
    let first = witness(&w);
    run(&mut w, &mut a, 70009, false);
    assert_eq!(witness(&w), first);
    restore(&w);
    run(&mut w, &mut a, 70007, false);
    run(&mut w, &mut a, 70007, true);
    let refreshed = witness(&w);
    assert_ne!(refreshed, first);
    assert_eq!(
        refreshed[fixture::OBJECTIVE.to_string()]["after_counter"],
        0
    );
    restore(&w);
    run(&mut w, &mut a, 70008, false);
    run(&mut w, &mut a, 70008, true);
    assert_eq!(
        progress(&w)
            .quests
            .progress(fixture::OBJECTIVE)
            .unwrap()
            .status,
        q::Status::Complete
    );
    assert_eq!(witness(&w), serde_json::json!({}));
    restore(&w);
}
