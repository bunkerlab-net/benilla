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
        serde_json::to_value(crate::digest(b"benilla-native-state-v64\0", &state)).unwrap();
    let digest: crate::Digest = serde_json::from_value(cp["digest"].clone()).unwrap();
    let tip: crate::Digest = serde_json::from_value(cp["history_tip"].clone()).unwrap();
    cp["checksum"] = serde_json::to_value(crate::digest(
        b"benilla-native-checkpoint-v64\0",
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
    assert_eq!(NATIVE_FORMAT_VERSION, 64);
    // The compiler token is uninhabited even when the synthetic feature is unified.
    assert_eq!(std::mem::size_of::<NativeCapability>(), 0);
}

pub(super) fn profile_identity_for_format(
    p: &Profile,
    version: u32,
    policy: &str,
) -> crate::Digest {
    crate::digest(
        format!("benilla-native-profile-v{version}\0").as_bytes(),
        &(
            policy,
            NPC_SPELL_POLICY,
            ADMIN_POLICY,
            benilla_sim::player_slow::PLAYER_SLOW_POLICY,
            RNG_POLICY,
            spell::SPELL_EXECUTION_POLICY_ID,
            ai::CREATURE_AI_POLICY_ID,
            benilla_sim::creature_motion::CREATURE_MOTION_POLICY_ID,
            benilla_sim::creature_regeneration::CREATURE_REGENERATION_POLICY_ID,
            mana::MAGE_RESOURCE_POLICY_ID,
            benilla_sim::starter_init::STARTER_INIT_POLICY_ID,
            benilla_sim::npc_melee::NPC_MELEE_POLICY_ID,
            equipment::EQUIPMENT_POLICY,
            benilla_sim::player_death::PLAYER_DEATH_POLICY,
            p,
        ),
    )
}

fn resign_identity_checkpoint(
    value: serde_json::Value,
    state_format: u32,
    checkpoint_format: u32,
) -> Checkpoint {
    let mut checkpoint: Checkpoint = serde_json::from_value(value).unwrap();
    checkpoint.digest = crate::digest(
        format!("benilla-native-state-v{state_format}\0").as_bytes(),
        &checkpoint.state,
    );
    checkpoint.checksum = crate::digest(
        format!("benilla-native-checkpoint-v{checkpoint_format}\0").as_bytes(),
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    checkpoint
}

const PREVIOUS_NATIVE_IDENTITIES: [(u32, &str); 30] = [
    (34, "benilla-native-event-ai-aggro-text-v34"),
    (35, "benilla-native-corpse-carried-items-v35"),
    (36, "benilla-native-source-combat-timers-v36"),
    (37, "benilla-native-initial-cast-genesis-v37"),
    (38, "benilla-native-creature-generation-v38"),
    (39, "benilla-native-creature-equipment-v39"),
    (40, "benilla-native-reference-loot-v40"),
    (41, "benilla-native-creature-addons-v41"),
    (42, "benilla-native-addon-capabilities-v42"),
    (43, "benilla-native-dry-habitat-v43"),
    (44, "benilla-native-combat-assistance-v44"),
    (45, "benilla-native-assistance-neighbors-v45"),
    (46, "benilla-native-peer-help-v46"),
    (47, "benilla-native-positive-peer-help-v47"),
    (48, "benilla-native-wander-leg-v48"),
    (49, "benilla-native-home-wander-v49"),
    (50, "benilla-native-home-wander-v50"),
    (51, "benilla-native-home-wander-v51"),
    (52, "benilla-native-player-mainhand-v52"),
    (53, "benilla-native-player-mainhand-v53"),
    (54, "benilla-native-backpack-split-v54"),
    (55, "benilla-native-backpack-destroy-v55"),
    (56, "benilla-native-quest-abandon-v56"),
    (57, "benilla-native-reputation-controls-v57"),
    (58, "benilla-native-watched-faction-v58"),
    (59, "benilla-native-action-bar-toggles-v59"),
    (60, "benilla-native-backpack-autostore-v60"),
    (61, "benilla-native-equipment-origin-v61"),
    (62, "benilla-native-equipment-handle-registry-v62"),
    (63, "benilla-native-armor-membership-v63"),
];

#[test]
fn native64_profile_state_record_and_checkpoint_domains_are_exact() {
    assert_eq!(NATIVE_FORMAT_VERSION, 64);
    assert_eq!(NATIVE_POLICY, "benilla-native-armor-relocation-v64");
    assert_eq!(
        RNG_POLICY,
        "benilla-native-sha256-counter-le64-rejection-fraction53-v2"
    );
    assert_eq!(crate::FORMAT_VERSION, 2);
    let mut w = world();
    assert_eq!(
        w.profile().digest(),
        profile_identity_for_format(w.profile(), 64, NATIVE_POLICY)
    );
    for (old_format, old_policy) in PREVIOUS_NATIVE_IDENTITIES {
        for (format, policy) in [
            (old_format, old_policy),
            (old_format, NATIVE_POLICY),
            (64, old_policy),
        ] {
            assert_ne!(
                w.profile().digest(),
                profile_identity_for_format(w.profile(), format, policy)
            );
        }
    }
    let initial = w.checkpoint().unwrap();
    advance(&mut w, 2, &mut SyntheticAuthority::default());
    let checkpoint = w.checkpoint().unwrap();
    let value = serde_json::to_value(&checkpoint).unwrap();
    assert_eq!(value["version"], 64);
    assert_eq!(value["state"]["version"], 64);
    assert_eq!(value["state"]["policy"], NATIVE_POLICY);
    assert_eq!(
        checkpoint.digest,
        crate::digest(b"benilla-native-state-v64\0", &checkpoint.state)
    );
    assert_eq!(checkpoint.digest, w.state_digest());
    assert_eq!(
        checkpoint.checksum,
        crate::digest(
            b"benilla-native-checkpoint-v64\0",
            &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
        )
    );
    for record in w.records() {
        assert_eq!(record.version, 64);
        assert_eq!(record.profile, w.profile().digest());
        assert_eq!(
            record.digest(),
            crate::digest(b"benilla-native-record-v64\0", record)
        );
        for (format, _) in PREVIOUS_NATIVE_IDENTITIES {
            assert_ne!(
                record.digest(),
                crate::digest(
                    format!("benilla-native-record-v{format}\0").as_bytes(),
                    record
                )
            );
        }
    }
    assert_eq!(
        Checkpoint::decode(&checkpoint.encode().unwrap()).unwrap(),
        checkpoint
    );
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), checkpoint);
}

#[test]
fn previous_native_checkpoint_headers_and_policies_reject_with_current_outer_hashes() {
    let w = world();
    let initial = w.checkpoint().unwrap();
    for (old_format, old_policy) in PREVIOUS_NATIVE_IDENTITIES {
        for (outer, state, policy) in [
            (old_format, old_format, old_policy),
            (old_format, 64, NATIVE_POLICY),
            (64, old_format, NATIVE_POLICY),
            (64, 64, old_policy),
        ] {
            for hash_format in [old_format, 64] {
                let mut value = serde_json::to_value(&initial).unwrap();
                value["version"] = outer.into();
                value["state"]["version"] = state.into();
                value["state"]["policy"] = policy.into();
                let old = resign_identity_checkpoint(value, hash_format, hash_format);
                assert_eq!(
                    Checkpoint::decode(&old.encode().unwrap()).unwrap_err(),
                    NativeError::InvalidCheckpoint
                );
                assert!(
                    NativeWorld::restore_synthetic_component_test(w.profile().clone(), old)
                        .is_err()
                );
            }
        }
    }
}

#[test]
fn previous_native_state_checkpoint_and_profile_context_cannot_reseal_as_native64() {
    let w = world();
    let initial = w.checkpoint().unwrap();
    for (old_format, old_policy) in PREVIOUS_NATIVE_IDENTITIES {
        for (state_format, checkpoint_format) in
            [(old_format, old_format), (old_format, 64), (64, old_format)]
        {
            let old = resign_identity_checkpoint(
                serde_json::to_value(&initial).unwrap(),
                state_format,
                checkpoint_format,
            );
            assert_eq!(
                Checkpoint::decode(&old.encode().unwrap()).unwrap_err(),
                NativeError::InvalidCheckpoint
            );
            assert!(matches!(
                NativeWorld::restore_synthetic_component_test(w.profile().clone(), old),
                Err(NativeError::InvalidCheckpoint)
            ));
        }
        for (format, policy) in [
            (old_format, old_policy),
            (old_format, NATIVE_POLICY),
            (64, old_policy),
        ] {
            let mut value = serde_json::to_value(&initial).unwrap();
            value["state"]["profile"] =
                serde_json::to_value(profile_identity_for_format(w.profile(), format, policy))
                    .unwrap();
            let old = resign_identity_checkpoint(value, 64, 64);
            // Decode checks the envelope; only restore has the executable profile context.
            let decoded = Checkpoint::decode(&old.encode().unwrap()).unwrap();
            assert!(matches!(
                NativeWorld::restore_synthetic_component_test(w.profile().clone(), decoded),
                Err(NativeError::Incompatible)
            ));
        }
    }
}

#[test]
fn previous_native_records_and_rehashed_context_reject_without_committing_a_prefix() {
    let mut w = world();
    let initial = w.checkpoint().unwrap();
    advance(&mut w, 1, &mut SyntheticAuthority::default());
    let middle = w.checkpoint().unwrap();
    advance(&mut w, 1, &mut SyntheticAuthority::default());
    let end = w.checkpoint().unwrap();
    let records = w.records().to_vec();
    for (format, policy) in PREVIOUS_NATIVE_IDENTITIES {
        for case in 0..7 {
            let mut forged = records.clone();
            match case {
                0 => forged[1].version = format,
                1 => forged[1].profile = profile_identity_for_format(w.profile(), format, policy),
                2 => {
                    forged[1].before = crate::digest(
                        format!("benilla-native-state-v{format}\0").as_bytes(),
                        &middle.state,
                    )
                }
                3 => {
                    forged[1].after = crate::digest(
                        format!("benilla-native-state-v{format}\0").as_bytes(),
                        &end.state,
                    )
                }
                4 => {
                    forged[1].previous_record = crate::digest(
                        format!("benilla-native-record-v{format}\0").as_bytes(),
                        &forged[0],
                    )
                }
                5 => {
                    forged[1].profile =
                        profile_identity_for_format(w.profile(), format, NATIVE_POLICY)
                }
                _ => forged[1].profile = profile_identity_for_format(w.profile(), 64, policy),
            }
            let bytes = encode_records(&forged).unwrap();
            if case == 0 {
                assert_eq!(decode_records(&bytes), Err(NativeError::InvalidCheckpoint));
            } else {
                assert_eq!(decode_records(&bytes).unwrap(), forged);
            }
            let mut replay =
                NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial.clone())
                    .unwrap();
            assert_eq!(replay.replay(&forged), Err(NativeError::ReplayMismatch));
            assert_eq!(replay.checkpoint().unwrap(), initial);
            assert!(replay.records().is_empty());
        }
    }
}

#[test]
fn native51_melee_domains_are_exact_and_old_evidence_cannot_reseal_as_current_records() {
    let mut p = initialized_profile();
    p.npcs[0].ai.spawn = point(1.0);
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    let mut w = NativeWorld::synthetic_component_test(p, [1; 16], player(), 9).unwrap();
    let mut authority = KernelAuthority::default();
    let (before, record) = (0..100)
        .find_map(|_| {
            let before = w.clone();
            let record = w
                .step(w.state.tick + 1, None, &mut authority)
                .unwrap()
                .clone();
            record
                .replies
                .iter()
                .any(|r| matches!(r.request.query, Query::NativeMelee { .. }))
                .then_some((before, record))
        })
        .expect("first ordinary melee action");
    assert!(before.state.casts.is_empty());
    assert_eq!(before.state.combat.as_ref().unwrap().next_action, 1);
    let request = &record
        .replies
        .iter()
        .find(|r| matches!(r.request.query, Query::NativeMelee { .. }))
        .unwrap()
        .request;
    let Query::NativeMelee {
        npc_orientation,
        action,
        state_binding,
        player_position,
        npc_position,
        ..
    } = request.query
    else {
        unreachable!()
    };
    let id = request.binding.npc.unwrap();
    // This first attack has no cast, reactive, damage or equipment prefix. The
    // source CombatStarted callback enters combat before the action is bound.
    let mut combat = before.state.combat.as_ref().unwrap().clone();
    if !combat.in_combat {
        combat.in_combat = true;
        combat.combat_tick_ms = 1200;
    }
    let action_payload = (
        before.state.world,
        before.state.player,
        record.tick,
        before.profile_digest,
        before.profile.geometry_revision,
        id,
        npc_position,
        before.state.npcs[&id.spawn].health,
        before.state.npcs[&id.spawn]
            .effective_profile(&before.profile)
            .unwrap(),
        npc_orientation,
        player_position,
        before.state.orientation,
        before.state.health,
        before.state.resources.caster.checkpoint(),
        combat.snapshot(),
        Vec::<(u64, spell::CastCheckpoint)>::new(),
    );
    assert_eq!(
        state_binding,
        crate::digest(b"benilla-native-melee-action-v51\0", &action_payload)
    );
    let tip = record
        .events
        .iter()
        .find_map(|event| match event {
            Event::Combat(CombatEvent::ActionCommitted {
                action: observed,
                binding,
                tip,
            }) => {
                assert_eq!((*observed, *binding), (action, state_binding));
                Some(*tip)
            }
            _ => None,
        })
        .unwrap();
    let chain_payload = (
        before.state.combat.as_ref().unwrap().action_tip,
        action,
        state_binding,
        w.state.rng_counter,
    );
    assert_eq!(
        tip,
        crate::digest(b"benilla-native-melee-chain-v51\0", &chain_payload)
    );
    let next = w
        .step(w.state.tick + 1, None, &mut authority)
        .unwrap()
        .clone();
    let initial = before.checkpoint().unwrap();
    // Format52 changes root envelopes, not this established v51 callback domain.
    for (format, _) in PREVIOUS_NATIVE_IDENTITIES
        .into_iter()
        .filter(|(format, _)| *format < 51)
    {
        for old_action in [false, true] {
            let mut forged = vec![record.clone(), next.clone()];
            if old_action {
                let old_binding = crate::digest(
                    format!("benilla-native-melee-action-v{format}\0").as_bytes(),
                    &action_payload,
                );
                assert_ne!(state_binding, old_binding);
                for reply in &mut forged[0].replies {
                    if let Query::NativeMelee { state_binding, .. } = &mut reply.request.query {
                        *state_binding = old_binding;
                    }
                }
            } else {
                let old_tip = crate::digest(
                    format!("benilla-native-melee-chain-v{format}\0").as_bytes(),
                    &chain_payload,
                );
                assert_ne!(tip, old_tip);
                for event in &mut forged[0].events {
                    if let Event::Combat(CombatEvent::ActionCommitted { tip, .. }) = event {
                        *tip = old_tip;
                    }
                }
            }
            forged[1].previous_record = forged[0].digest();
            let decoded = decode_records(&encode_records(&forged).unwrap()).unwrap();
            let mut replay = NativeWorld::restore_synthetic_component_test(
                before.profile.clone(),
                initial.clone(),
            )
            .unwrap();
            assert_eq!(replay.replay(&decoded), Err(NativeError::ReplayMismatch));
            assert_eq!(replay.checkpoint().unwrap(), initial);
            assert!(replay.records().is_empty());
        }
    }
}

#[test]
fn previous_native_visitor_callback_evidence_rejects_with_rehashed_record_chain() {
    use super::synthetic_two_owner_fixture as fixture;
    let mut w =
        NativeWorld::synthetic_component_test(fixture::profile(), [8; 16], player(), 7).unwrap();
    let initial = w.checkpoint().unwrap();
    advance(&mut w, 3, &mut fixture::CompleteAuthority::default());
    let records = w.records().to_vec();
    let callbacks = records
        .iter()
        .flat_map(|r| &r.replies)
        .filter_map(|reply| match &reply.request.query {
            Query::SourceMotion(MotionQuery::SceneVisitor(r)) => Some((&r.witness, &r.callback)),
            Query::SourceMotion(
                MotionQuery::VisitorVisibility(r) | MotionQuery::VisitorSight(r),
            ) => Some((&r.witness, &r.callback)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!callbacks.is_empty());
    for (witness, callback) in callbacks {
        assert_eq!(
            witness.callback,
            crate::digest(b"benilla-native-visitor-callback-v51\0", callback)
        );
    }
    // Format52 changes root envelopes, not this established v51 callback domain.
    for (format, _) in PREVIOUS_NATIVE_IDENTITIES
        .into_iter()
        .filter(|(format, _)| *format < 51)
    {
        let mut forged = records.clone();
        let mut changed = 0;
        for record in &mut forged {
            for reply in &mut record.replies {
                let pair = match &mut reply.request.query {
                    Query::SourceMotion(MotionQuery::SceneVisitor(r)) => {
                        Some((&mut r.witness, &r.callback))
                    }
                    Query::SourceMotion(
                        MotionQuery::VisitorVisibility(r) | MotionQuery::VisitorSight(r),
                    ) => Some((&mut r.witness, &r.callback)),
                    _ => None,
                };
                if let Some((witness, callback)) = pair {
                    witness.callback = crate::digest(
                        format!("benilla-native-visitor-callback-v{format}\0").as_bytes(),
                        callback,
                    );
                    changed += 1;
                }
            }
        }
        assert!(changed > 0);
        for index in 1..forged.len() {
            forged[index].previous_record = forged[index - 1].digest();
        }
        let decoded = decode_records(&encode_records(&forged).unwrap()).unwrap();
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), initial.clone())
                .unwrap();
        assert_eq!(replay.replay(&decoded), Err(NativeError::ReplayMismatch));
        assert_eq!(replay.checkpoint().unwrap(), initial);
        assert!(replay.records().is_empty());
    }
}

fn replace_identity_digest(
    value: &mut serde_json::Value,
    old: &serde_json::Value,
    replacement: &serde_json::Value,
) -> usize {
    if value == old {
        *value = replacement.clone();
        return 1;
    }
    match value {
        serde_json::Value::Object(fields) => fields
            .values_mut()
            .map(|v| replace_identity_digest(v, old, replacement))
            .sum(),
        serde_json::Value::Array(values) => values
            .iter_mut()
            .map(|v| replace_identity_digest(v, old, replacement))
            .sum(),
        _ => 0,
    }
}

#[test]
fn previous_native_event_ai_evidence_rejects_rehashed_checkpoints_and_records() {
    use super::synthetic_event_ai_fixture as fixture;
    let mut w = fixture::world(7);
    let initial = w.checkpoint().unwrap();
    advance(&mut w, 3, &mut fixture::Authority::default());
    let evaluation = w
        .records()
        .iter()
        .flat_map(|r| &r.events)
        .find_map(|event| match event {
            Event::EventAiEvaluation(evaluation) => Some(evaluation.as_ref()),
            _ => None,
        })
        .unwrap();
    let callback = evaluation.callback.digest();
    assert_eq!(
        callback,
        crate::digest(
            b"benilla-native-event-ai-callback-v51\0",
            &evaluation.callback
        )
    );
    assert_eq!(
        evaluation.actions[0]
            .audience
            .as_ref()
            .unwrap()
            .request
            .callback,
        callback
    );
    let checkpoint = w.checkpoint().unwrap();
    let value = serde_json::to_value(&checkpoint).unwrap();
    assert_eq!(
        value["state"]["event_ai"]["history"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        value["state"]["npcs"]["2"]["events"]["event_ai_anchor"]["count"],
        1
    );
    assert_eq!(
        value["state"]["npcs"]["2"]["events"]["event_ai_anchor"]["tip"],
        serde_json::to_value(crate::digest(
            b"benilla-native-event-ai-callback-chain-v51\0",
            &([0u8; 32], 1u64, evaluation)
        ))
        .unwrap()
    );
    // Format52 changes root envelopes, not this established v51 callback domain.
    for (format, _) in PREVIOUS_NATIVE_IDENTITIES
        .into_iter()
        .filter(|(format, _)| *format < 51)
    {
        let old_callback = crate::digest(
            format!("benilla-native-event-ai-callback-v{format}\0").as_bytes(),
            &evaluation.callback,
        );
        assert_ne!(callback, old_callback);
        for old_action in [false, true] {
            let mut forged = value.clone();
            let tip = if old_action {
                assert!(
                    replace_identity_digest(
                        &mut forged["state"]["event_ai"],
                        &serde_json::to_value(callback).unwrap(),
                        &serde_json::to_value(old_callback).unwrap()
                    ) > 0
                );
                let changed: EventAiEvaluation = serde_json::from_value(
                    forged["state"]["event_ai"]["history"][0]["EnterCombat"].clone(),
                )
                .unwrap();
                // Reseal the independent anchor as well; restored semantic history
                // must still reject the old callback identity inside its audience.
                crate::digest(
                    b"benilla-native-event-ai-callback-chain-v51\0",
                    &([0u8; 32], 1u64, changed),
                )
            } else {
                crate::digest(
                    format!("benilla-native-event-ai-callback-chain-v{format}\0").as_bytes(),
                    &([0u8; 32], 1u64, evaluation),
                )
            };
            forged["state"]["npcs"]["2"]["events"]["event_ai_anchor"]["tip"] =
                serde_json::to_value(tip).unwrap();
            let forged = resign_identity_checkpoint(forged, 64, 64);
            let decoded = Checkpoint::decode(&forged.encode().unwrap()).unwrap();
            assert!(
                NativeWorld::restore_synthetic_component_test(w.profile.clone(), decoded).is_err()
            );
        }
        let mut forged = serde_json::to_value(w.records()).unwrap();
        assert!(
            replace_identity_digest(
                &mut forged,
                &serde_json::to_value(callback).unwrap(),
                &serde_json::to_value(old_callback).unwrap()
            ) > 0
        );
        let mut forged: Vec<TickRecord> = serde_json::from_value(forged).unwrap();
        for index in 1..forged.len() {
            forged[index].previous_record = forged[index - 1].digest();
        }
        let decoded = decode_records(&encode_records(&forged).unwrap()).unwrap();
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial.clone())
                .unwrap();
        assert_eq!(replay.replay(&decoded), Err(NativeError::ReplayMismatch));
        assert_eq!(replay.checkpoint().unwrap(), initial);
        assert!(replay.records().is_empty());
    }
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
