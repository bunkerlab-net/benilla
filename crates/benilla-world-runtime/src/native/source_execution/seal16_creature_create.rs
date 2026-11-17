// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:906-934,1687-1740; Unit.cpp:9389-9395,10957-10970.
use super::*;
pub(super) fn continue_create<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    data: &EffectiveSpawnData,
    template: &LoadedCreatureInfo,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<GridCandidateOutcome> {
    // CreatureCreatePos has no close object, and staticSpawn is true, Creature.cpp:105-120.
    let position = data.row.pose;
    prefix.lookups.push("creature_select_final_point_static");
    context
        .objects
        .set_pose(object, SourcePose(position.map(f32::to_bits)))?;
    if !creatures::valid_position(position) {
        context.objects.destroy(object, context.cells)?;
        return Ok(GridCandidateOutcome::FailedAndDestroyed(object));
    }
    prefix
        .lookups
        .push("creature_final_position_relocated_valid");
    let battleground = prefix.map_entry.battleground();
    prefix.creature_battleground = Some(battleground);
    prefix.lookups.push("creature_create_battleground");
    if battleground {
        return Err(refuse(
            "execution.creature.loaded_battleground_create_owner",
        ));
    }
    if prefix.map_entry.id == 0 && data.row.guid == 636 {
        let current = prefix.creature_entry.as_ref().unwrap();
        assert_eq!((template.words[0], current.equipment), (14508, 14508));
        assert_eq!(current.virtual_items, [7420, 0, 0]);
        assert_eq!(current.virtual_item_info, [[0x0d010702, 3], [0, 0], [0, 0]]);
        assert_ne!(template.words[17] & 2, 0);
        assert_eq!(template.words[23] & 0x8000, 0);
        assert_eq!(prefix.swimming_census.reached_441, 1);
        assert_eq!(prefix.swimming_census.swimming_not_set, 1);
        assert_eq!(
            current.movement_flags & benilla_sim::creature_capabilities::MOVEFLAG_SWIMMING,
            0
        );
        assert!(prefix.lookups.contains(&"creature_init_entry_remainder"));
        assert!(prefix.lookups.ends_with(&[
            "creature_final_position_relocated_valid",
            "creature_create_battleground",
        ]));
        assert_eq!(
            (prefix.creature_battleground, prefix.creature_zone),
            (Some(false), None)
        );
    }
    let location = context
        .products
        .zone_and_area(prefix.map_entry.id, position[..3].try_into().unwrap())?;
    prefix.zone_lookups.insert(
        SpawnKey {
            kind: GridSpawnKind::Creature,
            guid: data.row.guid,
        },
        location,
    );
    let zone = location[0];
    prefix.creature_zone = Some(zone);
    prefix.lookups.push("creature_create_zone");
    prefix.lookups.push("creature_outdoor_pvp_get_script");
    if prefix.outdoor_pvp.get_script(zone)?.is_some() {
        return Err(refuse("execution.creature.loaded_outdoor_pvp_create_owner"));
    }
    prefix.lookups.push("creature_instance_data");
    if let Some(script) = prefix.instance_data.as_mut() {
        prefix.lookups.push("creature_instance_on_creature_create");
        script.creature_create(
            template.words[0],
            context
                .objects
                .get(object)?
                .guid
                .ok_or_else(|| refuse("execution.object_guid_absent"))?
                .raw(),
        );
    }
    prefix.linking.ensure_loaded()?;
    prefix.lookups.push("creature_linking_slave_lookup");
    if prefix
        .linking
        .lookup(template.words[0], data.row.guid, prefix.map_entry.id)
        .is_some()
    {
        return Err(refuse(
            "execution.creature.loaded_linking_slave_holder_owner",
        )); // Creature.cpp:925.
    }
    prefix.lookups.push("creature_linking_trigger_lookup");
    if prefix
        .linking
        .event_trigger(template.words[0], data.row.guid, prefix.map_entry.id)
    {
        prefix.creature_linking_trigger = true; // Creature.cpp:928, before AddMasterToHolder.
        return Err(refuse(
            "execution.creature.loaded_linking_master_holder_owner",
        ));
    }
    prefix.lookups.push("creature_load_addon");
    let guid = context
        .objects
        .get(object)?
        .guid
        .ok_or_else(|| refuse("execution.object_guid_absent"))?;
    let addon = get_creature_addon(
        Some(prefix.creature_addons),
        if guid.high() == HighGuid::Pet as u16 {
            HighGuid::Pet
        } else {
            HighGuid::Creature
        },
        data.row.guid,
        template.words[0],
    )?;
    if guid.high() != HighGuid::Pet as u16 {
        prefix.lookups.push("creature_db_addon_lookup");
    }
    if guid.high() == HighGuid::Pet as u16
        || prefix
            .creature_addons
            .table_get(CreatureAddonTable::Guid, data.row.guid)
            .is_none()
    {
        prefix.lookups.push("creature_template_addon_lookup");
    }
    motion_initialize::register_unit(prefix, object, template, context)?;
    let fields = prefix.creature_entry.as_mut().unwrap();
    let live = prefix.units.get_mut(context.objects, object)?;
    if prefix.map_entry.id == 0 && data.row.guid == 40520 {
        assert!(live.auras.aura_holders.is_empty());
        assert!(live.auras.aura_types.is_empty());
        assert!(prefix.lookups.ends_with(&[
            "creature_load_addon",
            "creature_db_addon_lookup",
            "creature_template_addon_lookup",
        ]));
    }
    // Unit.cpp:330-366; Unit.h:2487,2507 construct the aura lists empty.
    // UpdateEntry has applied its immunity set, but has not cast a spell.
    let unit = SpellUnitState {
        player: false,
        gameobject: false,
        alive: true,
        standing: fields.addon.stand == 0,
        player_controlled: fields.unit_flags & 8 != 0,
        combat: fields.unit_flags & UNIT_FLAG_COMBAT != 0,
        battleground_wait_leave: false,
        gm: false,
        moving: fields.movement_flags & 0x0400_60cf != 0,
        falling_far: fields.movement_flags & 0x4000 != 0,
        stealth: live.auras.has_aura_type(16),
        form: 0,
        class: template.words[36],
        combo_target: None,
        mounted: fields.addon.mount != 0,
        taxi: fields.unit_flags & 0x0010_0000 != 0,
        totem: false, // Generic Creature subtype, Creature.h:595,622.
        untargetable: fields.unit_flags & 0x0001_0000 != 0,
        has_ai: false,
        position: context.objects.get(object)?.pose.unwrap().0,
    };
    fields.addon.load(
        addon,
        false,
        live,
        &mut addon_cast::Execution {
            object,
            objects: context.objects,
            terrain: context.products,
            stores: prefix.spell_stores,
            targets: &prefix.spell_targets,
            immunities: &fields.immunities,
            events: &mut fields.unit_events,
            starts: &mut fields.spell_starts,
            unit,
            level: fields.level.as_ref().unwrap().level,
            cast_speed: fields.cast_speed,
            clocks: prefix.clocks,
            raw: prefix.random.creature_raw_position()?,
            max_casts: addon_cast::max_casts(prefix.config)?,
        },
    )?;
    prefix.lookups.push("creature_addon_applied");
    prefix.lookups.push("creature_create_return");
    if prefix
        .groups
        .creature_group(context.objects.get(object)?.map.unwrap(), data.row.guid)?
        .is_some()
    {
        return Err(refuse(
            "execution.creature.Creature_1681.loaded_creature_group_owner",
        ));
    }
    context
        .objects
        .set_respawn_definition(object, SourcePose(position.map(f32::to_bits)), true)?;
    fields.respawn_radius = Some(data.row.radius.to_bits());
    prefix
        .motions
        .get_mut(&object)
        .unwrap()
        .bind_respawn_radius(data.row.radius);
    prefix.lookups.push("creature_respawn_coordinate_radius");
    let delay = prefix
        .random
        .creature_urand(data.row.respawn[0], data.row.respawn[1])?;
    fields.respawn_delay = Some(delay);
    prefix.lookups.push("creature_respawn_delay_draw");
    // Object.cpp:3076-3078 compares the actual allocated GUID counter.
    let new_spawning = data.row.guid != 0 && data.row.guid != guid.counter();
    if !new_spawning {
        fields.corpse_delay = fields.corpse_delay.min(delay.wrapping_mul(9) / 10);
    }
    context.objects.get_mut(object)?.alive = true; // Creature.cpp:1694.
    fields.respawn_time = Some(context.saved_respawn(GridSpawnKind::Creature, data.row.guid)?);
    prefix.lookups.push("creature_persistent_respawn_read");
    let clock = prefix
        .clocks
        .creature_respawn(prefix.thread, prefix.random.creature_raw_position()?)?;
    fields.respawn_clock = Some((clock.site_line, clock.time_line, clock.seconds));
    prefix.lookups.push("creature_respawn_calendar_consumed");
    if fields.respawn_time.unwrap() > clock.seconds {
        if new_spawning {
            context.objects.destroy(object, context.cells)?;
            return Ok(GridCandidateOutcome::FailedAndDestroyed(object));
        }
        context.objects.mark_dead(object)?;
        return Err(refuse("execution.creature.Creature_1705.SetHealth_owner"));
    }
    if fields.respawn_time != Some(0) {
        fields.respawn_time = Some(0); // Creature.cpp:1715, before the unbound save call.
        return Err(refuse(
            "execution.creature.Creature_1717.loaded_save_respawn_owner",
        ));
    }
    prefix.lookups.push("creature_spawned_by_linked_mob_lookup");
    // The Create lookup already selected the same loaded linking information.
    if prefix
        .linking
        .lookup(template.words[0], data.row.guid, prefix.map_entry.id)
        .is_some_and(|info| {
            info.flags & (LINKING_BOSS_DEAD | LINKING_BOSS_ALIVE) != 0
                && (info.master_guid != 0 || info.range != 0)
        })
    {
        return Err(refuse(
            "execution.creature.Creature_1723.loaded_linked_spawn_owner",
        ));
    }
    let map = context.objects.get(object)?.map.unwrap();
    let cell =
        benilla_sim::resident_map::compute_cell_pair(position[0].to_bits(), position[1].to_bits())
            .map_err(|_| refuse("execution.creature.map_add_coordinates"))?;
    if context.objects.get(object)?.active {
        context.load_grid(cell, GridLoadTrigger::Enter, prefix)?;
    } else {
        context.load_grid(cell, GridLoadTrigger::TerrainOnly, prefix)?;
    }
    let cell = context.cells.cell(cell)?;
    context.objects.link_cell(object, cell, context.cells)?;
    prefix.lookups.push("creature_map_add_grid_link");
    context.objects.register_world_lookup(object)?;
    prefix.lookups.push("creature_world_guid_registered");
    // WorldObject ctor :1170 starts notification false; UpdateEntry's string gates ran.
    context.objects.mark_in_world(object)?;
    prefix.lookups.push("creature_base_add_to_world");
    let fields = prefix.creature_entry.as_mut().unwrap();
    // Fresh ordinary Creature, no created-by spell or player controller, Unit.cpp:9393-9394.
    let (_, value) = super::super::super::super::spawn_loader_config::assignment(
        prefix.config,
        "CreatureRespawnAggroDelay",
    )?;
    let delay = value
        .parse::<i32>()
        .map_err(|_| refuse("execution.creature.loaded_notify_delay_config"))?;
    fields.notify_delay = Some(notify_delay(delay));
    fields
        .unit_events
        .schedule_ai_notify(fields.notify_delay.unwrap(), false)?;
    prefix.lookups.push("creature_schedule_ai_notify");
    require(
        context.objects.get(object)?.map == Some(map),
        "execution.creature.map_binding",
    )?;
    prefix.lookups.push("creature_ai_notify_enqueued");
    // World.cpp:485-494 was bound to the empty forced-map set in load_first.
    if template.words[23] & 0x1000 != 0 {
        context.set_active_object_state(object, true, prefix)?;
    }
    prefix.lookups.push("creature_force_active_checked");
    if prefix.creature_entry.as_ref().unwrap().count_spawns {
        return Err(refuse("execution.creature.Map_2995.AddToSpawnCount_owner"));
    }
    prefix.lookups.push("creature_spawn_count_checked");
    if context.objects.get(object)?.active {
        context.add_to_active(object, prefix)?;
    }
    prefix.lookups.push("creature_map_active_checked");
    context
        .objects
        .viewpoint_added_to_world(object, cell, context.cells, |_, _, _| {
            Err(refuse("execution.camera.Camera_104.added_to_world_owner"))
        })?;
    prefix.lookups.push("creature_map_viewpoint_added");
    context.objects.map_add_visibility(
        SourceVisibilityUpdate {
            object,
            map_distance_bits: prefix.map_visibility_bits,
            bounding_radius_bits: prefix.creature_entry.as_ref().unwrap().radius,
        },
        context.cells,
        &mut prefix.visibility,
        |_, _, _, _| {
            Err(refuse(
                "execution.visibility.GridNotifiers_36.camera_update_owner",
            ))
        },
        |_, _, _, _| Err(refuse("execution.visibility.Map_1348.player_update_owner")),
    )?;
    prefix.lookups.push("creature_map_visibility_complete");
    motion_initialize::initialize(prefix, object, template, context)?;
    prefix.lookups.push("creature_motion_initialized");
    // Creature.cpp:883 -> CreatureAISelector.cpp:42 -> ScriptDevAIMgr.cpp:365.
    let script = prefix.scripts.get_script(template.words[96])?;
    prefix.lookups.push("creature_ai_script_registry_lookup");
    if let Some(script) = script {
        let factory = script
            .slots
            .get("GetAI")
            .ok_or_else(|| refuse("execution.creature.ScriptDevAIMgr_367.GetAI_slot_owner"))?;
        if *factory {
            return Err(refuse("execution.creature.ScriptDevAIMgr_370.GetAI_owner"));
        }
    }
    prefix.lookups.push("creature_ai_script_null");
    // World.cpp:1381 -> CreatureAIRegistry.cpp:35-48. Reuse F5.01's identity and
    // std::map traversal, not a second list of factory names or scores.
    prefix.lookups.push("creature_ai_registry_bound");
    // Creature.h:621-623 and Unit.cpp:330,429: this DB-loaded Creature is not a Pet,
    // temporary summon or Totem and has no charmer or owner.
    let fields = prefix.creature_entry.as_ref().unwrap();
    let neutral = prefix.ai_factions.neutral_to_all(fields.faction);
    let selected = prefix.ai_registry.select_after_script(
        context.objects,
        object,
        CreatureAiInputs {
            effective_entry: template.words[0],
            ai_name: String::from_utf8(template.strings[2].clone())
                .map_err(|_| refuse("execution.creature.GetAIName_encoding_owner"))?,
            script_name: String::from_utf8(
                prefix
                    .scripts
                    .names()
                    .get(template.words[96] as usize)
                    .ok_or_else(|| refuse("execution.creature.ScriptName_index_owner"))?
                    .clone(),
            )
            .map_err(|_| refuse("execution.creature.ScriptName_encoding_owner"))?,
            control: CreatureAiControl::default(),
            guard: template.words[23] & 0x400 != 0,
            no_aggro_on_sight: template.words[23] & 2 != 0,
            neutral_to_all: neutral,
        },
    )?;
    prefix.lookups.push("creature_ai_selected");
    eprintln!(
        "F6_CREATURE_AI map={} entry={} input={:?} branch={:?} executable={:?}",
        prefix.map_entry.id,
        template.words[0],
        selected.inputs(),
        selected.branch(),
        selected.executable()
    );
    prefix.selected_ai = Some(selected);
    ai_initialize::initialize(prefix, object, context)?;
    if prefix.spawn_templates.rows()[&data.spawn_template.unwrap()].relay_id != 0 {
        return Err(refuse(
            "execution.creature.Creature_1743.ScriptsStart_owner",
        ));
    }
    prefix.lookups.push("creature_spawn_relay_checked");
    if prefix.creature_linking_trigger && context.objects.get(object)?.alive {
        return Err(refuse(
            "execution.creature.Creature_1747.DoCreatureLinkingEvent_owner",
        ));
    }
    if prefix.groups.creature_group(map, data.row.guid)?.is_some() {
        return Err(refuse(
            "execution.creature.Creature_1751.GetFormationData_owner",
        ));
    }
    prefix.lookups.push("creature_respawn_group_checked");
    let rabbit_day =
        super::super::super::super::spawn_loader_config::assignment(prefix.config, "RabbitDay")?
            .1
            .parse::<u32>()
            .map_err(|_| refuse("execution.creature.rabbit_day_config"))?;
    if context.objects.get(object)?.alive && rabbit_day != 0 {
        return Err(refuse("execution.creature.Creature_1761.localtime_owner"));
    }
    prefix.lookups.push("creature_load_from_db_return");
    Ok(GridCandidateOutcome::Loaded(object))
}

#[path = "seal16_addon_cast.rs"]
mod addon_cast;
#[path = "seal16_creature_ai.rs"]
mod ai_initialize;
#[path = "seal16_creature_motion.rs"]
mod motion_initialize;

fn notify_delay(value: i32) -> u32 {
    // World.cpp:704,2395-2402,2430-2437 resets negative uint32 config to its default.
    if value < 0 {
        5000
    } else {
        value as u32
    }
}

#[test]
fn notify_delay_uses_source_default_only_for_negative_values() {
    assert_eq!(notify_delay(-1), 5000);
    assert_eq!(notify_delay(0), 0);
    assert_eq!(notify_delay(5000), 5000);
    assert_eq!(notify_delay(i32::MAX), 2147483647);
}
