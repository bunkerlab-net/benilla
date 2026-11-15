// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:906-934,1687-1738; Unit.cpp:9389-9395,10959-10961.
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
    let zone = context
        .products
        .zone_and_area(prefix.map_entry.id, position[..3].try_into().unwrap())?[0];
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
    struct Execution;
    impl CreatureAddonExecutionOwner for Execution {}
    let fields = prefix.creature_entry.as_mut().unwrap();
    fields.addon.load(addon, false, &mut Execution)?;
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
    prefix.lookups.push("creature_schedule_ai_notify");
    require(
        context.objects.get(object)?.map == Some(map),
        "execution.creature.map_binding",
    )?;
    Err(refuse(CREATURE_NEXT_OWNER)) // Unit.cpp:10959-10961, ctor :360 starts m_AINotifyEvent null.
}

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
