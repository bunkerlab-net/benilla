// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-shaped synthetic metadata only. No production capability is created.
use super::{synthetic_fixture::*, *};
use benilla_sim::player_death as d;
fn profile_with_death() -> Profile {
    let mut p = initialized_profile();
    p.npcs[0].combat.as_mut().unwrap().minimum_damage = float(10000.0);
    p.npcs[0].combat.as_mut().unwrap().maximum_damage = float(10000.0);
    p.configure_player_death(
        d::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|d| (d, Some(false)))).unwrap(),
    )
    .unwrap();
    p
}
fn new(p: Profile, seed: u64) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [1; 16], player(), seed).unwrap()
}
fn reload(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
fn command(w: &NativeWorld, target: u64) -> OwnedInput {
    let mut i = cast(w, 60001);
    if let Command::Cast { target: t, .. } = &mut i.input.command {
        *t = w.state.npcs[&target].id;
    }
    i
}
fn run(w: &mut NativeWorld, to: u64, a: &mut impl Authority) {
    while w.state.tick < to {
        w.step(w.state.tick + 1, None, a).unwrap();
    }
}
#[test]
fn lethal_cancels_preparing_but_keeps_flight_death_loss_and_exact_resume() {
    let mut w = new(profile_with_death(), 7);
    let start = w.checkpoint().unwrap();
    let mut a = KernelAuthority::default();
    for tick in 1..=30 {
        let i = if [1, 6, 15].contains(&tick) && w.state.health > 0 {
            Some(command(&w, 2))
        } else {
            None
        };
        w.step(tick, i, &mut a).unwrap();
        if w.state.health == 0 {
            break;
        }
    }
    assert_eq!(w.state.health, 0);
    let death_tick = w.state.tick;
    let interrupted = w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .find_map(|event| match event {
            Event::Death(d::DeathAction::InterruptCurrentSpell { cast_id, .. }) => Some(*cast_id),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        w.state
            .player_events
            .spell_lifetime(interrupted)
            .unwrap()
            .finished,
        Some((w.state.now_ms, FinishedSpellReason::Interrupted))
    );
    assert_eq!(w.state.player_events.current_spell(), None);
    let record = w.last_tick().unwrap();
    assert!(record.events.iter().any(|e| matches!(
        e,
        Event::Death(d::DeathAction::InterruptCurrentSpell { .. })
    )));
    assert!(w
        .state
        .casts
        .values()
        .any(|c| matches!(c.execution.state(), spell::CastState::Traveling { .. })));
    assert!(w
        .state
        .casts
        .values()
        .all(|c| matches!(c.execution.state(), spell::CastState::Traveling { .. })));
    assert_eq!(
        w.state
            .combat
            .as_ref()
            .unwrap()
            .equipment
            .item(4)
            .unwrap()
            .current_durability,
        0
    );
    let events = &record.events;
    let health = events
        .iter()
        .position(|e| matches!(e, Event::Death(d::DeathAction::SetHealthZero)))
        .unwrap();
    let durability = events
        .iter()
        .position(|e| matches!(e, Event::Death(d::DeathAction::NpcDeathDurabilityLoss)))
        .unwrap();
    assert!(health < durability);
    assert_eq!(
        record
            .draws
            .iter()
            .filter(|d| matches!(
                d.purpose,
                DrawPurpose::DurabilityChance { .. }
                    | DrawPurpose::DefenseGain { .. }
                    | DrawPurpose::Pushback
            ))
            .count(),
        0
    );
    assert!(w.state.resources.caster.mana > 0);
    assert_eq!(
        w.state.npcs[&2].ai.victim_detached_at_ms(),
        Some(w.state.now_ms)
    );
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), start).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let mut restored = reload(&w);
    run(&mut w, death_tick + 1, &mut a);
    run(&mut restored, death_tick + 1, &mut a);
    assert_eq!(w.state_digest(), restored.state_digest());
    assert!(w.state.player_events.spell_lifetime(interrupted).is_none());
    let snapshot = w
        .state
        .combat
        .as_ref()
        .unwrap()
        .death
        .as_ref()
        .unwrap()
        .snapshot();
    assert_eq!(snapshot.phase, d::DeathPhase::Corpse);
    assert_eq!(snapshot.death_timer_ms, 359900);
    assert!(w
        .last_tick()
        .unwrap()
        .events
        .contains(&Event::Death(d::DeathAction::RootMovement)));
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::ReturningHome);
    reload(&w);
}
struct TwoNpcAuthority {
    inner: KernelAuthority,
    aggro_at: u64,
    fail_late: bool,
    b_queries: usize,
    tick: u64,
}
impl Authority for TwoNpcAuthority {
    fn resolve(&mut self, r: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if self.tick != r.binding.tick {
            self.tick = r.binding.tick;
            self.b_queries = 0;
        }
        if r.binding.npc.is_some_and(|n| n.spawn == 3) && matches!(r.query, Query::Npc { .. }) {
            self.b_queries += 1;
            if self.fail_late && r.binding.tick == self.aggro_at && self.b_queries == 2 {
                return Err(NativeError::AuthorityUnavailable("late synthetic geometry"));
            }
        }
        let mut result = self.inner.resolve(r)?;
        if r.binding.npc.is_some_and(|n| n.spawn == 2) {
            if let Facts::Npc(f) = &mut result.facts {
                f.can_initiate_attack = r.binding.tick >= self.aggro_at;
            }
        }
        Ok(result)
    }
}
fn two(aggro_at: u64) -> (NativeWorld, TwoNpcAuthority) {
    let mut p = profile_with_death();
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    let mut b = p.npcs[0].clone();
    b.ai.creature_id = 3;
    b.ai.reaction = ai::Reaction::Defensive;
    b.combat.as_mut().unwrap().minimum_damage = float(0.0);
    b.combat.as_mut().unwrap().maximum_damage = float(0.0);
    p.npcs.push(b);
    (
        new(p, 7),
        TwoNpcAuthority {
            inner: KernelAuthority::default(),
            aggro_at,
            fail_late: false,
            b_queries: 0,
            tick: 0,
        },
    )
}
#[test]
fn two_npc_event_time_damage_then_lethal_detaches_before_later_ai_and_rolls_back_errors() {
    let (mut w, mut a) = two(16);
    let i = command(&w, 3);
    w.step(1, Some(i), &mut a).unwrap();
    run(&mut w, 15, &mut a);
    let before = w.checkpoint().unwrap();
    a.fail_late = true;
    assert!(matches!(
        w.step(16, None, &mut a),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
    a.fail_late = false;
    a.tick = 0;
    w.step(16, None, &mut a).unwrap();
    assert_eq!(w.state.health, 0);
    assert!(w.state.npcs[&3].health < 3000);
    assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::ReturningHome);
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Combat);
    assert_eq!(w.state.npcs[&2].ai.threat(), ai::Real::ZERO);
    assert!(w.state.npcs[&2].ai.hostile_reference().is_none());
    assert_eq!(w.state.npcs[&2].ai.current_victim(), None);
    assert_eq!(w.state.npcs[&2].ai.target(), Some(SOLO_UNIT));
    assert_eq!(w.state.npcs[&2].ai.victim_detached_at_ms(), Some(1600));
    let r = w.last_tick().unwrap();
    let hit=r.events.iter().position(|e| matches!(e,Event::Spell { target, event:spell::SpellEvent::DirectDamage { .. }, .. } if target.spawn==3)).unwrap();
    let killed = r
        .events
        .iter()
        .position(|e| matches!(e, Event::Death(d::DeathAction::SetHealthZero)))
        .unwrap();
    assert!(hit < killed);
    reload(&w);
    run(&mut w, 20, &mut a);
}
#[test]
fn existing_threat_is_removed_and_dead_caster_dot_does_not_deadlock_next_frame() {
    let (mut w, mut a) = two(20);
    let i = command(&w, 3);
    w.step(1, Some(i), &mut a).unwrap();
    run(&mut w, 19, &mut a);
    assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::Combat);
    assert!(w.state.npcs[&3].ai.threat() != ai::Real::ZERO);
    w.step(20, None, &mut a).unwrap();
    assert_eq!(w.state.health, 0);
    // B's aura is removed by its normal same-frame evade, not by player's death.
    assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::ReturningHome);
    assert!(w.state.npcs[&3].ai.hostile_reference().is_none());
    assert_eq!(w.state.npcs[&3].ai.current_victim(), None);
    run(&mut w, 23, &mut a);
    reload(&w);
}
#[test]
fn corpse_timer_expiry_commits_unavailable_release_and_later_ticks_still_run() {
    let mut w = new(profile_with_death(), 7);
    let mut a = KernelAuthority::default();
    let i = command(&w, 2);
    w.step(1, Some(i), &mut a).unwrap();
    while w.state.health > 0 {
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    let death_tick = w.state.tick;
    let mana = w.state.resources.caster.mana;
    let mut replay = reload(&w);
    let receipt = w.checkpoint().unwrap();
    w.checkpoint_committed(&receipt).unwrap();
    let until = death_tick + 3602;
    while w.state.tick < until {
        w.step(w.state.tick + 1, None, &mut a).unwrap();
        if w.records().len() == 1000 || w.state.tick == until {
            replay.replay(w.records()).unwrap();
            assert_eq!(replay.state_digest(), w.state_digest());
            let checkpoint = w.checkpoint().unwrap();
            w.checkpoint_committed(&checkpoint).unwrap();
            replay = reload(&w);
        }
    }
    let d = w
        .state
        .combat
        .as_ref()
        .unwrap()
        .death
        .as_ref()
        .unwrap()
        .snapshot();
    assert!(d.release_due);
    assert_eq!(d.death_timer_ms, 0);
    assert_eq!(d.phase, d::DeathPhase::Corpse);
    assert_eq!(w.state.resources.caster.mana, mana);
    assert_eq!(w.state.position, point(0.0));
}
#[test]
fn resigned_death_clock_killer_and_detached_state_tamper_is_rejected() {
    let mut w = new(profile_with_death(), 7);
    let mut a = KernelAuthority::default();
    let i = command(&w, 2);
    w.step(1, Some(i), &mut a).unwrap();
    while w.state.health > 0 {
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    for key in ["timer", "tick", "killer", "detach", "health", "old"] {
        let mut v = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
        match key {
            "timer" => v["state"]["combat"]["death"]["death_timer_ms"] = 42.into(),
            "tick" => v["state"]["combat"]["death"]["died_at_tick"] = 1.into(),
            "killer" => v["state"]["combat"]["death_killer"]["generation"] = 999.into(),
            "detach" => v["state"]["npcs"]["2"]["ai"]["victim_detached_at_ms"] = 0.into(),
            "health" => v["state"]["health"] = 1.into(),
            "old" => v["state"]["version"] = 2.into(),
            _ => unreachable!(),
        }
        let mut checkpoint: Checkpoint = serde_json::from_value(v).unwrap();
        checkpoint.digest = crate::digest(b"benilla-native-state-v68\0", &checkpoint.state);
        checkpoint.checksum = crate::digest(
            b"benilla-native-checkpoint-v68\0",
            &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint).is_err(),
            "{key}"
        );
    }
}
#[test]
fn dead_caster_dot_ticks_before_killer_selects_no_target_next_frame() {
    let mut w = new(profile_with_death(), 7);
    let mut a = TwoNpcAuthority {
        inner: KernelAuthority::default(),
        aggro_at: 20,
        fail_late: false,
        b_queries: 0,
        tick: 0,
    };
    let i = command(&w, 2);
    w.step(1, Some(i), &mut a).unwrap();
    run(&mut w, 19, &mut a);
    w.step(20, None, &mut a).unwrap();
    assert_eq!(w.state.health, 0);
    assert!(!w.state.npcs[&2].auras.is_empty());
    let hp = w.state.npcs[&2].health;
    w.step(21, None, &mut a).unwrap();
    assert!(w.state.npcs[&2].health < hp);
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::Spell {
            event: spell::SpellEvent::PeriodicDamage { .. },
            ..
        }
    )));
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::ReturningHome);
    reload(&w);
}
#[test]
fn receipt_prelude_preserves_same_frame_npc_corpse_deferral() {
    let mut p = profile_with_death();
    p.npcs[0].max_health = 1;
    let mut w = new(p, 7);
    let mut a = KernelAuthority::default();
    let i = command(&w, 2);
    w.step(1, Some(i), &mut a).unwrap();
    run(&mut w, 16, &mut a);
    assert_eq!(w.state.npcs[&2].health, 0);
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Corpse);
    let value = serde_json::to_value(w.state.npcs[&2].ai.checkpoint()).unwrap();
    assert_eq!(value["death_at_ms"], 1600);
    assert_eq!(value["now_ms"], 1600);
    reload(&w);
    run(&mut w, 21, &mut a);
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Despawned);
    reload(&w);
}
#[test]
fn receipt_token_rejects_another_same_id_prepared_state_before_emitting_effects() {
    use ai::movement::Revision;
    let p = profile_with_death().npcs[0].ai.clone();
    let observation = |receipts| ai::Observation {
        sequence: 1,
        elapsed_ms: 100,
        geometry_revision: Revision([5; 32]),
        target: None,
        can_initiate_attack: false,
        casting_non_melee: false,
        stationary_melee_refresh_eligible: false,
        creature_moving_and_running: false,
        movement: None,
        receipts,
    };
    let mut died = ai::SoloCreatureAi::new(p.clone()).unwrap();
    let death = observation(vec![ai::Receipt {
        id: 1,
        event: ai::CombatEvent::Died,
    }]);
    let token = died.begin_receipts(&death).unwrap();
    let mut living = ai::SoloCreatureAi::new(p).unwrap();
    let empty = observation(vec![]);
    let _own = living.begin_receipts(&empty).unwrap();
    let before = living.checkpoint();
    let mut called = false;
    assert!(living
        .finish_receipts(empty, token, &mut |_, _, _| {
            called = true;
            Ok(ai::MainhandResult::Unchanged)
        })
        .is_err());
    assert!(!called);
    assert_eq!(living.checkpoint(), before);
    assert_eq!(living.phase(), ai::Phase::Idle);
}

#[test]
fn rehashed_owned_hostile_reference_forgery_is_rejected() {
    let (mut w, mut a) = two(20);
    let i = command(&w, 3);
    w.step(1, Some(i), &mut a).unwrap();
    run(&mut w, 19, &mut a);
    assert!(w.state.npcs[&3].ai.hostile_reference().is_some());
    for case in [
        "missing",
        "target",
        "invalid_online",
        "offline_current",
        "detach",
        "version",
    ] {
        let mut v = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
        let ai = &mut v["state"]["npcs"]["3"]["ai"];
        match case {
            "missing" => ai["hostile_references"]["reference"] = serde_json::Value::Null,
            "target" => ai["hostile_references"]["reference"]["target_guid"] = 42.into(),
            "invalid_online" => ai["hostile_references"]["reference"]["valid"] = false.into(),
            "offline_current" => {
                ai["hostile_references"]["reference"]["membership"] = "Offline".into()
            }
            "detach" => ai["victim_detached_at_ms"] = w.state.now_ms.into(),
            "version" => ai["version"] = 4.into(),
            _ => unreachable!(),
        }
        let mut checkpoint: Checkpoint = serde_json::from_value(v).unwrap();
        checkpoint.digest = crate::digest(b"benilla-native-state-v68\0", &checkpoint.state);
        checkpoint.checksum = crate::digest(
            b"benilla-native-checkpoint-v68\0",
            &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint).is_err(),
            "{case}"
        );
    }
}

#[test]
fn supported_player_liveness_preserves_first_life_and_rejects_inconsistent_operands() {
    let mut w = new(profile_with_death(), 7);
    assert!(w.state.player_alive());
    // This is an isolated inconsistent observation, not an admitted zero-HP life.
    let mut transient = w.state.clone();
    transient.health = 0;
    assert!(transient
        .combat
        .as_ref()
        .unwrap()
        .death
        .as_ref()
        .unwrap()
        .is_alive());
    assert!(!transient.player_alive());
    let mut a = KernelAuthority::default();
    let i = command(&w, 2);
    w.step(1, Some(i), &mut a).unwrap();
    for _ in 0..200 {
        if !w.state.player_alive() {
            break;
        }
        w.step(w.state.tick + 1, None, &mut a).unwrap();
    }
    assert!(!w.state.player_alive());
    assert_eq!(w.state.health, 0);
    assert!(!reload(&w).state.player_alive());
    let mut malformed = w.state.clone();
    malformed.health = 1;
    assert!(!malformed.player_alive());
    // Positive health is not a source-owned ghost, and the old strict restore
    // still rejects it even with newly computed envelope hashes.
    let mut value = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    value["state"]["health"] = 1.into();
    let mut checkpoint: Checkpoint = serde_json::from_value(value).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v68\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v68\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint).is_err());
    let mut legacy = new(profile(), 7).state.clone();
    assert!(legacy.player_alive());
    legacy.health = 0;
    assert!(!legacy.player_alive());
}
