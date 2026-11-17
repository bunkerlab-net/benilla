// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:882; MotionMaster.cpp:68-94; RandomMovementGenerator.cpp:25-48.
use super::*;

struct MotionOwner<'a> {
    geometry: &'a mut SourceTerrainManager,
    passengers: &'a mut SourcePassengers,
    spline_thread: &'a mut SourceSplineThread,
    thread: u64,
    groups: &'a SourceSpawnGroups,
    waypoint_paths: std::rc::Rc<SourceWaypointPaths>,
    spell_stores: &'a SourceSpellStores,
    map: MapIdentity,
    database_guid: u32,
    lookups: &'a mut Vec<&'static str>,
    flags: u32,
    charmer: Option<ObjectHandle>,
}
impl MotionExecutionOwner for MotionOwner<'_> {
    fn spline_thread(&mut self) -> Result<&mut SourceSplineThread> {
        self.spline_thread.for_thread(self.thread)
    }
    fn pathfinder_passengers(&self) -> Result<&SourcePassengers> {
        Ok(self.passengers)
    }
    fn spline_passengers_mut(&mut self) -> Result<&mut SourcePassengers> {
        Ok(self.passengers)
    }
    fn waypoint_casting_for_movement(
        &mut self,
        unit: ObjectHandle,
        context: &MotionContext<'_>,
    ) -> Result<bool> {
        let slots = &context.units.get(context.objects, unit)?.spell_slots;
        let result = slots.is_non_melee_spell_casted(
            &(),
            self.spell_stores,
            false,
            false,
            true,
            true,
            false,
        )?;
        self.lookups.push("creature_live_spell_movement_query");
        Ok(result)
    }
    fn waypoint_paths(&mut self) -> Result<std::rc::Rc<SourceWaypointPaths>> {
        self.lookups.push("creature_loaded_waypoint_manager");
        Ok(self.waypoint_paths.clone())
    }
    fn pathfinder_geometry(&mut self) -> Result<&mut SourceTerrainManager> {
        Ok(self.geometry)
    }
    fn waypoint_formation_path(&mut self, _: ObjectHandle) -> Result<Option<u32>> {
        let path = self
            .groups
            .waypoint_formation_path(self.map, self.database_guid)?;
        self.lookups.push(if path.is_some() {
            "creature_waypoint_formation_path"
        } else {
            "creature_waypoint_default_path"
        });
        Ok(path)
    }
    fn client_controlling(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        // Unit.cpp:6528-6548. A fresh ordinary creature has no possessing charmer.
        if self.flags & 8 != 0 && self.charmer.is_some() && self.flags & 0x0100_0000 != 0 {
            return Err(refuse("execution.creature.Unit_6536.GetCharmer_owner"));
        }
        Ok(None)
    }
}
pub(super) fn register_unit<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    template: &LoadedCreatureInfo,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<()> {
    let fields = prefix.creature_entry.as_mut().unwrap();
    let live = prefix.units.get_mut(context.objects, object)?;
    live.populate_loaded_creature(template);
    live.unit_class = Some(template.words[36]);
    live.immunities = std::mem::take(&mut fields.immunities);
    live.flags = fields.unit_flags;
    live.faction = fields.faction;
    live.static_flags = fields.settings.flags;
    live.linking_event_trigger = Some(prefix.creature_linking_trigger);
    let level = fields.level.as_ref().unwrap();
    live.power_fields = Some(SourceUnitPowerFields {
        level: level.level,
        create_health: level.create_health,
        create_mana: level.create_mana,
        auxiliary_current: Some([level.power[1], level.power[2], level.power[4]]),
        auxiliary_maximum: Some([
            level.maximum_power[1],
            level.maximum_power[2],
            level.maximum_power[4],
        ]),
    });
    live.vitals = Some(SourceUnitVitals {
        health: level.health,
        max_health: level.maximum_health,
        mana: level.power[0],
        max_mana: level.maximum_power[0],
        has_mana: fields.power == 0,
        energy: level.power[3],
        max_energy: level.maximum_power[3],
    });
    live.can_aggro = Some(fields.can_aggro);
    // Creature.cpp:2888-2922,435-452; Unit.cpp:8439-8441. Retain InitEntry's
    // actual MovementInfo flags and model/template rates for BuildIntPath.
    let mut movement = SourceCreatureMovement::construct();
    movement.flags = Some(fields.movement_flags);
    for (rate, bits) in movement.rates.iter_mut().zip(fields.speed_rates) {
        *rate = Some(f32::from_bits(bits));
    }
    live.movement = Some(movement);
    Ok(())
}

pub(super) fn initialize<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    _template: &LoadedCreatureInfo,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<()> {
    let fields = prefix.creature_entry.as_mut().unwrap();
    let live = prefix.units.get(context.objects, object)?;
    let mut owner = MotionOwner {
        geometry: context.products,
        passengers: prefix.passengers,
        spline_thread: prefix.spline_thread,
        thread: prefix.thread,
        groups: prefix.groups,
        waypoint_paths: prefix.waypoint_paths.clone(),
        spell_stores: prefix.spell_stores,
        map: context.objects.get(object)?.map.unwrap(),
        database_guid: context.objects.get(object)?.database_guid,
        lookups: &mut prefix.lookups,
        flags: live.flags,
        charmer: live.charmer,
    };
    prefix.threats.register(object)?;
    // Only the empty constructor master exists before AIM_Initialize.
    require(
        prefix.motions.get(&object).unwrap().is_empty(),
        "execution.creature.initial_motion_empty_binding",
    )?;
    let mut master = SourceMotionMaster::new(
        object,
        MotionDefault {
            movement_type: fields.default_movement,
            default_path: 0, // MotionMaster.h:104; no SetDefaultPathId in Create/LoadFromDB.
        },
    );
    master.bind_respawn_radius(f32::from_bits(fields.respawn_radius.unwrap()));
    let result = master.initialize(
        &mut MotionContext {
            objects: context.objects,
            units: &mut prefix.units,
        },
        &mut owner,
    );
    fields.motion_state = Some(prefix.units.get(context.objects, object)?.state);
    prefix.motions.insert(object, master);
    result
}
