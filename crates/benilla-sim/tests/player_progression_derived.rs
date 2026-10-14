// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic formula probes; these are not distributed Classic content rows.
use benilla_sim::{
    classic_spell::SpellFloat,
    player_progression::{derived::*, *},
    starter_init::{Attributes, ModifierSnapshot},
};
fn audit() -> Audit {
    Audit {
        group_or_raid: Evidence::Absent,
        pet_or_controlled_unit: Evidence::Absent,
        used_talents: Evidence::Absent,
        shapeshift: Evidence::Absent,
        level_change_scripts: Evidence::Absent,
        dynamic_or_level_scaling_modifiers: Evidence::Absent,
        resource_or_stat_aura_hooks: Evidence::Absent,
        unsupported_item_effects: Evidence::Absent,
        non_mana_power_modifiers: Evidence::Absent,
        extra_spell_crit_or_regen_modifiers: Evidence::Absent,
    }
}
fn input() -> DerivedInputs {
    DerivedInputs {
        weapons: vec![DerivedWeapon {
            slot: 15,
            skill: 136,
            delay_ms: 2750,
            damage: vec![
                (4.25f32.to_bits(), 7.75f32.to_bits(), 0),
                (0, 0, 0),
                (0, 0, 0),
                (0, 0, 0),
                (0, 0, 0),
            ],
            broken: false,
        }],
        audit: DerivedAudit {
            attack_power_or_damage_modifiers: DerivedEvidence::Absent,
            crit_or_avoidance_modifiers: DerivedEvidence::Absent,
            spell_damage_or_hit_modifiers: DerivedEvidence::Absent,
            regeneration_modifiers: DerivedEvidence::Absent,
            weapon_enchantments: DerivedEvidence::Absent,
            ammo: DerivedEvidence::Absent,
            shapeshift_or_disarm: DerivedEvidence::Absent,
            parry_block_or_dual_wield: DerivedEvidence::Absent,
        },
    }
}
fn snapshot(level: u32) -> Snapshot {
    let neutral = [0, 0x3f800000, 0, 0x3f800000];
    let modifiers = ModifierSnapshot {
        create: Attributes([
            17 + level as i32,
            23 + level as i32,
            29 + level as i32,
            31 + level as i32 * 2,
            37 + level as i32,
        ]),
        create_health: 41 + level * 3,
        create_mana: 59 + level * 4,
        attributes: [neutral; 5],
        health: neutral,
        mana: neutral,
        resistances: [neutral; 7],
    };
    Snapshot {
        policy: benilla_sim::player_progression::POLICY_ID.into(),
        catalog_identity: [0; 32],
        race: 1,
        class: 8,
        level,
        alive: true,
        xp: 0,
        next_level_xp: 100,
        played_time_level: 0,
        session_level: level,
        free_talent_points: level.saturating_sub(9),
        used_talent_points: 0,
        numeric: NumericState::from_modifiers(level, &modifiers, 1, [1, 0, 0, 0, 0]).unwrap(),
        modifiers,
        skills: [95, 136, 162]
            .into_iter()
            .enumerate()
            .map(|(position, id)| SkillState {
                id,
                position: position as u8,
                value: 3,
                maximum: (level * 5) as u16,
                step: 0,
                permanent_bonus: 2,
                temporary_bonus: -1,
                status: SkillStatus::Unchanged,
            })
            .collect(),
        audit: audit(),
    }
}
fn assert_bits(actual: SpellFloat, expected: f32) {
    assert_eq!(actual.bits(), expected.to_bits());
}
#[test]
fn every_hook_produces_source_fields_at_all_supported_levels() {
    for level in 1..=MAX_SUPPORTED_LEVEL {
        let s = snapshot(level);
        let out = DerivedStats::from_snapshot(&s, &input()).unwrap();
        let a = s.numeric.attributes.0.map(|x| x as f32);
        let l = level as f32;
        // Pinned Player.cpp/StatSystem.cpp expression order, native binary32 oracle.
        let rate = 12.9f32 * (60.0 - l) / 59.0 + 20.0f32 * (l - 1.0) / 59.0;
        let crit = a[1] / rate;
        let delta = (4i32 - level as i32 * 5) as f32 * 0.04f32;
        assert_eq!(out.attack_power[0].base, (a[0] - 10.0) as u32);
        assert_eq!(out.attack_power[1].base, (a[1] - 10.0) as u32);
        assert_bits(out.damage[0].minimum, (a[0] - 10.0) / 14.0 * 2.75 + 4.25);
        assert_bits(out.damage[0].maximum, (a[0] - 10.0) / 14.0 * 2.75 + 7.75);
        assert_bits(out.damage[1].minimum, 0.0);
        assert_bits(out.damage[2].minimum, (a[1] - 10.0) / 14.0 * 2.0 + 1.0);
        assert_bits(out.damage[2].maximum, (a[1] - 10.0) / 14.0 * 2.0 + 2.0);
        for field in out.crit_base_percent {
            assert_bits(field, crit);
        }
        assert_bits(out.crit_percent[0], crit);
        assert_bits(out.crit_percent[1], 0.0);
        assert_bits(out.crit_percent[2], crit);
        assert_bits(
            out.crit_display_percent[0],
            (crit + delta).clamp(0.0, 100.0),
        );
        assert_bits(
            out.crit_display_percent[1],
            (crit + -(level as i32 * 5) as f32 * 0.04f32).clamp(0.0, 100.0),
        );
        assert_bits(out.defense.dodge, 3.25f32 + crit);
        assert_bits(
            out.defense.dodge_display,
            (3.25f32 + crit + delta).clamp(0.0, 100.0),
        );
        assert_bits(out.defense.parry, 0.0);
        assert_bits(out.defense.block, 0.0);
        for field in out.spell_crit_percent {
            assert_bits(field, 3.70f32 + a[3] / (14.77f32 + 0.65f32 * l));
        }
        assert_eq!(out.spell_damage_bonus, [0; 7]);
        assert_eq!(out.weapon_hit_chance, [0; 3]);
        assert_eq!(
            out.normal_mana_regen,
            s.numeric.mana_regeneration.normal_per_second
        );
        assert_eq!(
            out.interrupted_mana_regen,
            s.numeric.mana_regeneration.interrupted_per_second
        );
    }
}
#[test]
fn defense_callback_reads_new_level_and_old_agility_only() {
    let mut view = snapshot(1);
    let mut out = DerivedStats::from_snapshot(&view, &input()).unwrap();
    let before = out.clone();
    view.level = 2;
    view.skills[0].value = 10;
    out.apply_hook(StatHook::DefenseBonuses, &view, &input())
        .unwrap();
    let expected = benilla_sim::npc_melee::mage_base_dodge(2, 24).unwrap();
    assert_eq!(out.defense.dodge, expected);
    assert_ne!(out.defense, before.defense);
    let defense = out.defense;
    out.defense = before.defense;
    assert_eq!(out, before);
    assert_ne!(
        defense.dodge,
        DerivedStats::from_snapshot(&snapshot(2), &input())
            .unwrap()
            .defense
            .dodge
    );
}
#[test]
fn broken_weapon_and_absent_weapon_have_different_source_damage_and_skill() {
    let mut s = snapshot(5);
    s.skills[2].value = 1;
    let mut inputs = input();
    let equipped = DerivedStats::from_snapshot(&s, &inputs).unwrap();
    inputs.weapons[0].broken = true;
    let broken = DerivedStats::from_snapshot(&s, &inputs).unwrap();
    assert_bits(broken.damage[0].minimum, 12.0f32 / 14.0 * 2.0);
    assert_eq!(broken.damage[0].minimum, broken.damage[0].maximum);
    assert_ne!(
        broken.crit_display_percent[0],
        equipped.crit_display_percent[0]
    );
    inputs.weapons.clear();
    let unarmed = DerivedStats::from_snapshot(&s, &inputs).unwrap();
    assert_bits(unarmed.damage[0].minimum, 12.0f32 / 14.0 * 2.0 + 1.0);
    assert_bits(unarmed.damage[0].maximum, 12.0f32 / 14.0 * 2.0 + 2.0);
    assert_eq!(
        unarmed.crit_display_percent[0],
        broken.crit_display_percent[0]
    );
}
#[test]
fn rejection_is_transactional_and_missing_dependencies_are_not_zeroed() {
    let s = snapshot(3);
    let inputs = input();
    let original = DerivedStats::from_snapshot(&s, &inputs).unwrap();
    for index in 0..8 {
        let mut inputs = input();
        let cells = [
            &mut inputs.audit.attack_power_or_damage_modifiers,
            &mut inputs.audit.crit_or_avoidance_modifiers,
            &mut inputs.audit.spell_damage_or_hit_modifiers,
            &mut inputs.audit.regeneration_modifiers,
            &mut inputs.audit.weapon_enchantments,
            &mut inputs.audit.ammo,
            &mut inputs.audit.shapeshift_or_disarm,
            &mut inputs.audit.parry_block_or_dual_wield,
        ];
        *cells.into_iter().nth(index).unwrap() = DerivedEvidence::Unknown;
        let mut out = original.clone();
        assert!(out.apply_hook(StatHook::ManaRegen, &s, &inputs).is_err());
        assert_eq!(out, original);
    }
    for mutate in [
        |i: &mut DerivedInputs| {
            i.weapons[0].damage.pop();
        },
        |i: &mut DerivedInputs| {
            i.weapons[0].slot = 17;
        },
        |i: &mut DerivedInputs| {
            i.weapons[0].damage[1] = (0, 1f32.to_bits(), 0);
        },
        |i: &mut DerivedInputs| {
            i.weapons[0].damage[0].2 = 2;
        },
        |i: &mut DerivedInputs| {
            i.weapons[0].damage[0].0 = f32::NAN.to_bits();
        },
    ] {
        let mut inputs = input();
        mutate(&mut inputs);
        assert!(DerivedStats::from_snapshot(&s, &inputs).is_err());
    }
    let mut out = original.clone();
    assert!(out
        .apply_hook(StatHook::WeaponHitChance { attack: 3 }, &s, &inputs)
        .is_err());
    assert_eq!(out, original);
    let mut s = s;
    s.numeric.attributes.0[0] = 9;
    assert!(DerivedStats::from_snapshot(&s, &inputs).is_err());
}
#[test]
fn signed_zero_damage_maxima_reject_without_mutating_cached_fields() {
    let state = snapshot(3);
    let original = DerivedStats::from_snapshot(&state, &input()).unwrap();
    for minimum in [0, 0x8000_0000] {
        for maximum in [0, 0x8000_0000] {
            let mut inputs = input();
            inputs.weapons[0].damage[0] = (minimum, maximum, 0);
            assert_eq!(
                inputs.validate(&state),
                Err(Error::Unsupported(
                    "ancillary one physical weapon damage line"
                )),
            );
            assert!(DerivedStats::from_snapshot(&state, &inputs).is_err());
            let mut actual = original.clone();
            assert!(actual
                .apply_hook(
                    StatHook::AttackPowerAndDamage { ranged: false },
                    &state,
                    &inputs,
                )
                .is_err());
            assert_eq!(actual, original);
        }
    }
}

#[test]
fn audited_empty_lists_execute_and_clear_the_actual_target_fields() {
    let s = snapshot(3);
    let inputs = input();
    let mut out = DerivedStats::from_snapshot(&s, &inputs).unwrap();
    out.spell_damage_bonus[2] = 77;
    out.weapon_hit_chance[1] = 6;
    out.apply_hook(StatHook::SpellDamageBonus, &s, &inputs)
        .unwrap();
    assert_eq!(out.spell_damage_bonus, [0; 7]);
    assert_eq!(out.weapon_hit_chance[1], 6);
    out.apply_hook(StatHook::WeaponHitChance { attack: 1 }, &s, &inputs)
        .unwrap();
    assert_eq!(out.weapon_hit_chance, [0; 3]);
}

fn recipe_case(
    level: u32,
    mode: u32,
) -> (
    Snapshot,
    DerivedInputs,
    benilla_sim::starter_init::StatRecipe,
    u8,
) {
    use benilla_sim::starter_init as i;
    let mut state = snapshot(level);
    state.skills[2].value = 1; // Both bonuses sum to +1: native unarmed value2.
    let mut inputs = input();
    if mode == 7 {
        inputs.weapons[0].delay_ms = 0;
    }
    let mut weapon = i::ItemRow {
        id: 900_002_001,
        class: 2,
        subclass: 10,
        inventory_type: 17,
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
        stats: [i::ItemStat { kind: 0, value: 0 }; 10],
        armor: 0,
        resistances: [0; 6],
        block: 0,
        spells: [i::ItemSpell { id: 0, trigger: 0 }; 5],
        damage: std::array::from_fn(|index| {
            let row = inputs.weapons[0].damage[index];
            i::ItemDamage {
                min_bits: row.0,
                max_bits: row.1,
                school: row.2,
            }
        }),
        delay_ms: inputs.weapons[0].delay_ms,
        max_durability: 12,
    };
    let kind = match mode {
        1 | 3 | 8 => 3,
        2 => 5,
        4 => 4,
        5 => 6,
        6 => 7,
        _ => 0,
    };
    weapon.stats[0] = i::ItemStat {
        kind,
        value: if matches!(mode, 0 | 7) { 0 } else { 5 },
    };
    if mode == 8 {
        weapon.stats[1] = i::ItemStat { kind: 5, value: 7 };
        weapon.stats[2] = i::ItemStat { kind: 3, value: 3 };
    }
    let mut items = vec![weapon.clone()];
    if mode == 3 {
        items[0].stats.fill(i::ItemStat { kind: 0, value: 0 });
        weapon.id += 1;
        weapon.class = 4;
        weapon.subclass = 0;
        weapon.inventory_type = 5;
        weapon.damage.fill(i::ItemDamage {
            min_bits: 0,
            max_bits: 0,
            school: 0,
        });
        weapon.delay_ms = 0;
        items.push(weapon);
    }
    let initial_input = i::StarterInput {
        race: 1,
        class: 8,
        gender: 0,
        level: 1,
        class_power_type: 0,
        create_attributes: snapshot(1).modifiers.create,
        create_health: 44,
        create_mana: 63,
        default_skills: i::Rows::Complete(vec![i::DefaultSkill { id: 136, step: 0 }]),
        default_spells: i::Rows::Complete(vec![]),
        skill_lines: vec![i::SkillLineRow {
            id: 136,
            category: 6,
        }],
        skill_race_class: i::Rows::Complete(vec![]),
        skill_tiers: vec![],
        skill_abilities: i::Rows::Complete(vec![]),
        spells: vec![],
        active_support: vec![],
        item_grants: i::Rows::Complete(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| i::ItemGrant {
                    source: i::GrantSource::Outfit,
                    source_order: index as u32,
                    item: item.id,
                    count: 1,
                })
                .collect(),
        ),
        items,
        item_creation_hooks: false,
    };
    let mage = i::initialize_mage(&initial_input).unwrap();
    let recipe = mage
        .stat_recipe
        .rebase(
            level,
            state.modifiers.create,
            state.modifiers.create_health,
            state.modifiers.create_mana,
        )
        .unwrap();
    state.modifiers = recipe.snapshot().modifiers;
    state.numeric =
        NumericState::from_modifiers(level, &state.modifiers, 1, [1, 0, 0, 0, 0]).unwrap();
    (state, inputs, recipe, if mode == 3 { 4 } else { 15 })
}
fn fixture_cells(s: &DerivedStats) -> Vec<u32> {
    let mut out = vec![
        s.attack_power[0].base,
        s.attack_power[1].base,
        s.attack_power[0].total.bits(),
        s.attack_power[1].total.bits(),
    ];
    for d in s.damage {
        out.extend([d.minimum.bits(), d.maximum.bits()]);
    }
    for fields in [
        &s.crit_base_percent[..],
        &s.crit_percent,
        &s.crit_display_percent,
        &s.spell_crit_percent,
    ] {
        out.extend(fields.iter().map(|x| x.bits()));
    }
    out.extend(
        [
            s.defense.dodge,
            s.defense.dodge_display,
            s.defense.parry,
            s.defense.parry_display,
            s.defense.block,
            s.defense.block_display,
        ]
        .map(SpellFloat::bits),
    );
    out.extend(s.spell_damage_bonus.map(|v| v as u32));
    out.extend([s.normal_mana_regen.bits(), s.interrupted_mana_regen.bits()]);
    out.extend(s.weapon_hit_chance.map(|v| v as u32));
    out
}
#[test]
fn extracted_source_ancillary_fields_and_item_callback_order() {
    use benilla_sim::starter_init::Resources;
    let mut count = 0;
    for line in include_str!("fixtures/player_derived_cmangos.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let cells: Vec<_> = line.split('\t').collect();
        let level = cells[0].parse().unwrap();
        let mode = cells[1].parse().unwrap();
        let stage: u32 = cells[2].parse().unwrap();
        let expected: Vec<u32> = cells[3].split(',').map(|v| v.parse().unwrap()).collect();
        let (mut state, mut inputs, mut recipe, slot) = recipe_case(level, mode);
        let mut out = DerivedStats::from_snapshot(&state, &inputs).unwrap();
        if stage > 0 {
            let before = inputs.clone();
            let recompute = recipe
                .set_item_broken(
                    slot,
                    true,
                    Resources {
                        health: state.numeric.health,
                        max_health: state.numeric.max_health,
                        mana: state.numeric.powers[0],
                        max_mana: state.numeric.max_powers[0],
                    },
                )
                .unwrap();
            if slot == 15 {
                inputs.weapons[0].broken = true;
            }
            out.apply_item_break(&state, &before, &inputs, slot, &recompute)
                .unwrap();
            // The trace must preserve repeated ItemStat callbacks rather than
            // collapsing a duplicate agility modifier into its final sum.
            if mode == 8 {
                assert_eq!(
                    recompute
                        .attribute_steps
                        .iter()
                        .map(|s| s.0)
                        .collect::<Vec<_>>(),
                    [1, 3, 1]
                );
            }
            if stage == 2 {
                state.modifiers = recipe.snapshot().modifiers;
                state.numeric =
                    NumericState::from_modifiers(level, &state.modifiers, 1, [1, 0, 0, 0, 0])
                        .unwrap();
                out = DerivedStats::from_snapshot(&state, &inputs).unwrap();
            }
        }
        assert_eq!(fixture_cells(&out), expected, "{line}");
        count += 1;
    }
    assert_eq!(count, 1620);
}
#[test]
fn break_witness_must_be_replayed_and_does_not_silently_refresh_caches() {
    use benilla_sim::starter_init::Resources;
    for mode in [0, 1, 3, 7, 8] {
        let (state, before, mut recipe, slot) = recipe_case(4, mode);
        let initial = DerivedStats::from_snapshot(&state, &before).unwrap();
        let recompute = recipe
            .set_item_broken(
                slot,
                true,
                Resources {
                    health: 1,
                    max_health: state.numeric.max_health,
                    mana: 1,
                    max_mana: state.numeric.max_powers[0],
                },
            )
            .unwrap();
        let mut after = before.clone();
        if slot == 15 {
            after.weapons[0].broken = true;
        }
        let mut actual = initial.clone();
        actual
            .apply_item_break(&state, &before, &after, slot, &recompute)
            .unwrap();
        if matches!(mode, 0 | 7) {
            assert_eq!(actual.crit_display_percent, initial.crit_display_percent);
        }
        if mode == 7 {
            assert_eq!(actual, initial);
        }
        let mut forged = recompute.clone();
        forged.effective_attributes.0[0] += 1;
        let mut rejected = initial.clone();
        assert!(rejected
            .apply_item_break(&state, &before, &after, slot, &forged)
            .is_err());
        assert_eq!(rejected, initial);
        // Source reducer output is suitable as an exact, strict replay witness.
        let json = serde_json::to_string(&recompute).unwrap();
        assert_eq!(
            serde_json::from_str::<benilla_sim::starter_init::EquipmentRecompute>(&json).unwrap(),
            recompute
        );
    }
}
