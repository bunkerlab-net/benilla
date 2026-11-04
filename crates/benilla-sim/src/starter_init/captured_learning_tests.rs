// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored loader rows and initial-cast context, never installation data.
use super::*;

fn line(id: u32, category: u32) -> [u32; 22] {
    let mut w = [0; 22];
    w[0] = id;
    w[1] = category;
    w
}
fn info(id: u32, skill: u32, tier: u32) -> [u32; 8] {
    [id, skill, 1, 128, 0x10, 0, tier, 0]
}
fn ability(id: u32, skill: u32, spell: u32) -> [u32; 15] {
    let mut w = [0; 15];
    w[0] = id;
    w[1] = skill;
    w[2] = spell;
    w[9] = 2;
    w
}
fn numeric(id: u32) -> LearningSpellNumeric {
    let mut w = [0; 173];
    w[0] = id;
    LearningSpellNumeric {
        source_identity: [17; 32],
        id,
        client_words: w,
        effective_words: w,
        duration_ms: Some(0),
    }
}
fn authored_chain_probe() -> Rows<CapturedSpellChain> {
    Rows::Complete(vec![CapturedSpellChain {
        spell: 1,
        previous: 0,
        first: 1,
        rank: 1,
        required: 0,
    }])
}
fn authored_learn_probe() -> Rows<CapturedLearnSpell> {
    Rows::Complete(vec![CapturedLearnSpell {
        entry: 1,
        spell: 2,
        active: false,
    }])
}
fn fixture() -> CapturedStarterLearning {
    CapturedStarterLearning {
        source_identity: [17; 32],
        selected_spell_ids: vec![133, 6603],
        skill_training: Rows::Complete(vec![]),
        creation_skills: Rows::Complete(vec![
            CreationSkill {
                race_mask: 0,
                class_mask: 0,
                skill: 8,
                step: 0,
            },
            CreationSkill {
                race_mask: 1,
                class_mask: 128,
                skill: 95,
                step: 0,
            },
        ]),
        creation_spells: Rows::Complete(vec![
            CreationSpell {
                race: 1,
                class: 8,
                spell: 6603,
            },
            CreationSpell {
                race: 1,
                class: 8,
                spell: 133,
            },
        ]),
        skill_lines: Rows::Complete(vec![line(8, 7), line(95, 6)]),
        skill_race_class: Rows::Complete(vec![info(20, 8, 0), info(10, 95, 0)]),
        skill_tiers: Rows::Complete(vec![]),
        skill_abilities: Rows::Complete(vec![ability(30, 8, 133)]),
        talents: Rows::Complete(vec![]),
        chains: Rows::Complete(vec![]),
        learn_spells: Rows::Complete(vec![]),
        global_chain_probe: Rows::Complete(vec![]),
        global_learn_probe: Rows::Complete(vec![]),
        spell_existence: Rows::Complete(vec![(133, true), (6603, true)]),
        numeric: Rows::Complete(vec![numeric(133), numeric(6603)]),
    }
}
fn empty_input() -> StarterInput {
    StarterInput {
        race: 1,
        class: 8,
        gender: 0,
        level: 1,
        class_power_type: 0,
        create_attributes: Attributes([20, 20, 20, 23, 22]),
        create_health: 31,
        create_mana: 100,
        default_skills: Rows::Missing,
        default_spells: Rows::Missing,
        skill_lines: vec![],
        skill_race_class: Rows::Missing,
        skill_tiers: vec![],
        skill_abilities: Rows::Missing,
        spells: vec![],
        active_support: vec![],
        item_grants: Rows::Complete(vec![]),
        items: vec![],
        item_creation_hooks: false,
    }
}
fn bind(c: &CapturedStarterLearning) -> Result<StarterInput> {
    let plan = c.compile(1, 8)?;
    let numeric = c.numeric.require("numeric", 0)?;
    let hooks = Rows::Complete(
        numeric
            .iter()
            .map(|r| LearningInitialHooks {
                spell: r.id,
                source_identity: c.source_identity,
                external_hooks: Some(false),
            })
            .collect(),
    );
    let spells = plan.bind_spell_rows(numeric, &hooks)?;
    plan.populate(empty_input(), spells)
}
fn add_spell(c: &mut CapturedStarterLearning, row: LearningSpellNumeric) {
    c.selected_spell_ids.push(row.id);
    if let Rows::Complete(rows) = &mut c.spell_existence {
        rows.push((row.id, true));
    }
    if let Rows::Complete(rows) = &mut c.numeric {
        rows.push(row);
    }
}

#[test]
fn shared_python_rust_ability_filter_cases() {
    let fixture_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../scripts/native_starter/ability_filter_cases.json"
    ))
    .unwrap();
    for case in fixture_json["cases"].as_array().unwrap() {
        let mut c = fixture();
        let mut row = ability(31, 8, 90001);
        row[3] = case["race_mask"].as_u64().unwrap() as u32;
        row[4] = case["class_mask"].as_u64().unwrap() as u32;
        row[7] = case["minimum"].as_u64().unwrap() as u32;
        row[9] = case["acquire"].as_u64().unwrap() as u32;
        c.skill_abilities = Rows::Complete(vec![row]);
        c.skill_training = Rows::Complete(vec![(90001, None)]);
        let included = case["included"].as_bool().unwrap();
        if included {
            add_spell(&mut c, numeric(90001));
        }
        let plan = c.compile(1, 8).unwrap();
        assert_eq!(
            plan.ability_dispositions(),
            &[(31, case["disposition"].as_str().unwrap())]
        );
        assert_eq!(
            plan.abilities.iter().any(|row| row.spell == 90001),
            included
        );
    }
}

#[test]
fn missing_sql_repair_probe_cannot_be_inferred_from_client_absence() {
    let mut c = fixture();
    let mut row = ability(31, 8, 90001);
    row[9] = 0;
    c.skill_abilities = Rows::Complete(vec![row]);
    assert_eq!(
        c.compile(1, 8),
        Err(missing("SQL skill training probe", 90001))
    );
}

#[test]
fn repair_and_reached_skill_steps_read_basepoints_not_mechanics() {
    let mut c = fixture();
    if let Rows::Complete(rows) = &mut c.skill_abilities {
        rows[0][9] = 0;
    }
    if let Rows::Complete(rows) = &mut c.numeric {
        rows[0].effective_words[61] = 118;
        rows[0].effective_words[106] = 8;
        rows[0].effective_words[76] = 4;
        rows[1].effective_words[61] = 44;
        rows[1].effective_words[106] = 8;
        rows[1].effective_words[76] = 7;
    }
    let plan = c.compile(1, 8).unwrap();
    assert_eq!(plan.ability_dispositions(), &[(30, "profession_repair")]);
}

#[test]
fn source_roots_use_canonical_order_and_recursive_initializer_trace() {
    let c = fixture();
    let plan = c.compile(1, 8).unwrap();
    assert_eq!(plan.default_spells(), &[133, 6603]);
    assert_eq!(
        plan.default_skills(),
        &[
            DefaultSkill { id: 8, step: 0 },
            DefaultSkill { id: 95, step: 0 }
        ]
    );
    let actual = bind(&c).unwrap();
    let mut explicit = empty_input();
    explicit.default_skills = Rows::Complete(vec![
        DefaultSkill { id: 8, step: 0 },
        DefaultSkill { id: 95, step: 0 },
    ]);
    explicit.default_spells = Rows::Complete(vec![133, 6603]);
    explicit.skill_lines = vec![
        SkillLineRow { id: 8, category: 7 },
        SkillLineRow {
            id: 95,
            category: 6,
        },
    ];
    explicit.skill_race_class = Rows::Complete(vec![
        SkillRaceClassRow {
            row_id: 10,
            skill: 95,
            race_mask: 1,
            class_mask: 128,
            flags: 16,
            tier: 0,
        },
        SkillRaceClassRow {
            row_id: 20,
            skill: 8,
            race_mask: 1,
            class_mask: 128,
            flags: 16,
            tier: 0,
        },
    ]);
    explicit.skill_abilities = Rows::Complete(vec![SkillAbilityRow {
        row_id: 30,
        skill: 8,
        spell: 133,
        forward_spell_id: 0,
        race_mask: 0,
        class_mask: 0,
        required_value: 0,
        learn_on_get_skill: 2,
        max_value: 0,
    }]);
    explicit.spells = [133, 6603]
        .into_iter()
        .map(|id| SpellRow {
            id,
            words: numeric(id).effective_words,
            duration_ms: Some(0),
            previous_rank: None,
            talent: false,
            learned_spells: Rows::Complete(vec![]),
            external_initial_hooks: false,
        })
        .collect();
    assert_eq!(
        initialize_mage(&actual).unwrap(),
        initialize_mage(&explicit).unwrap()
    );
    let mage = initialize_mage(&actual).unwrap();
    assert!(mage.abilities.contains_key(&133));
    assert!(mage.abilities.contains_key(&6603));
    assert_eq!(mage.skills[&8].maximum, 5);
    let learned = mage
        .trace
        .iter()
        .position(|e| *e == InitEvent::SpellLearned { spell: 133 })
        .unwrap();
    let sql = mage
        .trace
        .iter()
        .position(|e| *e == InitEvent::DefaultSpells)
        .unwrap();
    assert!(learned < sql);
}

#[test]
fn canonical_loader_permutations_preserve_initializer_and_missing_inputs_refuse() {
    let original = fixture();
    let expected = bind(&original).unwrap();
    let mut permuted = original.clone();
    if let Rows::Complete(rows) = &mut permuted.creation_skills {
        rows.reverse();
    }
    if let Rows::Complete(rows) = &mut permuted.creation_spells {
        rows.reverse();
    }
    if let Rows::Complete(rows) = &mut permuted.learn_spells {
        rows.reverse();
    }
    let actual = bind(&permuted).unwrap();
    assert_eq!(expected, actual);
    assert_eq!(
        initialize_mage(&expected).unwrap(),
        initialize_mage(&actual).unwrap()
    );
    permuted.creation_spells = Rows::Missing;
    assert!(permuted.compile(1, 8).is_err());
}

#[test]
fn sql_wildcards_do_not_make_dbc_masks_wildcards_and_dispositions_are_retained() {
    let mut c = fixture();
    if let Rows::Complete(rows) = &mut c.skill_race_class {
        rows[0][2] = 0;
    }
    let plan = c.compile(1, 8).unwrap();
    assert_eq!(plan.default_skills(), &[DefaultSkill { id: 95, step: 0 }]);
    assert_eq!(
        plan.skill_dispositions()[0],
        SkillLoaderDisposition::Unobtainable { skill: 8 }
    );
    if let Rows::Complete(rows) = &mut c.creation_skills {
        rows[0].race_mask = 2;
    }
    assert_eq!(
        c.compile(1, 8).unwrap().skill_dispositions(),
        &[
            SkillLoaderDisposition::Loaded(DefaultSkill { id: 95, step: 0 }),
            SkillLoaderDisposition::UnmatchedMasks,
        ]
    );
}

#[test]
fn skill_step_zero_one_sixteen_seventeen_and_observed_tier_maxima() {
    for step in [0, 1, 16] {
        let mut c = fixture();
        if let Rows::Complete(rows) = &mut c.creation_skills {
            rows[0].step = step;
        }
        if step != 0 {
            if let Rows::Complete(rows) = &mut c.skill_race_class {
                rows[0][6] = 9;
            }
            assert_eq!(c.compile(1, 8).unwrap_err(), missing("SkillTiers", 9));
            let mut tier = [0; 33];
            tier[0] = 9;
            tier[16 + step as usize] = 75;
            c.skill_tiers = Rows::Complete(vec![tier]);
            assert_eq!(
                c.compile(1, 8).unwrap().default_skills()[0].step,
                step as u16
            );
            tier[16 + step as usize] = 0;
            c.skill_tiers = Rows::Complete(vec![tier]);
            assert_eq!(c.compile(1, 8).unwrap().default_skills().len(), 1);
        } else {
            assert_eq!(c.compile(1, 8).unwrap().default_skills().len(), 2);
        }
    }
    let mut c = fixture();
    if let Rows::Complete(rows) = &mut c.creation_skills {
        rows[0].step = 17;
    }
    assert!(c.compile(1, 8).is_err());
}

#[test]
fn duplicate_effective_skills_and_dbc_rows_refuse_instead_of_overwriting() {
    let mut c = fixture();
    if let Rows::Complete(rows) = &mut c.creation_skills {
        rows.push(rows[0]);
    }
    assert!(c.compile(1, 8).is_err());
    let mut c = fixture();
    if let Rows::Complete(rows) = &mut c.skill_abilities {
        rows.push(rows[0]);
    }
    assert!(c.compile(1, 8).is_err());
}

#[test]
fn global_empty_learn_branch_differs_from_selected_empty_and_all_slots_are_scanned() {
    for slot in 0..3 {
        let mut c = fixture();
        if let Rows::Complete(rows) = &mut c.numeric {
            rows[0].effective_words[61 + slot] = 36;
            rows[0].effective_words[109 + slot] = 6603;
        }
        let plan = c.compile(1, 8).unwrap();
        assert!(plan.learned.is_empty());
        c.global_learn_probe = authored_learn_probe();
        let plan = c.compile(1, 8).unwrap();
        assert_eq!(
            plan.learned[&133],
            vec![LearnSpell {
                spell_id: 6603,
                active: true,
                auto_learned: false
            }]
        );
        c.learn_spells = Rows::Complete(vec![CapturedLearnSpell {
            entry: 133,
            spell: 6603,
            active: false,
        }]);
        assert_eq!(
            c.compile(1, 8).unwrap().learned[&133],
            vec![LearnSpell {
                spell_id: 6603,
                active: false,
                auto_learned: false
            }]
        );
    }
}

#[test]
fn derived_auto_learning_flags_preserve_pet_passive_talent_and_skill_step() {
    for kind in 0..4 {
        let mut c = fixture();
        c.global_learn_probe = authored_learn_probe();
        if let Rows::Complete(rows) = &mut c.numeric {
            rows[0].effective_words[61] = 36;
            rows[0].effective_words[109] = 6603;
            match kind {
                0 => rows[0].effective_words[82] = 5,
                1 => rows[0].effective_words[6] = 0x40,
                2 => rows[0].effective_words[62] = 44,
                _ => {}
            }
        }
        if kind == 3 {
            let mut t = [0; 21];
            t[0] = 1;
            t[4] = 133;
            c.talents = Rows::Complete(vec![t]);
        }
        assert!(c.compile(1, 8).unwrap().learned[&133][0].auto_learned);
    }
}

#[test]
fn forward_ranks_preserve_first_metadata_and_existing_initializer_refuses_successors() {
    let mut c = fixture();
    if let Rows::Complete(rows) = &mut c.skill_abilities {
        rows[0][8] = 6603;
        rows.push(ability(10, 95, 6603));
    }
    let plan = c.compile(1, 8).unwrap();
    assert_eq!(plan.rank(133).unwrap().previous, 0);
    assert_eq!(plan.rank(133).unwrap().rank, 1);
    assert_eq!(plan.rank(6603).unwrap().previous, 133);
    assert!(matches!(
        initialize_mage(&bind(&c).unwrap()),
        Err(InitError::Unsupported {
            domain: "initial spellbook successor",
            ..
        })
    ));
    c.chains = Rows::Complete(vec![CapturedSpellChain {
        spell: 6603,
        previous: 0,
        first: 6603,
        rank: 1,
        required: 0,
    }]);
    c.global_chain_probe = authored_chain_probe();
    assert!(c.compile(1, 8).is_err());
}

#[test]
fn talents_rank_cycles_conflicts_and_missing_existence_refuse_precisely() {
    let mut c = fixture();
    let mut t = [0; 21];
    t[0] = 1;
    t[4] = 133;
    t[5] = 6603;
    c.talents = Rows::Complete(vec![t]);
    let plan = c.compile(1, 8).unwrap();
    assert_eq!(plan.rank(6603).unwrap().rank, 2);
    assert!(matches!(
        initialize_mage(&bind(&c).unwrap()),
        Err(InitError::Unsupported {
            domain: "initial spell rank/talent",
            ..
        })
    ));
    let mut other = t;
    other[0] = 2;
    c.talents = Rows::Complete(vec![t, other]);
    assert!(c.compile(1, 8).is_err());
    let mut c = fixture();
    if let Rows::Complete(rows) = &mut c.skill_abilities {
        rows[0][8] = 6603;
        let mut a = ability(10, 95, 6603);
        a[8] = 133;
        rows.push(a);
    }
    assert!(c.compile(1, 8).is_err());
    let mut c = fixture();
    if let Rows::Complete(rows) = &mut c.skill_abilities {
        rows[0][8] = 999;
    }
    assert_eq!(
        c.compile(1, 8).unwrap_err(),
        missing("server SpellEntry existence", 999)
    );
}

#[test]
fn unknown_tables_numeric_hooks_selected_ids_and_identity_do_not_become_absent() {
    for kind in 0..8 {
        let mut c = fixture();
        match kind {
            0 => c.talents = Rows::Missing,
            1 => c.numeric = Rows::Missing,
            2 => c.global_chain_probe = Rows::Missing,
            3 => c.global_learn_probe = Rows::Missing,
            4 => c.skill_abilities = Rows::Missing,
            5 => c.selected_spell_ids.pop().map(|_| ()).unwrap(),
            6 => c.source_identity = [18; 32],
            _ => c.spell_existence = Rows::Missing,
        }
        assert!(c.compile(1, 8).is_err(), "case {kind}");
    }
    let c = fixture();
    let plan = c.compile(1, 8).unwrap();
    let numeric = c.numeric.require("numeric", 0).unwrap();
    assert!(plan.bind_spell_rows(numeric, &Rows::Missing).is_err());
    let hooks = Rows::Complete(
        numeric
            .iter()
            .map(|r| LearningInitialHooks {
                spell: r.id,
                source_identity: c.source_identity,
                external_hooks: None,
            })
            .collect(),
    );
    assert!(plan.bind_spell_rows(numeric, &hooks).is_err());
    let mut changed = numeric.to_vec();
    changed[0].client_words[6] = 1;
    assert!(plan.bind_spell_rows(&changed, &hooks).is_err());
}

#[test]
fn recursive_spell_learning_cycles_refuse_and_inactive_links_are_not_rewritten() {
    let mut c = fixture();
    c.global_learn_probe = authored_learn_probe();
    c.learn_spells = Rows::Complete(vec![
        CapturedLearnSpell {
            entry: 133,
            spell: 6603,
            active: false,
        },
        CapturedLearnSpell {
            entry: 6603,
            spell: 133,
            active: true,
        },
    ]);
    assert_eq!(
        c.compile(1, 8).unwrap_err(),
        InitError::Invalid("learning spell cycle")
    );
}

#[test]
fn authored_mandatory_passives_and_dodge_use_existing_preparation() {
    let mut c = fixture();
    for id in [81, 227, 20597, 20598, 20599, 20864] {
        let authored = super::super::initial_cast_phases::tests::authored(id);
        let mut n = numeric(id);
        n.client_words = authored.row.words;
        n.effective_words = authored.row.words;
        n.duration_ms = authored.row.duration_ms;
        add_spell(&mut c, n);
        if let Rows::Complete(rows) = &mut c.creation_spells {
            rows.push(CreationSpell {
                race: 1,
                class: 8,
                spell: id,
            });
        }
    }
    let input = bind(&c).unwrap();
    let ids = vec![81, 227, 20597, 20598, 20599, 20864];
    let deps = StarterInitialDependencies {
        source_identity: input.source_identity(),
        store_identity: [21; 32],
        hook_scope_identity: [22; 32],
        queried_spell_ids: ids.clone(),
        rows: Rows::Complete(
            ids.iter()
                .map(|id| {
                    let authored = super::super::initial_cast_phases::tests::authored(*id);
                    let p = authored.prerequisites.unwrap();
                    let row = input.spells.iter().find(|r| r.id == *id).unwrap();
                    StarterInitialDependency {
                        spell_id: *id,
                        source_hash: row.source_hash(),
                        cast_time: InitialSpellLookup::Missing {
                            index: row.words[18],
                        },
                        duration: if row.words[30] == 0 {
                            InitialSpellLookup::Missing { index: 0 }
                        } else {
                            InitialSpellLookup::Found(SpellDurationSource {
                                id: row.words[30],
                                base_ms: -1,
                                per_level_ms: 0,
                                maximum_ms: -1,
                            })
                        },
                        server_side: 0,
                        server_attributes: 0,
                        bonus_coefficients: p.effect_bonus_coefficient_bits,
                        attack_power_coefficients: [0; 3],
                    }
                })
                .collect(),
        ),
    };
    let order = StarterCastOrder {
        creation: InitialCastOrderEvidence::Authored { identity: [23; 32] },
        first_entry: InitialCastOrderEvidence::Authored { identity: [24; 32] },
        entry_skill_order: Some(vec![8, 95]),
    };
    let prepared = prepare_starter(&input, &deps, &order).unwrap();
    assert_eq!(prepared.created().effective_attributes.0[4], 23);
    assert_eq!(prepared.created().weapon_proficiency, 1 << 10);
    assert!(prepared
        .created()
        .initial_auras
        .iter()
        .any(|a| a.spell_id == 20599 && a.amount == 10));
    assert!(prepared
        .creation_plan()
        .occurrences()
        .iter()
        .any(|cast| cast.row().id == 81));
    let mut unknown = order.clone();
    unknown.creation = InitialCastOrderEvidence::Unknown;
    assert!(prepare_starter(&input, &deps, &unknown).is_err());
    let mut missing_passive = input.clone();
    missing_passive.spells.retain(|s| s.id != 20598);
    assert!(prepare_starter(&missing_passive, &deps, &order).is_err());
}

#[test]
fn collection_reference_and_numeric_domains_are_bounded_before_cloning() {
    for kind in 0..9 {
        let mut c = fixture();
        match kind {
            0 => {
                c.creation_skills = Rows::Complete(vec![
                    CreationSkill {
                        race_mask: 0,
                        class_mask: 0,
                        skill: 8,
                        step: 0
                    };
                    513
                ])
            }
            1 => c.skill_lines = Rows::Complete(vec![[0; 22]; 513]),
            2 => c.skill_race_class = Rows::Complete(vec![[0; 8]; 4097]),
            3 => c.skill_tiers = Rows::Complete(vec![[0; 33]; 513]),
            4 => c.skill_abilities = Rows::Complete(vec![[0; 15]; 4097]),
            5 => c.talents = Rows::Complete(vec![[0; 21]; 4097]),
            6 => c.spell_existence = Rows::Complete(vec![(1, true); 16_385]),
            7 => c.numeric = Rows::Complete(vec![numeric(133); 513]),
            _ => {
                c.learn_spells = Rows::Complete(vec![
                    CapturedLearnSpell {
                        entry: 133,
                        spell: 6603,
                        active: true
                    };
                    16_385
                ])
            }
        }
        assert!(c.compile(1, 8).is_err(), "case {kind}");
    }
    for bits in [0x7fc0_0000, 0x7f80_0000, 0x8000_0000] {
        let mut c = fixture();
        if let Rows::Complete(rows) = &mut c.numeric {
            rows[0].effective_words[73] = bits;
        }
        assert!(c.compile(1, 8).is_err());
    }
}

#[test]
fn language_weapon_class_and_mono_ranges_stay_owned_by_the_initializer() {
    for (category, expected) in [(10, (300, 300)), (6, (1, 5)), (7, (5, 5)), (9, (5, 5))] {
        let mut c = fixture();
        if let Rows::Complete(rows) = &mut c.skill_lines {
            rows[0][1] = category;
        }
        let mage = initialize_mage(&bind(&c).unwrap()).unwrap();
        assert_eq!((mage.skills[&8].value, mage.skills[&8].maximum), expected);
    }
}

#[test]
fn first_applicable_info_and_equal_spell_ability_insertion_follow_dbc_row_ids() {
    let mut c = fixture();
    c.skill_race_class = Rows::Complete(vec![
        info(200, 8, 0),
        [2, 8, 1, 128, 0, 0, 9, 0],
        info(10, 95, 0),
    ]);
    let mut tier = [0; 33];
    tier[0] = 9;
    tier[17] = 17;
    c.skill_tiers = Rows::Complete(vec![tier]);
    let mage = initialize_mage(&bind(&c).unwrap()).unwrap();
    assert_eq!((mage.skills[&8].value, mage.skills[&8].maximum), (1, 17));
    if let Rows::Complete(rows) = &mut c.skill_abilities {
        rows.push(ability(1, 95, 133));
    }
    let plan = c.compile(1, 8).unwrap();
    assert_eq!(
        plan.abilities.iter().map(|r| r.row_id).collect::<Vec<_>>(),
        [1, 30]
    );
    assert_eq!(plan.abilities.iter().filter(|r| r.spell == 133).count(), 2);
}

#[test]
fn first_valid_skill_effect_recursively_trains_another_skill_spell() {
    let mut c = fixture();
    if let Rows::Complete(rows) = &mut c.creation_skills {
        rows.pop();
    }
    if let Rows::Complete(rows) = &mut c.numeric {
        rows[0].effective_words[61] = 118;
        rows[0].effective_words[106] = 0;
        rows[0].effective_words[62] = 118;
        rows[0].effective_words[107] = 95;
        rows[0].effective_words[68] = 1;
    }
    if let Rows::Complete(rows) = &mut c.skill_race_class {
        rows[1][6] = 9;
    }
    let mut tier = [0; 33];
    tier[0] = 9;
    tier[17] = 75;
    c.skill_tiers = Rows::Complete(vec![tier]);
    if let Rows::Complete(rows) = &mut c.skill_abilities {
        rows.push(ability(31, 95, 6603));
    }
    let mage = initialize_mage(&bind(&c).unwrap()).unwrap();
    assert_eq!(mage.skills[&95].maximum, 75);
    let learned: Vec<_> = mage
        .trace
        .iter()
        .filter_map(|event| match event {
            InitEvent::SpellLearned { spell } => Some(*spell),
            _ => None,
        })
        .collect();
    assert_eq!(learned, [133, 6603]);
}

#[test]
fn shared_authored_sql_dbc_obtainability_cases() {
    let fixture_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../scripts/native_starter/learning_cases.json"
    ))
    .unwrap();
    for case in fixture_json["cases"].as_array().unwrap() {
        let mut capture = fixture();
        let step = case["step"].as_u64().unwrap() as u32;
        if let Rows::Complete(rows) = &mut capture.creation_skills {
            rows[0].race_mask = case["sql_masks"][0].as_u64().unwrap() as u32;
            rows[0].class_mask = case["sql_masks"][1].as_u64().unwrap() as u32;
            rows[0].step = step;
        }
        if let Rows::Complete(rows) = &mut capture.skill_race_class {
            rows[0][2] = case["dbc_masks"][0].as_u64().unwrap() as u32;
            rows[0][3] = case["dbc_masks"][1].as_u64().unwrap() as u32;
            rows[0][6] = 9;
        }
        if let Some(maximum) = case["tier_max"].as_u64() {
            let mut tier = [0; 33];
            tier[0] = 9;
            if (1..=16).contains(&step) {
                tier[16 + step as usize] = maximum as u32;
            }
            capture.skill_tiers = Rows::Complete(vec![tier]);
        }
        let result = capture.compile(1, 8);
        assert_eq!(
            result.is_err(),
            case["refused"].as_bool().unwrap(),
            "{case}"
        );
        if let Ok(plan) = result {
            assert_eq!(
                plan.default_skills().iter().any(|r| r.id == 8),
                case["loaded"].as_bool().unwrap(),
                "{case}"
            );
        }
    }
}

#[test]
fn optional_active_executor_refusal_does_not_delete_effective_learning_links() {
    let mut c = fixture();
    let mut armor = numeric(168);
    armor.effective_words[61] = 6;
    armor.effective_words[91] = 22;
    add_spell(&mut c, armor);
    c.creation_spells = Rows::Complete(vec![CreationSpell {
        race: 1,
        class: 8,
        spell: 168,
    }]);
    c.global_learn_probe = authored_learn_probe();
    c.learn_spells = Rows::Complete(vec![CapturedLearnSpell {
        entry: 168,
        spell: 6603,
        active: true,
    }]);
    let mage = initialize_mage(&bind(&c).unwrap()).unwrap();
    assert!(matches!(
        mage.abilities[&168].availability,
        AbilityAvailability::Unavailable(_)
    ));
    assert!(mage.abilities.contains_key(&6603));
    assert_eq!(
        mage.trace
            .iter()
            .filter(|event| **event == InitEvent::SpellLearned { spell: 6603 })
            .count(),
        1
    );
}

#[test]
fn zero_mode_profession_repair_keeps_the_first_valid_skill_effect() {
    let mut c = fixture();
    if let Rows::Complete(rows) = &mut c.creation_skills {
        rows[0].step = 2;
    }
    if let Rows::Complete(rows) = &mut c.skill_lines {
        rows[0][1] = 11;
    }
    if let Rows::Complete(rows) = &mut c.skill_race_class {
        rows[0][4] = 0;
        rows[0][6] = 9;
    }
    if let Rows::Complete(rows) = &mut c.skill_abilities {
        rows[0][9] = 0;
    }
    if let Rows::Complete(rows) = &mut c.numeric {
        rows[0].effective_words[61] = 118;
        rows[0].effective_words[106] = 8;
        rows[0].effective_words[67] = 1;
    }
    let mut tier = [0; 33];
    tier[0] = 9;
    tier[17] = 75;
    tier[18] = 150;
    c.skill_tiers = Rows::Complete(vec![tier]);
    let input = bind(&c).unwrap();
    let mage = initialize_mage(&input).unwrap();
    assert_eq!((mage.skills[&8].step, mage.skills[&8].maximum), (1, 75));
    assert_eq!(
        mage.trace
            .iter()
            .filter(|e| **e == InitEvent::SpellLearned { spell: 133 })
            .count(),
        1
    );
    assert_eq!(first_entry(&input, &mage).unwrap().skills[&8].maximum, 75);
}

#[test]
fn forward_chain_depth_conflicting_predecessors_and_rank_source_domains_refuse() {
    let mut c = fixture();
    let mut rows = vec![];
    for offset in 0..34 {
        let id = 50000 + offset;
        add_spell(&mut c, numeric(id));
        let mut a = ability(100 + offset, 8, id);
        a[8] = if offset == 33 { 0 } else { id + 1 };
        rows.push(a);
    }
    c.skill_abilities = Rows::Complete(rows);
    assert!(matches!(
        c.compile(1, 8),
        Err(InitError::Unsupported {
            domain: "learning rank depth",
            ..
        })
    ));
    let mut c = fixture();
    add_spell(&mut c, numeric(50000));
    let mut first = ability(1, 8, 133);
    first[8] = 6603;
    let mut other = ability(2, 8, 50000);
    other[8] = 6603;
    c.skill_abilities = Rows::Complete(vec![first, other, ability(3, 8, 6603)]);
    assert!(matches!(
        c.compile(1, 8),
        Err(InitError::Unsupported {
            domain: "conflicting forward predecessor",
            ..
        })
    ));
    let mut c = fixture();
    c.global_chain_probe = authored_chain_probe();
    c.chains = Rows::Complete(vec![CapturedSpellChain {
        spell: 133,
        previous: 0,
        first: 133,
        rank: 128,
        required: 0,
    }]);
    assert!(c.compile(1, 8).is_err());
    assert!(fixture().compile(2, 8).is_err());
    assert!(fixture().compile(1, 1).is_err());
}

#[test]
fn global_probes_retain_raw_rows_and_require_exact_selected_agreement() {
    let mut c = fixture();
    let learn = CapturedLearnSpell {
        entry: 133,
        spell: 6603,
        active: false,
    };
    c.global_learn_probe = Rows::Complete(vec![learn]);
    assert_eq!(
        c.compile(1, 8).unwrap_err(),
        InitError::Invalid("learning global probe selected-row binding")
    );
    c.learn_spells = Rows::Complete(vec![learn]);
    assert!(c.compile(1, 8).is_ok());
    c.global_learn_probe = Rows::Complete(vec![learn, learn]);
    assert_eq!(
        c.compile(1, 8).unwrap_err(),
        InitError::Invalid("learning global probe bound")
    );
    let mut c = fixture();
    let chain = CapturedSpellChain {
        spell: 133,
        previous: 0,
        first: 133,
        rank: 1,
        required: 0,
    };
    c.global_chain_probe = Rows::Complete(vec![chain]);
    assert_eq!(
        c.compile(1, 8).unwrap_err(),
        InitError::Invalid("learning global probe selected-row binding")
    );
    c.chains = Rows::Complete(vec![chain]);
    assert!(c.compile(1, 8).is_ok());
}

#[test]
fn numeric_candidate_keeps_sql_words_and_requires_independent_hooks() {
    let mut capture = fixture();
    let Rows::Complete(numeric) = &mut capture.numeric else {
        unreachable!()
    };
    numeric[0].client_words[76] = 1;
    numeric[0].effective_words[76] = (-7i32) as u32;
    numeric[0].duration_ms = None;
    let compiled = capture.compile(1, 8).unwrap();
    let numeric = capture.numeric.require("numeric", 0).unwrap();
    assert!(compiled.bind_spell_rows(numeric, &Rows::Missing).is_err());
    let unknown = Rows::Complete(
        numeric
            .iter()
            .map(|row| LearningInitialHooks {
                spell: row.id,
                source_identity: capture.source_identity,
                external_hooks: None,
            })
            .collect(),
    );
    assert!(compiled.bind_spell_rows(numeric, &unknown).is_err());
    let hooks = Rows::Complete(
        numeric
            .iter()
            .map(|row| LearningInitialHooks {
                spell: row.id,
                source_identity: capture.source_identity,
                external_hooks: Some(false),
            })
            .collect(),
    );
    let bound = compiled.bind_spell_rows(numeric, &hooks).unwrap();
    assert_eq!(bound[0].words[76], (-7i32) as u32);
    assert_eq!(bound[0].duration_ms, None);
    let mut conflicting = numeric.to_vec();
    conflicting[0].effective_words[76] = 1;
    assert!(compiled.bind_spell_rows(&conflicting, &hooks).is_err());
    conflicting = numeric.to_vec();
    conflicting[0].source_identity[0] ^= 1;
    assert!(compiled.bind_spell_rows(&conflicting, &hooks).is_err());
}

#[test]
fn sparse_dbc_ids_define_loader_views_not_physical_record_order() {
    let mut capture = fixture();
    if let Rows::Complete(rows) = &mut capture.skill_abilities {
        let mut excluded = ability(4_000_000, 8, 133);
        excluded[3] = 2;
        rows.push(excluded);
    }
    let expected = capture.compile(1, 8).unwrap();
    let mut shuffled = capture;
    if let Rows::Complete(rows) = &mut shuffled.skill_race_class {
        rows.reverse();
    }
    if let Rows::Complete(rows) = &mut shuffled.skill_abilities {
        rows.reverse();
    }
    assert_eq!(shuffled.compile(1, 8).unwrap(), expected);
}
