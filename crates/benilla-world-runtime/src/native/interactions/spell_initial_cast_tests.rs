// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored rows and indexed resolutions, not client captures or native admission.
use super::*;

pub(super) fn authored_dependency(
    spell_id: u32,
    cast_index: u32,
    duration_index: u32,
) -> InitialCastDependencies {
    InitialCastDependencies {
        spell_id,
        cast_time: InitialSpellLookup::Missing { index: cast_index },
        duration: InitialSpellLookup::Missing {
            index: duration_index,
        },
        aura_script_names: SpellSourceRows::Complete(vec![]),
        server_side: 0,
        server_attributes: 0,
    }
}
fn passive(id: u32) -> init::SpellRow {
    let (aura, misc) = match id {
        20598 => (137, 4),
        20599 => (156, 0),
        _ => (29, 3),
    };
    let mut words = [0; 173];
    words[0] = id;
    words[6] = 0x40;
    words[58] = u32::MAX;
    words[61] = 6;
    words[67] = 1;
    words[76] = 4;
    words[82] = 1;
    words[91] = aura;
    words[106] = misc;
    init::SpellRow {
        id,
        words,
        duration_ms: Some(0),
        previous_rank: None,
        talent: false,
        learned_spells: init::Rows::Complete(vec![]),
        external_initial_hooks: false,
    }
}
fn add_initial(initial: &mut init::StarterInput, row: init::SpellRow) {
    let init::Rows::Complete(defaults) = &mut initial.default_spells else {
        unreachable!()
    };
    defaults.push(row.id);
    initial.spells.push(row);
}
fn with_passive(id: u32) -> (Profile, init::StarterInput, SpellAuthorityInput) {
    let (mut p, mut initial, mut source) = inputs();
    add_initial(&mut initial, passive(id));
    source.modifier_spell_ids = SpellSourceRows::Complete(vec![133, id]);
    source.initial_cast_dependencies =
        SpellSourceRows::Complete(vec![authored_dependency(id, 0, 0)]);
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    (p, initial, source)
}
fn dependency(source: &mut SpellAuthorityInput) -> &mut InitialCastDependencies {
    let SpellSourceRows::Complete(rows) = &mut source.initial_cast_dependencies else {
        unreachable!()
    };
    &mut rows[0]
}

#[test]
fn empty_actual_trace_requires_explicit_empty_dependencies() {
    let (mut p, initial, source) = inputs();
    p.initialize_spell_authority(&initial, source).unwrap();
    let catalog = p
        .interactions
        .as_ref()
        .unwrap()
        .spell_authority
        .as_ref()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&catalog.initial_cast).unwrap()["cast_trace"],
        serde_json::json!([])
    );
    for unknown in [false, true] {
        let (mut p, initial, mut source) = inputs();
        source.initial_cast_dependencies = if unknown {
            SpellSourceRows::Unknown
        } else {
            SpellSourceRows::Complete(vec![authored_dependency(60100, 0, 0)])
        };
        let before = p.clone();
        assert!(p.initialize_spell_authority(&initial, source).is_err());
        assert_eq!(p, before);
    }
}

#[test]
fn audited_holder_ids_require_exact_shape_and_source_bound_amounts() {
    // Diplomacy's raw predicate is covered in initial_cast_source's unit test;
    // a runtime reputation consumer additionally needs its own bound genesis.
    for id in [20598, 60100] {
        let (mut p, initial, source) = with_passive(id);
        p.initialize_spell_authority(&initial, source).unwrap();
        assert_eq!(p.combat.as_ref().unwrap().initial_auras[0].amount, 5);
        p.validate().unwrap();
    }
    // A numerically neutral aura cannot disguise an unaudited hardcoded ID.
    for id in [20594, 24575, 32053, 81, 20597, 20864] {
        let (mut p, initial, source) = with_passive(id);
        let before = p.clone();
        assert!(
            p.initialize_spell_authority(&initial, source).is_err(),
            "ID{id}"
        );
        assert_eq!(p, before);
    }
}

#[test]
fn complete_recursive_trace_preserves_order_and_known_spell_deduplication() {
    let (mut p, mut initial, mut source) = inputs();
    let mut parent = passive(60100);
    parent.learned_spells = init::Rows::Complete(vec![init::LearnSpell {
        spell_id: 20598,
        active: true,
        auto_learned: false,
    }]);
    add_initial(&mut initial, parent);
    add_initial(&mut initial, passive(20598));
    source.modifier_spell_ids = SpellSourceRows::Complete(vec![133, 20598, 60100]);
    source.initial_cast_dependencies = SpellSourceRows::Complete(vec![
        authored_dependency(20598, 0, 0),
        authored_dependency(60100, 0, 0),
    ]);
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    p.initialize_spell_authority(&initial, source).unwrap();
    let proof = &p
        .interactions
        .as_ref()
        .unwrap()
        .spell_authority
        .as_ref()
        .unwrap()
        .initial_cast;
    assert_eq!(
        serde_json::to_value(proof).unwrap()["cast_trace"],
        serde_json::json!([60100, 20598])
    );
}

#[test]
fn nonholders_and_recursive_nonholder_parents_are_retained_and_rejected() {
    for effect in [20, 36] {
        let (mut p, mut initial, mut source) = inputs();
        let mut row = passive(81);
        row.words[61] = effect;
        row.words[91] = 0;
        row.words[106] = 0;
        if effect == 36 {
            row.words[109] = 20598;
            initial.spells.push(passive(20598));
        }
        add_initial(&mut initial, row);
        let mage = init::initialize_mage(&initial).unwrap();
        let proof =
            InitialCastProof::new(&initial, &mage, SpellSourceRows::Complete(vec![])).unwrap();
        let expected = if effect == 36 {
            vec![81, 20598]
        } else {
            vec![81]
        };
        assert_eq!(proof.cast_ids().collect::<Vec<_>>(), expected);
        if effect == 36 {
            // This initializer trace is retained even though the fresh-entry
            // contract refuses its nondependent recursive learning earlier.
            let before = p.clone();
            assert!(matches!(
                p.initialize_player(&initial, f::float(100.0), 1),
                Err(NativeError::Initialization(init::InitError::Unsupported {
                    domain: "first-entry nondependent learn-spell replay",
                    id: 81,
                    detail: 36,
                }))
            ));
            assert_eq!(p, before);
            continue;
        }
        p.initialize_player(&initial, f::float(100.0), 1).unwrap();
        source.modifier_spell_ids = SpellSourceRows::Complete(
            [133]
                .into_iter()
                .chain(expected.iter().copied())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        );
        source.initial_cast_dependencies = SpellSourceRows::Complete(
            expected
                .iter()
                .map(|id| authored_dependency(*id, 0, 0))
                .collect(),
        );
        let before = p.clone();
        assert!(p.initialize_spell_authority(&initial, source).is_err());
        assert_eq!(p, before);
    }
    // The real Dodge81 effect20 has a 0..6 draw before EffectEmpty. This authored
    // random shape must fail rather than dropping that observable source draw.
    let (_, mut initial, _) = inputs();
    let mut dodge = passive(81);
    dodge.words[61] = 20;
    dodge.words[64] = 6;
    dodge.words[67] = 0;
    dodge.words[91] = 0;
    dodge.words[106] = 0;
    add_initial(&mut initial, dodge);
    assert!(init::initialize_mage(&initial).is_err());
}

#[test]
fn index_zero_one_and_nonzero_require_actual_bound_lookup_evidence() {
    for index in [0, 1, 71] {
        for found in [false, true] {
            let (mut p, mut initial, mut source) = with_passive(60100);
            initial.spells.last_mut().unwrap().words[18] = index;
            dependency(&mut source).cast_time = if found {
                InitialSpellLookup::Found(SpellCastTimeSource {
                    id: index,
                    base_ms: 0,
                    per_level_ms: 0,
                    minimum_ms: 0,
                })
            } else {
                InitialSpellLookup::Missing { index }
            };
            p.initialize_player(&initial, f::float(100.0), 1).unwrap();
            p.initialize_spell_authority(&initial, source).unwrap();
        }
    }
    for case in 0..12 {
        let (mut p, initial, mut source) = with_passive(60100);
        let d = dependency(&mut source);
        match case {
            0 => d.cast_time = InitialSpellLookup::Unknown,
            1 => d.cast_time = InitialSpellLookup::Missing { index: 1 },
            2 => {
                d.cast_time = InitialSpellLookup::Found(SpellCastTimeSource {
                    id: 1,
                    base_ms: 0,
                    per_level_ms: 0,
                    minimum_ms: 0,
                })
            }
            3 => {
                d.cast_time = InitialSpellLookup::Found(SpellCastTimeSource {
                    id: 0,
                    base_ms: 1,
                    per_level_ms: 0,
                    minimum_ms: 0,
                })
            }
            4 => {
                d.cast_time = InitialSpellLookup::Found(SpellCastTimeSource {
                    id: 0,
                    base_ms: 0,
                    per_level_ms: 1,
                    minimum_ms: 0,
                })
            }
            5 => d.duration = InitialSpellLookup::Unknown,
            6 => d.duration = InitialSpellLookup::Missing { index: 1 },
            7 => {
                d.duration = InitialSpellLookup::Found(SpellDurationSource {
                    id: 0,
                    base_ms: -1,
                    per_level_ms: 0,
                    maximum_ms: -1,
                })
            }
            8 => {
                d.duration = InitialSpellLookup::Found(SpellDurationSource {
                    id: 0,
                    base_ms: i32::MIN,
                    per_level_ms: 0,
                    maximum_ms: i32::MIN,
                })
            }
            9 => {
                d.duration = InitialSpellLookup::Found(SpellDurationSource {
                    id: 0,
                    base_ms: 0,
                    per_level_ms: 0,
                    maximum_ms: 1,
                })
            }
            10 => d.aura_script_names = SpellSourceRows::Unknown,
            _ => d.aura_script_names = SpellSourceRows::Complete(vec!["authored_hook".into()]),
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
fn permanent_duration_is_derived_and_bound_to_raw_index_and_initializer() {
    for index in [0, 1, 72] {
        let (mut p, mut initial, mut source) = with_passive(60100);
        let row = initial.spells.last_mut().unwrap();
        row.words[30] = index;
        row.duration_ms = Some(-1);
        dependency(&mut source).duration = InitialSpellLookup::Found(SpellDurationSource {
            id: index,
            base_ms: -1,
            per_level_ms: 0,
            maximum_ms: -1,
        });
        p.initialize_player(&initial, f::float(100.0), 1).unwrap();
        p.initialize_spell_authority(&initial, source).unwrap();
    }
}

#[test]
fn initial_indexed_lookups_cannot_contradict_retained_active_source_rows() {
    for case in 0..3 {
        let (mut p, mut initial, mut source) = with_passive(60100);
        let active = source.spells[0].clone();
        let row = initial.spells.last_mut().unwrap();
        match case {
            0 => {
                row.words[18] = active.cast_time.id;
                dependency(&mut source).cast_time = InitialSpellLookup::Missing {
                    index: active.cast_time.id,
                };
            }
            1 => {
                row.words[18] = active.cast_time.id;
                dependency(&mut source).cast_time =
                    InitialSpellLookup::Found(SpellCastTimeSource {
                        id: active.cast_time.id,
                        base_ms: 0,
                        per_level_ms: 0,
                        minimum_ms: 0,
                    });
            }
            _ => {
                let duration = active.duration.unwrap();
                row.words[30] = duration.id;
                row.duration_ms = Some(-1);
                dependency(&mut source).duration = InitialSpellLookup::Found(SpellDurationSource {
                    id: duration.id,
                    base_ms: -1,
                    per_level_ms: 0,
                    maximum_ms: -1,
                });
            }
        }
        p.initialize_player(&initial, f::float(100.0), 1).unwrap();
        // The local shape/index checks accept each dependency in isolation.
        // Only the already-retained active source makes it contradictory.
        let mage = init::initialize_mage(&initial).unwrap();
        let proof =
            InitialCastProof::new(&initial, &mage, source.initial_cast_dependencies.clone())
                .unwrap();
        proof.validate(&p, [&fixture::hooks()], []).unwrap();
        let before = p.clone();
        assert!(matches!(
            p.initialize_spell_authority(&initial, source),
            Err(NativeError::AuthorityBinding)
        ));
        assert_eq!(p, before);
    }
}

#[test]
fn repeated_initial_indices_require_identical_found_or_missing_evidence() {
    for case in 0..6 {
        let (mut p, mut initial, mut source) = with_passive(20598);
        add_initial(&mut initial, passive(60100));
        source.modifier_spell_ids = SpellSourceRows::Complete(vec![133, 20598, 60100]);
        let mut dependencies = vec![
            authored_dependency(20598, 0, 0),
            authored_dependency(60100, 0, 0),
        ];
        let zero_cast = InitialSpellLookup::Found(SpellCastTimeSource {
            id: 0,
            base_ms: 0,
            per_level_ms: 0,
            minimum_ms: 0,
        });
        let zero_duration = InitialSpellLookup::Found(SpellDurationSource {
            id: 0,
            base_ms: 0,
            per_level_ms: 0,
            maximum_ms: 0,
        });
        match case {
            0 => {} // Identical Missing evidence is consistent.
            1 => {
                for row in &mut dependencies {
                    row.cast_time = zero_cast.clone();
                    row.duration = zero_duration.clone();
                }
            }
            2 => dependencies[1].cast_time = zero_cast,
            3 => dependencies[1].duration = zero_duration,
            4 => {
                dependencies[0].duration = zero_duration;
                dependencies[1].duration = InitialSpellLookup::Found(SpellDurationSource {
                    id: 0,
                    base_ms: -1,
                    per_level_ms: 0,
                    maximum_ms: -1,
                });
                initial.spells.last_mut().unwrap().duration_ms = Some(-1);
            }
            _ => {
                for row in &mut dependencies {
                    row.duration = InitialSpellLookup::Found(SpellDurationSource {
                        id: 0,
                        base_ms: -1,
                        per_level_ms: 0,
                        maximum_ms: -1,
                    });
                }
                for row in initial.spells.iter_mut().filter(|row| row.id != 133) {
                    row.duration_ms = Some(-1);
                }
            }
        }
        source.initial_cast_dependencies = SpellSourceRows::Complete(dependencies);
        p.initialize_player(&initial, f::float(100.0), 1).unwrap();
        let before = p.clone();
        let result = p.initialize_spell_authority(&initial, source);
        if matches!(case, 0 | 1 | 5) {
            result.unwrap();
            p.validate().unwrap();
        } else {
            assert!(matches!(result, Err(NativeError::AuthorityBinding)));
            assert_eq!(p, before);
        }
    }
}

#[test]
fn hidden_mechanics_inactive_targets_and_extra_effects_do_not_enter_full_raw_domain() {
    for (index, value) in [
        (5, 16),
        (79, 1),
        (80, 1),
        (81, 1),
        (37, 1.0f32.to_bits()),
        (83, 5),
        (84, 5),
        (85, 1),
        (86, 1),
        (87, 1),
        (62, 20),
        (63, 20),
        (25, 100),
        (31, 1),
        (59, 1),
        (94, 1),
        (109, 20598),
        (160, 1),
    ] {
        let (mut p, mut initial, source) = with_passive(60100);
        initial.spells.last_mut().unwrap().words[index] = value;
        if p.initialize_player(&initial, f::float(100.0), 1).is_ok() {
            let before = p.clone();
            assert!(
                p.initialize_spell_authority(&initial, source).is_err(),
                "word{index}"
            );
            assert_eq!(p, before);
        }
    }
}

#[test]
fn exact_dependency_hook_and_rank_scopes_reject_omission_and_extras() {
    for case in 0..7 {
        let (mut p, initial, mut source) = with_passive(60100);
        match case {
            0 => source.modifier_spell_ids = SpellSourceRows::Complete(vec![133]),
            1 => source.initial_cast_dependencies = SpellSourceRows::Complete(vec![]),
            2 => {
                let extra = dependency(&mut source).clone();
                let SpellSourceRows::Complete(rows) = &mut source.initial_cast_dependencies else {
                    unreachable!()
                };
                rows.push(extra);
            }
            3 => source.modifier_hooks.spell_script_names = SpellSourceRows::Unknown,
            4 => {
                source.modifier_hooks.spell_script_names =
                    SpellSourceRows::Complete(vec!["authored_hook".into()])
            }
            5 => {
                source.modifier_hooks.spell_rank_chain =
                    SpellSourceRows::Complete(vec![SpellRankSource {
                        spell_id: 60100,
                        previous_spell: 0,
                        first_spell: 60100,
                        rank: 1,
                        required_spell: 0,
                    }])
            }
            _ => {
                source.spells[0].hooks.spell_rank_chain =
                    SpellSourceRows::Complete(vec![SpellRankSource {
                        spell_id: 60101,
                        previous_spell: 60100,
                        first_spell: 60100,
                        rank: 2,
                        required_spell: 0,
                    }])
            }
        }
        assert!(
            p.initialize_spell_authority(&initial, source).is_err(),
            "case{case}"
        );
    }
}

#[test]
fn future_passive_cast_when_learned_skill_step_and_talent_rows_cannot_hide_in_pool() {
    for case in 0..4 {
        let (mut p, mut initial, source) = inputs();
        let mut row = passive(60100);
        match case {
            0 => {}
            1 => {
                row.words[6] = 0;
                row.words[7] = 0x8000_0000;
            }
            2 => {
                row.words[6] = 0;
                row.words[61] = 44;
            }
            _ => {
                row.words[6] = 0;
                row.talent = true;
            }
        }
        initial.spells.push(row);
        p.initialize_player(&initial, f::float(100.0), 1).unwrap();
        let before = p.clone();
        assert!(
            p.initialize_spell_authority(&initial, source).is_err(),
            "case{case}"
        );
        assert_eq!(p, before);
    }
}

#[test]
fn progression_training_identity_binds_future_pool_even_when_genesis_matches() {
    let (mut p, initial, rows) = crate::native::synthetic_quest_fixture::setup();
    p.initialize_quest_progression(&initial, rows).unwrap();
    let mage = init::initialize_mage(&initial).unwrap();
    let proof = InitialCastProof::new(&initial, &mage, SpellSourceRows::Complete(vec![])).unwrap();
    proof.validate(&p, [&fixture::hooks()], []).unwrap();

    let mut altered = initial.clone();
    let mut extra = passive(60100);
    extra.words[6] = 0;
    extra.words[61] = 0;
    extra.words[91] = 0;
    extra.words[106] = 0;
    altered.spells.push(extra);
    let other_mage = init::initialize_mage(&altered).unwrap();
    assert_eq!(mage, other_mage);
    let other_proof =
        InitialCastProof::new(&altered, &other_mage, SpellSourceRows::Complete(vec![])).unwrap();
    assert!(matches!(
        other_proof.validate(&p, [&fixture::hooks()], []),
        Err(NativeError::InvalidProfile)
    ));
}

#[test]
fn legacy_domain_does_not_require_full_raw_initial_dependencies() {
    let (mut p, initial, mut source) = fixture::inputs();
    source.initial_cast_dependencies = SpellSourceRows::Unknown;
    p.initialize_spell_authority(&initial, source).unwrap();
    assert!(p
        .interactions
        .as_ref()
        .unwrap()
        .spell_authority
        .as_ref()
        .unwrap()
        .initial_cast
        .is_none());
}
