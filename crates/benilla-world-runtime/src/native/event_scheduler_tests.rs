// SPDX-License-Identifier: GPL-3.0-or-later
use super::{synthetic_fixture as f, *};
use benilla_sim::event_processor::EventKind;
fn step(w: &mut NativeWorld) {
    w.step(
        w.state.tick + 1,
        None,
        &mut f::SyntheticAuthority::default(),
    )
    .unwrap();
}
fn open(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
fn tamper(
    w: &NativeWorld,
    change: impl FnOnce(&mut serde_json::Value),
) -> Result<NativeWorld, NativeError> {
    let mut cp = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    change(&mut cp["state"]);
    let state: super::persistence::StateCheckpoint =
        serde_json::from_value(cp["state"].clone()).unwrap();
    let digest = crate::digest(b"benilla-native-state-v82\0", &state);
    let tip: crate::Digest = serde_json::from_value(cp["history_tip"].clone()).unwrap();
    cp["digest"] = serde_json::to_value(digest).unwrap();
    cp["checksum"] = serde_json::to_value(crate::digest(
        b"benilla-native-checkpoint-v82\0",
        &(&state, digest, tip),
    ))
    .unwrap();
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        serde_json::from_value(cp).unwrap(),
    )
}
#[test]
fn queue_uses_unit_clock_and_retains_source_delay_and_cleanup_across_reopen() {
    let mut p = f::profile();
    p.clock_origin_ms = 123_000;
    p.npcs[0].ai.clock_origin_ms = 123_000;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let genesis = w.clone();
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    assert_eq!(w.state.player_events.clock_ms(), 100);
    assert_eq!(w.state.now_ms, 123_100);
    let event = w.state.player_events.queue_snapshot().queued[0].clone();
    assert_eq!(event.add_time_ms, 0);
    assert_eq!(event.exec_time_ms, 101);
    assert_eq!(event.kind, EventKind::SpellUpdate { cast_id: 1 });
    step(&mut w);
    step(&mut w);
    let released = w.state.casts[&1]
        .release_binding
        .as_ref()
        .unwrap()
        .captured_ms;
    let spell::CastState::Traveling { impact_at_ms } = w.state.casts[&1].execution.state() else {
        panic!()
    };
    let mut reopened = open(&w);
    while w.state.now_ms < impact_at_ms {
        let prior = w.state.now_ms;
        step(&mut w);
        step(&mut reopened);
        assert_eq!(w.state_digest(), reopened.state_digest());
        if w.state.now_ms < impact_at_ms {
            assert_eq!(w.state.casts[&1].execution.last_update_ms(), released);
        }
        assert!(w.state.now_ms > prior);
    }
    assert!(w.state.casts.is_empty());
    let lifetime = w.state.player_events.spell_lifetime(1).unwrap();
    assert_eq!(
        lifetime.finished,
        Some((w.state.now_ms, FinishedSpellReason::DelayedFinished))
    );
    assert_eq!(
        w.state.player_events.queue_snapshot().queued[0].exec_time_ms,
        w.state.player_events.clock_ms() + 1
    );
    assert_eq!(w.state.player_events.current_spell(), None);
    let restored = open(&w);
    step(&mut w);
    assert!(w.state.player_events.queue_snapshot().queued.is_empty());
    let mut restored = restored;
    step(&mut restored);
    assert_eq!(w.state_digest(), restored.state_digest());
    let mut replay = genesis;
    replay.replay(w.records()).unwrap();
    assert_eq!(w.state_digest(), replay.state_digest());
}
#[test]
fn rehashed_queue_and_lifetime_attacks_fail_closed() {
    let mut w = f::world();
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    for attack in 0..8 {
        assert!(
            tamper(&w, |s| match attack {
                0 => s["player_events"]["queue"]["clock_ms"] = 0.into(),
                1 => {
                    s["player_events"]["queue"]["queued"][0]["exec_time_ms"] = 102.into();
                    s["player_events"]["queue"]["queued"][0]["queue"]["deadline_ms"] = 102.into();
                }
                2 => s["player_events"]["current_spell"] = serde_json::Value::Null,
                3 => s["player_events"]["spells"]["1"]["last_callback_ms"] = 0.into(),
                4 => s["player_events"]["spells"]
                    .as_object_mut()
                    .unwrap()
                    .clear(),
                5 => s["player_events"]["queue"]["queued"]
                    .as_array_mut()
                    .unwrap()
                    .clear(),
                6 =>
                    s["player_events"]["player"]["character"] =
                        serde_json::to_value([9u8; 16]).unwrap(),
                _ => s["player_events"]["spells"]["1"]["delay_start_ms"] = 1.into(),
            })
            .is_err(),
            "attack {attack}"
        );
    }
    step(&mut w);
    step(&mut w);
    step(&mut w);
    assert!(
        tamper(&w, |s| s["casts"]["1"]["execution"]["last_update_ms"] =
            s["now_ms"].clone())
        .is_err()
    );
    while !w.state.casts.is_empty() {
        step(&mut w);
    }
    for attack in 0..3 {
        assert!(tamper(&w, |s| match attack {
            0 => s["player_events"]["current_spell"] = 1.into(),
            1 => s["player_events"]["spells"]["1"]["finished"][0] = 0.into(),
            _ => s["player_events"]["spells"]["1"]["finished"][1] = "PrepareFailed".into(),
        })
        .is_err());
    }
}
#[test]
fn cancellation_and_permanent_teardown_have_single_source_destructor_effect() {
    let mut w = f::world();
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    let removed_event = w.state.player_events.spell_lifetime(1).unwrap().event_id;
    let effects = w.clone().teardown().unwrap();
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(
                e,
                Event::Spell {
                    event: spell::SpellEvent::Failed(spell::CastFailure::Interrupted),
                    ..
                }
            ))
            .count(),
        1
    );
    let command = OwnedInput {
        authenticated_owner: f::player().owner,
        input: Input {
            world: w.state.world,
            player: w.state.player,
            tick: 2,
            sequence: 1,
            command: Command::CancelCast {
                spell_id: 60001,
                target: CancelCastTarget::Instance(1),
            },
        },
    };
    w.step(2, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    assert!(w.state.casts.is_empty());
    assert!(w.state.player_events.queue.event(removed_event).is_none());
    let command = f::cast(&w, 60001);
    w.step(3, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    assert!(w.state.player_events.spell_lifetime(2).unwrap().event_id > removed_event);
    assert_eq!(open(&w).state_digest(), w.state_digest());
}
#[test]
fn failed_prepare_still_allocates_then_deletes_source_event_without_current_reference() {
    let mut p = f::profile();
    p.player.current_mana = 0;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    assert!(w.state.casts.is_empty());
    let snapshot = w.state.player_events.queue_snapshot();
    assert_eq!(snapshot.next_event_id, 2);
    assert!(snapshot.queued.is_empty());
    assert_eq!(w.state.next_cast_id, 2);
    assert_eq!(open(&w).state_digest(), w.state_digest());
}

#[test]
fn equal_deadline_notify_and_spell_callbacks_follow_actual_insertion_order() {
    use super::synthetic_motion_fixture as m;
    for notify_first in [false, true] {
        let mut p = m::profile();
        p.source_motion.as_mut().unwrap().relocation_aggro_delay_ms =
            if notify_first { 101 } else { 1 };
        let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
        let genesis = w.clone();
        let mut a = m::SyntheticMotionAuthority {
            visits: Some(false),
            ..Default::default()
        };
        let command = f::cast(&w, 60001);
        w.step(1, Some(command), &mut a).unwrap();
        let notify_tick = if notify_first { 2 } else { 3 };
        for tick in 2..=3 {
            a.queries.clear();
            let input = (tick == notify_tick).then(|| OwnedInput {
                authenticated_owner: f::player().owner,
                input: Input {
                    world: w.state.world,
                    player: w.state.player,
                    tick,
                    sequence: w.state.next_sequence,
                    command: Command::MovementPose {
                        destination: w.state.position,
                        orientation: Orientation::new(0.5).unwrap(),
                        movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
                    },
                },
            });
            w.step(tick, input, &mut a).unwrap();
        }
        let order = a
            .queries
            .iter()
            .filter_map(|r| match &r.query {
                Query::Spell {
                    phase: CastPhase::Release,
                    ..
                } => Some("spell"),
                Query::SourceMotion(MotionQuery::Visitor(v)) if v.owner == VisitorOwner::Player => {
                    Some("notify")
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            if notify_first {
                vec!["notify", "spell"]
            } else {
                vec!["spell", "notify"]
            }
        );
        assert_eq!(open(&w).state_digest(), w.state_digest());
        let mut replay = genesis;
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.state_digest(), w.state_digest());
    }
}
#[test]
fn zero_travel_readd_executes_in_same_update_and_retains_finished_cleanup() {
    let mut p = f::profile();
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        panic!()
    };
    spec.projectile_speed = f::float(1_000_000.0);
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    step(&mut w);
    step(&mut w);
    let spells = w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Spell { event, .. } => Some(*event),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(spells
        .iter()
        .any(|e| matches!(e, spell::SpellEvent::Launched { impact_at_ms: 300 })));
    assert!(spells
        .iter()
        .any(|e| matches!(e, spell::SpellEvent::Completed)));
    assert!(w.state.casts.is_empty());
    assert_eq!(
        w.state.player_events.queue_snapshot().queued[0].exec_time_ms,
        301
    );
    assert_eq!(open(&w).state_digest(), w.state_digest());
    step(&mut w);
    assert!(w.state.player_events.queue_snapshot().queued.is_empty());
}

#[test]
fn forced_notify_replacement_uses_fresh_identity_and_rejects_stale_handle() {
    let mut queue = PlayerEvents::new([8; 16], f::player());
    queue.schedule_notify(10, false).unwrap();
    let old = queue.notify.unwrap();
    queue.schedule_notify(1, false).unwrap();
    assert_eq!(queue.notify, Some(old));
    queue.schedule_notify(10, true).unwrap();
    let new = queue.notify.unwrap();
    assert!(new > old);
    assert!(queue.queue.event(old).is_none());
    assert!(!queue.queue.modify_event_time(old, 0).unwrap());
    assert_eq!(queue.queue_snapshot().queued.len(), 1);
    assert_eq!(queue.queue_snapshot().queued[0].exec_time_ms, 10);
}

#[test]
fn current_slot_cannot_rebind_an_older_flight_or_disappear_while_newest_is_live() {
    let mut w = f::world();
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    step(&mut w);
    step(&mut w);
    let command = f::cast(&w, 60001);
    w.step(4, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    step(&mut w);
    step(&mut w);
    assert_eq!(w.state.casts.len(), 2);
    assert!(tamper(&w, |s| s["player_events"]["current_spell"] = 1.into()).is_err());
    assert!(tamper(&w, |s| s["player_events"]["current_spell"] =
        serde_json::Value::Null)
    .is_err());
    assert!(tamper(&w, |s| {
        s["player_events"]["current_spell"] = 1.into();
        s["player_events"]["last_current_spell"] = 1.into();
    })
    .is_err());
    assert_eq!(open(&w).state_digest(), w.state_digest());
}
#[test]
fn late_world_failure_rolls_back_queue_rng_damage_outputs_and_finished_lifetime() {
    struct FailNpc(f::SyntheticAuthority);
    impl Authority for FailNpc {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if matches!(request.query, Query::Npc { .. }) {
                return Err(NativeError::AuthorityUnavailable("after event callback"));
            }
            self.0.resolve(request)
        }
    }
    let mut p = f::profile();
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        panic!()
    };
    spec.projectile_speed = f::float(1_000_000.0);
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    step(&mut w);
    let before = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    assert!(w.step(3, None, &mut FailNpc(Default::default())).is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(w.records(), records);
    assert!(w
        .state
        .player_events
        .spell_lifetime(1)
        .unwrap()
        .finished
        .is_none());
    step(&mut w);
    assert!(w.state.npcs[&2].health < 1000);
    assert!(w.state.rng_counter > 0);
    assert_eq!(
        w.last_tick()
            .unwrap()
            .events
            .iter()
            .filter(|e| matches!(
                e,
                Event::Spell {
                    event: spell::SpellEvent::Completed,
                    ..
                }
            ))
            .count(),
        1
    );
    assert_eq!(open(&w).state_digest(), w.state_digest());
}

#[test]
fn delayed_callback_cancels_old_target_generation_after_respawn_and_replays() {
    let mut p = f::profile();
    p.npcs[0].max_health = 1;
    p.npcs[0].ai.corpse_decay_ms = 0;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        panic!()
    };
    spec.projectile_speed = f::float(20.0);
    let mut slow = spec.clone();
    slow.spell_id = 60003;
    slow.projectile_speed = f::float(1.0);
    p.abilities.push(Ability {
        id: 60003,
        school: 2,
        admission: AbilityAdmission::Available(slow),
    });
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let genesis = w.clone();
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    step(&mut w);
    step(&mut w);
    let command = f::cast(&w, 60003);
    w.step(4, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    while w.state.npcs[&2].id.generation == 0 {
        assert!(w.state.tick < 50);
        step(&mut w);
    }
    assert_eq!(w.state.casts[&2].target.generation, 0);
    let spell::CastState::Traveling { impact_at_ms } = w.state.casts[&2].execution.state() else {
        panic!()
    };
    let mut reopened = open(&w);
    while w.state.now_ms < impact_at_ms {
        step(&mut w);
        step(&mut reopened);
        assert_eq!(w.state_digest(), reopened.state_digest());
        assert_eq!(w.state.npcs[&2].health, 1);
    }
    assert!(w.state.casts.is_empty());
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(e,
        Event::Spell {cast_id:2,target,event:spell::SpellEvent::Failed(spell::CastFailure::TargetLost),..} if target.generation==0)));
    assert_eq!(open(&w).state_digest(), w.state_digest());
    let mut replay = genesis;
    replay.replay(w.records()).unwrap();
    assert_eq!(w.state_digest(), replay.state_digest());
}

#[test]
fn rehashed_equal_deadline_swap_cannot_reverse_source_enqueue_chronology() {
    use super::synthetic_motion_fixture as m;
    let mut p = m::profile();
    p.source_motion.as_mut().unwrap().relocation_aggro_delay_ms = 101;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let mut a = m::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    };
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    let command = OwnedInput {
        authenticated_owner: f::player().owner,
        input: Input {
            world: w.state.world,
            player: w.state.player,
            tick: 2,
            sequence: 1,
            command: Command::MovementPose {
                destination: w.state.position,
                orientation: Orientation::new(0.5).unwrap(),
                movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
            },
        },
    };
    w.step(2, Some(command), &mut a).unwrap();
    assert_eq!(open(&w).state_digest(), w.state_digest());
    assert!(tamper(&w, |s| {
        let queue = s["player_events"]["queue"]["queued"]
            .as_array_mut()
            .unwrap();
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0]["exec_time_ms"], queue[1]["exec_time_ms"]);
        let old = queue[0]["queue"]["insertion_id"].clone();
        queue[0]["queue"]["insertion_id"] = queue[1]["queue"]["insertion_id"].clone();
        queue[1]["queue"]["insertion_id"] = old;
        queue.swap(0, 1);
    })
    .is_err());
}
#[test]
fn deleted_event_allocator_and_legacy_terminal_clock_cannot_be_rehashed_backwards() {
    let mut p = f::profile();
    p.player.current_mana = 0;
    let mut failed = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let command = f::cast(&failed, 60001);
    failed
        .step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    assert!(tamper(&failed, |s| {
        s["player_events"]["queue"]["next_event_id"] = 1.into();
        s["player_events"]["queue"]["next_insertion_id"] = 1.into();
    })
    .is_err());
    let mut w = f::world();
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    step(&mut w);
    step(&mut w);
    // Authored legacy no-death-host state; the existing fallback cancels flight.
    w.state.health = 0;
    step(&mut w);
    assert_eq!(
        w.state.player_events.spell_lifetime(1).unwrap().finished,
        Some((400, FinishedSpellReason::Interrupted))
    );
    assert_eq!(open(&w).state_digest(), w.state_digest());
    assert!(tamper(&w, |s| {
        s["player_events"]["spells"]["1"]["last_callback_ms"] = 0.into();
        s["player_events"]["spells"]["1"]["finished"][0] = 0.into();
    })
    .is_err());
}

#[test]
fn same_frame_delayed_cleanup_cannot_swap_both_predecessor_and_current_order() {
    let mut p = f::profile();
    p.npcs[0].ai.spawn = f::point(20.0);
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        panic!()
    };
    spec.projectile_speed = f::float(20.0);
    let mut second = spec.clone();
    second.spell_id = 60003;
    second.cast_ms = 100;
    second.projectile_speed = f::float(25.0);
    p.abilities.push(Ability {
        id: 60003,
        school: 2,
        admission: AbilityAdmission::Available(second),
    });
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let command = f::cast(&w, 60001);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    step(&mut w);
    step(&mut w);
    let command = f::cast(&w, 60003);
    w.step(4, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    while w.state.tick < 13 {
        step(&mut w);
    }
    assert_eq!(w.state.player_events.queue_snapshot().queued.len(), 2);
    assert_eq!(
        w.state
            .player_events
            .spell_lifetime(1)
            .unwrap()
            .last_callback_ms,
        Some(1300)
    );
    assert_eq!(
        w.state
            .player_events
            .spell_lifetime(2)
            .unwrap()
            .last_callback_ms,
        Some(1300)
    );
    assert_eq!(open(&w).state_digest(), w.state_digest());
    assert!(tamper(&w, |s| {
        let a = s["player_events"]["spells"]["1"]["last_dispatch"]["insertion_id"].clone();
        s["player_events"]["spells"]["1"]["last_dispatch"]["insertion_id"] =
            s["player_events"]["spells"]["2"]["last_dispatch"]["insertion_id"].clone();
        s["player_events"]["spells"]["2"]["last_dispatch"]["insertion_id"] = a;
        let q = s["player_events"]["queue"]["queued"]
            .as_array_mut()
            .unwrap();
        let a = q[0]["queue"]["insertion_id"].clone();
        q[0]["queue"]["insertion_id"] = q[1]["queue"]["insertion_id"].clone();
        q[1]["queue"]["insertion_id"] = a;
        q.swap(0, 1);
    })
    .is_err());
    step(&mut w);
    assert!(w.state.player_events.queue_snapshot().queued.is_empty());
}

#[test]
fn early_command_boundary_rejections_do_not_claim_prepared_spell_lifetimes() {
    let mut w = f::world();
    let command = f::cast(&w, 60002);
    w.step(1, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    assert_eq!(w.state.next_cast_id, 1);
    assert_eq!(w.state.player_events.queue_snapshot().next_event_id, 1);
    let command = f::cast(&w, 60001);
    w.step(2, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    let allocated = w.state.player_events.queue_snapshot().next_event_id;
    let command = f::cast(&w, 60001);
    w.step(3, Some(command), &mut f::SyntheticAuthority::default())
        .unwrap();
    assert_eq!(
        w.state.player_events.queue_snapshot().next_event_id,
        allocated
    );
    assert_eq!(w.state.next_cast_id, 2);
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .contains(&Event::Rejected(Rejection::AlreadyCasting)));
}

#[test]
fn source_notify_allocator_and_input_clock_cannot_regress_across_reopen() {
    use super::synthetic_motion_fixture as m;
    let mut p = m::profile();
    p.source_motion.as_mut().unwrap().relocation_aggro_delay_ms = 201;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let mut a = m::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    };
    w.step(1, None, &mut a).unwrap();
    assert!(tamper(&w, |s| {
        s["player_events"]["queue"]["next_event_id"] = 1.into();
        s["player_events"]["queue"]["next_insertion_id"] = 1.into();
    })
    .is_err());
    let command = OwnedInput {
        authenticated_owner: f::player().owner,
        input: Input {
            world: w.state.world,
            player: w.state.player,
            tick: 2,
            sequence: 0,
            command: Command::MovementPose {
                destination: w.state.position,
                orientation: Orientation::new(0.5).unwrap(),
                movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
            },
        },
    };
    w.step(2, Some(command), &mut a).unwrap();
    assert_eq!(open(&w).state_digest(), w.state_digest());
    assert!(tamper(&w, |s| {
        let e = &mut s["player_events"]["queue"]["queued"][0];
        assert_eq!(e["add_time_ms"], 100);
        assert_eq!(e["exec_time_ms"], 301);
        e["add_time_ms"] = 99.into();
        e["exec_time_ms"] = 300.into();
        e["queue"]["deadline_ms"] = 300.into();
    })
    .is_err());
}

#[test]
fn callback_predecessors_and_retained_cleanup_reopen_at_fine_and_coarse_ticks() {
    for tick_ms in [1, 7, 100, 1000] {
        let mut p = f::profile();
        p.tick_ms = tick_ms;
        let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
            panic!()
        };
        spec.cast_ms = 5;
        spec.effects[1] = spell::MageEffect::Empty;
        spec.projectile_speed = f::float(1111.0);
        let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
        let genesis = w.clone();
        let command = f::cast(&w, 60001);
        w.step(1, Some(command), &mut f::SyntheticAuthority::default())
            .unwrap();
        let mut reopened = open(&w);
        let mut seen_finished = false;
        while !w.state.player_events.queue_snapshot().queued.is_empty() {
            assert!(w.state.tick < 50);
            step(&mut w);
            step(&mut reopened);
            assert_eq!(
                w.state_digest(),
                reopened.state_digest(),
                "tick_ms {tick_ms}"
            );
            seen_finished |= w
                .state
                .player_events
                .spell_lifetime(1)
                .is_some_and(|l| l.finished.is_some());
            reopened = open(&w);
        }
        assert!(seen_finished);
        let mut replay = genesis;
        replay.replay(w.records()).unwrap();
        assert_eq!(w.state_digest(), replay.state_digest());
    }
}
#[test]
fn input_and_callback_budgets_bound_rehashed_allocator_watermarks() {
    use super::synthetic_motion_fixture as m;
    let mut w = m::world();
    w.step(1, None, &mut m::SyntheticMotionAuthority::default())
        .unwrap();
    for which in ["next_event_id", "next_insertion_id"] {
        assert!(tamper(&w, |s| s["player_events"]["queue"][which] = u64::MAX.into()).is_err());
    }
}
