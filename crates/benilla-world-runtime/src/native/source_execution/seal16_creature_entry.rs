// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:353-612, Unit.cpp:10199-10275, fresh uncharmed non-pet construction.
use super::*;
use benilla_sim::creature_capabilities::{MOVEFLAG_HOVER, MOVEFLAG_LEVITATING, MOVEFLAG_WALK_MODE};
#[derive(Debug)]
pub(in crate::native::source_execution) struct EntryFields {
    pub native_display: u32,
    pub display: u32,
    pub radius: u32,
    pub reach: u32,
    pub base_speeds: [u32; 2],
    pub model_run_speed: u32,
    pub speed_rates: [u32; 2],
    pub gender: u8,
    pub power: u8,
    pub equipment: u32,
    pub virtual_items: [u32; 3],
    pub name: Vec<u8>,
    pub cast_speed: u32,
    pub movement_flags: u32,
    pub default_movement: u32,
    pub damage_school: u32,
    pub capabilities: [bool; 6],
    pub detection: u32,
    pub corpse_delay: u32,
    pub interaction_pause: u32,
    pub addon: CreatureAddonState,
    pub respawn_radius: Option<u32>,
    pub respawn_delay: Option<u32>,
    pub respawn_time: Option<i64>,
    pub respawn_clock: Option<(u64, u64, i64)>,
    pub notify_delay: Option<u32>,
    pub misc_flags: u8,
    pub level: Option<benilla_sim::creature_init::SourceLevelStats>,
    pub faction: u32,
    pub gossip: u32,
    pub npc_flags: u32,
    pub attack_times: [u32; 3],
    pub unit_flags: u32,
    pub dynamic_flags: u32,
    pub resistances: [u32; 6],
    pub invisible: bool,
    pub ignore_mmap: bool,
    pub count_spawns: bool,
    pub can_modify_stats: bool,
    pub pvp: bool,
    pub requested_spell_list: u32,
    pub spell_list_disabled: Option<bool>,
    pub spell_list: Option<CreatureSpellList>,
    pub immunities: CreatureImmunityState,
    pub chain_immune: bool,
    pub settings: SourceCreatureSettings,
    pub control: FreshCreatureControl,
}
fn uint_config(config: &[u8], key: &str) -> Result<u32> {
    super::super::super::super::spawn_loader_config::assignment(config, key)?
        .1
        .parse::<i32>()
        .map(|value| value as u32)
        .map_err(|_| refuse("execution.creature.loaded_uint_config"))
}
fn movement_enum(value: u32) -> Result<u32> {
    require(
        value <= 31,
        "execution.creature.Creature_453.movement_enum_undefined",
    )?;
    Ok(value)
}
fn school_enum(value: u32) -> Result<u32> {
    require(
        value <= 7,
        "execution.creature.Creature_458.school_enum_undefined",
    )?;
    Ok(value)
}
fn spell_list_key(entry: u32, template_list: u32) -> Option<u32> {
    let key = if template_list != 0 {
        template_list
    } else {
        entry.wrapping_mul(100)
    };
    (key != 0).then_some(key)
}
fn model_fields(template: &LoadedCreatureInfo, model: [u32; 8]) -> EntryFields {
    let native_scale = f32::from_bits(template.words[14]);
    let current_scale = f32::from_bits(template.words[14]);
    let normalized = if native_scale != 0.0 {
        current_scale / native_scale
    } else {
        1.0
    };
    let base_speeds = [3, 4].map(|column| {
        let override_bits = template.words[if column == 3 { 29 } else { 30 }];
        if f32::from_bits(override_bits) != 0.0 {
            override_bits
        } else {
            model[column]
        }
    });
    let speed_rates = base_speeds.map(|bits| {
        let value = f32::from_bits(bits);
        // Unit.cpp:8494 uses std::max with the selected rate as its first operand.
        (if value < 0.01 { 0.01 } else { value }).to_bits()
    });
    EntryFields {
        native_display: model[0],
        display: model[0],
        radius: (normalized * f32::from_bits(model[1])).to_bits(),
        reach: (normalized * f32::from_bits(model[2])).to_bits(),
        base_speeds,
        model_run_speed: model[4],
        speed_rates,
        gender: model[5] as u8,
        power: match template.words[36] {
            1 => 1,
            4 => 3,
            _ => 0,
        },
        equipment: 0,
        virtual_items: [0; 3],
        name: Vec::new(),
        cast_speed: 0,
        movement_flags: MOVEFLAG_WALK_MODE,
        default_movement: 0,
        damage_school: 0,
        capabilities: [false; 6],
        detection: 20f32.to_bits(),
        corpse_delay: 60,
        interaction_pause: 0,
        addon: CreatureAddonState::default(),
        respawn_radius: None,
        respawn_delay: None,
        respawn_time: None,
        respawn_clock: None,
        notify_delay: None,
        misc_flags: 0,
        level: None,
        faction: 0,
        gossip: 0,
        npc_flags: 0,
        attack_times: [0; 3],
        unit_flags: 0,
        dynamic_flags: 0,
        resistances: [0; 6],
        invisible: false,
        ignore_mmap: false,
        count_spawns: false,
        can_modify_stats: false,
        pvp: false,
        requested_spell_list: 0,
        spell_list_disabled: None,
        spell_list: None,
        immunities: CreatureImmunityState::default(),
        chain_immune: false,
        settings: SourceCreatureSettings::default(),
        control: FreshCreatureControl::default(),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn continue_entry(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    data: &EffectiveSpawnData,
    original: &LoadedCreatureInfo,
    template: &LoadedCreatureInfo,
    event: Option<CreatureEventData>,
    model: [u32; 8],
    terrain: &mut SourceTerrainManager,
    map: u32,
    objects: &mut SourceObjects,
) -> Result<()> {
    // Unit.cpp:330,429-430,8514-8531; MotionMaster.cpp:692-698 has no generators before AIM_Initialize.
    require(
        prefix
            .motions
            .get(&object)
            .is_some_and(SourceMotionMaster::is_empty),
        "execution.creature.initial_speed_motion_binding",
    )?;
    let mut fields = model_fields(template, model);
    prefix
        .lookups
        .push("creature_native_display_speed_gender_power");
    let spawn = prefix
        .spawn_templates
        .rows()
        .get(
            &data
                .spawn_template
                .ok_or_else(|| refuse("execution.creature.spawn_template_undefined"))?,
        )
        .ok_or_else(|| refuse("execution.creature.spawn_template_undefined"))?;
    let equipment = if let Some(event) = event {
        if event.equipment != 0 {
            Some((event.equipment, false))
        } else if event.entry != 0 {
            Some((template.words[93], true))
        } else {
            None
        }
    } else if spawn.equipment_id == -1 {
        Some((
            if template.words[93] == 0 {
                original.words[93]
            } else {
                template.words[93]
            },
            template.words[93] == 0,
        ))
    } else if spawn.equipment_id != 0 {
        Some((spawn.equipment_id as u32, false))
    } else {
        None
    };
    if equipment.is_some_and(|(id, _)| id != 0) {
        prefix.creature_entry = Some(fields);
        return Err(refuse("execution.creature.loaded_equipment_template_owner"));
    }
    // Creature.cpp:403 uses constructor settings, before ResetStaticFlags at :631.
    fields.name = original.strings[0].clone();
    fields.cast_speed = 1f32.to_bits();
    if template.words[17] & 4 != 0 {
        fields.movement_flags |= MOVEFLAG_LEVITATING;
    }
    prefix.lookups.push("creature_equipment_name_cast_levitate");
    if template.words[17] & 2 != 0 && template.words[23] & 0x8000 == 0 {
        // Creature.cpp:338 and Object.cpp:128-130 wrote this scale before ChooseDisplayId.
        let scale = prefix
            .init_entry_fields
            .ok_or_else(|| refuse("execution.creature.initial_object_scale_binding"))?
            .1;
        let result = prefix.collision_models.init_entry_swimming(
            fields.native_display,
            f32::from_bits(scale),
            &mut fields.movement_flags,
            |height| {
                terrain.is_swimmable(
                    map,
                    [data.row.pose[0], data.row.pose[1], data.row.pose[2]],
                    height,
                    None,
                )
            },
            &mut prefix.swimming_census,
        );
        if let Err(error) = result {
            prefix.creature_entry = Some(fields);
            return Err(error);
        }
    }
    if spawn.spawn_flags & 1 != 0 {
        fields.movement_flags &= !MOVEFLAG_WALK_MODE;
    }
    if spawn.spawn_flags & 2 != 0 {
        fields.movement_flags |= MOVEFLAG_HOVER;
    }
    if spawn.spawn_flags & 4 != 0 {
        fields.movement_flags |= MOVEFLAG_LEVITATING;
    }
    fields.default_movement = movement_enum(u32::from(data.row.movement))?;
    fields.damage_school = school_enum(template.words[62])?;
    let extra = template.words[23];
    fields.capabilities = [
        extra & 4 == 0,
        extra & 0x10 == 0,
        extra & 0x800000 != 0,
        extra & 0x80000 != 0,
        extra & 0x800 == 0,
        false,
    ];
    fields.detection = (template.words[31] as f32).to_bits();
    fields.corpse_delay = if template.words[88] != 0 {
        template.words[88]
    } else if prefix
        .encounters
        .by_credit()
        .get(&template.words[0])
        .is_some_and(|rows| {
            rows.iter()
                .any(|row| row.credit_type == 0 && row.entry.map == data.row.map)
        })
    {
        3600
    } else {
        uint_config(
            prefix.config,
            match template.words[37] {
                4 => "Corpse.Decay.RARE",
                1 => "Corpse.Decay.ELITE",
                2 => "Corpse.Decay.RAREELITE",
                3 => "Corpse.Decay.WORLDBOSS",
                _ => "Corpse.Decay.NORMAL",
            },
        )?
    };
    fields.interaction_pause = if template.words[87] as i32 != -1 {
        template.words[87]
    } else {
        uint_config(prefix.config, "InteractionPauseTimer")?
    };
    prefix.lookups.push("creature_init_entry_remainder");
    fields.addon.sheath = 1;
    fields.misc_flags = 0x10;
    let level =
        draw_source_creature_level([template.words[3], template.words[4]], Some(prefix.random))?;
    let mut stats =
        prefix
            .level_definitions
            .stats(template.words[0], level, false, prefix.level_config)?;
    stats.apply_spawn(
        spawn.cur_health,
        spawn.cur_mana,
        true,
        template.words[36],
        template.words[18],
    );
    fields.level = Some(stats);
    prefix.lookups.push("creature_level_stats_spawn");
    fields.faction = if spawn.faction != 0 {
        spawn.faction
    } else {
        template.words[13]
    };
    fields.gossip = template.words[86];
    fields.npc_flags = if spawn.npc_flags == -1 {
        template.words[20]
    } else {
        spawn.npc_flags as u32
    };
    fields.attack_times = [template.words[60], template.words[60], template.words[61]];
    fields.unit_flags = if spawn.unit_flags == -1 {
        template.words[21]
    } else {
        spawn.unit_flags as u32
    };
    fields.dynamic_flags = template.words[22];
    fields.resistances = template.words[72..78].try_into().unwrap();
    fields.invisible = extra & 0x80 != 0;
    fields.ignore_mmap = extra & 0x4000 != 0;
    fields.count_spawns = extra & 0x200000 != 0;
    if template.words[37] == 3 {
        prefix.creature_entry = Some(fields);
        return Err(refuse(
            "execution.creature.loaded_world_boss_immunity_owner",
        ));
    }
    fields.can_modify_stats = true;
    fields.pvp = prefix
        .faction_flags
        .get(&fields.faction)
        .is_some_and(|flags| flags & 0x1000 != 0);
    prefix.lookups.push("creature_update_entry_flags_stats_pvp");
    let key = spell_list_key(template.words[0], template.words[89]).unwrap_or(0);
    fields.requested_spell_list = key;
    if key == 0 {
        fields.spell_list_disabled = Some(true);
        prefix.lookups.push("creature_spell_list_disabled_zero");
    } else {
        let map = prefix
            .creature_spells
            .ok_or_else(|| refuse("execution.creature.loaded_spell_list_store_owner"))?;
        prefix.lookups.push("creature_map_spell_list_lookup");
        if let Some(list) = map.get(key) {
            let mut live = list.clone();
            // The source copy is private to this creature; Availability draws use its map stream.
            live.filter_availability(prefix.random)?;
            fields.spell_list_disabled = Some(live.disabled);
            fields.spell_list = Some(live);
            prefix.lookups.push("creature_spell_list_copy_availability");
        }
    }
    prefix.lookups.push("creature_immunity_set_lookup");
    fields
        .immunities
        .update(template.words[0], 0, Some(prefix.creature_immunities))?;
    prefix.lookups.push("creature_immunity_set_updated");
    fields.chain_immune = template.words[16] == 8; // Creature.h:624, Unit.h:2073.
    prefix.lookups.push("creature_critter_chain_immunity");
    if event.is_some() {
        prefix.creature_entry = Some(fields);
        return Err(refuse("execution.creature.loaded_game_event_spells_owner"));
    }
    if template.words[91] != 0 || template.words[92] != 0 || spawn.string_id != 0 {
        prefix.creature_entry = Some(fields);
        return Err(refuse("execution.creature.loaded_string_id_owner"));
    }
    prefix.lookups.push("creature_event_and_string_gates");
    // Creature.cpp:631 writes all settings before any effects, CreatureSettings.cpp:31-49.
    let result = fields
        .settings
        .reset(template.words[25..29].try_into().unwrap(), |application| {
            match application {
                CreatureStaticFlagApplication::Visibility(distance) => {
                    objects.set_visibility_distance_override(object, distance)?
                }
                CreatureStaticFlagApplication::UnitFlag(mask) => fields.unit_flags |= mask,
                CreatureStaticFlagApplication::ImmuneToPlayer
                | CreatureStaticFlagApplication::ImmuneToNpc => {
                    let mask = if application == CreatureStaticFlagApplication::ImmuneToPlayer {
                        0x100
                    } else {
                        0x200
                    };
                    fields.control.set_immune(
                        mask,
                        &mut fields.unit_flags,
                        objects.get(object)?.in_world,
                        |kind, guid| {
                            let Some(map) = objects.get(object)?.map else {
                                return Ok(None);
                            };
                            let found = objects.lookup_world_object(map, guid).filter(|&handle| {
                                let kind_found = objects.get(handle).unwrap().kind;
                                if matches!(
                                    kind,
                                    ControlledLookup::Pet | ControlledLookup::Guardian
                                ) {
                                    kind_found == SourceObjectKind::Pet
                                } else {
                                    matches!(
                                        kind_found,
                                        SourceObjectKind::Player
                                            | SourceObjectKind::Creature
                                            | SourceObjectKind::Pet
                                    )
                                }
                            });
                            if kind == ControlledLookup::Totem && found.is_some() {
                                return Err(refuse(
                                    "execution.creature_settings.Unit_6854.totem_fields_owner",
                                ));
                            }
                            Ok(found)
                        },
                        |_, mask| {
                            Err(refuse(if mask == 0x100 {
                                "execution.creature_settings.Unit_10704.controlled_flags_owner"
                            } else {
                                "execution.creature_settings.Unit_10685.controlled_flags_owner"
                            }))
                        },
                    )?;
                }
            }
            Ok(())
        });
    prefix.creature_entry = Some(fields);
    result?;
    prefix.lookups.push("creature_static_flags_reset");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_display_uses_loaded_gender_normalized_scale_and_template_speed_overrides() {
        // Creature.cpp:360-385, Unit.cpp:10262-10274, Creature.cpp:2888-2917.
        let mut template = SqlTemplateRow {
            ordinal: 0,
            words: [0; 99],
            strings: std::array::from_fn(|_| Vec::new()),
        };
        template.words[14] = 2f32.to_bits();
        template.words[29] = 0.5f32.to_bits();
        template.words[36] = 4;
        let fields = model_fields(
            &template,
            [
                1192,
                1.5f32.to_bits(),
                2f32.to_bits(),
                1f32.to_bits(),
                1.25f32.to_bits(),
                1,
                0,
                0,
            ],
        );
        assert_eq!(
            (
                fields.native_display,
                fields.display,
                fields.gender,
                fields.power
            ),
            (1192, 1192, 1, 3)
        );
        assert_eq!(
            (fields.radius, fields.reach),
            (1.5f32.to_bits(), 2f32.to_bits())
        );
        assert_eq!(fields.base_speeds, [0.5f32.to_bits(), 1.25f32.to_bits()]);
        assert_eq!(fields.speed_rates, fields.base_speeds);
        assert_eq!(fields.virtual_items, [0; 3]);
    }
    #[test]
    fn entry_enum_conversions_accept_representable_non_enumerators_and_refuse_undefined_values() {
        // MotionMaster.h:49-79 has enum range 0..31; SharedDefines.h:169-180 has 0..7.
        assert_eq!(movement_enum(31).unwrap(), 31);
        assert_eq!(
            movement_enum(32).unwrap_err().id,
            "execution.creature.Creature_453.movement_enum_undefined"
        );
        assert_eq!(school_enum(7).unwrap(), 7);
        assert_eq!(
            school_enum(8).unwrap_err().id,
            "execution.creature.Creature_458.school_enum_undefined"
        );
    }
    #[test]
    fn spell_list_zero_branch_and_legacy_key_preserve_source_wrapping_multiplication() {
        // Creature.cpp:609-612,2428-2439 disables only zero, before the map lookup.
        assert_eq!(spell_list_key(4692, 0), Some(469200));
        assert_eq!(spell_list_key(4692, 7), Some(7));
        assert_eq!(spell_list_key(0, 0), None);
        assert_eq!(spell_list_key(0x40000000, 0), None);
    }
}
