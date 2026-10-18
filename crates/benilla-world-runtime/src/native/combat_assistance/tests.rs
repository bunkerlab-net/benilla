// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored Unit-owner component tests; no native admission or real traversal claim.
use super::*;
use crate::native::synthetic_fixture as f;
use benilla_sim::event_processor::{CallbackKind, EventKind, EventProcessor};

fn events() -> NpcEvents {
    let mut events = NpcEvents::new(
        [8; 16],
        crate::NpcId {
            spawn: 2,
            generation: 0,
        },
        &f::profile(),
    )
    .unwrap();
    events.assistance = Some(CombatAssistanceState::new([7; 32]));
    events.assistance_anchor = Some(CombatAssistanceAnchor::default());
    events
}
fn request(events: &NpcEvents) -> CombatAssistanceRequest {
    CombatAssistanceRequest {
        binding: Binding {
            world: events.world,
            tick: 1,
            profile: [2; 32],
            authority_policy: [3; 32],
            geometry: [4; 32],
            npc: Some(events.npc),
        },
        content: [5; 32],
        source: [7; 32],
        generation_source: [6; 32],
        callback: EventAiCallbackKind::CombatReceipt {
            receipt: ai::Receipt {
                id: events.assistance().unwrap().next_occurrence(),
                event: ai::CombatEvent::AttackedBy { target: SOLO_UNIT },
            },
        },
        moment: InteractionMoment::Frame { action_ordinal: 1 },
        now_ms: 5100,
        occurrence: events.assistance().unwrap().next_occurrence(),
        unit_clock_ms: events.clock_ms(),
        owner_pose: Pose {
            position: f::point(6.0),
            orientation: ai::Real::ZERO,
        },
        enemy: f::player(),
        enemy_pose: Pose {
            position: f::point(0.0),
            orientation: ai::Real::ZERO,
        },
        radius: f::real(5.0),
        owner_bounding_radius: f::real(0.5),
        scene: [9; 32],
        scene_inventory: vec![events.npc],
        rng_counter: 12,
        event_ai_callback: None,
    }
}
fn facts(request: &CombatAssistanceRequest) -> CombatAssistanceFacts {
    CombatAssistanceFacts::AuthoredCompleteEmptyTraversal {
        request: Box::new(request.clone()),
        self_container: AssistanceSelfContainer::Grid,
    }
}
fn enter(events: &mut NpcEvents, delay: u32, flags: u32) -> AssistancePayload {
    let token = events.prepare_assistance(delay, flags).unwrap();
    let request = request(events);
    let receivers = (flags & NO_CALL_ASSIST == 0).then(|| facts(&request));
    let traversal = receivers.as_ref().map(|_| AssistanceTraversalReceipt {
        request: crate::digest(b"benilla-native-assistance-traversal-v50\0", &request),
        candidates: vec![AssistanceCandidateReceipt {
            container: AssistanceSelfContainer::Grid,
            npc: events.npc,
            denial: AssistanceDenial::SelfCandidate,
        }],
    });
    events
        .finish_assistance(request, receivers, traversal, token, flags)
        .unwrap();
    events
        .assistance()
        .unwrap()
        .pending()
        .last()
        .unwrap()
        .clone()
}
fn update(events: &mut NpcEvents, diff: u32, enemy: bool, combat: bool) -> Vec<AssistanceOutcome> {
    let mut queue = std::mem::take(&mut events.queue);
    let mut transaction = queue.begin_update(diff).unwrap();
    let mut outcomes = vec![];
    while let Some(due) = transaction.pop_due().unwrap() {
        outcomes.push(
            events
                .dispatch_assistance(1, due, enemy.then(f::player), combat)
                .unwrap(),
        );
        if due.callback == CallbackKind::Execute {
            transaction.finish_execute(true).unwrap();
        } else {
            transaction.finish_abort(true).unwrap();
        }
    }
    transaction.commit().unwrap();
    events.queue = queue;
    outcomes
}
#[test]
fn flags_literal_previous_latch_and_two_source_ordered_events() {
    let mut events = events();
    assert!(events.assistance().unwrap().can_check_for_help());
    assert!(!events.assistance().unwrap().already_called());
    let first = enter(&mut events, 2000, 2);
    assert!(!first.saved_already_called);
    assert!(first.receivers.is_some());
    assert!(!events.assistance().unwrap().can_check_for_help());
    assert!(events.assistance().unwrap().already_called());
    assert_eq!(first.linking_event, first.reenable_event + 1);
    let queue = events.queue_snapshot();
    assert_eq!(
        queue.queued[0].kind,
        EventKind::ReenableHelp { occurrence: 1 }
    );
    assert_eq!(
        queue.queued[1].kind,
        EventKind::AggroLinking { occurrence: 1 }
    );
    assert_eq!(queue.queued[0].exec_time_ms, 2000);
    assert_eq!(queue.queued[1].exec_time_ms, 2000);
    assert!(update(&mut events, 1999, true, true).is_empty());
    assert_eq!(
        update(&mut events, 1, true, true),
        [
            AssistanceOutcome::Reenabled,
            AssistanceOutcome::EmptyContinuation {
                call_assistance: false
            }
        ]
    );
    events.validate_assistance_queue().unwrap();
    // Scalar source control: a direct TriggerAggroLinkingEvent call when the
    // prior source latch is already true. Ordinary repeated combat hits never
    // reach the runtime CombatStarted suffix and cannot create this second pair.
    let next = enter(&mut events, 0, 2);
    assert!(next.saved_already_called);
    assert_eq!(
        update(&mut events, 0, true, true)[1],
        AssistanceOutcome::EmptyContinuation {
            call_assistance: true
        }
    );
}
#[test]
fn positive_template_radius_and_raw_no_call_bit_are_independent() {
    let mut events = events();
    let payload = enter(&mut events, 5, NO_CALL_ASSIST | 2);
    assert!(!payload.saved_already_called);
    assert!(payload.receivers.is_none());
    assert!(events.assistance().unwrap().already_called());
    assert_eq!(events.queue_snapshot().queued.len(), 2);
    assert_eq!(
        update(&mut events, 5, true, true)[1],
        AssistanceOutcome::EmptyContinuation {
            call_assistance: false
        }
    );
    let mut r = request(&events);
    assert_eq!(r.traversal_radius().unwrap(), f::real(5.5));
    r.radius = f::real(900.0);
    assert_eq!(r.traversal_radius().unwrap().bits(), 0x4405_5555);
    for radius in [0.0, -1.0] {
        r.radius = f::real(radius);
        assert!(r.validate(&facts(&r)).is_err());
    }
}
#[test]
fn combat_stop_retains_queue_and_respawn_rebind_keeps_old_life_callbacks() {
    let mut events = events();
    let payload = enter(&mut events, 5, 2);
    let queue = events.queue_snapshot();
    events
        .assistance_combat_stop(1, AssistanceStopReason::Death)
        .unwrap();
    assert_eq!(events.queue_snapshot(), queue);
    assert!(!events.assistance().unwrap().can_check_for_help());
    assert!(!events.assistance().unwrap().already_called());
    events
        .rebind_respawn(crate::NpcId {
            spawn: 2,
            generation: 1,
        })
        .unwrap();
    assert_eq!(
        events
            .assistance()
            .unwrap()
            .pending()
            .next()
            .unwrap()
            .request
            .binding
            .npc
            .unwrap()
            .generation,
        0
    );
    assert_eq!(events.queue_snapshot(), queue);
    assert_eq!(
        update(&mut events, 5, true, false),
        [
            AssistanceOutcome::Reenabled,
            AssistanceOutcome::OwnerNotInCombat
        ]
    );
    assert_eq!(events.queue.next_event_id(), payload.linking_event + 1);
}
#[test]
fn missing_enemy_returns_after_reenable_and_teardown_never_executes_effects() {
    let mut events = events();
    enter(&mut events, 0, 2);
    assert_eq!(
        update(&mut events, 0, false, true),
        [
            AssistanceOutcome::Reenabled,
            AssistanceOutcome::EnemyMissing
        ]
    );
    enter(&mut events, 50, 2);
    events.teardown().unwrap();
    assert!(!events.assistance().unwrap().can_check_for_help());
    assert!(events.assistance().unwrap().already_called());
    assert_eq!(events.assistance().unwrap().pending().count(), 0);
    let history = events.assistance().unwrap().history();
    assert!(history[history.len() - 2..].iter().all(|h| matches!(
        h,
        AssistanceHistory::Callback {
            outcome: AssistanceOutcome::Aborted,
            ..
        }
    )));
}
#[test]
fn zero_delay_uses_queue_clock_and_cannot_execute_at_entry_call_site() {
    let mut events = events();
    update(&mut events, 900, true, false);
    let payload = enter(&mut events, 0, 2);
    assert_eq!(payload.request.now_ms, 5100);
    assert_eq!(payload.deadline_ms, 900);
    assert_eq!(events.queue_snapshot().queued.len(), 2);
    assert!(!events.assistance().unwrap().can_check_for_help());
    assert_eq!(update(&mut events, 0, true, true).len(), 2);
}
#[test]
fn active_own_callback_rejects_before_queue_mutation() {
    let mut events = events();
    events.queue.add_event(1, EventKind::AiNotify, 0).unwrap();
    let mut queue = std::mem::take(&mut events.queue);
    let mut tx = queue.begin_update(1).unwrap();
    tx.pop_due().unwrap().unwrap();
    events.callback = Some(tx.callback_snapshot().unwrap());
    let before = events.clone();
    assert!(matches!(
        events.prepare_assistance(5, 2),
        Err(NativeError::UnsupportedMechanic(_))
    ));
    assert_eq!(events, before);
    tx.finish_execute(true).unwrap();
    tx.commit().unwrap();
}
#[test]
fn missing_nonempty_and_every_stale_request_dimension_reject() {
    let r = request(&events());
    for bad in [
        CombatAssistanceFacts::Missing,
        CombatAssistanceFacts::Unknown,
        CombatAssistanceFacts::NonemptyCandidates,
    ] {
        assert!(r.validate(&bad).is_err());
    }
    let mutations: &[fn(&mut CombatAssistanceRequest)] = &[
        |r| r.binding.world[0] ^= 1,
        |r| r.binding.tick += 1,
        |r| r.binding.profile[0] ^= 1,
        |r| r.binding.authority_policy[0] ^= 1,
        |r| r.binding.geometry[0] ^= 1,
        |r| r.binding.npc.as_mut().unwrap().generation += 1,
        |r| r.content[0] ^= 1,
        |r| r.source[0] ^= 1,
        |r| r.generation_source[0] ^= 1,
        |r| r.occurrence += 1,
        |r| r.unit_clock_ms += 1,
        |r| r.now_ms += 1,
        |r| r.owner_pose.position.x = f::real(7.0),
        |r| r.enemy.character[0] ^= 1,
        |r| r.enemy_pose.position.x = f::real(1.0),
        |r| r.radius = f::real(6.0),
        |r| r.owner_bounding_radius = f::real(0.6),
        |r| r.scene[0] ^= 1,
        |r| r.scene_inventory[0].generation += 1,
        |r| r.rng_counter += 1,
        |r| r.event_ai_callback = Some([10; 32]),
        |r| r.moment = InteractionMoment::Frame { action_ordinal: 2 },
        |r| {
            let EventAiCallbackKind::CombatReceipt { receipt } = &mut r.callback else {
                unreachable!()
            };
            receipt.id += 1;
        },
    ];
    for mutate in mutations {
        let mut other = r.clone();
        mutate(&mut other);
        assert_eq!(
            other.validate(&facts(&r)),
            Err(NativeError::AuthorityBinding)
        );
    }
}
#[test]
fn required_optional_snapshots_and_exact_queue_payload_mapping() {
    let mut events = events();
    enter(&mut events, 2000, 2);
    let json = serde_json::to_value(events.snapshot()).unwrap();
    for path in [
        vec!["assistance"],
        vec!["assistance_anchor"],
        vec!["event_ai_anchor"],
    ] {
        let mut value = json.clone();
        value.as_object_mut().unwrap().remove(path[0]);
        assert!(
            serde_json::from_value::<crate::native::npc_events::NpcEventsSnapshot>(value).is_err()
        );
    }
    for field in ["receivers", "traversal"] {
        let mut value =
            serde_json::to_value(events.assistance().unwrap().pending().next().unwrap()).unwrap();
        value.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<AssistancePayload>(value).is_err());
    }
    let mut value = serde_json::to_value(request(&events)).unwrap();
    value.as_object_mut().unwrap().remove("event_ai_callback");
    assert!(serde_json::from_value::<CombatAssistanceRequest>(value).is_err());
    let mutations: &[fn(&mut benilla_sim::event_processor::EventProcessorSnapshot)] = &[
        |q| q.queued[0].kind = EventKind::ReenableHelp { occurrence: 2 },
        |q| q.queued[0].kind = EventKind::AggroLinking { occurrence: 1 },
        |q| {
            q.queued[0].exec_time_ms += 1;
            q.queued[0].queue.as_mut().unwrap().deadline_ms += 1;
        },
        |q| q.queued[0].add_time_ms += 1,
        |q| q.queued[0].to_abort = true,
    ];
    for mutate in mutations {
        let mut changed = events.clone();
        let mut queue = changed.queue_snapshot();
        mutate(&mut queue);
        if let Ok(queue) = EventProcessor::restore(queue) {
            changed.queue = queue;
            assert!(changed.validate_assistance_queue().is_err());
        }
    }
}
#[test]
fn prior_equal_deadline_event_preserves_insertion_order() {
    let mut events = events();
    events.queue.add_event(1, EventKind::AiNotify, 50).unwrap();
    let payload = enter(&mut events, 50, 2);
    assert_eq!(
        events
            .queue_snapshot()
            .queued
            .iter()
            .map(|e| e.event_id)
            .collect::<Vec<_>>(),
        vec![1, payload.reenable_event, payload.linking_event]
    );
}
#[test]
fn independent_original_cpp_source_vectors_match_the_unit_owner() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../scripts/native_assistance_source_oracle/vectors.json"
    ))
    .unwrap();
    assert_eq!(
        vectors["source_revision"],
        "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
    );
    let selected = [
        "first_entry_positive_radius",
        "repeated_hit_no_new_pair",
        "zero_delay_waits_for_update",
        "queue_clock_lags_map_clock",
        "missing_enemy_at_execution",
        "combat_stop_before_execution",
        "corpse_advances_events",
        "teardown_force_aborts_without_execute",
        "no_call_assist_saved_false",
    ];
    for name in selected {
        let case = vectors["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap();
        let input = &case["input"];
        let mut e = events();
        let mut combat = false;
        let mut enemy = true;
        let mut queue = e.queue_snapshot();
        queue.clock_ms = input["initial_queue_clock"].as_u64().unwrap();
        e.queue = EventProcessor::restore(queue).unwrap();
        let mut initial = serde_json::to_value(e.assistance().unwrap()).unwrap();
        initial["already_called"] = input["initial_already_called"].clone();
        e.assistance = Some(serde_json::from_value(initial).unwrap());
        for (operation, observation) in input["operations"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["observations"].as_array().unwrap().iter().skip(1))
        {
            let words: Vec<_> = operation.as_str().unwrap().split_whitespace().collect();
            match words[0] {
                "entry" if !combat => {
                    enter(
                        &mut e,
                        input["delay_ms"].as_u64().unwrap() as u32,
                        input["raw_extra_flags"].as_u64().unwrap() as u32,
                    );
                    combat = true;
                }
                "entry" | "map_advance" => {}
                "update" => {
                    update(&mut e, words[1].parse().unwrap(), enemy, combat);
                }
                "enemy_present" => enemy = words[1] == "1",
                "combat_stop" | "die" => {
                    combat = false;
                    e.assistance_combat_stop(
                        1,
                        if words[0] == "die" {
                            AssistanceStopReason::Death
                        } else {
                            AssistanceStopReason::Evade
                        },
                    )
                    .unwrap();
                }
                "teardown" => e.teardown().unwrap(),
                other => panic!("unhandled {other}"),
            }
            assert_eq!(
                e.clock_ms(),
                observation["queue_clock"].as_u64().unwrap(),
                "{name}: {operation}"
            );
            assert_eq!(
                e.assistance().unwrap().can_check_for_help(),
                observation["can_check_for_help"].as_bool().unwrap(),
                "{name}: {operation}"
            );
            assert_eq!(
                e.assistance().unwrap().already_called(),
                observation["already_called"].as_bool().unwrap(),
                "{name}: {operation}"
            );
            let queue = e.queue_snapshot();
            let expected = observation["queued"].as_array().unwrap();
            assert_eq!(queue.queued.len(), expected.len(), "{name}: {operation}");
            for (actual, expected) in queue.queued.iter().zip(expected) {
                assert_eq!(actual.event_id, expected["id"].as_u64().unwrap());
                assert_eq!(actual.add_time_ms, expected["add_clock"].as_u64().unwrap());
                assert_eq!(actual.exec_time_ms, expected["deadline"].as_u64().unwrap());
                assert_eq!(
                    match actual.kind {
                        EventKind::ReenableHelp { .. } => "ReenableHelp",
                        EventKind::AggroLinking { .. } => "AggroLinking",
                        _ => panic!(),
                    },
                    expected["kind"].as_str().unwrap()
                );
            }
        }
    }
}

#[test]
fn callback_entry_uses_live_transaction_and_matches_source_zero_delay_order() {
    // Source SetInCombatState EntryEvent oracle, not an inferred queue model.
    let vectors: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/native_assistance_source_oracle/vectors.json"
    )))
    .unwrap();
    let vector = vectors["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "callback_entry_zero_delay_source_only")
        .unwrap();
    let source = vector["observations"].as_array().unwrap().last().unwrap();
    let mut events = events();
    events.queue.add_event(1, EventKind::AiNotify, 0).unwrap();
    let mut queue = std::mem::take(&mut events.queue);
    let mut tx = queue.begin_update(7).unwrap();
    let entry = tx.pop_due().unwrap().unwrap();
    let entry_callback = tx.callback_snapshot().unwrap();
    events.callback = Some(entry_callback.clone());
    let token = events
        .prepare_assistance_with_transaction(0, 2, Some(&mut tx))
        .unwrap();
    let mut request = request(&events);
    request.callback = EventAiCallbackKind::Sight {
        notification_owner: VisitorParticipant::Creature(events.npc),
        callback: entry_callback,
    };
    let receivers = Some(facts(&request));
    let traversal = Some(AssistanceTraversalReceipt {
        request: crate::digest(b"benilla-native-assistance-traversal-v50\0", &request),
        candidates: vec![AssistanceCandidateReceipt {
            container: AssistanceSelfContainer::Grid,
            npc: events.npc,
            denial: AssistanceDenial::SelfCandidate,
        }],
    });
    events
        .finish_assistance_with_transaction(request, receivers, traversal, token, 2, Some(&mut tx))
        .unwrap();
    assert_eq!(
        events.queue.clock_ms(),
        0,
        "placeholder must remain untouched"
    );
    assert_eq!(events.queue.next_event_id(), 1);
    assert_eq!(events.clock_ms(), 7);
    let scheduled = events.queue_snapshot().queued;
    assert_eq!(scheduled.len(), 2);
    for (actual, expected) in scheduled
        .iter()
        .zip(source["scheduled"].as_array().unwrap().iter().skip(1))
    {
        assert_eq!(actual.event_id, expected["id"].as_u64().unwrap());
        assert_eq!(actual.add_time_ms, expected["add_clock"].as_u64().unwrap());
        assert_eq!(actual.exec_time_ms, expected["deadline"].as_u64().unwrap());
    }
    tx.finish_execute(true).unwrap();
    events.callback = None;
    let mut executed = vec![entry.event_id];
    let mut outcomes = vec![];
    while let Some(due) = tx.pop_due().unwrap() {
        executed.push(due.event_id);
        outcomes.push(
            events
                .dispatch_assistance(1, due, Some(f::player()), true)
                .unwrap(),
        );
        tx.finish_execute(true).unwrap();
    }
    tx.commit().unwrap();
    events.queue = queue;
    assert_eq!(serde_json::to_value(executed).unwrap(), source["executed"]);
    assert_eq!(events.clock_ms(), source["queue_clock"].as_u64().unwrap());
    assert_eq!(
        outcomes,
        [
            AssistanceOutcome::Reenabled,
            AssistanceOutcome::EmptyContinuation {
                call_assistance: false
            }
        ]
    );
    assert!(events.assistance().unwrap().can_check_for_help());
    events.validate_assistance_queue().unwrap();
}
