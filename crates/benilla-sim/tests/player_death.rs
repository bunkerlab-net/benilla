// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::player_death::*;

fn support() -> DeathSupport {
    DeathSupport::checked(&REQUIRED_ABSENCES.map(|dependency| (dependency, Some(false)))).unwrap()
}
fn fresh(instanceable: bool, delay: bool) -> PlayerDeathState {
    PlayerDeathState::new(
        7,
        DeathConfig {
            map_instanceable: instanceable,
            pve_reclaim_delay_enabled: delay,
        },
        0,
        1000,
    )
    .unwrap()
}
fn aura_sample() -> [DeathAura; 4] {
    [
        DeathAura {
            holder_id: 11,
            spell_id: 100,
            attributes: 0,
            attributes_ex3: 0,
            caster_aura_state: 0,
        },
        DeathAura {
            holder_id: 12,
            spell_id: 101,
            attributes: 0x40,
            attributes_ex3: 0,
            caster_aura_state: 0,
        },
        DeathAura {
            holder_id: 13,
            spell_id: 102,
            attributes: 0,
            attributes_ex3: 0x0010_0000,
            caster_aura_state: 0,
        },
        DeathAura {
            holder_id: 14,
            spell_id: 103,
            attributes: 0x40,
            attributes_ex3: 0x0010_0000,
            caster_aura_state: 0,
        },
    ]
}
fn input<'a>(
    support: &'a DeathSupport,
    auras: &'a [DeathAura],
    spells: &'a [CurrentSpell],
) -> DeathInput<'a> {
    DeathInput {
        tick: 1,
        now_seconds: 1000,
        player_id: 7,
        class: 8,
        killer_npc_id: 9,
        health_before: 50,
        lethal_damage: 50,
        mana_before: 31,
        durability_loss: true,
        killing_spell_attributes_ex3: None,
        support,
        auras,
        current_spells: spells,
        released_flight_ids: &[41, 42],
    }
}
fn spell_state(value: i64) -> CurrentSpellState {
    use CurrentSpellState::*;
    [
        Created, Targeting, Casting, Delayed, Traveling, Landing, Channeling, Finished,
    ][value as usize]
}
fn spells(state: i64, traits: i64) -> [CurrentSpell; 3] {
    [
        CurrentSpellSlot::Generic,
        CurrentSpellSlot::AutoRepeat,
        CurrentSpellSlot::Channeled,
    ]
    .map(|slot| CurrentSpell {
        slot,
        cast_id: 21 + slot as u64,
        state: spell_state(state),
        triggered: traits & 1 != 0,
        allow_cast_while_casting: traits & 2 != 0,
    })
}
fn token(action: &DeathAction) -> String {
    use DeathAction::*;
    match action {
        KillProc {
            killer_npc_id,
            player_id,
        } => {
            assert_eq!((*killer_npc_id, *player_id), (9, 7));
            "kill_proc".into()
        }
        SetHealthZero => "health_zero".into(),
        KillerKilledUnit {
            killer_npc_id,
            player_id,
        } => {
            assert_eq!((*killer_npc_id, *player_id), (9, 7));
            "killer_killed".into()
        }
        KillerPetOwnerKilledUnit => "pet_owner_killed".into(),
        SetPvpDeathFalse => "pvp_false".into(),
        NpcDeathDurabilityLoss => "durability_10".into(),
        SendDurabilityDamageDeath => "durability_message".into(),
        ClearDrunkenness => "drunk_zero".into(),
        ClearComboPoints => "combo_clear".into(),
        ClearResurrectRequest => "res_request".into(),
        RemovePet => "remove_pet".into(),
        RemoveGuardians => "guardians".into(),
        RemoveAuraByDeath {
            holder_id,
            spell_id,
        } => format!("aura:{holder_id}:{spell_id}"),
        BreakCharmOutgoing => "charm_out".into(),
        BreakCharmIncoming => "charm_in".into(),
        RemoveMiniPet => "minipet".into(),
        UnsummonTotems => "totems".into(),
        StopMoving => "stop_move".into(),
        ClearMotion => "clear_motion".into(),
        MoveIdle => "idle".into(),
        StopEvade => "stop_evade".into(),
        ClearHealthBelow20AuraState => "health20_clear".into(),
        ClearAllReactives => "reactives".into(),
        ClearDiminishings => "diminishings".into(),
        CombatStop => "combat_stop".into(),
        ClearComboPointHolders => "combo_holders".into(),
        SendAutoRepeatCancel => "auto_cancel".into(),
        InterruptCurrentSpell { slot, cast_id } => format!("interrupt:{}:{cast_id}", *slot as u8),
        SetDeathState { phase } => format!("state:{}", *phase as u8),
        RootMovement => "root".into(),
        ClearDynamicFlags => "dynamic_zero".into(),
        SetReleaseTimerFlag { enabled } => format!("release_flag:{}", u8::from(*enabled)),
        SetDeathTimer { milliseconds } => format!("timer:{milliseconds}"),
        SetDeathExpireTime { seconds } => format!("expire:{seconds}"),
        UpdateVisibility => "visibility".into(),
        // The oracle stops at these call boundaries. Rust explicitly reports
        // both dependencies as pending, never claims either operation completed.
        ReleaseUnavailable { automatic } => {
            assert!(*automatic);
            "build_repop,repop_graveyard".into()
        }
    }
}
fn trace(actions: &[DeathAction]) -> String {
    actions.iter().map(token).collect::<Vec<_>>().join(",")
}

#[test]
fn pinned_cpp_differential_order_aura_cast_and_timers() {
    let support = support();
    let auras = aura_sample();
    let mut counts = [0; 3];
    for (line, row) in include_str!("fixtures/player_death_cmangos.tsv")
        .lines()
        .enumerate()
    {
        let parts: Vec<_> = row.split(" | ").collect();
        assert_eq!(parts.len(), 3);
        let mut columns = parts[0].split_whitespace();
        let kind = columns.next().unwrap();
        let values: Vec<i64> = columns.map(|s| s.parse().unwrap()).collect();
        let expected: Vec<u64> = parts[1]
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        let (actions, actual) = if kind == "k" || kind == "c" {
            let current_spells = spells(values[0], values[1]);
            let mut death_input = input(&support, &auras, &current_spells);
            death_input.durability_loss = values[2] != 0;
            death_input.killing_spell_attributes_ex3 = (values[3] >= 0).then_some(values[3] as u32);
            let death = fresh(false, true).prepare_kill(death_input).unwrap();
            assert_eq!(death.removed_aura_holder_ids, [11]);
            assert_eq!(death.retained_aura_holder_ids, [12, 13, 14]);
            assert_eq!(death.retained_released_flight_ids, [41, 42]);
            assert_eq!(death.mana_before_equipment_callback, 31);
            if kind == "k" {
                counts[0] += 1;
                (
                    death.actions,
                    vec![
                        0,
                        u64::from(death.mana_before_equipment_callback),
                        death.next.phase() as u64,
                    ],
                )
            } else {
                counts[1] += 1;
                let retained: Vec<_> = current_spells
                    .into_iter()
                    .filter(|s| death.retained_current_cast_ids.contains(&s.cast_id))
                    .collect();
                let update = death
                    .next
                    .prepare_update(
                        DeathUpdate {
                            tick: 1,
                            now_seconds: 1000,
                            diff_ms: 17,
                            in_world: true,
                        },
                        &retained,
                    )
                    .unwrap();
                let snapshot = update.next.snapshot();
                (
                    update.actions,
                    vec![
                        u64::from(snapshot.death_timer_ms),
                        snapshot.death_expire_time_seconds,
                        u64::from(update.next.corpse_reclaim_delay_seconds(1000)),
                    ],
                )
            }
        } else {
            assert_eq!(kind, "t");
            counts[2] += 1;
            let mut snapshot = fresh(values[0] != 0, values[1] != 0).snapshot();
            snapshot.phase = DeathPhase::JustDied;
            snapshot.died_at_tick = Some(0);
            snapshot.death_expire_time_seconds = values[2] as u64;
            snapshot.death_expire_before_death_seconds = Some(values[2] as u64);
            let state = PlayerDeathState::from_snapshot(snapshot).unwrap();
            let update = state
                .prepare_update(
                    DeathUpdate {
                        tick: 1,
                        now_seconds: 1000,
                        diff_ms: values[3] as u32,
                        in_world: true,
                    },
                    &[],
                )
                .unwrap();
            let snapshot = update.next.snapshot();
            PlayerDeathState::from_snapshot(snapshot.clone()).unwrap();
            (
                update.actions,
                vec![
                    u64::from(snapshot.death_timer_ms),
                    snapshot.death_expire_time_seconds,
                    u64::from(update.next.corpse_reclaim_delay_seconds(1000)),
                ],
            )
        };
        assert_eq!(actual, expected, "numeric row {line}");
        assert_eq!(trace(&actions), parts[2], "order row {line}");
    }
    assert_eq!(counts, [192, 192, 308]);
}

#[test]
fn pending_generic_casts_cancel_including_triggered_and_cast_while_casting() {
    for state in 0..4 {
        for traits in 0..4 {
            let current = spells(state, traits);
            let plan = fresh(false, true)
                .prepare_kill(input(&support(), &[], &current))
                .unwrap();
            assert_eq!(plan.interrupted_cast_ids, [21, 22, 23]);
            assert!(plan.retained_current_cast_ids.is_empty());
            assert_eq!(plan.retained_released_flight_ids, [41, 42]);
        }
    }
}
#[test]
fn traveling_generic_and_detached_flights_survive_death() {
    let current = spells(4, 3);
    let plan = fresh(false, true)
        .prepare_kill(input(&support(), &[], &current))
        .unwrap();
    assert!(plan.interrupted_cast_ids.is_empty());
    assert_eq!(plan.retained_current_cast_ids, [21, 22, 23]);
    assert_eq!(plan.retained_released_flight_ids, [41, 42]);
    assert!(!plan.actions.contains(&DeathAction::SendAutoRepeatCancel));
}
#[test]
fn passive_effects_and_death_persistent_active_auras_are_retained() {
    let plan = fresh(false, true)
        .prepare_kill(input(&support(), &aura_sample(), &[]))
        .unwrap();
    assert_eq!(plan.removed_aura_holder_ids, [11]);
    assert_eq!(plan.retained_aura_holder_ids, [12, 13, 14]);
    assert_eq!(plan.mana_before_equipment_callback, 31);
    let loss = plan
        .actions
        .iter()
        .position(|a| matches!(a, DeathAction::NpcDeathDurabilityLoss))
        .unwrap();
    let zero = plan
        .actions
        .iter()
        .position(|a| matches!(a, DeathAction::SetHealthZero))
        .unwrap();
    let remove = plan
        .actions
        .iter()
        .position(|a| matches!(a, DeathAction::RemoveAuraByDeath { .. }))
        .unwrap();
    assert!(zero < loss && loss < remove);
}
#[test]
fn evidence_requires_every_dependency_and_rejects_each_unsupported_branch() {
    for dependency in REQUIRED_ABSENCES {
        let absent: Vec<_> = REQUIRED_ABSENCES
            .into_iter()
            .filter(|d| *d != dependency)
            .map(|d| (d, Some(false)))
            .collect();
        assert_eq!(
            DeathSupport::checked(&absent),
            Err(DeathError::MissingDependency(dependency))
        );
        for (value, error) in [
            (None, DeathError::MissingDependency(dependency)),
            (Some(true), DeathError::Unsupported(dependency)),
        ] {
            let mut evidence = REQUIRED_ABSENCES.map(|d| (d, Some(false)));
            evidence.iter_mut().find(|v| v.0 == dependency).unwrap().1 = value;
            assert_eq!(DeathSupport::checked(&evidence), Err(error));
        }
    }
    let mut repeated = REQUIRED_ABSENCES.map(|d| (d, Some(false))).to_vec();
    repeated.push(repeated[0]);
    assert_eq!(
        DeathSupport::checked(&repeated),
        Err(DeathError::DuplicateDependency(Dependency::Battleground))
    );
}
#[test]
fn kill_is_a_staged_plan_and_rejects_nonlethal_repeated_or_malformed_inputs() {
    let state = fresh(false, true);
    let initial = state.snapshot();
    let support = support();
    let mut request = input(&support, &[], &[]);
    request.lethal_damage = 49;
    assert_eq!(
        state.prepare_kill(request.clone()).unwrap_err(),
        DeathError::NonLethal
    );
    request.lethal_damage = 50;
    let plan = state.prepare_kill(request.clone()).unwrap();
    assert_eq!(state.snapshot(), initial);
    assert_eq!(
        plan.next.prepare_kill(request.clone()).unwrap_err(),
        DeathError::AlreadyDead
    );
    for class in [0, 1, 5, 9] {
        request.class = class;
        assert!(state.prepare_kill(request.clone()).is_err());
    }
    request.class = 8;
    request.health_before = 0;
    assert!(state.prepare_kill(request.clone()).is_err());
    request.health_before = 50;
    request.player_id = 8;
    assert!(state.prepare_kill(request.clone()).is_err());
    assert_eq!(state.snapshot(), initial);
}
#[test]
fn malformed_aura_cast_and_flight_sets_fail_before_a_plan() {
    let state = fresh(false, true);
    let support = support();
    let mut aura = aura_sample();
    aura.swap(0, 1);
    assert!(state.prepare_kill(input(&support, &aura, &[])).is_err());
    aura = aura_sample();
    aura[1].holder_id = aura[0].holder_id;
    assert!(state.prepare_kill(input(&support, &aura, &[])).is_err());
    let mut casts = spells(2, 0);
    casts[1].cast_id = casts[0].cast_id;
    assert!(state.prepare_kill(input(&support, &[], &casts)).is_err());
    casts = spells(2, 0);
    casts[1].slot = casts[0].slot;
    assert!(state.prepare_kill(input(&support, &[], &casts)).is_err());
    let mut request = input(&support, &[], &[]);
    request.released_flight_ids = &[42, 41];
    assert!(state.prepare_kill(request).is_err());
}
#[test]
fn same_update_death_timer_expiry_is_pending_and_world_updates_continue() {
    let dead = fresh(false, true)
        .prepare_kill(input(&support(), &[], &[]))
        .unwrap()
        .next;
    assert_eq!(
        dead.release_eligibility(),
        ReleaseEligibility::WaitingForKillPlayer
    );
    let first = dead
        .prepare_update(
            DeathUpdate {
                tick: 1,
                now_seconds: 1000,
                diff_ms: 360_000,
                in_world: true,
            },
            &[],
        )
        .unwrap();
    assert_eq!(first.next.phase(), DeathPhase::Corpse);
    assert_eq!(first.next.snapshot().death_timer_ms, 0);
    assert_eq!(
        first.next.release_eligibility(),
        ReleaseEligibility::EligibleButUnavailable {
            automatic_due: true
        }
    );
    assert_eq!(
        first.actions.last(),
        Some(&DeathAction::ReleaseUnavailable { automatic: true })
    );
    let later = first
        .next
        .prepare_update(
            DeathUpdate {
                tick: 2,
                now_seconds: 1001,
                diff_ms: 1000,
                in_world: true,
            },
            &[],
        )
        .unwrap();
    assert!(later.actions.is_empty());
    assert_eq!(later.next.phase(), DeathPhase::Corpse);
    assert!(later.next.snapshot().release_due);
    assert_eq!(later.next.snapshot().last_tick, 2);
}
#[test]
fn instance_body_keeps_timer_but_has_no_automatic_release_countdown() {
    let dead = fresh(true, true)
        .prepare_kill(input(&support(), &[], &[]))
        .unwrap()
        .next;
    let result = dead
        .prepare_update(
            DeathUpdate {
                tick: 1,
                now_seconds: 1000,
                diff_ms: u32::MAX,
                in_world: true,
            },
            &[],
        )
        .unwrap();
    assert_eq!(result.next.snapshot().death_timer_ms, DEATH_TIMER_MS);
    assert!(!result.next.snapshot().release_due);
    assert!(result
        .actions
        .contains(&DeathAction::SetReleaseTimerFlag { enabled: false }));
    assert_eq!(
        result.next.release_eligibility(),
        ReleaseEligibility::EligibleButUnavailable {
            automatic_due: false
        }
    );
}
#[test]
fn outside_world_does_not_advance_death_or_timers_and_duplicate_updates_fail() {
    let state = fresh(false, true);
    let frame = DeathUpdate {
        tick: 1,
        now_seconds: 1000,
        diff_ms: 1,
        in_world: false,
    };
    let idle = state.prepare_update(frame, &[]).unwrap();
    assert_eq!(idle.next, state);
    assert!(idle.actions.is_empty());
    let frame = DeathUpdate {
        in_world: true,
        ..frame
    };
    let first = state.prepare_update(frame, &[]).unwrap().next;
    assert_eq!(
        first.prepare_update(frame, &[]).unwrap_err(),
        DeathError::RepeatedUpdate
    );
    assert_eq!(
        first
            .prepare_update(
                DeathUpdate {
                    now_seconds: 999,
                    tick: 2,
                    ..frame
                },
                &[]
            )
            .unwrap_err(),
        DeathError::ClockRegression
    );
    assert_eq!(
        state
            .prepare_update(
                DeathUpdate {
                    now_seconds: u64::MAX,
                    ..frame
                },
                &[]
            )
            .unwrap_err(),
        DeathError::ClockOverflow
    );
}
#[test]
fn checked_restore_rejects_forged_or_incompatible_lifecycles() {
    let base = fresh(false, true).snapshot();
    for mutate in [
        |s: &mut PlayerDeathSnapshot| s.version += 1,
        |s: &mut PlayerDeathSnapshot| s.player_id = 0,
        |s: &mut PlayerDeathSnapshot| s.phase = DeathPhase::Corpse,
        |s: &mut PlayerDeathSnapshot| s.death_timer_ms = 1,
        |s: &mut PlayerDeathSnapshot| s.release_due = true,
        |s: &mut PlayerDeathSnapshot| s.last_update_tick = Some(1),
        |s: &mut PlayerDeathSnapshot| s.death_expire_time_seconds = 1901,
    ] {
        let mut snapshot = base.clone();
        mutate(&mut snapshot);
        assert!(PlayerDeathState::from_snapshot(snapshot).is_err());
    }
    assert_eq!(
        PlayerDeathState::restore(base.clone(), 8, base.config).unwrap_err(),
        DeathError::RestoreMismatch
    );
    let bad_config = DeathConfig {
        map_instanceable: true,
        ..base.config
    };
    assert_eq!(
        PlayerDeathState::restore(base.clone(), 7, bad_config).unwrap_err(),
        DeathError::RestoreMismatch
    );
    let dead = fresh(false, true)
        .prepare_kill(input(&support(), &[], &[]))
        .unwrap()
        .next;
    let corpse = dead
        .prepare_update(
            DeathUpdate {
                tick: 1,
                now_seconds: 1000,
                diff_ms: 17,
                in_world: true,
            },
            &[],
        )
        .unwrap()
        .next;
    assert_eq!(
        PlayerDeathState::restore(corpse.snapshot(), 7, base.config).unwrap(),
        corpse
    );
    let mut corrupted = corpse.snapshot();
    corrupted.release_due = true;
    assert!(PlayerDeathState::from_snapshot(corrupted).is_err());
}
#[test]
fn reclaim_delay_uses_seconds_and_expiration_boundary_not_death_timer() {
    let dead = fresh(false, true)
        .prepare_kill(input(&support(), &[], &[]))
        .unwrap()
        .next;
    let corpse = dead
        .prepare_update(
            DeathUpdate {
                tick: 1,
                now_seconds: 1000,
                diff_ms: 17,
                in_world: true,
            },
            &[],
        )
        .unwrap()
        .next;
    // At the exact source clock second: 300 / 300 == 1, then 299 / 300 == 0.
    assert_eq!(corpse.corpse_reclaim_delay_seconds(1000), 60);
    assert_eq!(corpse.corpse_reclaim_delay_seconds(1001), 30);
    assert_eq!(corpse.corpse_reclaim_delay_seconds(1301), 30);
}

#[test]
fn conditional_passive_holders_are_rejected_not_silently_kept_or_removed() {
    let mut aura = aura_sample();
    aura[1].caster_aura_state = 2;
    assert_eq!(
        fresh(false, true)
            .prepare_kill(input(&support(), &aura, &[]))
            .unwrap_err(),
        DeathError::UnsupportedAuraState(2)
    );
}

#[test]
fn native_fixed_step_restore_derives_phase_clock_and_timer_from_event_ticks() {
    let state = fresh(false, true);
    let config = state.snapshot().config;
    let mut clock = NativeDeathClock {
        clock_origin_ms: 1_000_000,
        tick_ms: 50,
        current_tick: 0,
    };
    assert_eq!(
        PlayerDeathState::restore_native_fixed_step(state.snapshot(), 7, config, clock).unwrap(),
        state
    );
    let updated = state
        .prepare_update(
            DeathUpdate {
                tick: 1,
                now_seconds: 1000,
                diff_ms: 50,
                in_world: true,
            },
            &[],
        )
        .unwrap()
        .next;
    let dead = updated
        .prepare_kill(input(&support(), &[], &[]))
        .unwrap()
        .next;
    clock.current_tick = 1;
    assert_eq!(
        PlayerDeathState::restore_native_fixed_step(dead.snapshot(), 7, config, clock).unwrap(),
        dead
    );
    let corpse = dead
        .prepare_update(
            DeathUpdate {
                tick: 2,
                now_seconds: 1000,
                diff_ms: 50,
                in_world: true,
            },
            &[],
        )
        .unwrap()
        .next;
    clock.current_tick = 2;
    assert_eq!(
        PlayerDeathState::restore_native_fixed_step(corpse.snapshot(), 7, config, clock).unwrap(),
        corpse
    );
    for mutate in [
        |s: &mut PlayerDeathSnapshot| s.death_timer_ms += 1,
        |s: &mut PlayerDeathSnapshot| s.death_expire_time_seconds += 1,
        |s: &mut PlayerDeathSnapshot| s.last_now_seconds += 1,
        |s: &mut PlayerDeathSnapshot| s.last_update_tick = Some(1),
        |s: &mut PlayerDeathSnapshot| s.death_expire_before_death_seconds = Some(1),
    ] {
        let mut snapshot = corpse.snapshot();
        mutate(&mut snapshot);
        assert!(PlayerDeathState::restore_native_fixed_step(snapshot, 7, config, clock).is_err());
    }
    let mut late = corpse.snapshot();
    late.last_tick = 3;
    late.last_update_tick = Some(3);
    late.corpse_at_tick = Some(3);
    clock.current_tick = 3;
    assert!(PlayerDeathState::restore_native_fixed_step(late, 7, config, clock).is_err());
    // Generic checked shape permits a deferred body; the native policy cannot.
    let mut late_dead = dead.snapshot();
    late_dead.last_tick = 2;
    late_dead.last_update_tick = Some(2);
    clock.current_tick = 2;
    assert!(PlayerDeathState::from_snapshot(late_dead.clone()).is_ok());
    assert!(PlayerDeathState::restore_native_fixed_step(late_dead, 7, config, clock).is_err());
}
#[test]
fn native_fixed_step_restore_derives_release_due_and_instance_timer() {
    for instance in [false, true] {
        let state = fresh(instance, false);
        let config = state.snapshot().config;
        let state = state
            .prepare_update(
                DeathUpdate {
                    tick: 1,
                    now_seconds: 1000,
                    diff_ms: 50,
                    in_world: true,
                },
                &[],
            )
            .unwrap()
            .next;
        let dead = state
            .prepare_kill(input(&support(), &[], &[]))
            .unwrap()
            .next;
        let mut state = dead;
        for tick in 2..=7202 {
            let now = (1_000_000 + tick * 50) / 1000;
            state = state
                .prepare_update(
                    DeathUpdate {
                        tick,
                        now_seconds: now,
                        diff_ms: 50,
                        in_world: true,
                    },
                    &[],
                )
                .unwrap()
                .next;
            if [2, 7200, 7201, 7202].contains(&tick) {
                let clock = NativeDeathClock {
                    clock_origin_ms: 1_000_000,
                    tick_ms: 50,
                    current_tick: tick,
                };
                assert_eq!(
                    PlayerDeathState::restore_native_fixed_step(state.snapshot(), 7, config, clock)
                        .unwrap(),
                    state
                );
            }
        }
        assert_eq!(state.snapshot().release_due, !instance);
    }
}
