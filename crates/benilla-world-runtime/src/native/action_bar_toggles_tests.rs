// SPDX-License-Identifier: GPL-3.0-or-later
//! Source raw byte descriptor semantics; authored worlds, no content admission.
use super::*;
fn input(w: &NativeWorld, toggles: u8) -> OwnedInput {
    synthetic_quest_fixture::command(w, Command::SetActionBarToggles { toggles })
}
fn apply(w: &mut NativeWorld, value: u8) {
    let command = input(w, value);
    w.step(
        w.state.tick + 1,
        Some(command),
        &mut synthetic_quest_fixture::QuestAuthority::default(),
    )
    .unwrap();
}
fn writes(w: &NativeWorld) -> serde_json::Value {
    serde_json::to_value(w.checkpoint().unwrap().state).unwrap()["action_bar_toggles"]["writes"]
        .clone()
}
fn reopen(w: &NativeWorld) -> NativeWorld {
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), w.checkpoint().unwrap());
    restored
}
#[test]
fn all_256_action_bar_bytes_preserve_raw_values_reopen_and_replay() {
    let mut w = synthetic_quest_fixture::world();
    assert_eq!(w.state.action_bar_toggles(), 0);
    let initial = w.checkpoint().unwrap();
    for value in 0..=255u8 {
        apply(&mut w, value);
        assert_eq!(w.state.action_bar_toggles(), value);
        assert_eq!(writes(&w).as_array().unwrap().len(), usize::from(value));
        reopen(&w);
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn same_value_writes_are_noops_and_batch_writes_keep_authenticated_sequence_order() {
    let mut w = synthetic_quest_fixture::world();
    apply(&mut w, 0);
    assert_eq!(writes(&w), serde_json::json!([]));
    let first = input(&w, 1);
    let mut duplicate = input(&w, 1);
    duplicate.input.sequence += 1;
    let mut last = input(&w, 2);
    last.input.sequence += 2;
    let sequence = first.input.sequence;
    w.step_batch(
        w.state.tick + 1,
        vec![first, duplicate, last],
        &mut synthetic_quest_fixture::QuestAuthority::default(),
    )
    .unwrap();
    assert_eq!(
        writes(&w),
        serde_json::json!([{ "sequence":sequence,"value":1 }, { "sequence":sequence+2,"value":2 }])
    );
    let before = writes(&w);
    apply(&mut w, 2);
    assert_eq!(writes(&w), before);
    reopen(&w);
}
struct Fail;
impl Authority for Fail {
    fn resolve(&mut self, _: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        Err(NativeError::AuthorityUnavailable("toggles rollback test"))
    }
}
#[test]
fn later_authority_failure_rolls_back_toggles_value_and_receipt() {
    let mut w = synthetic_quest_fixture::world();
    let before = w.checkpoint().unwrap();
    let first = input(&w, 0xf5);
    let mut second = synthetic_quest_fixture::command(
        &w,
        Command::Move {
            destination: synthetic_fixture::point(1.0),
        },
    );
    second.input.sequence += 1;
    assert!(matches!(
        w.step_batch(w.state.tick + 1, vec![first, second], &mut Fail),
        Err(NativeError::AuthorityUnavailable("toggles rollback test"))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
}
#[test]
fn toggles_value_and_malformed_write_witnesses_reject_rechecksummed_checkpoints() {
    let mut w = synthetic_quest_fixture::world();
    apply(&mut w, 1);
    apply(&mut w, 255);
    for mutation in [
        "value",
        "missing",
        "future_sequence",
        "duplicate_sequence",
        "noop",
    ] {
        let mut forged = w.checkpoint().unwrap();
        let mut encoded = serde_json::to_value(&forged.state).unwrap();
        let toggles = &mut encoded["action_bar_toggles"];
        match mutation {
            "value" => toggles["value"] = serde_json::json!(0),
            "missing" => {
                toggles["writes"].as_array_mut().unwrap().pop();
            }
            "future_sequence" => {
                toggles["writes"][1]["sequence"] = serde_json::json!(w.state.next_sequence)
            }
            "duplicate_sequence" => {
                toggles["writes"][1]["sequence"] = toggles["writes"][0]["sequence"].clone()
            }
            "noop" => toggles["writes"][0]["value"] = serde_json::json!(0),
            _ => unreachable!(),
        }
        forged.state = serde_json::from_value(encoded).unwrap();
        forged.digest = crate::digest(b"benilla-native-state-v75\0", &forged.state);
        forged.checksum = crate::digest(
            b"benilla-native-checkpoint-v75\0",
            &(&forged.state, forged.digest, forged.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), forged).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn changed_write_capacity_is_bounded_but_same_value_remains_a_noop() {
    let mut state = action_bar_toggles::ActionBarTogglesState::default();
    for sequence in 0..2048 {
        state.write(((sequence + 1) % 2) as u8, sequence).unwrap();
    }
    state
        .validate(&synthetic_quest_fixture::profile(), 2048)
        .unwrap();
    let before = state.clone();
    state.write(0, 2048).unwrap();
    assert_eq!(state, before);
    assert!(matches!(state.write(1, 2048), Err(NativeError::Limit)));
    assert_eq!(state, before);
    let mut encoded = serde_json::to_value(state).unwrap();
    encoded["writes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"sequence":2048,"value":1}));
    assert!(serde_json::from_value::<action_bar_toggles::ActionBarTogglesState>(encoded).is_err());
}

#[test]
fn missing_packed_field_owners_reject_command_and_transplanted_write_ledger() {
    let mut bare = synthetic_fixture::world();
    let before = bare.checkpoint().unwrap();
    let command = input(&bare, 0xf5);
    assert!(bare
        .step(
            bare.state.tick + 1,
            Some(command),
            &mut synthetic_fixture::SyntheticAuthority::default()
        )
        .is_err());
    assert_eq!(bare.checkpoint().unwrap(), before);
    let mut qualified = synthetic_quest_fixture::world();
    apply(&mut qualified, 0xf5);
    let ledger = serde_json::to_value(qualified.checkpoint().unwrap().state).unwrap()
        ["action_bar_toggles"]
        .clone();
    // Give the bare world an actual accepted sequence boundary, not a future-write error.
    let watch = synthetic_quest_fixture::command(&bare, Command::SetWatchedFaction { list_id: 0 });
    bare.step(
        bare.state.tick + 1,
        Some(watch),
        &mut synthetic_fixture::SyntheticAuthority::default(),
    )
    .unwrap();
    let mut forged = bare.checkpoint().unwrap();
    let mut encoded = serde_json::to_value(&forged.state).unwrap();
    encoded["action_bar_toggles"] = ledger;
    forged.state = serde_json::from_value(encoded).unwrap();
    forged.digest = crate::digest(b"benilla-native-state-v75\0", &forged.state);
    forged.checksum = crate::digest(
        b"benilla-native-checkpoint-v75\0",
        &(&forged.state, forged.digest, forged.history_tip),
    );
    assert!(NativeWorld::restore_synthetic_component_test(bare.profile.clone(), forged).is_err());
}
