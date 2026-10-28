// SPDX-License-Identifier: GPL-3.0-or-later
//! Ground source-generation component checks complement the byte-backed dry loop.
use super::*;

fn source_row(p: &Profile) -> CombatAssistanceSource {
    let raw = p.creature_generations.as_ref().unwrap().source(2).unwrap();
    CombatAssistanceSource {
        spawn_id: 2,
        entry: raw.spawn_entry,
        generation_source: raw.identity(),
        call_for_help: f::real(5.0),
        excluded_hooks: SourceVisitorRows::Complete(vec![]),
    }
}
fn selected(delay: u32) -> Profile {
    let mut p = synthetic_event_ai_fixture::install(composed(authored_source(Some(1), None)), 100);
    p.initialize_combat_assistance(vec![source_row(&p)], delay)
        .unwrap();
    p
}
#[derive(Default)]
struct Empty {
    base: synthetic_event_ai_fixture::Authority,
}
impl Authority for Empty {
    fn resolve(&mut self, r: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if let Query::CombatAssistance(request) = &r.query {
            return Ok(AuthorityReply {
                request: r.clone(),
                facts: Facts::CombatAssistance(
                    CombatAssistanceFacts::AuthoredCompleteEmptyTraversal {
                        request: request.clone(),
                        self_container: AssistanceSelfContainer::Grid,
                    },
                ),
            });
        }
        self.base.resolve(r)
    }
    fn observe_derived(&mut self, r: &AuthorityReply) -> Result<(), NativeError> {
        self.base.observe_derived(r)
    }
}
fn entered(delay: u32) -> (NativeWorld, Empty) {
    entered_profile(selected(delay))
}
fn entered_profile(profile: Profile) -> (NativeWorld, Empty) {
    let mut w = world(profile, 7);
    let mut a = Empty::default();
    // This explicit authored visibility failure suppresses proactive sight. The
    // real hostile-hit callback remains the only combat entry in this fixture.
    a.base.base.visibility = Some(ai::movement::Check::Failed);
    // A resisted hostile spell enters combat without killing this one-health
    // authored spawn; the source callback is independent of effective damage.
    a.base.base.base.base.resist_chance = Some(10000);
    let input = f::cast(&w, 60001);
    w.step(1, Some(input), &mut a).unwrap();
    for _ in 0..50 {
        if w.state.npcs[&2]
            .events
            .assistance()
            .unwrap()
            .already_called()
        {
            a.base.base.base.base.resist_chance = Some(0);
            a.base.base.base.base.critical_chance = Some(0);
            return (w, a);
        }
        step(&mut w, &mut a);
    }
    panic!("source hostile callback did not enter combat");
}
fn changed(w: &NativeWorld, mutate: impl FnOnce(&mut serde_json::Value)) -> Checkpoint {
    let mut cp = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&cp.state).unwrap();
    let before = state["npcs"]["2"]["events"].clone();
    mutate(&mut state["npcs"]["2"]["events"]);
    assert_ne!(
        state["npcs"]["2"]["events"], before,
        "mutation must change the checkpoint"
    );
    cp.state = serde_json::from_value(state).unwrap();
    cp.digest = crate::digest(b"benilla-native-state-v56\0", &cp.state);
    cp.checksum = crate::digest(
        b"benilla-native-checkpoint-v56\0",
        &(&cp.state, cp.digest, cp.history_tip),
    );
    cp
}
#[test]
fn rehashed_assistance_payload_queue_history_and_old_policy_mutations_reject() {
    let (w, _) = entered(2000);
    assert_eq!(restore(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
    let mutations: &[fn(&mut serde_json::Value)] = &[
        |e| e["policy"] = "cmangos-8ec338a1-native-npc-events-v3".into(),
        |e| e["assistance"]["can_check_for_help"] = true.into(),
        |e| e["assistance"]["already_called"] = false.into(),
        |e| e["assistance"]["next_occurrence"] = 9.into(),
        |e| e["assistance"]["history"][0]["Enter"]["saved_already_called"] = true.into(),
        |e| e["assistance"]["history"][0]["Enter"]["reenable_event"] = 900.into(),
        |e| e["assistance"]["history"][0]["Enter"]["linking_event"] = 901.into(),
        |e| e["assistance"]["history"][0]["Enter"]["reenable_insertion"] = 900.into(),
        |e| e["assistance"]["history"][0]["Enter"]["deadline_ms"] = 900.into(),
        |e| e["assistance"]["history"][0]["Enter"]["request"]["rng_counter"] = 0.into(),
        |e| {
            let byte = &mut e["assistance"]["history"][0]["Enter"]["request"]["source"][0];
            *byte = (byte.as_u64().unwrap() ^ 1).into();
        },
        |e| {
            e["assistance"]["history"][0]["Enter"]["request"]["binding"]["npc"]["generation"] =
                1.into()
        },
        |e| {
            e["assistance"]["history"][0]["Enter"]["request"]["event_ai_callback"] =
                serde_json::Value::Null
        },
        |e| e["assistance"]["history"][0]["Enter"]["receivers"] = serde_json::Value::Null,
        |e| e["assistance"]["pending"]["1"]["payload"]["saved_already_called"] = true.into(),
        |e| {
            let mut extra = e["assistance"]["history"][0].clone();
            extra["Enter"]["request"]["occurrence"] = 2.into();
            e["assistance"]["history"]
                .as_array_mut()
                .unwrap()
                .push(extra);
        },
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), changed(&w, mutate))
                .is_err(),
            "mutation {index}"
        );
    }
    for field in ["assistance", "assistance_anchor"] {
        let mut missing = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
        assert!(missing["state"]["npcs"]["2"]["events"]
            .as_object_mut()
            .unwrap()
            .remove(field)
            .is_some());
        assert!(
            serde_json::from_value::<Checkpoint>(missing).is_err(),
            "missing {field}"
        );
    }
}
#[test]
fn completed_callbacks_remain_semantically_checked_after_compaction() {
    let (mut w, mut a) = entered(0);
    step(&mut w, &mut a);
    assert_eq!(
        w.state.npcs[&2]
            .events
            .assistance()
            .unwrap()
            .pending()
            .count(),
        0
    );
    let cp = w.checkpoint().unwrap();
    w.checkpoint_committed(&cp).unwrap();
    assert_eq!(restore(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
    let forged = changed(&w, |events| {
        let history = events["assistance"]["history"].as_array_mut().unwrap();
        let callback = history
            .iter_mut()
            .find(|h| h["Callback"]["outcome"].get("EmptyContinuation").is_some())
            .unwrap();
        callback["Callback"]["outcome"]["EmptyContinuation"]["call_assistance"] = true.into();
    });
    assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), forged).is_err());
}
#[test]
fn selected_raw_source_requires_positive_radius_exact_generation_and_complete_no_hooks() {
    let p = selected(2000);
    let good = source_row(&p);
    let mut rows = vec![];
    for radius in [0.0, -1.0] {
        let mut row = good.clone();
        row.call_for_help = f::real(radius);
        rows.push(row);
    }
    let mut row = good.clone();
    row.generation_source[0] ^= 1;
    rows.push(row);
    let mut row = good.clone();
    row.entry += 1;
    rows.push(row);
    for hooks in [
        SourceVisitorRows::Missing,
        SourceVisitorRows::Unknown,
        SourceVisitorRows::Complete(vec![CombatAssistanceHook::CreatureLinking]),
        SourceVisitorRows::Complete(vec![CombatAssistanceHook::CreatureGroup]),
    ] {
        let mut row = good.clone();
        row.excluded_hooks = hooks;
        rows.push(row);
    }
    for row in rows {
        let mut changed = p.clone();
        assert!(changed
            .initialize_combat_assistance(vec![row], 2000)
            .is_err());
        assert_eq!(changed, p);
    }
}
#[test]
fn selected_aggressive_own_notify_entry_owns_queue_event_ai_and_same_update_zero_delay() {
    for delay in [0, 2000] {
        let mut w = world(selected(delay), 7);
        let initial = w.clone();
        let mut a = Empty::default();
        step(&mut w, &mut a);
        w.step(2, None, &mut a).unwrap();
        let npc = &w.state.npcs[&2];
        assert_eq!(npc.ai.phase(), ai::Phase::Combat);
        assert_eq!(
            npc.ai.last_receipt_id(),
            0,
            "sight must not fabricate AttackedBy"
        );
        let assistance = npc.events.assistance().unwrap();
        let AssistanceHistory::Enter(entry) = &assistance.history()[0] else {
            panic!("source combat entry");
        };
        let EventAiCallbackKind::Sight {
            notification_owner,
            callback,
        } = &entry.request.callback
        else {
            panic!("actual source sight cause");
        };
        assert_eq!(*notification_owner, VisitorParticipant::Creature(npc.id));
        assert_eq!(entry.request.unit_clock_ms, callback.due.now_ms);
        assert_eq!(entry.deadline_ms, callback.due.now_ms + u64::from(delay));
        assert_eq!(entry.reenable_event, callback.queue.next_event_id);
        assert_eq!(entry.linking_event, entry.reenable_event + 1);
        assert!(entry.request.event_ai_callback.is_some());
        assert_eq!(assistance.pending().count(), usize::from(delay != 0));
        assert_eq!(assistance.can_check_for_help(), delay == 0);
        assert_eq!(restore(&w).checkpoint().unwrap(), w.checkpoint().unwrap());
        let mut replay = initial;
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    }
}

#[test]
fn completed_history_cannot_be_erased_even_without_event_ai_after_combat_stop() {
    let mut profile = composed(authored_source(Some(1), None));
    assert!(profile.event_ai().is_none());
    profile
        .initialize_combat_assistance(vec![source_row(&profile)], 0)
        .unwrap();
    let (mut w, mut authority) = entered_profile(profile);
    step(&mut w, &mut authority);
    kill(&mut w, &mut authority);
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Corpse);
    let assistance = w.state.npcs[&2].events.assistance().unwrap();
    assert!(assistance.can_check_for_help());
    assert!(!assistance.already_called());
    assert_eq!(assistance.pending().count(), 0);
    let checkpoint = w.checkpoint().unwrap();
    w.checkpoint_committed(&checkpoint).unwrap();
    let erased = changed(&w, |events| {
        let state = &mut events["assistance"];
        state["history"] = serde_json::json!([]);
        state["pending"] = serde_json::json!({});
        state["next_occurrence"] = 1.into();
        state["can_check_for_help"] = true.into();
        state["already_called"] = false.into();
    });
    assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), erased).is_err());
}
