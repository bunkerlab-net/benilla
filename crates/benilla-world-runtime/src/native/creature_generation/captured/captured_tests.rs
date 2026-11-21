// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored rows only, including the real selectors but no database values.
use super::*;
use serde_json::json;

fn captured(entry: u32) -> (CreatureCapturedInput, CreatureGenerationSource) {
    let mut template: CreatureCapturedRow = TEMPLATE_FIELDS
        .iter()
        .map(|&key| (key.into(), json!(0)))
        .collect();
    for field in ["Name", "SubName", "AIName", "ScriptName"] {
        template.insert(field.into(), json!(""));
    }
    for &field in FLOAT_FIELDS {
        template.insert(field.into(), json!({"decimal": "0"}));
    }
    for (field, value) in [
        ("Entry", entry),
        ("MinLevel", 1),
        ("MaxLevel", 2),
        ("UnitClass", 1),
        ("Faction", 25),
        ("CreatureType", 7),
        ("DisplayId1", 31),
        ("DisplayIdProbability1", 1),
        ("MeleeBaseAttackTime", 2500),
        ("InhabitType", 3),
        ("RegenerateStats", 10),
    ] {
        template.insert(field.into(), json!(value));
    }
    for (field, value) in [
        ("Scale", "1.25"),
        ("HealthMultiplier", "2"),
        ("PowerMultiplier", "3"),
        ("ArmorMultiplier", "1.5"),
        ("DamageMultiplier", "1.5"),
        ("DamageVariance", "0.5"),
        ("StrengthMultiplier", "1"),
        ("AgilityMultiplier", "1"),
        ("StaminaMultiplier", "1"),
        ("IntellectMultiplier", "1"),
        ("SpiritMultiplier", "1"),
    ] {
        template.insert(field.into(), json!({"decimal": value}));
    }
    template.insert("AIName".into(), json!("EventAI"));
    let class_levels = (1..=2).map(|level| {
        json!({"Class": 1, "Level": level, "BaseMana": 100, "BaseMeleeAttackPower": 14 * level,
            "BaseRangedAttackPower": 14, "BaseArmor": 60 + 40 * level, "Strength": 10,
            "Agility": 10, "Stamina": 10, "Intellect": 10, "Spirit": 10,
            "BaseHealthExp0": 40 + 40 * level, "BaseDamageExp0": {"decimal": (4 * level).to_string()},
            "BaseDamageExp0OLD": {"decimal": "123"}}).as_object().unwrap().clone().into_iter().collect()
    }).collect();
    let input = CreatureCapturedInput {
        templates: init::Rows::Complete(vec![template]),
        class_levels: init::Rows::Complete(class_levels),
        faction_templates: init::Rows::Complete(vec![CreatureCapturedFactionTemplate {
            id: 25,
            faction: 123456,
            flags: 0,
            faction_group: 8,
            friend_group: 0,
            enemy_group: 0,
            enemies: [0; 4],
            friends: [123456, 0, 0, 0],
        }]),
    };
    let mut source = super::super::super::synthetic_creature_generation_fixture::source(2, 1, 2);
    source.spawn_entry = entry;
    source.input.rates.normal_health = SpellFloat::from_bits(1.5_f32.to_bits()).unwrap();
    source.input.rates.normal_damage = SpellFloat::from_bits(2_f32.to_bits()).unwrap();
    (input, source)
}
fn template(input: &mut CreatureCapturedInput) -> &mut CreatureCapturedRow {
    let init::Rows::Complete(rows) = &mut input.templates else {
        unreachable!()
    };
    &mut rows[0]
}
fn resolve(source: &CreatureGenerationSource, level: u32) -> init::InitializedCreature {
    let plan = init::prepare(&source.input).unwrap();
    let model = plan.model_roll_range().map(|r| r.0);
    let gender = plan.gender_roll_range(model).unwrap().map(|r| r.0);
    plan.resolve(init::SelectionRolls {
        model,
        gender,
        level: Some(level),
    })
    .unwrap()
}
#[test]
fn captured_vermin_two_levels_apply_loaded_stats_and_multipliers() {
    let (input, context) = captured(6);
    let source = input.compile(context).unwrap();
    for (level, health, armor, low, high) in
        [(1, 240, 140, 12_f32, 18_f32), (2, 360, 200, 24_f32, 36_f32)]
    {
        let result = resolve(&source, level);
        assert_eq!(result.faction, 25);
        assert_eq!(result.stats.resources.max_health, health);
        assert_eq!(result.stats.resources.max_mana, 100);
        assert_eq!(result.stats.armor, armor);
        assert_eq!(result.stats.damage.minimum.bits(), low.to_bits());
        assert_eq!(result.stats.damage.maximum.bits(), high.to_bits());
    }
    let plan = init::prepare(&source.input).unwrap();
    assert_eq!(plan.level_roll_range(), Some((1, 2)));
    for level in [None, Some(0), Some(3)] {
        assert!(init::prepare(&source.input)
            .unwrap()
            .resolve(init::SelectionRolls {
                model: Some(0),
                gender: Some(0),
                level,
            })
            .is_err());
    }
}
#[test]
fn generation_rng_reaches_both_level_endpoints_and_replays_exactly() {
    let (input, context) = captured(6);
    let source = input.compile(context).unwrap();
    let catalog =
        CreatureGenerationCatalog::validate_sources(init::Rows::Complete(vec![source]), vec![2])
            .unwrap();
    let mut counts = [0; 2];
    for seed in 0..64 {
        let npc = crate::NpcId {
            spawn: 2,
            generation: 0,
        };
        let mut cursor = 0;
        let (owner, draws) = catalog
            .initial([7; 16], [8; 32], npc, seed, &mut cursor)
            .unwrap();
        counts[usize::from(owner.result().level - 1)] += 1;
        let draw = draws
            .iter()
            .find(|d| matches!(d.purpose, DrawPurpose::CreatureLevel { .. }))
            .unwrap();
        assert_eq!((draw.low, draw.high), (1, 2));
        assert_eq!(draw.value, i64::from(owner.result().level));
        let mut again = 0;
        let replay = catalog
            .initial([7; 16], [8; 32], npc, seed, &mut again)
            .unwrap();
        assert_eq!((owner, draws, cursor), (replay.0, replay.1, again));
    }
    assert!(counts.into_iter().all(|n| n >= 16));
}
#[test]
fn missing_conflicting_unsupported_and_out_of_range_rows_refuse() {
    let (good, context) = captured(6);
    let mut bad = good.clone();
    bad.class_levels = init::Rows::Missing;
    assert!(bad.compile(context.clone()).is_err());
    let mut bad = good.clone();
    let init::Rows::Complete(rows) = &mut bad.class_levels else {
        unreachable!()
    };
    rows.pop();
    assert!(bad.compile(context.clone()).is_err());
    let mut bad = good.clone();
    let init::Rows::Complete(rows) = &mut bad.class_levels else {
        unreachable!()
    };
    rows.push(rows[0].clone());
    assert!(bad.compile(context.clone()).is_err());
    let mut bad = good.clone();
    bad.faction_templates = init::Rows::Complete(vec![]);
    assert!(bad.compile(context.clone()).is_err());
    let mut bad = good.clone();
    let init::Rows::Complete(rows) = &mut bad.faction_templates else {
        unreachable!()
    };
    rows.push(rows[0].clone());
    assert!(bad.compile(context.clone()).is_err());
    let mut bad = good.clone();
    let init::Rows::Complete(rows) = &mut bad.templates else {
        unreachable!()
    };
    rows.push(rows[0].clone());
    assert!(bad.compile(context.clone()).is_err());
    for (field, value) in [
        ("Rank", json!(1)),
        ("UnitFlags", json!(0x80000000_u32)),
        ("NpcFlags", json!(0x80000000_u32)),
        ("ExtraFlags", json!(4)),
        ("ExtraFlags", json!(0x10002)),
        ("StaticFlags1", json!(2)),
        ("StaticFlags1", json!(4)),
        ("MaxLevel", json!(21)),
        ("UnitClass", json!(3)),
        ("AIName", json!("OtherAI")),
        ("ScriptName", json!("named_script")),
        ("HealthMultiplier", json!({"decimal": "1000001"})),
        ("DamageVariance", json!({"decimal": "2.1"})),
        ("MinLevel", json!(-1)),
    ] {
        let mut bad = good.clone();
        template(&mut bad).insert(field.into(), value);
        assert!(bad.compile(context.clone()).is_err(), "{field}");
    }
    let mut bad = good.clone();
    template(&mut bad).remove("DamageMultiplier");
    assert!(bad.compile(context.clone()).is_err());
    let mut bad = good;
    template(&mut bad).insert("Unknown".into(), json!(0));
    assert!(bad.compile(context).is_err());
}
#[test]
fn northshire_selectors_retain_no_xp_and_existing_kill_formula_awards_zero() {
    use benilla_sim::player_progression::kill_xp;
    for entry in [823, 197, 196, 198] {
        let (mut input, context) = captured(entry);
        for (field, value) in [
            ("StaticFlags1", json!(2)),
            ("UnitFlags", json!(0x1300)),
            ("ExtraFlags", json!(0x10002)),
            ("Civilian", json!(1)),
            ("AIName", json!("")),
        ] {
            template(&mut input).insert(field.into(), value);
        }
        let source = input.compile(context).unwrap();
        assert_eq!(source.input.template.static_flags[0], 2);
        let result = resolve(&source, 1);
        let victim = kill_xp::Victim {
            guid: 2,
            entry,
            level: 1,
            max_health: result.stats.resources.max_health,
            damage_by_others: 0,
            experience_multiplier_bits: 1_f32.to_bits(),
            is_no_xp: source.input.template.static_flags[0] & 2 != 0,
            is_totem: false,
            is_pet: false,
            is_critter: false,
            is_elite: false,
            player_controlled: false,
        };
        assert_eq!(
            kill_xp::gain(
                1,
                victim,
                kill_xp::Rules {
                    kill_rate_bits: 1_f32.to_bits()
                }
            )
            .unwrap(),
            0
        );
    }
}

#[test]
fn native_kill_policy_must_bind_retained_no_xp_flag() {
    let (mut profile, _, mut progression) = super::super::super::synthetic_quest_fixture::setup();
    let sources: Vec<_> = profile
        .npcs
        .iter()
        .map(|npc| {
            let mut source = super::super::super::synthetic_creature_generation_fixture::source(
                npc.ai.creature_id,
                1,
                1,
            );
            source.spawn_entry = npc.entry;
            source.input.template.entry = npc.entry;
            source.input.template.static_flags[0] = 2;
            source.input.template.unit_flags = 0x100;
            source
        })
        .collect();
    let order = sources.iter().map(|s| s.spawn_id).collect();
    profile.creature_generations = Some(
        CreatureGenerationCatalog::validate_sources(init::Rows::Complete(sources), order).unwrap(),
    );
    assert!(progression.kill.validate(&profile, false).is_err());
    for row in &mut progression.kill.creatures {
        row.is_no_xp = true;
    }
    progression.kill.validate(&profile, false).unwrap();
}

#[test]
fn unit_multipliers_and_fixed_level_use_the_existing_no_draw_branch() {
    let (mut input, mut context) = captured(6);
    template(&mut input).insert("MaxLevel".into(), json!(1));
    for field in [
        "HealthMultiplier",
        "PowerMultiplier",
        "ArmorMultiplier",
        "DamageMultiplier",
    ] {
        template(&mut input).insert(field.into(), json!({"decimal": "1"}));
    }
    context.input.rates.normal_health = SpellFloat::ONE;
    context.input.rates.normal_damage = SpellFloat::ONE;
    let source = input.compile(context).unwrap();
    let plan = init::prepare(&source.input).unwrap();
    assert_eq!(plan.level_roll_range(), None);
    let result = plan
        .resolve(init::SelectionRolls {
            model: Some(0),
            gender: Some(0),
            level: None,
        })
        .unwrap();
    assert_eq!(result.stats.resources.max_health, 80);
    assert_eq!(result.stats.armor, 100);
    assert_eq!(result.stats.damage.minimum.bits(), 4_f32.to_bits());
    assert_eq!(result.stats.damage.maximum.bits(), 6_f32.to_bits());
}

#[test]
fn mana_class_uses_power_multiplier_and_spawn_faction_requires_its_own_row() {
    let (mut input, mut context) = captured(6);
    template(&mut input).insert("UnitClass".into(), json!(8));
    template(&mut input).insert("RegenerateStats".into(), json!(2));
    let init::Rows::Complete(rows) = &mut input.class_levels else {
        unreachable!()
    };
    for row in rows {
        row.insert("Class".into(), json!(8));
    }
    assert_eq!(
        resolve(&input.clone().compile(context.clone()).unwrap(), 1)
            .stats
            .resources
            .max_mana,
        300
    );
    let init::Rows::Complete(spawns) = &mut context.input.spawn_template else {
        unreachable!()
    };
    spawns[0].faction = 12;
    assert!(input.clone().compile(context.clone()).is_err());
    let init::Rows::Complete(factions) = &mut input.faction_templates else {
        unreachable!()
    };
    let mut override_row = factions[0].clone();
    override_row.id = 12;
    factions.push(override_row);
    assert_eq!(resolve(&input.compile(context).unwrap(), 1).faction, 12);
}
