// SPDX-License-Identifier: GPL-3.0-or-later
//! Binds live map owners plus the two observed Creature.h:801 clock inputs.
use super::*;

struct SolenorBinding<'a, D> {
    definitions: &'a mut D,
    sets: &'a mut GridSpawnSets,
    geometry: &'a mut SourceTerrainManager,
    cells: &'a mut SourceCells,
    groups: &'a SourceSpawnGroups,
    waypoint_paths: &'a std::rc::Rc<SourceWaypointPaths>,
    spell_stores: &'a SourceSpellStores,
    passengers: &'a mut SourcePassengers,
    spline_thread: &'a mut SourceSplineThread,
    clocks: &'a mut ActiveClocks,
    motions: &'a mut BTreeMap<ObjectHandle, SourceMotionMaster>,
    visibility: &'a mut SourceVisibilityCensus,
    config: SourceUnitRelocationConfig,
    lookups: &'a mut Vec<&'static str>,
    thread: u64,
}

pub(in super::super) fn with_solenor_binding<D: GridDefinitions>(
    prefix: &mut Prefix<'_>,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    invoke: impl FnOnce(
        &mut SourceObjects,
        BuiltinAiFactoryOwners<'_>,
        &mut dyn SolenorResetOwner,
    ) -> Result<()>,
    template: &CreatureAiTemplateOperands,
) -> Result<()> {
    let reset = &mut SolenorBinding {
        definitions: context.definitions,
        sets: context.sets,
        geometry: context.products,
        cells: context.cells,
        groups: prefix.groups,
        waypoint_paths: &prefix.waypoint_paths,
        spell_stores: prefix.spell_stores,
        passengers: prefix.passengers,
        spline_thread: prefix.spline_thread,
        clocks: prefix.clocks,
        motions: &mut prefix.motions,
        visibility: &mut prefix.visibility,
        config: prefix
            .relocation_config
            .ok_or_else(|| refuse("execution.relocation.World_743.config_identity"))?,
        lookups: &mut prefix.lookups,
        thread: prefix.thread,
    };
    invoke(
        context.objects,
        BuiltinAiFactoryOwners {
            units: &mut prefix.units,
            ais: &mut prefix.ais,
            random: prefix.random,
            base: BuiltinAiBase::Loaded {
                combat: &mut prefix.combat,
                config: prefix.ai_config,
                template,
            },
        },
        reset,
    )
}

impl<D: GridDefinitions> SolenorBinding<'_, D> {
    fn motion(
        &mut self,
        unit: ObjectHandle,
        context: &mut MotionContext<'_>,
        invoke: impl FnOnce(
            &mut SourceMotionMaster,
            &mut MotionContext<'_>,
            &mut dyn MotionExecutionOwner,
        ) -> Result<()>,
    ) -> Result<()> {
        let mut owner = MotionOwner {
            objects: context.objects,
            geometry: self.geometry,
            passengers: self.passengers,
            spline_thread: self.spline_thread,
            thread: self.thread,
            groups: self.groups,
            waypoint_paths: self.waypoint_paths.clone(),
            spell_stores: self.spell_stores,
            map: self.cells.map(),
            lookups: self.lookups,
            broadcast: Some((self.cells, self.config.map_distance_bits)),
        };
        let master = self
            .motions
            .get_mut(&unit)
            .ok_or_else(|| refuse("execution.ai.silithus.MotionMaster_owner"))?;
        invoke(master, context, &mut owner)
    }
}
impl<D: GridDefinitions> SolenorResetOwner for SolenorBinding<'_, D> {
    fn solenor_respawn_clock(
        &mut self,
        _: ObjectHandle,
        random: &dyn CreatureRandom,
    ) -> Result<ClockSample> {
        let clock = self
            .clocks
            .solenor_respawn(self.thread, random.creature_raw_position()?)?;
        Ok(ClockSample::authored(ClockKind::WallSeconds, clock.seconds))
    }
    fn current_motion_type(&self, unit: ObjectHandle) -> Result<u32> {
        self.motions
            .get(&unit)
            .map(SourceMotionMaster::current_type)
            .ok_or_else(|| refuse("execution.ai.silithus_88.GetCurrentMovementGeneratorType_owner"))
    }
    fn reinitialize_motion(
        &mut self,
        unit: ObjectHandle,
        context: &mut MotionContext<'_>,
    ) -> Result<()> {
        let movement = context
            .units
            .get(context.objects, unit)?
            .loaded_creature
            .default_movement;
        self.motion(unit, context, |master, context, owner| {
            master.default.movement_type = movement;
            master.initialize(context, owner)
        })
    }
}
impl<D: GridDefinitions> NearTeleportOwner for SolenorBinding<'_, D> {
    fn teleport_motion(
        &mut self,
        unit: ObjectHandle,
        operation: NearTeleportMotion,
        context: &mut MotionContext<'_>,
    ) -> Result<()> {
        let live = context.units.get(context.objects, unit)?;
        let check = if matches!(operation, NearTeleportMotion::Interrupt) {
            assert!(live.movespline.finalized());
            assert_ne!(live.state & WANDER_MOVING, 0);
            assert_eq!(live.state & 0x0444_0000, 0);
            assert_eq!(
                live.movement.as_ref().unwrap().flags.unwrap() & 0x0040_0101,
                0x100
            );
            assert_eq!(live.loaded_creature.npc_flags, 1);
            Some((
                context.objects.pose(unit)?,
                live.spline_outbound.len(),
                live.spline_last_move_ms,
            ))
        } else {
            None
        };
        self.motion(unit, context, |master, context, owner| {
            operation.apply(master, context, owner)
        })?;
        if let Some((pose, packets, clock)) = check {
            let live = context.units.get(context.objects, unit)?;
            assert_eq!(context.objects.pose(unit)?, pose);
            assert_eq!(
                self.motions[&unit].reset_position()?,
                Some(pose.0.map(f32::from_bits))
            );
            assert_eq!(
                (live.spline_outbound.len(), live.spline_last_move_ms),
                (packets, clock)
            );
            assert_eq!(live.state & WANDER_MOVING, 0);
        }
        Ok(())
    }
    fn teleport_relocate(
        &mut self,
        unit: ObjectHandle,
        pose: SourcePose,
        objects: &mut SourceObjects,
        units: &mut SourceUnits,
    ) -> Result<()> {
        let before = units.get(objects, unit)?.unit_events.inventory();
        let mut context = GridFactoryContext {
            definitions: self.definitions,
            products: self.geometry,
            sets: self.sets,
            objects,
            cells: self.cells,
        };
        context.move_creature(
            unit,
            pose,
            &mut SourceLoadedRelocation {
                units,
                config: self.config,
                visibility: self.visibility,
            },
        )?;
        let after = units.get(context.objects, unit)?.unit_events.inventory();
        assert_eq!(before, after);
        // silithus.cpp:87 and GridDefines.h:147-166 select cell (280,562).
        let destination = context.cells.cell(pose.cell()?)?;
        assert_eq!((destination.x, destination.y), (280, 562));
        assert_eq!((destination.grid.x, destination.grid.y), (17, 35));
        assert_eq!(context.cells.registered_cell(unit), Some(destination));
        assert_eq!(
            context.objects.viewpoint_cell(unit)?,
            context.cells.registered_cell(unit)
        );
        Ok(())
    }
    fn teleport_broadcast(
        &mut self,
        unit: ObjectHandle,
        packet: &SourceMovementPacket,
        context: &MotionContext<'_>,
    ) -> Result<()> {
        let receipt = context.objects.message_broadcast(
            unit,
            self.cells,
            self.config.map_distance_bits,
            context.units.get(context.objects, unit)?.bounding_radius,
        )?;
        assert!(matches!(
            receipt,
            SourceBroadcast::Visited { loaded_cells: 1.. }
        ));
        assert_eq!(packet.opcode, 0x0ee);
        self.lookups
            .push("creature_solenor_heartbeat_no_recipients");
        Ok(())
    }
}
