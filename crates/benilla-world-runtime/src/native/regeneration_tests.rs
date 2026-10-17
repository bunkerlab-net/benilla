// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored component fixtures, not native-content admission or a real database.
use super::{synthetic_fixture::*, *};
use benilla_sim::creature_regeneration as regen;

#[derive(Default)]
struct AuthorityFixture {
    inner: SyntheticAuthority,
    aggro: bool,
    reject_spawn: Option<u64>,
    unreachable: bool,
}
impl Authority for AuthorityFixture {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if request
            .binding
            .npc
            .is_some_and(|id| Some(id.spawn) == self.reject_spawn)
        {
            return Err(NativeError::AuthorityUnavailable(
                "authored later NPC failure",
            ));
        }
        let mut result = self.inner.resolve(request)?;
        if let Facts::Npc(facts) = &mut result.facts {
            facts.can_attack_on_sight = self.aggro;
            facts.accessible = !self.unreachable;
        }
        if self.unreachable
            && matches!(request.query, Query::Movement(ref r) if matches!(r.purpose, ai::movement::MovementPurpose::Chase { .. }))
        {
            if let Facts::Movement(outcome) = &mut result.facts {
                outcome.result =
                    ai::movement::MovementResult::Blocked(ai::movement::BlockReason::Obstacle);
            }
        }
        Ok(result)
    }
}
fn advance(w: &mut NativeWorld, count: u64, a: &mut impl Authority) {
    for _ in 0..count {
        w.step(w.state().tick + 1, None, a).unwrap();
    }
}
fn restore(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
fn authored(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], player(), 1234).unwrap()
}
#[test]
fn idle_regeneration_boundary_checkpoint_replay_caps_and_no_rng() {
    let mut w = world();
    w.state.npcs.get_mut(&2).unwrap().health = 1;
    let mut a = AuthorityFixture::default();
    let start = w.checkpoint().unwrap();
    assert_eq!(w.state().npcs[&2].regen_elapsed_ms, 0);
    advance(&mut w, 49, &mut a);
    assert_eq!(w.state().npcs[&2].health, 1);
    assert_eq!(w.state().npcs[&2].regen_elapsed_ms, 4900);
    let mut resumed = restore(&w);
    advance(&mut w, 1, &mut a);
    advance(&mut resumed, 1, &mut a);
    assert_eq!(w.state_digest(), resumed.state_digest());
    assert_eq!(w.state().npcs[&2].health, 334);
    assert_eq!(w.state().npcs[&2].regen_elapsed_ms, 0);
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::NpcHealthRegenerated {
            before: 1,
            after: 334,
            ..
        }
    )));
    assert_eq!(w.state().rng_counter, 0);
    advance(&mut w, 150, &mut a);
    assert_eq!(w.state().npcs[&2].health, 1000);
    assert_eq!(w.state().npcs[&2].regen_elapsed_ms, 0);
    assert!(!w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::NpcHealthRegenerated { .. })));
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), start).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(w.state_digest(), replay.state_digest());
}
#[test]
fn same_update_aggro_blocks_health_and_still_consumes_pulse() {
    let mut p = profile();
    p.npcs[0].ai.spawn = point(1.0);
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    let mut w = authored(p);
    w.state.npcs.get_mut(&2).unwrap().health = 10;
    let mut a = AuthorityFixture::default();
    advance(&mut w, 49, &mut a);
    a.aggro = true;
    advance(&mut w, 1, &mut a);
    assert_eq!(w.state().npcs[&2].ai.phase(), ai::Phase::Combat);
    assert_eq!(w.state().npcs[&2].health, 10);
    assert_eq!(w.state().npcs[&2].regen_elapsed_ms, 0);
    assert!(!w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::NpcHealthRegenerated { .. })));
}
#[test]
fn evade_after_damage_heals_on_pulse_without_home_reset_or_instant_refill() {
    let mut p = profile();
    p.npcs[0].ai.spawn = point(1.0);
    p.npcs[0].ai.reaction = ai::Reaction::Defensive;
    let mut w = authored(p);
    let mut a = AuthorityFixture::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 39, &mut a);
    let damaged = w.state().npcs[&2].health;
    assert!(damaged < 1000);
    let input = OwnedInput {
        authenticated_owner: player().owner,
        input: Input {
            world: w.state().world,
            tick: 41,
            player: player(),
            sequence: w.state().next_sequence,
            command: Command::Move {
                destination: point(100.0),
            },
        },
    };
    w.step(41, Some(input), &mut a).unwrap();
    // Failed chase starts the source unreachable timer, whose last five seconds
    // permit regeneration even while the NPC remains in combat.
    a.unreachable = true;
    let mut healed_during_evade = false;
    for _ in 0..200 {
        let previous = w.state().npcs[&2].health;
        advance(&mut w, 1, &mut a);
        let npc = &w.state().npcs[&2];
        if npc.health > previous {
            assert_eq!(npc.regen_elapsed_ms, 0);
            assert!(npc.ai.phase() != ai::Phase::Combat || npc.ai.is_evade_regeneration());
            healed_during_evade = true;
        }
        if npc.ai.phase() == ai::Phase::Idle && npc.health == 1000 {
            break;
        }
    }
    assert!(healed_during_evade);
    let resumed = restore(&w);
    assert_eq!(resumed.state_digest(), w.state_digest());
}
#[test]
fn lethal_pulse_boundary_corpse_respawn_freeze_and_preserve_constructor_timer() {
    let mut p = profile();
    p.npcs[0].max_health = 1;
    let mut w = authored(p);
    let mut a = AuthorityFixture::default();
    advance(&mut w, 7, &mut a);
    let command = cast(&w, 60001);
    w.step(8, Some(command), &mut a).unwrap();
    let mut before = 100;
    while w.state().npcs[&2].health > 0 {
        before = w.state().npcs[&2].regen_elapsed_ms;
        advance(&mut w, 1, &mut a);
    }
    assert_eq!(w.state().tick, 50);
    assert_eq!(before, 4900);
    assert_eq!(w.state().npcs[&2].regen_elapsed_ms, before);
    let mut resumed = restore(&w);
    while w.state().npcs[&2].id.generation == 0 {
        advance(&mut w, 1, &mut a);
        advance(&mut resumed, 1, &mut a);
        assert_eq!(w.state().npcs[&2].regen_elapsed_ms, before);
        assert_eq!(w.state_digest(), resumed.state_digest());
    }
    assert_eq!(w.state().npcs[&2].health, 1);
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::Npc {
            effect: ai::Effect::Respawned { .. },
            ..
        }
    )));
    advance(&mut w, 1, &mut a);
    assert_eq!(w.state().npcs[&2].regen_elapsed_ms, (before + 100) % 5000);
}
#[test]
fn later_npc_failure_rolls_back_earlier_regeneration_and_journal() {
    let mut p = profile();
    let mut other = p.npcs[0].clone();
    other.ai.creature_id = 3;
    p.npcs.push(other);
    let mut w = authored(p);
    w.state.npcs.get_mut(&2).unwrap().health = 10;
    let mut a = AuthorityFixture::default();
    advance(&mut w, 49, &mut a);
    let cp = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    a.reject_spawn = Some(3);
    assert!(w.step(50, None, &mut a).is_err());
    assert_eq!(w.checkpoint().unwrap(), cp);
    assert_eq!(w.records(), records);
    a.reject_spawn = None;
    advance(&mut w, 1, &mut a);
    assert_eq!(w.state().npcs[&2].health, 343);
}
#[test]
fn profile_requires_known_health_and_power_policy_and_binds_it_to_identity() {
    let mut p = profile();
    let before = p.digest();
    p.npcs[0].health_regeneration.regenerate_stats = 0;
    assert_ne!(p.digest(), before);
    assert!(p.validate().is_ok());
    p.npcs[0].health_regeneration.power = regen::PowerEvidence::Unknown;
    assert!(p.validate().is_err());
    p.npcs[0].health_regeneration.power = regen::PowerEvidence::Known {
        power_type: 0,
        max_power: 100,
    };
    p.npcs[0].health_regeneration.regenerate_stats = 10;
    assert!(p.validate().is_err());
    p.npcs[0].health_regeneration.regenerate_stats = 2;
    assert!(p.validate().is_ok());
}

#[test]
fn disabled_health_and_interaction_flags_preserve_damage_but_advance_timer() {
    for (regenerate_stats, creature_type_flags) in [(0, 0), (1, 0), (2, 0x40000)] {
        let mut p = profile();
        p.npcs[0].health_regeneration.regenerate_stats = regenerate_stats;
        p.npcs[0].health_regeneration.creature_type_flags = creature_type_flags;
        let mut w = authored(p);
        w.state.npcs.get_mut(&2).unwrap().health = 5;
        advance(&mut w, 51, &mut AuthorityFixture::default());
        assert_eq!(w.state().npcs[&2].health, 5);
        assert_eq!(w.state().npcs[&2].regen_elapsed_ms, 100);
        assert!(w
            .records()
            .iter()
            .flat_map(|r| &r.events)
            .all(|e| !matches!(e, Event::NpcHealthRegenerated { .. })));
        assert_eq!(restore(&w).state_digest(), w.state_digest());
    }
}

#[test]
fn restore_rejects_unreachable_accumulator_bounds_grid_and_genesis() {
    let mut w = world();
    w.state.npcs.get_mut(&2).unwrap().regen_elapsed_ms = 100;
    assert!(NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        w.checkpoint().unwrap()
    )
    .is_err());
    w.state.npcs.get_mut(&2).unwrap().regen_elapsed_ms = 0;
    advance(&mut w, 49, &mut AuthorityFixture::default());
    for invalid in [1, 4999, 5000, u32::MAX] {
        w.state.npcs.get_mut(&2).unwrap().regen_elapsed_ms = invalid;
        assert!(NativeWorld::restore_synthetic_component_test(
            w.profile().clone(),
            w.checkpoint().unwrap()
        )
        .is_err());
    }
}

#[test]
fn original_dot_then_unreachable_evade_absorbs_without_invalid_receipt_and_reopens() {
    for duration_ms in [2000, 20000] {
        let mut p = profile();
        p.npcs[0].ai.spawn = point(1.0);
        p.npcs[0].ai.reaction = ai::Reaction::Defensive;
        let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
            unreachable!()
        };
        let spell::MageEffect::PeriodicDamage {
            duration_ms: duration,
            ..
        } = &mut spec.effects[1]
        else {
            unreachable!()
        };
        *duration = duration_ms;
        let mut w = authored(p);
        let mut a = AuthorityFixture::default();
        let command = cast(&w, 60001);
        w.step(1, Some(command), &mut a).unwrap();
        advance(&mut w, 29, &mut a);
        assert!(!w.state().npcs[&2].auras.is_empty());
        let command = OwnedInput {
            authenticated_owner: player().owner,
            input: Input {
                world: w.state().world,
                tick: 31,
                player: player(),
                sequence: w.state().next_sequence,
                command: Command::Move {
                    destination: point(100.0),
                },
            },
        };
        w.step(31, Some(command), &mut a).unwrap();
        a.unreachable = true;
        advance(&mut w, 2, &mut a);
        assert!(w.state().npcs[&2].ai.is_evading());
        assert!(!w.state().npcs[&2].auras.is_empty());
        let anchor = w.checkpoint().unwrap();
        let mut resumed = restore(&w);
        let mut absorbed = 0;
        let mut entered_home = false;
        for _ in 0..110 {
            let receipt = w.state().npcs[&2].ai.last_receipt_id();
            let threat = w.state().npcs[&2].ai.threat();
            let rng = w.state().rng_counter;
            let before = w.state().npcs[&2].health;
            advance(&mut w, 1, &mut a);
            advance(&mut resumed, 1, &mut a);
            assert_eq!(w.state_digest(), resumed.state_digest());
            let events = &w.last_tick().unwrap().events;
            if let Some(healed) = events
                .iter()
                .position(|e| matches!(e, Event::NpcHealthRegenerated { .. }))
            {
                assert!(events
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| matches!(e, Event::Npc { .. }))
                    .all(|(index, _)| index < healed));
            }
            if events.iter().any(|e|matches!(e,Event::Spell{event:spell::SpellEvent::PeriodicDamage{amount:0,effective:0,absorb,resist:0,..},..} if *absorb>0)) {
                absorbed += 1;
                assert_eq!(w.state().npcs[&2].ai.last_receipt_id(), receipt);
                assert_eq!(w.state().rng_counter, rng);
                assert!(w.last_tick().unwrap().draws.is_empty());
                if !events.iter().any(|e| matches!(e, Event::Npc { effect: ai::Effect::EvadeStarted(_), .. })) {
                    assert_eq!(w.state().npcs[&2].ai.threat(), threat);
                }
                assert!(w.state().npcs[&2].health >= before);
                assert!(!events.iter().any(|e|matches!(e,Event::Spell{event:spell::SpellEvent::Immune,..})));
            }
            if let Some(evade) = events.iter().position(|e| {
                matches!(
                    e,
                    Event::Npc {
                        effect: ai::Effect::EvadeStarted(_),
                        ..
                    }
                )
            }) {
                entered_home = true;
                assert!(w.state().npcs[&2].auras.is_empty());
                if duration_ms == 20000 {
                    let removed = events
                        .iter()
                        .position(|e| {
                            matches!(
                                e,
                                Event::Spell {
                                    event: spell::SpellEvent::AuraRemoved,
                                    ..
                                }
                            )
                        })
                        .unwrap();
                    assert!(removed < evade);
                }
            }
        }
        assert!(absorbed > 0);
        assert!(entered_home);
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), anchor).unwrap();
        let records = w
            .records()
            .iter()
            .filter(|r| r.tick >= 34)
            .cloned()
            .collect::<Vec<_>>();
        replay.replay(&records).unwrap();
        assert_eq!(replay.state_digest(), w.state_digest());
    }
}
