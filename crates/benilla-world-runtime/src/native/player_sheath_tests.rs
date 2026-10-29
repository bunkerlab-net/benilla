// SPDX-License-Identifier: GPL-3.0-or-later
//! Real component commands and source-authored owners, not native admission.
use super::*;
use synthetic_equipment_fixture::EquipmentAuthority;
fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [1; 16], synthetic_fixture::player(), 42).unwrap()
}
fn step(w: &mut NativeWorld, command: Command) {
    let input = synthetic_quest_fixture::command(w, command);
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut EquipmentAuthority::default(),
    )
    .unwrap();
}
fn sheath(w: &mut NativeWorld, state: u32) {
    step(w, Command::SetSheathed { state });
}
fn reopen(w: &NativeWorld) {
    let cp = w.checkpoint().unwrap();
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&cp.encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), cp);
}
fn inventory(w: &NativeWorld) -> &benilla_sim::inventory::State {
    w.state
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
}
#[test]
fn valid_states_do_not_require_weapon_and_invalid_raw_values_never_truncate() {
    let mut w = world(synthetic_equipment_fixture::profile());
    assert!(w.profile.supports_player_sheath());
    assert!(w
        .state
        .combat
        .as_ref()
        .unwrap()
        .equipment
        .item(15)
        .is_none());
    assert_eq!(w.state.sheath_state(), 0);
    let anchor = w.checkpoint().unwrap();
    for expected in [0, 1, 1, 2, 2, 0] {
        sheath(&mut w, expected);
        assert_eq!(w.state.sheath_state(), expected as u8);
        for invalid in [3, 255, 256, u32::MAX] {
            sheath(&mut w, invalid);
            assert_eq!(w.state.sheath_state(), expected as u8);
        }
    }
    reopen(&w);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), anchor).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn unsupported_valid_request_is_atomic_but_invalid_raw_is_source_noop() {
    let mut w = synthetic_fixture::world();
    assert!(!w.profile.supports_player_sheath());
    let checkpoint = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    let input = synthetic_quest_fixture::command(&w, Command::SetSheathed { state: 1 });
    assert!(w
        .step(
            w.state.tick + 1,
            Some(input),
            &mut synthetic_fixture::SyntheticAuthority::default()
        )
        .is_err());
    assert_eq!(w.checkpoint().unwrap(), checkpoint);
    assert_eq!(w.records(), records);
    for state in [3, 256, u32::MAX] {
        let input = synthetic_quest_fixture::command(&w, Command::SetSheathed { state });
        w.step(
            w.state.tick + 1,
            Some(input),
            &mut synthetic_fixture::SyntheticAuthority::default(),
        )
        .unwrap();
        assert_eq!(w.state.sheath_state(), 0);
    }
    reopen(&w);
}
#[test]
fn sheath_changes_preserve_current_equipment_stats_and_allocator_across_detach() {
    let mut w = world(synthetic_equipment_fixture::weapon_profile());
    let anchor = w.checkpoint().unwrap();
    for detached in [false, true, false] {
        if detached {
            step(
                &mut w,
                Command::EquipmentMove(EquipmentMoveRequest {
                    source: 15,
                    destination: 23,
                }),
            );
        } else if w
            .state
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(15)
            .is_none()
        {
            step(
                &mut w,
                Command::EquipmentMove(EquipmentMoveRequest {
                    source: 23,
                    destination: 15,
                }),
            );
        }
        assert_eq!(
            w.state
                .combat
                .as_ref()
                .unwrap()
                .equipment
                .item(15)
                .is_none(),
            detached
        );
        let items = inventory(&w).clone();
        let equipment = w.state.combat.as_ref().unwrap().equipment.snapshot();
        let stats = w.state.combat.as_ref().unwrap().stats.clone();
        let recipe = w.state.combat.as_ref().unwrap().recipe.snapshot();
        for state in [1, 2, 0] {
            sheath(&mut w, state);
            assert_eq!(w.state.sheath_state(), state as u8);
            assert_eq!(inventory(&w), &items);
            assert_eq!(
                w.state.combat.as_ref().unwrap().equipment.snapshot(),
                equipment
            );
            assert_eq!(w.state.combat.as_ref().unwrap().stats, stats);
            assert_eq!(w.state.combat.as_ref().unwrap().recipe.snapshot(), recipe);
        }
        reopen(&w);
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), anchor).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn actual_combat_and_death_do_not_guard_sheath_commands() {
    for lethal in [false, true] {
        let p = if lethal {
            synthetic_equipment_fixture::lethal_weapon_profile()
        } else {
            synthetic_equipment_fixture::weapon_profile()
        };
        let mut w = world(p);
        let target = w.state.npcs[&2].id;
        // A distant defensive creature does not enter combat merely because
        // the player selected AttackStart. Use actual source spell damage.
        step(
            &mut w,
            Command::Cast {
                ability: 133,
                target,
            },
        );
        sheath(&mut w, 2);
        assert_eq!(w.state.sheath_state(), 2);
        for _ in 0..150 {
            if if lethal {
                !w.state.player_alive()
            } else {
                w.state.combat.as_ref().unwrap().in_combat
            } {
                break;
            }
            w.step(w.state.tick + 1, None, &mut EquipmentAuthority::default())
                .unwrap();
        }
        if lethal {
            assert!(!w.state.player_alive());
            assert_eq!(w.state.health, 0);
        } else {
            assert!(w.state.combat.as_ref().unwrap().in_combat);
        }
        let anchor = w.checkpoint().unwrap();
        let prefix = w.records().len();
        sheath(&mut w, 1);
        assert_eq!(w.state.sheath_state(), 1);
        if lethal {
            assert_eq!(w.state.health, 0);
            assert!(!w.state.player_alive());
        }
        reopen(&w);
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), anchor).unwrap();
        replay.replay(&w.records()[prefix..]).unwrap();
        assert_eq!(replay.state_digest(), w.state_digest());
    }
}

#[test]
fn required_ledger_rejects_rehashed_corruption_and_same_value_does_not_add_writes() {
    let mut w = world(synthetic_equipment_fixture::weapon_profile());
    sheath(&mut w, 1);
    let first = serde_json::to_value(&w.checkpoint().unwrap().state).unwrap();
    assert_eq!(
        first["player_sheath"]["writes"],
        serde_json::json!([{"sequence":0,"value":1}])
    );
    reopen(&w); // Zero is the genuine first command sequence, never a forged sentinel.
    for state in [1, 3, 256, u32::MAX] {
        sheath(&mut w, state);
    }
    let unchanged = serde_json::to_value(&w.checkpoint().unwrap().state).unwrap();
    assert_eq!(unchanged["player_sheath"], first["player_sheath"]);
    sheath(&mut w, 2);
    reopen(&w);
    for fault in ["value", "write", "future", "duplicate", "missing", "noop"] {
        let mut cp = w.checkpoint().unwrap();
        let mut json = serde_json::to_value(&cp.state).unwrap();
        match fault {
            "value" => json["player_sheath"]["value"] = serde_json::json!(3),
            "write" => json["player_sheath"]["writes"][1]["value"] = serde_json::json!(3),
            "future" => {
                json["player_sheath"]["writes"][1]["sequence"] =
                    serde_json::json!(w.state.next_sequence)
            }
            "duplicate" => json["player_sheath"]["writes"][1]["sequence"] = serde_json::json!(0),
            "missing" => {
                json.as_object_mut().unwrap().remove("player_sheath");
            }
            "noop" => {
                json["player_sheath"]["writes"][1]["value"] = serde_json::json!(1);
                json["player_sheath"]["value"] = serde_json::json!(1);
            }
            _ => unreachable!(),
        }
        if fault == "missing" {
            assert!(serde_json::from_value::<persistence::StateCheckpoint>(json).is_err());
            continue;
        }
        cp.state = serde_json::from_value(json).unwrap();
        cp.digest = crate::digest(b"benilla-native-state-v81\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v81\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).is_err(),
            "{fault}"
        );
    }
}
