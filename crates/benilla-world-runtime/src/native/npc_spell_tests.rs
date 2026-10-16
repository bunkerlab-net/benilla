// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored SpellStart/controller-state fixtures, not stock daze/content evidence.
use super::{synthetic_fixture as f, synthetic_slow_fixture as fixture, *};
use benilla_sim::event_processor::EventKind;
fn trigger(tick: u64, after_npc_update: bool) -> fixture::SyntheticSlowTrigger {
    fixture::SyntheticSlowTrigger {
        tick,
        spell_id: 61004,
        caster: crate::NpcId {
            spawn: 2,
            generation: 0,
        },
        after_npc_update,
    }
}
fn world(triggers: Vec<fixture::SyntheticSlowTrigger>) -> NativeWorld {
    NativeWorld::synthetic_component_test(fixture::with_triggers(triggers), [8; 16], f::player(), 7)
        .unwrap()
}
fn step(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state().tick + 1, None, a).unwrap();
}
fn reopen(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
        .unwrap()
}
#[test]
fn synchronous_trigger_applies_but_finished_spell_remains_unit_owned_until_due() {
    let mut w = world(vec![trigger(1, true)]);
    let initial = w.clone();
    let mut a = fixture::SyntheticNpcSpellAuthority::default();
    step(&mut w, &mut a);
    let npc = &w.state().npcs[&2];
    let lifetime = npc.events.spells().next().unwrap().clone();
    assert_eq!(
        lifetime.terminal,
        Some(NpcSpellTerminal::Hit {
            effect_applied: true
        })
    );
    let event = npc
        .events
        .queue_snapshot()
        .queued
        .into_iter()
        .find(|e| matches!(e.kind, EventKind::SpellUpdate { .. }))
        .unwrap();
    assert_eq!((event.add_time_ms, event.exec_time_ms), (100, 101));
    assert_eq!(lifetime.event_id, event.event_id);
    let holder = w
        .state()
        .player_slow
        .as_ref()
        .unwrap()
        .holders()
        .next()
        .unwrap();
    assert_eq!(
        (holder.remaining_ms, holder.created_after_player_update),
        (400, true)
    );
    assert!(w.state().casts.is_empty());
    assert_eq!(w.state().player_events.current_spell(), None);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
    let before = npc.ai.pursuit_timer_ms();
    step(&mut w, &mut a);
    assert!(w.state().npcs[&2].events.spells().next().is_none());
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(e,Event::NpcSpell(NpcSpellEvent::Destroyed { lifetime: l, aborted: false }) if l==&lifetime)));
    assert!(w.state().npcs[&2].ai.pursuit_timer_ms() <= before);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
    let mut replay = initial;
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn strict_failure_keeps_event_and_stops_before_target_map_and_hit_checks() {
    let mut w = world(vec![trigger(1, true)]);
    let mut a = fixture::SyntheticNpcSpellAuthority {
        fail: Some(NpcSpellCheck::TargetNotTaxiFlying),
        ..Default::default()
    };
    step(&mut w, &mut a);
    assert_eq!(a.checks, [NpcSpellCheck::TargetNotTaxiFlying]);
    assert_eq!(
        w.state().npcs[&2].events.spells().next().unwrap().terminal,
        Some(NpcSpellTerminal::BadTargets)
    );
    assert!(w
        .state()
        .player_slow
        .as_ref()
        .unwrap()
        .holders()
        .next()
        .is_none());
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
    step(&mut w, &mut a);
    assert!(w.state().npcs[&2].events.spells().next().is_none());
}
#[test]
fn filter_effect_immunity_full_immunity_and_evade_are_distinct_source_outcomes() {
    for (check, terminal) in [
        (NpcSpellCheck::TargetNotGm, NpcSpellTerminal::TargetFiltered),
        (
            NpcSpellCheck::EffectNotImmune,
            NpcSpellTerminal::Hit {
                effect_applied: false,
            },
        ),
        (NpcSpellCheck::SpellNotImmune, NpcSpellTerminal::Immune),
        (NpcSpellCheck::TargetNotEvading, NpcSpellTerminal::Evade),
    ] {
        let mut w = world(vec![trigger(1, true)]);
        let mut a = fixture::SyntheticNpcSpellAuthority {
            fail: Some(check),
            ..Default::default()
        };
        step(&mut w, &mut a);
        assert_eq!(
            w.state().npcs[&2].events.spells().next().unwrap().terminal,
            Some(terminal)
        );
        assert!(w
            .state()
            .player_slow
            .as_ref()
            .unwrap()
            .holders()
            .next()
            .is_none());
        if check == NpcSpellCheck::EffectNotImmune {
            assert!(a.checks.contains(&NpcSpellCheck::SpellNotImmune));
        }
        if check == NpcSpellCheck::TargetNotEvading {
            assert!(!a.checks.contains(&NpcSpellCheck::CanAttack));
        }
    }
}
#[test]
fn unknown_or_nonempty_host_hooks_roll_back_spell_queue_holder_and_whole_tick() {
    for i in 0..4 {
        let mut w = world(vec![trigger(1, true)]);
        let before = w.state_digest();
        let mut a = fixture::SyntheticNpcSpellAuthority::default();
        if i == 0 {
            a.unknown = Some(NpcSpellCheck::SpellNotImmune);
        } else {
            let mut closure = fixture::closed_spell_host();
            match i {
                1 => closure.spell_magnets = InteractionEvidence::Present,
                2 => closure.hit_procs = InteractionEvidence::Unknown,
                _ => closure.npc_spell_hit_target_hooks = InteractionEvidence::Present,
            }
            a.closure = Some(closure);
        }
        assert!(w.step(1, None, &mut a).is_err());
        assert_eq!(w.state_digest(), before);
        assert!(w.records().is_empty());
        step(&mut w, &mut fixture::SyntheticNpcSpellAuthority::default());
        assert_eq!(w.state().npcs[&2].events.spells().count(), 1);
    }
}
#[test]
fn before_unit_trigger_is_deleted_same_update_and_mixed_queue_preserves_deadline_order() {
    let mut w = world(vec![trigger(1, true), trigger(2, false), trigger(2, true)]);
    let mut a = fixture::SyntheticNpcSpellAuthority::default();
    step(&mut w, &mut a);
    let first = w.state().npcs[&2].events.spells().next().unwrap().clone();
    step(&mut w, &mut a);
    let destroyed: Vec<_> = w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .filter_map(|e| match e {
            Event::NpcSpell(NpcSpellEvent::Destroyed { lifetime, .. }) => Some(lifetime.cast_id),
            _ => None,
        })
        .collect();
    assert_eq!(destroyed, [first.cast_id, first.cast_id + 1]);
    let retained = w.state().npcs[&2].events.spells().next().unwrap();
    assert_eq!(retained.cast_id, first.cast_id + 2);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
    assert!(w.clone().teardown().is_ok());
}
#[test]
fn requests_bind_enqueued_event_and_current_generation_and_replies_cannot_retarget() {
    struct Stale(fixture::SyntheticNpcSpellAuthority);
    impl Authority for Stale {
        fn resolve(&mut self, r: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            let mut reply = self.0.resolve(r)?;
            if let Query::NpcSpell(NpcSpellQuery::Closure(context)) = &mut reply.request.query {
                assert!(context
                    .queue
                    .queued
                    .iter()
                    .any(|e| e.event_id == context.lifetime.event_id));
                context.caster.generation += 1;
            }
            Ok(reply)
        }
    }
    let mut w = world(vec![trigger(1, true)]);
    let before = w.state_digest();
    assert_eq!(
        w.step(1, None, &mut Stale(Default::default())).unwrap_err(),
        NativeError::AuthorityBinding
    );
    assert_eq!(before, w.state_digest());
    let mut t = trigger(1, true);
    t.caster.generation = 1;
    let mut w = world(vec![t]);
    assert_eq!(
        w.step(1, None, &mut fixture::SyntheticNpcSpellAuthority::default())
            .unwrap_err(),
        NativeError::UnknownTarget
    );
}
#[test]
fn triggered_entry_rejects_magic_and_daze_without_inventing_source_rows() {
    let mut p = fixture::with_triggers(vec![trigger(1, true)]);
    let mut source = player_slow::source::synthetic_source(61004, -37, 1703);
    source.spell.words[164] = 1;
    p.player_slow.as_mut().unwrap().catalog =
        PlayerSlowCatalog::validate_sources(vec![source]).unwrap();
    assert!(p.validate().is_err());
    let mut p = fixture::with_triggers(vec![trigger(1, true)]);
    p.player_slow.as_mut().unwrap().fixture_triggers[0].spell_id = 1604;
    assert!(p.validate().is_err());
}

#[test]
fn raw_none_hit_has_no_added_rng_and_keeps_exact_check_order() {
    let mut w = world(vec![trigger(1, true)]);
    let mut baseline = world(vec![]);
    let mut a = fixture::SyntheticNpcSpellAuthority::default();
    step(&mut w, &mut a);
    step(
        &mut baseline,
        &mut fixture::SyntheticNpcSpellAuthority::default(),
    );
    assert_eq!(
        w.last_tick().unwrap().draws,
        baseline.last_tick().unwrap().draws
    );
    assert_eq!(
        w.state().npcs[&2].ai.pursuit_timer_ms(),
        baseline.state().npcs[&2].ai.pursuit_timer_ms()
    );
    assert_eq!(
        w.state().npcs[&2].ai.threat(),
        baseline.state().npcs[&2].ai.threat()
    );
    assert_eq!(
        a.checks,
        [
            NpcSpellCheck::TargetNotTaxiFlying,
            NpcSpellCheck::TargetNotUntargetable,
            NpcSpellCheck::TargetNotSpiritOfRedemption,
            NpcSpellCheck::CanAttackSpell,
            NpcSpellCheck::LocationAllowed,
            NpcSpellCheck::TargetGmVisible,
            NpcSpellCheck::TargetNotGm,
            NpcSpellCheck::FilterTargetNotUntargetable,
            NpcSpellCheck::EffectNotImmune,
            NpcSpellCheck::TargetNotEvading,
            NpcSpellCheck::SpellNotImmune,
            NpcSpellCheck::CanAttack,
            NpcSpellCheck::CasterDetectedByTarget,
        ]
    );
}
#[test]
fn checkpoint_rejects_forged_spell_owner_source_time_and_event_order() {
    let mut w = world(vec![trigger(1, true), trigger(1, true)]);
    step(&mut w, &mut fixture::SyntheticNpcSpellAuthority::default());
    for mutation in 0..9 {
        let mut checkpoint = w.checkpoint().unwrap();
        let mut state = serde_json::to_value(&checkpoint.state).unwrap();
        let events = &mut state["npcs"]["2"]["events"];
        match mutation {
            0 => events["spells"]["1"]["caster"]["generation"] = 1.into(),
            1 => events["spells"]["1"]["target"]["character"] = 99.into(),
            2 => events["spells"]["1"]["source_binding"][0] = 99.into(),
            3 => events["spells"]["1"]["started_tick"] = 2.into(),
            4 => events["spells"]["1"]["event_add_ms"] = 0.into(),
            5 => events["spells"]["1"]["terminal"] = serde_json::Value::Null,
            6 => events["next_cast_id"] = 99.into(),
            7 => {
                events["spells"].as_object_mut().unwrap().remove("1");
            }
            _ => {
                let one = events["spells"]["1"]["event_id"].clone();
                let two = events["spells"]["2"]["event_id"].clone();
                events["spells"]["1"]["event_id"] = two;
                events["spells"]["2"]["event_id"] = one;
            }
        }
        let Ok(state) = serde_json::from_value(state) else {
            continue;
        };
        checkpoint.state = state;
        checkpoint.digest = crate::digest(b"benilla-native-state-v26\0", &checkpoint.state);
        checkpoint.checksum = crate::digest(
            b"benilla-native-checkpoint-v26\0",
            &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn equal_deadline_notify_executes_before_later_inserted_finished_spell() {
    let mut p = fixture::with_triggers(vec![trigger(1, false)]);
    p.source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = 1;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let mut a = fixture::SyntheticNpcSpellAuthority {
        fail: Some(NpcSpellCheck::TargetNotTaxiFlying),
        ..Default::default()
    };
    step(&mut w, &mut a);
    let events = &w.last_tick().unwrap().events;
    let entered = events
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
    let destroyed = events
        .iter()
        .position(|e| matches!(e, Event::NpcSpell(NpcSpellEvent::Destroyed { .. })))
        .unwrap();
    assert!(entered < destroyed);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}
#[test]
fn stronger_distinct_slow_does_not_make_later_weaker_source_spell_bounce() {
    let mut strong = trigger(1, true);
    strong.spell_id = 61005;
    let mut w = world(vec![strong, trigger(1, true)]);
    step(&mut w, &mut fixture::SyntheticNpcSpellAuthority::default());
    assert_eq!(w.state().player_slow.as_ref().unwrap().holders().count(), 2);
    assert!(w.state().npcs[&2].events.spells().all(|s| s.terminal
        == Some(NpcSpellTerminal::Hit {
            effect_applied: true
        })));
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}

#[test]
fn cleaned_spell_still_advances_the_unit_event_allocator_on_restore() {
    let mut w = world(vec![trigger(1, true)]);
    let mut a = fixture::SyntheticNpcSpellAuthority::default();
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    let mut checkpoint = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&checkpoint.state).unwrap();
    let events = &mut state["npcs"]["2"]["events"];
    assert!(events["spells"].as_object().unwrap().is_empty());
    events["notify"] = serde_json::Value::Null;
    events["queue"]["queued"] = serde_json::json!([]);
    events["queue"]["next_event_id"] = 2.into();
    events["queue"]["next_insertion_id"] = 2.into();
    checkpoint.state = serde_json::from_value(state).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v26\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v26\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    assert!(
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint).is_err()
    );
}

#[test]
fn finished_terminal_and_same_tick_holder_cannot_contradict_each_other_on_restore() {
    let mut w = world(vec![trigger(1, true)]);
    step(&mut w, &mut fixture::SyntheticNpcSpellAuthority::default());
    let mut checkpoint = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&checkpoint.state).unwrap();
    state["npcs"]["2"]["events"]["spells"]["1"]["terminal"] = serde_json::json!("BadTargets");
    state["npcs"]["2"]["events"]["spells"]["1"]["applied_holder"] = serde_json::Value::Null;
    checkpoint.state = serde_json::from_value(state).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v26\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v26\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    assert!(
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint).is_err()
    );
}

#[test]
fn current_generation_recast_replaces_same_source_unit_holder_after_respawn() {
    let mut later = trigger(20, true);
    later.caster.generation = 1;
    let mut p = fixture::with_triggers(vec![trigger(1, true), later]);
    p.npcs[0].ai.corpse_decay_ms = 300;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    p.player_slow.as_mut().unwrap().catalog =
        PlayerSlowCatalog::validate_sources(vec![player_slow::source::synthetic_source(
            61004, -40, 10000,
        )])
        .unwrap();
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap();
    let mut a = fixture::SyntheticNpcSpellAuthority::default();
    step(&mut w, &mut a);
    let old = w
        .state()
        .player_slow
        .as_ref()
        .unwrap()
        .holders()
        .next()
        .unwrap()
        .id;
    w.state.npcs.get_mut(&2).unwrap().health = 0; // Authored external source death.
    for _ in 2..20 {
        step(&mut w, &mut a);
        assert_eq!(reopen(&w).state_digest(), w.state_digest());
    }
    assert_eq!(w.state().npcs[&2].id.generation, 1);
    assert_eq!(
        w.state()
            .player_slow
            .as_ref()
            .unwrap()
            .holders()
            .next()
            .unwrap()
            .caster
            .generation,
        0
    );
    step(&mut w, &mut a);
    let replacement = w
        .state()
        .player_slow
        .as_ref()
        .unwrap()
        .holders()
        .next()
        .unwrap();
    assert!(replacement.id > old);
    assert_eq!(replacement.caster.generation, 1);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}

#[test]
fn replaced_retained_holder_cannot_claim_an_id_before_the_source_allocator_start() {
    let mut w = world(vec![trigger(1, true), trigger(1, true)]);
    step(&mut w, &mut fixture::SyntheticNpcSpellAuthority::default());
    let mut checkpoint = w.checkpoint().unwrap();
    let mut state = serde_json::to_value(&checkpoint.state).unwrap();
    let slow = &mut state["player_slow"];
    slow["next_holder_id"] = (slow["next_holder_id"].as_u64().unwrap() - 1).into();
    for holder in slow["holders"].as_array_mut().unwrap() {
        holder["id"] = (holder["id"].as_u64().unwrap() - 1).into();
    }
    for life in state["npcs"]["2"]["events"]["spells"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        life["applied_holder"] = (life["applied_holder"].as_u64().unwrap() - 1).into();
    }
    checkpoint.state = serde_json::from_value(state).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v26\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v26\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    assert!(
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), checkpoint).is_err()
    );
}
