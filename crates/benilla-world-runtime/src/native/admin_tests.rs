// SPDX-License-Identifier: GPL-3.0-or-later
use super::synthetic_fixture::player;
use super::synthetic_quest_fixture::{self as fixture, command, QuestAuthority};
use super::*;
use benilla_sim::{player_progression as pg, quest_lifecycle as q};

fn profile() -> Profile {
    let mut p = fixture::profile();
    p.admin = Some(AdminPolicy {
        world: [1; 16],
        world_owner: player().owner,
        allowlist: Default::default(),
        revision: 7,
    });
    p
}
fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [1; 16], player(), 42).unwrap()
}
fn request(w: &NativeWorld, key: u64, operation: AdminOperation) -> AdminRequest {
    let mut id = [0; 16];
    id[..8].copy_from_slice(&key.to_le_bytes());
    AdminRequest {
        key: id,
        world: w.state.world,
        player: w.state.player,
        policy_revision: 7,
        operation,
    }
}
fn apply(w: &mut NativeWorld, r: AdminRequest) {
    let input = command(w, Command::Admin(r));
    w.step(
        w.state.tick + 1,
        Some(input),
        &mut QuestAuthority::default(),
    )
    .unwrap();
}
fn progression(w: &NativeWorld) -> &QuestProgressionState {
    w.state.quest_progression.as_ref().unwrap()
}
fn denied(w: &NativeWorld, reason: AdminRejection) {
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::Rejected(Rejection::Admin(r)) if *r == reason)));
}
#[test]
fn default_disabled_external_owner_and_policy_context_are_distinct() {
    let mut w = fixture::world();
    let r = request(&w, 1, AdminOperation::GainOneLevel);
    apply(&mut w, r);
    denied(&w, AdminRejection::Disabled);
    assert_eq!(w.player_level(), 1);
    let mut w = world();
    let r = request(&w, 1, AdminOperation::GainOneLevel);
    let mut wrong = command(&w, Command::Admin(r.clone()));
    wrong.authenticated_owner = [99; 16];
    let before = w.state_digest();
    assert_eq!(
        w.step(1, Some(wrong), &mut QuestAuthority::default())
            .unwrap_err(),
        NativeError::InvalidInput
    );
    assert_eq!(w.state_digest(), before);
    let mut stale = r.clone();
    stale.policy_revision += 1;
    apply(&mut w, stale);
    denied(&w, AdminRejection::StalePolicy);
    let mut wrong = r.clone();
    wrong.world = [99; 16];
    apply(&mut w, wrong);
    denied(&w, AdminRejection::WrongContext);
    let mut wrong = r.clone();
    wrong.player.character = [99; 16];
    apply(&mut w, wrong);
    denied(&w, AdminRejection::WrongContext);
    let mut zero = r;
    zero.key = [0; 16];
    apply(&mut w, zero);
    denied(&w, AdminRejection::InvalidKey);
    assert_eq!(w.player_level(), 1);
    let mut p = profile();
    p.admin.as_mut().unwrap().world_owner = [99; 16];
    let mut w = NativeWorld::synthetic_component_test(p.clone(), [1; 16], player(), 42).unwrap();
    let r = request(&w, 1, AdminOperation::GainOneLevel);
    apply(&mut w, r);
    denied(&w, AdminRejection::Unauthorized);
    p.admin.as_mut().unwrap().allowlist.insert(player().owner);
    let mut w = NativeWorld::synthetic_component_test(p.clone(), [1; 16], player(), 42).unwrap();
    let r = request(&w, 1, AdminOperation::GainOneLevel);
    apply(&mut w, r);
    assert_eq!(w.player_level(), 2);
    assert!(NativeWorld::synthetic_component_test(p, [2; 16], player(), 42).is_err());
}
#[test]
fn direct_level_keeps_existing_xp_runs_hooks_and_honestly_caps_at_supplied_rows() {
    let mut w = world();
    let genesis = w.checkpoint().unwrap();
    let mut authority = QuestAuthority::default();
    fixture::kill_target(&mut w, &mut authority);
    assert_eq!(
        (w.player_level(), progression(&w).player.view().xp),
        (2, 10)
    );
    let before_calls = progression(&w).derived_calls.len();
    let r = request(&w, 1, AdminOperation::GainOneLevel);
    apply(&mut w, r.clone());
    assert_eq!(
        (w.player_level(), progression(&w).player.view().xp),
        (3, 10)
    );
    assert!(progression(&w).derived_calls.len() > before_calls);
    assert_eq!(w.state.combat.as_ref().unwrap().skills[&8].maximum, 15);
    let snapshot = w.admin_snapshot(player().owner).unwrap();
    assert_eq!(
        (
            snapshot.supported_maximum,
            snapshot.configured_maximum,
            snapshot.classic_ceiling
        ),
        (3, 60, 60)
    );
    assert_eq!(pg::MAX_SUPPORTED_LEVEL, 60);
    let receipt = w.admin_receipt(&r.key).unwrap().clone();
    let calls = progression(&w).derived_calls.clone();
    apply(&mut w, r.clone());
    assert_eq!(w.admin_receipt(&r.key), Some(&receipt));
    assert_eq!(progression(&w).derived_calls, calls);
    let capped = request(&w, 2, AdminOperation::GainOneLevel);
    apply(&mut w, capped.clone());
    assert!(matches!(
        w.admin_receipt(&capped.key).unwrap().effect,
        AdminEffect::Level {
            before: 3,
            after: 3,
            xp: 10,
            supported_maximum: 3
        }
    ));
    assert_eq!(progression(&w).derived_calls, calls);
    let mut conflicting = r;
    conflicting.operation = AdminOperation::SetXpGain { enabled: false };
    apply(&mut w, conflicting);
    denied(&w, AdminRejection::RequestConflict);
    let mut replay = NativeWorld::restore_synthetic_component_test(profile(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let restored = NativeWorld::restore_synthetic_component_test(
        profile(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
}
#[test]
fn xp_off_keeps_xp_rest_and_credit_money_but_emits_no_xp_awards() {
    let mut w = world();
    let mut a = QuestAuthority::default();
    fixture::accept(&mut w, &mut a);
    let off = request(&w, 1, AdminOperation::SetXpGain { enabled: false });
    apply(&mut w, off);
    let rest = progression(&w).rest;
    fixture::kill_target(&mut w, &mut a);
    assert_eq!((w.player_level(), progression(&w).player.view().xp), (1, 0));
    assert_eq!(progression(&w).rest, rest);
    assert_eq!(
        progression(&w).quests.progress(70001).unwrap().status,
        q::Status::Complete
    );
    assert!(
        !w.records().iter().flat_map(|r| &r.events).any(|e| matches!(
            e,
            Event::QuestProgression(QuestProgressionEvent::KillExperience(_))
        ))
    );
    let reward = command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(w.state.tick + 1, Some(reward), &mut a).unwrap();
    assert_eq!((w.player_level(), progression(&w).player.view().xp), (1, 0));
    assert_eq!(progression(&w).money, 18);
    assert!(progression(&w).quests.progress(70001).unwrap().rewarded);
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::QuestProgression(QuestProgressionEvent::Quest {
            event: q::Event::RewardAnnouncement { xp: 0, .. },
            ..
        })
    )));
    assert!(!w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::QuestProgression(QuestProgressionEvent::Progression(_))
    )));
    let restored =
        NativeWorld::restore_synthetic_component_test(profile(), w.checkpoint().unwrap()).unwrap();
    assert!(!progression(&restored).xp_enabled);
    let on = request(&w, 2, AdminOperation::SetXpGain { enabled: true });
    apply(&mut w, on);
    assert!(progression(&w).xp_enabled);
    assert_eq!(progression(&w).player.view().xp, 0);
}
#[test]
fn toggles_preserve_nonzero_xp_and_turn_rewards_back_on() {
    let mut w = world();
    let mut a = QuestAuthority::default();
    fixture::accept(&mut w, &mut a);
    fixture::kill_target(&mut w, &mut a);
    assert_eq!(progression(&w).player.view().xp, 10);
    let off = request(&w, 1, AdminOperation::SetXpGain { enabled: false });
    apply(&mut w, off);
    assert_eq!(progression(&w).player.view().xp, 10);
    let on = request(&w, 2, AdminOperation::SetXpGain { enabled: true });
    apply(&mut w, on);
    assert_eq!(progression(&w).player.view().xp, 10);
    let reward = command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    w.step(w.state.tick + 1, Some(reward), &mut a).unwrap();
    assert_eq!(
        (w.player_level(), progression(&w).player.view().xp),
        (3, 30)
    );
}
#[test]
fn receipts_are_bounded_without_eviction_or_stopping_ordinary_ticks() {
    let mut w = world();
    // Real commands, 64 per fixed update; no forged receipt insertions.
    for batch in 0..MAX_ADMIN_RECEIPTS / MAX_INPUTS_PER_TICK {
        let inputs = (0..MAX_INPUTS_PER_TICK)
            .map(|i| {
                let request = request(
                    &w,
                    (batch * MAX_INPUTS_PER_TICK + i + 1) as u64,
                    AdminOperation::SetXpGain { enabled: false },
                );
                let mut input = command(&w, Command::Admin(request));
                input.input.sequence += i as u64;
                input
            })
            .collect();
        w.step_batch(w.state.tick + 1, inputs, &mut QuestAuthority::default())
            .unwrap();
    }
    let first = request(&w, 1, AdminOperation::SetXpGain { enabled: false });
    let receipt = w.admin_receipt(&first.key).unwrap().clone();
    let overflow = request(&w, 10_000, AdminOperation::GainOneLevel);
    apply(&mut w, overflow);
    denied(&w, AdminRejection::ReceiptCapacity);
    assert_eq!(w.player_level(), 1);
    apply(&mut w, first.clone());
    assert_eq!(w.admin_receipt(&first.key), Some(&receipt));
    w.step(w.state.tick + 1, None, &mut QuestAuthority::default())
        .unwrap();
    let restored =
        NativeWorld::restore_synthetic_component_test(profile(), w.checkpoint().unwrap()).unwrap();
    assert_eq!(
        restored
            .admin_snapshot(player().owner)
            .unwrap()
            .receipts_remaining,
        0
    );
}
#[test]
fn altered_receipt_policy_and_unreceipted_xp_lock_fail_restore() {
    let mut w = world();
    let off = request(&w, 1, AdminOperation::SetXpGain { enabled: false });
    apply(&mut w, off);
    let checkpoint = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    for field in ["policy", "flag", "receipt", "sequence"] {
        let mut forged = checkpoint.clone();
        match field {
            "policy" => {
                forged["state"]["admin"]["receipts"][0]["request"]["policy_revision"] = 8.into()
            }
            "flag" => forged["state"]["quest_progression"]["xp_enabled"] = true.into(),
            "receipt" => forged["state"]["admin"]["receipts"] = serde_json::json!([]),
            "sequence" => forged["state"]["admin"]["receipts"][0]["sequence"] = 9999.into(),
            _ => unreachable!(),
        }
        let state: super::persistence::StateCheckpoint =
            serde_json::from_value(forged["state"].clone()).unwrap();
        let digest = crate::digest(b"benilla-native-state-v29\0", &state);
        forged["digest"] = serde_json::to_value(digest).unwrap();
        let tip: crate::Digest = serde_json::from_value(forged["history_tip"].clone()).unwrap();
        forged["checksum"] = serde_json::to_value(crate::digest(
            b"benilla-native-checkpoint-v29\0",
            &(state, digest, tip),
        ))
        .unwrap();
        let decoded: Checkpoint = serde_json::from_value(forged).unwrap();
        assert!(
            NativeWorld::restore_synthetic_component_test(profile(), decoded).is_err(),
            "{field}"
        );
    }
    let mut revoked = profile();
    revoked.admin = None;
    assert!(
        NativeWorld::restore_synthetic_component_test(revoked, w.checkpoint().unwrap()).is_err()
    );
}

#[test]
fn later_authority_failure_rolls_back_admin_level_receipt_and_hooks() {
    let mut w = world();
    let r = request(&w, 1, AdminOperation::GainOneLevel);
    let first = command(&w, Command::Admin(r.clone()));
    let mut second = command(
        &w,
        Command::AcceptQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    second.input.sequence += 1;
    let before = w.checkpoint().unwrap();
    let mut a = QuestAuthority {
        wrong: true,
        ..Default::default()
    };
    assert_eq!(
        w.step_batch(1, vec![first, second], &mut a).unwrap_err(),
        NativeError::AuthorityBinding
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    assert!(w.admin_receipt(&r.key).is_none());
    apply(&mut w, r);
    assert_eq!(w.player_level(), 2);
}

#[test]
fn ordinary_xp_beyond_supplied_rows_refuses_atomically_without_fabricating_level_four() {
    let (mut p, input, mut rows) = fixture::setup();
    rows.quests[0].reward.xp.reward_money_max_level = 600;
    p.initialize_quest_progression(&input, rows).unwrap();
    p.admin = profile().admin;
    let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 42).unwrap();
    let mut a = QuestAuthority::default();
    fixture::accept(&mut w, &mut a);
    fixture::kill_target(&mut w, &mut a);
    let r = request(&w, 1, AdminOperation::GainOneLevel);
    apply(&mut w, r);
    assert_eq!(w.player_level(), 3);
    let before = w.checkpoint().unwrap();
    let reward = command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver: w.state.npcs[&3].id,
        },
    );
    assert!(w.step(w.state.tick + 1, Some(reward), &mut a).is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(w.player_level(), 3);
    assert!(!progression(&w).quests.progress(70001).unwrap().rewarded);
}
