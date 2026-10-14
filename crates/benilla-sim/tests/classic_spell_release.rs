// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic probes. No native spell, starter or database content rows.
use benilla_sim::classic_spell::*;

fn number(value: f32) -> SpellFloat {
    SpellFloat::from_bits(value.to_bits()).unwrap()
}
fn value(base: i32, sides: i32, points: f32, dice: f32) -> EffectValue {
    EffectValue {
        base_points: base,
        base_dice: 1,
        die_sides: sides,
        points_per_level: number(points),
        dice_per_level: number(dice),
        base_level: 1,
        spell_level: 1,
        max_level: 0,
    }
}
fn recipe(rank: u16, create_mana: u32) -> MageProjectileSpec {
    MageProjectileSpec {
        spell_id: 900001,
        effects: [
            MageEffect::SchoolDamage(value(9, 6, 10., 1.)),
            MageEffect::PeriodicDamage {
                value: value(1, 1, 2., 0.),
                amplitude_ms: 1000,
                duration_ms: 4000,
            },
            MageEffect::Empty,
        ],
        spell_rank: rank,
        cast_ms: cast_time(1500, -100, 500, rank, 1).unwrap(),
        mana_cost: mana_cost(10, 10, 1, create_mana, rank, 1).unwrap(),
        global_category: 133,
        global_recovery_ms: 1500,
        recovery_ms: 500,
        category: 42,
        category_recovery_ms: 750,
        projectile_speed: number(20.),
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
fn launch(rank: u16) -> LaunchContext {
    LaunchContext {
        distance_squared: number(100.),
        resist_chance: 400,
        critical_chance: 500,
        rolls: LaunchRolls {
            hit: 401,
            direct: [Some(6 + i32::from(rank / 5) - 1), None, None],
            critical: Some(500),
        },
    }
}
fn prepare(spec: MageProjectileSpec) -> (MageProjectileCast, CasterResources) {
    let mut resources = CasterResources::new(100).unwrap();
    let (cast, events) =
        MageProjectileCast::prepare(spec.clone(), 1, 2, 0, &mut resources, checks()).unwrap();
    assert_eq!(
        events,
        vec![SpellEvent::Started {
            cast_ms: spec.cast_ms
        }]
    );
    (cast, resources)
}
fn direct(events: &[SpellEvent]) -> u32 {
    events
        .iter()
        .find_map(|e| match e {
            SpellEvent::DirectDamage { amount, .. } => Some(*amount),
            _ => None,
        })
        .unwrap()
}

#[test]
fn all_source_release_rows_match_including_create_mana_only_thresholds() {
    let mut count = 0;
    let mut unchanged_rank_cost_changes = 0;
    for row in include_str!("fixtures/spell_release_cmangos.tsv").lines() {
        let v: Vec<i64> = row.split_whitespace().map(|s| s.parse().unwrap()).collect();
        assert_eq!(v.len(), 15, "{row}");
        let prepared = recipe(v[0] as u16, 100);
        let current = recipe(v[1] as u16, v[2] as u32);
        let context = MageProjectileReleaseContext::from_specs(&prepared, &current).unwrap();
        assert_eq!(i64::from(prepared.cast_ms), v[4], "{row}");
        assert_eq!(i64::from(current.cast_ms), v[5], "{row}");
        assert_eq!(i64::from(prepared.mana_cost), v[6], "{row}");
        assert_eq!(i64::from(context.mana_cost), v[7], "{row}");
        if v[0] == v[1] && v[2] == 250 {
            unchanged_rank_cost_changes += 1;
            assert_eq!(current.mana_cost, prepared.mana_cost + 1, "{row}");
        }
        let mut resources = CasterResources::new(10000).unwrap();
        let (mut cast, _) =
            MageProjectileCast::prepare(prepared.clone(), 1, 2, 0, &mut resources, checks())
                .unwrap();
        resources.mana = v[3] as u32;
        cast.update(
            u64::from(prepared.cast_ms - 1),
            prepared.cast_ms - 1,
            &mut resources,
            checks(),
            None,
        )
        .unwrap();
        assert_eq!(cast.state(), CastState::Casting { remaining_ms: 1 });
        assert_eq!(cast.release_context(), None);
        let failure = cast
            .release_failure_with_context(
                &resources,
                checks(),
                u64::from(prepared.cast_ms),
                context,
            )
            .unwrap();
        assert_eq!(failure.is_none(), v[8] != 0, "{row}");
        let rolls = failure.is_none().then(|| launch(current.spell_rank));
        assert_eq!(i64::from(rolls.is_some()), v[12], "{row}");
        let before = resources.clone();
        let events = cast
            .update_with_release(
                u64::from(prepared.cast_ms),
                1,
                &mut resources,
                checks(),
                rolls,
                Some(context),
            )
            .unwrap();
        assert_eq!(i64::from(resources.mana), v[9], "{row}");
        assert_eq!(
            i64::from(resources.last_mana_use_ms.is_some()),
            v[13],
            "{row}"
        );
        assert_eq!(
            i64::from(resources.spell_ready_at(prepared.spell_id).is_some()),
            v[14],
            "{row}"
        );
        if v[8] == 0 {
            assert_eq!(
                events,
                vec![SpellEvent::Failed(CastFailure::NoPower)],
                "{row}"
            );
            assert_eq!(resources, before, "{row}");
            assert_eq!(cast.release_context(), None);
        } else {
            assert!(
                events.contains(&SpellEvent::ManaSpent(current.mana_cost)),
                "{row}"
            );
            assert_eq!(cast.release_context(), Some(context));
            let impact_at = u64::from(prepared.cast_ms) + 500;
            let mut default_cast = cast.clone();
            let mut default_target = DamageTarget::new(2, 1000).unwrap();
            let default_events = default_cast
                .impact(impact_at, &mut default_target, true, false, [None; 3])
                .unwrap();
            assert_eq!(i64::from(direct(&default_events)), v[10], "{row}");
            assert_eq!(
                default_target.aura().unwrap().amount,
                2 + 2 * u32::from(current.spell_rank / 5 - 1)
            );
            let mut target = DamageTarget::new(2, 1000).unwrap();
            let impact = cast
                .impact_at_rank(25, impact_at, &mut target, true, false, [None; 3])
                .unwrap();
            assert_eq!(i64::from(direct(&impact)), v[10], "{row}");
            assert_eq!(i64::from(target.aura().unwrap().amount), v[11], "{row}");
        }
        count += 1;
    }
    assert_eq!(count, 360);
    assert_eq!(unchanged_rank_cost_changes, 25);
}

#[test]
fn current_cast_duration_never_replaces_prepare_timer_pushback_or_gcd() {
    let prepared = recipe(5, 100);
    let mut current = recipe(20, 250);
    current.cast_ms = 300;
    let context = MageProjectileReleaseContext::from_specs(&prepared, &current).unwrap();
    let (mut cast, mut resources) = prepare(prepared.clone());
    cast.update(1000, 1000, &mut resources, checks(), None)
        .unwrap();
    assert_eq!(
        cast.damage_pushback(99).unwrap(),
        vec![SpellEvent::Pushback(1000)]
    );
    assert_eq!(cast.state(), CastState::Casting { remaining_ms: 1500 });
    assert_eq!(resources.global_ready_at(133), Some(1500));
    cast.update(2499, 1499, &mut resources, checks(), None)
        .unwrap();
    assert_eq!(cast.state(), CastState::Casting { remaining_ms: 1 });
    let checkpoint = cast.checkpoint();
    assert!(MageProjectileCast::restore(checkpoint.clone(), current.clone(), 2499).is_err());
    assert_eq!(
        MageProjectileCast::restore(checkpoint, prepared, 2499).unwrap(),
        cast
    );
    let mut cancelled = cast.clone();
    let mut cancelled_resources = resources.clone();
    assert_eq!(
        cancelled.cancel(&mut cancelled_resources, CastFailure::Interrupted),
        vec![SpellEvent::Failed(CastFailure::Interrupted)]
    );
    assert_eq!(cancelled_resources.global_ready_at(133), None);
    assert_eq!(cancelled_resources.mana, 100);
    assert_eq!(cancelled.release_context(), None);
    cast.update_with_release(
        2500,
        1,
        &mut resources,
        checks(),
        Some(launch(20)),
        Some(context),
    )
    .unwrap();
    assert_eq!(cast.state(), CastState::Traveling { impact_at_ms: 3000 });
    assert_eq!(resources.global_ready_at(133), Some(1500));
    assert_eq!(resources.spell_ready_at(900001), Some(3000));
}

#[test]
fn exact_prepare_and_independent_release_witnesses_are_required_for_restore() {
    assert_eq!(SPELL_CHECKPOINT_VERSION, 1);
    assert_eq!(CAST_CHECKPOINT_VERSION, 3);
    assert_eq!(AURA_CHECKPOINT_VERSION, 2);
    let prepared = recipe(5, 100);
    let current = recipe(20, 250);
    let context = MageProjectileReleaseContext::from_specs(&prepared, &current).unwrap();
    let (mut cast, mut resources) = prepare(prepared.clone());
    cast.update(100, 100, &mut resources, checks(), None)
        .unwrap();
    let pending = cast.checkpoint();
    assert_eq!(
        MageProjectileCast::restore_with_release(pending.clone(), prepared.clone(), None, 100)
            .unwrap(),
        cast
    );
    assert!(MageProjectileCast::restore_with_release(
        pending.clone(),
        prepared.clone(),
        Some(context),
        100
    )
    .is_err());
    let mut premature = serde_json::to_value(&pending).unwrap();
    premature["release"] = serde_json::to_value(context).unwrap();
    assert!(MageProjectileCast::restore_with_release(
        serde_json::from_value(premature).unwrap(),
        prepared.clone(),
        Some(context),
        100,
    )
    .is_err());
    assert!(MageProjectileCast::restore(pending, current.clone(), 100).is_err());
    let mut replay = cast.clone();
    let mut replay_resources = resources.clone();
    let result = cast
        .update_with_release(
            1500,
            1400,
            &mut resources,
            checks(),
            Some(launch(20)),
            Some(context),
        )
        .unwrap();
    assert_eq!(
        result,
        replay
            .update_with_release(
                1500,
                1400,
                &mut replay_resources,
                checks(),
                Some(launch(20)),
                Some(context)
            )
            .unwrap()
    );
    assert_eq!(resources, replay_resources);
    assert_eq!(cast, replay);
    let flight = cast.checkpoint();
    assert!(MageProjectileCast::restore(flight.clone(), prepared.clone(), 1500).is_err());
    assert!(
        MageProjectileCast::restore_with_release(flight.clone(), prepared.clone(), None, 1500)
            .is_err()
    );
    assert!(
        MageProjectileCast::restore_with_release(flight.clone(), current, Some(context), 1500)
            .is_err()
    );
    let restored = MageProjectileCast::restore_with_release(
        flight.clone(),
        prepared.clone(),
        Some(context),
        1500,
    )
    .unwrap();
    assert_eq!(restored, cast);
    restored.validate_resources(&resources).unwrap();
    let mut forged_context = context;
    forged_context.mana_cost += 1;
    assert!(MageProjectileCast::restore_with_release(
        flight.clone(),
        prepared.clone(),
        Some(forged_context),
        1500
    )
    .is_err());
    for field in ["spell_rank", "mana_cost"] {
        let mut forged = serde_json::to_value(&flight).unwrap();
        forged["release"][field] = serde_json::json!(1);
        assert!(MageProjectileCast::restore_with_release(
            serde_json::from_value(forged).unwrap(),
            prepared.clone(),
            Some(context),
            1500
        )
        .is_err());
    }
    let mut unbound_flight = serde_json::to_value(&flight).unwrap();
    unbound_flight["release"] = serde_json::Value::Null;
    assert!(MageProjectileCast::restore_with_release(
        serde_json::from_value(unbound_flight).unwrap(),
        prepared.clone(),
        None,
        1500,
    )
    .is_err());
    let mut old_version = serde_json::to_value(&flight).unwrap();
    old_version["version"] = serde_json::json!(1);
    assert!(MageProjectileCast::restore_with_release(
        serde_json::from_value(old_version).unwrap(),
        prepared.clone(),
        Some(context),
        1500
    )
    .is_err());
    let mut swapped_prepare = serde_json::to_value(&flight).unwrap();
    swapped_prepare["prepared_spec"]["cast_ms"] = serde_json::json!(300);
    assert!(MageProjectileCast::restore_with_release(
        serde_json::from_value(swapped_prepare).unwrap(),
        prepared,
        Some(context),
        1500
    )
    .is_err());
    let mut replay_target = DamageTarget::new(2, 1000).unwrap();
    let mut target = replay_target.clone();
    let mut restored = restored;
    assert_eq!(
        cast.impact_at_rank(25, 2000, &mut target, true, false, [None; 3])
            .unwrap(),
        restored
            .impact_at_rank(25, 2000, &mut replay_target, true, false, [None; 3])
            .unwrap()
    );
    assert_eq!(target, replay_target);

    // Resource checks follow release cost, including both zero-cost transitions.
    for (prepared_cost, release_cost) in [(0, 10), (10, 0)] {
        let mut p = recipe(5, 100);
        p.mana_cost = prepared_cost;
        let (mut c, mut r) = prepare(p);
        c.update_with_release(
            1500,
            1500,
            &mut r,
            checks(),
            Some(launch(5)),
            Some(MageProjectileReleaseContext {
                spell_rank: 5,
                mana_cost: release_cost,
            }),
        )
        .unwrap();
        c.validate_resources(&r).unwrap();
        assert_eq!(r.last_mana_use_ms.is_some(), release_cost > 0);
        r.last_mana_use_ms = None;
        assert_eq!(c.validate_resources(&r).is_err(), release_cost > 0);
    }
    let (mut unchanged, mut resources) = prepare(recipe(5, 100));
    unchanged
        .update(1500, 1500, &mut resources, checks(), Some(launch(5)))
        .unwrap();
    assert_eq!(
        MageProjectileCast::restore(unchanged.checkpoint(), recipe(5, 100), 1500).unwrap(),
        unchanged
    );
}

#[test]
fn immutable_changes_early_context_invalid_dice_and_flight_rebinding_are_atomic() {
    let prepared = recipe(5, 100);
    let current = recipe(20, 250);
    let context = MageProjectileReleaseContext::from_specs(&prepared, &current).unwrap();
    let changes: [fn(&mut MageProjectileSpec); 11] = [
        |p| p.spell_id += 1,
        |p| p.effects[0] = MageEffect::SchoolDamage(value(10, 6, 10., 1.)),
        |p| p.global_category += 1,
        |p| p.global_recovery_ms += 1,
        |p| p.recovery_ms += 1,
        |p| p.category += 1,
        |p| p.category_recovery_ms += 1,
        |p| p.projectile_speed = number(21.),
        |p| p.movement_interrupts = false,
        |p| p.damage_pushback = false,
        |p| p.effects[1] = MageEffect::Empty,
    ];
    for change in changes {
        let mut bad = current.clone();
        change(&mut bad);
        assert!(MageProjectileReleaseContext::from_specs(&prepared, &bad).is_err());
    }
    let (mut cast, mut resources) = prepare(prepared);
    let before = cast.clone();
    let resource_before = resources.clone();
    for (now, diff) in [(0, 1500), (100, 100)] {
        assert!(cast
            .update_with_release(now, diff, &mut resources, checks(), None, Some(context))
            .is_err());
        assert_eq!(cast, before);
        assert_eq!(resources, resource_before);
    }
    for bad in [
        MageProjectileReleaseContext {
            spell_rank: 0,
            ..context
        },
        MageProjectileReleaseContext {
            mana_cost: 1_000_001,
            ..context
        },
    ] {
        assert!(cast
            .update_with_release(
                1500,
                1500,
                &mut resources,
                checks(),
                Some(launch(20)),
                Some(bad)
            )
            .is_err());
        assert_eq!(cast, before);
        assert_eq!(resources, resource_before);
    }
    let mut bad_rolls = launch(20);
    bad_rolls.rolls.direct[0] = Some(10);
    assert!(cast
        .update_with_release(
            1500,
            1500,
            &mut resources,
            checks(),
            Some(bad_rolls),
            Some(context)
        )
        .is_err());
    assert_eq!(cast, before);
    assert_eq!(resources, resource_before);
    cast.update_with_release(
        1500,
        1500,
        &mut resources,
        checks(),
        Some(launch(20)),
        Some(context),
    )
    .unwrap();
    let before = cast.clone();
    let resource_before = resources.clone();
    assert!(cast
        .update_with_release(1600, 100, &mut resources, checks(), None, Some(context))
        .is_err());
    assert!(cast
        .release_failure_with_context(&resources, checks(), 1600, context)
        .is_err());
    assert_eq!(cast, before);
    assert_eq!(resources, resource_before);
    cast.update(1600, 100, &mut resources, checks(), None)
        .unwrap();
    assert_eq!(cast.release_context(), Some(context));
    let mut target = DamageTarget::new(2, 1000).unwrap();
    cast.impact(2000, &mut target, true, false, [None; 3])
        .unwrap();
    let before = cast.clone();
    assert!(cast
        .update_with_release(2100, 100, &mut resources, checks(), None, Some(context))
        .is_err());
    assert_eq!(cast, before);
    assert_eq!(resources, resource_before);
}

#[test]
fn release_power_loss_preserves_gcd_balance_cooldowns_and_rng() {
    let prepared = recipe(5, 100);
    let current = recipe(20, 250);
    let context = MageProjectileReleaseContext::from_specs(&prepared, &current).unwrap();
    let (mut cast, mut resources) = prepare(prepared);
    resources.mana = context.mana_cost - 1;
    let before = cast.clone();
    let resource_before = resources.clone();
    assert_eq!(
        cast.release_failure_with_context(&resources, checks(), 1500, context)
            .unwrap(),
        Some(CastFailure::NoPower)
    );
    // Any supplied launch dice on the failed branch are an error, not a consumed roll.
    assert!(cast
        .update_with_release(
            1500,
            1500,
            &mut resources,
            checks(),
            Some(launch(20)),
            Some(context)
        )
        .is_err());
    assert_eq!(cast, before);
    assert_eq!(resources, resource_before);
    let mut draws = 0;
    let launch = if cast
        .release_failure_with_context(&resources, checks(), 1500, context)
        .unwrap()
        .is_none()
    {
        draws += 1;
        Some(launch(20))
    } else {
        None
    };
    assert_eq!(
        cast.update_with_release(1500, 1500, &mut resources, checks(), launch, Some(context))
            .unwrap(),
        vec![SpellEvent::Failed(CastFailure::NoPower)]
    );
    assert_eq!(draws, 0);
    assert_eq!(resources, resource_before);
    assert_eq!(resources.global_ready_at(133), Some(1500));
    assert_eq!(resources.spell_ready_at(900001), None);
    assert_eq!(cast.release_context(), None);
    let before = cast.clone();
    assert!(cast
        .update_with_release(1600, 100, &mut resources, checks(), None, Some(context))
        .is_err());
    assert_eq!(cast, before);
}
