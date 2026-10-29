// SPDX-License-Identifier: GPL-3.0-or-later
// All identifiers/rows here are invented arithmetic cases, not game database rows.
use benilla_sim::classic_spell::SpellFloat;
use benilla_sim::creature_init::*;
fn f(value: f32) -> SpellFloat {
    SpellFloat::from_bits(value.to_bits()).unwrap()
}
fn bits(value: u32) -> SpellFloat {
    SpellFloat::from_bits(value).unwrap()
}
fn input() -> Input {
    Input {
        spawn_id: 91,
        template: Template {
            entry: 99,
            minimum_level: 1,
            maximum_level: 20,
            unit_class: 1,
            rank: 0,
            displays: [
                DisplayChoice {
                    id: 31,
                    probability: 2,
                },
                DisplayChoice {
                    id: 32,
                    probability: 3,
                },
                DisplayChoice {
                    id: 33,
                    probability: 1,
                },
                DisplayChoice {
                    id: 0,
                    probability: 0,
                },
            ],
            scale: f(1.25),
            faction: 17,
            creature_type: 1,
            type_flags: 0,
            static_flags: [0; 4],
            extra_flags: 0,
            string_id1: 0,
            string_id2: 0,
            regenerate_stats: 10,
            health_multiplier: f(1.),
            power_multiplier: f(1.),
            armor_multiplier: f(1.),
            damage_multiplier: f(1.),
            damage_variance: f(0.35),
            attribute_multipliers: [f(1.); 5],
            legacy: LegacyStats {
                minimum_health: 61,
                maximum_health: 603,
                minimum_mana: 97,
                maximum_mana: 331,
                armor: 17,
                minimum_melee_damage: f(1.75),
                maximum_melee_damage: f(5.5),
                melee_attack_power: 60,
                ranged_attack_power: 70,
            },
            mainhand_interval_ms: 2300,
            damage_school: 0,
            resistances: [0; 6],
            speed_walk: f(0.),
            speed_run: f(0.),
            corpse_delay_seconds: 0,
            equipment_template_id: 0,
            unit_flags: 0,
            npc_flags: 0,
        },
        string_ids: Rows::Complete(vec![]),
        class_levels: Rows::Complete(
            (1..=20)
                .map(|level| ClassLevelRow {
                    class: 1,
                    level,
                    base_health: 73,
                    base_mana: 137,
                    base_armor: 59,
                    base_melee_attack_power: f(100.),
                    base_ranged_attack_power: f(70.25),
                    base_damage: f(7.125),
                    attributes: [19, 23, 21, 22, 17],
                })
                .collect(),
        ),
        models: Rows::Complete(
            (31..=34)
                .map(|id| ModelRow {
                    id,
                    bounding_radius: f(0.5),
                    combat_reach: f(1.5),
                    speed_walk: f(1.),
                    speed_run: f(1.),
                    gender: u8::from(id == 34),
                    other_gender_id: match id {
                        31 => 34,
                        34 => 31,
                        _ => 0,
                    },
                })
                .collect(),
        ),
        displays: Rows::Complete(
            (31..=34)
                .map(|id| DisplayRow { id, scale: f(1.75) })
                .collect(),
        ),
        spawn_template: Rows::Complete(vec![SpawnTemplate {
            entry: 0,
            model_id: 0,
            equipment_id: -1,
            current_health: 0,
            current_mana: 0,
            faction: 0,
            npc_flags: -1,
            unit_flags: -1,
            spawn_flags: 0,
            relay_id: 0,
            string_id: 0,
        }]),
        equipment_templates: Rows::Complete(vec![]),
        item_templates: Rows::Complete(vec![]),
        spawn_addons: Rows::Complete(vec![]),
        template_addons: Rows::Complete(vec![]),
        external_features: Rows::Complete(vec![]),
        rates: Rates {
            normal_health: f(1.),
            normal_damage: f(1.),
            normal_corpse_decay_seconds: 83,
        },
    }
}
fn spawn(i: &mut Input) -> &mut SpawnTemplate {
    match &mut i.spawn_template {
        Rows::Complete(rows) => &mut rows[0],
        _ => unreachable!(),
    }
}
fn initialize(i: &Input) -> InitializedCreature {
    prepare(i)
        .unwrap()
        .resolve(SelectionRolls {
            model: Some(0),
            gender: Some(1),
            level: Some(1),
        })
        .unwrap()
}
#[test]
fn raw_loader_bonus_removal_and_creation() {
    let mut i = input();
    i.template.unit_class = 8;
    if let Rows::Complete(rows) = &mut i.class_levels {
        for row in rows {
            row.class = 8;
        }
    }
    let out = initialize(&i);
    assert_eq!(out.stats.create_mana, 87); // 137 - [20 + (22 - 20)*15]
    assert_eq!(out.stats.resources.max_mana, 137);
    assert_eq!(out.stats.base_armor, f(13.)); // 59 - 23*2
    assert_eq!(out.stats.armor, 59);
    assert_eq!(out.stats.loaded_base_ranged_attack_power, f(57.25));
    assert_eq!(out.stats.damage.attack_power, 119); // Mage adds effective STR.
    assert_eq!(out.stats.resources.health, 73);
    assert_eq!(out.model.bounding_radius, f(0.5)); // Not multiplied by Scale.
    assert_eq!(out.model.combat_reach, f(1.5));
    assert_eq!(out.model.walk_speed, f(2.5));
    assert_eq!(out.model.run_speed, f(7.));
    assert_eq!(out.entry_corpse_delay_seconds, 83);
}
#[test]
fn source_semantic_selection_draws_are_conditional_and_strict() {
    let mut i = input();
    let plan = prepare(&i).unwrap();
    assert_eq!(plan.model_roll_range(), Some((0, 5)));
    assert_eq!(plan.gender_roll_range(Some(0)).unwrap(), Some((0, 1)));
    assert_eq!(plan.gender_roll_range(Some(2)).unwrap(), None);
    assert_eq!(plan.level_roll_range(), Some((1, 20)));
    let out = plan
        .resolve(SelectionRolls {
            model: Some(0),
            gender: Some(0),
            level: Some(19),
        })
        .unwrap();
    assert_eq!(
        (out.model.display_id, out.model.gender, out.level),
        (34, 1, 19)
    );
    for rolls in [
        SelectionRolls {
            model: None,
            gender: Some(1),
            level: Some(1),
        },
        SelectionRolls {
            model: Some(2),
            gender: Some(1),
            level: Some(1),
        },
        SelectionRolls {
            model: Some(0),
            gender: None,
            level: Some(1),
        },
        SelectionRolls {
            model: Some(6),
            gender: None,
            level: Some(1),
        },
        SelectionRolls {
            model: Some(0),
            gender: Some(2),
            level: Some(1),
        },
        SelectionRolls {
            model: Some(0),
            gender: Some(1),
            level: None,
        },
        SelectionRolls {
            model: Some(0),
            gender: Some(1),
            level: Some(21),
        },
    ] {
        assert!(matches!(
            prepare(&i).unwrap().resolve(rolls),
            Err(InitError::Roll(_))
        ));
    }
    i.template.minimum_level = 7;
    i.template.maximum_level = 7;
    i.template.displays = [
        DisplayChoice {
            id: 32,
            probability: 1,
        },
        DisplayChoice {
            id: 0,
            probability: 0,
        },
        DisplayChoice {
            id: 0,
            probability: 0,
        },
        DisplayChoice {
            id: 0,
            probability: 0,
        },
    ];
    let plan = prepare(&i).unwrap();
    assert_eq!(plan.model_roll_range(), Some((0, 0)));
    assert_eq!(plan.level_roll_range(), None);
    assert_eq!(
        plan.resolve(SelectionRolls {
            model: Some(0),
            gender: None,
            level: None
        })
        .unwrap()
        .level,
        7
    );
    spawn(&mut i).model_id = 31;
    let plan = prepare(&i).unwrap();
    assert_eq!(plan.model_roll_range(), None);
    assert_eq!(
        plan.resolve(SelectionRolls {
            model: None,
            gender: Some(0),
            level: None
        })
        .unwrap()
        .model
        .display_id,
        34
    );
}
#[test]
fn spawn_overrides_clamp_and_nonregenerating_mana_zero_is_not_full() {
    let mut i = input();
    i.template.unit_class = 8;
    if let Rows::Complete(rows) = &mut i.class_levels {
        for row in rows {
            row.class = 8;
        }
    }
    let s = spawn(&mut i);
    s.current_health = 99_999;
    s.current_mana = 99_999;
    s.faction = 18;
    s.unit_flags = 16;
    s.npc_flags = 32;
    s.spawn_flags = 1;
    let out = initialize(&i);
    assert_eq!(
        out.stats.resources,
        Resources {
            health: 73,
            max_health: 73,
            mana: 137,
            max_mana: 137
        }
    );
    assert_eq!(
        (
            out.faction,
            out.unit_flags,
            out.npc_flags,
            out.starts_running
        ),
        (18, 16, 32, true)
    );
    i.template.regenerate_stats = 2;
    spawn(&mut i).current_mana = 0;
    spawn(&mut i).current_health = 17;
    assert_eq!(initialize(&i).stats.resources.mana, 0);
    assert_eq!(initialize(&i).stats.resources.health, 17);
    i.template.mainhand_interval_ms = 0;
    i.template.corpse_delay_seconds = 29;
    i.template.static_flags[1] = 0x10;
    i.template.creature_type = 8;
    i.template.type_flags = 0x40000;
    let out = initialize(&i);
    assert_eq!(out.mainhand_interval_ms, 2000);
    assert_eq!(out.entry_corpse_delay_seconds, 29);
    assert!(out.no_crushing_blows && out.creature_is_critter && !out.health_regenerates);
}
#[test]
fn model_scale_fallback_and_speed_precedence() {
    let mut i = input();
    i.template.scale = f(0.);
    i.template.speed_walk = f(0.7);
    i.template.speed_run = f(1.25);
    let out = initialize(&i);
    assert_eq!(out.model.scale, f(1.75));
    assert_eq!(out.model.bounding_radius, f(0.5));
    assert_eq!(out.model.combat_reach, f(1.5));
    assert_eq!(out.model.walk_speed, f(1.75));
    assert_eq!(out.model.run_speed, f(8.75));
}
#[test]
fn missing_rows_are_not_defaults_or_empty_capture() {
    let mut cases = Vec::new();
    let mut i = input();
    i.class_levels = Rows::Missing;
    cases.push(i);
    let mut i = input();
    i.class_levels = Rows::Complete(vec![]);
    cases.push(i);
    let mut i = input();
    if let Rows::Complete(rows) = &mut i.class_levels {
        rows.retain(|r| r.level != 20);
    }
    cases.push(i);
    let mut i = input();
    i.models = Rows::Missing;
    cases.push(i);
    let mut i = input();
    if let Rows::Complete(rows) = &mut i.models {
        rows.retain(|r| r.id != 34);
    }
    cases.push(i);
    let mut i = input();
    i.displays = Rows::Missing;
    cases.push(i);
    let mut i = input();
    if let Rows::Complete(rows) = &mut i.displays {
        rows.retain(|r| r.id != 34);
    }
    cases.push(i);
    let mut i = input();
    i.spawn_template = Rows::Missing;
    cases.push(i);
    let mut i = input();
    i.external_features = Rows::Missing;
    cases.push(i);
    for i in cases {
        assert!(matches!(prepare(&i), Err(InitError::Missing { .. })));
    }
    let mut i = input();
    i.spawn_template = Rows::Complete(vec![]);
    assert!(prepare(&i).is_err());
}
#[test]
fn unsupported_and_invalid_inputs_fail_before_selection() {
    let mut cases = Vec::new();
    for feature in [
        ExternalFeature::PetSummonOrControl,
        ExternalFeature::InstanceOrBattleground,
        ExternalFeature::FlightSwimmingTransportOrMount,
        ExternalFeature::ExistingStateOrForcedLevel,
        ExternalFeature::GameEventOrPoolSubstitution,
        ExternalFeature::ConditionalOrGroupedSpawn,
        ExternalFeature::LinkingOrAssistance,
        ExternalFeature::Addon,
        ExternalFeature::InitialAuraOrUnitModifier,
        ExternalFeature::ScriptOrAiSubstitution,
        ExternalFeature::SpellList,
        ExternalFeature::MapOrOutdoorPvpCreationHook,
        ExternalFeature::EquipmentOrLootWeapon,
        ExternalFeature::EncounterCorpseOverride,
    ] {
        let mut i = input();
        i.external_features = Rows::Complete(vec![feature]);
        cases.push(i);
    }
    for class in [0, 3, 5, 9, 255] {
        let mut i = input();
        i.template.unit_class = class;
        cases.push(i);
    }
    let mut i = input();
    i.template.rank = 1;
    cases.push(i);
    let mut i = input();
    i.template.minimum_level = 0;
    cases.push(i);
    let mut i = input();
    i.template.maximum_level = 21;
    cases.push(i);
    let mut i = input();
    i.template.minimum_level = 20;
    i.template.maximum_level = 1;
    cases.push(i);
    let mut i = input();
    i.template.damage_school = 1;
    cases.push(i);
    let mut i = input();
    i.template.mainhand_interval_ms = 60001;
    cases.push(i);
    let mut i = input();
    i.template.static_flags[0] = 8;
    cases.push(i);
    let mut i = input();
    i.template.regenerate_stats = 16;
    cases.push(i);
    let mut i = input();
    i.template.unit_flags = 8;
    cases.push(i);
    let mut i = input();
    i.template.equipment_template_id = 2;
    cases.push(i);
    let mut i = input();
    spawn(&mut i).relay_id = 1;
    cases.push(i);
    let mut i = input();
    spawn(&mut i).string_id = 1;
    cases.push(i);
    let mut i = input();
    spawn(&mut i).spawn_flags = 2;
    cases.push(i);
    let mut i = input();
    spawn(&mut i).unit_flags = -2;
    cases.push(i);
    let mut i = input();
    spawn(&mut i).unit_flags = i64::from(u32::MAX) + 1;
    cases.push(i);
    let mut i = input();
    spawn(&mut i).equipment_id = -2;
    cases.push(i);
    let mut i = input();
    spawn(&mut i).npc_flags = -2;
    cases.push(i);
    let mut i = input();
    i.template.displays[0].probability = u32::MAX;
    cases.push(i);
    let mut i = input();
    for d in &mut i.template.displays {
        d.probability = 0;
    }
    cases.push(i);
    let mut i = input();
    i.template.health_multiplier = f(-1.);
    cases.push(i);
    let mut i = input();
    i.template.power_multiplier = f(-1.);
    cases.push(i);
    let mut i = input();
    i.template.damage_multiplier = f(-1.);
    cases.push(i);
    let mut i = input();
    i.template.damage_variance = f(2.01);
    cases.push(i);
    let mut i = input();
    i.template.armor_multiplier = f(-1.);
    i.template.damage_multiplier = f(0.);
    i.template.legacy.ranged_attack_power = 1_000_001;
    cases.push(i);
    let mut i = input();
    i.rates.normal_damage = f(-1.);
    cases.push(i);
    let mut i = input();
    i.rates.normal_health = f(-1.);
    cases.push(i);
    let mut i = input();
    i.template.attribute_multipliers[0] = f(1_000_000.);
    cases.push(i);
    let mut i = input();
    if let Rows::Complete(r) = &mut i.class_levels {
        r[0].base_damage = f(0.1);
    }
    cases.push(i);
    let mut i = input();
    if let Rows::Complete(r) = &mut i.class_levels {
        r.push(r[0]);
    }
    cases.push(i);
    let mut i = input();
    if let Rows::Complete(r) = &mut i.models {
        r[0].gender = 3;
    }
    cases.push(i);
    let mut i = input();
    if let Rows::Complete(r) = &mut i.models {
        r[0].other_gender_id = 31;
    }
    cases.push(i);
    let mut i = input();
    if let Rows::Complete(r) = &mut i.models {
        r[3].speed_run = f(0.);
    }
    cases.push(i);
    let mut i = input();
    if let Rows::Complete(r) = &mut i.models {
        r.push(r[0]);
    }
    cases.push(i);
    let mut i = input();
    if let Rows::Complete(r) = &mut i.displays {
        r.push(r[0]);
    }
    cases.push(i);
    for (index, i) in cases.into_iter().enumerate() {
        assert!(prepare(&i).is_err(), "accepted invalid case {index}");
    }
    for invalid in [0x7f800000, 0x7fc00000, 1, 0xff800000] {
        assert!(SpellFloat::from_bits(invalid).is_err());
    }
}
#[test]
fn regeneration_preserves_native_five_second_single_tick_accumulator() {
    let mut i = input();
    i.template.unit_class = 8;
    if let Rows::Complete(r) = &mut i.class_levels {
        for row in r {
            row.class = 8;
        }
    }
    let out = initialize(&i);
    let before = RegenState {
        elapsed_ms: 4999,
        health: 1,
        mana: 0,
    };
    assert_eq!(regenerate_out_of_combat(&out, before, 0).unwrap(), before);
    assert_eq!(
        regenerate_out_of_combat(&out, before, 1).unwrap(),
        RegenState {
            elapsed_ms: 0,
            health: 25,
            mana: 45
        }
    );
    assert_eq!(
        regenerate_out_of_combat(&out, before, 10001).unwrap(),
        RegenState {
            elapsed_ms: 10000,
            health: 25,
            mana: 45
        }
    );
    assert!(regenerate_out_of_combat(
        &out,
        RegenState {
            elapsed_ms: 0,
            health: 0,
            mana: 0
        },
        1
    )
    .is_err());
    assert!(regenerate_out_of_combat(
        &out,
        RegenState {
            elapsed_ms: u32::MAX,
            health: 1,
            mana: 0
        },
        1
    )
    .is_err());
}
#[test]
fn pinned_cpp_differential() {
    let mut counts = [0; 7];
    for (line_number, line) in include_str!("fixtures/creature_init_cmangos.tsv")
        .lines()
        .enumerate()
    {
        let mut words = line.split_whitespace();
        let kind = words.next().unwrap();
        let n: Vec<u32> = words.map(|w| w.parse().unwrap()).collect();
        let mut i = input();
        match kind {
            "a" => {
                assert_eq!(n.len(), 11);
                counts[6] += 1;
                // Arbitrary preexisting upper-byte words are compared against
                // the private byte setter separately; this API is fresh-only.
                if n[0] != 0x1001 {
                    continue;
                }
                if n[1] != 0 {
                    i.template_addons = Rows::Complete(vec![TemplateAddonRow {
                        entry: i.template.entry,
                        addon: Addon {
                            mount: 0,
                            stand_state: 0,
                            sheath_state: n[2],
                            emote: 0,
                            move_flags: n[9],
                            auras: vec![],
                        },
                    }]);
                }
                if n[3] != 0 {
                    i.spawn_addons = Rows::Complete(vec![SpawnAddonRow {
                        guid: i.spawn_id,
                        addon: Addon {
                            mount: 0,
                            stand_state: 0,
                            sheath_state: n[4],
                            emote: 0,
                            move_flags: n[10],
                            auras: vec![],
                        },
                    }]);
                }
                assert_eq!(initialize(&i).unit_bytes_2, n[6]);
                assert_eq!(n[8], 0, "addon initialization draws no randomness");
            }
            "e" => {
                assert_eq!(n.len(), 26);
                counts[5] += 1;
                i.template.equipment_template_id = 17;
                i.template.unit_flags = n[5];
                spawn(&mut i).equipment_id = n[6] as i32;
                i.equipment_templates = Rows::Complete(vec![
                    EquipmentTemplateRow {
                        entry: 17,
                        item_entries: [501, 0, 0],
                    },
                    EquipmentTemplateRow {
                        entry: 18,
                        item_entries: [0; 3],
                    },
                ]);
                i.item_templates = Rows::Complete(vec![ItemTemplateRow {
                    entry: 501,
                    display_id: n[2],
                    class: n[0],
                    subclass: n[1],
                    material: n[3] as i32,
                    inventory_type: 13,
                    sheath: n[4],
                    delay_ms: n[7],
                    damage: [ItemDamage {
                        minimum: bits(n[8]),
                        maximum: bits(n[9]),
                        school: 6,
                    }; 5],
                }]);
                let out = initialize(&i);
                let mut fields = [0; 9];
                if let Some(item) = out.equipment.mainhand {
                    fields[0] = item.display_id;
                    fields[3..5].copy_from_slice(&item.info);
                }
                let has_weapon = out.equipment.mainhand.is_some_and(|item| item.has_weapon);
                assert_eq!(out.equipment.equipment_template_id, n[10]);
                assert_eq!(
                    fields,
                    n[11..20],
                    "source equipment fields line {}",
                    line_number + 1
                );
                assert_eq!(u32::from(has_weapon), n[20]);
                assert_eq!(
                    if has_weapon && out.unit_flags & 0x0020_0000 != 0 {
                        f(0.5)
                    } else {
                        f(1.)
                    },
                    bits(n[21])
                );
                assert_eq!(
                    [
                        out.stats.damage.attack_power,
                        out.stats.damage.minimum.bits(),
                        out.stats.damage.maximum.bits(),
                        out.mainhand_interval_ms,
                    ],
                    n[22..26],
                    "source equipped damage line {}",
                    line_number + 1
                );
            }
            "s" => {
                assert_eq!(n.len(), 49);
                counts[0] += 1;
                i.template.unit_class = n[0];
                i.template.health_multiplier = bits(n[2]);
                i.template.power_multiplier = bits(n[3]);
                i.template.armor_multiplier = bits(n[4]);
                i.template.damage_multiplier = bits(n[5]);
                i.template.damage_variance = bits(n[6]);
                i.rates.normal_health = bits(n[7]);
                i.rates.normal_damage = bits(n[8]);
                for index in 0..5 {
                    i.template.attribute_multipliers[index] = bits(n[9 + index]);
                }
                if let Rows::Complete(rows) = &mut i.class_levels {
                    for r in rows {
                        r.class = n[0];
                        r.base_health = n[14];
                        r.base_mana = n[15];
                        r.base_armor = n[16];
                        r.base_melee_attack_power = bits(n[17]);
                        r.base_ranged_attack_power = bits(n[18]);
                        r.base_damage = bits(n[19]);
                        r.attributes.copy_from_slice(&n[20..25]);
                    }
                }
                let stats = prepare(&i)
                    .unwrap_or_else(|e| panic!("prepare line {}: {e}", line_number + 1))
                    .resolve(SelectionRolls {
                        model: Some(0),
                        gender: Some(1),
                        level: Some(n[1]),
                    })
                    .unwrap()
                    .stats;
                let mut actual = Vec::new();
                actual.extend(stats.create_attributes.map(SpellFloat::bits));
                actual.extend(stats.attributes);
                actual.extend([
                    stats.create_health,
                    stats.create_mana,
                    stats.base_armor.bits(),
                    stats.armor as u32,
                    stats.base_melee_attack_power.bits(),
                    stats.loaded_base_ranged_attack_power.bits(),
                    stats.damage_multiplier.bits(),
                    stats.base_weapon_minimum.bits(),
                    stats.base_weapon_maximum.bits(),
                    stats.damage.attack_power,
                    stats.damage.minimum.bits(),
                    stats.damage.maximum.bits(),
                    stats.resources.max_health,
                    stats.resources.max_mana,
                ]);
                assert_eq!(actual, &n[25..], "source stats line {}", line_number + 1);
            }
            "m" => {
                assert_eq!(n.len(), 5);
                counts[1] += 1;
                let plan = prepare(&i).unwrap();
                let gender = plan.gender_roll_range(Some(n[0])).unwrap().map(|_| n[1]);
                let count = 1 + u32::from(gender.is_some());
                let out = plan
                    .resolve(SelectionRolls {
                        model: Some(n[0]),
                        gender,
                        level: Some(1),
                    })
                    .unwrap();
                assert_eq!(
                    (out.model.display_id, count),
                    (n[3], n[4]),
                    "source model line {}",
                    line_number + 1
                );
            }
            "v" => {
                assert_eq!(n.len(), 6);
                counts[2] += 1;
                i.template.speed_walk = bits(n[0]);
                i.template.speed_run = bits(n[1]);
                if let Rows::Complete(rows) = &mut i.models {
                    for row in rows {
                        row.speed_walk = bits(n[2]);
                        row.speed_run = bits(n[3]);
                    }
                }
                let model = initialize(&i).model;
                assert_eq!(
                    (model.walk_speed.bits(), model.run_speed.bits()),
                    (n[4], n[5]),
                    "source speed line {}",
                    line_number + 1
                );
            }
            "r" => {
                assert_eq!(n.len(), 8);
                counts[3] += 1;
                let mut profile = initialize(&i);
                profile.power_type = PowerType::Mana;
                profile.stats.resources.max_health = n[0];
                profile.stats.resources.max_mana = n[1];
                profile.health_regenerates = n[4] & 1 != 0;
                profile.power_regenerates_out_of_combat = n[4] & 2 != 0;
                let after = regenerate_out_of_combat(
                    &profile,
                    RegenState {
                        elapsed_ms: n[2],
                        health: 1,
                        mana: n[1] / 2,
                    },
                    n[3],
                )
                .unwrap();
                assert_eq!(
                    (after.health, after.mana, after.elapsed_ms),
                    (n[5], n[6], n[7]),
                    "source regen line {}",
                    line_number + 1
                );
            }
            "t" => {
                assert_eq!(n.len(), 7);
                counts[4] += 1;
                let timing = SpawnTimingInputs {
                    database_guid: 91,
                    runtime_guid_low: n[3],
                    minimum_respawn_seconds: n[0],
                    maximum_respawn_seconds: n[1],
                }
                .resolve(n[2], n[4])
                .unwrap();
                assert_eq!(
                    (timing.respawn_delay_seconds, timing.corpse_delay_seconds),
                    (n[5], n[6]),
                    "source timing line {}",
                    line_number + 1
                );
            }
            _ => panic!("unknown fixture row"),
        }
    }
    assert_eq!(counts, [1057, 12, 1225, 1536, 256, 192, 448]);
}

#[test]
fn spawn_timing_is_a_separate_always_consumed_draw_and_caps_legacy_corpses() {
    let legacy = SpawnTimingInputs {
        database_guid: 91,
        runtime_guid_low: 91,
        minimum_respawn_seconds: 50,
        maximum_respawn_seconds: 10,
    };
    assert_eq!(legacy.respawn_roll_range().unwrap(), (50, 50));
    assert_eq!(
        legacy.resolve(83, 50).unwrap(),
        SpawnTiming {
            respawn_delay_seconds: 50,
            corpse_delay_seconds: 45
        }
    );
    assert!(legacy.resolve(83, 49).is_err());
    let dynamic = SpawnTimingInputs {
        runtime_guid_low: 92,
        ..legacy
    };
    assert_eq!(dynamic.resolve(83, 50).unwrap().corpse_delay_seconds, 83);
    assert!(SpawnTimingInputs {
        database_guid: 0,
        ..legacy
    }
    .respawn_roll_range()
    .is_err());
    assert!(SpawnTimingInputs {
        minimum_respawn_seconds: u32::MAX,
        ..legacy
    }
    .respawn_roll_range()
    .is_err());
}
#[test]
fn signed_resistances_and_transient_ap_casts_are_validated() {
    let mut i = input();
    i.template.resistances = [-3, 7, 11, 0, 19, 1];
    assert_eq!(initialize(&i).stats.resistances, [59, -3, 7, 11, 0, 19, 1]);
    i.template.resistances[0] = i32::MIN;
    assert!(prepare(&i).is_err());
    i = input();
    i.template.unit_class = 4;
    if let Rows::Complete(rows) = &mut i.class_levels {
        for r in rows {
            r.class = 4;
            r.base_melee_attack_power = f(1.);
        }
    }
    assert!(prepare(&i).is_err()); // First STR update sees agility=0, causing negative uint AP.
    i = input();
    i.template.attribute_multipliers[1] = f(1.025);
    if let Rows::Complete(rows) = &mut i.class_levels {
        for r in rows {
            r.base_ranged_attack_power = f(0.);
        }
    }
    assert!(prepare(&i).is_ok()); // Fractional negative ranged AP truncates to zero, which is defined.
    let mut p = initialize(&input());
    p.stats.resources.max_health = u32::MAX;
    assert!(regenerate_out_of_combat(
        &p,
        RegenState {
            elapsed_ms: 5000,
            health: 1,
            mana: 0
        },
        0
    )
    .is_err());
}

#[test]
fn native_speed_floor_and_ambiguous_radius_repair() {
    let mut i = input();
    for rate in [0.001, 0.009, 0.01, 0.011] {
        i.template.speed_walk = f(rate);
        i.template.speed_run = f(rate);
        let model = initialize(&i).model;
        assert_eq!(model.walk_speed, f(rate.max(0.01) * 2.5));
        assert_eq!(model.run_speed, f(rate.max(0.01) * 7.0));
    }
    i.template.speed_walk = f(0.);
    i.template.speed_run = f(0.);
    if let Rows::Complete(rows) = &mut i.models {
        for row in rows {
            row.speed_walk = f(0.001);
            row.speed_run = f(0.001);
        }
    }
    let model = initialize(&i).model;
    assert_eq!(model.walk_speed, f(0.01 * 2.5));
    assert_eq!(model.run_speed, f(0.01 * 7.0));
    if let Rows::Complete(rows) = &mut i.models {
        rows[0].bounding_radius = f(0.);
    }
    assert!(prepare(&i).is_err());
}

fn model_receipt(i: &Input) -> ModelSelectionReceipt {
    let plan = prepare(i).unwrap();
    let model = plan.model_roll_range().map(|(low, _)| low);
    let gender = plan.gender_roll_range(model).unwrap().map(|_| 0);
    let level = plan.level_roll_range().map(|(low, _)| low);
    plan.resolve_with_model_receipt(SelectionRolls {
        model,
        gender,
        level,
    })
    .unwrap()
    .1
}

#[test]
fn initial_model_receipt_retains_source_projection_and_draw_order() {
    let i = input();
    let rolls = SelectionRolls {
        model: Some(0),
        gender: Some(0),
        level: Some(1),
    };
    let original = prepare(&i).unwrap().resolve(rolls).unwrap();
    let (mut result, receipt) = prepare(&i)
        .unwrap()
        .resolve_with_model_receipt(rolls)
        .unwrap();
    assert_eq!(result, original);
    assert_eq!(receipt.policy(), CREATURE_MODEL_SELECTION_POLICY_ID);
    assert_eq!(receipt.initialization_policy(), CREATURE_INIT_POLICY_ID);
    assert_eq!(receipt.entry(), 99);
    assert_eq!(receipt.template_displays(), &i.template.displays);
    assert_eq!(receipt.raw_template_scale(), f(1.25));
    assert_eq!(receipt.spawn_template_entry(), 0);
    assert_eq!(receipt.spawn_model_id(), 0);
    assert_eq!(receipt.original_display_id(), 31);
    assert_eq!(receipt.native_display_id(), 34);
    assert_eq!(receipt.gender(), 1);
    assert_eq!(receipt.object_scale(), f(1.25));
    assert_eq!(receipt.bounding_radius(), f(0.5));
    assert_eq!(receipt.combat_reach(), f(1.5));
    assert_eq!(
        receipt
            .display_scales()
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        vec![31, 32, 33, 34]
    );
    assert_eq!(receipt.gender_rows()[0].display_id(), 31);
    assert_eq!(receipt.gender_rows()[0].gender(), 0);
    assert_eq!(receipt.gender_rows()[0].other_gender_id(), 34);
    let draws = receipt.draws().map(|d| (d.purpose(), d.range(), d.value()));
    assert_eq!(
        draws,
        [
            (ModelSelectionDrawPurpose::Model, Some((0, 5)), Some(0)),
            (ModelSelectionDrawPurpose::Gender, Some((0, 1)), Some(0)),
            (ModelSelectionDrawPurpose::Level, Some((1, 20)), Some(1)),
        ]
    );
    // Public arithmetic outputs cannot rewrite the immutable selected native model.
    result.model.display_id = 999;
    result.model.scale = f(99.);
    assert_ne!(result.model.display_id, receipt.native_display_id());
    assert_ne!(result.model.scale, receipt.object_scale());
    assert_eq!(receipt.native_display_id(), 34);
    assert_eq!(receipt.object_scale(), f(1.25));
    let serialized = serde_json::to_value(&receipt).unwrap();
    assert!(serialized.get("stats").is_none());
    assert!(serialized.get("levels").is_none());
    assert_eq!(serialized["native_display_id"], 34);
    assert_eq!(serialized["object_scale"], f(1.25).bits());
}

#[test]
fn initial_model_receipt_preserves_overrides_fallback_scale_and_absent_draws() {
    let mut i = input();
    i.template.minimum_level = 1;
    i.template.maximum_level = 1;
    i.template.scale = f(-1.);
    spawn(&mut i).entry = 7;
    spawn(&mut i).model_id = 32;
    if let Rows::Complete(rows) = &mut i.displays {
        rows.iter_mut().find(|r| r.id == 31).unwrap().scale = f(2.);
        rows.iter_mut().find(|r| r.id == 32).unwrap().scale = f(3.);
        rows.reverse();
    }
    let receipt = model_receipt(&i);
    assert_eq!(receipt.spawn_template_entry(), 7);
    assert_eq!(receipt.spawn_model_id(), 32);
    assert_eq!(receipt.original_display_id(), 32);
    assert_eq!(receipt.native_display_id(), 32);
    assert_eq!(receipt.raw_template_scale(), f(-1.));
    assert_eq!(receipt.object_scale(), f(2.)); // First template display, not selected override.
    assert!(receipt
        .draws()
        .iter()
        .all(|draw| draw.range().is_none() && draw.value().is_none()));
    assert_eq!(receipt.display_scales()[0].id, 31);
    let old = receipt.clone();
    i.template.legacy.armor += 1; // Width receipt binds a model projection, not unrelated stats.
    assert_eq!(model_receipt(&i), old);
    i.template.displays[0].probability += 1;
    assert_ne!(model_receipt(&i), old); // Retain raw choices even under a spawn override.
}

#[test]
fn initial_model_receipt_requires_valid_full_plan_and_exact_draw_presence() {
    let mut i = input();
    i.template.displays[1] = DisplayChoice {
        id: 0,
        probability: 0,
    };
    i.template.displays[2] = DisplayChoice {
        id: 0,
        probability: 0,
    };
    i.template.displays[0].probability = 1;
    let receipt = model_receipt(&i);
    assert_eq!(receipt.draws()[0].range(), Some((0, 0)));
    assert_eq!(receipt.draws()[0].value(), Some(0));
    assert!(prepare(&i)
        .unwrap()
        .resolve_with_model_receipt(SelectionRolls {
            model: None,
            gender: Some(0),
            level: Some(1),
        })
        .is_err());
    assert!(prepare(&i)
        .unwrap()
        .resolve_with_model_receipt(SelectionRolls {
            model: Some(0),
            gender: None,
            level: Some(1),
        })
        .is_err());
    i.class_levels = Rows::Missing;
    assert!(prepare(&i).is_err());
    i = input();
    i.external_features = Rows::Missing;
    assert!(prepare(&i).is_err());
    i.external_features = Rows::Complete(vec![ExternalFeature::InitialAuraOrUnitModifier]);
    assert!(prepare(&i).is_err());
}

#[test]
fn initial_width_checks_identity_and_only_exact_zero_selects_default() {
    let receipt = model_receipt(&input());
    assert!(receipt.collision_width(31, 51, f(1.)).is_err());
    assert!(receipt.collision_width(34, 0, f(1.)).is_err());
    for raw in [f(0.), f(-0.)] {
        let width = receipt.collision_width(34, 51, raw).unwrap();
        assert_eq!(width.policy(), CREATURE_COLLISION_WIDTH_POLICY_ID);
        assert_eq!(width.native_display_id(), 34);
        assert_eq!(width.model_id(), 51);
        assert_eq!(width.object_scale(), f(1.25));
        assert_eq!(width.raw_width(), raw);
        assert_eq!(width.width(), bits(0x3f02_7bb3));
    }
    assert_eq!(
        receipt.collision_width(34, 51, f(-1.)).unwrap().width(),
        f(-1.25)
    );
    assert_eq!(
        receipt.collision_width(34, 51, f(0.5)).unwrap().width(),
        f(0.625)
    );
    assert!(receipt.collision_width(34, 51, f(1_000_000.)).is_err());
}

#[test]
fn initial_width_matches_unchanged_source_fixture_and_refuses_numeric_outside_domain() {
    let mut i = input();
    i.template.maximum_level = 1;
    let mut count = 0;
    let mut rejected = 0;
    for line in include_str!("fixtures/creature_collision_width_cmangos.test-fixture").lines() {
        let row: Vec<u32> = line
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(row.len(), 4);
        i.template.scale = bits(row[0]);
        let receipt = model_receipt(&i);
        let result = receipt.collision_width(receipt.native_display_id(), 51, bits(row[1]));
        let magnitude = row[2] & 0x7fff_ffff;
        if SpellFloat::from_bits(row[2]).is_err() || (magnitude == 0 && row[1] & 0x7fff_ffff != 0) {
            assert!(result.is_err(), "out-of-domain source product: {line}");
            rejected += 1;
        } else {
            assert_eq!(result.unwrap().width().bits(), row[3], "{line}");
        }
        count += 1;
    }
    assert_eq!(count, 2295);
    assert!(rejected > 0);
}

#[test]
fn initial_height_checks_native_identity_and_only_exact_zero_selects_default() {
    let receipt = model_receipt(&input());
    assert_eq!(receipt.original_display_id(), 31);
    assert_eq!(receipt.native_display_id(), 34);
    assert!(receipt.collision_height(31, 51, f(1.)).is_err());
    assert!(receipt.collision_height(34, 0, f(1.)).is_err());
    for raw in [f(0.), f(-0.)] {
        let height = receipt.collision_height(34, 51, raw).unwrap();
        assert_eq!(height.policy(), CREATURE_COLLISION_HEIGHT_POLICY_ID);
        assert_eq!(height.native_display_id(), 34);
        assert_eq!(height.model_id(), 51);
        assert_eq!(height.object_scale(), f(1.25));
        assert_eq!(height.raw_height(), raw);
        assert_eq!(height.height(), bits(0x4002_007e));
        let serialized = serde_json::to_value(height).unwrap();
        assert_eq!(serialized["native_display_id"], 34);
        assert_eq!(serialized["raw_height"], raw.bits());
        assert_eq!(serialized["height"], 0x4002_007eu32);
    }
    assert_eq!(
        receipt.collision_height(34, 51, f(-1.)).unwrap().height(),
        f(-1.25)
    );
    assert_eq!(
        receipt.collision_height(34, 51, f(2.)).unwrap().height(),
        f(2.5)
    );
    assert!(receipt.collision_height(34, 51, f(1_000_000.)).is_err());
    // SQL geometry and caller-mutable output are not height inputs.
    assert_eq!(receipt.bounding_radius(), f(0.5));
    assert_eq!(receipt.combat_reach(), f(1.5));
}

#[test]
fn initial_height_preserves_positive_effective_scale_domain_and_template_fallback() {
    let mut i = input();
    if let Rows::Complete(rows) = &mut i.displays {
        rows.iter_mut().find(|row| row.id == 31).unwrap().scale = f(2.);
        rows.iter_mut().find(|row| row.id == 34).unwrap().scale = f(7.);
    }
    for raw_template_scale in [f(0.), f(-0.), f(-1.)] {
        i.template.scale = raw_template_scale;
        let receipt = model_receipt(&i);
        assert_eq!(receipt.raw_template_scale(), raw_template_scale);
        assert_eq!(receipt.object_scale(), f(2.));
        assert_eq!(receipt.native_display_id(), 34);
        // First actual template display sets initial scale, not selected gender.
        let height = receipt.collision_height(34, 51, f(3.)).unwrap();
        assert_eq!(height.height(), f(6.));
    }
    for invalid_scale in [f(0.), f(-0.), f(-1.)] {
        let mut invalid = i.clone();
        invalid.template.scale = f(1.);
        if let Rows::Complete(rows) = &mut invalid.displays {
            rows[0].scale = invalid_scale;
        }
        assert!(prepare(&invalid).is_err());
    }
    for raw in [
        1,
        0x007f_ffff,
        0x4974_2401,
        0x7f7f_ffff,
        0x7f80_0000,
        0x7fc0_0000,
    ] {
        assert!(SpellFloat::from_bits(raw).is_err());
        assert!(SpellFloat::from_bits(raw | 0x8000_0000).is_err());
    }
}

#[test]
fn initial_height_matches_unchanged_source_fixture_and_refuses_numeric_outside_domain() {
    let mut i = input();
    i.template.maximum_level = 1;
    let mut count = 0;
    let mut refused_input = 0;
    let mut refused_product = 0;
    let mut source_overflow = 0;
    let mut exact_zero_default = 0;
    let mut underflow_zero = 0;
    for line in include_str!("fixtures/creature_collision_height_cmangos.test-fixture").lines() {
        let row: Vec<u32> = line
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(row.len(), 4);
        i.template.scale = bits(row[0]);
        let receipt = model_receipt(&i);
        assert_eq!(receipt.object_scale().bits(), row[0]);
        let magnitude = row[2] & 0x7fff_ffff;
        match SpellFloat::from_bits(row[1]) {
            Err(_) => {
                // These source probes cannot enter the public arithmetic API.
                refused_input += 1;
                if magnitude == 0x7f80_0000 {
                    source_overflow += 1;
                }
            }
            Ok(raw_height) => {
                let result = receipt.collision_height(receipt.native_display_id(), 51, raw_height);
                let rounded_to_zero = magnitude == 0 && row[1] & 0x7fff_ffff != 0;
                if SpellFloat::from_bits(row[2]).is_err() || rounded_to_zero {
                    assert!(result.is_err(), "out-of-domain source product: {line}");
                    refused_product += 1;
                    if rounded_to_zero {
                        assert_eq!(row[3], 0x4002_007e);
                        underflow_zero += 1;
                    }
                } else {
                    let result = result.unwrap();
                    assert_eq!(result.height().bits(), row[3], "{line}");
                    assert_eq!(result.raw_height().bits(), row[1]);
                    assert_eq!(result.object_scale().bits(), row[0]);
                    if magnitude == 0 {
                        assert_eq!(row[3], 0x4002_007e);
                        exact_zero_default += 1;
                    }
                }
            }
        }
        count += 1;
    }
    assert_eq!(count, 2408);
    assert_eq!(refused_input, 91);
    assert!(refused_product > 0);
    assert!(source_overflow > 0);
    assert!(underflow_zero > 0);
    assert_eq!(exact_zero_default, 26);
}

#[path = "creature_init/equipment.rs"]
mod equipment;

#[path = "creature_init/addon.rs"]
mod addon;

#[test]
fn required_string_catalog_normalizes_loaded_pair_without_losing_raw_identity() {
    let mut i = input();
    i.template.string_id1 = 9;
    i.template.string_id2 = 7;
    i.string_ids = Rows::Missing;
    assert!(matches!(
        prepare(&i),
        Err(InitError::Missing {
            table: "string_id complete cold catalog",
            ..
        })
    ));
    i.template.string_id1 = 0;
    i.template.string_id2 = 0;
    assert!(matches!(prepare(&i), Err(InitError::Missing { .. })));
    i.template.string_id1 = 9;
    i.template.string_id2 = 7;
    i.string_ids = Rows::Complete(vec![]);
    let resolve = |i: &Input| {
        prepare(i)
            .unwrap()
            .resolve(SelectionRolls {
                model: Some(0),
                gender: Some(1),
                level: Some(1),
            })
            .unwrap()
    };
    assert_eq!(resolve(&i).effective_string_ids, [0, 0]);
    i.string_ids = Rows::Complete(vec![
        StringIdRow {
            id: 9,
            name: "".into(),
        },
        StringIdRow {
            id: 9,
            name: "later duplicate key".into(),
        },
        StringIdRow {
            id: 5,
            name: "same name".into(),
        },
        StringIdRow {
            id: 7,
            name: "same name".into(),
        },
    ]);
    assert_eq!(resolve(&i).effective_string_ids, [9, 7]);
    let raw = serde_json::to_vec(&i).unwrap();
    let Rows::Complete(rows) = &mut i.string_ids else {
        unreachable!()
    };
    rows.reverse();
    rows[0].name = "任意名".into();
    assert_ne!(serde_json::to_vec(&i).unwrap(), raw);
    assert_eq!(resolve(&i).effective_string_ids, [9, 7]);
    i.template.string_id2 = 9;
    assert_eq!(resolve(&i).effective_string_ids, [9, 9]);
    i.template.string_id2 = 1234;
    assert_eq!(resolve(&i).effective_string_ids, [9, 0]);
}

#[test]
fn string_catalog_domain_and_retention_bounds_precede_preparation() {
    let mut i = input();
    i.template.string_id1 = i32::MAX as u32;
    i.string_ids = Rows::Complete(vec![StringIdRow {
        id: i32::MAX,
        name: String::new(),
    }]);
    assert!(prepare(&i).is_ok());
    i.template.string_id1 = 0x8000_0000;
    assert!(matches!(
        prepare(&i),
        Err(InitError::Unsupported("positive signed string ID domain"))
    ));
    i.template.string_id1 = 0;
    for id in [0, -1, i32::MIN] {
        i.string_ids = Rows::Complete(vec![StringIdRow {
            id,
            name: "retained".into(),
        }]);
        assert!(matches!(
            prepare(&i),
            Err(InitError::Unsupported("positive signed string ID domain"))
        ));
    }
    i.string_ids = Rows::Complete(vec![StringIdRow {
        id: 1,
        name: "x".repeat(MAX_CREATURE_INIT_SOURCE_BYTES),
    }]);
    assert_eq!(
        i.validate_retention_bounds(),
        Err(InitError::Invalid("source retention byte bound"))
    );
    i.string_ids = Rows::Complete(vec![
        StringIdRow {
            id: 1,
            name: String::new()
        };
        MAX_CREATURE_INIT_SOURCE_ROWS + 1
    ]);
    assert_eq!(
        i.validate_retention_bounds(),
        Err(InitError::Invalid("source retention row bound"))
    );
}
