// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored pose/replay fixtures; no production content or facing predicate admission.
use super::{synthetic_fixture as fixture, *};

fn commands(w: &NativeWorld, commands: Vec<Command>) -> Vec<OwnedInput> {
    commands
        .into_iter()
        .enumerate()
        .map(|(i, command)| OwnedInput {
            authenticated_owner: w.state().player.owner,
            input: Input {
                world: w.state().world,
                tick: w.state().tick + 1,
                player: w.state().player,
                sequence: w.state().next_sequence + i as u64,
                command,
            },
        })
        .collect()
}
fn pose(x: f32, radians: f32) -> Command {
    Command::Pose {
        destination: fixture::point(x),
        orientation: Orientation::new(radians).unwrap(),
    }
}
fn empty_world() -> NativeWorld {
    let mut profile = fixture::profile();
    profile.npcs.clear();
    profile.player.orientation = Orientation::new(0.25).unwrap();
    NativeWorld::synthetic_component_test(profile, [8; 16], fixture::player(), 7).unwrap()
}

#[test]
fn normalized_binary32_preserves_source_whole_turn_behavior_and_canonical_wire() {
    let tau = std::f32::consts::TAU;
    // MapManager::NormalizeOrientation keeps TAU for negative whole turns.
    for (input, expected) in [
        (0.0, 0),
        (-0.0, 0),
        (tau, 0),
        (2.0 * tau, 0),
        (-tau, 0x40c9_0fdb),
        (-2.0 * tau, 0x40c9_0fdb),
        (1.0, 0x3f80_0000),
        (-1.0, 0x40a9_0fdb),
        (7.0, 0x3f37_8128),
        (-7.0, 0x40b2_1fb6),
        (f32::from_bits(1), 1),
        (-f32::from_bits(1), 0x40c9_0fdb),
    ] {
        let value = Orientation::new(input).unwrap();
        assert_eq!(value.bits(), expected, "{input:?}");
        let wire = serde_json::to_vec(&value).unwrap();
        assert_eq!(serde_json::from_slice::<Orientation>(&wire).unwrap(), value);
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(Orientation::new(value), Err(NativeError::InvalidInput));
    }
    for bits in [
        0x8000_0000u32,
        0x40c9_0fdc,
        0x7f80_0000,
        0x7fc0_0000,
        0xbf80_0000,
    ] {
        assert!(Orientation::from_bits(bits).is_err());
        assert!(serde_json::from_str::<Orientation>(&bits.to_string()).is_err());
    }
}

#[test]
fn orientation_is_profile_bound_and_restore_never_uses_a_session_pose() {
    let w = empty_world();
    assert_eq!(w.state().orientation, w.profile().player.orientation);
    let mut wrong = w.profile().clone();
    wrong.player.orientation = Orientation::new(2.0).unwrap();
    assert_ne!(wrong.digest(), w.profile_digest());
    let mut forged = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&forged.state).unwrap();
    state["orientation"] = serde_json::json!(Orientation::new(2.0).unwrap());
    forged.state = serde_json::from_value(state).unwrap();
    forged.digest = crate::digest(b"benilla-native-state-v69\0", &forged.state);
    forged.checksum = crate::digest(
        b"benilla-native-checkpoint-v69\0",
        &(&forged.state, forged.digest, forged.history_tip),
    );
    assert!(matches!(
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged),
        Err(NativeError::InvalidCheckpoint)
    ));

    assert!(matches!(
        NativeWorld::restore_synthetic_component_test(wrong, w.checkpoint().unwrap()),
        Err(NativeError::Incompatible)
    ));
}

#[test]
fn same_tick_turn_cast_turn_has_current_prepare_release_witnesses_without_interruption() {
    let mut p = fixture::profile();
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        unreachable!()
    };
    spec.cast_ms = 50;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], fixture::player(), 7).unwrap();
    let genesis = w.clone();
    let input = commands(
        &w,
        vec![
            pose(0.0, 1.0),
            Command::Cast {
                ability: 60001,
                target: w.state().npcs[&2].id,
            },
            pose(0.0, 2.0),
        ],
    );
    w.step_batch(1, input, &mut fixture::SyntheticAuthority::default())
        .unwrap();
    assert!(w.last_tick().unwrap().replies.iter().any(|r| matches!(r.request.query,
        Query::Spell { phase: CastPhase::Prepare, player_orientation, .. } if player_orientation == Orientation::new(1.0).unwrap())));
    let input = commands(&w, vec![pose(0.0, 3.0)]);
    w.step_batch(2, input, &mut fixture::SyntheticAuthority::default())
        .unwrap();
    assert!(w.last_tick().unwrap().replies.iter().any(|r| matches!(r.request.query,
        Query::Spell { phase: CastPhase::Release, player_orientation, .. } if player_orientation == Orientation::new(3.0).unwrap())));
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::Spell {
            event: spell::SpellEvent::Launched { .. },
            ..
        }
    )));
    assert!(!w
        .records()
        .iter()
        .flat_map(|r| &r.replies)
        .any(|r| matches!(r.request.query, Query::PlayerMovement { .. })));
    assert!(
        !w.records().iter().flat_map(|r| &r.events).any(|e| matches!(
            e,
            Event::Spell {
                event: spell::SpellEvent::Failed(_),
                ..
            }
        ))
    );
    let mut replay = genesis.clone();
    replay
        .replay(&decode_records(&encode_records(w.records()).unwrap()).unwrap())
        .unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    let mut forged = w.records().to_vec();
    let Command::Pose { orientation, .. } = &mut forged[0].inputs[0].command else {
        unreachable!()
    };
    *orientation = Orientation::new(1.5).unwrap();
    let mut invalid = genesis;
    let before = invalid.state_digest();
    assert!(invalid.replay(&forged).is_err());
    assert_eq!(invalid.state_digest(), before);
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
    assert_eq!(restored.state().orientation, Orientation::new(3.0).unwrap());
}

#[derive(Default)]
struct OneSegment {
    requests: Vec<AuthorityRequest>,
}
impl Authority for OneSegment {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let Query::PlayerMovement { accepted_path, .. } = &request.query else {
            panic!("unexpected request")
        };
        self.requests.push(request.clone());
        Ok(AuthorityReply {
            request: request.clone(),
            facts: Facts::PlayerMovement {
                accepted: accepted_path.is_empty(),
            },
        })
    }
}
#[test]
fn turns_do_not_request_geometry_append_distance_or_refresh_frame_budget() {
    let mut w = empty_world();
    let mut authority = OneSegment::default();
    let input = commands(
        &w,
        vec![
            pose(0.0, 1.0),
            pose(1.0, 2.0),
            pose(1.0, 3.0),
            pose(2.0, 4.0),
        ],
    );
    w.step_batch(1, input, &mut authority).unwrap();
    assert_eq!(authority.requests.len(), 2);
    for request in &authority.requests {
        let Query::PlayerMovement {
            frame_origin,
            elapsed_ms,
            ..
        } = &request.query
        else {
            unreachable!()
        };
        assert_eq!(*frame_origin, fixture::point(0.0));
        assert_eq!(*elapsed_ms, 100);
    }
    assert!(
        matches!(&authority.requests[0].query, Query::PlayerMovement { accepted_path, .. } if accepted_path.is_empty())
    );
    assert!(
        matches!(&authority.requests[1].query, Query::PlayerMovement { accepted_path, .. } if accepted_path == &[fixture::point(1.0)])
    );
    assert_eq!(w.state().position, fixture::point(1.0));
    assert_eq!(w.state().orientation, Orientation::new(3.0).unwrap());
    assert_eq!(w.state().next_sequence, 4);
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .contains(&Event::Rejected(Rejection::MovementBlocked)));
    let input = commands(
        &w,
        vec![Command::Move {
            destination: fixture::point(2.0),
        }],
    );
    w.step_batch(2, input, &mut OneSegment::default()).unwrap();
    assert_eq!(w.state().orientation, Orientation::new(3.0).unwrap());
}

#[test]
fn invalid_envelope_and_authority_failure_rollback_prior_same_frame_turns() {
    let mut w = empty_world();
    let original = w.checkpoint().unwrap();
    let mut input = commands(&w, vec![pose(0.0, 1.0)]);
    input[0].input.sequence += 1;
    assert_eq!(
        w.step_batch(1, input, &mut fixture::SyntheticAuthority::default()),
        Err(NativeError::InvalidInput)
    );
    assert_eq!(w.checkpoint().unwrap(), original);
    let input = commands(&w, vec![pose(0.0, 1.0), pose(1.0, 2.0)]);
    let mut missing = fixture::SyntheticAuthority {
        missing_navigation: true,
        ..Default::default()
    };
    assert!(matches!(
        w.step_batch(1, input, &mut missing),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert_eq!(w.checkpoint().unwrap(), original);
    assert!(w.records().is_empty());
}

#[test]
fn dead_pose_rejection_preserves_both_coordinates_and_orientation() {
    let mut w = empty_world();
    w.state.health = 0;
    let prior = (w.state().position, w.state().orientation);
    let input = commands(&w, vec![pose(0.0, 1.0), pose(1.0, 2.0)]);
    w.step_batch(1, input, &mut OneSegment::default()).unwrap();
    assert_eq!((w.state().position, w.state().orientation), prior);
    assert_eq!(
        w.last_tick()
            .unwrap()
            .events
            .iter()
            .filter(|e| **e == Event::Rejected(Rejection::Dead))
            .count(),
        2
    );
}

#[test]
fn previous_pose_less_checkpoints_and_records_need_explicit_migration() {
    let mut w = empty_world();
    let mut old = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    old["version"] = 8.into();
    old["state"]["version"] = 8.into();
    old["state"].as_object_mut().unwrap().remove("orientation");
    assert!(Checkpoint::decode(&serde_json::to_vec(&old).unwrap()).is_err());
    let input = commands(&w, vec![pose(0.0, 1.0)]);
    w.step_batch(1, input, &mut OneSegment::default()).unwrap();
    let mut record = w.records().to_vec();
    record[0].version = 8;
    assert!(decode_records(&encode_records(&record).unwrap()).is_err());
    let mut invalid = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    invalid["state"]["orientation"] = serde_json::json!(0x7f80_0000u32);
    assert!(Checkpoint::decode(&serde_json::to_vec(&invalid).unwrap()).is_err());
}
