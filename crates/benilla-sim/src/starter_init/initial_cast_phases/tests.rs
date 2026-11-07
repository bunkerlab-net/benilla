// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicitly authored mechanism rows and resolutions, not captured game data.
use super::*;

pub(crate) fn authored(id: u32) -> AuthoredInitialCast {
    let s = shape(id).unwrap();
    let holder = s.effects[0].0 == 6;
    let weapon_group = matches!(id, 81 | 203 | 204 | 227 | 522 | 668 | 2382 | 5009);
    let mut w = [0; 173];
    w[0] = id;
    w[6] = if holder || id == 81 { 0x50 } else { 0xc0 };
    w[7] = if id == 3050 { 0x0200_0000 } else { 0 };
    w[18] = u32::from(id != 81);
    w[25] = 101;
    w[28] = u32::from(id == 81);
    w[29] = u32::from(id == 81 || id == 668);
    w[30] = if holder { 21 } else { 0 };
    w[36] = 1;
    w[58] = s.item_class as u32;
    w[59] = s.item_mask;
    w[164] = u32::from(weapon_group);
    w[165] = u32::from(weapon_group);
    w[166] = if holder { 0 } else { u32::MAX };
    let mut coefficients = [0; 3];
    for slot in 0..3 {
        w[167 + slot] = if !holder || slot < s.effects.len() {
            ONE
        } else {
            0
        };
    }
    for (slot, &(opcode, aura, misc)) in s.effects.iter().enumerate() {
        w[61 + slot] = opcode;
        w[82 + slot] = u32::from(holder);
        w[91 + slot] = aura;
        w[106 + slot] = misc as u32;
        if holder {
            w[64 + slot] = 1;
            w[67 + slot] = 1;
            w[76 + slot] = if id == 20599 { 9 } else { 4 };
        }
        if holder || id == 81 {
            coefficients[slot] = ONE;
        }
    }
    if id == 81 {
        w[64] = 6;
        w[97] = ONE;
    }
    if id == 3050 {
        w[64] = 1;
        w[67] = 1;
        w[73] = 0x40a0_0000;
        w[76] = u32::MAX;
    }
    let duration_ms = if holder { -1 } else { 0 };
    AuthoredInitialCast {
        row: SpellRow {
            id,
            words: w,
            duration_ms: Some(duration_ms),
            previous_rank: None,
            talent: false,
            learned_spells: Rows::Complete(vec![]),
            external_initial_hooks: false,
        },
        prerequisites: Some(AuthoredPhasePrerequisites {
            fresh_unshapeshifted_player_not_in_world: true,
            successful_conditions_and_location_checks: true,
            spell_and_aura_scripts_absent: true,
            spell_modifiers_absent: true,
            neutral_proc_and_finish_paths: true,
            server_side: 0,
            server_attributes: 0,
            effect_bonus_coefficient_bits: coefficients,
            effect_bonus_coefficient_from_ap_bits: [0; 3],
        }),
        resolutions: Some(AuthoredPhaseResolutions {
            cast_time_index: w[18],
            cast_time_ms: 0,
            duration_index: w[30],
            duration_ms,
        }),
    }
}
fn zero_masks() -> InitialProficiencyMasks {
    InitialProficiencyMasks {
        weapon: 0,
        armor: 0,
    }
}
fn plan(casts: &[AuthoredInitialCast]) -> InitialCastPlan {
    InitialCastPlan::from_authored(casts).unwrap()
}

#[test]
fn authored_projection_retains_all_fifteen_occurrences_nineteen_effects_and_masks() {
    let ids = [
        81, 203, 204, 227, 522, 668, 2382, 3050, 5009, 9078, 9125, 20597, 20598, 20599, 20864,
    ];
    let casts: Vec<_> = ids.into_iter().map(authored).collect();
    let p = plan(&casts);
    assert_eq!(
        p.occurrences()
            .iter()
            .map(|c| c.row().id)
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(
        p.occurrences()[7].effects()[0].phase,
        InitialEffectPhase::UnscheduledDetect
    );
    let r = p.evaluate(&[4], zero_masks()).unwrap();
    assert_eq!(r.draws().len(), 1);
    assert_eq!(
        r.events()
            .iter()
            .filter(|e| matches!(e, InitialPhaseEvent::Dispatch { .. }))
            .count(),
        19
    );
    assert_eq!(
        r.events()
            .iter()
            .filter(|e| matches!(
                e,
                InitialPhaseEvent::Dispatch {
                    phase: InitialEffectPhase::Targetless,
                    ..
                }
            ))
            .count(),
        13
    );
    assert_eq!(
        r.final_masks(),
        InitialProficiencyMasks {
            weapon: 0x84400,
            armor: 3
        }
    );
    assert_eq!(r.holders().len(), 4);
    assert_eq!(
        r.holders()
            .iter()
            .flat_map(|h| &h.auras)
            .map(|a| (a.aura_type, a.misc_value, a.amount))
            .collect::<Vec<_>>(),
        [
            (98, 43, 5),
            (98, 55, 5),
            (137, 4, 5),
            (156, 0, 10),
            (98, 54, 5),
            (98, 160, 5)
        ]
    );
    p.verify_evaluation(&r).unwrap();
}

#[test]
fn dodge_requests_and_evaluates_every_outcome_before_its_empty_handler() {
    let p = plan(&[authored(81)]);
    assert_eq!(
        p.draw_requests(),
        [InitialIntegerDrawRequest {
            occurrence: 0,
            spell_id: 81,
            effect_slot: 0,
            low: 0,
            high: 6
        }]
    );
    for value in 0..=6 {
        let r = p.evaluate(&[value], zero_masks()).unwrap();
        assert_eq!(
            r.events(),
            &[
                InitialPhaseEvent::CastStart {
                    occurrence: 0,
                    spell_id: 81
                },
                InitialPhaseEvent::IntegerDraw(r.draws()[0]),
                InitialPhaseEvent::Amount {
                    occurrence: 0,
                    slot: 0,
                    phase: InitialEffectPhase::Targetless,
                    value
                },
                InitialPhaseEvent::Dispatch {
                    occurrence: 0,
                    slot: 0,
                    phase: InitialEffectPhase::Targetless,
                    opcode: 20
                },
                InitialPhaseEvent::CastComplete {
                    occurrence: 0,
                    spell_id: 81
                },
            ]
        );
        assert!(r.holders().is_empty());
        assert_eq!(r.final_masks(), zero_masks());
    }
    for rolls in [&[][..], &[-1][..], &[7][..], &[0, 1][..]] {
        assert!(p.evaluate(rolls, zero_masks()).is_err());
    }
    // Failed evaluation cannot mutate the immutable plan or a prior result.
    assert_eq!(p, plan(&[authored(81)]));
}

#[test]
fn detect_records_a_cast_without_amount_scaling_rank_draw_or_handler() {
    let p = plan(&[authored(3050)]);
    assert_eq!(p.occurrences()[0].targetless_mask(), 0);
    assert_eq!(p.occurrences()[0].self_mask(), 0);
    assert_eq!(p.occurrences()[0].effects()[0].amount, None);
    assert_eq!(p.occurrences()[0].row().words[73], 0x40a0_0000);
    let r = p.evaluate(&[], zero_masks()).unwrap();
    assert_eq!(
        r.events(),
        &[
            InitialPhaseEvent::CastStart {
                occurrence: 0,
                spell_id: 3050
            },
            InitialPhaseEvent::CastComplete {
                occurrence: 0,
                spell_id: 3050
            },
        ]
    );
    assert!(p.evaluate(&[0], zero_masks()).is_err());
    assert!(r.holders().is_empty());
}

#[test]
fn racial_holders_construct_both_slots_before_any_registration_or_application() {
    for (id, skills) in [(20597, [43, 55]), (20864, [54, 160])] {
        let p = plan(&[authored(id)]);
        let r = p.evaluate(&[], zero_masks()).unwrap();
        assert_eq!(r.holders().len(), 1);
        assert_eq!(
            r.holders()[0]
                .auras
                .iter()
                .map(|a| a.misc_value)
                .collect::<Vec<_>>(),
            skills
        );
        assert_eq!(
            r.holders()[0]
                .auras
                .iter()
                .map(|a| a.effect_index)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(r.holders()[0].source_hash, p.occurrences()[0].source_hash());
        let position = |predicate: fn(&InitialPhaseEvent) -> bool| {
            r.events().iter().position(predicate).unwrap()
        };
        assert!(
            position(|e| matches!(e, InitialPhaseEvent::HolderCreated { .. }))
                < position(|e| matches!(e, InitialPhaseEvent::Amount { .. }))
        );
        assert!(
            position(|e| matches!(
                e,
                InitialPhaseEvent::AuraConstructed {
                    aura: InitialAura {
                        effect_index: 1,
                        ..
                    },
                    ..
                }
            )) < position(|e| matches!(e, InitialPhaseEvent::AuraRegistered { .. }))
        );
        assert!(
            position(|e| matches!(e, InitialPhaseEvent::AuraRegistered { slot: 1, .. }))
                < position(|e| matches!(e, InitialPhaseEvent::AuraApplied { .. }))
        );
        // There is no skill insertion API: both auras survive independently of skills.
    }
}

#[test]
fn occurrence_identity_includes_no_state_casts_raw_words_and_authored_order() {
    let a = plan(&[authored(81), authored(3050), authored(81), authored(203)]);
    assert_eq!(
        a.draw_requests()
            .iter()
            .map(|r| r.occurrence)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    let b = plan(&[authored(81), authored(81), authored(3050), authored(203)]);
    let c = plan(&[authored(81), authored(81), authored(203)]);
    let ar = a.evaluate(&[0, 6], zero_masks()).unwrap();
    assert_eq!(
        ar.final_masks(),
        b.evaluate(&[0, 6], zero_masks()).unwrap().final_masks()
    );
    assert_ne!(a.identity(), b.identity());
    assert_ne!(a.identity(), c.identity());
    assert!(b.verify_evaluation(&ar).is_err());
    let mut modified = authored(3050);
    modified.row.words[117] = 1234; // Retained visual field still changes identity.
    assert_ne!(
        plan(&[modified]).identity(),
        plan(&[authored(3050)]).identity()
    );
    let mut altered = ar.clone();
    altered
        .events
        .retain(|e| !matches!(e, InitialPhaseEvent::IntegerDraw(_)));
    assert!(a.verify_evaluation(&altered).is_err());
    let mut altered = ar.clone();
    altered.draws.swap(0, 1);
    assert!(a.verify_evaluation(&altered).is_err());
    let mut altered = ar;
    altered.events.remove(5); // Omit a no-effect cast marker.
    assert!(a.verify_evaluation(&altered).is_err());
}

#[test]
fn no_draw_sides_and_binary32_cached_guards_match_existing_amount_vectors() {
    for line in include_str!("../../../tests/fixtures/starter_effect_amount_cmangos.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let v: Vec<i64> = line.split('\t').map(|s| s.parse().unwrap()).collect();
        let mut cast = authored(203);
        cast.row.words[76] = v[0] as i32 as u32;
        cast.row.words[67] = v[1] as u32;
        cast.row.words[64] = v[2] as u32;
        let p = plan(&[cast]);
        assert!(p.draw_requests().is_empty());
        assert!(p.evaluate(&[0], zero_masks()).is_err());
        let r = p.evaluate(&[], zero_masks()).unwrap();
        assert!(r.events().contains(&InitialPhaseEvent::Amount {
            occurrence: 0,
            slot: 0,
            phase: InitialEffectPhase::Targetless,
            value: v[3] as i32
        }));
    }
    for (base, dice) in [(i32::MAX, 1), (i32::MAX, 0), (0, u32::MAX)] {
        let mut cast = authored(203);
        cast.row.words[76] = base as u32;
        cast.row.words[67] = dice;
        assert!(InitialCastPlan::from_authored(&[cast]).is_err());
    }
    let mut cast = authored(20597);
    cast.row.words[76] = 32767;
    cast.row.words[67] = 1;
    assert!(matches!(
        InitialCastPlan::from_authored(&[cast]),
        Err(InitError::Numeric("initial skill aura amount"))
    ));
}

#[test]
fn requirements_are_explicit_bound_and_never_inferred_from_an_index() {
    for mutate in [
        |c: &mut AuthoredInitialCast| c.prerequisites = None,
        |c: &mut AuthoredInitialCast| c.resolutions = None,
        |c: &mut AuthoredInitialCast| c.row.duration_ms = None,
        |c: &mut AuthoredInitialCast| c.resolutions.as_mut().unwrap().cast_time_index += 1,
        |c: &mut AuthoredInitialCast| c.resolutions.as_mut().unwrap().duration_index += 1,
        |c: &mut AuthoredInitialCast| c.resolutions.as_mut().unwrap().cast_time_ms = 1,
        |c: &mut AuthoredInitialCast| c.resolutions.as_mut().unwrap().duration_ms = 0,
        |c: &mut AuthoredInitialCast| {
            c.prerequisites
                .as_mut()
                .unwrap()
                .fresh_unshapeshifted_player_not_in_world = false
        },
        |c: &mut AuthoredInitialCast| {
            c.prerequisites
                .as_mut()
                .unwrap()
                .successful_conditions_and_location_checks = false
        },
        |c: &mut AuthoredInitialCast| {
            c.prerequisites
                .as_mut()
                .unwrap()
                .spell_and_aura_scripts_absent = false
        },
        |c: &mut AuthoredInitialCast| {
            c.prerequisites.as_mut().unwrap().spell_modifiers_absent = false
        },
        |c: &mut AuthoredInitialCast| {
            c.prerequisites
                .as_mut()
                .unwrap()
                .neutral_proc_and_finish_paths = false
        },
    ] {
        let mut cast = authored(20597);
        mutate(&mut cast);
        assert!(InitialCastPlan::from_authored(&[cast]).is_err());
    }
    let mut cast = authored(20597);
    cast.row.duration_ms = Some(0);
    cast.resolutions.as_mut().unwrap().duration_ms = 0;
    assert!(InitialCastPlan::from_authored(&[cast.clone()]).is_err());
    cast.row.words[30] = 0;
    cast.resolutions.as_mut().unwrap().duration_index = 0;
    assert!(InitialCastPlan::from_authored(&[cast]).is_ok()); // Explicit authored index0 branch.
}

#[test]
fn classifier_rejects_extra_effects_hooks_children_targets_and_broad_opcodes() {
    for id in [81, 3050, 227, 20597, 20599, 20864] {
        for index in [
            2, 4, 5, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 19, 20, 21, 22, 23, 24, 26, 31, 32, 37,
            39, 40, 60, 63, 70, 79, 85, 88, 94, 100, 103, 109, 112, 156, 157, 158, 159, 160, 161,
            162, 163, 170, 171, 172,
        ] {
            let mut cast = authored(id);
            cast.row.words[index] = 1;
            assert!(
                InitialCastPlan::from_authored(&[cast]).is_err(),
                "id={id}, cell={index}"
            );
        }
        for mutate in [
            |c: &mut AuthoredInitialCast| c.row.external_initial_hooks = true,
            |c: &mut AuthoredInitialCast| c.row.previous_rank = Some(1),
            |c: &mut AuthoredInitialCast| c.row.talent = true,
            |c: &mut AuthoredInitialCast| c.row.learned_spells = Rows::Missing,
            |c: &mut AuthoredInitialCast| {
                c.row.learned_spells = Rows::Complete(vec![super::super::LearnSpell {
                    spell_id: 7,
                    active: true,
                    auto_learned: false,
                }])
            },
            |c: &mut AuthoredInitialCast| c.prerequisites.as_mut().unwrap().server_attributes = 1,
            |c: &mut AuthoredInitialCast| c.prerequisites.as_mut().unwrap().server_side = 1,
            |c: &mut AuthoredInitialCast| {
                c.prerequisites
                    .as_mut()
                    .unwrap()
                    .effect_bonus_coefficient_bits[0] ^= 1
            },
            |c: &mut AuthoredInitialCast| {
                c.prerequisites
                    .as_mut()
                    .unwrap()
                    .effect_bonus_coefficient_from_ap_bits[0] = ONE
            },
        ] {
            let mut cast = authored(id);
            mutate(&mut cast);
            assert!(InitialCastPlan::from_authored(&[cast]).is_err());
        }
    }
    for index in [61, 64, 67, 73, 76, 82, 91, 106] {
        let mut cast = authored(3050);
        cast.row.words[index] ^= 1;
        assert!(
            InitialCastPlan::from_authored(&[cast]).is_err(),
            "Detect cell={index}"
        );
    }
    for id in [20597, 20864] {
        for index in [62, 83, 92, 107] {
            let mut cast = authored(id);
            cast.row.words[index] = 0;
            assert!(InitialCastPlan::from_authored(&[cast]).is_err());
        }
        let mut cast = authored(id);
        cast.row.words[107] = cast.row.words[106];
        assert!(InitialCastPlan::from_authored(&[cast]).is_err());
    }
    for id in [203, 227, 20597] {
        let mut cast = authored(id);
        cast.row.words[64] = 6;
        assert!(InitialCastPlan::from_authored(&[cast]).is_err());
    }
    for id in [203, 20598] {
        for cell in [70, 73, 112] {
            let mut cast = authored(id);
            cast.row.words[cell] = ONE;
            assert!(InitialCastPlan::from_authored(&[cast]).is_err());
        }
    }
    let mut cast = authored(3050);
    cast.row.id = 49;
    cast.row.words[0] = 49;
    assert!(InitialCastPlan::from_authored(&[cast]).is_err());
    assert!(InitialCastPlan::from_authored(&[authored(20597), authored(20597)]).is_err());
    assert!(
        InitialCastPlan::from_authored(&vec![authored(203); MAX_AUTHORED_INITIAL_CASTS + 1])
            .is_err()
    );
}

/// Test-only scheduling constructor deliberately bypasses morphology, never
/// exported. It tests phase mechanics without expanding production admission.
fn mechanism_plan(
    cast: AuthoredInitialCast,
    effects: Vec<InitialEffectOperation>,
) -> InitialCastPlan {
    let occurrences = vec![InitialCastOccurrencePlan {
        ordinal: 0,
        source_hash: cast.row.source_hash(),
        row: cast.row.clone(),
        provenance: InitialCastProvenance::Authored(Box::new(cast)),
        effects,
    }];
    let identity = plan_identity(&occurrences);
    InitialCastPlan {
        occurrences,
        identity,
    }
}

#[test]
fn mixed_phase_probe_runs_targetless_before_self_even_when_self_has_slot_zero() {
    let mut cast = authored(20598);
    cast.row.words[62] = 25;
    let effects = vec![
        InitialEffectOperation {
            slot: 0,
            opcode: 6,
            phase: InitialEffectPhase::SelfHolder,
            aura: 137,
            misc: 4,
            amount: Some(InitialAmountOperation::Constant { value: 5 }),
        },
        InitialEffectOperation {
            slot: 1,
            opcode: 25,
            phase: InitialEffectPhase::Targetless,
            aura: 0,
            misc: 0,
            amount: Some(InitialAmountOperation::Constant { value: 0 }),
        },
    ];
    assert!(InitialCastPlan::from_authored(&[cast.clone()]).is_err());
    let p = mechanism_plan(cast, effects);
    let r = p.evaluate(&[], zero_masks()).unwrap();
    assert_eq!(
        r.events()
            .iter()
            .filter_map(|e| if let InitialPhaseEvent::Dispatch { slot, .. } = e {
                Some(*slot)
            } else {
                None
            })
            .collect::<Vec<_>>(),
        [1, 0]
    );
    assert!(matches!(
        r.events()[3],
        InitialPhaseEvent::HolderCreated { .. }
    ));
}

#[test]
fn full_mask_overlap_skips_whole_grant_instead_of_filling_missing_bits() {
    let mut cast = authored(2382);
    cast.row.words[59] = 3; // Mechanism-only adversarial mask, not an audited row.
    assert!(InitialCastPlan::from_authored(&[cast.clone()]).is_err());
    let effect = InitialEffectOperation {
        slot: 1,
        opcode: 60,
        phase: InitialEffectPhase::Targetless,
        aura: 0,
        misc: 0,
        amount: Some(InitialAmountOperation::Constant { value: 0 }),
    };
    let p = mechanism_plan(cast, vec![effect]);
    assert_eq!(
        p.evaluate(
            &[],
            InitialProficiencyMasks {
                weapon: 1,
                armor: 0
            }
        )
        .unwrap()
        .final_masks()
        .weapon,
        1
    );
    assert_eq!(
        p.evaluate(&[], zero_masks()).unwrap().final_masks().weapon,
        3
    );
}
