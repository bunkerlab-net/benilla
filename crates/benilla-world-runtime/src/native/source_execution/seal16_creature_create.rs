// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:906-934; first static, ordinary creature's real Create prefix.
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
    if guid.high() != HighGuid::Pet as u16 {
        prefix.lookups.push("creature_db_addon_lookup");
        return Err(refuse(CREATURE_NEXT_OWNER)); // Creature.cpp:2299 is GetCreatureAddon's first store read.
    }
    Err(refuse(
        "execution.creature.loaded_template_addon_store_owner",
    )) // Creature.cpp:2302.
}
