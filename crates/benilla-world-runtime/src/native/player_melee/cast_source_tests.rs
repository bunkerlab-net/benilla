// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored raw Fireball-shaped rows, not native content or stock-value claims.
use super::*;
use crate::native::{
    synthetic_fixture, synthetic_quest_fixture, synthetic_spell_fixture,
    synthetic_two_owner_fixture::CompleteAuthority,
};

#[derive(Default)]
struct CastAuthority {
    scene: CompleteAuthority,
    spell: synthetic_spell_fixture::SpatialOnly,
}
impl Authority for CastAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(
            request.query,
            Query::SourceSpell(_) | Query::SourceLocation(_)
        ) {
            self.spell.resolve(request)
        } else {
            self.scene.resolve(request)
        }
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        if matches!(reply.request.query, Query::SourceSpell(_)) {
            self.spell.observe_derived(reply)
        } else {
            self.scene.observe_derived(reply)
        }
    }
}
fn world(health: u32) -> NativeWorld {
    NativeWorld::synthetic_component_test(
        super::tests::source_fireball_profile(health),
        [1; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap()
}
fn step(world: &mut NativeWorld, authority: &mut CastAuthority, command: Option<Command>) {
    let description = format!("{command:?}");
    let input = command.map(|command| synthetic_quest_fixture::command(world, command));
    let tick = world.state.tick + 1;
    world.step(tick, input, authority).unwrap_or_else(|error| {
        panic!("source cast tick {tick}, command {description}: {error:?}")
    });
}
fn restore(world: &NativeWorld) {
    let checkpoint = Checkpoint::decode(&world.checkpoint().unwrap().encode().unwrap()).unwrap();
    let restored =
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), checkpoint).unwrap();
    assert_eq!(world.state_digest(), restored.state_digest());
}
#[test]
fn source_fireball_keeps_periodic_effect_and_runs_with_mainhand() {
    let mut world = world(100);
    let target = world.state.npcs[&2].id;
    let mut authority = CastAuthority::default();
    step(
        &mut world,
        &mut authority,
        Some(Command::AttackStart { target }),
    );
    step(
        &mut world,
        &mut authority,
        Some(Command::Cast {
            ability: 133,
            target,
        }),
    );
    let mut launched = false;
    let mut direct = false;
    let mut periodic = false;
    let mut aura_before = world.state.npcs[&2].auras.contains_key(&133);
    let mut melee_with_aura = false;
    for _ in 0..60 {
        for event in &world.last_tick().unwrap().events {
            if let Event::Spell {
                ability: 133,
                event,
                ..
            } = event
            {
                launched |= matches!(event, spell::SpellEvent::Launched { .. });
                direct |= matches!(event, spell::SpellEvent::DirectDamage { .. });
                periodic |= matches!(event, spell::SpellEvent::PeriodicDamage { .. });
            }
        }
        let aura_after = world.state.npcs[&2].auras.contains_key(&133);
        if aura_before
            && aura_after
            && world
                .last_tick()
                .unwrap()
                .events
                .iter()
                .any(|event| matches!(event, Event::PlayerMelee { .. }))
        {
            melee_with_aura = true;
            assert_eq!(
                world.state.npcs[&2].auras[&133].health,
                world.state.npcs[&2].health
            );
        }
        restore(&world);
        if launched && direct && periodic && melee_with_aura {
            break;
        }
        aura_before = aura_after;
        step(&mut world, &mut authority, None);
    }
    assert!(launched && direct && periodic && melee_with_aura);
    assert!(world
        .state
        .combat
        .as_ref()
        .unwrap()
        .melee_timers
        .melee_attacking());
}

#[test]
fn source_direct_kill_stops_attacking_exact_target_before_mainhand() {
    let mut world = world(1);
    let target = world.state.npcs[&2].id;
    let mut authority = CastAuthority::default();
    step(
        &mut world,
        &mut authority,
        Some(Command::Cast {
            ability: 133,
            target,
        }),
    );
    step(
        &mut world,
        &mut authority,
        Some(Command::AttackStart { target }),
    );
    let mut stopped = false;
    for _ in 0..40 {
        let events = &world.last_tick().unwrap().events;
        if world.state.npcs[&2].health == 0 {
            assert!(events.iter().any(|event| matches!(event, Event::Spell { event: spell::SpellEvent::DirectDamage { effective, .. }, .. } if *effective > 0)));
            assert_eq!(events.iter().filter(|event| matches!(event, Event::PlayerAttackStop { target: actual } if *actual == target)).count(), 1);
            assert!(events
                .iter()
                .any(|event| matches!(event, Event::PlayerAttackCancel)));
            assert!(!events
                .iter()
                .any(|event| matches!(event, Event::PlayerMelee { .. })));
            assert_eq!(
                world.state.combat.as_ref().unwrap().melee_timers.victim(),
                None
            );
            restore(&world);
            stopped = true;
            break;
        }
        step(&mut world, &mut authority, None);
    }
    assert!(stopped);
}

#[test]
fn source_cast_kills_other_target_without_stopping_mainhand_victim() {
    let mut world = world(100);
    let victim = world.state.npcs[&2].id;
    let spell_target = world.state.npcs[&3].id;
    let mut authority = CastAuthority::default();
    step(
        &mut world,
        &mut authority,
        Some(Command::AttackStart { target: victim }),
    );
    step(
        &mut world,
        &mut authority,
        Some(Command::Cast {
            ability: 133,
            target: spell_target,
        }),
    );
    let mut hit = false;
    for _ in 0..40 {
        hit |= world.last_tick().unwrap().events.iter().any(|event| matches!(event, Event::Spell { target, event: spell::SpellEvent::DirectDamage { .. }, .. } if *target == spell_target));
        assert_eq!(
            world.state.combat.as_ref().unwrap().melee_timers.victim(),
            Some(victim)
        );
        restore(&world);
        if hit {
            break;
        }
        step(&mut world, &mut authority, None);
    }
    assert!(hit);
    assert_eq!(world.state.npcs[&3].health, 0);
    assert!(!world
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|event| matches!(
            event,
            Event::PlayerAttackStop { .. } | Event::PlayerAttackCancel
        )));
}

#[test]
fn source_periodic_kill_stops_mainhand_without_resetting_its_timer() {
    // Keep the exact seed/commands and observe the real direct damage first.
    // Author the lethal fixture one HP above that damage, so the first periodic
    // tick, not a later mainhand, exercises the target-death callback.
    let mut calibration = world(100);
    let calibration_target = calibration.state.npcs[&2].id;
    let mut calibration_authority = CastAuthority::default();
    step(
        &mut calibration,
        &mut calibration_authority,
        Some(Command::Cast {
            ability: 133,
            target: calibration_target,
        }),
    );
    step(
        &mut calibration,
        &mut calibration_authority,
        Some(Command::AttackStart {
            target: calibration_target,
        }),
    );
    let direct = loop {
        if let Some(amount) = calibration
            .last_tick()
            .unwrap()
            .events
            .iter()
            .find_map(|event| match event {
                Event::Spell {
                    event: spell::SpellEvent::DirectDamage { amount, .. },
                    ..
                } => Some(*amount),
                _ => None,
            })
        {
            break amount;
        }
        assert!(
            calibration.state.tick < 40,
            "no direct hit in fixed seed calibration"
        );
        step(&mut calibration, &mut calibration_authority, None);
    };
    assert!(direct > 0 && direct < 100);
    let mut world = world(direct + 1);
    let target = world.state.npcs[&2].id;
    let mut authority = CastAuthority::default();
    step(
        &mut world,
        &mut authority,
        Some(Command::Cast {
            ability: 133,
            target,
        }),
    );
    step(
        &mut world,
        &mut authority,
        Some(Command::AttackStart { target }),
    );
    let mut killed = false;
    for _ in 0..40 {
        let before = world
            .state
            .combat
            .as_ref()
            .unwrap()
            .melee_timers
            .remaining_ms();
        step(&mut world, &mut authority, None);
        restore(&world);
        for event in &world.last_tick().unwrap().events {
            if let Event::Spell {
                event:
                    spell::SpellEvent::DirectDamage {
                        amount, effective, ..
                    },
                ..
            } = event
            {
                assert_eq!((*amount, *effective), (direct, direct));
            }
        }
        if world.state.npcs[&2].health == 0 {
            let events = &world.last_tick().unwrap().events;
            assert!(events.iter().any(|event| matches!(event, Event::Spell { event: spell::SpellEvent::PeriodicDamage { effective, .. }, .. } if *effective > 0)), "direct calibration {direct}, lethal tick {}, events {events:?}", world.state.tick);
            assert_eq!(events.iter().filter(|event| matches!(event, Event::PlayerAttackStop { target: actual } if *actual == target)).count(), 1);
            assert_eq!(
                world.state.combat.as_ref().unwrap().melee_timers.victim(),
                None
            );
            assert_eq!(
                world
                    .state
                    .combat
                    .as_ref()
                    .unwrap()
                    .melee_timers
                    .remaining_ms(),
                before.map(|timer| timer.saturating_sub(world.profile.tick_ms))
            );
            killed = true;
            break;
        }
    }
    assert!(killed);
}

#[test]
fn source_cast_counts_down_while_blocked_and_resets_then_decrements_on_launch() {
    let mut world = world(100);
    let target = world.state.npcs[&2].id;
    let mut authority = CastAuthority::default();
    step(
        &mut world,
        &mut authority,
        Some(Command::AttackStart { target }),
    );
    let before = world
        .state
        .combat
        .as_ref()
        .unwrap()
        .melee_timers
        .remaining_ms();
    assert!(before[0] > 0);
    step(
        &mut world,
        &mut authority,
        Some(Command::Cast {
            ability: 133,
            target,
        }),
    );
    assert_eq!(
        world
            .state
            .combat
            .as_ref()
            .unwrap()
            .melee_timers
            .remaining_ms(),
        before.map(|timer| timer.saturating_sub(world.profile.tick_ms))
    );
    assert!(super::casting::blocks(&world.state).unwrap());
    let mut launched = false;
    for _ in 0..100 {
        let before = world
            .state
            .combat
            .as_ref()
            .unwrap()
            .melee_timers
            .remaining_ms();
        step(&mut world, &mut authority, None);
        let events = &world.last_tick().unwrap().events;
        assert!(!events.iter().any(|event| matches!(
            event,
            Event::PlayerMelee { .. } | Event::PlayerSwingError { .. }
        )));
        launched = events.iter().any(|event| {
            matches!(
                event,
                Event::Spell {
                    event: spell::SpellEvent::Launched { .. },
                    ..
                }
            )
        });
        if launched {
            let combat = world.state.combat.as_ref().unwrap();
            let interval = world
                .profile
                .combat
                .as_ref()
                .unwrap()
                .source_attack_times_for_recipe(&combat.recipe.snapshot())
                .unwrap()[0];
            assert_eq!(
                combat.melee_timers.remaining_ms()[0],
                interval.saturating_sub(world.profile.tick_ms)
            );
            assert!(world
                .state
                .casts
                .values()
                .any(|cast| matches!(cast.execution.state(), spell::CastState::Traveling { .. })));
            assert!(!super::casting::blocks(&world.state).unwrap());
            restore(&world);
            break;
        }
        assert!(super::casting::blocks(&world.state).unwrap());
        assert_eq!(
            world
                .state
                .combat
                .as_ref()
                .unwrap()
                .melee_timers
                .remaining_ms(),
            before.map(|timer| timer.saturating_sub(world.profile.tick_ms))
        );
    }
    assert!(launched);
}

#[test]
fn old_projectile_kill_stops_victim_but_preserves_newer_current_cast() {
    let mut world = world(1);
    let target = world.state.npcs[&2].id;
    let newer_target = world.state.npcs[&3].id;
    let mut authority = CastAuthority::default();
    step(
        &mut world,
        &mut authority,
        Some(Command::Cast {
            ability: 133,
            target,
        }),
    );
    step(
        &mut world,
        &mut authority,
        Some(Command::AttackStart { target }),
    );
    let impact_at = loop {
        if let Some(impact) =
            world
                .state
                .casts
                .values()
                .find_map(|cast| match cast.execution.state() {
                    spell::CastState::Traveling { impact_at_ms } => Some(impact_at_ms),
                    _ => None,
                })
        {
            break impact;
        }
        assert!(world.state.tick < 20);
        step(&mut world, &mut authority, None);
    };
    while world.state.now_ms + u64::from(world.profile.tick_ms) < impact_at {
        step(&mut world, &mut authority, None);
    }
    let newer_id = world.state.next_cast_id;
    step(
        &mut world,
        &mut authority,
        Some(Command::Cast {
            ability: 133,
            target: newer_target,
        }),
    );
    assert_eq!(world.state.npcs[&2].health, 0);
    assert_eq!(
        world.state.combat.as_ref().unwrap().melee_timers.victim(),
        None
    );
    assert_eq!(world.state.player_events.current_spell(), Some(newer_id));
    assert!(matches!(
        world.state.casts[&newer_id].execution.state(),
        spell::CastState::Casting { .. }
    ));
    assert!(super::casting::blocks(&world.state).unwrap());
    restore(&world);
}
