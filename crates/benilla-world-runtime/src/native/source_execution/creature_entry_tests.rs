// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::creature_init::{self as init, SourceLevelTemplate};
use std::collections::{BTreeMap, BTreeSet};

fn template() -> LoadedCreatureInfo {
    let mut row = SqlTemplateRow {
        ordinal: 0,
        words: [0; 99],
        strings: [b"Test creature".to_vec(), Vec::new(), Vec::new()],
    };
    row.words[0] = 7;
    row.words[3..5].copy_from_slice(&[5, 5]);
    row.words[14] = 2f32.to_bits();
    row.words[17] = 6;
    row.words[31] = 18;
    row.words[36] = 1;
    row.words[87] = u32::MAX;
    row
}

fn spawn() -> LoadedCreatureSpawnTemplate {
    LoadedCreatureSpawnTemplate {
        npc_flags: 8,
        unit_flags: 0x10000,
        faction: 17,
        model_id: 1192,
        equipment_id: 0,
        cur_health: 1,
        cur_mana: 0,
        spawn_flags: 7,
        relay_id: 0,
        string_id: 0,
    }
}

const MODEL: [u32; 8] = [
    1192,
    1.5f32.to_bits(),
    2f32.to_bits(),
    1f32.to_bits(),
    1.25f32.to_bits(),
    1,
    0,
    0,
];

fn construction(template: &LoadedCreatureInfo) -> CreatureConstruction {
    let mut state = CreatureConstruction::new(
        ObjectHandle {
            thread: 6,
            construction: 1,
        },
        CreatureSpawn {
            database_guid: 42,
            entry: template.words[0],
            position: [0.; 4],
            model: MODEL[0],
            respawn_seconds: [90; 2],
            respawn_radius: 5.,
            relay: 0,
        },
        template.words[0],
        CreatureTemplate {
            entry: template.words[0],
            dynamic_guid: false,
            active: false,
            count_spawns: false,
            displays: [DisplayChoice {
                id: 0,
                probability: 0,
            }; 4],
            levels: [template.words[3], template.words[4]],
            corpse_seconds: template.words[88],
            can_fly: true,
        },
        None,
        None,
    );
    state.corpse_delay = 60;
    state
}

#[test]
fn fresh_loaded_init_and_update_entry_write_live_fields_in_order() {
    let mut row = template();
    row.words[23] = 0x28_4010;
    row.words[60..63].copy_from_slice(&[1800, 2400, 4]);
    row.words[72..78].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
    let spawn = spawn();
    let entry = LoadedCreatureEntry {
        original: &row,
        effective: &row,
        spawn: &spawn,
        movement: 31,
        map: 0,
    };
    let mut live = SourceUnitFields::constructed_creature();
    let mut state = construction(&row);
    CreatureConstruction::init_loaded_entry_base(&mut live, &row);
    assert_eq!(
        (
            live.entry,
            live.loaded_creature.scale,
            live.loaded_creature.race,
            live.unit_class
        ),
        (7, Some(2f32.to_bits()), 0, Some(1))
    );
    assert_eq!(
        (
            live.power_type,
            live.faction,
            live.flags,
            live.cast_speed,
            live.vitals
        ),
        (None, 0, 0, 0, None)
    );
    let model = CreatureConstruction::apply_loaded_model(&mut live, &row, MODEL).unwrap();
    state.model = Some(model);
    let equipment = LoadedEquipmentTemplates::load(Vec::new(), BTreeMap::new()).unwrap();
    state
        .load_entry_equipment(&mut live, &entry, &equipment)
        .unwrap();
    assert_eq!(live.loaded_creature.name, b"Test creature");
    assert_eq!(live.cast_speed, 1f32.to_bits());
    let mut reads = Vec::new();
    state
        .finish_loaded_init_entry(
            &mut live,
            &entry,
            &LoadedInstanceEncounters::default(),
            |key| {
                reads.push(key);
                Ok(85)
            },
            |display, scale| {
                assert_eq!((display, scale), (1192, 2.));
                Ok(true)
            },
        )
        .unwrap();
    assert_eq!(
        reads,
        [
            CreatureEntryConfig::NormalCorpseDecay,
            CreatureEntryConfig::InteractionPause
        ]
    );
    assert_eq!(
        (state.corpse_delay, live.loaded_creature.interaction_pause),
        (85, 85)
    );
    assert_eq!(live.movement.as_ref().unwrap().flags, Some(0x40200400));
    assert_eq!(
        (
            live.loaded_creature.default_movement,
            live.loaded_creature.damage_school
        ),
        (31, 4)
    );
    assert!(
        live.loaded_creature.can_parry && !live.loaded_creature.can_block && live.force_attacking
    );
    let mut addon = CreatureAddonState::default();
    CreatureConstruction::begin_loaded_update_entry(&mut live, &mut addon);
    assert_eq!((addon.sheath, live.loaded_creature.misc_flags), (1, 0x10));
    state.select_level(None).unwrap();
    assert_eq!((state.level, state.rolls.level), (5, None));
    let input = super::super::super::synthetic_creature_generation_fixture::source(42, 5, 5).input;
    let t = &input.template;
    let init::Rows::Complete(classes) = input.class_levels else {
        unreachable!()
    };
    let definitions = CreatureLevelDefinitions::from_rows(
        vec![SourceLevelTemplate {
            entry: 7,
            levels: [5, 5],
            unit_class: 1,
            rank: 0,
            health_multiplier: t.health_multiplier,
            power_multiplier: t.power_multiplier,
            armor_multiplier: t.armor_multiplier,
            damage_multiplier: t.damage_multiplier,
            damage_variance: t.damage_variance,
            attribute_multipliers: t.attribute_multipliers,
            legacy: t.legacy,
            legacy_ranged_damage: [init_float(3.), init_float(6.)],
            regenerate_stats: t.regenerate_stats,
        }],
        classes,
    )
    .unwrap();
    let config = b"Rate.Creature.Normal.HP=1\nRate.Creature.Normal.Damage=1\nRate.Creature.Elite.Elite.HP=1\nRate.Creature.Elite.Elite.Damage=1\nRate.Creature.Elite.RAREELITE.HP=1\nRate.Creature.Elite.RAREELITE.Damage=1\nRate.Creature.Elite.WORLDBOSS.HP=1\nRate.Creature.Elite.WORLDBOSS.Damage=1\nRate.Creature.Elite.RARE.HP=1\nRate.Creature.Elite.RARE.Damage=1\n";
    let config = CreatureLevelConfig::parse(config, Sha256Id::of(config)).unwrap();
    let stats = state
        .apply_loaded_level(&mut live, &entry, &definitions, &config)
        .unwrap();
    assert_eq!(live.vitals.unwrap().health, 1);
    assert_eq!(
        live.power_fields.as_ref().unwrap().create_health,
        stats.create_health
    );
    CreatureConstruction::update_loaded_entry_flags(
        &mut live,
        &entry,
        &BTreeMap::from([(17, 0x1000)]),
    )
    .unwrap();
    assert_eq!(
        (live.faction, live.loaded_creature.npc_flags, live.flags),
        (17, 8, 0x10000)
    );
    assert_eq!(live.loaded_creature.attack_times, [1800, 1800, 2400]);
    assert_eq!(live.loaded_creature.resistances, [1, 2, 3, 4, 5, 6]);
    assert!(
        live.loaded_creature.ignore_mmap
            && live.loaded_creature.count_spawns
            && live.loaded_creature.pvp
    );
}

fn init_float(value: f32) -> benilla_sim::classic_spell::SpellFloat {
    benilla_sim::classic_spell::SpellFloat::from_bits(value.to_bits()).unwrap()
}

#[test]
fn corpse_decay_precedes_interaction_config_and_world_boss_stops_before_pvp() {
    let store = LoadedDungeonEncounters::load([DungeonEncounterEntry {
        id: 1,
        map: 0,
        difficulty: 0,
        data: 0,
        index: 0,
        complete_world_state: 0,
        names: Default::default(),
        name_flags: 0,
        icon: 0,
    }]);
    let (encounters, _) = LoadedInstanceEncounters::load(
        None,
        Some(&[[1, 0, 7, 0]]),
        &store,
        &EncounterReferences {
            creatures: BTreeSet::from([7]),
            spells: BTreeSet::new(),
        },
    );
    for (map, explicit, expected) in [(0, 130, 130), (0, 0, 3600), (1, 0, 240)] {
        let mut row = template();
        row.words[17] = 1;
        row.words[37] = 3;
        row.words[87..89].copy_from_slice(&[180, explicit]);
        let spawn = spawn();
        let entry = LoadedCreatureEntry {
            original: &row,
            effective: &row,
            spawn: &spawn,
            movement: 0,
            map,
        };
        let mut live = SourceUnitFields::constructed_creature();
        let mut state = construction(&row);
        CreatureConstruction::init_loaded_entry_base(&mut live, &row);
        CreatureConstruction::apply_loaded_model(&mut live, &row, MODEL).unwrap();
        state
            .finish_loaded_init_entry(
                &mut live,
                &entry,
                &encounters,
                |key| {
                    assert_eq!(
                        (map, explicit, key),
                        (1, 0, CreatureEntryConfig::WorldBossCorpseDecay)
                    );
                    Ok(240)
                },
                |_, _| panic!("land creature asks for terrain"),
            )
            .unwrap();
        assert_eq!(
            (state.corpse_delay, live.loaded_creature.interaction_pause),
            (expected, 180)
        );
        assert_eq!(
            CreatureConstruction::update_loaded_entry_flags(
                &mut live,
                &entry,
                &BTreeMap::from([(17, 0x1000)])
            )
            .unwrap_err()
            .id,
            "execution.creature.loaded_world_boss_immunity_owner"
        );
        assert!(!live.loaded_creature.can_modify_stats && !live.loaded_creature.pvp);
        assert_eq!((live.faction, live.flags), (17, 0x10000));
    }
}

#[test]
fn init_entry_class_power_and_select_level_mana_agree_for_all_source_cases() {
    let mut row = template();
    let value = init::SourceLevelStats {
        level: 5,
        create_attributes_bits: [0; 5],
        attributes: [0; 5],
        create_health: 100,
        create_mana: 50,
        maximum_health: 100,
        health: 100,
        maximum_power: [50, 1000, 0, 100, 0],
        power: [50, 15, 0, 100, 0],
        armor: 0,
        attack_power: [0; 2],
        weapon_damage_bits: [[0; 2]; 3],
        damage_multiplier_bits: 0,
    };
    for (class, power, has_mana) in [(1, 1, false), (2, 0, true), (4, 3, false), (8, 0, true)] {
        row.words[36] = class;
        let mut live = SourceUnitFields::constructed_creature();
        CreatureConstruction::init_loaded_entry_base(&mut live, &row);
        assert_eq!(live.power_type, None);
        CreatureConstruction::apply_loaded_model(&mut live, &row, MODEL).unwrap();
        live.apply_source_level(&value, class);
        assert_eq!(live.power_type, Some(power));
        assert_eq!(live.vitals.unwrap().has_mana, has_mana);
        assert_eq!(
            live.power_fields.as_ref().unwrap().auxiliary_current,
            Some([15, 0, 0])
        );
    }
}
