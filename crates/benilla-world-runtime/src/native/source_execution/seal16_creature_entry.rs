// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:353-612, Unit.cpp:10199-10275, fresh uncharmed non-pet construction.
use super::*;
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
    pub virtual_item_info: [[u32; 2]; 3],
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
    pub motion_state: Option<u32>,
    pub can_aggro: bool,
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
    pub chain_immune: bool,
    pub settings: SourceCreatureSettings,
}
fn uint_config(config: &[u8], key: CreatureEntryConfig) -> Result<u32> {
    use CreatureEntryConfig::*;
    let name = match key {
        NormalCorpseDecay => "Corpse.Decay.NORMAL",
        EliteCorpseDecay => "Corpse.Decay.ELITE",
        RareEliteCorpseDecay => "Corpse.Decay.RAREELITE",
        WorldBossCorpseDecay => "Corpse.Decay.WORLDBOSS",
        RareCorpseDecay => "Corpse.Decay.RARE",
        InteractionPause => "InteractionPauseTimer",
    };
    super::super::super::super::spawn_loader_config::assignment(config, name)?
        .1
        .parse::<i32>()
        .map(|value| value as u32)
        .map_err(|_| refuse("execution.creature.loaded_uint_config"))
}
impl EntryFields {
    fn observe(
        live: &SourceUnitFields,
        state: &CreatureConstruction,
        addon: CreatureAddonState,
        level: Option<benilla_sim::creature_init::SourceLevelStats>,
    ) -> Self {
        let entry = &live.loaded_creature;
        let movement = live.movement.as_ref().unwrap();
        Self {
            native_display: entry.native_display,
            display: entry.display,
            radius: live.bounding_radius.unwrap(),
            reach: live.combat_reach.unwrap(),
            base_speeds: entry.base_speeds,
            model_run_speed: entry.model_run_speed,
            speed_rates: std::array::from_fn(|kind| movement.rates[kind].unwrap().to_bits()),
            gender: entry.gender,
            power: live.power_type.unwrap(),
            equipment: entry.equipment.id,
            virtual_items: entry.equipment.slots.map(|slot| slot.display),
            virtual_item_info: entry.equipment.slots.map(|slot| slot.info),
            name: entry.name.clone(),
            cast_speed: live.cast_speed,
            movement_flags: movement.flags.unwrap(),
            default_movement: entry.default_movement,
            damage_school: entry.damage_school,
            capabilities: [
                entry.can_parry,
                entry.can_block,
                entry.can_dual_wield,
                live.force_attacking,
                live.can_call_assistance,
                entry.no_reputation,
            ],
            detection: entry.detection,
            corpse_delay: state.corpse_delay,
            interaction_pause: entry.interaction_pause,
            addon,
            respawn_radius: None,
            respawn_delay: None,
            respawn_time: None,
            respawn_clock: None,
            notify_delay: None,
            motion_state: None,
            can_aggro: live.can_aggro.unwrap(),
            misc_flags: entry.misc_flags,
            level,
            faction: live.faction,
            gossip: entry.gossip,
            npc_flags: entry.npc_flags,
            attack_times: entry.attack_times,
            unit_flags: live.flags,
            dynamic_flags: entry.dynamic_flags,
            resistances: entry.resistances,
            invisible: entry.invisible,
            ignore_mmap: entry.ignore_mmap,
            count_spawns: entry.count_spawns,
            can_modify_stats: entry.can_modify_stats,
            pvp: entry.pvp,
            requested_spell_list: 0,
            chain_immune: entry.chain_immune,
            settings: SourceCreatureSettings {
                flags: live.static_flags,
            },
        }
    }
}

pub(super) fn apply_model(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    template: &LoadedCreatureInfo,
    model: [u32; 8],
    objects: &SourceObjects,
) -> Result<()> {
    require(
        prefix
            .motions
            .get(&object)
            .is_some_and(SourceMotionMaster::is_empty),
        "execution.creature.initial_speed_motion_binding",
    )?;
    let live = prefix.units.get_mut(objects, object)?;
    CreatureConstruction::apply_loaded_model(live, template, model)?;
    // The first display has the template scale, Creature.cpp:338,366; Unit.cpp:10266.
    let native_scale = f32::from_bits(template.words[14]);
    let normalized = if native_scale != 0.0 {
        f32::from_bits(live.loaded_creature.scale.unwrap()) / native_scale
    } else {
        1.0
    };
    assert_eq!(
        live.combat_reach,
        Some((normalized * f32::from_bits(model[2])).to_bits())
    );
    prefix
        .lookups
        .push("creature_native_display_speed_gender_power");
    Ok(())
}

pub(super) fn finish_init_entry(
    prefix: &mut Prefix<'_>,
    state: &mut CreatureConstruction,
    entry: &LoadedCreatureEntry<'_>,
    terrain: &mut SourceTerrainManager,
    objects: &SourceObjects,
) -> Result<()> {
    let object = state.object;
    let live = prefix.units.get_mut(objects, object)?;
    let mut addon = CreatureAddonState::default();
    let result = state.load_entry_equipment(live, entry, prefix.equipment);
    prefix.equipment_writes.insert(
        state.spawn.database_guid,
        live.loaded_creature.equipment.clone(),
    );
    if let Err(error) = result {
        prefix.creature_entry = Some(EntryFields::observe(live, state, addon, None));
        return Err(error);
    }
    prefix.lookups.push("creature_equipment_name_cast_levitate");
    let [x, y, z, _] = state.spawn.position;
    let result = state.finish_loaded_init_entry(
        live,
        entry,
        prefix.encounters,
        |key| uint_config(prefix.config, key),
        |display, scale| {
            prefix.collision_models.init_entry_swimmable(
                display,
                scale,
                |height| terrain.is_swimmable(entry.map, [x, y, z], height, None),
                &mut prefix.swimming_census,
            )
        },
    );
    if let Err(error) = result {
        prefix.creature_entry = Some(EntryFields::observe(live, state, addon, None));
        return Err(error);
    }
    prefix.lookups.push("creature_init_entry_remainder");
    CreatureConstruction::begin_loaded_update_entry(live, &mut addon);
    prefix.creature_entry = Some(EntryFields::observe(live, state, addon, None));
    Ok(())
}

pub(super) fn apply_level(
    prefix: &mut Prefix<'_>,
    state: &CreatureConstruction,
    entry: &LoadedCreatureEntry<'_>,
    objects: &SourceObjects,
) -> Result<()> {
    let live = prefix.units.get_mut(objects, state.object)?;
    let stats =
        state.apply_loaded_level(live, entry, prefix.level_definitions, prefix.level_config)?;
    prefix.creature_entry.as_mut().unwrap().level = Some(stats);
    prefix.lookups.push("creature_level_stats_spawn");
    Ok(())
}

pub(super) fn update_entry(
    prefix: &mut Prefix<'_>,
    state: &CreatureConstruction,
    entry: &LoadedCreatureEntry<'_>,
    objects: &mut SourceObjects,
) -> Result<()> {
    let object = state.object;
    let template = entry.effective;
    let live = prefix.units.get_mut(objects, object)?;
    let result = CreatureConstruction::update_loaded_entry_flags(live, entry, prefix.faction_flags);
    let prior = prefix.creature_entry.take().unwrap();
    let mut fields = EntryFields::observe(live, state, prior.addon, prior.level);
    if let Err(error) = result {
        prefix.creature_entry = Some(fields);
        return Err(error);
    }
    prefix.lookups.push("creature_update_entry_flags_stats_pvp");
    let key = source_creature_spell_list_key(live.entry, template.words[89]).unwrap_or(0);
    fields.requested_spell_list = key;
    if key != 0 && prefix.creature_spells.is_some() {
        prefix.lookups.push("creature_map_spell_list_lookup");
    }
    if live.set_fresh_creature_spell_list(key, prefix.creature_spells, prefix.random)? {
        prefix.lookups.push(if key != 0 {
            "creature_spell_list_copy_availability"
        } else {
            "creature_spell_list_disabled_zero"
        });
    }
    prefix.lookups.push("creature_immunity_set_lookup");
    CreatureConstruction::update_loaded_entry_immunities(
        live,
        template,
        Some(prefix.creature_immunities),
    )?;
    prefix.lookups.push("creature_immunity_set_updated");
    fields.chain_immune = live.loaded_creature.chain_immune;
    prefix.lookups.push("creature_critter_chain_immunity");
    if let Err(error) = state.require_loaded_entry_callbacks(entry) {
        prefix.creature_entry = Some(fields);
        return Err(error);
    }
    prefix.lookups.push("creature_event_and_string_gates");
    let result = SourceCreatureSettings::reset_live(
        live,
        template.words[25..29].try_into().unwrap(),
        object,
        objects,
    );
    fields.settings.flags = live.static_flags;
    fields.unit_flags = live.flags;
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
        let mut live = SourceUnitFields::constructed_creature();
        CreatureConstruction::init_loaded_entry_base(&mut live, &template);
        CreatureConstruction::apply_loaded_model(
            &mut live,
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
        )
        .unwrap();
        let fields = &live.loaded_creature;
        assert_eq!(
            (
                fields.native_display,
                fields.display,
                fields.gender,
                live.power_type.unwrap()
            ),
            (1192, 1192, 1, 3)
        );
        assert_eq!(
            (live.bounding_radius.unwrap(), live.combat_reach.unwrap()),
            (1.5f32.to_bits(), 2f32.to_bits())
        );
        assert_eq!(fields.base_speeds, [0.5f32.to_bits(), 1.25f32.to_bits()]);
        assert_eq!(
            live.movement.unwrap().rates[..2],
            fields.base_speeds.map(|bits| Some(f32::from_bits(bits)))
        );
        assert_eq!(fields.equipment.slots.map(|slot| slot.display), [0; 3]);
    }
    #[test]
    fn entry_enum_conversions_accept_representable_non_enumerators_and_refuse_undefined_values() {
        // MotionMaster.h:49-79 has enum range 0..31; SharedDefines.h:169-180 has 0..7.
        use creatures::{movement_enum, school_enum};
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
        assert_eq!(source_creature_spell_list_key(4692, 0), Some(469200));
        assert_eq!(source_creature_spell_list_key(4692, 7), Some(7));
        assert_eq!(source_creature_spell_list_key(0, 0), None);
        assert_eq!(source_creature_spell_list_key(0x40000000, 0), None);
    }
}
