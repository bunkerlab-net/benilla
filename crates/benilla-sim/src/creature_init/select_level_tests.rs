use super::*;
fn select_level_stats(
    t: &SourceLevelTemplate,
    row: Option<&ClassLevelRow>,
    level: u32,
    forced: bool,
    rates: SourceLevelRates,
) -> Result<SourceLevelStats> {
    let loaded = row.copied().map(load_source_class_level).transpose()?;
    super::select_level_stats(t, loaded.as_ref(), level, forced, rates)
}
fn f(value: f32) -> SpellFloat {
    SpellFloat::from_bits(value.to_bits()).unwrap()
}
fn template() -> SourceLevelTemplate {
    SourceLevelTemplate {
        entry: 1,
        levels: [10, 12],
        unit_class: 8,
        rank: 0,
        health_multiplier: f(1.0),
        power_multiplier: f(1.0),
        armor_multiplier: f(1.0),
        damage_multiplier: f(1.0),
        damage_variance: f(0.5),
        attribute_multipliers: [f(1.0); 5],
        legacy: LegacyStats {
            minimum_health: 100,
            maximum_health: 200,
            minimum_mana: 50,
            maximum_mana: 100,
            armor: 10,
            minimum_melee_damage: f(2.0),
            maximum_melee_damage: f(4.0),
            melee_attack_power: 0,
            ranged_attack_power: 0,
        },
        legacy_ranged_damage: [f(3.0), f(6.0)],
        regenerate_stats: 8,
    }
}
fn row() -> ClassLevelRow {
    ClassLevelRow {
        class: 8,
        level: 11,
        base_health: 100,
        base_mana: 200,
        base_armor: 100,
        base_melee_attack_power: f(14.0),
        base_ranged_attack_power: f(14.0),
        base_damage: f(8.0),
        attributes: [20; 5],
    }
}
fn rates() -> SourceLevelRates {
    SourceLevelRates {
        health: f(1.0),
        damage: f(1.0),
    }
}
#[test]
fn modern_resources_and_all_five_powers_follow_source() {
    // Creature.cpp:1317-1368,1425-1455,1485-1489; StatSystem.cpp:186-200.
    let value = select_level_stats(&template(), Some(&row()), 11, false, rates()).unwrap();
    assert_eq!((value.create_health, value.create_mana), (100, 180));
    assert_eq!(value.maximum_power, [200, 0, 100, 0, 1_000_000]);
    assert_eq!(value.health, 100);
    assert_eq!(value.armor, 100);
}
#[test]
fn sentinel_and_legacy_damage_branches_are_independent() {
    // Creature.cpp:1317-1327,1371-1404: modern armor preserves unsigned -1.
    let mut t = template();
    t.health_multiplier = f(-1.0);
    t.power_multiplier = f(-1.0);
    assert_eq!(
        select_level_stats(&t, Some(&row()), 11, false, rates()),
        Err(InitError::Numeric(
            "SelectLevel_health_rate_conversion_undefined"
        ))
    );
    t.armor_multiplier = f(-1.0);
    let value = select_level_stats(&t, Some(&row()), 11, false, rates()).unwrap();
    assert_eq!((value.create_health, value.create_mana), (150, 75));
    assert_eq!(value.maximum_power[0], 95);
    assert_eq!(
        value.weapon_damage_bits[1],
        [1f32.to_bits(), 2f32.to_bits()]
    );
}
#[test]
fn missing_class_stats_uses_real_legacy_fallback_and_forced_level_quirk() {
    // Creature.cpp:1313,1371-1414: integer division precedes multiplication.
    let value = select_level_stats(&template(), None, 11, false, rates()).unwrap();
    assert_eq!((value.create_health, value.create_mana), (150, 75));
    let value = select_level_stats(&template(), None, 20, true, rates()).unwrap();
    assert_eq!((value.create_health, value.create_mana), (320, 160));
    let mut t = template();
    t.levels[1] = 0;
    assert_eq!(
        select_level_stats(&t, None, 20, true, rates()),
        Err(InitError::Numeric(
            "SelectLevel_forced_level_division_undefined"
        ))
    );
}
#[test]
fn unsigned_conversion_accepts_truncation_but_refuses_two_to_32() {
    // Creature.cpp:1417 converts the binary32 product to uint32 before clamp.
    let mut t = template();
    t.health_multiplier = f(-1.0);
    let mut r = rates();
    r.health = f(0.5);
    assert_eq!(
        select_level_stats(&t, Some(&row()), 11, false, r)
            .unwrap()
            .create_health,
        2_147_483_648
    );
    r.health = f(1.0);
    assert_eq!(
        select_level_stats(&t, Some(&row()), 11, false, r),
        Err(InitError::Numeric(
            "SelectLevel_health_rate_conversion_undefined"
        ))
    );
}
#[test]
fn spawn_current_overrides_clamp_and_obey_mana_regeneration() {
    // Creature.cpp:526-536; Unit.cpp SetHealth/SetPower clamp to maxima.
    let mut value = select_level_stats(&template(), Some(&row()), 11, false, rates()).unwrap();
    value.apply_spawn(0, 0, true, 8, 8);
    assert_eq!((value.health, value.power[0]), (100, 200));
    value.apply_spawn(500, 500, false, 8, 8);
    assert_eq!((value.health, value.power[0]), (0, 200));
    value.apply_spawn(1, 0, true, 8, 0);
    assert_eq!((value.health, value.power[0]), (1, 0));
}

#[test]
fn independent_damage_sentinel_keeps_modern_health_and_armor() {
    // Creature.cpp:1334,1371,1379,1393. Only the legacy damage branch runs.
    let mut t = template();
    t.damage_multiplier = f(-1.0);
    let value = select_level_stats(&t, Some(&row()), 11, false, rates()).unwrap();
    assert_eq!(
        (value.create_health, value.create_mana, value.armor),
        (100, 180, 100)
    );
    assert_eq!(
        value.weapon_damage_bits[0],
        [(-2f32).to_bits(), (-4f32).to_bits()]
    );
    t.health_multiplier = f(0.0);
    t.power_multiplier = f(0.0);
    let value = select_level_stats(&t, Some(&row()), 11, false, rates()).unwrap();
    assert_eq!((value.maximum_health, value.maximum_power[0]), (100, 200));
}

#[test]
fn class_getter_miss_and_array_boundary_have_distinct_source_paths() {
    // ObjectMgr.cpp:793-798; ObjectMgr.h:1354; SharedDefines.h:104.
    let mut r = row();
    r.base_damage = f(0.1);
    assert_eq!(
        select_level_stats(&template(), Some(&r), 11, false, rates())
            .unwrap()
            .create_health,
        150
    );
    assert!(select_level_stats(&template(), None, 70, true, rates()).is_ok());
    assert_eq!(
        select_level_stats(&template(), None, 71, true, rates()),
        Err(InitError::Numeric(
            "GetCreatureClassLvlStats_index_undefined"
        ))
    );
    let mut t = template();
    t.unit_class = 11;
    assert!(select_level_stats(&t, None, 11, false, rates()).is_ok());
    t.unit_class = 12;
    assert_eq!(
        select_level_stats(&t, None, 11, false, rates()),
        Err(InitError::Numeric(
            "GetCreatureClassLvlStats_index_undefined"
        ))
    );
}

#[test]
fn mana_sentinel_refuses_only_at_update_max_power_conversion() {
    // Creature.cpp:1323,1436,1453; StatSystem.cpp:190-200.
    let mut t = template();
    let mut r = row();
    r.attributes[3] = 0;
    r.base_mana = 4_294_967_040;
    assert_eq!(
        select_level_stats(&t, Some(&r), 11, false, rates())
            .unwrap()
            .maximum_power[0],
        4_294_967_040
    );
    t.power_multiplier = f(-1.0);
    assert_eq!(
        select_level_stats(&t, Some(&r), 11, false, rates()),
        Err(InitError::Numeric(
            "UpdateMaxPower_mana_conversion_undefined"
        ))
    );
}

#[test]
fn signed_attribute_and_armor_conversion_boundaries_refuse_by_site() {
    // StatSystem.cpp:623,669 and Creature.cpp:1331.
    let mut r = row();
    r.attributes[4] = 2_147_483_520;
    assert_eq!(
        select_level_stats(&template(), Some(&r), 11, false, rates())
            .unwrap()
            .attributes[4],
        2_147_483_520
    );
    r.attributes[4] = 2_147_483_648;
    assert_eq!(
        select_level_stats(&template(), Some(&r), 11, false, rates()),
        Err(InitError::Numeric(
            "UpdateStats_attribute_conversion_undefined"
        ))
    );
    r = row();
    r.attributes[1] = 0;
    r.base_armor = 2_147_483_520;
    assert_eq!(
        select_level_stats(&template(), Some(&r), 11, false, rates())
            .unwrap()
            .armor,
        2_147_483_520
    );
    r.base_armor = 2_147_483_648;
    assert_eq!(
        select_level_stats(&template(), Some(&r), 11, false, rates()),
        Err(InitError::Numeric("UpdateArmor_conversion_undefined"))
    );
}

#[test]
fn attack_power_and_maximum_health_conversions_guard_both_sides() {
    // StatSystem.cpp:724,817. The unsigned conversion must precede clamping.
    let mut t = template();
    let mut r = row();
    let mut rates = rates();
    t.damage_multiplier = f(4000.0);
    r.base_melee_attack_power = f(1_000_000.0);
    assert!(select_level_stats(&t, Some(&r), 11, false, rates).is_ok());
    t.damage_multiplier = f(5000.0);
    assert_eq!(
        select_level_stats(&t, Some(&r), 11, false, rates),
        Err(InitError::Numeric(
            "UpdateAttackPowerAndDamage_melee_conversion_undefined"
        ))
    );
    t = template();
    r = row();
    t.damage_multiplier = f(4000.0);
    r.base_ranged_attack_power = f(1_000_000.0);
    r.attributes[1] = 0;
    assert!(select_level_stats(&t, Some(&r), 11, false, rates).is_ok());
    t.damage_multiplier = f(5000.0);
    assert_eq!(
        select_level_stats(&t, Some(&r), 11, false, rates),
        Err(InitError::Numeric(
            "UpdateAttackPowerAndDamage_ranged_conversion_undefined"
        ))
    );
    t = template();
    r = row();
    t.health_multiplier = f(1000.0);
    r.base_health = 4_000_000;
    assert_eq!(
        select_level_stats(&t, Some(&r), 11, false, rates)
            .unwrap()
            .maximum_health,
        4_000_000_000
    );
    r.base_health = 5_000_000;
    assert_eq!(
        select_level_stats(&t, Some(&r), 11, false, rates),
        Err(InitError::Numeric("UpdateMaxHealth_conversion_undefined"))
    );
    rates.health = f(0.0);
    assert_eq!(
        select_level_stats(&t, Some(&r), 11, false, rates)
            .unwrap()
            .create_health,
        1
    );
}

#[test]
fn class_loader_mana_and_legacy_interpolation_guard_uint_boundaries() {
    // ObjectMgr.cpp:779, StatSystem.cpp:159-164. Loader conversion precedes
    // the clamp against BaseMana, even when that field contains zero.
    let mut r = row();
    r.base_mana = 0;
    r.attributes[3] = 100_000_000;
    assert!(load_source_class_level(r).is_ok());
    r.attributes[3] = 300_000_000;
    assert_eq!(
        load_source_class_level(r),
        Err(InitError::Numeric(
            "LoadCreatureClassLvlStats_mana_conversion_undefined"
        ))
    );
    // Creature.cpp:1383. A uint32 difference rounds to 2^32 in binary32.
    let mut t = template();
    t.legacy.minimum_health = 0;
    t.legacy.maximum_health = u32::MAX;
    assert_eq!(
        select_level_stats(&t, None, 11, false, rates())
            .unwrap()
            .create_health,
        2_147_483_648
    );
    assert_eq!(
        select_level_stats(&t, None, 12, false, rates()),
        Err(InitError::Numeric(
            "SelectLevel_legacy_interpolation_conversion_undefined"
        ))
    );
}

#[test]
fn update_all_stats_refuses_in_source_attribute_order() {
    // StatSystem.cpp:641-642,623-630. Strength's AP update precedes spirit.
    let mut t = template();
    let mut r = row();
    t.damage_multiplier = f(5000.0);
    r.base_melee_attack_power = f(1_000_000.0);
    r.attributes[4] = u32::MAX;
    assert_eq!(
        select_level_stats(&t, Some(&r), 11, false, rates()),
        Err(InitError::Numeric(
            "UpdateAttackPowerAndDamage_melee_conversion_undefined"
        ))
    );
}

#[test]
fn rank_rates_precede_template_multipliers_and_scale_legacy_base_damage() {
    // Creature.cpp:1308-1310,1395-1398,1417; StatSystem.cpp:766-767.
    let mut t = template();
    t.health_multiplier = f(2.0);
    let rates = SourceLevelRates {
        health: f(3.0),
        damage: f(4.0),
    };
    let value = select_level_stats(&t, Some(&row()), 11, true, rates).unwrap();
    assert_eq!((value.create_health, value.maximum_health), (300, 600));
    assert_eq!(value.damage_multiplier_bits, 4f32.to_bits());
    assert_eq!(
        value.weapon_damage_bits[0],
        [6f32.to_bits(), 10f32.to_bits()]
    );
    t.armor_multiplier = f(-1.0);
    let value = select_level_stats(&t, Some(&row()), 11, true, rates).unwrap();
    assert_eq!((value.create_health, value.maximum_health), (450, 900));
    assert_eq!(
        value.weapon_damage_bits[0],
        [8f32.to_bits(), 16f32.to_bits()]
    );
}

#[test]
fn attribute_sentinels_zero_armor_and_negative_weapon_minimum_are_defined() {
    // Creature.cpp:1330-1368. Unlike the bounded kernel, variance above two
    // preserves a negative base weapon minimum rather than refusing it.
    let mut t = template();
    t.attribute_multipliers = [f(-1.0); 5];
    t.armor_multiplier = f(0.0);
    t.damage_variance = f(4.0);
    let value = select_level_stats(&t, Some(&row()), 11, false, rates()).unwrap();
    assert_eq!(value.attributes, [0; 5]);
    assert_eq!(value.armor, 0);
    assert_eq!(value.maximum_power[0], 180);
    assert_eq!(
        value.weapon_damage_bits[0],
        [(-8f32).to_bits(), 24f32.to_bits()]
    );
}

#[test]
fn power_multiplier_applies_only_to_the_active_mana_type() {
    // Creature.cpp:1483; StatSystem.cpp:190-200. A warrior's mana is still set.
    let mut t = template();
    t.power_multiplier = f(2.0);
    assert_eq!(
        select_level_stats(&t, Some(&row()), 11, false, rates())
            .unwrap()
            .maximum_power[0],
        400
    );
    t.unit_class = 1;
    let mut r = row();
    r.class = 1;
    assert_eq!(
        select_level_stats(&t, Some(&r), 11, false, rates())
            .unwrap()
            .maximum_power[0],
        200
    );
}

#[test]
fn legacy_negative_attack_power_refuses_after_the_defined_zero_boundary() {
    // Creature.cpp:1395-1404; StatSystem.cpp:703-715,724.
    let mut t = template();
    t.damage_multiplier = f(-1.0);
    t.legacy.melee_attack_power = 20;
    assert_eq!(
        select_level_stats(&t, Some(&row()), 11, false, rates())
            .unwrap()
            .attack_power[0],
        0
    );
    t.legacy.melee_attack_power = 21;
    assert_eq!(
        select_level_stats(&t, Some(&row()), 11, false, rates()),
        Err(InitError::Numeric(
            "UpdateAttackPowerAndDamage_melee_conversion_undefined"
        ))
    );
}
