// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use synthetic_player_appearance_fixture as a;
use synthetic_source_spline_fixture as f;
fn world() -> NativeWorld {
    let player = synthetic_fixture::player();
    let mut p = f::surviving_profile();
    p.initialize_player_appearance(a::request(player), &a::catalog())
        .unwrap();
    NativeWorld::synthetic_component_test(p, [8; 16], player, 42).unwrap()
}
fn step(w: &mut NativeWorld, command: Command) {
    let input = synthetic_quest_fixture::command(w, command);
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut f::CompleteAuthority::default(),
    )
    .unwrap();
}
fn reopen(w: &NativeWorld) {
    let cp = w.checkpoint().unwrap();
    let opened = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&cp.encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(opened.checkpoint().unwrap(), cp);
}
fn resign(cp: &mut Checkpoint, value: serde_json::Value) {
    cp.state = serde_json::from_value(value).unwrap();
    cp.digest = crate::digest(b"benilla-native-state-v72\0", &cp.state);
    cp.checksum = crate::digest(
        b"benilla-native-checkpoint-v72\0",
        &(&cp.state, cp.digest, cp.history_tip),
    );
}
#[test]
fn source_xor_toggles_preserve_batch_sequence_and_replay() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    let commands = [
        Command::ToggleHelm,
        Command::SetWatchedFaction { list_id: -12 },
        Command::ToggleHelm,
        Command::ToggleCloak,
    ];
    let inputs = commands
        .into_iter()
        .enumerate()
        .map(|(i, command)| {
            let mut input = synthetic_quest_fixture::command(&w, command);
            input.input.sequence += i as u64;
            input
        })
        .collect();
    w.step_batch(1, inputs, &mut f::CompleteAuthority::default())
        .unwrap();
    assert_eq!(w.state.player_hide_flags(), 0x800);
    let json = serde_json::to_value(w.checkpoint().unwrap().state).unwrap();
    assert_eq!(
        json["player_hide_flags"]["toggles"],
        serde_json::json!([
            {"sequence":0,"helm":true},{"sequence":2,"helm":true},{"sequence":3,"helm":false}
        ])
    );
    assert_eq!(w.state.watched_faction(), -12);
    step(&mut w, Command::ToggleCloak);
    assert_eq!(w.state.player_hide_flags(), 0);
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn actual_combat_does_not_guard_source_hide_toggles() {
    let mut w = world();
    let target = w.state.npcs[&2].id;
    step(&mut w, Command::AttackStart { target });
    assert!(w.state.combat.as_ref().unwrap().in_combat);
    let health = w.state.health;
    step(&mut w, Command::ToggleHelm);
    assert_eq!(w.state.player_hide_flags(), 0x400);
    assert!(w.state.health <= health);
    assert!(w.state.combat.as_ref().unwrap().in_combat);
    reopen(&w);
}
#[test]
fn invalid_or_missing_hide_ledger_is_rejected_even_with_current_hashes() {
    let mut w = world();
    step(&mut w, Command::ToggleHelm);
    step(&mut w, Command::ToggleCloak);
    for corruption in ["flags", "future", "order", "target", "missing"] {
        let mut cp = w.checkpoint().unwrap();
        let mut value = serde_json::to_value(&cp.state).unwrap();
        let ledger = &mut value["player_hide_flags"];
        match corruption {
            "flags" => ledger["flags"] = serde_json::json!(0xc10),
            "future" => ledger["toggles"][1]["sequence"] = serde_json::json!(w.state.next_sequence),
            "order" => ledger["toggles"][1]["sequence"] = serde_json::json!(0),
            "target" => ledger["toggles"][0]["helm"] = serde_json::json!(false),
            "missing" => {
                value.as_object_mut().unwrap().remove("player_hide_flags");
            }
            _ => unreachable!(),
        }
        if corruption == "missing" {
            assert!(serde_json::from_value::<persistence::StateCheckpoint>(value).is_err());
            continue;
        }
        resign(&mut cp, value);
        assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err());
    }
}
#[test]
fn unsupported_domain_refuses_commands_and_rehashed_nondefault_ledger() {
    let mut bare = synthetic_fixture::world();
    let before = bare.checkpoint().unwrap();
    let input = synthetic_quest_fixture::command(&bare, Command::ToggleHelm);
    assert!(matches!(
        bare.step(
            1,
            Some(input),
            &mut synthetic_fixture::SyntheticAuthority::default()
        ),
        Err(NativeError::UnsupportedMechanic(_))
    ));
    assert_eq!(bare.checkpoint().unwrap(), before);
    let mut qualified = world();
    step(&mut qualified, Command::ToggleHelm);
    let ledger = serde_json::to_value(qualified.checkpoint().unwrap().state).unwrap()
        ["player_hide_flags"]
        .clone();
    let input = synthetic_quest_fixture::command(&bare, Command::SetWatchedFaction { list_id: 0 });
    bare.step(
        1,
        Some(input),
        &mut synthetic_fixture::SyntheticAuthority::default(),
    )
    .unwrap();
    let mut cp = bare.checkpoint().unwrap();
    let mut value = serde_json::to_value(&cp.state).unwrap();
    value["player_hide_flags"] = ledger;
    resign(&mut cp, value);
    assert!(NativeWorld::restore_synthetic_component_test(bare.profile.clone(), cp).is_err());
    // Checkpoint construction also refuses a forged in-memory ledger.
    bare.state.player_hide_flags = qualified.state.player_hide_flags.clone();
    assert!(bare.checkpoint().is_err());
}
#[test]
fn bounded_toggle_history_refuses_without_mutation_and_decodes_bounded() {
    let p = world().profile.clone();
    let mut owner = player_hide_flags::PlayerHideFlags::default();
    for sequence in 0..2048 {
        owner.toggle(sequence % 2 == 0, sequence).unwrap();
    }
    owner.validate(&p, 2048).unwrap();
    let before = owner.clone();
    assert!(matches!(owner.toggle(true, 2048), Err(NativeError::Limit)));
    assert_eq!(owner, before);
    let mut encoded = serde_json::to_value(owner).unwrap();
    encoded["toggles"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"sequence":2048,"helm":true}));
    assert!(serde_json::from_value::<player_hide_flags::PlayerHideFlags>(encoded).is_err());
}
