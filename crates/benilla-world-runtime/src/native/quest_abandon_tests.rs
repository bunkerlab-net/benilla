// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored first-party command/replay probes; no native content admission.
use super::super::synthetic_reputation_fixture as repfx;
use super::*;

fn apply_abandon_command(w: &mut NativeWorld, c: Command) {
    let input = command(w, c);
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut QuestAuthority::default(),
    )
    .unwrap();
}
fn accept_abandon_fixture(w: &mut NativeWorld, quest: u32) {
    let giver = w.state.npcs[&3].id;
    apply_abandon_command(w, Command::AcceptQuest { quest, giver });
}
fn reopen_abandon(w: &NativeWorld) -> NativeWorld {
    let reopened = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(reopened.state_digest(), w.state_digest());
    reopened
}

#[test]
fn reputation_objective_abandon_refuses_and_rolls_back_prior_batch_abandon() {
    let mut w = repfx::world();
    accept_abandon_fixture(&mut w, repfx::GRANT);
    accept_abandon_fixture(&mut w, repfx::OBJECTIVE);
    let before = w.checkpoint().unwrap();
    let first = command(&w, Command::AbandonQuest { slot: 0 });
    let mut second = command(&w, Command::AbandonQuest { slot: 1 });
    second.input.sequence += 1;
    let result = w.step_batch(
        w.state.tick + 1,
        vec![first, second],
        &mut QuestAuthority::default(),
    );
    assert!(matches!(
        result,
        Err(NativeError::UnsupportedMechanic(
            "reputation-objective quest abandonment"
        ))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(
        w.state
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .view()
            .log[..2],
        [Some(repfx::GRANT), Some(repfx::OBJECTIVE)]
    );
    reopen_abandon(&w);
}

#[test]
fn ordinary_enabled_reputation_abandon_reaccept_reward_and_reopen_replay() {
    let mut w = repfx::world();
    let genesis = w.checkpoint().unwrap();
    accept_abandon_fixture(&mut w, repfx::GRANT);
    let rep_before = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation
        .clone();
    assert!(rep_before.is_some());
    apply_abandon_command(&mut w, Command::AbandonQuest { slot: 0 });
    let p = w.state.quest_progression.as_ref().unwrap();
    assert_eq!(
        p.quests.progress(repfx::GRANT).unwrap().status,
        q::Status::None
    );
    assert!(!p.quests.progress(repfx::GRANT).unwrap().rewarded);
    assert_eq!(p.reputation, rep_before);
    assert_eq!(p.quests.view().log[0], None);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    w = reopen_abandon(&w);
    let none = w.checkpoint().unwrap();
    accept_abandon_fixture(&mut w, repfx::GRANT);
    assert_eq!(
        w.state
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .progress(repfx::GRANT)
            .unwrap()
            .status,
        q::Status::Complete
    );
    let giver = w.state.npcs[&3].id;
    apply_abandon_command(
        &mut w,
        Command::TurnInQuest {
            quest: repfx::GRANT,
            giver,
        },
    );
    let rewarded = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .quests
        .progress(repfx::GRANT)
        .unwrap()
        .clone();
    assert!(rewarded.rewarded);
    let rep_rewarded = w
        .state
        .quest_progression
        .as_ref()
        .unwrap()
        .reputation
        .clone();
    apply_abandon_command(&mut w, Command::AbandonQuest { slot: 0 });
    assert_eq!(
        w.state
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .progress(repfx::GRANT),
        Some(&rewarded)
    );
    assert_eq!(
        w.state.quest_progression.as_ref().unwrap().reputation,
        rep_rewarded
    );
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), none).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    reopen_abandon(&w);
}

fn resign_abandon_checkpoint(value: &mut serde_json::Value) {
    let state: super::super::persistence::StateCheckpoint =
        serde_json::from_value(value["state"].clone()).unwrap();
    let digest = crate::digest(b"benilla-native-state-v79\0", &state);
    let tip: crate::Digest = serde_json::from_value(value["history_tip"].clone()).unwrap();
    value["digest"] = serde_json::to_value(digest).unwrap();
    value["checksum"] = serde_json::to_value(crate::digest(
        b"benilla-native-checkpoint-v79\0",
        &(&state, digest, tip),
    ))
    .unwrap();
}

#[test]
fn rechecksummed_none_history_cannot_be_logged_rewarded_or_made_unlogged_active() {
    let mut w = repfx::world();
    accept_abandon_fixture(&mut w, repfx::GRANT);
    let accepted = w.checkpoint().unwrap();
    apply_abandon_command(&mut w, Command::AbandonQuest { slot: 0 });
    let record = w.last_tick().unwrap().clone();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), accepted).unwrap();
    replay.replay(&[record]).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let before = w.checkpoint().unwrap();
    for fault in 0..3 {
        let mut forged = serde_json::to_value(&before).unwrap();
        let quests = &mut forged["state"]["quest_progression"]["quests"];
        assert_eq!(
            quests["records"]["70001"]["status"],
            serde_json::json!("None")
        );
        match fault {
            0 => quests["log"][0] = serde_json::json!(repfx::GRANT),
            1 => quests["records"]["70001"]["rewarded"] = serde_json::json!(true),
            _ => quests["records"]["70001"]["status"] = serde_json::json!("Incomplete"),
        }
        resign_abandon_checkpoint(&mut forged);
        assert!(NativeWorld::restore_synthetic_component_test(
            w.profile.clone(),
            serde_json::from_value(forged).unwrap()
        )
        .is_err());
        assert_eq!(w.checkpoint().unwrap(), before);
    }
    reopen_abandon(&w);
}
