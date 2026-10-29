// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use synthetic_player_appearance_fixture as f;
fn world() -> NativeWorld {
    let player = synthetic_fixture::player();
    NativeWorld::synthetic_component_test(f::profile(f::request(player)), [8; 16], player, 42)
        .unwrap()
}
#[test]
fn exact_selected_cosmetics_are_immutable_across_real_ticks_and_reopen() {
    let mut w = world();
    let before = w.state().player_appearance().unwrap().clone();
    assert_eq!(before.player_bytes(), 0x00010001);
    assert_eq!(before.choices().facial_hair, 1);
    let initial = w.checkpoint().unwrap();
    w.step(
        1,
        None,
        &mut synthetic_fixture::SyntheticAuthority::default(),
    )
    .unwrap();
    assert_eq!(w.state().player_appearance(), Some(&before));
    let checkpoint = w.checkpoint().unwrap();
    let reopened =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint.clone())
            .unwrap();
    assert_eq!(reopened.checkpoint().unwrap(), checkpoint);
    let mut replayed =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replayed.replay(w.records()).unwrap();
    assert_eq!(replayed.state_digest(), w.state_digest());
}
#[test]
fn selected_identity_unsupported_choices_and_reinitialization_fail_atomically() {
    let player = synthetic_fixture::player();
    let mut p = f::profile(f::request(player));
    let before = p.clone();
    for field in [0, 1, 2, 3] {
        let mut request = f::request(player);
        match field {
            0 => request.gender = 1,
            1 => request.class = 1,
            2 => request.choices.skin = 3,
            _ => request.choices.skin = 0,
        }
        assert!(p
            .initialize_player_appearance(request, &f::catalog())
            .is_err());
        assert_eq!(p, before);
    }
    let mut other = player;
    other.character[0] ^= 1;
    assert!(NativeWorld::synthetic_component_test(p, [8; 16], other, 42).is_err());
}
#[test]
fn required_appearance_owner_rejects_rehashed_missing_transplanted_and_edited_state() {
    let w = world();
    for key in [
        "choices", "world", "player", "profile", "source", "null", "missing",
    ] {
        let mut cp = w.checkpoint().unwrap();
        let mut encoded = serde_json::to_value(&cp.state).unwrap();
        let a = &mut encoded["source_player_appearance"];
        match key {
            "choices" => a["choices"]["skin"] = serde_json::json!(0),
            "world" => a["world"][0] = serde_json::json!(99),
            "player" => a["player"]["character"][0] = serde_json::json!(99),
            "profile" | "source" => a[key][0] = serde_json::json!(a[key][0].as_u64().unwrap() ^ 1),
            "null" => *a = serde_json::Value::Null,
            "missing" => {
                encoded
                    .as_object_mut()
                    .unwrap()
                    .remove("source_player_appearance");
            }
            _ => unreachable!(),
        }
        if key == "missing" {
            assert!(serde_json::from_value::<persistence::StateCheckpoint>(encoded).is_err());
            continue;
        }
        cp.state = serde_json::from_value(encoded).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v75\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v75\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), cp).is_err(),
            "{key}"
        );
    }
}
#[test]
fn optional_appearance_remains_unknown_and_cannot_be_transplanted() {
    let bare = synthetic_fixture::world();
    assert!(bare.state().player_appearance().is_none());
    let mut forged = bare.checkpoint().unwrap();
    let mut encoded = serde_json::to_value(&forged.state).unwrap();
    encoded["source_player_appearance"] =
        serde_json::to_value(world().state().player_appearance()).unwrap();
    forged.state = serde_json::from_value(encoded).unwrap();
    forged.digest = crate::digest(b"benilla-native-state-v75\0", &forged.state);
    forged.checksum = crate::digest(
        b"benilla-native-checkpoint-v75\0",
        &(&forged.state, forged.digest, forged.history_tip),
    );
    assert!(NativeWorld::restore_synthetic_component_test(bare.profile().clone(), forged).is_err());
}
