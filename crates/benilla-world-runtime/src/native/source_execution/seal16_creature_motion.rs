// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:882; MotionMaster.cpp:68-94; RandomMovementGenerator.cpp:25-48.
use super::*;

struct MotionOwner<'a> {
    objects: &'a SourceObjects,
    geometry: &'a mut SourceTerrainManager,
    passengers: &'a mut SourcePassengers,
    spline_thread: &'a mut SourceSplineThread,
    thread: u64,
    groups: &'a SourceSpawnGroups,
    waypoint_paths: std::rc::Rc<SourceWaypointPaths>,
    spell_stores: &'a SourceSpellStores,
    map: MapIdentity,
    lookups: &'a mut Vec<&'static str>,
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
    fn waypoint_formation_path(&mut self, unit: ObjectHandle) -> Result<Option<u32>> {
        let path = self
            .groups
            .waypoint_formation_path(self.map, self.objects.get(unit)?.database_guid)?;
        self.lookups.push(if path.is_some() {
            "creature_waypoint_formation_path"
        } else {
            "creature_waypoint_default_path"
        });
        Ok(path)
    }
    fn client_controlling(
        &mut self,
        unit: ObjectHandle,
        context: &MotionContext<'_>,
    ) -> Result<Option<ObjectHandle>> {
        context.units.possessing_client(context.objects, unit)
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
    let mut owner = MotionOwner {
        objects: context.objects,
        geometry: context.products,
        passengers: prefix.passengers,
        spline_thread: prefix.spline_thread,
        thread: prefix.thread,
        groups: prefix.groups,
        waypoint_paths: prefix.waypoint_paths.clone(),
        spell_stores: prefix.spell_stores,
        map: context.objects.get(object)?.map.unwrap(),
        lookups: &mut prefix.lookups,
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

struct LinkingRespawn<'a> {
    motion: MotionOwner<'a>,
    units: &'a mut SourceUnits,
    motions: &'a mut BTreeMap<ObjectHandle, SourceMotionMaster>,
    births: &'a BTreeMap<ObjectHandle, (u32, i64)>,
    follow: &'a mut Option<(ObjectHandle, ObjectHandle, [u32; 2])>,
}
impl LinkingEventOwner for LinkingRespawn<'_> {
    fn linking_creature(
        &mut self,
        map: MapIdentity,
        unit: ObjectHandle,
    ) -> Result<LinkingCreature> {
        let object = self.motion.objects.get(unit)?;
        require(object.map == Some(map), "execution.linking.object_map_kind")?;
        let live = self.units.get(self.motion.objects, unit)?;
        let pose = object
            .respawn_pose
            .ok_or_else(|| refuse("execution.linking.master_respawn_pose_owner"))?;
        let guid = object
            .guid
            .ok_or_else(|| refuse("execution.linking.guid_owner"))?;
        let &(respawn_delay, respawn_time) = self.births.get(&unit).ok_or_else(|| {
            refuse("execution.linking.CreatureLinkingMgr_596.respawn_fields_owner")
        })?;
        Ok(LinkingCreature {
            object: unit,
            entry: live.entry,
            database: object.database_guid,
            respawn_xy: [pose.0[0], pose.0[1]],
            new_spawning: source_creature_new_spawning(object.database_guid, guid.counter()),
            death: if object.alive {
                CreatureDeathState::Alive
            } else {
                CreatureDeathState::Dead
            },
            pet: object.kind == SourceObjectKind::Pet,
            controlled: live.flags & 8 != 0,
            combat: live.in_combat(),
            respawn_delay,
            respawn_time,
        })
    }
    fn linking_lookup(
        &mut self,
        map: MapIdentity,
        lookup: LinkingLookup,
    ) -> Result<Option<ObjectHandle>> {
        Ok(lookup.resolve(map, self.motion.objects))
    }
    fn linking_wall(&mut self) -> Result<ClockSample> {
        Err(refuse(
            "execution.linking.CreatureLinkingMgr_596.wall_clock_owner",
        ))
    }
    fn linking_schedule(
        &mut self,
        _: LinkingScheduleRequest,
        _: &mut SourceCreatureLinkingHolder,
    ) -> Result<()> {
        Err(refuse(
            "execution.linking.CreatureLinkingMgr_534.RespawnCreature_owner",
        ))
    }
    fn linking_action(
        &mut self,
        request: LinkingActionRequest,
        holder: &mut SourceCreatureLinkingHolder,
    ) -> Result<()> {
        let LinkingAction::Follow(master) = request.action else {
            return Err(request.action.missing_owner());
        };
        let unit = request.object;
        let objects = self.motion.objects;
        let offsets = holder.follow_offsets(objects, self.units, unit, master)?;
        let result = SourceMotionMaster::move_follow(
            unit,
            || {
                self.motions.get_mut(&unit).ok_or_else(|| {
                    refuse("execution.movement.CreatureLinkingMgr_637.GetMotionMaster_owner")
                })
            },
            &mut MotionContext {
                objects,
                units: self.units,
            },
            &mut self.motion,
        );
        if result
            .as_ref()
            .is_err_and(|err| err.id == FOLLOW_REFERENCE_ATTACH_OWNER)
        {
            *self.follow = Some((unit, master, offsets.map(f32::to_bits)));
        }
        result
    }
}

pub(super) fn linking_respawn<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<()> {
    let map = prefix.link_holder.map();
    prefix.link_holder.event(
        prefix.linking,
        &mut LinkingRespawn {
            motion: MotionOwner {
                objects: context.objects,
                geometry: context.products,
                passengers: prefix.passengers,
                spline_thread: prefix.spline_thread,
                thread: prefix.thread,
                groups: prefix.groups,
                waypoint_paths: prefix.waypoint_paths.clone(),
                spell_stores: prefix.spell_stores,
                map,
                lookups: &mut prefix.lookups,
            },
            units: &mut prefix.units,
            motions: &mut prefix.motions,
            births: &prefix.link_births,
            follow: &mut prefix.link_follow,
        },
        LinkingEvent::Respawn,
        object,
        None,
    )
}
