// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-observed numeric shape plus explicitly AUTHORED client resolutions.
//! These are not captured SpellCastTimes16/SpellDuration35/SpellRange35 rows.
use super::*;
use crate::native::{synthetic_fixture as f, synthetic_spell_fixture as fixture};

#[path = "spell_initial_cast_tests.rs"]
mod initial_cast_tests;
#[path = "spell_full_raw_lifecycle_tests.rs"]
mod lifecycle_tests;
#[path = "spell_location_tests.rs"]
mod location_tests;
#[path = "spell_full_raw_progression_tests.rs"]
mod progression_tests;

fn inputs() -> (Profile, init::StarterInput, SpellAuthorityInput) {
    let (mut p, mut initial, _) = fixture::inputs();
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        unreachable!()
    };
    let value = |base_points, die_sides, per_level| spell::EffectValue {
        base_points,
        base_dice: 1,
        die_sides,
        points_per_level: f::float(per_level),
        dice_per_level: spell::SpellFloat::ZERO,
        base_level: 1,
        spell_level: 1,
        max_level: 5,
    };
    spec.effects = [
        spell::MageEffect::SchoolDamage(value(13, 9, 0.6)),
        spell::MageEffect::PeriodicDamage {
            value: value(0, 1, 0.0),
            amplitude_ms: 2000,
            duration_ms: 6000,
        },
        spell::MageEffect::Empty,
    ];
    spec.cast_ms = 200; // Authored resolution, not the actual client index16 value.
    spec.mana_cost = 30;
    spec.global_category = 133;
    spec.global_recovery_ms = 1500;
    spec.recovery_ms = 0;
    spec.projectile_speed = f::float(24.0);
    spec.movement_interrupts = true;
    spec.damage_pushback = true;
    let mut source = fixture::source(&p);
    let raw = &mut source.spells[0];
    raw.words[6] = NOT_SHAPESHIFT;
    raw.words[13] = 0;
    raw.words[18] = 16;
    raw.cast_time.id = 16;
    raw.words[21] = 15;
    raw.words[25] = 101;
    raw.words[30] = 35;
    raw.duration.as_mut().unwrap().id = 35;
    raw.words[36] = 35;
    raw.range.id = 35;
    raw.words[59] = u32::MAX;
    raw.words[161] = 0x4000_0001;
    raw.words[166] = u32::MAX;
    raw.words[169] = 0x3f80_0000;
    raw.bonus_coefficients = [FIREBALL_BONUS_COEFFICIENT, 0, 0];
    // Fixture::source selects chain count1 for its generic authored effects;
    // the observed133 row has0. The component is explicit-target in either case.
    raw.words[100] = 0;
    raw.words[101] = 0;
    raw.facing_flags = 0;
    initial.spells[0].words = raw.words.clone().try_into().unwrap();
    let hit = p
        .source_motion
        .as_mut()
        .unwrap()
        .spell_hits
        .get_mut(&133)
        .unwrap();
    hit.attributes.copy_from_slice(&raw.words[6..11]);
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    (p, initial, source)
}

fn profile() -> Profile {
    let (mut p, initial, source) = inputs();
    p.initialize_spell_authority(&initial, source).unwrap();
    fixture::initialize_authored_location(&mut p);
    p
}
fn world(seed: u64) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [8; 16], f::player(), seed).unwrap()
}
fn advance(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state.tick + 1, None, a).unwrap();
}
fn start(w: &mut NativeWorld, a: &mut impl Authority) {
    let cast = f::cast(w, 133);
    w.step(w.state.tick + 1, Some(cast), a).unwrap();
}
fn timers(w: &NativeWorld) -> [u32; 2] {
    w.state.combat.as_ref().unwrap().melee_timers.remaining_ms()
}
fn rehash(value: serde_json::Value) -> Checkpoint {
    let mut checkpoint: Checkpoint = serde_json::from_value(value).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v38\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v38\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    Checkpoint::decode(&checkpoint.encode().unwrap()).unwrap()
}

#[test]
fn full_raw_numeric_shape_retains_every_extended_field_and_unknown_client_rows_fail() {
    let (mut p, initial, source) = inputs();
    let raw = source.spells[0].clone();
    p.initialize_spell_authority(&initial, source.clone())
        .unwrap();
    let retained = &p
        .interactions
        .as_ref()
        .unwrap()
        .spell_authority
        .as_ref()
        .unwrap()
        .spells[&133];
    assert_eq!(retained, &raw);
    assert_eq!(raw.words[21], 15);
    assert_eq!(raw.bonus_coefficients[0], 0x3dfbe76d);
    assert_eq!(raw.words[25], 101);
    assert_eq!(raw.words[59] as i32, -1);
    for field in 0..3 {
        let (mut p, initial, mut source) = inputs();
        match field {
            0 => source.spells[0].cast_time.id = 0,
            1 => source.spells[0].duration = None,
            _ => source.spells[0].range.id = 0,
        }
        let before = p.clone();
        assert!(p.initialize_spell_authority(&initial, source).is_err());
        assert_eq!(p, before);
    }
}

#[test]
fn successful_and_resisted_releases_reset_without_autoattack_and_reopen_after_cast_deletion() {
    let mut outcomes = [false; 2];
    for seed in 0..150 {
        if outcomes == [true; 2] {
            break;
        }
        let mut w = world(seed);
        let initial = w.checkpoint().unwrap();
        assert_eq!(timers(&w), [0, 0]);
        let mut a = fixture::spatial_only();
        start(&mut w, &mut a);
        assert_eq!(timers(&w), [0, 0]);
        advance(&mut w, &mut a);
        advance(&mut w, &mut a);
        assert_eq!(timers(&w), [1900, 0]);
        let launch = w.state.casts[&1].execution.launch_context().unwrap();
        let resisted = launch.rolls.hit <= launch.resist_chance;
        outcomes[usize::from(resisted)] = true;
        assert!(w
            .last_tick()
            .unwrap()
            .draws
            .iter()
            .filter(|d| d.cast_id == 1)
            .all(|d| matches!(
                d.purpose,
                DrawPurpose::Hit | DrawPurpose::Direct { .. } | DrawPurpose::Critical
            )));
        for _ in 0..24 {
            let cp = w.checkpoint().unwrap();
            let mut reopened =
                NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).unwrap();
            advance(&mut w, &mut a);
            advance(&mut reopened, &mut fixture::spatial_only());
            assert_eq!(w.checkpoint().unwrap(), reopened.checkpoint().unwrap());
        }
        assert!(w.state.casts.is_empty());
        assert_eq!(timers(&w), [0, 0]);
        assert!(w.state.combat.as_ref().unwrap().melee_reset.is_some());
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    }
    assert_eq!(outcomes, [true, true]);
}

#[test]
fn cancellation_release_failure_and_observer_failure_do_not_commit_a_reset() {
    for case in 0..3 {
        let mut w = world(7);
        let mut a = fixture::spatial_only();
        start(&mut w, &mut a);
        advance(&mut w, &mut a);
        if case == 0 {
            let cancel = crate::native::synthetic_quest_fixture::command(
                &w,
                Command::CancelCast {
                    spell_id: 133,
                    target: CancelCastTarget::Instance(1),
                },
            );
            w.step(3, Some(cancel), &mut a).unwrap();
        } else if case == 1 {
            a.line_of_sight = Some(ai::movement::Check::Failed);
            advance(&mut w, &mut a);
        } else {
            a.fail_phase = Some(SourceSpellPhase::ReleaseHit);
            let before = w.checkpoint().unwrap();
            assert!(w.step(3, None, &mut a).is_err());
            assert_eq!(w.checkpoint().unwrap(), before);
        }
        assert_eq!(timers(&w), [0, 0]);
        assert!(w.state.combat.as_ref().unwrap().melee_reset.is_none());
    }
}

#[test]
fn retained_reset_and_idle_state_reject_rehashed_tampering_after_projectile_finishes() {
    let mut w = world(7);
    let mut a = fixture::spatial_only();
    start(&mut w, &mut a);
    for _ in 0..10 {
        advance(&mut w, &mut a);
    }
    assert!(w.state.casts.is_empty());
    let cp = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    for case in 0..20 {
        let mut changed = cp.clone();
        let combat = &mut changed["state"]["combat"];
        match case {
            0 => combat["melee_timers"]["remaining_ms"][0] = serde_json::json!(123),
            1 => combat["melee_timers"]["melee_attacking"] = serde_json::json!(true),
            2 => {
                combat["melee_timers"]["victim"] =
                    serde_json::to_value(w.state.npcs[&2].id).unwrap()
            }
            3 => combat["melee_timers"]["shapeshift_form"] = serde_json::json!(1),
            4 => {
                combat["melee_timers"]["attack_speed_factors"][0] = serde_json::json!(0x40000000u32)
            }
            5 => combat["melee_reset"]["intervals_ms"][0] = serde_json::json!(2500),
            6 => combat["melee_reset"]["released"]["captured_ms"] = serde_json::json!(200),
            7 => combat["melee_reset"]["player"]["owner"] = serde_json::to_value([9; 16]).unwrap(),
            8 => combat["melee_reset"]["target"]["generation"] = serde_json::json!(99),
            9 => combat["melee_reset"] = serde_json::Value::Null,
            10 => combat["melee_reset"]["cast_id"] = serde_json::json!(2),
            11 => {
                combat["melee_reset"]["release_receipt"]["request"]["binding"]["npc"]
                    ["generation"] = serde_json::json!(1)
            }
            12 => {
                combat["melee_reset"]["release_receipt"]["request"]["binding"]["tick"] =
                    serde_json::json!(2)
            }
            13 => combat["melee_reset"]["origin"]["x"] = serde_json::json!(f::real(1.0).bits()),
            14 => {
                combat["melee_reset"]["release_receipt"]["request"]["query"]["SourceSpell"]
                    ["Semantics"]["scheduler"]["due"]["add_time_ms"] = serde_json::json!(100)
            }
            15 => {
                combat["melee_reset"]["release_receipt"]["request"]["query"]["SourceSpell"]
                    ["Semantics"]["scheduler"]["due"]["scheduled_ms"] = serde_json::json!(300)
            }
            16 => {
                combat["melee_reset"]["release_receipt"]["request"]["query"]["SourceSpell"]
                    ["Semantics"]["history"] = serde_json::to_value([0; 32]).unwrap()
            }
            17 => {
                combat["melee_reset"]["release_receipt"]["request"]["binding"]["world"] =
                    serde_json::to_value([7; 16]).unwrap()
            }
            18 => combat["melee_reset"]["execution"]["pushbacks"] = serde_json::json!(1),
            _ => combat["melee_reset"]["execution"]["pushback_added_ms"] = serde_json::json!(1),
        }
        if matches!(case, 11 | 12 | 14..=17) {
            // Exercise semantic bindings even after an attacker reseals both
            // ordinary integrity digests, not only the receipt checksum.
            let receipt: AuthorityReply =
                serde_json::from_value(combat["melee_reset"]["release_receipt"].clone()).unwrap();
            combat["melee_reset"]["release_receipt_digest"] = serde_json::to_value(crate::digest(
                b"benilla-native-melee-release-receipt-v38\0",
                &receipt,
            ))
            .unwrap();
        }
        assert_ne!(changed, cp, "case{case} must change the retained evidence");
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), rehash(changed))
                .is_err(),
            "case{case}"
        );
    }
    for field in ["melee_reset", "melee_timers"] {
        let mut missing = cp.clone();
        missing["state"]["combat"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<Checkpoint>(missing).is_err());
    }
}

#[test]
fn previous_native_release_receipts_reject_after_current_checkpoint_rehash() {
    let mut w = world(7);
    let mut authority = fixture::spatial_only();
    start(&mut w, &mut authority);
    for _ in 0..10 {
        advance(&mut w, &mut authority);
    }
    assert!(w.state.casts.is_empty());
    let checkpoint = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    let retained = &checkpoint["state"]["combat"]["melee_reset"];
    let receipt: AuthorityReply =
        serde_json::from_value(retained["release_receipt"].clone()).unwrap();
    assert_eq!(
        retained["release_receipt_digest"],
        serde_json::to_value(crate::digest(
            b"benilla-native-melee-release-receipt-v38\0",
            &receipt,
        ))
        .unwrap()
    );
    for (version, policy) in [
        (35, "benilla-native-corpse-carried-items-v35"),
        (36, "benilla-native-source-combat-timers-v36"),
        (37, "benilla-native-initial-cast-genesis-v37"),
    ] {
        for old_receipt_domain in [false, true] {
            let mut forged = checkpoint.clone();
            let reset = &mut forged["state"]["combat"]["melee_reset"];
            let mut receipt: AuthorityReply =
                serde_json::from_value(reset["release_receipt"].clone()).unwrap();
            if !old_receipt_domain {
                receipt.request.binding.profile =
                    crate::native::tests::profile_identity_for_format(w.profile(), version, policy);
                reset["release_receipt"] = serde_json::to_value(&receipt).unwrap();
            }
            reset["release_receipt_digest"] = serde_json::to_value(crate::digest(
                format!(
                    "benilla-native-melee-release-receipt-v{}\0",
                    if old_receipt_domain { version } else { 38 }
                )
                .as_bytes(),
                &receipt,
            ))
            .unwrap();
            let decoded = rehash(forged);
            assert!(
                NativeWorld::restore_synthetic_component_test(w.profile.clone(), decoded).is_err()
            );
        }
    }
}

#[test]
fn previous_native_source_phase_profiles_reject_with_rehashed_record_chain() {
    let mut w = world(7);
    let initial = w.checkpoint().unwrap();
    let mut authority = fixture::spatial_only();
    start(&mut w, &mut authority);
    for _ in 0..10 {
        advance(&mut w, &mut authority);
    }
    for (version, policy) in [
        (35, "benilla-native-corpse-carried-items-v35"),
        (36, "benilla-native-source-combat-timers-v36"),
        (37, "benilla-native-initial-cast-genesis-v37"),
    ] {
        for scheduled in [false, true] {
            let mut records = w.records().to_vec();
            let mut changed = 0;
            for record in &mut records {
                for reply in &mut record.replies {
                    let context = match &reply.request.query {
                        Query::SourceSpell(SourceSpellQuery::Semantics(context)) => context,
                        _ => continue,
                    };
                    if context.scheduler.is_some() != scheduled {
                        continue;
                    }
                    reply.request.binding.profile =
                        crate::native::tests::profile_identity_for_format(
                            w.profile(),
                            version,
                            policy,
                        );
                    changed += 1;
                }
            }
            assert!(changed > 0, "scheduled={scheduled}");
            for index in 1..records.len() {
                records[index].previous_record = records[index - 1].digest();
            }
            let decoded = decode_records(&encode_records(&records).unwrap()).unwrap();
            let mut replay =
                NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial.clone())
                    .unwrap();
            assert_eq!(replay.replay(&decoded), Err(NativeError::ReplayMismatch));
            assert_eq!(replay.checkpoint().unwrap(), initial);
            assert!(replay.records().is_empty());
        }
    }
}

#[test]
fn complete_hook_scope_and_every_incoming_control_source_are_required() {
    for case in 0..10 {
        let (mut p, initial, mut source) = inputs();
        match case {
            0 => source.modifier_spell_ids = SpellSourceRows::Unknown,
            1 => source.modifier_spell_ids = SpellSourceRows::Complete(vec![]),
            2 => source.modifier_spell_ids = SpellSourceRows::Complete(vec![133, 2479]),
            3 => source.modifier_hooks.spell_proc_event = SpellSourceRows::Complete(vec![133]),
            4 => {
                source.modifier_hooks.spell_script_names =
                    SpellSourceRows::Complete(vec!["aura_script".into()])
            }
            5 => source.modifier_hooks.spell_affect = SpellSourceRows::Complete(vec![133]),
            6 => source.npcs[0].addon_auras = SpellSourceRows::Complete(vec![12]),
            7 => source.npcs[0].spawn_addon_auras = SpellSourceRows::Complete(vec![27]),
            8 => source.npcs[0].spell_list_id = 68,
            _ => source.npcs[0].script_id = 1,
        }
        let before = p.clone();
        assert!(
            p.initialize_spell_authority(&initial, source).is_err(),
            "case{case}"
        );
        assert_eq!(p, before);
    }
}

#[test]
fn coefficients_and_raw_sentinels_are_exact_not_wildcards() {
    let (_, _, source) = inputs();
    for case in 0..12 {
        let mut raw = source.spells[0].clone();
        match case {
            0 => raw.bonus_coefficients[0] = 0x7fc00000,
            1 => raw.bonus_coefficients[0] = 0xbdfbe76d,
            2 => raw.bonus_coefficients[0] = 0x3f800000,
            3 => raw.attack_power_coefficients[0] = 1,
            4 => raw.words[6] |= 1,
            5 => raw.words[161] |= 2,
            6 => raw.words[25] = 100,
            7 => raw.words[59] = 1,
            8 => raw.words[169] = 0x40000000,
            9 => raw.words[84] = 6,
            10 => raw.words[21] |= 16,
            _ => raw.words[24] = 1,
        }
        assert!(raw.validate().is_err(), "case{case}");
    }
}

#[test]
fn combat_zero_delay_boundary_is_explicit_and_source_speed24_has_positive_minimum() {
    for distance in [
        0.0,
        -0.0,
        0.0001,
        1.0,
        24.99,
        25.0,
        25.01,
        900.0,
        1_000_000.0,
    ] {
        let delay = spell::projectile_delay(f::float(distance), f::float(24.0)).unwrap();
        assert!(delay >= 208, "squared distance{distance}");
        if distance <= 25.0 {
            assert_eq!(delay, 208);
        }
    }
    for speed_bits in [
        5000.0f32.to_bits() - 1,
        5000.0f32.to_bits(),
        5000.0f32.to_bits() + 1,
        1_000_000.0f32.to_bits(),
    ] {
        let (mut p, mut initial, mut source) = inputs();
        let speed = spell::SpellFloat::from_bits(speed_bits).unwrap();
        let zero = spell::projectile_delay(spell::SpellFloat::ZERO, speed).unwrap() == 0;
        let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
            unreachable!()
        };
        spec.projectile_speed = speed;
        source.spells[0].words[37] = speed_bits;
        initial.spells[0].words[37] = speed_bits;
        p.initialize_player(&initial, f::float(100.0), 1).unwrap();
        let before = p.clone();
        let result = p.initialize_spell_authority(&initial, source);
        assert_eq!(result.is_err(), zero);
        if zero {
            assert_eq!(
                result,
                Err(NativeError::UnsupportedMechanic(
                    "zero-delay combat-reset release history"
                ))
            );
            assert_eq!(p, before);
        }
    }
    // The new history restriction does not alter the legacy non-COMBAT host.
    let (mut p, mut initial, mut source) = fixture::inputs();
    let speed = f::float(1_000_000.0);
    let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        unreachable!()
    };
    spec.projectile_speed = speed;
    initial.spells[0].words[37] = speed.bits();
    source.spells[0].words[37] = speed.bits();
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    p.initialize_spell_authority(&initial, source).unwrap();
}

#[test]
fn unchanged_add_unit_target_source_projectile_boundary_vectors_match() {
    use sha2::{Digest as _, Sha256};
    // Exact unchanged Spell.cpp960–981 source block SHA256:
    // 794dea5c72afdd0d932f9dc3d5fce6b456a55e0fa1d36fb2edd6f3b6ec7d0810
    // Synthetic numeric inputs only; no installed client rows or lifecycle claim.
    let rows = include_str!("spell_projectile_delay_oracle.tsv");
    assert_eq!(
        format!("{:x}", Sha256::digest(rows.as_bytes())),
        "9bd1bc69fb21713fea6234c561a3a5f746e0c7e3ae1b2de8b2093e7df7d24623"
    );
    let mut count = 0;
    let mut rejected = 0;
    for row in rows.lines().skip(1) {
        let values = row.split('\t').collect::<Vec<_>>();
        assert_eq!(values.len(), 6);
        let distance_bits = values[1].parse::<u32>().unwrap();
        let distance = spell::SpellFloat::from_bits(distance_bits);
        // The independent C++ oracle also records subnormal and above1e6
        // distances. Preserve these as explicit native-domain rejection cases.
        if matches!(distance_bits, 1 | 1_232_348_161) {
            assert!(distance.is_err(), "{row}");
            rejected += 1;
            continue;
        }
        let distance = distance.unwrap();
        let speed = spell::SpellFloat::from_bits(values[3].parse().unwrap()).unwrap();
        assert_eq!(
            spell::projectile_delay(distance, speed).unwrap(),
            values[5].parse::<u32>().unwrap(),
            "{row}"
        );
        count += 1;
    }
    assert_eq!((count, rejected), (141, 16));
}

#[test]
fn inactive_known_spell_interrupts_do_not_become_live_holder_dependencies() {
    let (mut p, mut initial, source) = inputs();
    let raw = &mut initial.spells[1];
    raw.id = 2479;
    raw.words[0] = 2479;
    raw.words[22] = 4096;
    if let init::Rows::Complete(rows) = &mut initial.default_spells {
        rows[1] = 2479;
    }
    if let init::Rows::Complete(rows) = &mut initial.skill_abilities {
        rows[1].spell = 2479;
    }
    initial.active_support[1].spell_id = 2479;
    p.abilities[1].id = 2479;
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    assert!(p.combat.as_ref().unwrap().initial_auras.is_empty());
    assert!(matches!(
        &p.abilities[1].admission,
        AbilityAdmission::Unavailable { .. }
    ));
    p.initialize_spell_authority(&initial, source).unwrap();
    p.validate().unwrap();
}

#[test]
fn actual_initial_holders_expand_hook_scope_and_retained_raw_rows_are_revalidated() {
    let (mut p, mut initial, mut source) = inputs();
    let mut passive = initial.spells[1].clone();
    passive.id = 60100;
    passive.words = [0; 173];
    passive.words[0] = 60100;
    passive.words[6] = 0x40;
    passive.words[58] = u32::MAX;
    passive.words[61] = 6;
    passive.words[76] = 4;
    passive.words[82] = 1;
    passive.words[91] = 29;
    passive.words[106] = 3;
    passive.duration_ms = Some(0);
    initial.spells.push(passive);
    if let init::Rows::Complete(rows) = &mut initial.default_spells {
        rows.push(60100);
    }
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    assert_eq!(p.combat.as_ref().unwrap().initial_auras[0].spell_id, 60100);
    assert!(p
        .initialize_spell_authority(&initial, source.clone())
        .is_err());
    source.modifier_spell_ids = SpellSourceRows::Complete(vec![133, 60100]);
    source.initial_cast_dependencies =
        SpellSourceRows::Complete(vec![initial_cast_tests::authored_dependency(60100, 0, 0)]);
    p.initialize_spell_authority(&initial, source).unwrap();
    for word in [
        6, 9, 16, 22, 61, 64, 67, 70, 73, 76, 91, 106, 109, 112, 160, 161, 162,
    ] {
        let mut changed = p.clone();
        let catalog = changed
            .interactions
            .as_mut()
            .unwrap()
            .spell_authority
            .as_mut()
            .unwrap();
        let row = catalog
            .initial_modifiers
            .spell_words
            .iter_mut()
            .find(|r| r[0] == 60100)
            .unwrap();
        row[word] ^= 1;
        assert!(changed.validate().is_err(), "raw word{word}");
    }
    let mut changed = p.clone();
    changed
        .interactions
        .as_mut()
        .unwrap()
        .spell_authority
        .as_mut()
        .unwrap()
        .modifier_spell_ids = SpellSourceRows::Complete(vec![133]);
    assert!(changed.validate().is_err());
}
