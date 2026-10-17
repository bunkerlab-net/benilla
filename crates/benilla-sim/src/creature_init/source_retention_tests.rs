// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic source identity/retention cases, not database rows or a live RNG run.
use super::*;
use serde_json::{json, Value};

fn rows_mut<T>(rows: &mut Rows<T>) -> &mut Vec<T> {
    match rows {
        Rows::Complete(rows) => rows,
        Rows::Missing => panic!("test needs complete rows"),
    }
}
fn f(value: f32) -> SpellFloat {
    SpellFloat::from_bits(value.to_bits()).unwrap()
}
fn input() -> Input {
    Input {
        template: Template {
            entry: 99,
            minimum_level: 1,
            maximum_level: 1,
            unit_class: 1,
            rank: 0,
            displays: [
                DisplayChoice {
                    id: 11,
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
            ],
            scale: SpellFloat::ONE,
            faction: 17,
            creature_type: 1,
            type_flags: 0,
            static_flags: [0; 4],
            regenerate_stats: 10,
            health_multiplier: SpellFloat::ONE,
            power_multiplier: SpellFloat::ONE,
            armor_multiplier: SpellFloat::ONE,
            damage_multiplier: SpellFloat::ONE,
            damage_variance: SpellFloat::ZERO,
            attribute_multipliers: [SpellFloat::ONE; 5],
            legacy: LegacyStats {
                minimum_health: 61,
                maximum_health: 603,
                minimum_mana: 97,
                maximum_mana: 331,
                armor: 17,
                minimum_melee_damage: SpellFloat::ONE,
                maximum_melee_damage: f(2.),
                melee_attack_power: 60,
                ranged_attack_power: 70,
            },
            mainhand_interval_ms: 2300,
            damage_school: 0,
            resistances: [-1, 2, -3, 4, -5, 6],
            speed_walk: SpellFloat::ZERO,
            speed_run: SpellFloat::ZERO,
            corpse_delay_seconds: 0,
            equipment_template_id: 0,
            unit_flags: 0,
            npc_flags: 0,
        },
        class_levels: Rows::Complete(
            (1..=2)
                .map(|level| ClassLevelRow {
                    class: 1,
                    level,
                    base_health: 73,
                    base_mana: 137,
                    base_armor: 59,
                    base_melee_attack_power: f(100.),
                    base_ranged_attack_power: f(70.),
                    base_damage: f(7.),
                    attributes: [19, 23, 21, 22, 17],
                })
                .collect(),
        ),
        models: Rows::Complete(
            (11..=12)
                .map(|id| ModelRow {
                    id,
                    bounding_radius: SpellFloat::ONE,
                    combat_reach: SpellFloat::ONE,
                    speed_walk: SpellFloat::ONE,
                    speed_run: SpellFloat::ONE,
                    gender: 0,
                    other_gender_id: 0,
                })
                .collect(),
        ),
        displays: Rows::Complete(
            (11..=12)
                .map(|id| DisplayRow {
                    id,
                    scale: SpellFloat::ONE,
                })
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
        external_features: Rows::Complete(vec![]),
        rates: Rates {
            normal_health: SpellFloat::ONE,
            normal_damage: SpellFloat::ONE,
            normal_corpse_decay_seconds: 83,
        },
    }
}
fn rolls() -> SelectionRolls {
    SelectionRolls {
        model: Some(0),
        gender: None,
        level: None,
    }
}
fn content(input: &Input) -> Vec<u8> {
    input.validate_retention_bounds().unwrap();
    serde_json::to_vec(input).unwrap()
}

#[test]
fn serialization_retains_every_raw_field_and_original_rows() {
    let input = input();
    let one = SpellFloat::ONE.bits();
    let classes: Vec<Value> = (1..=2)
        .map(|level| {
            json!({
                "class": 1,
                "level": level,
                "base_health": 73,
                "base_mana": 137,
                "base_armor": 59,
                "base_melee_attack_power": f(100.).bits(),
                "base_ranged_attack_power": f(70.).bits(),
                "base_damage": f(7.).bits(),
                "attributes": [19, 23, 21, 22, 17],
            })
        })
        .collect();
    let models: Vec<Value> = (11..=12)
        .map(|id| {
            json!({
                "id": id,
                "bounding_radius": one,
                "combat_reach": one,
                "speed_walk": one,
                "speed_run": one,
                "gender": 0,
                "other_gender_id": 0,
            })
        })
        .collect();
    let expected = json!({
        "template": {
            "entry": 99,
            "minimum_level": 1,
            "maximum_level": 1,
            "unit_class": 1,
            "rank": 0,
            "displays": [
                {"id": 11, "probability": 1},
                {"id": 0, "probability": 0},
                {"id": 0, "probability": 0},
                {"id": 0, "probability": 0},
            ],
            "scale": one,
            "faction": 17,
            "creature_type": 1,
            "type_flags": 0,
            "static_flags": [0, 0, 0, 0],
            "regenerate_stats": 10,
            "health_multiplier": one,
            "power_multiplier": one,
            "armor_multiplier": one,
            "damage_multiplier": one,
            "damage_variance": 0,
            "attribute_multipliers": [one, one, one, one, one],
            "legacy": {
                "minimum_health": 61,
                "maximum_health": 603,
                "minimum_mana": 97,
                "maximum_mana": 331,
                "armor": 17,
                "minimum_melee_damage": one,
                "maximum_melee_damage": f(2.).bits(),
                "melee_attack_power": 60,
                "ranged_attack_power": 70,
            },
            "mainhand_interval_ms": 2300,
            "damage_school": 0,
            "resistances": [-1, 2, -3, 4, -5, 6],
            "speed_walk": 0,
            "speed_run": 0,
            "corpse_delay_seconds": 0,
            "equipment_template_id": 0,
            "unit_flags": 0,
            "npc_flags": 0,
        },
        "class_levels": {"Complete": classes},
        "models": {"Complete": models},
        "displays": {"Complete": [{"id": 11, "scale": one}, {"id": 12, "scale": one}]},
        "spawn_template": {"Complete": [{
            "entry": 0,
            "model_id": 0,
            "equipment_id": -1,
            "current_health": 0,
            "current_mana": 0,
            "faction": 0,
            "npc_flags": -1,
            "unit_flags": -1,
            "spawn_flags": 0,
            "relay_id": 0,
            "string_id": 0,
        }]},
        "external_features": {"Complete": []},
        "rates": {"normal_health": one, "normal_damage": one, "normal_corpse_decay_seconds": 83},
    });
    assert_eq!(serde_json::to_value(&input).unwrap(), expected);
}

fn assert_same_resolution_different_source(change: impl FnOnce(&mut Input)) {
    let original = input();
    let mut changed = original.clone();
    change(&mut changed);
    assert_ne!(content(&original), content(&changed));
    assert_eq!(
        prepare(&original)
            .unwrap()
            .resolve_with_model_receipt(rolls())
            .unwrap(),
        prepare(&changed)
            .unwrap()
            .resolve_with_model_receipt(rolls())
            .unwrap(),
    );
}

#[test]
fn unselected_unused_and_equivalent_raw_inputs_remain_distinct() {
    assert_same_resolution_different_source(|i| i.template.legacy.minimum_health += 1);
    assert_same_resolution_different_source(|i| {
        i.template.speed_walk = SpellFloat::from_bits(0x8000_0000).unwrap();
    });
    assert_same_resolution_different_source(|i| {
        rows_mut(&mut i.spawn_template)[0].current_health = MAX_AMOUNT;
    });
    assert_same_resolution_different_source(|i| {
        rows_mut(&mut i.class_levels)[1].base_health += 1;
    });
    assert_same_resolution_different_source(|i| {
        rows_mut(&mut i.models)[1].bounding_radius = f(2.);
    });
    assert_same_resolution_different_source(|i| {
        rows_mut(&mut i.displays)[1].scale = f(2.);
    });
    assert_same_resolution_different_source(|i| {
        let rows = rows_mut(&mut i.class_levels);
        let mut row = rows[1];
        row.level = 3;
        rows.push(row);
    });
    assert_same_resolution_different_source(|i| {
        let rows = rows_mut(&mut i.models);
        let mut row = rows[1];
        row.id = 13;
        rows.push(row);
    });
    assert_same_resolution_different_source(|i| {
        rows_mut(&mut i.displays).push(DisplayRow {
            id: 13,
            scale: SpellFloat::ONE,
        });
    });
    assert_same_resolution_different_source(|i| rows_mut(&mut i.class_levels).reverse());
    assert_same_resolution_different_source(|i| rows_mut(&mut i.models).reverse());
    assert_same_resolution_different_source(|i| rows_mut(&mut i.displays).reverse());
}

#[test]
fn complete_unsupported_rows_preserve_values_and_order_without_becoming_admission() {
    let mut input = input();
    input.external_features = Rows::Complete(vec![
        ExternalFeature::Addon,
        ExternalFeature::InitialAuraOrUnitModifier,
    ]);
    let first = content(&input);
    rows_mut(&mut input.external_features).reverse();
    assert_ne!(first, content(&input));
    assert!(matches!(prepare(&input), Err(InitError::Unsupported(_))));
    input.external_features = Rows::Complete(vec![]);
    let spawn_rows = rows_mut(&mut input.spawn_template);
    let mut second = spawn_rows[0];
    second.entry = 1;
    second.string_id = 2;
    spawn_rows.push(second);
    let first = content(&input);
    rows_mut(&mut input.spawn_template).reverse();
    assert_ne!(first, content(&input));
    assert!(matches!(prepare(&input), Err(InitError::Invalid(_))));
}

#[test]
fn missing_and_captured_empty_are_distinct_for_every_source_collection() {
    macro_rules! check {
        ($field:ident) => {{
            let mut missing = input();
            missing.$field = Rows::Missing;
            let mut empty = missing.clone();
            empty.$field = Rows::Complete(vec![]);
            assert_ne!(content(&missing), content(&empty));
            assert_eq!(serde_json::to_value(&missing.$field).unwrap(), "Missing");
            assert_eq!(
                serde_json::to_value(&empty.$field).unwrap(),
                json!({"Complete": []}),
            );
            assert!(matches!(prepare(&missing), Err(InitError::Missing { .. })));
        }};
    }
    check!(class_levels);
    check!(models);
    check!(displays);
    check!(spawn_template);
    check!(external_features);
    let mut empty_features = input();
    empty_features.external_features = Rows::Complete(vec![]);
    assert!(prepare(&empty_features).is_ok());
}

#[test]
fn source_row_budget_is_aggregate_and_precedes_validation_and_arithmetic() {
    macro_rules! check {
        ($field:ident, $row:expr) => {{
            let mut oversized = input();
            oversized.template.entry = 0;
            oversized.$field = Rows::Complete(vec![$row; MAX_CREATURE_INIT_SOURCE_ROWS + 1]);
            assert_eq!(
                oversized.validate_retention_bounds(),
                Err(InitError::Invalid("source retention row bound")),
            );
            assert!(matches!(
                prepare(&oversized),
                Err(InitError::Invalid("source retention row bound"))
            ));
        }};
    }
    let mut source = input();
    check!(class_levels, rows_mut(&mut source.class_levels)[0]);
    check!(models, rows_mut(&mut source.models)[0]);
    check!(displays, rows_mut(&mut source.displays)[0]);
    check!(spawn_template, rows_mut(&mut source.spawn_template)[0]);
    check!(external_features, ExternalFeature::Addon);

    // Other collections account for five rows. At the exact aggregate limit the
    // budget succeeds; duplicate keyed rows still fail ordinary source validation.
    let mut exact = input();
    let row = rows_mut(&mut exact.class_levels)[0];
    exact.class_levels = Rows::Complete(vec![row; MAX_CREATURE_INIT_SOURCE_ROWS - 5]);
    assert_eq!(exact.validate_retention_bounds(), Ok(()));
    assert!(matches!(
        prepare(&exact),
        Err(InitError::Invalid("duplicate classlevel row"))
    ));
    rows_mut(&mut exact.displays).push(DisplayRow {
        id: 13,
        scale: SpellFloat::ONE,
    });
    assert!(rows_mut(&mut exact.class_levels).len() < MAX_CREATURE_INIT_SOURCE_ROWS);
    assert_eq!(
        exact.validate_retention_bounds(),
        Err(InitError::Invalid("source retention row bound")),
    );
    assert!(matches!(
        prepare(&exact),
        Err(InitError::Invalid("source retention row bound"))
    ));
}

#[test]
fn serialized_selection_preserves_omitted_draws_and_cannot_supply_missing_ones() {
    let omitted = rolls();
    let supplied = SelectionRolls {
        model: Some(0),
        gender: Some(0),
        level: Some(1),
    };
    assert_eq!(
        serde_json::to_value(omitted).unwrap(),
        json!({"model": 0, "gender": null, "level": null}),
    );
    assert_ne!(
        serde_json::to_vec(&omitted).unwrap(),
        serde_json::to_vec(&supplied).unwrap(),
    );
    assert!(matches!(
        prepare(&input()).unwrap().resolve(supplied),
        Err(InitError::Roll(_))
    ));
    let missing_model = SelectionRolls {
        model: None,
        ..omitted
    };
    assert!(matches!(
        prepare(&input()).unwrap().resolve(missing_model),
        Err(InitError::Roll(_))
    ));
}

#[test]
fn initialized_serialization_covers_all_output_fields_and_nested_values() {
    let resolved = prepare(&input()).unwrap().resolve(rolls()).unwrap();
    let s = resolved.stats;
    let m = resolved.model;
    let expected = json!({
        "entry": resolved.entry,
        "level": resolved.level,
        "power_type": "Rage",
        "stats": {
            "create_attributes": s.create_attributes,
            "attributes": s.attributes,
            "create_health": s.create_health,
            "create_mana": s.create_mana,
            "base_armor": s.base_armor,
            "armor": s.armor,
            "resistances": s.resistances,
            "base_melee_attack_power": s.base_melee_attack_power,
            "loaded_base_ranged_attack_power": s.loaded_base_ranged_attack_power,
            "damage_multiplier": s.damage_multiplier,
            "base_weapon_minimum": s.base_weapon_minimum,
            "base_weapon_maximum": s.base_weapon_maximum,
            "damage": {
                "minimum": s.damage.minimum,
                "maximum": s.damage.maximum,
                "attack_power": s.damage.attack_power,
            },
            "resources": {
                "health": s.resources.health,
                "max_health": s.resources.max_health,
                "mana": s.resources.mana,
                "max_mana": s.resources.max_mana,
            },
        },
        "model": {
            "display_id": m.display_id,
            "gender": m.gender,
            "scale": m.scale,
            "bounding_radius": m.bounding_radius,
            "combat_reach": m.combat_reach,
            "walk_speed": m.walk_speed,
            "run_speed": m.run_speed,
        },
        "faction": resolved.faction,
        "unit_flags": resolved.unit_flags,
        "npc_flags": resolved.npc_flags,
        "starts_running": resolved.starts_running,
        "no_crushing_blows": resolved.no_crushing_blows,
        "creature_is_critter": resolved.creature_is_critter,
        "mainhand_interval_ms": resolved.mainhand_interval_ms,
        "entry_corpse_delay_seconds": resolved.entry_corpse_delay_seconds,
        "health_regenerates": resolved.health_regenerates,
        "power_regenerates_out_of_combat": resolved.power_regenerates_out_of_combat,
        "power_regenerates_in_combat": resolved.power_regenerates_in_combat,
    });
    assert_eq!(serde_json::to_value(resolved).unwrap(), expected);
}
