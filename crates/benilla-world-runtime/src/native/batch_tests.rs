// SPDX-License-Identifier: GPL-3.0-or-later
use super::{synthetic_fixture as fixture, *};

fn commands(world: &NativeWorld, commands: Vec<Command>) -> Vec<OwnedInput> {
    commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| OwnedInput {
            authenticated_owner: world.state().player.owner,
            input: Input {
                world: world.state().world,
                tick: world.state().tick + 1,
                player: world.state().player,
                sequence: world.state().next_sequence + index as u64,
                command,
            },
        })
        .collect()
}
fn cast(world: &NativeWorld) -> Command {
    Command::Cast {
        ability: 60001,
        target: world.state().npcs[&2].id,
    }
}
fn cancel(sequence: u64) -> Command {
    Command::CancelCast {
        spell_id: 60001,
        target: CancelCastTarget::PriorInput(sequence),
    }
}

#[test]
fn source_order_cast_cancel_recast_preserves_gcd_mana_and_replay() {
    let mut profile = fixture::profile();
    let AbilityAdmission::Available(spec) = &mut profile.abilities[0].admission else {
        unreachable!()
    };
    spec.cast_ms = 50;
    let mut w =
        NativeWorld::synthetic_component_test(profile, [8; 16], fixture::player(), 7).unwrap();
    let genesis = w.clone();
    let inputs = commands(&w, vec![cast(&w), cancel(0), cast(&w), cancel(2), cast(&w)]);
    w.step_batch(1, inputs, &mut fixture::KernelAuthority::default())
        .unwrap();
    assert_eq!(w.state().tick, 1);
    assert_eq!(w.state().now_ms, 100);
    assert_eq!(w.state().next_sequence, 5);
    assert_eq!(w.state().resources.caster.mana, 90);
    assert_eq!(w.state().resources.caster.global_ready_at(9), Some(300));
    assert_eq!(w.state().casts.keys().copied().collect::<Vec<_>>(), vec![3]);
    let events = &w.last_tick().unwrap().events;
    let spells = events
        .iter()
        .filter_map(|e| match e {
            Event::Spell { cast_id, event, .. } => Some((*cast_id, *event)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        spells,
        vec![
            (1, spell::SpellEvent::Started { cast_ms: 50 }),
            (
                1,
                spell::SpellEvent::Failed(spell::CastFailure::Interrupted)
            ),
            (2, spell::SpellEvent::Started { cast_ms: 50 }),
            (
                2,
                spell::SpellEvent::Failed(spell::CastFailure::Interrupted)
            ),
            (3, spell::SpellEvent::Started { cast_ms: 50 }),
        ]
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::InputResult { accepted: true, .. }))
            .count(),
        5
    );
    w.step_batch(2, vec![], &mut fixture::KernelAuthority::default())
        .unwrap();
    assert!(w.last_tick().unwrap().inputs.is_empty());
    assert_eq!(w.state().next_sequence, 5);
    assert_eq!(w.state().resources.caster.mana, 80);
    let encoded = encode_records(w.records()).unwrap();
    let mut replay = genesis;
    replay.replay(&decode_records(&encoded).unwrap()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    let restored = NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
}

#[test]
fn rejected_cast_reference_never_cancels_an_older_active_instance() {
    let mut w = fixture::world();
    let inputs = commands(&w, vec![cast(&w)]);
    w.step_batch(1, inputs, &mut fixture::KernelAuthority::default())
        .unwrap();
    let inputs = commands(&w, vec![cast(&w), cancel(1)]);
    w.step_batch(2, inputs, &mut fixture::KernelAuthority::default())
        .unwrap();
    assert!(matches!(
        w.state().casts[&1].execution.state(),
        spell::CastState::Casting { .. }
    ));
    assert_eq!(w.state().next_cast_id, 2);
    assert_eq!(w.state().next_sequence, 3);
    let outcomes = w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .filter_map(|e| match e {
            Event::InputResult { sequence, accepted } => Some((*sequence, *accepted)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(outcomes, vec![(1, false), (2, true)]);
    assert!(!w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::Spell {
            event: spell::SpellEvent::Failed(spell::CastFailure::Interrupted),
            ..
        }
    )));
}

struct Spy {
    calls: usize,
    fail_movement: bool,
    inner: fixture::KernelAuthority,
}
impl Authority for Spy {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.calls += 1;
        if self.fail_movement && matches!(request.query, Query::PlayerMovement { .. }) {
            return Err(NativeError::AuthorityUnavailable("batch test movement"));
        }
        self.inner.resolve(request)
    }
}

#[test]
fn invalid_envelopes_and_nonprior_cancel_references_reject_entire_batch_before_authority() {
    let w = fixture::world();
    let good = commands(&w, vec![cast(&w), cancel(0)]);
    let mut bad = Vec::new();
    let mut batch = good.clone();
    batch[1].authenticated_owner = [9; 16];
    bad.push(batch);
    let mut batch = good.clone();
    batch[1].input.player.character = [9; 16];
    bad.push(batch);
    let mut batch = good.clone();
    batch[1].input.world = [9; 16];
    bad.push(batch);
    let mut batch = good.clone();
    batch[1].input.tick = 2;
    bad.push(batch);
    let mut batch = good.clone();
    batch[1].input.sequence = 0;
    bad.push(batch);
    let mut batch = good;
    batch[1].input.command = cancel(2);
    bad.push(batch);
    bad.push(commands(&w, vec![Command::ReleaseSpirit, cancel(0)]));
    bad.push(commands(&w, vec![cancel(0)]));
    bad.push(commands(
        &w,
        vec![
            cast(&w),
            Command::CancelCast {
                spell_id: 60001,
                target: CancelCastTarget::Instance(1),
            },
        ],
    ));
    for inputs in bad {
        let mut world = w.clone();
        let mut spy = Spy {
            calls: 0,
            fail_movement: false,
            inner: Default::default(),
        };
        assert_eq!(
            world.step_batch(1, inputs, &mut spy).unwrap_err(),
            NativeError::InvalidInput
        );
        assert_eq!(spy.calls, 0);
        assert_eq!(world.checkpoint().unwrap(), w.checkpoint().unwrap());
        assert!(world.records().is_empty());
    }
    let mut world = w.clone();
    let oversized = commands(&w, vec![Command::ReleaseSpirit; MAX_INPUTS_PER_TICK + 1]);
    assert_eq!(
        world
            .step_batch(1, oversized, &mut fixture::KernelAuthority::default())
            .unwrap_err(),
        NativeError::TooLarge
    );
    let mut nested = serde_json::to_value(commands(&w, vec![cast(&w)])[0].input.clone()).unwrap();
    nested["command"] = serde_json::json!({"Batch": []});
    assert!(serde_json::from_value::<Input>(nested).is_err());
}

#[test]
fn later_authority_failure_rolls_back_earlier_cast_then_exact_retry_commits_once() {
    let mut w = fixture::world();
    let before = w.clone();
    let inputs = commands(
        &w,
        vec![
            cast(&w),
            Command::Move {
                destination: fixture::point(0.25),
            },
        ],
    );
    let mut spy = Spy {
        calls: 0,
        fail_movement: true,
        inner: Default::default(),
    };
    assert!(matches!(
        w.step_batch(1, inputs.clone(), &mut spy),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert!(spy.calls >= 2);
    assert_eq!(w.checkpoint().unwrap(), before.checkpoint().unwrap());
    assert!(w.records().is_empty());
    spy.fail_movement = false;
    w.step_batch(1, inputs, &mut spy).unwrap();
    assert_eq!(w.state().next_sequence, 2);
    assert_eq!(w.state().next_cast_id, 2);
    assert_eq!(w.state().position, fixture::point(0.25));
    let mut replay = before;
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}

#[test]
fn v3_records_and_snapshots_cannot_cross_the_v4_boundary() {
    let mut w = fixture::world();
    w.step_batch(1, vec![], &mut fixture::KernelAuthority::default())
        .unwrap();
    let mut old = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    old["version"] = 3.into();
    assert!(Checkpoint::decode(&serde_json::to_vec(&old).unwrap()).is_err());
    let mut record = w.records()[0].clone();
    record.version = 3;
    assert!(decode_records(&serde_json::to_vec(&vec![record]).unwrap()).is_err());
    let mut old_record = serde_json::to_value(&w.records()[0]).unwrap();
    old_record.as_object_mut().unwrap().remove("inputs");
    old_record["input"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<TickRecord>(old_record).is_err());
}

// Deliberately synthetic one-unit/second straight-line budget oracle. This proves the
// caller's cumulative contract, not native speed, terrain, collision or player authority.
#[derive(Default)]
struct PathBudget {
    seen: Vec<AuthorityRequest>,
    wrong_path_echo: bool,
    inner: fixture::KernelAuthority,
}
impl Authority for PathBudget {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let Query::PlayerMovement {
            frame_origin,
            accepted_path,
            destination,
            elapsed_ms,
        } = &request.query
        else {
            return self.inner.resolve(request);
        };
        assert!(accepted_path.len() < MAX_INPUTS_PER_TICK);
        self.seen.push(request.clone());
        let mut prior = *frame_origin;
        let distance = accepted_path
            .iter()
            .chain(std::iter::once(destination))
            .map(|point| {
                let distance = f64::from((point.x.as_f32() - prior.x.as_f32()).abs());
                assert_eq!(point.y.as_f32(), 0.0);
                assert_eq!(point.z.as_f32(), 0.0);
                prior = *point;
                distance
            })
            .sum::<f64>();
        let mut echo = request.clone();
        if self.wrong_path_echo {
            let Query::PlayerMovement { accepted_path, .. } = &mut echo.query else {
                unreachable!()
            };
            accepted_path.push(fixture::point(999.0));
        }
        Ok(AuthorityReply {
            request: echo,
            facts: Facts::PlayerMovement {
                accepted: distance <= f64::from(*elapsed_ms) / 1000.0,
            },
        })
    }
}

#[test]
fn movement_batch_shares_one_frame_path_budget_including_backtracking() {
    let mut w = fixture::world();
    let mut authority = PathBudget::default();
    let inputs = commands(
        &w,
        vec![
            Command::Move {
                destination: fixture::point(0.025),
            },
            Command::Move {
                destination: fixture::point(0.05),
            },
            Command::Move {
                destination: fixture::point(-0.025),
            }, // 0.125 travelled, rejected.
            Command::Move {
                destination: fixture::point(0.075),
            }, // Earlier rejection consumes no path.
        ],
    );
    let before = w.clone();
    w.step_batch(1, inputs, &mut authority).unwrap();
    assert_eq!(w.state().position, fixture::point(0.075));
    assert_eq!(w.state().next_sequence, 4);
    let paths = authority
        .seen
        .iter()
        .map(|request| match &request.query {
            Query::PlayerMovement {
                frame_origin,
                accepted_path,
                elapsed_ms,
                ..
            } => {
                assert_eq!(*frame_origin, fixture::point(0.0));
                assert_eq!(*elapsed_ms, 100);
                accepted_path.clone()
            }
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            vec![],
            vec![fixture::point(0.025)],
            vec![fixture::point(0.025), fixture::point(0.05)],
            vec![fixture::point(0.025), fixture::point(0.05)]
        ]
    );
    assert_eq!(
        w.last_tick()
            .unwrap()
            .events
            .iter()
            .filter(|e| matches!(e, Event::Rejected(Rejection::MovementBlocked)))
            .count(),
        1
    );
    let mut replay = before;
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    let inputs = commands(
        &w,
        vec![Command::Move {
            destination: fixture::point(0.1),
        }],
    );
    w.step_batch(2, inputs, &mut authority).unwrap();
    let Query::PlayerMovement {
        frame_origin,
        accepted_path,
        ..
    } = &authority.seen.last().unwrap().query
    else {
        unreachable!()
    };
    assert_eq!(*frame_origin, fixture::point(0.075));
    assert!(accepted_path.is_empty());
}

#[test]
fn cumulative_movement_echo_and_replay_bind_every_path_sample() {
    let mut w = fixture::world();
    let before = w.clone();
    let inputs = commands(
        &w,
        vec![Command::Move {
            destination: fixture::point(0.025),
        }],
    );
    let mut authority = PathBudget {
        wrong_path_echo: true,
        ..Default::default()
    };
    assert_eq!(
        w.step_batch(1, inputs.clone(), &mut authority).unwrap_err(),
        NativeError::AuthorityBinding
    );
    assert_eq!(w.checkpoint().unwrap(), before.checkpoint().unwrap());
    authority.wrong_path_echo = false;
    w.step_batch(1, inputs, &mut authority).unwrap();
    let mut record = w.last_tick().unwrap().clone();
    let reply = record
        .replies
        .iter_mut()
        .find(|r| matches!(r.request.query, Query::PlayerMovement { .. }))
        .unwrap();
    let Query::PlayerMovement { accepted_path, .. } = &mut reply.request.query else {
        unreachable!()
    };
    accepted_path.push(fixture::point(0.01));
    let mut replay = before;
    assert_eq!(
        replay.replay(&[record]).unwrap_err(),
        NativeError::ReplayMismatch
    );
    assert_eq!(replay.state().tick, 0);
}
