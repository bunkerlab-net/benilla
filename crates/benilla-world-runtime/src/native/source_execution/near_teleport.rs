// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

#[derive(Clone, Copy)]
pub enum NearTeleportMotion {
    Interrupt,
    Reset,
}

impl NearTeleportMotion {
    pub fn apply(
        self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        match self {
            Self::Interrupt => master.interrupt_for_teleport(context, owner),
            Self::Reset => master.reset_after_teleport(context, owner),
        }
    }

    fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::Interrupt => "execution.movement.Unit_10621.Interrupt_owner",
            Self::Reset => "execution.movement.Unit_10631.Reset_owner",
        })
    }
}

pub trait NearTeleportOwner {
    fn teleport_motion(
        &mut self,
        _unit: ObjectHandle,
        operation: NearTeleportMotion,
        _context: &mut MotionContext<'_>,
    ) -> Result<()> {
        Err(operation.missing_owner())
    }
    fn teleport_relocate(
        &mut self,
        _unit: ObjectHandle,
        _pose: SourcePose,
        _objects: &mut SourceObjects,
        _units: &mut SourceUnits,
    ) -> Result<()> {
        Err(refuse(
            "execution.movement.Unit_10623.CreatureRelocation_owner",
        ))
    }
    fn teleport_broadcast(
        &mut self,
        _unit: ObjectHandle,
        _packet: &SourceMovementPacket,
        _context: &MotionContext<'_>,
    ) -> Result<()> {
        Err(refuse(
            "execution.movement.Object_1920.Map_MessageBroadcast_owner",
        ))
    }
}
impl NearTeleportOwner for () {}

/// Unit.cpp:10604-10631. Relocation can replace the movement generator.
pub fn source_near_teleport(
    unit: ObjectHandle,
    pose: SourcePose,
    objects: &mut SourceObjects,
    units: &mut SourceUnits,
    owner: &mut (impl NearTeleportOwner + ?Sized),
) -> Result<()> {
    let kind = objects.get(unit)?.kind;
    require(
        kind != SourceObjectKind::Player,
        "execution.movement.Unit_10611.player_TeleportTo_owner",
    )?;
    source_disable_spline(units.get_mut(objects, unit)?)?;
    require(
        matches!(kind, SourceObjectKind::Creature | SourceObjectKind::Pet),
        "execution.movement.Unit_10617.creature_downcast_undefined",
    )?;
    owner.teleport_motion(
        unit,
        NearTeleportMotion::Interrupt,
        &mut MotionContext { objects, units },
    )?;
    owner.teleport_relocate(unit, pose, objects, units)?;
    let object = objects.get(unit)?;
    let guid = object
        .guid
        .ok_or_else(|| refuse("execution.movement.Unit_734.GetPackGUID_owner"))?;
    let movement = units
        .get(objects, unit)?
        .movement
        .as_ref()
        .ok_or_else(|| refuse("execution.movement.Unit_735.movement_info_owner"))?;
    let heartbeat = source_movement_heartbeat(guid, movement, objects.pose(unit)?)?;
    source_movement_send_to_set(object.in_world, heartbeat, |packet| {
        owner.teleport_broadcast(unit, packet, &MotionContext { objects, units })
    })?;
    owner.teleport_motion(
        unit,
        NearTeleportMotion::Reset,
        &mut MotionContext { objects, units },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_teleport_refuses_player_before_spline_and_creature_after_disabling_it() {
        let mut objects = SourceObjects::default();
        let player = objects.construct(1, SourceObjectKind::Player).unwrap();
        let creature = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let mut units = SourceUnits::default();
        let mut live = CreatureConstruction::construct_loaded_unit();
        live.movement.as_mut().unwrap().flags = Some(0x0040_0101);
        units.register(&objects, creature, live).unwrap();
        let pose = SourcePose([1f32.to_bits(); 4]);
        assert_eq!(
            source_near_teleport(player, pose, &mut objects, &mut units, &mut ())
                .unwrap_err()
                .id,
            "execution.movement.Unit_10611.player_TeleportTo_owner"
        );
        assert_eq!(
            source_near_teleport(creature, pose, &mut objects, &mut units, &mut ())
                .unwrap_err()
                .id,
            "execution.movement.Unit_10621.Interrupt_owner"
        );
        let live = units.get(&objects, creature).unwrap();
        assert!(live.movespline.finalized());
        assert_eq!(live.movement.as_ref().unwrap().flags, Some(0x100));
    }
}
