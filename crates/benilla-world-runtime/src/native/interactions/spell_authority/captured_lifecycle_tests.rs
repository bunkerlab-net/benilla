// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual-shaped authored recipes exercise the existing source host and scheduler.
use super::*;
use crate::native::{
    synthetic_captured_spell_fixture as captured, synthetic_fixture as f,
    synthetic_spell_fixture as spatial,
};

fn advance(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state.tick + 1, None, a).unwrap();
}
fn start(w: &mut NativeWorld, a: &mut impl Authority) {
    let input = f::cast(w, 133);
    w.step(w.state.tick + 1, Some(input), a).unwrap();
}
fn events(w: &NativeWorld) -> Vec<spell::SpellEvent> {
    w.last_tick()
        .unwrap()
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Spell { event, .. } => Some(*event),
            _ => None,
        })
        .collect()
}
fn released(seed: u64, health: u32) -> (NativeWorld, spatial::SpatialOnly) {
    let mut p = captured::captured_fireball_profile();
    p.npcs[0].max_health = health;
    // Authored NPC timing leaves a replacement cast window without disabling pushback.
    p.npcs[0].ai.mainhand_interval_ms = 10000;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], f::player(), seed).unwrap();
    let mut a = spatial::spatial_only();
    let mana = w.state.resources.caster.mana;
    start(&mut w, &mut a);
    assert!(events(&w).contains(&spell::SpellEvent::Started { cast_ms: 1500 }));
    assert_eq!(w.state.resources.caster.mana, mana);
    assert_eq!(
        w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
        [0, 0]
    );
    while matches!(
        w.state.casts[&1].execution.state(),
        spell::CastState::Casting { .. }
    ) {
        let before = w.state.resources.caster.mana;
        advance(&mut w, &mut a);
        if matches!(
            w.state.casts[&1].execution.state(),
            spell::CastState::Casting { .. }
        ) {
            assert_eq!(w.state.resources.caster.mana, before);
        } else {
            assert_eq!(w.state.resources.caster.mana, before - 30);
            assert_eq!(
                events(&w)
                    .iter()
                    .filter(|e| **e == spell::SpellEvent::ManaSpent(30))
                    .count(),
                1
            );
        }
        assert!(w.state.tick <= 20);
    }
    let launch = w.state.casts[&1].execution.launch_context().unwrap();
    assert_eq!(launch.rolls.direct[1], None);
    assert!(w
        .last_tick()
        .unwrap()
        .draws
        .iter()
        .all(|d| !matches!(d.purpose, DrawPurpose::Periodic { .. })));
    let life = w.state.player_events.spell_lifetime(1).unwrap();
    let delay = spell::projectile_delay(launch.distance_squared, f::float(24.0)).unwrap();
    assert!(delay >= 208);
    assert_eq!(
        life.travel_deadline_ms,
        Some(life.delay_start_ms.unwrap() + u64::from(delay))
    );
    assert_eq!(
        w.state.combat.as_ref().unwrap().melee_timers.remaining_ms(),
        [1900, 0]
    );
    (w, a)
}
#[test]
fn generated_release_damage_and_terminal_ticks_retain_source_order_and_draws() {
    let mut outcomes = [false; 2];
    for seed in 0..128 {
        let (mut w, mut a) = released(seed, 10000);
        let launch = w.state.casts[&1].execution.launch_context().unwrap();
        let resisted = launch.rolls.hit <= launch.resist_chance;
        if outcomes[usize::from(resisted)] {
            continue;
        }
        outcomes[usize::from(resisted)] = true;
        let mut transcript = vec![];
        let mut periodic_frames = vec![];
        while w.state.tick < 80 {
            advance(&mut w, &mut a);
            let frame = events(&w);
            if frame
                .iter()
                .any(|e| matches!(e, spell::SpellEvent::AuraApplied { .. }))
            {
                let aura = w.state.npcs[&2].auras[&133].aura().unwrap();
                assert_eq!((aura.amount, aura.duration_ms, aura.ticks), (1, 4000, 0));
                assert!(!frame
                    .iter()
                    .any(|e| matches!(e, spell::SpellEvent::PeriodicDamage { .. })));
                let applied = frame
                    .iter()
                    .position(|e| matches!(e, spell::SpellEvent::AuraApplied { .. }))
                    .unwrap();
                let direct = frame
                    .iter()
                    .position(|e| matches!(e, spell::SpellEvent::DirectDamage { .. }))
                    .unwrap();
                assert!(applied < direct);
            }
            if frame
                .iter()
                .any(|e| matches!(e, spell::SpellEvent::PeriodicDamage { .. }))
            {
                periodic_frames.push((
                    w.state.now_ms,
                    frame.contains(&spell::SpellEvent::AuraRemoved),
                ));
            }
            assert!(w
                .last_tick()
                .unwrap()
                .draws
                .iter()
                .all(|d| !matches!(d.purpose, DrawPurpose::Periodic { .. })));
            transcript.extend(frame);
        }
        if resisted {
            assert!(transcript.contains(&spell::SpellEvent::Resisted));
            assert!(!transcript
                .iter()
                .any(|e| matches!(e, spell::SpellEvent::AuraApplied { .. })));
        } else {
            assert_eq!(periodic_frames.len(), 2);
            assert_eq!(periodic_frames[1].0 - periodic_frames[0].0, 2000);
            assert!(periodic_frames[1].1);
            for event in &transcript {
                if let spell::SpellEvent::PeriodicDamage {
                    amount, effective, ..
                } = event
                {
                    assert_eq!((*amount, *effective), (1, 1));
                }
                if let spell::SpellEvent::DirectDamage {
                    amount, critical, ..
                } = event
                {
                    if *critical {
                        assert!([21, 22, 24, 25, 27, 28, 30, 31, 33].contains(amount));
                    } else {
                        assert!((14..=22).contains(amount));
                    }
                }
            }
        }
        assert!(w.state.npcs[&2].auras.values().all(|t| t.aura().is_none()));
        assert!(!a.derived.is_empty());
        assert!(a
            .queries
            .iter()
            .all(|r| !matches!(r.query, Query::Spell { .. })));
        if outcomes == [true; 2] {
            break;
        }
    }
    assert_eq!(outcomes, [true; 2]);
}
#[test]
fn generated_lethal_impact_and_visibility_change_cannot_leave_a_periodic_holder() {
    for visibility_failure in [false, true] {
        let (mut w, mut a) = (0..128)
            .find_map(|seed| {
                let (w, a) = released(seed, if visibility_failure { 10000 } else { 1 });
                let launch = w.state.casts[&1].execution.launch_context().unwrap();
                (launch.rolls.hit > launch.resist_chance).then_some((w, a))
            })
            .unwrap();
        if visibility_failure {
            a.impact_visibility = Some(ai::movement::Check::Failed);
        }
        let mut direct = false;
        for _ in 0..30 {
            advance(&mut w, &mut a);
            direct |= events(&w)
                .iter()
                .any(|e| matches!(e, spell::SpellEvent::DirectDamage { .. }));
            assert!(w.state.npcs[&2].auras.values().all(|t| t.aura().is_none()));
        }
        assert_eq!(direct, !visibility_failure);
    }
}
#[test]
fn generated_movement_cancellation_and_release_los_failure_do_not_spend_or_reset() {
    for movement in [false, true] {
        let mut w = NativeWorld::synthetic_component_test(
            captured::captured_fireball_profile(),
            [8; 16],
            f::player(),
            7,
        )
        .unwrap();
        let mut a = spatial::spatial_only();
        start(&mut w, &mut a);
        let mana = w.state.resources.caster.mana;
        if movement {
            let input = crate::native::synthetic_quest_fixture::command(
                &w,
                Command::MovementPose {
                    destination: f::point(0.25),
                    orientation: w.state.orientation,
                    movement_flags: benilla_sim::creature_motion::GroundMovementFlags::from_bits(1)
                        .unwrap(),
                },
            );
            w.step(w.state.tick + 1, Some(input), &mut a).unwrap();
        } else {
            a.line_of_sight = Some(ai::movement::Check::Failed);
        }
        for _ in 0..17 {
            advance(&mut w, &mut a);
        }
        assert!(w.state.casts.is_empty());
        assert_eq!(w.state.resources.caster.mana, mana);
        assert!(w.state.combat.as_ref().unwrap().melee_reset.is_none());
    }
}
#[test]
fn generated_same_caster_replaces_the_holder_and_unknown_geometry_rolls_back() {
    let replaced = (0..128).any(|seed| {
        let (mut w, mut a) = released(seed, 10000);
        let launch = w.state.casts[&1].execution.launch_context().unwrap();
        if launch.rolls.hit <= launch.resist_chance {
            return false;
        }
        while !events(&w)
            .iter()
            .any(|e| matches!(e, spell::SpellEvent::AuraApplied { .. }))
        {
            assert!(w.state.tick < 40);
            advance(&mut w, &mut a);
        }
        start(&mut w, &mut a);
        for _ in 0..30 {
            advance(&mut w, &mut a);
            if events(&w).contains(&spell::SpellEvent::AuraApplied { replaced: true }) {
                return true;
            }
        }
        false
    });
    assert!(
        replaced,
        "authored RNG must reach two successful same-caster impacts"
    );
    let mut fresh = NativeWorld::synthetic_component_test(
        captured::captured_fireball_profile(),
        [8; 16],
        f::player(),
        7,
    )
    .unwrap();
    let before = fresh.checkpoint().unwrap();
    let input = f::cast(&fresh, 133);
    struct Unavailable(spatial::SpatialOnly);
    impl Authority for Unavailable {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if matches!(
                request.query,
                Query::SourceSpell(SourceSpellQuery::LineOfSight(_))
            ) {
                return Err(NativeError::AuthorityUnavailable(
                    "authored geometry unavailable",
                ));
            }
            self.0.resolve(request)
        }
        fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
            self.0.observe_derived(reply)
        }
    }
    let mut unavailable = Unavailable(spatial::spatial_only());
    assert!(fresh.step(1, Some(input), &mut unavailable).is_err());
    assert_eq!(fresh.checkpoint().unwrap(), before);
}

#[test]
fn generated_source_host_rechecks_current_target_attackability_after_launch() {
    let (world, _) = released(7, 10000);
    let target = world.state.npcs[&2].id;
    assert!(CurrentSpell::new(&world.profile, &world.state, target, 133)
        .unwrap()
        .can_attack_spell()
        .unwrap());
    let mut later = world.state.clone();
    later.npcs.get_mut(&2).unwrap().health = 0;
    assert!(!CurrentSpell::new(&world.profile, &later, target, 133)
        .unwrap()
        .can_attack_spell()
        .unwrap());
}
