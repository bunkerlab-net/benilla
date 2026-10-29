// SPDX-License-Identifier: GPL-3.0-or-later
//! Source raw signed descriptor semantics; authored worlds, no content admission.
use super::*;
fn input(w: &NativeWorld, list_id: i32) -> OwnedInput {
    synthetic_quest_fixture::command(w, Command::SetWatchedFaction { list_id })
}
fn apply(w: &mut NativeWorld, value: i32) {
    let command = input(w, value);
    w.step(
        w.state.tick + 1,
        Some(command),
        &mut synthetic_fixture::SyntheticAuthority::default(),
    )
    .unwrap();
}
fn writes(w: &NativeWorld) -> serde_json::Value {
    serde_json::to_value(w.checkpoint().unwrap().state).unwrap()["watched_faction"]["writes"]
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
fn arbitrary_signed_watched_slots_without_reputation_catalog_replay_and_reopen() {
    let mut w = synthetic_fixture::world();
    assert!(w.state.quest_progression.is_none());
    assert_eq!(w.state.watched_faction(), -1);
    assert_eq!(writes(&w), serde_json::json!([]));
    let initial = w.checkpoint().unwrap();
    for (index, value) in [0, -2, 64, 123456, i32::MIN, i32::MAX, -1]
        .into_iter()
        .enumerate()
    {
        apply(&mut w, value);
        assert_eq!(w.state.watched_faction(), value);
        let retained = writes(&w);
        assert_eq!(retained.as_array().unwrap().len(), index + 1);
        assert_eq!(retained[index]["value"], value);
        assert_eq!(retained[index]["sequence"], w.state.next_sequence - 1);
        reopen(&w);
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
#[test]
fn same_value_writes_are_noops_and_batch_writes_keep_authenticated_sequence_order() {
    let mut w = synthetic_fixture::world();
    apply(&mut w, -1);
    assert_eq!(writes(&w), serde_json::json!([]));
    let first = input(&w, 0);
    let mut duplicate = input(&w, 0);
    duplicate.input.sequence += 1;
    let mut last = input(&w, -2);
    last.input.sequence += 2;
    let sequence = first.input.sequence;
    w.step_batch(
        w.state.tick + 1,
        vec![first, duplicate, last],
        &mut synthetic_fixture::SyntheticAuthority::default(),
    )
    .unwrap();
    assert_eq!(
        writes(&w),
        serde_json::json!([{ "sequence":sequence,"value":0 }, { "sequence":sequence+2,"value":-2 }])
    );
    let before = writes(&w);
    apply(&mut w, -2);
    assert_eq!(writes(&w), before);
    reopen(&w);
}
struct Fail;
impl Authority for Fail {
    fn resolve(&mut self, _: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        Err(NativeError::AuthorityUnavailable("watched rollback test"))
    }
}
#[test]
fn later_authority_failure_rolls_back_watched_value_and_receipt() {
    let mut w = synthetic_fixture::world();
    let before = w.checkpoint().unwrap();
    let first = input(&w, i32::MIN);
    let mut second = synthetic_quest_fixture::command(
        &w,
        Command::Move {
            destination: synthetic_fixture::point(1.0),
        },
    );
    second.input.sequence += 1;
    assert!(matches!(
        w.step_batch(w.state.tick + 1, vec![first, second], &mut Fail),
        Err(NativeError::AuthorityUnavailable("watched rollback test"))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
}
#[test]
fn watched_value_and_malformed_write_witnesses_reject_rechecksummed_checkpoints() {
    let mut w = synthetic_fixture::world();
    apply(&mut w, 0);
    apply(&mut w, i32::MAX);
    for mutation in [
        "value",
        "missing",
        "future_sequence",
        "duplicate_sequence",
        "noop",
    ] {
        let mut forged = w.checkpoint().unwrap();
        let mut encoded = serde_json::to_value(&forged.state).unwrap();
        let watched = &mut encoded["watched_faction"];
        match mutation {
            "value" => watched["value"] = serde_json::json!(-1),
            "missing" => {
                watched["writes"].as_array_mut().unwrap().pop();
            }
            "future_sequence" => {
                watched["writes"][1]["sequence"] = serde_json::json!(w.state.next_sequence)
            }
            "duplicate_sequence" => {
                watched["writes"][1]["sequence"] = watched["writes"][0]["sequence"].clone()
            }
            "noop" => watched["writes"][0]["value"] = serde_json::json!(-1),
            _ => unreachable!(),
        }
        forged.state = serde_json::from_value(encoded).unwrap();
        forged.digest = crate::digest(b"benilla-native-state-v79\0", &forged.state);
        forged.checksum = crate::digest(
            b"benilla-native-checkpoint-v79\0",
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
    let mut state = watched_faction::WatchedFactionState::default();
    for sequence in 0..2048 {
        state.write((sequence % 2) as i32, sequence).unwrap();
    }
    state.validate(2048).unwrap();
    let before = state.clone();
    state.write(1, 2048).unwrap();
    assert_eq!(state, before);
    assert!(matches!(state.write(0, 2048), Err(NativeError::Limit)));
    assert_eq!(state, before);
    let mut encoded = serde_json::to_value(state).unwrap();
    encoded["writes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"sequence":2048,"value":0}));
    assert!(serde_json::from_value::<watched_faction::WatchedFactionState>(encoded).is_err());
}
