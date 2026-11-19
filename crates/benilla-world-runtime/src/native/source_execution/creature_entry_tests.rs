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
    assert_eq!(live.resistances.create_nonphysical(), [1, 2, 3, 4, 5, 6]);
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
fn corpse_decay_precedes_interaction_config_and_world_boss_immunity() {
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
        CreatureConstruction::update_loaded_entry_flags(
            &mut live,
            &entry,
            &BTreeMap::from([(17, 0x1000)]),
        )
        .unwrap();
        assert!(live.loaded_creature.can_modify_stats && live.loaded_creature.pvp);
        for (aura, immune) in [(136, false), (137, true), (138, false)] {
            let spell = SpellTemplate {
                effect1: 6,
                effectapplyauraname1: aura,
                ..Default::default()
            };
            assert_eq!(
                live.immunities
                    .immune_to_self_effect(&spell, 0, &live.auras)
                    .unwrap(),
                immune
            );
        }
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
        armor_base_bits: 0,
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
        live.apply_source_level(&value, class).unwrap();
        assert_eq!(live.power_type, Some(power));
        assert_eq!(live.vitals.unwrap().has_mana, has_mana);
        assert_eq!(
            live.power_fields.as_ref().unwrap().auxiliary_current,
            Some([15, 0, 0])
        );
    }
}

#[test]
fn fresh_spell_list_disables_zero_and_filters_only_the_live_copy() {
    use std::sync::Arc;

    struct Rolls(Vec<(u32, u32)>);
    impl CreatureRandom for Rolls {
        fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
            panic!("Availability calls urand")
        }

        fn creature_urand(&mut self, low: u32, high: u32) -> Result<u32> {
            self.0.push((low, high));
            Ok(100)
        }
    }

    let (lists, _) = LoadedCreatureSpellLists::load(
        vec![SqlTemplateRow {
            ordinal: 0,
            words: [1, 0, 0, 0, 0, u32::MAX],
            strings: [b"current".to_vec()],
        }],
        vec![SqlTemplateRow {
            ordinal: 0,
            words: [700, 0, 0],
            strings: [b"list".to_vec()],
        }],
        vec![
            SqlTemplateRow {
                ordinal: 0,
                words: [700, 0, 10, 0, u32::MAX, 1, 0, 99, 0, 0, 0, 0, 0],
                strings: [],
            },
            SqlTemplateRow {
                ordinal: 1,
                words: [700, 1, 11, 0, u32::MAX, 1, 0, 100, 0, 0, 0, 0, 0],
                strings: [],
            },
        ],
        &BTreeMap::from([(10, 0), (11, 0)]),
    );
    let map = CreatureSpellMapData(Arc::new(lists));
    let mut live = CreatureConstruction::construct_loaded_unit();
    let mut random = Rolls(Vec::new());
    let key = source_creature_spell_list_key(7, 0).unwrap();
    assert_eq!(
        live.set_fresh_creature_spell_list(key, None, &mut random)
            .unwrap_err()
            .id,
        "execution.creature.loaded_spell_list_store_owner"
    );
    assert!(!live
        .set_fresh_creature_spell_list(
            source_creature_spell_list_key(7, 89).unwrap(),
            Some(&map),
            &mut random
        )
        .unwrap());
    assert_eq!(live.spell_list_disabled, None);
    assert!(random.0.is_empty());
    assert!(live
        .set_fresh_creature_spell_list(key, Some(&map), &mut random)
        .unwrap());
    assert_eq!(random.0, [(0, 100)]);
    assert_eq!(map.get(700).unwrap().spells.len(), 2);
    assert_eq!(
        live.spell_list
            .as_ref()
            .unwrap()
            .spells
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(live.spell_list_disabled, Some(false));

    let mut zero = CreatureConstruction::construct_loaded_unit();
    let wrapping_zero = source_creature_spell_list_key(0x40000000, 0).unwrap_or(0);
    assert!(zero
        .set_fresh_creature_spell_list(wrapping_zero, None, &mut random)
        .unwrap());
    assert_eq!(zero.spell_list_disabled, Some(true));
    assert!(zero.spell_list.is_none());
    assert_eq!(random.0, [(0, 100)]);
}

#[test]
fn loaded_update_applies_immunity_before_critter_and_refuses_event_before_string_id() {
    let mut row = template();
    row.words[16] = 8;
    row.words[91] = 11;
    let spawn = spawn();
    let entry = LoadedCreatureEntry {
        original: &row,
        effective: &row,
        spawn: &spawn,
        movement: 0,
        map: 0,
    };
    let mut live = CreatureConstruction::construct_loaded_unit();
    let mut state = construction(&row);
    CreatureConstruction::init_loaded_entry_base(&mut live, &row);
    assert_eq!(
        CreatureConstruction::update_loaded_entry_immunities(&mut live, &row, None)
            .unwrap_err()
            .id,
        "execution.creature.loaded_immunity_set_store_owner"
    );
    assert!(!live.critter && !live.loaded_creature.chain_immune);
    let (store, _) = LoadedCreatureImmunities::load(&[[7, 0, 0, 6]], |entry| entry == 7);
    CreatureConstruction::update_loaded_entry_immunities(&mut live, &row, Some(&store)).unwrap();
    assert_eq!(live.immunities.set, 0);
    assert!(live.critter && live.loaded_creature.chain_immune);
    state.event = Some(CreatureEventData::default());
    assert_eq!(
        state.require_loaded_entry_callbacks(&entry).unwrap_err().id,
        "execution.creature.loaded_game_event_spells_owner"
    );
    state.event = None;
    assert_eq!(
        state.require_loaded_entry_callbacks(&entry).unwrap_err().id,
        "execution.creature.loaded_string_id_owner"
    );
}

#[test]
fn loaded_static_flags_use_live_control_and_keep_the_source_refusal_prefix() {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(6, SourceObjectKind::Creature).unwrap();
    let child = objects.construct(6, SourceObjectKind::Pet).unwrap();
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let pet = SourceGuid::with_entry(HighGuid::Pet, 7, 1);
    let stale = SourceGuid::with_entry(HighGuid::Pet, 7, 2);
    objects.assign_guid(child, 0, pet).unwrap();
    objects.get_mut(unit).unwrap().map = Some(map);
    objects.get_mut(child).unwrap().map = Some(map);
    objects.register_world_lookup(child).unwrap();

    let mut live = CreatureConstruction::construct_loaded_unit();
    let saved = serde_json::to_vec(&live).unwrap();
    live.controlled.pet = stale;
    live.controlled.guardians.insert(pet);
    assert_eq!(serde_json::to_vec(&live).unwrap(), saved);
    let flags = [0x8000_0260, 2, 0xc00, 4];
    let failure =
        SourceCreatureSettings::reset_live(&mut live, flags, unit, &mut objects).unwrap_err();
    assert_eq!(
        failure.id,
        "execution.creature_settings.Unit_10704.controlled_flags_owner"
    );
    assert_eq!(live.static_flags, flags);
    assert_eq!(objects.visibility_override_bits(unit).unwrap(), 0x4405_5555);
    assert_eq!(live.flags, 0x0200_0100);
    assert_eq!(live.controlled.pet, SourceGuid::default());

    live.control_guids.charmer = SourceGuid::without_entry(HighGuid::Player, 9);
    SourceCreatureSettings::reset_live(&mut live, flags, unit, &mut objects).unwrap();
    assert_eq!(live.flags, 0x0200_0300);
}
