// SPDX-License-Identifier: GPL-3.0-or-later
//! All IDs/rows in this file are synthetic probes, never bundled Classic content.
use benilla_sim::starter_init::*;
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
    let mut s = spell(id);
    s.words[6] = 0x40;
    s.words[61] = 6;
    s.words[64] = 1;
    s.words[67] = 1;
    s.words[76] = (amount - 1) as u32;
    s.words[82] = 1;
    s.words[91] = kind;
    s.words[106] = misc as u32;
    s
}
fn item(id: u32) -> ItemRow {
    ItemRow {
        id,
        class: 4,
        subclass: 0,
        inventory_type: 5,
        allowable_class: u32::MAX,
        allowable_race: u32::MAX,
        required_level: 1,
        required_skill: 0,
        required_skill_rank: 0,
        required_spell: 0,
        required_honor_rank: 0,
        required_reputation_faction: 0,
        flags: 0,
        unique_max_count: 0,
        stackable: 1,
        bag_family: 0,
        container_slots: 0,
        item_set: 0,
        random_property: 0,
        duration: 0,
        scripted: false,
        stats: [ItemStat { kind: 0, value: 0 }; 10],
        armor: 0,
        resistances: [0; 6],
        block: 0,
        spells: [ItemSpell { id: 0, trigger: 0 }; 5],
        damage: [ItemDamage {
            min_bits: 0,
            max_bits: 0,
            school: 0,
        }; 5],
        delay_ms: 0,
        max_durability: 0,
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
        default_skills: Rows::Complete(vec![]),
        default_spells: Rows::Complete(vec![]),
        skill_lines: vec![],
        skill_race_class: Rows::Complete(vec![]),
        skill_tiers: vec![],
        skill_abilities: Rows::Complete(vec![]),
        spells: vec![],
        active_support: vec![],
        item_grants: Rows::Complete(vec![]),
        items: vec![],
        item_creation_hooks: false,
    }
}
#[test]
fn extracted_source_proficiency_masks_skip_any_overlapping_grant() {
    let mut count = 0;
    for line in include_str!("fixtures/starter_proficiency_cmangos.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<u32> = line
            .split('\t')
            .skip(1)
            .map(|value| value.parse().unwrap())
            .collect();
        let mut i = input();
        for (index, &mask) in fields[1..3].iter().enumerate() {
            let mut grant = spell(900_000_120 + index as u32);
            grant.words[6] = 0x40;
            grant.words[58] = fields[0];
            grant.words[59] = mask;
            grant.words[61] = 60;
            grant.words[82] = 1;
            i.spells.push(grant);
        }
        i.default_spells = Rows::Complete(i.spells.iter().map(|row| row.id).collect());
        let initialized = initialize_mage(&i).unwrap();
        assert_eq!(initialized.weapon_proficiency, fields[3], "{line}");
        assert_eq!(initialized.armor_proficiency, fields[4], "{line}");
        if fields[1] != fields[2] && fields[1] & fields[2] != 0 {
            assert!(matches!(
                first_entry(&i, &initialized),
                Err(InitError::Unsupported {
                    domain: "first-entry order-dependent proficiency masks",
                    ..
                })
            ));
        } else {
            first_entry(&i, &initialized).unwrap();
        }
        count += 1;
    }
    assert_eq!(count, 40);
}
fn info(row: u32, skill: u16, flags: u32, tier: u32) -> SkillRaceClassRow {
    SkillRaceClassRow {
        row_id: row,
        skill,
        race_mask: 1,
        class_mask: 1 << 7,
        flags,
        tier,
    }
}
fn mapping(row: u32, skill: u16, spell: u32, learn: u32) -> SkillAbilityRow {
    SkillAbilityRow {
        row_id: row,
        skill,
        spell,
        forward_spell_id: 0,
        race_mask: 0,
        class_mask: 0,
        required_value: 0,
        learn_on_get_skill: learn,
        max_value: 0,
    }
}
#[test]
fn extracted_source_stats_and_deferred_create_load_order() {
    let fixture = include_str!("fixtures/starter_init_cmangos.tsv");
    let mut count = 0;
    for line in fixture.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<_> = line.split('\t').collect();
        let v: Vec<i32> = f[..17].iter().map(|v| v.parse().unwrap()).collect();
        let mode = v[0] as usize;
        let mut i = input();
        i.create_attributes = Attributes([v[1]; 5]);
        i.create_health = 37;
        i.create_mana = 53;
        i.spells = vec![aura(900_000_001, 137, mode as i32, v[2])];
        i.default_spells = Rows::Complete(vec![900_000_001]);
        let mut it = item(900_000_002);
        it.stats[0] = ItemStat {
            kind: [4, 3, 7, 5, 6][mode],
            value: v[3],
        };
        it.stats[1] = ItemStat { kind: 1, value: 7 };
        it.stats[2] = ItemStat { kind: 0, value: 11 };
        it.armor = 13;
        i.items = vec![it];
        i.item_grants = Rows::Complete(vec![ItemGrant {
            source: GrantSource::Outfit,
            source_order: 0,
            item: 900_000_002,
            count: 1,
        }]);
        let out = initialize_mage(&i).unwrap();
        assert_eq!(
            out.effective_attributes.0,
            [v[7], v[8], v[9], v[10], v[11]],
            "{line}"
        );
        assert_eq!(
            out.resources,
            Resources {
                health: v[12] as u32,
                max_health: v[13] as u32,
                mana: v[14] as u32,
                max_mana: v[15] as u32
            },
            "{line}"
        );
        assert_eq!(out.resistances[0], v[16], "{line}");
        assert_eq!(f[17], "IKLEUarcsdbm012R");
        let enable = out
            .trace
            .iter()
            .position(|e| matches!(e, InitEvent::EnableStatModification))
            .unwrap();
        let equip = out
            .trace
            .iter()
            .position(|e| matches!(e, InitEvent::Equip { .. }))
            .unwrap();
        let learn = out
            .trace
            .iter()
            .position(|e| matches!(e, InitEvent::DefaultSpells))
            .unwrap();
        let skill = out
            .trace
            .iter()
            .position(|e| matches!(e, InitEvent::DefaultSkills))
            .unwrap();
        assert!(skill < learn && learn < equip && equip < enable);
        for event in &out.trace[..enable] {
            if let InitEvent::Stats {
                attributes,
                resources,
                ..
            } = event
            {
                assert_eq!(attributes.0[mode], v[4]);
                assert_eq!(resources.max_health, v[5] as u32);
                assert_eq!(resources.max_mana, v[6] as u32);
            }
        }
        count += 1;
    }
    assert_eq!(count, 840);
}
#[test]
fn no_missing_rows_can_become_empty_or_default() {
    let mut i = input();
    i.default_skills = Rows::Missing;
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Missing {
            table: "playercreateinfo_skills",
            ..
        })
    ));
    i.default_skills = Rows::Complete(vec![DefaultSkill { id: 60001, step: 0 }]);
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Missing {
            table: "SkillLine",
            id: 60001
        })
    ));
    let mut i = input();
    i.skill_abilities = Rows::Missing;
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Missing {
            table: "SkillLineAbility",
            ..
        })
    ));
    let mut i = input();
    i.item_grants = Rows::Missing;
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Missing {
            table: "complete outfit/custom grants",
            ..
        })
    ));
    let mut i = input();
    i.default_spells = Rows::Complete(vec![900_000_099]);
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Missing {
            table: "spell_template",
            id: 900_000_099
        })
    ));
}
#[test]
fn initial_unknown_effect_blocks_but_active_unknown_is_visible_unavailable() {
    let mut i = input();
    let mut s = spell(900_000_010);
    s.words[61] = 77;
    s.words[82] = 1;
    i.spells = vec![s];
    i.default_spells = Rows::Complete(vec![900_000_010]);
    let out = initialize_mage(&i).unwrap();
    assert!(matches!(
        &out.abilities[&900_000_010].availability,
        AbilityAvailability::Unavailable(_)
    ));
    i.spells[0].words[6] = 0x40;
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "initial effect",
            detail: 77,
            ..
        })
    ));
    i.spells[0].words[6] = 0;
    i.spells[0].words[7] = 0x80000000;
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "initial effect",
            detail: 77,
            ..
        })
    ));
}
#[test]
fn passive_unknown_aura_random_hook_and_stack_fail_closed() {
    let mut i = input();
    i.spells = vec![aura(900_000_020, 999, 4, 5)];
    i.default_spells = Rows::Complete(vec![900_000_020]);
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "initial aura",
            detail: 999,
            ..
        })
    ));
    i.spells[0] = aura(900_000_020, 137, 4, 5);
    i.spells[0].words[64] = 2;
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "initial effect random/scaling",
            ..
        })
    ));
    i.spells[0] = aura(900_000_020, 137, 4, 5);
    i.spells[0].external_initial_hooks = true;
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "initial spell external hook",
            ..
        })
    ));
    i.spells = vec![aura(900_000_020, 137, 4, 5), aura(900_000_021, 137, 4, 5)];
    i.default_spells = Rows::Complete(vec![900_000_020, 900_000_021]);
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "initial aura stacking",
            ..
        })
    ));
}
#[test]
fn default_skills_are_one_not_level_max_unless_source_maximized() {
    let mut i = input();
    i.skill_lines = vec![
        SkillLineRow {
            id: 60001,
            category: 6,
        },
        SkillLineRow {
            id: 60002,
            category: 10,
        },
        SkillLineRow {
            id: 60003,
            category: 7,
        },
    ];
    i.skill_race_class = Rows::Complete(vec![info(1, 60003, 0x10, 0)]);
    i.default_skills = Rows::Complete(vec![
        DefaultSkill { id: 60001, step: 0 },
        DefaultSkill { id: 60002, step: 0 },
        DefaultSkill { id: 60003, step: 0 },
    ]);
    let out = initialize_mage(&i).unwrap();
    assert_eq!(
        (out.skills[&60001].value, out.skills[&60001].maximum),
        (1, 5)
    );
    assert_eq!(
        (out.skills[&60002].value, out.skills[&60002].maximum),
        (300, 300)
    );
    assert_eq!(
        (out.skills[&60003].value, out.skills[&60003].maximum),
        (5, 5)
    );
    assert_eq!(out.current_skills().unwrap().value(60001), 1);
}
#[test]
fn skill_learning_is_recursive_dbc_row_order_and_bonus_reapplies_after_insertion() {
    let mut i = input();
    i.skill_lines = vec![
        SkillLineRow {
            id: 60001,
            category: 6,
        },
        SkillLineRow {
            id: 60002,
            category: 7,
        },
    ];
    i.skill_race_class = Rows::Complete(vec![info(1, 60002, 0x10, 0)]);
    i.default_skills = Rows::Complete(vec![DefaultSkill { id: 60001, step: 0 }]);
    i.spells = vec![
        aura(900_000_030, 98, 60001, 5),
        spell(900_000_031),
        spell(900_000_032),
    ];
    i.skill_abilities = Rows::Complete(vec![
        mapping(30, 60002, 900_000_032, 1),
        mapping(20, 60002, 900_000_031, 2),
        mapping(10, 60001, 900_000_030, 1),
    ]);
    i.default_spells = Rows::Complete(vec![900_000_031]);
    let out = initialize_mage(&i).unwrap();
    assert_eq!(out.skills[&60001].permanent_bonus, 5);
    assert_eq!(out.skills[&60001].current_value(), 6);
    assert_eq!(out.skills[&60002].value, 5);
    let learned: Vec<_> = out
        .trace
        .iter()
        .filter_map(|e| {
            if let InitEvent::SpellLearned { spell } = e {
                Some(*spell)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(learned, vec![900_000_030, 900_000_031, 900_000_032]);
}
#[test]
fn skill_step_returns_before_non_auto_dependencies_and_effect118_teaches_step() {
    let mut i = input();
    i.skill_lines = vec![SkillLineRow {
        id: 60003,
        category: 11,
    }];
    i.skill_race_class = Rows::Complete(vec![info(1, 60003, 0, 1)]);
    i.skill_tiers = vec![SkillTierRow {
        id: 1,
        values: [0; 16],
        maxima: [75; 16],
    }];
    let mut s = spell(900_000_040);
    s.words[61] = 44;
    s.words[82] = 1;
    s.words[64] = 1;
    s.words[67] = 1;
    s.words[106] = 60003;
    s.learned_spells = Rows::Complete(vec![LearnSpell {
        spell_id: 999_999_999,
        active: true,
        auto_learned: false,
    }]);
    i.spells = vec![s];
    i.default_spells = Rows::Complete(vec![900_000_040]);
    let out = initialize_mage(&i).unwrap();
    assert_eq!(
        (
            out.skills[&60003].value,
            out.skills[&60003].maximum,
            out.skills[&60003].step
        ),
        (1, 75, 1)
    );
    assert!(!out.abilities.contains_key(&999_999_999));
    i.spells[0].words[61] = 118;
    i.spells[0].learned_spells = Rows::Complete(vec![]);
    let out = initialize_mage(&i).unwrap();
    assert_eq!(out.skills[&60003].maximum, 75);
}
#[test]
fn all_matching_skill_infos_use_source_row_order_and_tiers_are_required() {
    let mut i = input();
    i.skill_lines = vec![SkillLineRow {
        id: 60001,
        category: 7,
    }];
    i.default_skills = Rows::Complete(vec![DefaultSkill { id: 60001, step: 0 }]);
    i.skill_race_class = Rows::Complete(vec![info(9, 60001, 0x10, 0), info(3, 60001, 0, 77)]);
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Missing {
            table: "SkillTiers",
            id: 77
        })
    ));
    i.skill_tiers = vec![SkillTierRow {
        id: 77,
        values: [0; 16],
        maxima: [61; 16],
    }];
    assert_eq!(initialize_mage(&i).unwrap().skills[&60001].maximum, 61);
}
#[test]
fn equipment_is_source_ordered_skill_gated_and_carried_items_grant_no_stats() {
    let mut i = input();
    let mut robe = item(900_000_050);
    robe.inventory_type = 20;
    robe.armor = 9;
    let mut other = item(900_000_051);
    other.armor = 900;
    let mut staff = item(900_000_052);
    staff.class = 2;
    staff.subclass = 10;
    staff.inventory_type = 17;
    staff.stats[0] = ItemStat { kind: 5, value: 99 };
    let mut food = item(900_000_053);
    food.inventory_type = 0;
    food.class = 0;
    food.stackable = 20;
    i.items = vec![robe, other, staff, food];
    i.item_grants = Rows::Complete(vec![
        ItemGrant {
            source: GrantSource::Outfit,
            source_order: 0,
            item: 900_000_050,
            count: 1,
        },
        ItemGrant {
            source: GrantSource::Outfit,
            source_order: 1,
            item: 900_000_051,
            count: 1,
        },
        ItemGrant {
            source: GrantSource::Outfit,
            source_order: 2,
            item: 900_000_052,
            count: 1,
        },
        ItemGrant {
            source: GrantSource::Custom,
            source_order: 0,
            item: 900_000_053,
            count: 4,
        },
    ]);
    let out = initialize_mage(&i).unwrap();
    assert_eq!(out.equipped.len(), 1);
    assert_eq!(out.equipped[&4].item, 900_000_050);
    assert_eq!(out.inventory.len(), 3);
    assert_eq!(out.effective_attributes, i.create_attributes);
    assert_eq!(out.resistances[0], 55);
    i.skill_lines = vec![SkillLineRow {
        id: 136,
        category: 6,
    }];
    i.default_skills = Rows::Complete(vec![DefaultSkill { id: 136, step: 0 }]);
    let out = initialize_mage(&i).unwrap();
    assert_eq!(out.equipped[&15].weapon_skill, 136);
    assert_eq!(out.effective_attributes.0[3], 130);
    assert!(out.resources.mana < out.resources.max_mana);
}
#[test]
fn equipment_effects_missing_rows_inventory_overflow_and_wrong_order_block() {
    let mut i = input();
    let mut it = item(900_000_060);
    it.spells[0] = ItemSpell {
        id: 900_000_061,
        trigger: 1,
    };
    i.items = vec![it];
    i.item_grants = Rows::Complete(vec![ItemGrant {
        source: GrantSource::Outfit,
        source_order: 0,
        item: 900_000_060,
        count: 1,
    }]);
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "initial item equip/proc spell",
            ..
        })
    ));
    i.items.clear();
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Missing {
            table: "item_template",
            id: 900_000_060
        })
    ));
    let mut it = item(900_000_060);
    it.inventory_type = 0;
    i.items = vec![it];
    i.item_grants = Rows::Complete(vec![ItemGrant {
        source: GrantSource::Outfit,
        source_order: 0,
        item: 900_000_060,
        count: 17,
    }]);
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "initial backpack capacity",
            ..
        })
    ));
    i.item_grants = Rows::Complete(vec![
        ItemGrant {
            source: GrantSource::Custom,
            source_order: 0,
            item: 900_000_060,
            count: 1,
        },
        ItemGrant {
            source: GrantSource::Outfit,
            source_order: 1,
            item: 900_000_060,
            count: 1,
        },
    ]);
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Invalid("outfit/custom source order"))
    ));
}

#[test]
fn extracted_source_default_and_step_skill_vectors() {
    let mut n = 0;
    for line in include_str!("fixtures/starter_skills_cmangos.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = line.split('\t').collect();
        let v: Vec<u32> = f[1..].iter().map(|x| x.parse().unwrap()).collect();
        let mut i = input();
        i.skill_tiers = vec![SkillTierRow {
            id: 1,
            values: [42; 16],
            maxima: std::array::from_fn(|n| ((n + 1) * 75) as u32),
        }];
        if f[0] == "D" {
            i.skill_lines = vec![SkillLineRow {
                id: 60001,
                category: v[0] as i32,
            }];
            i.skill_race_class = Rows::Complete(vec![info(1, 60001, v[1], v[2])]);
            i.default_skills = Rows::Complete(vec![DefaultSkill {
                id: 60001,
                step: v[3] as u16,
            }]);
            let result = initialize_mage(&i);
            if matches!(v[0], 7 | 8 | 9 | 11) && v[1] == 0 && v[2] == 0 {
                assert_eq!(v[4], 0);
                assert!(matches!(
                    result,
                    Err(InitError::Missing {
                        table: "usable default SkillRaceClassInfo/tier",
                        ..
                    })
                ));
            } else {
                let out = result.unwrap();
                if v[4] == 0 {
                    assert!(!out.skills.contains_key(&60001));
                } else {
                    let s = out.skills[&60001];
                    assert_eq!(
                        (u32::from(s.value), u32::from(s.maximum), u32::from(s.step)),
                        (v[4], v[5], v[6]),
                        "{line}"
                    );
                }
            }
        } else {
            i.skill_lines = vec![SkillLineRow {
                id: 60001,
                category: 11,
            }];
            i.skill_race_class = Rows::Complete(vec![info(1, 60001, v[0], 1)]);
            if v[1] != 0 {
                i.default_skills = Rows::Complete(vec![DefaultSkill { id: 60001, step: 0 }]);
            }
            let mut s = spell(900_000_070);
            s.words[61] = 44;
            s.words[82] = 1;
            s.words[64] = 1;
            s.words[67] = 1;
            s.words[76] = v[2] - 1;
            s.words[106] = 60001;
            i.spells = vec![s];
            i.default_spells = Rows::Complete(vec![900_000_070]);
            let out = initialize_mage(&i).unwrap();
            let s = out.skills[&60001];
            assert_eq!(
                (u32::from(s.value), u32::from(s.maximum), u32::from(s.step)),
                (v[3], v[4], v[5]),
                "{line}"
            );
        }
        n += 1;
    }
    assert_eq!(n, 108);
}

#[test]
fn dormant_or_conditioned_passives_cannot_hide_unknown_modifiers() {
    let mut i = input();
    let mut s = aura(900_000_080, 999, 0, 5);
    s.words[11] = 4;
    i.spells = vec![s];
    i.default_spells = Rows::Complete(vec![900_000_080]);
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "conditional initial passive",
            ..
        })
    ));
    i.spells[0].words[11] = 0;
    i.spells[0].words[16] = 1;
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "conditional initial passive",
            ..
        })
    ));
}

#[test]
fn retained_unlearned_skill_aura_and_effective_resource_derivatives() {
    let mut i = input();
    i.spells = vec![
        aura(900_000_081, 98, 60010, 15),
        aura(900_000_082, 137, 3, 5),
        aura(900_000_083, 137, 4, 5),
    ];
    i.default_spells = Rows::Complete(vec![900_000_081, 900_000_082, 900_000_083]);
    let out = initialize_mage(&i).unwrap();
    assert!(!out.skills.contains_key(&60010));
    assert_eq!(
        out.initial_auras[0],
        InitialAura {
            spell_id: 900_000_081,
            effect_index: 0,
            aura_type: 98,
            misc_value: 60010,
            amount: 15
        }
    );
    assert_eq!(out.effective_attributes.0[3], 32);
    assert_eq!(out.effective_attributes.0[4], 38);
    assert!(out.mana_regeneration().is_ok());
    assert!(out.spell_crit_percent().is_ok());
    assert!(out.resources.mana < out.resources.max_mana);
}

#[test]
fn item_break_uses_per_modifier_clamps_and_never_refills_on_repair() {
    let mut i = input();
    i.create_attributes.0[2] = 30;
    let mut it = item(900_000_090);
    it.max_durability = 7;
    // During removal stamina lowers max first, then removing the negative flat
    // health bonus raises max. One final clamp would miss the intervening loss.
    it.stats[0] = ItemStat { kind: 7, value: 10 };
    it.stats[1] = ItemStat {
        kind: 1,
        value: -50,
    };
    it.armor = 21;
    it.resistances[2] = 4;
    i.items = vec![it];
    i.item_grants = Rows::Complete(vec![ItemGrant {
        source: GrantSource::Outfit,
        source_order: 0,
        item: 900_000_090,
        count: 1,
    }]);
    let out = initialize_mage(&i).unwrap();
    assert_eq!(
        (out.equipped[&4].durability, out.equipped[&4].max_durability),
        (7, 7)
    );
    let mut recipe = out.stat_recipe.clone();
    let mut live = out.resources;
    live.health = live.max_health;
    live.mana = 17;
    let broken = recipe.set_item_broken(4, true, live).unwrap();
    assert_eq!(broken.modifier_steps[0].max_health, 111);
    assert_eq!(broken.resources.max_health, 161);
    assert_eq!(broken.resources.health, 111);
    assert_eq!(broken.resources.mana, 17);
    assert_eq!(broken.resistances[0], 46);
    assert_eq!(broken.resistances[3], 0);
    assert_eq!(
        StatRecipe::restore(&out.stat_recipe, &recipe.snapshot()).unwrap(),
        recipe
    );
    let unchanged = recipe.set_item_broken(4, true, broken.resources).unwrap();
    assert_eq!(unchanged.resources, broken.resources);
    assert!(unchanged.modifier_steps.is_empty());
    let repaired = recipe.set_item_broken(4, false, broken.resources).unwrap();
    assert_eq!(repaired.resources.health, 111);
    assert_eq!(repaired.resources.mana, 17);
    assert_eq!(repaired.resources.max_health, live.max_health);
    assert_eq!(recipe.snapshot(), out.stat_recipe.snapshot());
}

#[test]
fn recipe_snapshot_is_strict_and_bound_to_original_modifiers() {
    let mut i = input();
    let mut it = item(900_000_091);
    it.max_durability = 20;
    it.stats[0] = ItemStat { kind: 5, value: 10 };
    i.items = vec![it];
    i.item_grants = Rows::Complete(vec![ItemGrant {
        source: GrantSource::Outfit,
        source_order: 0,
        item: 900_000_091,
        count: 1,
    }]);
    let out = initialize_mage(&i).unwrap();
    let original = out.stat_recipe.snapshot();
    let text = serde_json::to_string(&original).unwrap();
    let decoded: StatRecipeSnapshot = serde_json::from_str(&text).unwrap();
    assert_eq!(decoded, original);
    let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
    json["extra"] = true.into();
    assert!(serde_json::from_value::<StatRecipeSnapshot>(json).is_err());
    let mut changed = original.clone();
    changed.items[0].stats[0].value += 1;
    assert!(StatRecipe::restore(&out.stat_recipe, &changed).is_err());
    let mut changed = original.clone();
    changed.modifiers.attributes[3][0] = 0x7fc00000;
    assert!(StatRecipe::restore(&out.stat_recipe, &changed).is_err());
    let mut changed = original.clone();
    changed.broken_slots = vec![4];
    assert!(StatRecipe::restore(&out.stat_recipe, &changed).is_err());
    let mut recipe = out.stat_recipe.clone();
    let before = recipe.clone();
    let mut bad = out.resources;
    bad.max_mana += 1;
    assert!(recipe.set_item_broken(4, true, bad).is_err());
    assert_eq!(recipe, before);
}

#[test]
fn broken_weapon_is_disabled_and_nondurable_item_cannot_break() {
    let mut i = input();
    i.skill_lines = vec![SkillLineRow {
        id: 136,
        category: 6,
    }];
    i.default_skills = Rows::Complete(vec![DefaultSkill { id: 136, step: 0 }]);
    let mut weapon = item(900_000_092);
    weapon.class = 2;
    weapon.subclass = 10;
    weapon.inventory_type = 17;
    weapon.max_durability = 12;
    weapon.stats[0] = ItemStat { kind: 6, value: 5 };
    let cloth = item(900_000_093);
    i.items = vec![weapon, cloth];
    i.item_grants = Rows::Complete(vec![
        ItemGrant {
            source: GrantSource::Outfit,
            source_order: 0,
            item: 900_000_092,
            count: 1,
        },
        ItemGrant {
            source: GrantSource::Outfit,
            source_order: 1,
            item: 900_000_093,
            count: 1,
        },
    ]);
    let out = initialize_mage(&i).unwrap();
    let mut recipe = out.stat_recipe.clone();
    assert!(recipe.set_item_broken(4, true, out.resources).is_err());
    let result = recipe.set_item_broken(15, true, out.resources).unwrap();
    assert!(result.disabled_weapon_slots.contains(&15));
    assert_eq!(result.effective_attributes.0[4], i.create_attributes.0[4]);
    assert_eq!(out.skills[&136].value, 1);
    assert_eq!(out.initial_auras.len(), 0);
}

#[test]
fn extracted_source_equipment_break_repair_vectors_and_intermediate_resources() {
    let mut count = 0;
    for line in include_str!("fixtures/starter_equipment_cmangos.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = line.split('\t').collect();
        let a: Vec<i32> = f[..15].iter().map(|x| x.parse().unwrap()).collect();
        let mut i = input();
        i.create_attributes = Attributes([a[0]; 5]);
        i.create_health = 41;
        i.create_mana = 59;
        let mut it = item(900_000_094);
        it.max_durability = 10;
        for (n, (kind, value)) in [(7, a[1]), (1, a[2]), (5, a[3]), (0, a[4])]
            .into_iter()
            .enumerate()
        {
            it.stats[n] = ItemStat { kind, value };
        }
        it.armor = 13;
        it.resistances[2] = 5;
        i.items = vec![it];
        i.item_grants = Rows::Complete(vec![ItemGrant {
            source: GrantSource::Outfit,
            source_order: 0,
            item: 900_000_094,
            count: 1,
        }]);
        let init = initialize_mage(&i).unwrap();
        let mut recipe = init.stat_recipe.clone();
        let live = Resources {
            health: a[5] as u32,
            max_health: a[6] as u32,
            mana: a[7] as u32,
            max_mana: a[8] as u32,
        };
        let broken = recipe.set_item_broken(4, true, live).unwrap();
        assert_eq!(
            broken.resources,
            Resources {
                health: a[9] as u32,
                max_health: a[10] as u32,
                mana: a[11] as u32,
                max_mana: a[12] as u32
            },
            "{line}"
        );
        assert_eq!(broken.resistances[0], a[13]);
        assert_eq!(broken.resistances[3], a[14]);
        let expected_steps: Vec<Resources> = f[15]
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let v: Vec<u32> = s
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.parse().unwrap())
                    .collect();
                Resources {
                    health: v[0],
                    max_health: v[1],
                    mana: v[2],
                    max_mana: v[3],
                }
            })
            .collect();
        assert_eq!(broken.modifier_steps, expected_steps, "{line}");
        assert_eq!(
            StatRecipe::restore(&init.stat_recipe, &recipe.snapshot()).unwrap(),
            recipe
        );
        let repaired = recipe.set_item_broken(4, false, broken.resources).unwrap();
        let r: Vec<u32> = f[16..].iter().map(|x| x.parse().unwrap()).collect();
        assert_eq!(
            repaired.resources,
            Resources {
                health: r[0],
                max_health: r[1],
                mana: r[2],
                max_mana: r[3]
            },
            "{line}"
        );
        assert_eq!(recipe.snapshot(), init.stat_recipe.snapshot());
        count += 1;
    }
    assert_eq!(count, 144);
}

#[test]
fn first_entry_skill_gate_returns_source_steps_and_blocks_unreplayed_changes() {
    let mut i = input();
    i.skill_lines = vec![
        SkillLineRow {
            id: 60001,
            category: 7,
        },
        SkillLineRow {
            id: 60002,
            category: 6,
        },
    ];
    i.skill_race_class = Rows::Complete(vec![info(1, 60001, 0x10, 0)]);
    i.default_skills = Rows::Complete(vec![
        DefaultSkill { id: 60001, step: 0 },
        DefaultSkill { id: 60002, step: 0 },
    ]);
    let created = initialize_mage(&i).unwrap();
    assert_eq!(created.skills[&60001].step, 1);
    let entry = first_entry_skills(&i, &created).unwrap();
    assert_eq!(entry[&60001].step, 0);
    assert_eq!(entry[&60001].value, 5);
    assert_eq!(entry[&60002].value, 1);
    i.skill_race_class = Rows::Complete(vec![info(1, 60001, 0, 1)]);
    i.skill_tiers = vec![SkillTierRow {
        id: 1,
        values: [0; 16],
        maxima: [75; 16],
    }];
    let created = initialize_mage(&i).unwrap();
    let entry = first_entry_skills(&i, &created).unwrap();
    assert_eq!(
        (
            entry[&60001].value,
            entry[&60001].maximum,
            entry[&60001].step
        ),
        (1, 1, 0)
    );
    let mut i = input();
    let mut s = spell(900_000_095);
    s.words[61] = 118;
    s.words[106] = 60002;
    s.words[67] = 1;
    i.spells = vec![s];
    i.default_spells = Rows::Complete(vec![900_000_095]);
    i.skill_lines = vec![SkillLineRow {
        id: 60002,
        category: 11,
    }];
    i.skill_race_class = Rows::Complete(vec![info(2, 60002, 0, 1)]);
    i.skill_tiers = vec![SkillTierRow {
        id: 1,
        values: [0; 16],
        maxima: [75; 16],
    }];
    let created = initialize_mage(&i).unwrap();
    let entry = first_entry_skills(&i, &created).unwrap();
    assert_eq!(
        (
            entry[&60002].value,
            entry[&60002].maximum,
            entry[&60002].step
        ),
        (1, 75, 1)
    );
}

#[test]
fn extracted_source_first_entry_normalization_and_step_replay_vectors() {
    let mut count = 0;
    for line in include_str!("fixtures/starter_reload_cmangos.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let v: Vec<u32> = line
            .split('\t')
            .skip(1)
            .map(|x| x.parse().unwrap())
            .collect();
        let mut i = input();
        i.skill_lines = vec![SkillLineRow {
            id: 60001,
            category: v[0] as i32,
        }];
        i.skill_race_class = Rows::Complete(vec![info(1, 60001, v[1], v[2])]);
        i.skill_tiers = vec![SkillTierRow {
            id: 1,
            values: [42; 16],
            maxima: std::array::from_fn(|n| ((n + 1) * 75) as u32),
        }];
        // Explicit synthetic saved-skill adapter for the source _LoadSkills kernel.
        let mut created = initialize_mage(&i).unwrap();
        created.skills.insert(
            60001,
            InitializedSkill {
                id: 60001,
                value: v[3] as u16,
                maximum: v[4] as u16,
                step: 0,
                temporary_bonus: 0,
                permanent_bonus: 0,
            },
        );
        if v[5] != 0 {
            let mut s = spell(900_000_096);
            s.words[61] = 118;
            s.words[67] = 1;
            s.words[76] = v[5] - 1;
            s.words[106] = 60001;
            i.spells = vec![s];
            i.default_spells = Rows::Complete(vec![900_000_096]);
            created.abilities.insert(
                900_000_096,
                KnownAbility {
                    spell_id: 900_000_096,
                    active: true,
                    availability: AbilityAvailability::Unavailable(
                        "active ability has no admitted executor".into(),
                    ),
                },
            );
        }
        let result = first_entry_skills(&i, &created);
        if v[5] != 0 && v[2] == 0 {
            assert!(
                matches!(
                    result,
                    Err(InitError::Missing {
                        table: "first-entry skill-step tier mapping",
                        ..
                    })
                ),
                "{line}"
            );
        } else {
            let skills = result.unwrap();
            let skill = skills[&60001];
            assert_eq!(
                (
                    u32::from(skill.value),
                    u32::from(skill.maximum),
                    u32::from(skill.step)
                ),
                (v[6], v[7], v[8]),
                "{line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 1296);
}

#[test]
fn first_entry_conflicting_step_setters_and_changed_learning_are_blocked() {
    let mut i = input();
    i.skill_lines = vec![SkillLineRow {
        id: 60001,
        category: 11,
    }];
    i.skill_race_class = Rows::Complete(vec![info(1, 60001, 0, 1)]);
    i.skill_tiers = vec![SkillTierRow {
        id: 1,
        values: [0; 16],
        maxima: std::array::from_fn(|n| ((n + 1) * 75) as u32),
    }];
    for (id, step) in [(900_000_097, 1), (900_000_098, 2)] {
        let mut s = spell(id);
        s.words[61] = 118;
        s.words[67] = 1;
        s.words[76] = step - 1;
        s.words[106] = 60001;
        i.spells.push(s);
    }
    i.default_spells = Rows::Complete(vec![900_000_097, 900_000_098]);
    let created = initialize_mage(&i).unwrap();
    assert!(matches!(
        first_entry_skills(&i, &created),
        Err(InitError::Unsupported {
            domain: "first-entry conflicting skill steps",
            ..
        })
    ));
    i.spells.pop();
    i.default_spells = Rows::Complete(vec![900_000_097]);
    let created = initialize_mage(&i).unwrap();
    i.spells.push(spell(900_000_099));
    i.skill_abilities = Rows::Complete(vec![mapping(1, 60001, 900_000_099, 1)]);
    assert!(matches!(
        first_entry_skills(&i, &created),
        Err(InitError::Unsupported {
            domain: "first-entry changed trained spell closure",
            ..
        })
    ));
}

#[test]
fn first_entry_static_modifier_invariance_requires_known_reload_semantics() {
    let mut i = input();
    i.spells = vec![aura(900_000_100, 137, -1, 5), aura(900_000_101, 137, 3, 5)];
    i.default_spells = Rows::Complete(vec![900_000_100, 900_000_101]);
    let created = initialize_mage(&i).unwrap();
    assert!(matches!(
        first_entry_skills(&i, &created),
        Err(InitError::Invalid(
            "first-entry overlapping percent modifier order"
        ))
    ));
    i.spells = vec![aura(900_000_100, 137, 3, 5)];
    i.spells[0].words[6] = 0;
    i.spells[0].words[7] = 0x80000000;
    i.default_spells = Rows::Complete(vec![900_000_100]);
    let created = initialize_mage(&i).unwrap();
    assert!(matches!(
        first_entry_skills(&i, &created),
        Err(InitError::Unsupported {
            domain: "first-entry nonpassive aura reload",
            ..
        })
    ));
}

#[test]
fn first_entry_trains_every_loaded_skill_before_relearning_defaults() {
    let mut i = input();
    i.skill_lines = vec![SkillLineRow {
        id: 60001,
        category: 6,
    }];
    i.default_skills = Rows::Complete(vec![DefaultSkill { id: 60001, step: 0 }]);
    i.spells = vec![aura(900_000_110, 29, 3, 2), spell(900_000_111)];
    i.skill_abilities = Rows::Complete(vec![mapping(1, 60001, 900_000_110, 1)]);
    i.default_spells = Rows::Complete(vec![900_000_111]);
    let created = initialize_mage(&i).unwrap();
    let replay = first_entry(&i, &created).unwrap();
    assert_eq!(replay.skills, created.skills);
    assert_eq!(replay.trace[0], InitEvent::LoadSavedSkills);
    assert_eq!(
        replay.trace[1],
        InitEvent::TrainLoadedSkill {
            skill: 60001,
            value: 1
        }
    );
    let trained = replay
        .trace
        .iter()
        .position(|e| *e == InitEvent::SpellLearned { spell: 900_000_110 })
        .unwrap();
    let defaults = replay
        .trace
        .iter()
        .position(|e| *e == InitEvent::DefaultSkills)
        .unwrap();
    let default_spells = replay
        .trace
        .iter()
        .position(|e| *e == InitEvent::DefaultSpells)
        .unwrap();
    let root = replay
        .trace
        .iter()
        .position(|e| *e == InitEvent::SpellLearned { spell: 900_000_111 })
        .unwrap();
    assert!(trained < defaults && defaults < default_spells && default_spells < root);
    let mut phases = String::from("P\t");
    for event in &replay.trace {
        match event {
            InitEvent::TrainLoadedSkill { skill, value } => {
                phases.push_str(&format!("T:{skill}:{value},"));
            }
            InitEvent::SpellLearned { spell: 900_000_111 } => {
                phases.push_str("A:900000111:1:1:1:0,");
            }
            _ => {}
        }
    }
    phases.push('\n');
    // Text fixture records are exact; Git may check them out with CRLF on Windows.
    // Exercise both newline conventions without trimming fields or record content.
    let fixture = include_str!("fixtures/starter_entry_phases_cmangos.tsv");
    let lf = fixture.replace("\r\n", "\n");
    let crlf = lf.replace('\n', "\r\n");
    for expected in [&lf, &crlf] {
        assert_eq!(
            phases.lines().collect::<Vec<_>>(),
            expected.lines().collect::<Vec<_>>()
        );
    }
    // Normalization can newly cross a training threshold even without any
    // effect-44/118 setter. That changed passive closure cannot disappear.
    i.skill_race_class = Rows::Complete(vec![info(1, 60001, 0x10, 0)]);
    if let Rows::Complete(rows) = &mut i.skill_abilities {
        rows[0].required_value = 2;
    }
    let created = initialize_mage(&i).unwrap();
    assert!(!created.abilities.contains_key(&900_000_110));
    assert!(matches!(
        first_entry(&i, &created),
        Err(InitError::Unsupported {
            domain: "first-entry changed trained spell closure",
            ..
        })
    ));
}

#[test]
fn first_entry_requires_nondependent_saved_spell_semantics_for_cast_learn_effects() {
    let mut i = input();
    let mut parent = spell(900_000_112);
    parent.words[7] = 0x80000000;
    parent.words[61] = 36;
    parent.words[82] = 1;
    parent.words[109] = 900_000_113;
    i.spells = vec![parent, spell(900_000_113)];
    i.default_spells = Rows::Complete(vec![900_000_112]);
    let created = initialize_mage(&i).unwrap();
    assert!(matches!(
        first_entry(&i, &created),
        Err(InitError::Unsupported {
            domain: "first-entry nondependent learn-spell replay",
            ..
        })
    ));
}

#[test]
fn extracted_source_initial_effect_amount_uses_binary32_before_integer_fields() {
    let mut count = 0;
    for line in include_str!("fixtures/starter_effect_amount_cmangos.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<i32> = line
            .split('\t')
            .map(|value| value.parse().unwrap())
            .collect();
        let mut i = input();
        let mut s = aura(900_000_140, 29, 0, 0);
        s.words[76] = fields[0] as u32;
        s.words[67] = fields[1] as u32;
        s.words[64] = fields[2] as u32;
        i.spells = vec![s];
        i.default_spells = Rows::Complete(vec![900_000_140]);
        if fields[3].unsigned_abs() <= 1_000_000 {
            let initialized = initialize_mage(&i).unwrap();
            assert_eq!(initialized.initial_auras[0].amount, fields[3], "{line}");
            assert_eq!(
                initialized.effective_attributes.0[0],
                (17 + fields[3]).max(0),
                "{line}"
            );
        } else {
            assert!(
                matches!(initialize_mage(&i), Err(InitError::Numeric(_))),
                "{line}"
            );
        }
        i.spells[0].words[91] = 30;
        i.spells[0].words[106] = 60001;
        i.skill_lines = vec![SkillLineRow {
            id: 60001,
            category: 6,
        }];
        i.default_skills = Rows::Complete(vec![DefaultSkill { id: 60001, step: 0 }]);
        if let Ok(expected) = i16::try_from(fields[3]) {
            let initialized = initialize_mage(&i).unwrap();
            assert_eq!(
                initialized.skills[&60001].temporary_bonus, expected,
                "{line}"
            );
        } else {
            assert!(
                matches!(
                    initialize_mage(&i),
                    Err(InitError::Numeric("initial skill aura amount"))
                ),
                "{line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 24);
}

#[test]
fn initial_effect_amount_blocks_undefined_integer_domains_and_replays_float_skill_steps() {
    let mut i = input();
    i.spells = vec![aura(900_000_141, 29, 0, 1)];
    i.default_spells = Rows::Complete(vec![900_000_141]);
    for (points, dice) in [(i32::MAX, 1u32), (i32::MAX, 0), (0, 0x80000000)] {
        i.spells[0].words[76] = points as u32;
        i.spells[0].words[67] = dice;
        assert!(matches!(initialize_mage(&i), Err(InitError::Numeric(_))));
    }
    let s = &mut i.spells[0];
    s.words[6] = 0;
    s.words[61] = 44;
    s.words[76] = (-16_777_217i32) as u32;
    s.words[67] = 16_777_218;
    s.words[106] = 60001;
    i.skill_lines = vec![SkillLineRow {
        id: 60001,
        category: 11,
    }];
    i.skill_race_class = Rows::Complete(vec![info(1, 60001, 0, 1)]);
    i.skill_tiers = vec![SkillTierRow {
        id: 1,
        values: [1; 16],
        maxima: std::array::from_fn(|index| (index as u32 + 1) * 75),
    }];
    let initialized = initialize_mage(&i).unwrap();
    assert_eq!(initialized.skills[&60001].step, 2);
    assert_eq!(
        first_entry_skills(&i, &initialized).unwrap()[&60001].step,
        2
    );
}

#[test]
fn spellbook_successor_relationships_cannot_hide_outside_previous_rank_metadata() {
    let mut i = input();
    i.spells = vec![spell(900_000_150), spell(900_000_151)];
    i.default_spells = Rows::Complete(vec![900_000_150, 900_000_151]);
    let mut link = mapping(1, 60001, 900_000_150, 0);
    link.forward_spell_id = 900_000_151;
    i.skill_abilities = Rows::Complete(vec![link]);
    assert!(i.spells.iter().all(|row| row.previous_rank.is_none()));
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "initial spellbook successor",
            id: 900_000_150,
            detail: 900_000_151,
        })
    ));
    // The same gate applies when the source relationship is reached by a skill.
    link.learn_on_get_skill = 1;
    i.skill_abilities = Rows::Complete(vec![link]);
    i.default_spells = Rows::Complete(vec![]);
    i.skill_lines = vec![SkillLineRow {
        id: 60001,
        category: 6,
    }];
    i.default_skills = Rows::Complete(vec![DefaultSkill { id: 60001, step: 0 }]);
    assert!(matches!(
        initialize_mage(&i),
        Err(InitError::Unsupported {
            domain: "initial spellbook successor",
            ..
        })
    ));
    link.forward_spell_id = 0;
    i.skill_abilities = Rows::Complete(vec![link]);
    assert!(initialize_mage(&i).unwrap().abilities[&900_000_150].active);
}

#[test]
fn passive_duration_index_zero_is_permanent_without_inventing_a_duration() {
    let mut i = input();
    let mut s = aura(900_000_171, 29, 3, 7);
    s.words[30] = 0;
    s.duration_ms = Some(0);
    i.default_spells = Rows::Complete(vec![s.id]);
    i.spells.push(s);
    let created = initialize_mage(&i).unwrap();
    assert_eq!(created.effective_attributes.0[3], 38);
    assert_eq!(created.initial_auras.len(), 1);
    first_entry(&i, &created).unwrap();
    for (index, duration, passive) in [
        (1, Some(0), true),
        (0, None, true),
        (0, Some(1), true),
        (0, Some(0), false),
    ] {
        i.spells[0].words[30] = index;
        i.spells[0].duration_ms = duration;
        i.spells[0].words[6] = if passive { 64 } else { 0 };
        i.spells[0].words[7] = if passive { 0 } else { 0x8000_0000 };
        assert!(
            matches!(
                initialize_mage(&i),
                Err(InitError::Unsupported {
                    domain: "initial aura duration/trigger/equipment",
                    ..
                })
            ),
            "{index}:{duration:?}:{passive}"
        );
    }
}

#[test]
fn level_rebased_recipe_break_restore_uses_current_create_rows_and_crit() {
    let mut input = input();
    let mut it = item(900_000_097);
    it.max_durability = 7;
    it.stats[0] = ItemStat { kind: 5, value: 6 };
    it.stats[1] = ItemStat { kind: 7, value: 4 };
    input.items = vec![it];
    input.item_grants = Rows::Complete(vec![ItemGrant {
        source: GrantSource::Outfit,
        source_order: 0,
        item: 900_000_097,
        count: 1,
    }]);
    let initial = initialize_mage(&input).unwrap();
    let create = Attributes([24, 31, 37, 49, 51]);
    let fresh = initial.stat_recipe.rebase(7, create, 89, 123).unwrap();
    assert_eq!(fresh.snapshot().version, 3);
    assert_eq!(fresh.snapshot().level, 7);
    let m = fresh.snapshot().modifiers;
    let numeric =
        benilla_sim::player_progression::NumericState::from_modifiers(7, &m, 0, [0; 5]).unwrap();
    let live = Resources {
        health: numeric.max_health,
        max_health: numeric.max_health,
        mana: 17,
        max_mana: numeric.max_powers[0],
    };
    let mut broken = fresh.clone();
    let result = broken.set_item_broken(4, true, live).unwrap();
    let expected = benilla_sim::mage_resource::mage_spell_crit(
        7,
        49,
        benilla_sim::mage_resource::SpellCritModifiers {
            all_schools: 0,
            school: 0,
        },
    )
    .unwrap();
    assert_eq!(result.spell_crit_percent, expected);
    assert_eq!(result.resources.mana, 17);
    assert_eq!(result.resources.max_mana, 123 + 20 + (49 - 20) * 15);
    assert_eq!(
        StatRecipe::restore(&fresh, &broken.snapshot()).unwrap(),
        broken
    );
    assert!(StatRecipe::restore(&initial.stat_recipe, &broken.snapshot()).is_err());
    let mut tampered = broken.snapshot();
    tampered.level = 1;
    assert!(StatRecipe::restore(&fresh, &tampered).is_err());
    tampered = broken.snapshot();
    tampered.modifiers.create_mana += 1;
    assert!(StatRecipe::restore(&fresh, &tampered).is_err());
    let mut earlier_broken = initial.stat_recipe.clone();
    earlier_broken
        .set_item_broken(4, true, initial.resources)
        .unwrap();
    assert_eq!(earlier_broken.rebase(7, create, 89, 123).unwrap(), broken);
    let repaired = broken.set_item_broken(4, false, result.resources).unwrap();
    assert_eq!(repaired.resources.mana, 17);
    assert_eq!(broken, fresh);
    assert!(fresh.rebase(60, create, 89, 123).is_ok());
    assert!(fresh.rebase(61, create, 89, 123).is_err());
}

#[test]
fn initial_general_reputation_passive_is_retained_once_without_stat_changes() {
    let base = input();
    let plain = initialize_mage(&base).unwrap();
    let mut i = base.clone();
    // Authored source-shaped fields; this permanent duration is synthetic, not
    // an observation of actual Diplomacy DurationIndex21 or any client DBC.
    let mut s = aura(900_000_221, 156, 0, 10);
    s.words[6] = 0x50;
    s.words[25] = 101;
    s.words[167] = 1.0f32.to_bits();
    i.default_spells = Rows::Complete(vec![s.id, s.id]);
    i.spells.push(s.clone());
    let created = initialize_mage(&i).unwrap();
    assert_eq!(created.resources, plain.resources);
    assert_eq!(created.effective_attributes, plain.effective_attributes);
    assert_eq!(created.resistances, plain.resistances);
    assert_eq!(created.skills, plain.skills);
    assert_eq!(created.stat_recipe.snapshot(), plain.stat_recipe.snapshot());
    assert_eq!(
        created.initial_auras,
        vec![InitialAura {
            spell_id: s.id,
            effect_index: 0,
            aura_type: 156,
            misc_value: 0,
            amount: 10,
        }]
    );
    assert_eq!(created.abilities.len(), 1);
    first_entry(&i, &created).unwrap();
    first_entry(&i, &created).unwrap();
    assert_eq!(initialize_mage(&i).unwrap(), created);
    let mut changed = created.clone();
    changed.initial_auras[0].amount = 11;
    assert!(first_entry(&i, &changed).is_err());
    changed = created.clone();
    changed.initial_auras.clear();
    assert!(first_entry(&i, &changed).is_err());
}

#[test]
fn initial_general_reputation_aura_keeps_unknown_lifecycle_and_other_aura_gates_closed() {
    let mut base = input();
    let s = aura(900_000_222, 156, 0, 10);
    base.default_spells = Rows::Complete(vec![s.id]);
    base.spells.push(s);
    for field in [
        "active",
        "stance",
        "interrupt",
        "trigger",
        "duration",
        "unknown_duration",
        "scaling",
        "random",
        "conditional",
        "190",
        "147",
        "hook",
        "stack",
    ] {
        let mut i = base.clone();
        match field {
            "active" => {
                i.spells[0].words[6] = 0;
                i.spells[0].words[7] = 0x8000_0000;
            }
            "stance" => {
                i.spells[0].words[11] = 1;
                i.spells[0].words[8] = 0x80000;
            }
            "interrupt" => i.spells[0].words[22] = 4096,
            "trigger" => i.spells[0].words[109] = 900_000_224,
            "duration" => i.spells[0].duration_ms = Some(1000),
            "unknown_duration" => i.spells[0].duration_ms = None,
            "scaling" => i.spells[0].words[73] = 1.0f32.to_bits(),
            "random" => i.spells[0].words[64] = 2,
            "conditional" => i.spells[0].words[16] = 1,
            "190" => i.spells[0].words[91] = 190,
            "147" => i.spells[0].words[91] = 147,
            "hook" => i.spells[0].external_initial_hooks = true,
            "stack" => {
                i.spells.push(aura(900_000_223, 156, 0, 10));
                i.default_spells = Rows::Complete(vec![900_000_222, 900_000_223]);
            }
            _ => unreachable!(),
        }
        assert!(initialize_mage(&i).is_err(), "{field}");
    }
}

#[test]
fn initial_spell_source_identity_binds_all_words_and_resolved_relationships() {
    let source = aura(900_000_225, 156, 0, 10);
    let original = source.source_hash();
    for index in 0..173 {
        let mut changed = source.clone();
        changed.words[index] ^= 1;
        assert_ne!(original, changed.source_hash(), "word {index}");
    }
    for field in [
        "id",
        "duration",
        "duration_missing",
        "rank",
        "talent",
        "learning_missing",
        "learning",
        "hook",
    ] {
        let mut changed = source.clone();
        match field {
            "id" => changed.id += 1,
            "duration" => changed.duration_ms = Some(0),
            "duration_missing" => changed.duration_ms = None,
            "rank" => changed.previous_rank = Some(0),
            "talent" => changed.talent = true,
            "learning_missing" => changed.learned_spells = Rows::Missing,
            "learning" => {
                changed.learned_spells = Rows::Complete(vec![LearnSpell {
                    spell_id: 900_000_226,
                    active: true,
                    auto_learned: false,
                }])
            }
            "hook" => changed.external_initial_hooks = true,
            _ => unreachable!(),
        }
        assert_ne!(original, changed.source_hash(), "{field}");
    }
}

#[test]
fn armor_membership_is_independent_and_preserves_source_clamp_order() {
    let mut i = input();
    i.create_attributes.0[2] = 30;
    let mut armor = item(900_000_190);
    armor.max_durability = 7;
    armor.stats[0] = ItemStat { kind: 7, value: 10 };
    armor.stats[1] = ItemStat {
        kind: 1,
        value: -50,
    };
    armor.stats[2] = ItemStat { kind: 5, value: 10 };
    armor.armor = 21;
    armor.resistances[2] = 4;
    i.items = vec![armor];
    i.item_grants = Rows::Complete(vec![ItemGrant {
        source: GrantSource::Outfit,
        source_order: 0,
        item: 900_000_190,
        count: 1,
    }]);
    let out = initialize_mage(&i).unwrap();
    let mut recipe = out.stat_recipe.clone();
    let mut live = out.resources;
    live.health = live.max_health;
    live.mana = live.max_mana;
    let removed = recipe.set_item_active(4, false, live).unwrap();
    assert_eq!(recipe.snapshot().inactive_slots, vec![4]);
    assert!(recipe.snapshot().broken_slots.is_empty());
    assert_eq!(removed.modifier_steps.len(), 5);
    assert_eq!(removed.modifier_steps[0].max_health, 111);
    assert_eq!(removed.modifier_steps[1].max_health, 161);
    assert_eq!(removed.resources.health, 111);
    assert!(removed.resources.mana < live.mana);
    assert_eq!(
        removed
            .attribute_steps
            .iter()
            .map(|s| s.0)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert!(StatRecipe::restore(&out.stat_recipe, &recipe.snapshot()).is_err());
    let noop = recipe.set_item_active(4, false, removed.resources).unwrap();
    assert!(noop.modifier_steps.is_empty());
    let active = recipe.set_item_active(4, true, removed.resources).unwrap();
    assert_eq!(active.resources.max_health, live.max_health);
    assert_eq!(active.resources.max_mana, live.max_mana);
    assert_eq!(active.resources.health, removed.resources.health);
    assert_eq!(active.resources.mana, removed.resources.mana);
    assert_eq!(recipe.snapshot(), out.stat_recipe.snapshot());

    let broken = recipe.set_item_broken(4, true, active.resources).unwrap();
    let broken_recipe = recipe.snapshot();
    let removed = recipe.set_item_active(4, false, broken.resources).unwrap();
    assert!(removed.modifier_steps.is_empty());
    assert_eq!(removed.resources, broken.resources);
    assert_eq!(recipe.snapshot().broken_slots, vec![4]);
    let before = recipe.clone();
    assert!(matches!(
        recipe.set_item_broken(4, false, removed.resources),
        Err(InitError::Unsupported { .. })
    ));
    assert_eq!(recipe, before);
    let active = recipe.set_item_active(4, true, removed.resources).unwrap();
    assert!(active.modifier_steps.is_empty());
    assert_eq!(active.resources, broken.resources);
    assert_eq!(recipe.snapshot(), broken_recipe);
}

#[test]
fn armor_membership_cannot_be_restored_as_unproven_flags() {
    let out = initialize_mage(&input()).unwrap();
    let fresh = out.stat_recipe;
    for slots in [vec![4], vec![4, 4], vec![5, 4], vec![255]] {
        let mut forged = fresh.snapshot();
        forged.inactive_slots = slots;
        assert!(StatRecipe::restore(&fresh, &forged).is_err());
    }
    let mut old = fresh.snapshot();
    old.version = 2;
    assert!(StatRecipe::restore(&fresh, &old).is_err());
    let mut json = serde_json::to_value(fresh.snapshot()).unwrap();
    json.as_object_mut().unwrap().remove("inactive_slots");
    assert!(serde_json::from_value::<StatRecipeSnapshot>(json).is_err());
    let mut recipe = fresh.clone();
    assert!(recipe.set_item_active(255, false, out.resources).is_err());
    assert_eq!(recipe, fresh);
}
