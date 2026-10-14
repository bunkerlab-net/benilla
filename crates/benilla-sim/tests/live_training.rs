// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic live callbacks against the production initializer learning path.
use benilla_sim::{player_progression as p, starter_init::*};

const SEED: u32 = 900_000_001;
const TRAINED: u32 = 900_000_002;
const CHILD: u32 = 900_000_302;
const LEAF: u32 = 900_000_902;
const SIBLING: u32 = 900_000_202;
fn spell(id: u32) -> SpellRow {
    let mut words = [0; 173];
    words[0] = id;
    words[58] = u32::MAX;
    SpellRow {
        id,
        words,
        duration_ms: Some(-1),
        previous_rank: None,
        talent: false,
        learned_spells: Rows::Complete(vec![]),
        external_initial_hooks: false,
    }
}
fn aura(id: u32, kind: u32, misc: i32, amount: i32) -> SpellRow {
    let mut row = spell(id);
    row.words[6] = 0x40;
    row.words[61] = 6;
    row.words[64] = 1;
    row.words[67] = 1;
    row.words[76] = (amount - 1) as u32;
    row.words[82] = 1;
    row.words[91] = kind;
    row.words[106] = misc as u32;
    row
}
fn mapping(
    row_id: u32,
    skill: u16,
    spell: u32,
    required_value: u32,
    learn: u32,
) -> SkillAbilityRow {
    SkillAbilityRow {
        row_id,
        skill,
        spell,
        forward_spell_id: 0,
        race_mask: 0,
        class_mask: 0,
        required_value,
        learn_on_get_skill: learn,
        max_value: 0,
    }
}
fn input() -> StarterInput {
    StarterInput {
        race: 1,
        class: 8,
        gender: 0,
        level: 1,
        class_power_type: 0,
        create_attributes: Attributes([17, 23, 29, 31, 37]),
        create_health: 41,
        create_mana: 59,
        default_skills: Rows::Complete(vec![DefaultSkill { id: 95, step: 0 }]),
        default_spells: Rows::Complete(vec![SEED]),
        skill_lines: vec![SkillLineRow {
            id: 95,
            category: 6,
        }],
        skill_race_class: Rows::Complete(vec![SkillRaceClassRow {
            row_id: 1,
            skill: 95,
            race_mask: 1,
            class_mask: 128,
            flags: 0,
            tier: 0,
        }]),
        skill_tiers: vec![],
        skill_abilities: Rows::Complete(vec![mapping(1, 95, TRAINED, 6, 1)]),
        spells: vec![spell(SEED), spell(TRAINED)],
        active_support: vec![ActiveAbilitySupport {
            spell_id: TRAINED,
            unavailable_reason: None,
        }],
        item_grants: Rows::Complete(vec![]),
        items: vec![],
        item_creation_hooks: false,
    }
}
fn dependency(spell_id: u32, active: bool) -> LearnSpell {
    LearnSpell {
        spell_id,
        active,
        auto_learned: false,
    }
}
fn nested_input() -> StarterInput {
    let mut input = input();
    input.spells[1].learned_spells =
        Rows::Complete(vec![dependency(CHILD, true), dependency(SIBLING, true)]);
    let mut child = spell(CHILD);
    child.learned_spells = Rows::Complete(vec![dependency(LEAF, true)]);
    input.spells.extend([child, spell(LEAF), spell(SIBLING)]);
    input
}
fn audit() -> p::Audit {
    p::Audit {
        group_or_raid: p::Evidence::Absent,
        pet_or_controlled_unit: p::Evidence::Absent,
        used_talents: p::Evidence::Absent,
        shapeshift: p::Evidence::Absent,
        level_change_scripts: p::Evidence::Absent,
        dynamic_or_level_scaling_modifiers: p::Evidence::Absent,
        resource_or_stat_aura_hooks: p::Evidence::Absent,
        unsupported_item_effects: p::Evidence::Absent,
        non_mana_power_modifiers: p::Evidence::Absent,
        extra_spell_crit_or_regen_modifiers: p::Evidence::Absent,
    }
}
fn view(input: &StarterInput, mage: &InitializedMage) -> p::Snapshot {
    let modifiers = mage.stat_recipe.snapshot().modifiers;
    let numeric = p::NumericState::from_modifiers(
        1,
        &modifiers,
        mage.resources.health,
        [mage.resources.mana, 0, 0, 100, 0],
    )
    .unwrap();
    p::Snapshot {
        policy: p::POLICY_ID.into(),
        catalog_identity: [23; 32],
        race: 1,
        class: 8,
        level: 1,
        alive: true,
        xp: 0,
        next_level_xp: 100,
        played_time_level: 0,
        session_level: 1,
        free_talent_points: 0,
        used_talent_points: 0,
        modifiers,
        numeric,
        skills: first_entry_skills(input, mage)
            .unwrap()
            .values()
            .enumerate()
            .map(|(position, s)| p::SkillState {
                id: s.id,
                position: position as u8,
                value: s.value,
                maximum: s.maximum,
                step: s.step,
                permanent_bonus: s.permanent_bonus,
                temporary_bonus: s.temporary_bonus,
                status: p::SkillStatus::Unchanged,
            })
            .collect(),
        audit: audit(),
    }
}
fn trained_view(input: &StarterInput, mage: &InitializedMage) -> p::Snapshot {
    let mut view = view(input, mage);
    view.level = 2;
    view.skills[0].value = 10;
    view.skills[0].maximum = 10;
    view.skills[0].status = p::SkillStatus::Changed;
    view
}
fn callback(view: &p::Snapshot) -> p::Event {
    p::Event::SkillTrainedSpells {
        id: 95,
        value: view.skills[0].value,
        step: view.skills[0].step,
    }
}

#[test]
fn actual_training_preserves_seed_and_uses_callback_time_state() {
    let mut input = input();
    input.spells.push(aura(SEED + 10, 98, 95, 3));
    input.spells.push(aura(SEED + 11, 29, 3, 2));
    let Rows::Complete(defaults) = &mut input.default_spells else {
        unreachable!()
    };
    defaults.extend([SEED + 10, SEED + 11]);
    let mage = initialize_mage(&input).unwrap();
    let fresh = LiveTraining::from_initializer(&input, &mage).unwrap();
    let before = fresh.snapshot();
    let mut live = fresh.clone();
    let at_callback = trained_view(&input, &mage);
    live.apply(&callback(&at_callback), &at_callback).unwrap();
    assert_eq!(fresh.snapshot(), before, "deep clone must remain isolated");
    assert_eq!(live.abilities()[&SEED], mage.abilities[&SEED]);
    assert_eq!(
        live.abilities()[&TRAINED].availability,
        AbilityAvailability::Available
    );
    assert_eq!(live.snapshot().calls[0].view, at_callback);
    assert_eq!(live.snapshot().calls[0].learned_spells, [TRAINED]);
    assert_eq!(
        live.snapshot().calls[0].view.modifiers.create,
        input.create_attributes
    );
    let mut final_view = at_callback.clone();
    final_view.modifiers.create.0[3] += 1;
    final_view.modifiers.create_health += 5;
    final_view.modifiers.create_mana += 6;
    final_view.numeric = p::NumericState::from_modifiers(
        2,
        &final_view.modifiers,
        final_view.numeric.health,
        final_view.numeric.powers,
    )
    .unwrap();
    final_view.session_level = 2;
    let encoded = serde_json::to_vec(&live.snapshot()).unwrap();
    let saved: TrainingSnapshot = serde_json::from_slice(&encoded).unwrap();
    let restored = LiveTraining::restore_authoritative(&fresh, &saved, &final_view).unwrap();
    assert_eq!(restored.snapshot(), live.snapshot());
}

#[test]
fn defense_gain_without_set_skill_does_not_invent_training_on_restore() {
    let input = input();
    let mage = initialize_mage(&input).unwrap();
    let fresh = LiveTraining::from_initializer(&input, &mage).unwrap();
    let final_view = trained_view(&input, &mage);
    let restored =
        LiveTraining::restore_authoritative(&fresh, &fresh.snapshot(), &final_view).unwrap();
    assert!(!restored.abilities().contains_key(&TRAINED));
}

#[test]
fn training_fixed_state_mutations_fail_atomically() {
    for kind in [29, 98] {
        let mut input = input();
        input.spells[1] = aura(TRAINED, kind, if kind == 98 { 95 } else { 3 }, 2);
        let mage = initialize_mage(&input).unwrap();
        let mut live = LiveTraining::from_initializer(&input, &mage).unwrap();
        let before = live.snapshot();
        let view = trained_view(&input, &mage);
        assert!(live.apply(&callback(&view), &view).is_err());
        assert_eq!(live.snapshot(), before);
    }
    let mut input = input();
    input.spells[1].words[6] = 0x40;
    input.spells[1].words[58] = 2;
    input.spells[1].words[59] = 0x100;
    input.spells[1].words[61] = 60;
    input.spells[1].words[82] = 1;
    let mage = initialize_mage(&input).unwrap();
    let mut live = LiveTraining::from_initializer(&input, &mage).unwrap();
    let before = live.snapshot();
    let view = trained_view(&input, &mage);
    assert!(live.apply(&callback(&view), &view).is_err());
    assert_eq!(live.snapshot(), before);
}

#[test]
fn new_skill_and_late_missing_dependency_cannot_partially_learn() {
    let mut input = input();
    input.skill_lines.push(SkillLineRow {
        id: 60002,
        category: 6,
    });
    let Rows::Complete(infos) = &mut input.skill_race_class else {
        unreachable!()
    };
    infos.push(SkillRaceClassRow {
        row_id: 2,
        skill: 60002,
        race_mask: 1,
        class_mask: 128,
        flags: 0,
        tier: 0,
    });
    let Rows::Complete(mappings) = &mut input.skill_abilities else {
        unreachable!()
    };
    mappings.push(mapping(2, 60002, TRAINED, 0, 2));
    let mage = initialize_mage(&input).unwrap();
    let mut live = LiveTraining::from_initializer(&input, &mage).unwrap();
    let before = live.snapshot();
    let view = trained_view(&input, &mage);
    assert!(live.apply(&callback(&view), &view).is_err());
    assert_eq!(live.snapshot(), before);

    input.spells[1].learned_spells = Rows::Missing;
    let mage = initialize_mage(&input).unwrap();
    assert!(LiveTraining::from_initializer(&input, &mage).is_err());
}

#[test]
fn replay_rejects_forged_spellbook_callback_and_initializer() {
    let input = input();
    let mage = initialize_mage(&input).unwrap();
    let fresh = LiveTraining::from_initializer(&input, &mage).unwrap();
    let at_callback = trained_view(&input, &mage);
    let mut live = fresh.clone();
    live.apply(&callback(&at_callback), &at_callback).unwrap();
    let mut saved = live.snapshot();
    saved.abilities.push(KnownAbility {
        spell_id: 999,
        active: true,
        availability: AbilityAvailability::Available,
    });
    assert!(LiveTraining::restore_authoritative(&fresh, &saved, &at_callback).is_err());
    saved = live.snapshot();
    saved.calls[0].value = 9;
    assert!(LiveTraining::restore_authoritative(&fresh, &saved, &at_callback).is_err());
    saved = live.snapshot();
    saved.calls[0].view.catalog_identity[0] ^= 1;
    assert!(LiveTraining::restore_authoritative(&fresh, &saved, &at_callback).is_err());
    let mut forged = mage.clone();
    forged.abilities.remove(&SEED);
    assert!(LiveTraining::from_initializer(&input, &forged).is_err());
}

#[test]
fn full_future_closure_and_support_change_training_identity() {
    let input = input();
    let mage = initialize_mage(&input).unwrap();
    let original = LiveTraining::from_initializer(&input, &mage)
        .unwrap()
        .identity();
    let mut changed = input.clone();
    changed.spells[1].words[166] = 123;
    let mage = initialize_mage(&changed).unwrap();
    let changed_hash = LiveTraining::from_initializer(&changed, &mage)
        .unwrap()
        .identity();
    assert_ne!(original, changed_hash);
    let mut changed = input.clone();
    changed.active_support[0].unavailable_reason = Some("missing future executor".into());
    let mage = initialize_mage(&changed).unwrap();
    assert_ne!(
        original,
        LiveTraining::from_initializer(&changed, &mage)
            .unwrap()
            .identity()
    );
}

#[test]
fn inert_passive_learning_and_authored_callback_order_are_retained() {
    let mut input = input();
    input.spells[1].words[6] = 0x40;
    input.spells[1].words[61] = 20;
    input.spells[1].words[82] = 1;
    let mage = initialize_mage(&input).unwrap();
    let fresh = LiveTraining::from_initializer(&input, &mage).unwrap();
    let mut live = fresh.clone();
    let mut first = trained_view(&input, &mage);
    first.skills[0].value = 6;
    live.apply(&callback(&first), &first).unwrap();
    let second = trained_view(&input, &mage);
    live.apply(&callback(&second), &second).unwrap();
    assert_eq!(
        live.abilities()[&TRAINED].availability,
        AbilityAvailability::PassiveApplied
    );
    let saved = live.snapshot();
    assert_eq!(saved.calls[0].learned_spells, [TRAINED]);
    assert!(saved.calls[1].learned_spells.is_empty());
    assert_eq!(
        saved
            .calls
            .iter()
            .map(|call| call.value)
            .collect::<Vec<_>>(),
        [6, 10]
    );
    let restored = LiveTraining::restore_authoritative(&fresh, &saved, &second).unwrap();
    assert_eq!(restored.snapshot(), saved);
    let mut reversed = saved;
    reversed.calls.reverse();
    assert!(LiveTraining::restore_authoritative(&fresh, &reversed, &second).is_err());
}

#[test]
fn same_value_defense_setter_is_unsupported_and_rolls_back_learning() {
    let mut input = input();
    let Rows::Complete(defaults) = &mut input.default_skills else {
        unreachable!()
    };
    defaults.push(DefaultSkill { id: 136, step: 0 });
    input.skill_lines.push(SkillLineRow {
        id: 136,
        category: 6,
    });
    let Rows::Complete(infos) = &mut input.skill_race_class else {
        unreachable!()
    };
    infos[0].tier = 77;
    infos.push(SkillRaceClassRow {
        row_id: 2,
        skill: 136,
        race_mask: 1,
        class_mask: 128,
        flags: 0,
        tier: 0,
    });
    let mut maxima = [0; 16];
    maxima[0] = 5;
    input.skill_tiers.push(SkillTierRow {
        id: 77,
        values: [0; 16],
        maxima,
    });
    input.skill_abilities = Rows::Complete(vec![mapping(1, 136, TRAINED, 6, 1)]);
    input.spells[1].words[6] = 0x40;
    input.spells[1].words[61] = 44;
    input.spells[1].words[76] = 1;
    input.spells[1].words[82] = 1;
    input.spells[1].words[106] = 95;
    let mage = initialize_mage(&input).unwrap();
    let fresh = LiveTraining::from_initializer(&input, &mage).unwrap();
    let mut live = fresh.clone();
    let before = live.snapshot();
    let mut view = view(&input, &mage);
    view.level = 2;
    let trained = view
        .skills
        .iter_mut()
        .find(|skill| skill.id == 136)
        .unwrap();
    trained.value = 10;
    trained.maximum = 10;
    trained.status = p::SkillStatus::Changed;
    let defense = view.skills.iter().find(|skill| skill.id == 95).unwrap();
    assert_eq!((defense.value, defense.maximum, defense.step), (1, 5, 1));
    assert_eq!(defense.status, p::SkillStatus::Unchanged);
    // Native SetSkillStep(95, 1) preserves this numeric tuple, but SetSkill
    // marks its status Changed and calls UpdateDefenseBonusesMod regardless.
    let event = p::Event::SkillTrainedSpells {
        id: 136,
        value: 10,
        step: 0,
    };
    assert!(matches!(
        live.apply(&event, &view),
        Err(InitError::Unsupported {
            domain: "live training changed fixed skills",
            ..
        })
    ));
    assert_eq!(live.snapshot(), before);
    assert!(!live.abilities().contains_key(&TRAINED));

    // Replay must also reject a previously accepted same-value setter journal.
    let mut saved = before;
    saved.abilities.push(KnownAbility {
        spell_id: TRAINED,
        active: true,
        availability: AbilityAvailability::PassiveApplied,
    });
    saved.calls.push(TrainingCallSnapshot {
        id: 136,
        value: 10,
        step: 0,
        view: view.clone(),
        learned_spells: vec![TRAINED],
    });
    assert!(LiveTraining::restore_authoritative(&fresh, &saved, &view).is_err());
}

#[test]
fn temporary_skill_setter_mutation_is_rejected_even_if_later_effect_restores_it() {
    let mut input = input();
    let Rows::Complete(defaults) = &mut input.default_skills else {
        unreachable!()
    };
    defaults.push(DefaultSkill { id: 60001, step: 1 });
    input.skill_lines.push(SkillLineRow {
        id: 60001,
        category: 11,
    });
    let Rows::Complete(infos) = &mut input.skill_race_class else {
        unreachable!()
    };
    infos.push(SkillRaceClassRow {
        row_id: 2,
        skill: 60001,
        race_mask: 1,
        class_mask: 128,
        flags: 0,
        tier: 77,
    });
    let mut maxima = [0; 16];
    maxima[0] = 5;
    maxima[1] = 10;
    input.skill_tiers.push(SkillTierRow {
        id: 77,
        values: [0; 16],
        maxima,
    });
    input.spells[1].words[6] = 0x40;
    for (index, step) in [2, 1].into_iter().enumerate() {
        input.spells[1].words[61 + index] = 44;
        input.spells[1].words[76 + index] = step;
        input.spells[1].words[82 + index] = 1;
        input.spells[1].words[106 + index] = 60001;
    }
    let mage = initialize_mage(&input).unwrap();
    let mut live = LiveTraining::from_initializer(&input, &mage).unwrap();
    let before = live.snapshot();
    let view = trained_view(&input, &mage);
    assert!(matches!(
        live.apply(&callback(&view), &view),
        Err(InitError::Unsupported {
            domain: "live training changed fixed skills",
            ..
        })
    ));
    assert_eq!(live.snapshot(), before);
}

#[test]
fn nested_notifications_are_post_order_while_initializer_insertions_remain_pre_order() {
    let input = nested_input();
    let mage = initialize_mage(&input).unwrap();
    let mut live = LiveTraining::from_initializer(&input, &mage).unwrap();
    let view = trained_view(&input, &mage);
    live.apply(&callback(&view), &view).unwrap();
    // Descending IDs distinguish this from a sorted spellbook difference.
    assert_eq!(
        live.snapshot().calls[0].learned_spells,
        [LEAF, CHILD, SIBLING, TRAINED]
    );
    assert!(live.abilities()[&LEAF].active);
    assert!(matches!(
        live.abilities()[&LEAF].availability,
        AbilityAvailability::Unavailable(_)
    ));

    let mut creation = input.clone();
    creation.default_spells = Rows::Complete(vec![TRAINED]);
    let created = initialize_mage(&creation).unwrap();
    let insertions: Vec<_> = created
        .trace
        .iter()
        .filter_map(|event| match event {
            InitEvent::SpellLearned { spell } => Some(*spell),
            _ => None,
        })
        .collect();
    assert_eq!(insertions, [TRAINED, CHILD, LEAF, SIBLING]);
    assert_eq!(
        first_entry(&creation, &created).unwrap().skills,
        created.skills
    );
}

#[test]
fn inactive_additions_notify_active_descendants_but_never_themselves() {
    let mut input = nested_input();
    input.spells[1].learned_spells =
        Rows::Complete(vec![dependency(CHILD, false), dependency(SIBLING, false)]);
    let mage = initialize_mage(&input).unwrap();
    let fresh = LiveTraining::from_initializer(&input, &mage).unwrap();
    let mut live = fresh.clone();
    let view = trained_view(&input, &mage);
    live.apply(&callback(&view), &view).unwrap();
    assert_eq!(live.snapshot().calls[0].learned_spells, [LEAF, TRAINED]);
    assert!(!live.abilities()[&CHILD].active);
    assert!(!live.abilities()[&SIBLING].active);
    assert!(live.abilities()[&LEAF].active);
    assert!(live.abilities()[&TRAINED].active);
    live.apply(&callback(&view), &view).unwrap();
    assert!(live.snapshot().calls[1].learned_spells.is_empty());
    assert_eq!(
        LiveTraining::restore_authoritative(&fresh, &live.snapshot(), &view)
            .unwrap()
            .snapshot(),
        live.snapshot()
    );
}

#[test]
fn passive_effect_learning_precedes_non_auto_dependencies_without_duplicate_packets() {
    let mut input = nested_input();
    let root = &mut input.spells[1];
    root.words[6] = 0x40;
    root.words[61] = 36;
    root.words[82] = 1;
    root.words[109] = CHILD;
    root.learned_spells = Rows::Complete(vec![
        LearnSpell {
            spell_id: CHILD,
            active: true,
            auto_learned: true,
        },
        dependency(SIBLING, true),
        dependency(CHILD, true),
        dependency(SEED, true),
    ]);
    let mage = initialize_mage(&input).unwrap();
    let mut live = LiveTraining::from_initializer(&input, &mage).unwrap();
    let view = trained_view(&input, &mage);
    live.apply(&callback(&view), &view).unwrap();
    assert_eq!(
        live.snapshot().calls[0].learned_spells,
        [LEAF, CHILD, SIBLING, TRAINED]
    );
    assert_eq!(
        live.abilities()[&TRAINED].availability,
        AbilityAvailability::PassiveApplied
    );
}

#[test]
fn callback_notifications_keep_skill_row_traversal_order_and_no_op_callbacks() {
    let mut input = nested_input();
    input.skill_abilities = Rows::Complete(vec![
        mapping(20, 95, TRAINED, 6, 1),
        mapping(10, 95, SIBLING, 6, 1),
    ]);
    let mage = initialize_mage(&input).unwrap();
    let mut live = LiveTraining::from_initializer(&input, &mage).unwrap();
    let initial = view(&input, &mage);
    live.apply(&callback(&initial), &initial).unwrap();
    assert!(live.snapshot().calls[0].learned_spells.is_empty());
    let view = trained_view(&input, &mage);
    live.apply(&callback(&view), &view).unwrap();
    assert_eq!(
        live.snapshot().calls[1].learned_spells,
        [SIBLING, LEAF, CHILD, TRAINED]
    );
    live.apply(&callback(&view), &view).unwrap();
    assert!(live.snapshot().calls[2].learned_spells.is_empty());
}

#[test]
fn nested_notification_journals_round_trip_and_reject_order_membership_and_version_tampering() {
    let input = nested_input();
    let mage = initialize_mage(&input).unwrap();
    let fresh = LiveTraining::from_initializer(&input, &mage).unwrap();
    let mut live = fresh.clone();
    let view = trained_view(&input, &mage);
    live.apply(&callback(&view), &view).unwrap();
    live.apply(&callback(&view), &view).unwrap();
    let encoded = serde_json::to_vec(&live.snapshot()).unwrap();
    let saved: TrainingSnapshot = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(saved.version, 2);
    assert!(LIVE_TRAINING_POLICY_ID.ends_with("-v2"));
    assert_eq!(
        LiveTraining::restore_authoritative(&fresh, &saved, &view)
            .unwrap()
            .snapshot(),
        saved
    );
    for forged in [
        vec![],
        vec![CHILD, LEAF, SIBLING, TRAINED],
        vec![LEAF, CHILD, SIBLING],
        vec![LEAF, CHILD, SIBLING, TRAINED, TRAINED],
        vec![LEAF, CHILD, SEED, TRAINED],
        vec![TRAINED; 4097],
    ] {
        let mut tampered = saved.clone();
        tampered.calls[0].learned_spells = forged;
        assert!(LiveTraining::restore_authoritative(&fresh, &tampered, &view).is_err());
    }
    let mut tampered = saved.clone();
    tampered.calls[1].learned_spells = vec![TRAINED];
    assert!(LiveTraining::restore_authoritative(&fresh, &tampered, &view).is_err());
    tampered = saved.clone();
    tampered.calls.swap(0, 1);
    assert!(LiveTraining::restore_authoritative(&fresh, &tampered, &view).is_err());
    tampered = saved.clone();
    tampered.version = 1;
    assert!(LiveTraining::restore_authoritative(&fresh, &tampered, &view).is_err());
    let mut missing_trace = serde_json::to_value(&saved).unwrap();
    missing_trace["calls"][0]
        .as_object_mut()
        .unwrap()
        .remove("learned_spells");
    assert!(serde_json::from_value::<TrainingSnapshot>(missing_trace).is_err());
    assert_eq!(fresh.snapshot().calls, []);
}

#[test]
fn late_unsupported_nested_learning_rolls_back_earlier_notifications_and_spellbook() {
    for unsupported in 0..5 {
        let mut input = nested_input();
        let sibling = input
            .spells
            .iter_mut()
            .find(|row| row.id == SIBLING)
            .unwrap();
        match unsupported {
            0 => sibling.talent = true,
            1 => sibling.previous_rank = Some(SEED),
            2 => sibling.external_initial_hooks = true,
            3 => {
                sibling.words[6] = 0x40;
                sibling.words[61] = 999;
                sibling.words[82] = 1;
            }
            _ => {
                let Rows::Complete(mappings) = &mut input.skill_abilities else {
                    unreachable!()
                };
                let mut successor = mapping(2, 95, SIBLING, 6, 1);
                successor.forward_spell_id = SEED;
                mappings.push(successor);
            }
        }
        let mage = initialize_mage(&input).unwrap();
        let mut live = LiveTraining::from_initializer(&input, &mage).unwrap();
        let before = live.snapshot();
        let view = trained_view(&input, &mage);
        assert!(live.apply(&callback(&view), &view).is_err());
        assert_eq!(live.snapshot(), before, "unsupported case {unsupported}");
    }
}

#[test]
fn inactive_to_active_transition_cannot_commit_a_partial_notification_journal() {
    let mut input = nested_input();
    input.spells[1].learned_spells = Rows::Complete(vec![
        dependency(CHILD, false),
        dependency(SIBLING, true),
        dependency(CHILD, true),
    ]);
    let mage = initialize_mage(&input).unwrap();
    let mut live = LiveTraining::from_initializer(&input, &mage).unwrap();
    let before = live.snapshot();
    let view = trained_view(&input, &mage);
    assert!(matches!(
        live.apply(&callback(&view), &view),
        Err(InitError::Unsupported {
            domain: "initial spell active-state transition",
            id: CHILD,
            ..
        })
    ));
    assert_eq!(live.snapshot(), before);
}

#[test]
fn skill_step_false_return_with_active_child_stays_transactionally_unsupported() {
    let mut input = nested_input();
    let Rows::Complete(infos) = &mut input.skill_race_class else {
        unreachable!()
    };
    infos[0].tier = 77;
    // SetSkillStep exits without a setter when the captured tier maximum is zero.
    input.skill_tiers.push(SkillTierRow {
        id: 77,
        values: [0; 16],
        maxima: [0; 16],
    });
    let root = &mut input.spells[1];
    root.words[61] = 36;
    root.words[82] = 1;
    root.words[109] = CHILD;
    root.words[62] = 44;
    root.words[77] = 1;
    root.words[83] = 1;
    root.words[107] = 95;
    let mage = initialize_mage(&input).unwrap();
    let fresh = LiveTraining::from_initializer(&input, &mage).unwrap();
    let mut live = fresh.clone();
    let before = live.snapshot();
    let view = trained_view(&input, &mage);
    assert_eq!(
        live.apply(&callback(&view), &view),
        Err(InitError::Invalid("live training changed spellbook domain"))
    );
    assert_eq!(live.snapshot(), before);
    let mut tampered = before;
    tampered.calls.push(TrainingCallSnapshot {
        id: 95,
        value: view.skills[0].value,
        step: view.skills[0].step,
        view: view.clone(),
        learned_spells: vec![LEAF, CHILD],
    });
    assert!(LiveTraining::restore_authoritative(&fresh, &tampered, &view).is_err());
}
