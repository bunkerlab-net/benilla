// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored generic contracts through the real learning path, never a Human capture.
use super::*;
use crate::starter_init::{
    initial_cast_phases::{tests::authored, InitialPhaseEvent},
    Attributes, DefaultSkill, LearnSpell, LiveTraining, SkillAbilityRow, SkillLineRow,
    SkillRaceClassRow,
};
fn input(ids: &[u32]) -> StarterInput {
    StarterInput {
        race: 1,
        class: 8,
        gender: 0,
        level: 1,
        class_power_type: 0,
        create_attributes: Attributes([17, 23, 29, 31, 37]),
        create_health: 41,
        create_mana: 59,
        default_skills: Rows::Complete(vec![]),
        default_spells: Rows::Complete(ids.to_vec()),
        skill_lines: vec![],
        skill_race_class: Rows::Complete(vec![]),
        skill_tiers: vec![],
        skill_abilities: Rows::Complete(vec![]),
        spells: ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|id| authored(id).row)
            .collect(),
        active_support: vec![],
        item_grants: Rows::Complete(vec![]),
        items: vec![],
        item_creation_hooks: false,
    }
}
fn dependencies(input: &StarterInput, ids: &[u32]) -> StarterInitialDependencies {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    StarterInitialDependencies {
        source_identity: input.source_identity(),
        store_identity: [12; 32],
        hook_scope_identity: [23; 32],
        queried_spell_ids: ids.clone(),
        rows: Rows::Complete(
            ids.into_iter()
                .map(|id| {
                    let fixture = authored(id);
                    let p = fixture.prerequisites.unwrap();
                    let row = input.spells.iter().find(|r| r.id == id).unwrap();
                    StarterInitialDependency {
                        spell_id: id,
                        source_hash: row.source_hash(),
                        // These tuples are explicitly authored test evidence, not inferred DBC data.
                        cast_time: InitialSpellLookup::Found(SpellCastTimeSource {
                            id: row.words[18],
                            base_ms: 0,
                            per_level_ms: 0,
                            minimum_ms: 0,
                        }),
                        duration: InitialSpellLookup::Found(SpellDurationSource {
                            id: row.words[30],
                            base_ms: row.duration_ms.unwrap(),
                            per_level_ms: 0,
                            maximum_ms: row.duration_ms.unwrap(),
                        }),
                        server_side: p.server_side,
                        server_attributes: p.server_attributes,
                        bonus_coefficients: p.effect_bonus_coefficient_bits,
                        attack_power_coefficients: p.effect_bonus_coefficient_from_ap_bits,
                    }
                })
                .collect(),
        ),
    }
}
fn order(ids: &[u16]) -> StarterCastOrder {
    StarterCastOrder {
        creation: InitialCastOrderEvidence::Authored { identity: [34; 32] },
        first_entry: InitialCastOrderEvidence::Authored { identity: [45; 32] },
        entry_skill_order: Some(ids.to_vec()),
    }
}
fn rows(d: &mut StarterInitialDependencies) -> &mut Vec<StarterInitialDependency> {
    let Rows::Complete(rows) = &mut d.rows else {
        panic!("test rows")
    };
    rows
}
fn skill(input: &mut StarterInput, id: u16, spell: u32) {
    let Rows::Complete(defaults) = &mut input.default_skills else {
        panic!("defaults")
    };
    defaults.push(DefaultSkill { id, step: 0 });
    input.skill_lines.push(SkillLineRow { id, category: 6 });
    let Rows::Complete(info) = &mut input.skill_race_class else {
        panic!("info")
    };
    info.push(SkillRaceClassRow {
        row_id: id.into(),
        skill: id,
        race_mask: 1,
        class_mask: 128,
        flags: 0,
        tier: 0,
    });
    let Rows::Complete(abilities) = &mut input.skill_abilities else {
        panic!("abilities")
    };
    abilities.push(SkillAbilityRow {
        row_id: id.into(),
        skill: id,
        spell,
        forward_spell_id: 0,
        race_mask: 0,
        class_mask: 0,
        required_value: 1,
        learn_on_get_skill: 1,
        max_value: 0,
    });
}
fn casts(plan: &InitialCastPlan) -> Vec<u32> {
    plan.occurrences().iter().map(|c| c.row().id).collect()
}
#[test]
fn genuine_learning_retains_pending_and_detect_without_fake_completion() {
    let input = input(&[81, 3050, 81, 20597, 20598, 20599, 20864]);
    let dependencies = dependencies(&input, &[81, 3050, 20597, 20598, 20599, 20864]);
    let prepared = prepare_starter(&input, &dependencies, &order(&[])).unwrap();
    assert!(super::super::initialize_mage(&input).is_err());
    assert!(super::super::first_entry(&input, prepared.created()).is_err());
    assert_eq!(
        casts(prepared.creation_plan()),
        [81, 3050, 20597, 20598, 20599, 20864]
    );
    assert_eq!(prepared.creation_plan().draw_requests().len(), 1);
    assert_eq!(prepared.first_entry_plan().draw_requests().len(), 1);
    assert_ne!(
        prepared.creation_plan().identity(),
        prepared.first_entry_plan().identity()
    );
    assert_eq!(prepared.created().initial_auras.len(), 6);
    assert!(prepared.created().can_dodge);
    assert!(
        prepared.created().skills.is_empty(),
        "skill auras never grant absent skills"
    );
    assert!(prepared.created().trace.iter().any(|e| matches!(
        e,
        InitEvent::PendingInitialAmount {
            spell: 81,
            low: 0,
            high: 6,
            ..
        }
    )));
    assert!(!prepared.created().trace.iter().any(|e| matches!(
        e,
        InitEvent::EffectApplied {
            spell: 81 | 3050,
            ..
        }
    )));
    for roll in 0..=6 {
        let evaluated = prepared
            .first_entry_plan()
            .evaluate(&[roll], prepared.first_entry_initial_masks())
            .unwrap();
        assert!(evaluated
            .events()
            .iter()
            .any(|e| matches!(e, InitialPhaseEvent::CastComplete { spell_id: 3050, .. })));
        assert!(!evaluated
            .events()
            .iter()
            .any(|e| matches!(e, InitialPhaseEvent::Dispatch { occurrence: 1, .. })));
        prepared
            .first_entry_plan()
            .verify_evaluation(&evaluated)
            .unwrap();
    }
    prepared.validate_against(&input).unwrap();
    assert_eq!(
        prepare_starter(&input, &dependencies, &order(&[])).unwrap(),
        prepared
    );
}
#[test]
fn loaded_skill_permutation_drives_actual_recursive_order_and_known_suppression() {
    let mut input = input(&[81, 3050, 203]);
    skill(&mut input, 95, 81);
    skill(&mut input, 162, 3050);
    let dependencies = dependencies(&input, &[81, 3050, 203]);
    let prepared = prepare_starter(&input, &dependencies, &order(&[162, 95])).unwrap();
    assert_eq!(casts(prepared.creation_plan()), [81, 3050, 203]);
    assert_eq!(casts(prepared.first_entry_plan()), [3050, 81, 203]);
    assert_eq!(prepared.first_entry_plan().draw_requests()[0].occurrence, 1);
    let traversal: Vec<_> = prepared
        .first_entry()
        .trace
        .iter()
        .filter_map(|e| match e {
            InitEvent::TrainLoadedSkill { skill, .. } => Some(*skill),
            _ => None,
        })
        .collect();
    assert_eq!(traversal, [162, 95]);
    let canonical = prepare_starter(&input, &dependencies, &order(&[95, 162])).unwrap();
    assert_ne!(prepared.identity(), canonical.identity());
    assert_eq!(prepared.created(), canonical.created());
    for invalid in [vec![95], vec![95, 95], vec![95, 163], vec![95, 162, 163]] {
        assert!(prepare_starter(&input, &dependencies, &order(&invalid)).is_err());
    }
}
#[test]
fn recursive_order_is_collected_at_learning_transition_not_default_list_filter() {
    let mut input = input(&[81, 3050]);
    let mut parent = authored(203).row;
    parent.id = 900_000_001;
    parent.words = [0; 173];
    parent.words[0] = parent.id;
    parent.words[58] = u32::MAX;
    parent.learned_spells = Rows::Complete(vec![
        LearnSpell {
            spell_id: 3050,
            active: true,
            auto_learned: false,
        },
        LearnSpell {
            spell_id: 81,
            active: true,
            auto_learned: false,
        },
    ]);
    input.default_spells = Rows::Complete(vec![parent.id, 81, 3050]);
    input.spells.push(parent);
    let prepared =
        prepare_starter(&input, &dependencies(&input, &[81, 3050]), &order(&[])).unwrap();
    assert_eq!(casts(prepared.creation_plan()), [3050, 81]);
    assert_eq!(casts(prepared.first_entry_plan()), [3050, 81]);
    assert_eq!(prepared.first_entry_plan().draw_requests().len(), 1);
}
#[test]
fn indexed_lookup_presence_and_complete_scope_are_not_zero_defaults() {
    let input = input(&[81, 3050]);
    let original = dependencies(&input, &[81, 3050]);
    let a = prepare_starter(&input, &original, &order(&[])).unwrap();
    let mut missing = original.clone();
    for r in rows(&mut missing) {
        r.duration = InitialSpellLookup::Missing { index: 0 };
    }
    let b = prepare_starter(&input, &missing, &order(&[])).unwrap();
    assert_ne!(a.identity(), b.identity());
    assert_eq!(a.created(), b.created());
    assert_ne!(
        LiveTraining::from_prepared(&input, &a).unwrap().identity(),
        LiveTraining::from_prepared(&input, &b).unwrap().identity()
    );
    let mut conflict = original.clone();
    rows(&mut conflict)[0].duration = InitialSpellLookup::Missing { index: 0 };
    assert!(prepare_starter(&input, &conflict, &order(&[])).is_err());
    for edit in 0..8 {
        let mut d = original.clone();
        match edit {
            0 => rows(&mut d)[0].cast_time = InitialSpellLookup::Unknown,
            1 => rows(&mut d)[1].duration = InitialSpellLookup::Unknown,
            2 => rows(&mut d)[0].cast_time = InitialSpellLookup::Missing { index: 1 },
            3 => {
                rows(&mut d)[0].cast_time = InitialSpellLookup::Found(SpellCastTimeSource {
                    id: 0,
                    base_ms: 1,
                    per_level_ms: 0,
                    minimum_ms: 0,
                })
            }
            4 => {
                rows(&mut d).pop();
            }
            5 => rows(&mut d)[0].source_hash[0] ^= 1,
            6 => d.rows = Rows::Missing,
            _ => d.queried_spell_ids.reverse(),
        }
        assert!(
            prepare_starter(&input, &d, &order(&[])).is_err(),
            "edit {edit}"
        );
    }
    let racial = input_for_racial();
    let mut d = dependencies(&racial, &[20598]);
    rows(&mut d)[0].duration = InitialSpellLookup::Missing { index: 21 };
    assert!(prepare_starter(&racial, &d, &order(&[])).is_err());
}
fn input_for_racial() -> StarterInput {
    input(&[20598])
}
#[test]
fn order_tags_input_and_no_dispatch_occurrences_all_bind_identity() {
    let input = input(&[81, 3050, 203]);
    let d = dependencies(&input, &[81, 3050, 203]);
    let a = prepare_starter(&input, &d, &order(&[])).unwrap();
    for evidence in [
        InitialCastOrderEvidence::Unknown,
        InitialCastOrderEvidence::ProvenProjectionEquivalent {
            proof_identity: [1; 32],
        },
    ] {
        let mut o = order(&[]);
        o.first_entry = evidence;
        assert!(prepare_starter(&input, &d, &o).is_err());
    }
    let mut observed = order(&[]);
    observed.first_entry = InitialCastOrderEvidence::ObservedLoadedOrder {
        capture_identity: [45; 32],
        loader_identity: [56; 32],
    };
    let b = prepare_starter(&input, &d, &observed).unwrap();
    assert_ne!(a.identity(), b.identity());
    let mut changed = input.clone();
    changed.default_spells = Rows::Complete(vec![3050, 81, 203]);
    let b = prepare_starter(
        &changed,
        &dependencies(&changed, &[81, 3050, 203]),
        &order(&[]),
    )
    .unwrap();
    assert_ne!(a.identity(), b.identity());
    assert!(a.validate_against(&changed).is_err());
    assert!(LiveTraining::from_prepared(&changed, &a).is_err());
    changed.default_spells = Rows::Complete(vec![81, 203]);
    let b = prepare_starter(&changed, &dependencies(&changed, &[81, 203]), &order(&[])).unwrap();
    assert_ne!(a.identity(), b.identity());
    assert_eq!(
        a.first_entry_plan().draw_requests(),
        b.first_entry_plan().draw_requests()
    );
    assert_eq!(
        a.created().effective_attributes,
        b.created().effective_attributes
    );
}
#[test]
fn altered_dodge_targets_rng_handlers_hooks_and_missing_closure_stay_closed() {
    for (cell, value) in [
        (61, 21),
        (64, 7),
        (67, 1),
        (73, 1),
        (76, 1),
        (82, 1),
        (109, 203),
        (112, 1),
    ] {
        let mut input = input(&[81]);
        input.spells[0].words[cell] = value;
        assert!(prepare_starter(&input, &dependencies(&input, &[81]), &order(&[])).is_err());
    }
    let mut input = input(&[81]);
    input.spells[0].external_initial_hooks = true;
    assert!(prepare_starter(&input, &dependencies(&input, &[81]), &order(&[])).is_err());
    input.spells[0].external_initial_hooks = false;
    input.spells[0].learned_spells = Rows::Missing;
    assert!(prepare_starter(&input, &dependencies(&input, &[81]), &order(&[])).is_err());
}
#[test]
fn prepared_training_is_cross_bound_and_no_rng_or_completion_is_hidden() {
    let input = input(&[81]);
    let prepared = prepare_starter(&input, &dependencies(&input, &[81]), &order(&[])).unwrap();
    let live = LiveTraining::from_prepared(&input, &prepared).unwrap();
    assert_eq!(live.source_identity(), input.source_identity());
    assert_eq!(live.preparation_identity(), Some(prepared.identity()));
    assert_ne!(live.identity(), live.source_identity());
    assert_eq!(live.snapshot().version, 2);
    assert_eq!(
        serde_json::to_value(&prepared).unwrap(),
        serde_json::to_value(prepared.identity()).unwrap()
    );
}

mod numeric_source;
