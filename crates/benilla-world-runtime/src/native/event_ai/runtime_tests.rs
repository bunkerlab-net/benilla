// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored runtime proof, separate from source capture and graphical assets.
use super::*;
use crate::native::{synthetic_event_ai_fixture as fixture, synthetic_fixture as f};

fn step(world: &mut NativeWorld, authority: &mut impl Authority) -> Result<(), NativeError> {
    world
        .step(world.state.tick + 1, None, authority)
        .map(|_| ())
}
fn entered(world: &mut NativeWorld, authority: &mut impl Authority) {
    for _ in 0..5 {
        step(world, authority).unwrap();
        if world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::EventAiEvaluation(_)))
        {
            return;
        }
    }
    panic!("authored source callback did not run");
}
fn evaluation(world: &NativeWorld) -> &EventAiEvaluation {
    world
        .last_tick()
        .unwrap()
        .events
        .iter()
        .find_map(|e| match e {
            Event::EventAiEvaluation(e) => Some(e.as_ref()),
            _ => None,
        })
        .unwrap()
}
fn event_draws(world: &NativeWorld) -> Vec<&Draw> {
    world
        .last_tick()
        .unwrap()
        .draws
        .iter()
        .filter(|d| {
            matches!(
                d.purpose,
                DrawPurpose::EventAiChance { .. } | DrawPurpose::EventAiAction { .. }
            )
        })
        .collect()
}
fn rehashed(mut value: serde_json::Value) -> Checkpoint {
    let state: persistence::StateCheckpoint =
        serde_json::from_value(value["state"].clone()).unwrap();
    let digest = crate::digest(b"benilla-native-state-v46\0", &state);
    let tip: crate::Digest = serde_json::from_value(value["history_tip"].clone()).unwrap();
    value["digest"] = serde_json::to_value(digest).unwrap();
    value["checksum"] = serde_json::to_value(crate::digest(
        b"benilla-native-checkpoint-v46\0",
        &(&state, digest, tip),
    ))
    .unwrap();
    serde_json::from_value(value).unwrap()
}
#[test]
fn genuine_sight_callback_consumes_shared_full_words_once_before_motion() {
    let mut world = fixture::world(7);
    assert!(world.state.event_ai.as_ref().unwrap().history.is_empty());
    assert_eq!(world.state.rng_counter, 0);
    entered(&mut world, &mut fixture::Authority::default());
    let record = world.last_tick().unwrap();
    let eval = evaluation(&world);
    assert_eq!(eval.occurrence, 0);
    assert_eq!(eval.actions.len(), 1);
    assert_eq!(
        eval.callback.npc,
        crate::NpcId {
            spawn: 2,
            generation: 0
        }
    );
    assert_eq!(eval.callback.invoker, f::player());
    assert!(matches!(
        eval.callback.kind,
        EventAiCallbackKind::Sight { .. }
    ));
    let draws = event_draws(&world);
    assert_eq!(draws.len(), 2);
    for (index, draw) in draws.iter().enumerate() {
        assert_eq!(draw.low, 0);
        assert_eq!(draw.high, i64::from(u32::MAX));
        assert_eq!(draw.consumed, 1);
        assert_eq!(
            draw.value as u32,
            rng::full_u32(world.state.world, 7, draw.first_counter)
        );
        if index > 0 {
            assert_eq!(draw.first_counter, draws[index - 1].first_counter + 1);
        }
    }
    assert_eq!(eval.actions[0].chance_counter, draws[0].first_counter);
    assert_eq!(eval.actions[0].action_counter, Some(draws[1].first_counter));
    let start = record
        .events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::Npc {
                    effect: ai::Effect::CombatStarted { .. },
                    ..
                }
            )
        })
        .unwrap();
    let speech = record
        .events
        .iter()
        .position(|e| matches!(e, Event::EventAiSpeech(_)))
        .unwrap();
    let motion = record
        .events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::Npc {
                    effect: ai::Effect::Motion(_),
                    ..
                }
            )
        })
        .unwrap();
    assert!(start < speech && speech < motion);
    assert_eq!(
        world.state.event_ai.as_ref().unwrap().owners[&2].holders[0].timer_ms,
        0
    );
    assert!(world.state.event_ai.as_ref().unwrap().owners[&2].holders[0].enabled);
    for _ in 0..3 {
        step(&mut world, &mut fixture::Authority::default()).unwrap();
        assert!(event_draws(&world).is_empty());
    }
}
#[test]
fn failed_chance_consumes_one_word_success_without_listener_consumes_two() {
    let seed = (0..1000)
        .find(|seed| rng::full_u32([8; 16], *seed, 0) % 100 >= 10)
        .unwrap();
    let mut world = NativeWorld::synthetic_component_test(
        fixture::profile_with_chance(10),
        [8; 16],
        f::player(),
        seed,
    )
    .unwrap();
    entered(&mut world, &mut fixture::Authority::default());
    assert_eq!(event_draws(&world).len(), 1);
    assert_eq!(evaluation(&world).actions[0].broadcast_id, None);
    assert!(world
        .last_tick()
        .unwrap()
        .events
        .iter()
        .all(|e| !matches!(e, Event::EventAiSpeech(_))));
    let mut world = fixture::world(7);
    let mut authority = fixture::Authority {
        outside: true,
        ..Default::default()
    };
    entered(&mut world, &mut authority);
    assert_eq!(event_draws(&world).len(), 2);
    assert!(evaluation(&world).actions[0].broadcast_id.is_some());
    assert!(evaluation(&world).actions[0].speech.is_none());
    assert_eq!(authority.queries.len(), 1);
    NativeWorld::restore_synthetic_component_test(
        world.profile.clone(),
        world.checkpoint().unwrap(),
    )
    .unwrap();
}
#[test]
fn missing_and_late_authority_rollback_rng_holder_history_and_retry() {
    for late in [false, true] {
        let mut world = fixture::world(7);
        step(&mut world, &mut fixture::Authority::default()).unwrap();
        let before = world.checkpoint().unwrap();
        let records = world.records().to_vec();
        let mut clean = world.clone();
        let mut authority = fixture::Authority {
            fail_audience: !late,
            ..Default::default()
        };
        if late {
            authority.base.fail_derived_owner = Some(2);
        }
        assert!(step(&mut world, &mut authority).is_err());
        assert_eq!(world.checkpoint().unwrap(), before);
        assert_eq!(world.records(), records);
        step(&mut world, &mut fixture::Authority::default()).unwrap();
        step(&mut clean, &mut fixture::Authority::default()).unwrap();
        assert_eq!(world.checkpoint().unwrap(), clean.checkpoint().unwrap());
        assert_eq!(world.records(), clean.records());
    }
}
#[test]
fn checkpoint_replay_keeps_semantics_and_never_fires_historical_callbacks() {
    let mut world = fixture::world(7);
    let initial = world.checkpoint().unwrap();
    entered(&mut world, &mut fixture::Authority::default());
    let final_checkpoint = world.checkpoint().unwrap();
    let mut restored =
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), initial).unwrap();
    restored.replay(world.records()).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), final_checkpoint);
    let mut reopened =
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), final_checkpoint)
            .unwrap();
    assert!(reopened.records().is_empty());
    step(&mut reopened, &mut fixture::Authority::default()).unwrap();
    assert!(event_draws(&reopened).is_empty());
    assert!(reopened
        .last_tick()
        .unwrap()
        .events
        .iter()
        .all(|e| !matches!(e, Event::EventAiSpeech(_))));
}
#[test]
fn recomputed_outer_hashes_do_not_admit_changed_semantics_or_dropped_holders() {
    let mut world = fixture::world(7);
    entered(&mut world, &mut fixture::Authority::default());
    let checkpoint = serde_json::to_value(world.checkpoint().unwrap()).unwrap();
    for case in 0..17 {
        let mut value = checkpoint.clone();
        let event = &mut value["state"]["event_ai"];
        match case {
            0 => event["catalog"][0] = 99.into(),
            1 => event["owners"]["2"]["id"]["generation"] = 1.into(),
            2 => event["owners"]["2"]["holders"][0]["enabled"] = false.into(),
            3 => event["owners"]["2"]["holders"][0]["event_id"] = 12.into(),
            4 => event["owners"]["2"]["holders"] = serde_json::json!([]),
            5 => event["owners"]["2"]["next_occurrence"] = 0.into(),
            6 => event["history"] = serde_json::json!([]),
            7 => event["history"][0]["EnterCombat"]["actions"] = serde_json::json!([]),
            8 => event["history"][0]["EnterCombat"]["actions"][0]["event_id"] = 12.into(),
            9 => event["history"][0]["EnterCombat"]["actions"][0]["chance_counter"] = 1.into(),
            10 => event["history"][0]["EnterCombat"]["actions"][0]["broadcast_id"] = 12.into(),
            11 => event["history"][0]["EnterCombat"]["callback"]["npc"]["generation"] = 1.into(),
            12 => {
                event["history"][0]["EnterCombat"]["actions"][0]["speech"]["text"] = "forged".into()
            }
            13 => {
                event["history"][0]["EnterCombat"]["actions"][0]["speech"]["language"] = 99.into()
            }
            14 => event["history"][0]["EnterCombat"]["callback"]["draft"][0] = 99.into(),
            15 => {
                event["history"] = serde_json::json!([]);
                event["owners"]["2"]["next_occurrence"] = 0.into();
            }
            _ => {
                *event = serde_json::to_value(fixture::world(7).state.event_ai.as_ref().unwrap())
                    .unwrap();
            }
        }
        assert!(
            NativeWorld::restore_synthetic_component_test(world.profile.clone(), rehashed(value))
                .is_err(),
            "case {case}"
        );
    }
}
#[test]
fn native33_and_missing_required_event_ai_fields_reject_even_rehashed() {
    let world = fixture::world(7);
    let mut value = serde_json::to_value(world.checkpoint().unwrap()).unwrap();
    value["version"] = 33.into();
    value["state"]["version"] = 33.into();
    let old = rehashed(value);
    assert!(Checkpoint::decode(&old.encode().unwrap()).is_err());
    assert!(NativeWorld::restore_synthetic_component_test(world.profile.clone(), old).is_err());
    let mut value = serde_json::to_value(world.checkpoint().unwrap()).unwrap();
    value["state"].as_object_mut().unwrap().remove("event_ai");
    assert!(serde_json::from_value::<Checkpoint>(value).is_err());
    let mut world = world;
    entered(&mut world, &mut fixture::Authority::default());
    let initial = fixture::world(7).checkpoint().unwrap();
    let mut records = world.records().to_vec();
    records[0].version = 33;
    assert!(decode_records(&encode_records(&records).unwrap()).is_err());
    let mut restored =
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), initial.clone())
            .unwrap();
    assert!(restored.replay(&records).is_err());
    assert_eq!(restored.checkpoint().unwrap(), initial);
}

#[test]
fn scheduled_hostile_spell_draws_periodic_then_chance_then_action_and_repeated_hit_is_silent() {
    let mut profile = fixture::profile();
    profile.npcs[0].ai.mainhand_interval_ms = 1000;
    let mut world =
        NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap();
    let initial = world.checkpoint().unwrap();
    let mut authority = fixture::Authority::default();
    authority.base.visibility = Some(ai::movement::Check::Failed);
    authority.base.base.base.resist_chance = Some(0);
    authority.base.base.base.critical_chance = Some(0);
    for _ in 0..2 {
        step(&mut world, &mut authority).unwrap();
    }
    assert_eq!(world.state.npcs[&2].ai.phase(), ai::Phase::Idle);
    let input = f::cast(&world, 60001);
    world
        .step(world.state.tick + 1, Some(input), &mut authority)
        .unwrap();
    loop {
        assert!(world.state.tick < 100);
        step(&mut world, &mut authority).unwrap();
        if !world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::EventAiEvaluation(_)))
        {
            continue;
        }
        let record = world.last_tick().unwrap();
        let periodic = record
            .draws
            .iter()
            .position(|d| matches!(d.purpose, DrawPurpose::Periodic { slot: 1 }))
            .unwrap();
        let chance = record
            .draws
            .iter()
            .position(|d| matches!(d.purpose, DrawPurpose::EventAiChance { .. }))
            .unwrap();
        let action = record
            .draws
            .iter()
            .position(|d| matches!(d.purpose, DrawPurpose::EventAiAction { .. }))
            .unwrap();
        assert!(periodic < chance && chance < action);
        assert_eq!(
            record.draws[chance].first_counter,
            record.draws[periodic].first_counter + 1
        );
        assert_eq!(
            record.draws[action].first_counter,
            record.draws[chance].first_counter + 1
        );
        assert!(matches!(
            evaluation(&world).callback.kind,
            EventAiCallbackKind::CombatReceipt {
                receipt: ai::Receipt {
                    event: ai::CombatEvent::AttackedBy { target: SOLO_UNIT },
                    ..
                }
            }
        ));
        break;
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), initial).unwrap();
    replay.replay(world.records()).unwrap();
    assert_eq!(world.checkpoint().unwrap(), replay.checkpoint().unwrap());
    let mut world = NativeWorld::restore_synthetic_component_test(
        world.profile.clone(),
        world.checkpoint().unwrap(),
    )
    .unwrap();
    let count = world.state.event_ai.as_ref().unwrap().owners[&2].next_occurrence;
    let input = f::cast(&world, 60001);
    world
        .step(world.state.tick + 1, Some(input), &mut authority)
        .unwrap();
    loop {
        assert!(world.state.tick < 100);
        step(&mut world, &mut authority).unwrap();
        assert!(event_draws(&world).is_empty());
        if world.state.casts.is_empty() {
            break;
        }
    }
    assert_eq!(
        world.state.event_ai.as_ref().unwrap().owners[&2].next_occurrence,
        count
    );
}

struct PlayerSightAfterSpell {
    base: fixture::Authority,
    permit_player: bool,
}
impl Authority for PlayerSightAfterSpell {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if self.permit_player
            && matches!(&request.query, Query::SourceMotion(MotionQuery::VisitorVisibility(s)) if matches!(s.notification_owner, VisitorParticipant::Player(_)))
        {
            return Ok(AuthorityReply {
                request: request.clone(),
                facts: Facts::SourceMotion(MotionFacts::VisitorVisibility(
                    ai::movement::Check::Passed,
                )),
            });
        }
        self.base.resolve(request)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.base.observe_derived(reply)
    }
}
#[test]
fn player_notify_after_spell_readds_preserves_distinct_insertion_identity_and_restores() {
    let mut profile = fixture::profile();
    profile.npcs[0].max_health = 1;
    profile.npcs[0].ai.corpse_decay_ms = 0;
    let mut world =
        NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap();
    let mut authority = PlayerSightAfterSpell {
        base: fixture::Authority::default(),
        permit_player: false,
    };
    authority.base.base.visibility = Some(ai::movement::Check::Failed);
    authority.base.base.base.base.resist_chance = Some(0);
    authority.base.base.base.base.critical_chance = Some(0);
    for _ in 0..2 {
        step(&mut world, &mut authority).unwrap();
    }
    let input = f::cast(&world, 60001);
    world
        .step(world.state.tick + 1, Some(input), &mut authority)
        .unwrap();
    loop {
        step(&mut world, &mut authority).unwrap();
        assert!(world.state.tick < 100);
        let npc = &world.state.npcs[&2];
        if npc.id.generation == 1 && npc.events.can_aggro() {
            break;
        }
    }
    assert_eq!(world.state.npcs[&2].ai.phase(), ai::Phase::Idle);
    assert_eq!(
        world.state.event_ai.as_ref().unwrap().owners[&2].next_occurrence,
        0
    );
    authority.permit_player = true;
    let input = OwnedInput {
        authenticated_owner: world.state.player.owner,
        input: Input {
            world: world.state.world,
            tick: world.state.tick + 1,
            player: world.state.player,
            sequence: world.state.next_sequence,
            command: Command::MovementPose {
                destination: f::point(0.1),
                orientation: Orientation::ZERO,
                movement_flags: m::GroundMovementFlags::STOPPED,
            },
        },
    };
    world
        .step(world.state.tick + 1, Some(input), &mut authority)
        .unwrap();
    for _ in 0..5 {
        if world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::EventAiEvaluation(_)))
        {
            break;
        }
        step(&mut world, &mut authority).unwrap();
    }
    let EventAiCallbackKind::Sight {
        notification_owner: VisitorParticipant::Player(_),
        callback,
    } = &evaluation(&world).callback.kind
    else {
        panic!("player-owned sight callback");
    };
    assert!(callback.due.insertion_id > callback.due.event_id);
    assert_eq!(evaluation(&world).callback.npc.generation, 1);
    NativeWorld::restore_synthetic_component_test(
        world.profile.clone(),
        world.checkpoint().unwrap(),
    )
    .unwrap();
}

#[test]
fn typed_restore_and_checkpoint_budget_precede_nested_callback_replay() {
    let mut world = fixture::world(7);
    entered(&mut world, &mut fixture::Authority::default());
    let mut checkpoint = serde_json::to_value(world.checkpoint().unwrap()).unwrap();
    let mut row = world.state.event_ai.as_ref().unwrap().history[0].clone();
    let EventAiHistory::EnterCombat(e) = &mut row else {
        unreachable!()
    };
    let EventAiCallbackKind::Sight { callback, .. } = &mut e.callback.kind else {
        unreachable!()
    };
    // Individually bounded vectors, but their aggregate replay/hashing work is
    // over the existing ceiling. Invalid identities must not run first.
    callback.queue.detached = vec![callback.queue.detached[0].clone(); 16];
    world.state.event_ai.as_mut().unwrap().history = vec![row; 500];
    assert!(
        world
            .state
            .event_ai
            .as_ref()
            .unwrap()
            .replay_work()
            .unwrap()
            > 65_536
    );
    checkpoint["state"]["event_ai"] = serde_json::to_value(&world.state.event_ai).unwrap();
    let checkpoint: Checkpoint = serde_json::from_value(checkpoint).unwrap();
    assert_eq!(
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), checkpoint).err(),
        Some(NativeError::TooLarge)
    );
    assert_eq!(world.checkpoint().err(), Some(NativeError::TooLarge));
}

#[test]
fn respawn_history_cannot_borrow_the_other_owner_or_skip_generations() {
    let mut profile = fixture::profile();
    profile.npcs[0].max_health = 1;
    profile.npcs[0].ai.corpse_decay_ms = 0;
    let mut world =
        NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap();
    let mut authority = fixture::Authority::default();
    authority.base.visibility = Some(ai::movement::Check::Failed);
    authority.base.base.base.resist_chance = Some(0);
    authority.base.base.base.critical_chance = Some(0);
    let input = f::cast(&world, 60001);
    world.step(1, Some(input), &mut authority).unwrap();
    while world.state.npcs[&2].id.generation == 0 {
        assert!(world.state.tick < 100);
        step(&mut world, &mut authority).unwrap();
    }
    let value = serde_json::to_value(world.checkpoint().unwrap()).unwrap();
    let index = world
        .state
        .event_ai
        .as_ref()
        .unwrap()
        .history
        .iter()
        .position(|h| matches!(h, EventAiHistory::Respawn { .. }))
        .unwrap();
    for case in 0..4 {
        let mut changed = value.clone();
        let row = &mut changed["state"]["event_ai"]["history"][index]["Respawn"];
        match case {
            0 => row["previous"]["spawn"] = 3.into(),
            1 => row["replacement"]["spawn"] = 3.into(),
            2 => row["replacement"]["generation"] = 2.into(),
            3 => row["previous"]["generation"] = 1.into(),
            _ => unreachable!(),
        }
        assert!(
            NativeWorld::restore_synthetic_component_test(world.profile.clone(), rehashed(changed))
                .is_err(),
            "respawn tamper {case}"
        );
    }
}

#[test]
fn late_journal_limit_and_corrupt_rng_counter_publish_no_partial_callback() {
    for corrupt_counter in [false, true] {
        let mut world = fixture::world(7);
        step(&mut world, &mut fixture::Authority::default()).unwrap();
        let clean = world.clone();
        if corrupt_counter {
            // Native38 rejects a counter outside its checked genesis/tick budget
            // before callbacks; the sampler separately tests atomic overflow.
            world.state.rng_counter = u64::MAX - 1;
        } else {
            // Record size is checked only after the staged callback finishes.
            world.journal_bytes = MAX_BYTES;
        }
        let state = world.snapshot_of(&world.state);
        let records = world.records().to_vec();
        let result = step(&mut world, &mut fixture::Authority::default());
        assert_eq!(
            result,
            Err(if corrupt_counter {
                NativeError::InvalidCheckpoint
            } else {
                NativeError::JournalFull
            })
        );
        assert_eq!(world.snapshot_of(&world.state), state);
        assert_eq!(world.records(), records);
        world.state.rng_counter = clean.state.rng_counter;
        world.journal_bytes = clean.journal_bytes;
        let mut clean = clean;
        step(&mut world, &mut fixture::Authority::default()).unwrap();
        step(&mut clean, &mut fixture::Authority::default()).unwrap();
        assert_eq!(world.checkpoint().unwrap(), clean.checkpoint().unwrap());
        assert_eq!(world.records(), clean.records());
    }
}

#[test]
fn entry_then_guid_holders_are_contiguous_and_rehashed_counter_gaps_reject() {
    let mut world = NativeWorld::synthetic_component_test(
        fixture::profile_with_entry_and_guid(),
        [8; 16],
        f::player(),
        7,
    )
    .unwrap();
    entered(&mut world, &mut fixture::Authority::default());
    let e = evaluation(&world);
    assert_eq!(
        e.actions.iter().map(|a| a.event_id).collect::<Vec<_>>(),
        vec![fixture::EVENT, fixture::EVENT + 1]
    );
    let draws = event_draws(&world);
    assert_eq!(draws.len(), 4);
    for pair in draws.windows(2) {
        assert_eq!(pair[1].first_counter, pair[0].first_counter + 1);
    }
    let checkpoint = world.checkpoint().unwrap();
    NativeWorld::restore_synthetic_component_test(world.profile.clone(), checkpoint.clone())
        .unwrap();
    let selected = world.profile.event_ai().unwrap();
    let event = selected.catalog().events(2).unwrap()[1];
    let history = &mut world.state.event_ai.as_mut().unwrap().history;
    let EventAiHistory::EnterCombat(e) = &mut history[0] else {
        unreachable!()
    };
    let action = &mut e.actions[1];
    action.chance_counter += 1;
    let counter = action.action_counter.as_mut().unwrap();
    *counter += 1;
    // Recompute the selection, speech and independent anchor too, so rejection
    // specifically needs the synchronous within-callback counter relationship.
    let mut holder = source::AggroTextHolder::new(event);
    holder.begin_enter_combat();
    holder
        .consume_chance(rng::full_u32(
            world.state.world,
            world.state.seed,
            action.chance_counter,
        ))
        .unwrap();
    let selection = holder
        .consume_action(rng::full_u32(world.state.world, world.state.seed, *counter))
        .unwrap();
    action.broadcast_id = Some(selection.text_id);
    action.text_slot = Some(selection.text_slot);
    action.speech =
        Some(EventAiState::speech(selected, &e.callback, e.occurrence, selection, None).unwrap());
    let mut anchor = EventAiCallbackAnchor::default();
    anchor.append(e).unwrap();
    world.state.npcs.get_mut(&2).unwrap().events.event_ai_anchor = Some(anchor);
    world.state.rng_counter += 1;
    let mut value = serde_json::to_value(checkpoint).unwrap();
    value["state"] = serde_json::to_value(world.snapshot_of(&world.state)).unwrap();
    assert!(
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), rehashed(value))
            .is_err()
    );
}
