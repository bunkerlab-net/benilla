// SPDX-License-Identifier: GPL-3.0-or-later
use super::{synthetic_fixture as f, synthetic_motion_fixture as fixture, *};
use benilla_sim::event_processor::{CallbackKind, EventKind};

fn step(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state().tick + 1, None, a).unwrap();
}
fn reopen(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
        .unwrap()
}
fn altered(w: &NativeWorld, change: impl FnOnce(&mut serde_json::Value)) -> Checkpoint {
    let mut checkpoint = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&checkpoint.state).unwrap();
    change(&mut state["npcs"]["2"]["events"]);
    checkpoint.state = serde_json::from_value(state).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v69\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v69\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    checkpoint
}

#[test]
fn npc_queue_initial_zero_deadline_and_nonzero_map_origin_are_source_clocks() {
    let mut p = fixture::profile();
    p.clock_origin_ms = 5000;
    p.npcs[0].ai.clock_origin_ms = 5000;
    p.source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = 0;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 5).unwrap();
    let initial = w.clone();
    let initial_event = w.state().npcs[&2].events.notify_handle().unwrap();
    assert_eq!(
        w.state().npcs[&2]
            .events
            .event(initial_event)
            .unwrap()
            .exec_time_ms,
        0
    );
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
    let mut a = fixture::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    };
    step(&mut w, &mut a);
    assert_eq!(w.state().npcs[&2].events.clock_ms(), 100);
    assert_eq!(w.state().now_ms, 5100);
    assert!(w.state().npcs[&2].events.event(initial_event).is_none());
    let request = a
        .queries
        .iter()
        .find_map(|request| match &request.query {
            Query::SourceMotion(MotionQuery::Visitor(v))
                if matches!(v.owner, VisitorOwner::Creature(_)) =>
            {
                Some(v)
            }
            _ => None,
        })
        .unwrap();
    assert!(request.creature_can_aggro);
    assert_eq!(request.callback.due.callback, CallbackKind::Execute);
    assert_eq!(request.callback.due.now_ms, 100);
    assert_eq!(request.callback.due.diff_ms, Some(100));
    assert_eq!(request.callback.due.scheduled_ms, 0);
    assert_eq!(request.callback.queue.detached.len(), 1);
    assert_eq!(
        request.callback.queue.detached[0].event_id,
        initial_event.event_id
    );
    assert!(request.callback.queue.queued.is_empty());
    assert_eq!(request.callback.queue.clock_ms, request.event_clock_ms);
    let mut replay = initial;
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}

#[test]
fn npc_notify_source_visitor_precedes_chase_route_and_retains_handle_until_done() {
    let mut w = fixture::world();
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    let handle = w.state().npcs[&2].events.notify_handle().unwrap();
    step(&mut w, &mut a);
    assert!(w.state().npcs[&2].events.event(handle).is_none());
    assert!(w.state().npcs[&2].events.can_aggro());
    let record = w.last_tick().unwrap();
    let requests = record
        .replies
        .iter()
        .map(|r| &r.request.query)
        .collect::<Vec<_>>();
    let visitor = requests.iter().position(|q| matches!(q,
        Query::SourceMotion(MotionQuery::Visitor(v)) if matches!(v.owner, VisitorOwner::Creature(_)))).unwrap();
    let route = requests
        .iter()
        .position(|q| {
            matches!(
                q,
                Query::SourceMotion(MotionQuery::Spatial(
                    benilla_sim::creature_motion::Query::Route(_)
                ))
            )
        })
        .unwrap();
    assert!(visitor < route);
    // The visitor-triggered chase does not replace the still-set pointer. A later
    // Unit movement relocation may schedule one fresh notification after Execute.
    assert!(w.state().npcs[&2].events.queue_snapshot().next_event_id <= 3);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}

#[test]
fn npc_notify_force_replacement_combat_abort_and_owner_handles_are_distinct() {
    let mut events = fixture::world().state().npcs[&2].events.clone();
    let first = events.notify_handle().unwrap();
    events.schedule_notify(0, false).unwrap();
    assert_eq!(events.notify_handle(), Some(first));
    events.schedule_notify(200, true).unwrap();
    let second = events.notify_handle().unwrap();
    assert_eq!(second.event_id, first.event_id + 1);
    assert!(events.event(first).is_none());
    assert!(events.event(second).is_some());
    events.entered_combat().unwrap();
    assert!(events.can_aggro());
    assert!(events.notify_handle().is_none());
    assert!(events.event(second).is_none());
    events.schedule_notify(100, false).unwrap();
    let third = events.notify_handle().unwrap();
    events.entered_combat().unwrap();
    assert_eq!(events.notify_handle(), Some(third)); // already-aggro relocation survives
    events.respawned(200).unwrap();
    events
        .rebind_respawn(crate::NpcId {
            generation: 1,
            ..third.npc
        })
        .unwrap();
    let new = events.notify_handle().unwrap();
    assert!(!events.can_aggro());
    assert!(events.event(third).is_none());
    assert!(events
        .event(NpcEventHandle {
            npc: third.npc,
            ..new
        })
        .is_none());
    assert!(events
        .event(NpcEventHandle {
            world: [9; 16],
            ..new
        })
        .is_none());
    assert!(events.event(new).is_some());
}

#[test]
fn npc_queue_survives_corpse_dead_respawn_replay_and_every_tick_reopen() {
    let mut p = fixture::profile();
    p.npcs[0].ai.corpse_decay_ms = 300;
    p.npcs[0].ai.respawn_delay_seconds = 2;
    p.source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = 500;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 5).unwrap();
    // Authored externally staged death exercises the admitted pre-Unit death path.
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    let initial = w.clone();
    let original = w.state().npcs[&2].events.notify_handle().unwrap();
    let mut a = fixture::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    };
    let mut frozen = None;
    let mut dead_handle = None;
    while w.state().npcs[&2].id.generation == 0 {
        assert!(w.state().tick < 30);
        let before_phase = w.state().npcs[&2].ai.phase();
        let before_clock = w.state().npcs[&2].events.clock_ms();
        step(&mut w, &mut a);
        let npc = &w.state().npcs[&2];
        if before_phase == ai::Phase::Despawned {
            assert_eq!(npc.events.clock_ms(), before_clock);
        } else {
            assert_eq!(npc.events.clock_ms(), before_clock + 100);
        }
        if npc.ai.phase() == ai::Phase::Despawned {
            let frozen = frozen.get_or_insert(npc.events.clock_ms());
            assert_eq!(npc.events.clock_ms(), *frozen);
            let handle = dead_handle.get_or_insert(npc.events.notify_handle().unwrap());
            assert_eq!(npc.events.notify_handle(), Some(*handle));
        }
        assert_eq!(reopen(&w).state_digest(), w.state_digest());
    }
    let npc = &w.state().npcs[&2];
    assert_eq!(npc.events.clock_ms(), frozen.unwrap());
    assert!(!npc.events.can_aggro());
    assert!(npc.events.event(original).is_none());
    assert!(npc.events.event(dead_handle.unwrap()).is_none());
    let notify = npc.events.notify_handle().unwrap();
    assert_eq!(notify.npc.generation, 1);
    assert!(notify.event_id > original.event_id);
    assert_eq!(
        npc.events.event(notify).unwrap().exec_time_ms,
        npc.events.clock_ms() + 500
    );
    let mut replay = initial;
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    for _ in 0..5 {
        step(&mut w, &mut a);
    }
    assert!(w.state().npcs[&2].events.can_aggro());
    assert!(w.state().npcs[&2].events.notify_handle().is_none());
}

#[test]
fn npc_visitor_failure_rolls_back_entire_frame_and_retry_replays_once() {
    struct FailCreature {
        inner: fixture::SyntheticMotionAuthority,
    }
    impl Authority for FailCreature {
        fn resolve(&mut self, r: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if matches!(&r.query, Query::SourceMotion(MotionQuery::Visitor(v)) if matches!(v.owner, VisitorOwner::Creature(_)))
            {
                return Err(NativeError::AuthorityUnavailable(
                    "authored NPC visitor failure",
                ));
            }
            self.inner.resolve(r)
        }
    }
    let mut w = fixture::world();
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    let before = w.clone();
    assert!(w
        .step(
            2,
            None,
            &mut FailCreature {
                inner: Default::default()
            }
        )
        .is_err());
    assert_eq!(w.checkpoint().unwrap(), before.checkpoint().unwrap());
    assert_eq!(w.records(), before.records());
    step(&mut w, &mut a);
    let mut expected = before;
    step(
        &mut expected,
        &mut fixture::SyntheticMotionAuthority::default(),
    );
    assert_eq!(w.state_digest(), expected.state_digest());
    assert_eq!(w.last_tick(), expected.last_tick());
}

#[test]
fn npc_restore_rejects_rehashed_foreign_owners_unowned_events_and_impossible_clocks() {
    let mut w = fixture::world();
    step(
        &mut w,
        &mut fixture::SyntheticMotionAuthority {
            visits: Some(false),
            ..Default::default()
        },
    );
    for kind in 0..17 {
        let forged = altered(&w, |e| match kind {
            0 => e["world"] = serde_json::to_value([9; 16]).unwrap(),
            1 => e["npc"]["generation"] = 1.into(),
            2 => e["npc"]["spawn"] = 3.into(),
            3 => e["queue"]["clock_ms"] = 0.into(),
            4 => e["skipped_updates"] = 1.into(),
            5 => e["queue"]["aborting"] = true.into(),
            6 => e["queue"]["queued"][0]["to_abort"] = true.into(),
            7 => e["queue"]["queued"][0]["kind"] = serde_json::json!({"SpellUpdate":{"cast_id":1}}),
            8 => e["notify"] = 2.into(),
            9 => e["queue"]["next_event_id"] = 3.into(),
            10 => e["queue"]["queued"][0]["add_time_ms"] = 1.into(),
            11 => e["queue"]["queued"][0]["exec_time_ms"] = 99.into(),
            12 => {
                let mut event = e["queue"]["queued"][0].take();
                event["queue"] = serde_json::Value::Null;
                e["queue"]["queued"] = serde_json::json!([]);
                e["queue"]["detached"] = serde_json::json!([event]);
            }
            13 => e["can_aggro"] = true.into(),
            14 => e["policy"] = "foreign".into(),
            15 => {
                e["queue"]["queued"][0]["event_id"] = 10.into();
                e["notify"] = 10.into();
            }
            16 => {
                e["notify"] = 2.into();
                e["queue"]["queued"][0]["event_id"] = 2.into();
                e["queue"]["queued"][0]["queue"]["insertion_id"] = 2.into();
                e["queue"]["next_event_id"] = 3.into();
                e["queue"]["next_insertion_id"] = 3.into();
            }
            _ => unreachable!(),
        });
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), forged).is_err(),
            "kind={kind}"
        );
    }
}

#[test]
fn npc_unimplemented_spell_events_fail_closed_and_permanent_teardown_finalizes_notify() {
    let mut w = fixture::world();
    let events = &mut w.state.npcs.get_mut(&2).unwrap().events;
    events
        .queue
        .add_event(2, EventKind::SpellUpdate { cast_id: 1 }, 0)
        .unwrap();
    let before = w.checkpoint().unwrap();
    assert!(w
        .step(1, None, &mut fixture::SyntheticMotionAuthority::default())
        .is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    let mut events = fixture::world().state().npcs[&2].events.clone();
    events.teardown().unwrap();
    assert!(events.notify_handle().is_none());
    assert!(events.queue_snapshot().queued.is_empty());
    assert!(events.queue_snapshot().aborting);
    events.teardown().unwrap();
    assert!(fixture::world().teardown().unwrap().is_empty());
}

#[test]
fn npc_restore_rejects_stranded_aggro_resurrected_due_event_and_generation_allocator_rewind() {
    let mut w = fixture::world();
    step(
        &mut w,
        &mut fixture::SyntheticMotionAuthority {
            visits: Some(false),
            ..Default::default()
        },
    );
    let stranded = altered(&w, |e| {
        e["notify"] = serde_json::Value::Null;
        e["queue"]["queued"] = serde_json::json!([]);
    });
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), stranded).is_err());
    let mut p = fixture::profile();
    p.source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = 100;
    let mut due_world = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 3).unwrap();
    let initial_events = serde_json::to_value(&due_world.checkpoint().unwrap().state).unwrap()
        ["npcs"]["2"]["events"]
        .clone();
    step(
        &mut due_world,
        &mut fixture::SyntheticMotionAuthority {
            visits: Some(false),
            ..Default::default()
        },
    );
    let resurrected = altered(&due_world, |e| {
        *e = initial_events;
        e["queue"]["clock_ms"] = 100.into();
    });
    assert!(NativeWorld::restore_synthetic_component_test(
        due_world.profile().clone(),
        resurrected
    )
    .is_err());
    let mut p = fixture::profile();
    p.npcs[0].ai.corpse_decay_ms = 300;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 3).unwrap();
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    let mut a = fixture::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    };
    step(&mut w, &mut a);
    assert_eq!(w.state().npcs[&2].ai.phase(), ai::Phase::Corpse);
    let paused_corpse = altered(&w, |e| {
        e["skipped_updates"] = 1.into();
        e["queue"]["clock_ms"] = 0.into();
    });
    assert!(
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), paused_corpse).is_err()
    );
    while w.state().npcs[&2].ai.phase() != ai::Phase::Despawned {
        step(&mut w, &mut a);
    }
    step(&mut w, &mut a);
    let paused_dead = altered(&w, |e| {
        e["skipped_updates"] = (e["skipped_updates"].as_u64().unwrap() + 1).into();
        e["queue"]["clock_ms"] = (e["queue"]["clock_ms"].as_u64().unwrap() - 100).into();
    });
    assert!(
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), paused_dead).is_err()
    );
    while w.state().npcs[&2].id.generation == 0 {
        step(&mut w, &mut a);
    }
    let rewound = altered(&w, |e| {
        e["notify"] = 1.into();
        e["queue"]["next_event_id"] = 2.into();
        e["queue"]["next_insertion_id"] = 2.into();
        e["queue"]["queued"][0]["event_id"] = 1.into();
        e["queue"]["queued"][0]["queue"]["insertion_id"] = 1.into();
    });
    assert!(NativeWorld::restore_synthetic_component_test(w.profile().clone(), rewound).is_err());
}

#[test]
fn npc_corpse_update_executes_retained_notify_and_sets_can_aggro_from_current_life() {
    let mut p = fixture::profile();
    p.npcs[0].ai.corpse_decay_ms = 1000;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 3).unwrap();
    let mut a = fixture::SyntheticMotionAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    assert!(w.state().npcs[&2].events.can_aggro());
    // Source relocation/visibility can leave a notify pending at death.
    w.state
        .npcs
        .get_mut(&2)
        .unwrap()
        .events
        .schedule_notify(100, true)
        .unwrap();
    let handle = w.state().npcs[&2].events.notify_handle().unwrap();
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    step(&mut w, &mut a);
    assert_eq!(w.state().npcs[&2].ai.phase(), ai::Phase::Corpse);
    assert!(!w.state().npcs[&2].events.can_aggro());
    assert!(w.state().npcs[&2].events.event(handle).is_none());
    let callback = w
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .find_map(|r| match &r.request.query {
            Query::SourceMotion(MotionQuery::Visitor(v))
                if matches!(v.owner, VisitorOwner::Creature(_)) =>
            {
                Some(v)
            }
            _ => None,
        })
        .unwrap();
    assert!(!callback.creature_can_aggro);
    assert_eq!(callback.creature_phase, ai::Phase::Corpse);
    assert_eq!(callback.callback.due.callback, CallbackKind::Execute);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}

#[test]
fn npc_notification_callback_runs_before_existing_periodic_aura_update() {
    let mut w = fixture::world();
    let mut a = fixture::SyntheticMotionAuthority {
        visits: Some(false),
        critical_chance: Some(0),
        ..Default::default()
    };
    let input = f::cast(&w, 60001);
    w.step(1, Some(input), &mut a).unwrap();
    while w.state().npcs[&2].auras.is_empty() {
        assert!(w.state().tick < 100);
        step(&mut w, &mut a);
    }
    // Exercise the actual runtime phase ordering with a source scheduling call;
    // no synthetic spell host or second event clock is introduced.
    w.state
        .npcs
        .get_mut(&2)
        .unwrap()
        .events
        .schedule_notify(100, true)
        .unwrap();
    step(&mut w, &mut a);
    let requests = w
        .last_tick()
        .unwrap()
        .replies
        .iter()
        .map(|r| &r.request.query)
        .collect::<Vec<_>>();
    let visitor = requests.iter().position(|q| matches!(q,
        Query::SourceMotion(MotionQuery::Visitor(v)) if matches!(v.owner, VisitorOwner::Creature(_)))).unwrap();
    let aura = requests
        .iter()
        .position(|q| {
            matches!(
                q,
                Query::Spell {
                    phase: CastPhase::Aura,
                    ..
                }
            )
        })
        .unwrap();
    assert!(visitor < aura);
    assert_eq!(
        requests
            .iter()
            .filter(|q| matches!(
                q,
                Query::Spell {
                    phase: CastPhase::Aura,
                    ..
                }
            ))
            .count(),
        1
    );
    let observations = requests
        .iter()
        .enumerate()
        .filter_map(|(index, q)| matches!(q, Query::Npc { .. }).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(observations.len(), 2);
    assert!(aura < observations[0] && observations[0] < observations[1]);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}
