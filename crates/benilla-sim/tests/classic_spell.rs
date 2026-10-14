// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic probes only. No native spell or starter content rows.
use benilla_sim::classic_spell::*;
fn float(bits: u32) -> SpellFloat {
    SpellFloat::from_bits(bits).unwrap()
}
fn value(base_points: i32, sides: i32) -> EffectValue {
    EffectValue {
        base_points,
        base_dice: 1,
        die_sides: sides,
        points_per_level: SpellFloat::ZERO,
        dice_per_level: SpellFloat::ZERO,
        base_level: 1,
        spell_level: 1,
        max_level: 0,
    }
}
fn spec() -> MageProjectileSpec {
    MageProjectileSpec {
        spell_id: 900001,
        effects: [
            MageEffect::SchoolDamage(value(9, 6)),
            MageEffect::PeriodicDamage {
                value: value(1, 1),
                amplitude_ms: 1000,
                duration_ms: 4000,
            },
            MageEffect::Empty,
        ],
        spell_rank: 5,
        cast_ms: 1500,
        mana_cost: 10,
        global_category: 133,
        global_recovery_ms: 1500,
        recovery_ms: 500,
        category: 42,
        category_recovery_ms: 500,
        projectile_speed: float(0x41a00000),
        movement_interrupts: true,
        damage_pushback: true,
    }
}
fn checks() -> CastChecks {
    CastChecks {
        caster_alive: true,
        target_exists: true,
        target_alive: true,
        position_changed: false,
        other_failure: None,
    }
}
fn launch() -> LaunchContext {
    LaunchContext {
        distance_squared: float(0x42c80000),
        resist_chance: 400,
        critical_chance: 500,
        rolls: LaunchRolls {
            hit: 401,
            direct: [Some(6), None, None],
            critical: Some(500),
        },
    }
}
fn prepare() -> (MageProjectileCast, CasterResources) {
    let mut mana = CasterResources::new(100).unwrap();
    let (cast, events) = MageProjectileCast::prepare(spec(), 1, 2, 0, &mut mana, checks()).unwrap();
    assert_eq!(events, vec![SpellEvent::Started { cast_ms: 1500 }]);
    (cast, mana)
}
fn flying() -> (MageProjectileCast, CasterResources) {
    let (mut cast, mut mana) = prepare();
    cast.update(1500, 1500, &mut mana, checks(), Some(launch()))
        .unwrap();
    (cast, mana)
}
#[test]
fn software_numbers_match_independent_pinned_cpp_blocks() {
    let mut seen = 0;
    let mut aura = None;
    let mut now = 2000;
    for line in include_str!("fixtures/spell_cmangos.tsv").lines() {
        let mut words = line.split_whitespace();
        let mode = words.next().unwrap();
        let v: Vec<i64> = words.map(|s| s.parse().unwrap()).collect();
        match mode {
            "E" => {
                let input = EffectValue {
                    base_points: v[1] as i32,
                    base_dice: 1,
                    die_sides: v[2] as i32,
                    points_per_level: float(v[3] as u32),
                    dice_per_level: float(0x3dcccccd),
                    base_level: 1,
                    spell_level: 2,
                    max_level: v[4] as u8,
                };
                let result = input.evaluate(
                    v[0] as u16,
                    if v[5] == -9999999 {
                        None
                    } else {
                        Some(v[5] as i32)
                    },
                );
                if v[6].unsigned_abs() > 1_000_000 {
                    assert!(result.is_err());
                } else {
                    assert_eq!(result.unwrap(), v[6] as i32, "{line}");
                }
            }
            "T" => assert_eq!(
                projectile_delay(float(v[0] as u32), float(v[1] as u32)).unwrap(),
                v[2] as u32,
                "{line}"
            ),
            "C" => {
                let threshold = combat_chance(float(v[0] as u32)).unwrap();
                assert_eq!(threshold, v[2] as u16, "{line}");
                assert_eq!(threshold > v[1] as u16, v[3] != 0, "{line}");
            }
            "D" => assert_eq!(critical_damage(v[0] as u32).unwrap(), v[1] as u32, "{line}"),
            "P" => assert_eq!(
                mana_cost(v[0] as u32, v[1] as u32, v[2] as u32, 123, v[3] as u16, 2).unwrap(),
                v[4] as u32,
                "{line}"
            ),
            "B" => assert_eq!(
                cast_time(v[0] as i32, v[1] as i32, v[2] as i32, v[3] as u16, 1).unwrap(),
                v[4] as u32,
                "{line}"
            ),
            "S" => {
                let mut p = spec();
                p.cast_ms = v[0] as u32;
                let mut mana = CasterResources::new(100).unwrap();
                let (mut cast, _) =
                    MageProjectileCast::prepare(p, 1, 2, 0, &mut mana, checks()).unwrap();
                let mut c = checks();
                c.target_alive = v[2] != 0;
                let launch_input = if v[5] != 0 { Some(launch()) } else { None };
                cast.update(2000, v[1] as u32, &mut mana, c, launch_input)
                    .unwrap();
                match cast.state() {
                    CastState::Failed(CastFailure::TargetDead) => assert_eq!(v[4], 1, "{line}"),
                    CastState::Traveling { .. } => assert_eq!(v[5], 1, "{line}"),
                    CastState::Casting { remaining_ms } => {
                        assert_eq!(remaining_ms, v[3] as u32, "{line}");
                        assert_eq!(v[4] + v[5], 0, "{line}");
                    }
                    state => panic!("{line}: {state:?}"),
                }
            }
            "M" => assert_eq!(
                zero_resistance_npc_miss(v[0] as u8, v[1] as u8, float(v[2] as u32)).unwrap(),
                v[3] as u16,
                "{line}"
            ),
            "A" => {
                let target = aura.get_or_insert_with(|| {
                    let (mut cast, _) = flying();
                    let mut target = DamageTarget::new(2, 10000).unwrap();
                    cast.impact(2000, &mut target, true, false, [None; 3])
                        .unwrap();
                    target
                });
                // Use a fresh long-duration profile to compare timer debt through
                // the entire source sequence, independent of expiry.
                if v[0] == 999 {
                    let mut p = spec();
                    p.effects[1] = MageEffect::PeriodicDamage {
                        value: value(1, 1),
                        amplitude_ms: 1000,
                        duration_ms: 60000,
                    };
                    let mut mana = CasterResources::new(100).unwrap();
                    let (mut cast, _) =
                        MageProjectileCast::prepare(p, 1, 2, 0, &mut mana, checks()).unwrap();
                    cast.update(1500, 1500, &mut mana, checks(), Some(launch()))
                        .unwrap();
                    *target = DamageTarget::new(2, 10000).unwrap();
                    cast.impact(2000, target, true, false, [None; 3]).unwrap();
                }
                // Source diff is map update diff; distinct map ticks can have 0ms.
                now += v[0].max(1) as u64;
                target.update_aura(now, v[0] as u32, false).unwrap();
                assert_eq!(target.aura().unwrap().timer_ms, v[1], "{line}");
                assert_eq!(target.aura().unwrap().ticks, v[2] as u32, "{line}");
            }
            _ => panic!("unknown fixture {line}"),
        }
        seen += 1;
    }
    assert_eq!(seen, 8723);
}
#[test]
fn prepare_cast_travel_and_periodic_are_real_state_transitions() {
    let (mut cast, mut mana) = prepare();
    assert_eq!(mana.mana, 100);
    assert_eq!(mana.global_ready_at(133), Some(1500));
    assert_eq!(mana.spell_ready_at(900001), None);
    assert_eq!(
        cast.update(0, 100, &mut mana, checks(), None).unwrap(),
        vec![]
    );
    assert_eq!(cast.state(), CastState::Casting { remaining_ms: 1500 });
    cast.update(1499, 1499, &mut mana, checks(), None).unwrap();
    assert_eq!(mana.mana, 100);
    let events = cast
        .update(1500, 1, &mut mana, checks(), Some(launch()))
        .unwrap();
    assert_eq!(
        events,
        vec![
            SpellEvent::CooldownStarted,
            SpellEvent::ManaSpent(10),
            SpellEvent::Launched { impact_at_ms: 2000 }
        ]
    );
    assert_eq!(mana.mana, 90);
    assert_eq!(mana.last_mana_use_ms, Some(1500));
    assert_eq!(mana.spell_ready_at(900001), Some(2000));
    let mut target = DamageTarget::new(2, 100).unwrap();
    assert!(cast
        .impact(1999, &mut target, true, false, [None; 3])
        .is_err());
    assert_eq!(target.health, 100);
    let events = cast
        .impact(2000, &mut target, true, false, [None; 3])
        .unwrap();
    assert_eq!(
        events,
        vec![
            SpellEvent::AuraApplied { replaced: false },
            SpellEvent::DirectDamage {
                amount: 15,
                effective: 15,
                critical: false
            },
            SpellEvent::Completed
        ]
    );
    assert_eq!(target.health, 85);
    assert_eq!(target.update_aura(2000, 100, false).unwrap(), vec![]);
    for tick in 1..=4 {
        let events = target.update_aura(2000 + tick * 1000, 1000, false).unwrap();
        assert_eq!(
            events[0],
            SpellEvent::PeriodicDamage {
                absorb: 0,
                resist: 0,
                amount: 2,
                effective: 2,
                tick: tick as u32
            }
        );
        if tick == 4 {
            assert_eq!(events[1], SpellEvent::AuraRemoved);
        }
    }
    assert_eq!(target.health, 77);
    assert!(target.aura().is_none());
    assert_eq!(cast.state(), CastState::Finished);
}
#[test]
fn source_hit_and_crit_boundary_comparisons_differ() {
    for (die, resisted) in [(399, true), (400, true), (401, false)] {
        let (mut cast, mut mana) = prepare();
        let mut ctx = launch();
        ctx.rolls.hit = die;
        if resisted {
            ctx.rolls.direct = [None; 3];
            ctx.rolls.critical = None;
        }
        cast.update(1500, 1500, &mut mana, checks(), Some(ctx))
            .unwrap();
        let mut target = DamageTarget::new(2, 100).unwrap();
        let events = cast
            .impact(2000, &mut target, true, false, [None; 3])
            .unwrap();
        assert_eq!(events.contains(&SpellEvent::Resisted), resisted);
        assert_eq!(target.health, if resisted { 100 } else { 85 });
        assert_eq!(mana.mana, 90); // a normal spell miss still costs mana
    }
    for (die, critical) in [(499, true), (500, false), (501, false)] {
        let (mut cast, mut mana) = prepare();
        let mut ctx = launch();
        ctx.rolls.critical = Some(die);
        cast.update(1500, 1500, &mut mana, checks(), Some(ctx))
            .unwrap();
        let mut target = DamageTarget::new(2, 100).unwrap();
        let events = cast
            .impact(2000, &mut target, true, false, [None; 3])
            .unwrap();
        assert!(events.contains(&SpellEvent::DirectDamage {
            amount: if critical { 22 } else { 15 },
            effective: if critical { 22 } else { 15 },
            critical
        }));
    }
}
#[test]
fn failures_do_not_spend_and_cancel_differs_from_stopcast() {
    let (mut cast, mut mana) = prepare();
    let mut c = checks();
    c.position_changed = true;
    assert_eq!(
        cast.update(100, 100, &mut mana, c, None).unwrap(),
        vec![SpellEvent::Failed(CastFailure::Interrupted)]
    );
    assert_eq!(mana.mana, 100);
    assert_eq!(mana.global_ready_at(133), None);
    assert!(cast.cancel(&mut mana, CastFailure::Interrupted).is_empty());
    let (mut cast, mut mana) = prepare();
    mana.mana = 9;
    assert_eq!(
        cast.update(1500, 1500, &mut mana, checks(), None).unwrap(),
        vec![SpellEvent::Failed(CastFailure::NoPower)]
    );
    assert_eq!(mana.global_ready_at(133), Some(1500));
    assert_eq!(mana.spell_ready_at(900001), None);
    let (mut cast, mut mana) = prepare();
    let mut c = checks();
    c.target_alive = false;
    cast.update(100, 100, &mut mana, c, None).unwrap();
    assert_eq!(cast.state(), CastState::Failed(CastFailure::TargetDead));
    assert_eq!(mana.global_ready_at(133), None);
}
#[test]
fn unsupported_or_bad_roll_paths_are_transactional() {
    for bad in [
        MageEffect::Unsupported {
            opcode: 6,
            aura: 8,
            trigger_spell: 0,
        },
        MageEffect::Unsupported {
            opcode: 64,
            aura: 0,
            trigger_spell: 123,
        },
    ] {
        let mut p = spec();
        p.effects[2] = bad;
        let mut mana = CasterResources::new(100).unwrap();
        let before = mana.clone();
        assert!(MageProjectileCast::prepare(p, 1, 2, 0, &mut mana, checks()).is_err());
        assert_eq!(mana, before);
    }
    let (mut cast, mut mana) = prepare();
    let before_cast = cast.clone();
    let before_mana = mana.clone();
    let mut bad = launch();
    bad.rolls.direct[0] = Some(7);
    assert!(cast
        .update(1500, 1500, &mut mana, checks(), Some(bad))
        .is_err());
    assert_eq!(cast, before_cast);
    assert_eq!(mana, before_mana);
    assert!(cast.update(1500, 1500, &mut mana, checks(), None).is_err());
    assert_eq!(cast, before_cast);
    assert_eq!(mana, before_mana);
    let mut bad = launch();
    bad.rolls.direct[1] = Some(1);
    assert!(cast
        .update(1500, 1500, &mut mana, checks(), Some(bad))
        .is_err());
    assert_eq!(cast, before_cast);
}
#[test]
fn launch_lost_target_and_impact_rechecks_are_not_damage() {
    let (mut cast, mut mana) = flying();
    let mut c = checks();
    c.target_exists = false;
    cast.update(1600, 100, &mut mana, c, None).unwrap();
    assert_eq!(cast.state(), CastState::Failed(CastFailure::TargetLost));
    assert_eq!(mana.mana, 90);
    assert_eq!(mana.spell_ready_at(900001), Some(2000));
    for (allowed, immune, hp) in [(false, false, 100), (true, true, 100), (true, false, 0)] {
        let (mut cast, _) = flying();
        let mut target = DamageTarget::new(2, hp).unwrap();
        cast.impact(2000, &mut target, allowed, immune, [None; 3])
            .unwrap();
        assert_eq!(target.health, hp);
        assert!(target.aura().is_none());
    }
    let (mut cast, _) = flying();
    let mut target = DamageTarget::new(3, 100).unwrap();
    let before = cast.clone();
    assert!(cast
        .impact(2000, &mut target, true, false, [None; 3])
        .is_err());
    assert_eq!(cast, before);
}
#[test]
fn same_caster_replacement_restarts_timer_and_lethal_hit_removes_aura() {
    let (mut first, _) = flying();
    let mut target = DamageTarget::new(2, 100).unwrap();
    first
        .impact(2000, &mut target, true, false, [None; 3])
        .unwrap();
    target.update_aura(2500, 500, false).unwrap();
    let (mut second, _) = flying();
    let events = second
        .impact(2500, &mut target, true, false, [None; 3])
        .unwrap();
    assert!(events.contains(&SpellEvent::AuraApplied { replaced: true }));
    assert_eq!(target.aura().unwrap().timer_ms, 1000);
    assert_eq!(target.aura().unwrap().duration_ms, 4000);
    assert_eq!(target.aura().unwrap().ticks, 0);
    let (mut lethal, _) = flying();
    target.health = 1;
    let events = lethal
        .impact(3000, &mut target, true, false, [None; 3])
        .unwrap();
    assert_eq!(target.health, 0);
    assert!(target.aura().is_none());
    assert!(events.contains(&SpellEvent::AuraRemoved));
}
#[test]
fn periodic_immunity_consumes_tick_and_death_cleans_up() {
    let (mut cast, _) = flying();
    let mut target = DamageTarget::new(2, 100).unwrap();
    cast.impact(2000, &mut target, true, false, [None; 3])
        .unwrap();
    assert_eq!(
        target.update_aura(3000, 1000, true).unwrap(),
        vec![SpellEvent::Immune]
    );
    assert_eq!(target.health, 85);
    assert_eq!(target.aura().unwrap().ticks, 1);
    target.health = 1;
    let events = target.update_aura(4000, 1000, false).unwrap();
    assert_eq!(
        events,
        vec![
            SpellEvent::PeriodicDamage {
                absorb: 0,
                resist: 0,
                amount: 2,
                effective: 1,
                tick: 2
            },
            SpellEvent::AuraRemoved
        ]
    );
}
#[test]
fn pushback_sequence_is_capped_and_consumes_explicit_resistance_die() {
    let mut p = spec();
    p.cast_ms = 10000;
    let mut mana = CasterResources::new(100).unwrap();
    let (mut cast, _) = MageProjectileCast::prepare(p, 1, 2, 0, &mut mana, checks()).unwrap();
    cast.update(6000, 6000, &mut mana, checks(), None).unwrap();
    assert_eq!(
        cast.damage_pushback(99).unwrap(),
        vec![SpellEvent::Pushback(1000)]
    );
    for (index, expected) in [800, 600, 400, 200, 200].into_iter().enumerate() {
        cast.update(6200 + index as u64 * 200, 200, &mut mana, checks(), None)
            .unwrap();
        assert_eq!(
            cast.damage_pushback(0).unwrap(),
            vec![SpellEvent::Pushback(expected)]
        );
    }
    let before = cast.clone();
    assert!(cast.damage_pushback(100).is_err());
    assert_eq!(cast, before);
    let (mut cast, _) = flying();
    assert!(cast.damage_pushback(0).is_err());
    let (mut cast, _) = prepare();
    assert_eq!(
        cast.damage_pushback(0).unwrap(),
        vec![SpellEvent::Pushback(0)]
    );
}
#[test]
fn numeric_domain_and_roll_absence_are_explicit() {
    for bits in [
        1, 0x007fffff, 0x7f800000, 0x7fc00000, 0xff800000, 0x7f7fffff,
    ] {
        assert!(SpellFloat::from_bits(bits).is_err());
    }
    assert!(projectile_delay(SpellFloat::ZERO, SpellFloat::ZERO).is_err());
    assert!(projectile_delay(float(0xbf800000), SpellFloat::ONE).is_err());
    assert_eq!(value(9, 1).evaluate(5, None).unwrap(), 10);
    assert!(value(9, 1).evaluate(5, Some(1)).is_err());
    assert!(value(9, 6).evaluate(5, None).is_err());
    assert_eq!(value(9, -3).roll_range(5).unwrap(), Some((-3, 1)));
    assert_eq!(mana_cost(10, 0, 10, 100, 5, 1).unwrap(), 20);
    assert_eq!(mana_cost(10, 20, 0, 100, 5, 2).unwrap(), 0);
    assert_eq!(cast_time(2000, -100, 500, 100, 1).unwrap(), 500);
    assert_eq!(
        zero_resistance_npc_miss(1, 1, float(0x40800000)).unwrap(),
        400
    );
    assert_eq!(
        zero_resistance_npc_miss(1, 2, float(0x40800000)).unwrap(),
        500
    );
    assert_eq!(
        zero_resistance_npc_miss(1, 4, float(0x40800000)).unwrap(),
        1700
    );
    assert_eq!(
        zero_resistance_npc_miss(20, 1, float(0x40800000)).unwrap(),
        100
    );
    assert!(zero_resistance_npc_miss(20, 21, float(0x40800000)).is_err());
}
#[test]
fn cooldowns_gate_prepare_and_late_cancel_does_not_cancel_a_new_gcd() {
    let (mut first, mut mana) = flying();
    let (_, blocked) =
        MageProjectileCast::prepare(spec(), 1, 2, 1999, &mut mana, checks()).unwrap();
    assert_eq!(blocked, vec![SpellEvent::Failed(CastFailure::NotReady)]);
    let (_, allowed) =
        MageProjectileCast::prepare(spec(), 1, 2, 2000, &mut mana, checks()).unwrap();
    assert!(matches!(allowed[0], SpellEvent::Started { .. }));
    assert_eq!(mana.global_ready_at(133), Some(3500));
    first.cancel(&mut mana, CastFailure::Interrupted);
    assert_eq!(mana.global_ready_at(133), Some(3500));
    assert_eq!(mana.mana, 90);
    let (mut another, mut mana) = flying();
    let mut different = spec();
    different.spell_id += 1;
    different.global_category += 1;
    let (_, blocked) =
        MageProjectileCast::prepare(different, 1, 2, 1999, &mut mana, checks()).unwrap();
    assert_eq!(blocked, vec![SpellEvent::Failed(CastFailure::NotReady)]); // category cooldown
    another.cancel(&mut mana, CastFailure::Interrupted);
}
#[test]
fn rejected_cross_caster_aura_collision_and_invalid_aura_updates_are_atomic() {
    let (mut first, _) = flying();
    let mut target = DamageTarget::new(2, 100).unwrap();
    first
        .impact(2000, &mut target, true, false, [None; 3])
        .unwrap();
    let mut mana = CasterResources::new(100).unwrap();
    let (mut other, _) = MageProjectileCast::prepare(spec(), 3, 2, 0, &mut mana, checks()).unwrap();
    other
        .update(1500, 1500, &mut mana, checks(), Some(launch()))
        .unwrap();
    let before_target = target.clone();
    let before_cast = other.clone();
    assert!(other
        .impact(2000, &mut target, true, false, [None; 3])
        .is_err());
    assert_eq!(target, before_target);
    assert_eq!(other, before_cast);
    assert!(target.update_aura(1999, 100, false).is_err());
    assert_eq!(target, before_target);
    assert!(target.update_aura(2001, 3600001, false).is_err());
    assert_eq!(target, before_target);
    target.update_aura(2000, 100, false).unwrap();
    let before_target = target.clone();
    assert!(target.update_aura(2000, 100, false).is_err());
    assert_eq!(target, before_target);
}
#[test]
fn extra_launch_dice_on_a_refused_cast_and_mutated_resource_domains_reject() {
    for failure in 0..4 {
        let (mut cast, mut mana) = prepare();
        let mut c = checks();
        match failure {
            0 => c.target_exists = false,
            1 => c.position_changed = true,
            2 => c.target_alive = false,
            _ => mana.mana = 0,
        }
        let before_cast = cast.clone();
        let before_mana = mana.clone();
        assert!(cast
            .update(1500, 1500, &mut mana, c, Some(launch()))
            .is_err());
        assert_eq!(cast, before_cast);
        assert_eq!(mana, before_mana);
    }
    let (mut cast, mut mana) = prepare();
    mana.mana = 1_000_001;
    let before = cast.clone();
    assert!(cast
        .update(1500, 1500, &mut mana, checks(), Some(launch()))
        .is_err());
    assert_eq!(cast, before);
    assert!(MageProjectileCast::prepare(spec(), 1, 2, 0, &mut mana, checks()).is_err());
    let (mut cast, _) = flying();
    let mut target = DamageTarget::new(2, 100).unwrap();
    target.health = 1_000_001;
    assert!(cast
        .impact(2000, &mut target, true, false, [None; 3])
        .is_err());
    assert!(target.update_aura(3000, 1000, false).is_err());
}

#[test]
fn current_impact_rank_changes_periodic_value_but_not_committed_direct_payload() {
    let mut recipe = spec();
    if let MageEffect::SchoolDamage(v) = &mut recipe.effects[0] {
        v.points_per_level = float(10f32.to_bits());
    }
    if let MageEffect::PeriodicDamage { value: v, .. } = &mut recipe.effects[1] {
        v.points_per_level = float(10f32.to_bits());
    }
    let mut resources = CasterResources::new(100).unwrap();
    let (mut cast, _) =
        MageProjectileCast::prepare(recipe, 1, 2, 0, &mut resources, checks()).unwrap();
    cast.update(1500, 1500, &mut resources, checks(), Some(launch()))
        .unwrap();
    let mut old = cast.clone();
    let mut a = DamageTarget::new(2, 1000).unwrap();
    let mut b = a.clone();
    let first = old.impact(2000, &mut a, true, false, [None; 3]).unwrap();
    let second = cast
        .impact_at_rank(10, 2000, &mut b, true, false, [None; 3])
        .unwrap();
    let direct = |events: &Vec<SpellEvent>| {
        events.iter().find_map(|e| match e {
            SpellEvent::DirectDamage { amount, .. } => Some(*amount),
            _ => None,
        })
    };
    assert_eq!(direct(&first), direct(&second));
    assert_eq!(b.aura().unwrap().amount, a.aura().unwrap().amount + 10);
    let before = cast.clone();
    let target_before = b.clone();
    assert!(cast
        .impact_at_rank(0, 2000, &mut b, true, false, [None; 3])
        .is_err());
    assert_eq!(cast, before);
    assert_eq!(b, target_before);
}

#[test]
fn source_periodic_evade_accounting_preserves_prior_absorb_and_resist_order() {
    let mut count = 0;
    for line in include_str!("fixtures/periodic_evade.tsv").lines() {
        let v: Vec<i64> = line.split('\t').map(|n| n.parse().unwrap()).collect();
        let result =
            periodic_damage_after_evade_mods(v[0] as u32, v[1] as u32, v[2] as i32, v[3] != 0)
                .unwrap();
        assert_eq!(
            (result.amount, result.absorb, result.resist),
            (v[4] as u32, v[5] as u32, v[2] as i32),
            "{line}"
        );
        assert_eq!(v[6], if v[3] != 0 { 0 } else { 12 });
        count += 1;
    }
    assert_eq!(count, 120);
}
#[test]
fn timed_evade_absorbs_but_keeps_aura_clock_and_school_immunity_precedes_it() {
    let (mut cast, _) = flying();
    let mut target = DamageTarget::new(2, 100).unwrap();
    cast.impact(2000, &mut target, true, false, [None; 3])
        .unwrap();
    target.health = 1; // Would be lethal if the tick reached ModifyHealth.
    let events = target
        .update_aura_with_evade(3000, 1000, false, true)
        .unwrap();
    assert_eq!(
        events,
        vec![SpellEvent::PeriodicDamage {
            amount: 0,
            effective: 0,
            absorb: 2,
            resist: 0,
            tick: 1
        }]
    );
    assert_eq!(target.health, 1);
    assert_eq!(target.aura().unwrap().ticks, 1);
    assert_eq!(target.aura().unwrap().duration_ms, 3000);
    assert_eq!(
        target
            .update_aura_with_evade(4000, 1000, true, true)
            .unwrap(),
        vec![SpellEvent::Immune]
    );
    assert_eq!(target.aura().unwrap().ticks, 2);
    let events = target
        .update_aura_with_evade(5000, 1000, false, false)
        .unwrap();
    assert!(matches!(
        events[0],
        SpellEvent::PeriodicDamage {
            amount: 2,
            effective: 1,
            absorb: 0,
            resist: 0,
            tick: 3
        }
    ));
    assert_eq!(target.health, 0);
    assert!(target.aura().is_none());
}

#[test]
fn old_cast_aura_and_periodic_event_formats_cannot_reenter_v3_execution() {
    let (mut cast, _) = flying();
    let cp = serde_json::to_value(cast.checkpoint()).unwrap();
    assert_eq!(cp["policy"], SPELL_EXECUTION_POLICY_ID);
    for (field, value) in [
        ("version", serde_json::json!(2)),
        (
            "policy",
            serde_json::json!("cmangos-8ec338a1-mage-projectile-periodic-binary32-v2"),
        ),
    ] {
        let mut old = cp.clone();
        old[field] = value;
        assert!(
            MageProjectileCast::restore(serde_json::from_value(old).unwrap(), spec(), 1500)
                .is_err()
        );
    }
    let mut old = cp;
    old.as_object_mut().unwrap().remove("policy");
    assert!(serde_json::from_value::<CastCheckpoint>(old).is_err());
    let mut target = DamageTarget::new(2, 100).unwrap();
    cast.impact(2000, &mut target, true, false, [None; 3])
        .unwrap();
    let cp = serde_json::to_value(target.checkpoint()).unwrap();
    assert_eq!(cp["policy"], SPELL_EXECUTION_POLICY_ID);
    for (field, value) in [
        ("version", serde_json::json!(1)),
        (
            "policy",
            serde_json::json!("cmangos-8ec338a1-mage-projectile-periodic-binary32-v2"),
        ),
    ] {
        let mut old = cp.clone();
        old[field] = value;
        assert!(
            DamageTarget::restore(serde_json::from_value(old).unwrap(), &spec(), 1, 2000).is_err()
        );
    }
    let mut old = cp.clone();
    old["aura"]["version"] = serde_json::json!(1);
    assert!(DamageTarget::restore(serde_json::from_value(old).unwrap(), &spec(), 1, 2000).is_err());
    let mut old = cp;
    old.as_object_mut().unwrap().remove("policy");
    assert!(serde_json::from_value::<DamageTargetCheckpoint>(old).is_err());
    assert!(serde_json::from_str::<SpellEvent>(
        r#"{"PeriodicDamage":{"amount":2,"effective":2,"tick":1}}"#
    )
    .is_err());
}

#[test]
fn scheduled_restore_binds_callback_time_without_relaxing_frame_restore() {
    let (cast, _) = flying();
    let checkpoint = cast.checkpoint();
    let context = cast.release_context().unwrap();
    assert!(MageProjectileCast::restore_with_release(
        checkpoint.clone(),
        spec(),
        Some(context),
        1600
    )
    .is_err());
    let restored = MageProjectileCast::restore_scheduled_with_release(
        checkpoint.clone(),
        spec(),
        Some(context),
        1600,
        1500,
    )
    .unwrap();
    assert_eq!(restored, cast);
    for (now, callback) in [(1499, 1500), (1600, 1499), (2000, 1500)] {
        assert!(MageProjectileCast::restore_scheduled_with_release(
            checkpoint.clone(),
            spec(),
            Some(context),
            now,
            callback
        )
        .is_err());
    }
    let (casting, _) = prepare();
    assert!(MageProjectileCast::restore_scheduled_with_release(
        casting.checkpoint(),
        spec(),
        None,
        1,
        0
    )
    .is_err());
}

#[test]
fn source_event_reentry_is_explicit_and_only_allows_traveling_same_clock() {
    let (mut cast, mut resources) = flying();
    let before = cast.clone();
    assert!(cast
        .update(1500, 1500, &mut resources, checks(), None)
        .is_err());
    assert_eq!(cast, before);
    assert!(cast
        .update_from_event_with_release(1500, 1500, &mut resources, checks(), None, None)
        .unwrap()
        .is_empty());
    let (mut casting, mut resources) = prepare();
    casting
        .update(0, 100, &mut resources, checks(), None)
        .unwrap();
    assert!(casting
        .update_from_event_with_release(0, 100, &mut resources, checks(), None, None)
        .is_err());
}
