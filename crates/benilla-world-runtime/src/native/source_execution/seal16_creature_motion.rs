// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:882; MotionMaster.cpp:68-94; RandomMovementGenerator.cpp:25-48.
use super::*;

struct MotionOwner<'a> {
    geometry: &'a mut SourceTerrainManager,
    flags: u32,
    charmer: Option<ObjectHandle>,
}
impl MotionExecutionOwner for MotionOwner<'_> {
    fn pathfinder_geometry(&mut self) -> Result<&mut SourceTerrainManager> {
        Ok(self.geometry)
    }
    fn client_controlling(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        // Unit.cpp:6528-6548. A fresh ordinary creature has no possessing charmer.
        if self.flags & 8 != 0 && self.charmer.is_some() && self.flags & 0x0100_0000 != 0 {
            return Err(refuse("execution.creature.Unit_6536.GetCharmer_owner"));
        }
        Ok(None)
    }
}
pub(super) fn initialize<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    template: &LoadedCreatureInfo,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<()> {
    let fields = prefix.creature_entry.as_mut().unwrap();
    let mut live = SourceUnitFields::loaded_creature(template);
    live.flags = fields.unit_flags;
    live.faction = fields.faction;
    live.static_flags = fields.settings.flags;
    live.linking_event_trigger = Some(prefix.creature_linking_trigger);
    let mut owner = MotionOwner {
        geometry: context.products,
        flags: live.flags,
        charmer: live.charmer,
    };
    let mut units = SourceUnits::default();
    units.register(context.objects, object, live)?;
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
            units: &mut units,
        },
        &mut owner,
    );
    fields.motion_state = Some(units.get(context.objects, object)?.state);
    prefix.motions.insert(object, master);
    result
}
