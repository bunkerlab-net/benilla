// SPDX-License-Identifier: GPL-3.0-or-later
use super::synthetic_fixture::*;
use super::*;
fn advance(w: &mut NativeWorld, count: u64, a: &mut impl Authority) {
    for _ in 0..count {
        w.step(w.state().tick + 1, None, a).unwrap();
    }
}
fn input(w: &NativeWorld, command: Command) -> OwnedInput {
    OwnedInput {
        authenticated_owner: player().owner,
        input: Input {
            world: w.state().world,
            tick: w.state().tick + 1,
            player: player(),
            sequence: w.state().next_sequence,
            command,
        },
    }
}
fn roundtrip(w: &NativeWorld) -> NativeWorld {
    let bytes = w.checkpoint().unwrap().encode().unwrap();
    NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&bytes).unwrap(),
    )
    .unwrap()
}
#[test]
fn cast_flight_periodic_retaliation_navigation_regen_checkpoint_and_replay() {
    let mut w = world();
    let mut a = SyntheticAuthority {
        melee_damage: 2,
        ..Default::default()
    };
    assert_eq!(w.state().health, 90);
    assert_eq!(w.state().resources.caster.mana, 90);
    let start = w.checkpoint().unwrap();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    assert!(matches!(
        w.state().casts[&1].execution.state(),
        spell::CastState::Casting { remaining_ms: 200 }
    ));
    w = roundtrip(&w);
    advance(&mut w, 2, &mut a);
    assert_eq!(w.state().resources.caster.mana, 80);
    assert_eq!(w.state().resources.last_mana_use_timer_ms(), 4900);
    assert!(matches!(
        w.state().casts[&1].execution.state(),
        spell::CastState::Traveling { .. }
    ));
    let command = cast(&w, 60001);
    w.step(4, Some(command), &mut a).unwrap();
    advance(&mut w, 2, &mut a);
    assert_eq!(w.state().casts.len(), 2);
    w = roundtrip(&w);
    advance(&mut w, 40, &mut a);
    assert!(w.state().npcs[&2].health < 1000);
    assert!(!w.state().npcs[&2].auras.is_empty());
    assert_eq!(w.state().npcs[&2].ai.phase(), ai::Phase::Combat);
    assert!(w.state().npcs[&2].ai.position().x.as_f32() < 16.0);
    w = roundtrip(&w);
    let resume = w.checkpoint().unwrap();
    advance(&mut w, 60, &mut a);
    assert!(w.state().resources.caster.mana > 70);
    assert!(w.state().health < 90);
    let records = w.records().to_vec();
    let expected = w.state_digest();
    let mut replay = NativeWorld::restore_synthetic_component_test(profile(), resume).unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(replay.state_digest(), expected);
    // Initial checkpoint is the synthetic domain, never a native or original-v2 save.
    let mut value = serde_json::to_value(start).unwrap();
    value["version"] = 1.into();
    assert!(NativeWorld::restore_synthetic_component_test(
        profile(),
        serde_json::from_value(value).unwrap()
    )
    .is_err());
}
#[test]
fn unavailable_actives_stay_known_and_reject_without_rng_or_cost() {
    let mut w = world();
    let command = cast(&w, 60002);
    let r = w
        .step(1, Some(command), &mut SyntheticAuthority::default())
        .unwrap();
    assert!(
        r.events.iter().any(|event| matches!(event,Event::Rejected(Rejection::UnavailableAbility{ability:60002,reason}) if reason.contains("77")))
    );
    assert!(r.draws.is_empty());
    assert_eq!(w.state().resources.caster.mana, 90);
    assert_eq!(w.profile().abilities.len(), 2);
}
#[test]
fn missing_geometry_is_atomic_including_rng_journal_sequence_and_components() {
    let mut w = world();
    let mut a = SyntheticAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 42, &mut a);
    let before = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    a.missing_navigation = true;
    assert!(matches!(
        w.step(44, None, &mut a),
        Err(NativeError::Navigation(ai::AiError::Movement(
            ai::movement::BlockReason::MissingGeometry
        )))
    ));
    assert_eq!(w.checkpoint().unwrap(), before);
    assert_eq!(w.records(), records);
    a.missing_navigation = false;
    w.step(44, None, &mut a).unwrap();
}
#[test]
fn release_draw_order_is_explicit_and_miss_omits_effect_crit_and_aura_draws() {
    let mut w = world();
    let mut a = SyntheticAuthority {
        resist_chance: 10000,
        ..Default::default()
    };
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 2, &mut a);
    assert_eq!(w.last_tick().unwrap().draws.len(), 1);
    assert_eq!(w.last_tick().unwrap().draws[0].purpose, DrawPurpose::Hit);
    let counter = w.state().rng_counter;
    advance(&mut w, 40, &mut a);
    assert_eq!(w.state().rng_counter, counter);
    assert_eq!(w.state().npcs[&2].health, 1000);
}
#[test]
fn stale_authority_binding_and_forged_replay_are_transactional() {
    let mut w = world();
    let before = w.checkpoint().unwrap();
    let command = cast(&w, 60001);
    assert_eq!(
        w.step(
            1,
            Some(command),
            &mut SyntheticAuthority {
                stale_binding: true,
                ..Default::default()
            }
        )
        .unwrap_err(),
        NativeError::AuthorityBinding
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    let mut a = SyntheticAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 2, &mut a);
    let mut records = w.records().to_vec();
    records[2].draws[0].value = 0;
    let mut replay =
        NativeWorld::restore_synthetic_component_test(profile(), before.clone()).unwrap();
    assert!(replay.replay(&records).is_err());
    assert_eq!(replay.checkpoint().unwrap(), before);
}
#[test]
fn movement_cancels_generic_cast_without_drawing_and_never_cancels_old_flight() {
    let mut w = world();
    let mut a = SyntheticAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 2, &mut a);
    let command = cast(&w, 60001);
    w.step(4, Some(command), &mut a).unwrap();
    let command = input(
        &w,
        Command::Move {
            destination: point(1.0),
        },
    );
    w.step(5, Some(command), &mut a).unwrap();
    assert_eq!(w.state().casts.len(), 1);
    assert!(matches!(
        w.state().casts[&1].execution.state(),
        spell::CastState::Traveling { .. }
    ));
    assert!(w.last_tick().unwrap().draws.is_empty());
}
#[test]
fn checkpoint_commit_keeps_history_tip_and_rejects_stale_commit() {
    let mut w = world();
    let old = w.checkpoint().unwrap();
    advance(&mut w, 1, &mut SyntheticAuthority::default());
    assert!(w.checkpoint_committed(&old).is_err());
    let cp = w.checkpoint().unwrap();
    let tip = w.journal_tip;
    w.checkpoint_committed(&cp).unwrap();
    assert!(w.records().is_empty());
    assert_eq!(w.journal_tip, tip);
    let mut restored = roundtrip(&w);
    advance(&mut restored, 1, &mut SyntheticAuthority::default());
    assert_eq!(restored.last_tick().unwrap().previous_record, tip);
}
fn mutate_resigned(
    w: &NativeWorld,
    change: impl FnOnce(&mut serde_json::Value),
) -> Result<NativeWorld, NativeError> {
    let mut cp = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    change(&mut cp["state"]);
    let state: super::persistence::StateCheckpoint =
        serde_json::from_value(cp["state"].clone()).unwrap();
    cp["digest"] =
        serde_json::to_value(crate::digest(b"benilla-native-state-v19\0", &state)).unwrap();
    let digest: crate::Digest = serde_json::from_value(cp["digest"].clone()).unwrap();
    let tip: crate::Digest = serde_json::from_value(cp["history_tip"].clone()).unwrap();
    cp["checksum"] = serde_json::to_value(crate::digest(
        b"benilla-native-checkpoint-v19\0",
        &(&state, digest, tip),
    ))
    .unwrap();
    NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        serde_json::from_value(cp).unwrap(),
    )
}
#[test]
fn resigning_does_not_admit_forged_cast_timers_aura_timers_or_mana_timers() {
    let mut w = world();
    let mut a = SyntheticAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    assert!(
        mutate_resigned(&w, |s| s["casts"]["1"]["execution"]["state"]["Casting"]
            ["remaining_ms"] = 1.into())
        .is_err()
    );
    assert!(mutate_resigned(&w, |s| s["last_mana_use_timer_ms"] = 5000.into()).is_err());
    advance(&mut w, 43, &mut a);
    assert!(
        mutate_resigned(&w, |s| s["npcs"]["2"]["auras"]["60001"]["aura"]
            ["timer_ms"] = (-100).into())
        .is_err()
    );
    assert!(
        mutate_resigned(&w, |s| s["npcs"]["2"]["auras"]["60001"]["aura"]
            ["duration_ms"] = 2000.into())
        .is_err()
    );
}
#[test]
fn journal_size_failure_rolls_back_full_tick_and_counter() {
    let mut w = world();
    w.journal_bytes = MAX_BYTES;
    let before = w.checkpoint().unwrap();
    let command = cast(&w, 60001);
    assert_eq!(
        w.step(1, Some(command), &mut SyntheticAuthority::default())
            .unwrap_err(),
        NativeError::TooLarge
    );
    assert_eq!(w.checkpoint().unwrap(), before);
}
#[test]
fn lethal_earlier_npc_cannot_invalidate_later_npc_damage_receipts() {
    let mut p = profile();
    p.npcs[0].ai.spawn = point(1.0);
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.npcs[0].ai.mainhand_interval_ms = 1500;
    let mut second = p.npcs[0].clone();
    second.ai.creature_id = 3;
    second.ai.reaction = ai::Reaction::Defensive;
    p.npcs.push(second);
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], player(), 1234).unwrap();
    let mut a = SyntheticAuthority::default();
    let command = input(
        &w,
        Command::Cast {
            ability: 60001,
            target: w.state().npcs[&3].id,
        },
    );
    w.step(1, Some(command), &mut a).unwrap();
    // Force the ordinary synthetic attack interval so the first NPC's next swing and
    // second NPC's projectile hit share tick 16, without editing executable state.
    advance(&mut w, 14, &mut a);
    a.melee_damage = 1000;
    // Projectile launch tick 3 + minimum 5-yard/4-yard-per-second travel reaches tick16.
    w.step(16, None, &mut a).unwrap();
    assert!(w.state().npcs[&3].health < 1000);
    assert_eq!(w.state().health, 0);
    advance(&mut w, 5, &mut a);
    assert_eq!(w.state().health, 0);
}
#[test]
fn old_state_unknown_fields_noncanonical_encoding_and_invalid_float_are_rejected() {
    let w = world();
    let cp = w.checkpoint().unwrap();
    let bytes = cp.encode().unwrap();
    let mut space = bytes.clone();
    space.push(b' ');
    assert!(Checkpoint::decode(&space).is_err());
    let mut json = serde_json::to_value(cp).unwrap();
    json["state"]["unexpected"] = true.into();
    assert!(serde_json::from_value::<Checkpoint>(json).is_err());
    let mut json = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    json["state"]["position"]["x"] = 0x7f800000u32.into();
    assert!(serde_json::from_value::<Checkpoint>(json).is_err());
}
#[test]
fn corpse_restore_rejects_resigned_inconsistent_death_schedule() {
    let mut p = profile();
    p.npcs[0].max_health = 1;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], player(), 1234).unwrap();
    let mut a = SyntheticAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 42, &mut a);
    assert_eq!(w.state().npcs[&2].health, 0);
    w = roundtrip(&w);
    assert!(
        mutate_resigned(&w, |s| s["npcs"]["2"]["ai"]["respawn_at_ms"] =
            999999.into())
        .is_err()
    );
    advance(&mut w, 20, &mut a);
    assert_eq!(w.state().npcs[&2].id.generation, 1);
    assert_eq!(w.state().npcs[&2].health, 1);
}
#[test]
fn resigning_does_not_admit_impossible_pushbacks_or_lagged_fixed_step_auras() {
    let mut p = profile();
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        unreachable!()
    };
    spec.cast_ms = 20_000;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], player(), 1234).unwrap();
    let mut a = SyntheticAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 100, &mut a);
    assert!(mutate_resigned(&w, |s| {
        let c = &mut s["casts"]["1"]["execution"];
        c["pushbacks"] = 1.into();
        c["pushback_added_ms"] = 10_000.into();
        c["state"]["Casting"]["remaining_ms"] = 20_000.into();
    })
    .is_err());
    let mut w = world();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 52, &mut a);
    assert!(mutate_resigned(&w, |s| {
        let aura = &mut s["npcs"]["2"]["auras"]["60001"]["aura"];
        aura["ticks"] = 1.into();
        aura["timer_ms"] = 0.into();
    })
    .is_err());
}
#[test]
fn fixed_step_corpse_cannot_restore_an_overdue_lifecycle_even_with_consistent_timers() {
    let mut p = profile();
    p.npcs[0].max_health = 1;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], player(), 1234).unwrap();
    let mut a = SyntheticAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 42, &mut a);
    assert!(mutate_resigned(&w, |s| {
        let ai = &mut s["npcs"]["2"]["ai"];
        ai["death_at_ms"] = 100.into();
        ai["corpse_at_ms"] = 600.into();
        ai["respawn_at_ms"] = 2000.into();
    })
    .is_err());
}
#[test]
fn original_fixture_snapshot_has_no_native_decode_path() {
    let original = br#"{"version":2,"content_digest":[],"rules_digest":[],"state":{}}"#;
    assert!(Checkpoint::decode(original).is_err());
    assert_eq!(crate::FORMAT_VERSION, 2);
    assert_eq!(NATIVE_FORMAT_VERSION, 19);
    // The compiler token is uninhabited even when the synthetic feature is unified.
    assert_eq!(std::mem::size_of::<NativeCapability>(), 0);
}
#[test]
fn dead_caster_dot_applies_without_invalid_threat_then_npc_evades() {
    let mut p = profile();
    p.npcs[0].ai.spawn = point(1.0);
    p.npcs[0].ai.mainhand_interval_ms = 400;
    let mut w = NativeWorld::synthetic_component_test(p, [8; 16], player(), 1234).unwrap();
    let mut a = SyntheticAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    advance(&mut w, 18, &mut a);
    a.melee_damage = 1000;
    w.step(20, None, &mut a).unwrap();
    assert_eq!(w.state().health, 0);
    let npc_health = w.state().npcs[&2].health;
    w.step(21, None, &mut a).unwrap();
    assert!(w.state().npcs[&2].health < npc_health);
    assert_eq!(w.state().npcs[&2].ai.phase(), ai::Phase::ReturningHome);
    assert!(w.state().npcs[&2].auras.is_empty());
    roundtrip(&w);
}
#[test]
fn live_casts_reject_removed_cooldown_or_mana_evidence() {
    let mut w = world();
    let mut a = SyntheticAuthority::default();
    let command = cast(&w, 60001);
    w.step(1, Some(command), &mut a).unwrap();
    assert!(mutate_resigned(&w, |s| {
        s["resources"]["global"].as_object_mut().unwrap().clear();
    })
    .is_err());
    advance(&mut w, 2, &mut a);
    assert!(mutate_resigned(&w, |s| {
        s["resources"]["spells"].as_object_mut().unwrap().clear();
    })
    .is_err());
    assert!(mutate_resigned(&w, |s| {
        s["resources"]["last_mana_use_ms"] = serde_json::Value::Null;
        s["last_mana_use_timer_ms"] = 0.into();
    })
    .is_err());
}
#[test]
fn canonical_journal_bytes_match_committed_accounting() {
    let mut w = world();
    advance(&mut w, 5, &mut SyntheticAuthority::default());
    let bytes = encode_records(w.records()).unwrap();
    assert_eq!(bytes.len(), w.journal_bytes);
    assert_eq!(decode_records(&bytes).unwrap(), w.records());
    let mut noncanonical = bytes;
    noncanonical.push(b' ');
    assert!(decode_records(&noncanonical).is_err());
}

#[test]
fn dedicated_journal_full_is_uncommitted_and_checkpoint_retry_is_exact() {
    let mut w = world();
    w.step(1, None, &mut SyntheticAuthority::default()).unwrap();
    let before = w.checkpoint().unwrap();
    w.journal_bytes = MAX_BYTES;
    assert!(w.checkpoint_due());
    let input = cast(&w, 60001);
    assert_eq!(
        w.step(2, Some(input.clone()), &mut SyntheticAuthority::default())
            .unwrap_err(),
        NativeError::JournalFull
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    // Store receipts drive this acknowledgment in app code. This runtime unit test owns
    // the exact captured checkpoint and isolates only the atomic journal-capacity behavior.
    w.checkpoint_committed(&before).unwrap();
    w.step(2, Some(input), &mut SyntheticAuthority::default())
        .unwrap();
    assert_eq!(w.state().tick, 2);
    assert_eq!(w.state().next_sequence, 1);
}
