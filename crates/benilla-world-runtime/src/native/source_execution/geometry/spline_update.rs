// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Unit.cpp:9908-9935,11000-11099.
use super::*;
use benilla_sim::source_spline::{FINAL_ANGLE, FINAL_POINT, FINAL_TARGET};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplineRelocation {
    pub unit: ObjectHandle,
    pub pose: SourcePose,
    pub relocate_only: bool,
}
impl SplineRelocation {
    /// The ordinary creature branch uses F3's actual relocation and cell owner.
    pub fn creature<D, P, F>(
        self,
        context: &mut GridFactoryContext<'_, D, P>,
        owner: &mut F,
    ) -> Result<()>
    where
        D: GridDefinitions,
        P: GridProductOwner,
        F: ActiveRelocationOwner<D, P>,
    {
        if self.relocate_only {
            context.objects.set_pose(self.unit, self.pose)
        } else {
            context
                .move_creature(self.unit, self.pose, owner)
                .map(|_| ())
        }
    }
}
/// Unit.cpp:11032-11089. Transport data precedes facing and the map clock.
pub fn source_update_spline_position(
    unit: ObjectHandle,
    relocate_only: bool,
    context: &mut MotionContext<'_>,
    owner: &mut (impl MotionExecutionOwner + ?Sized),
) -> Result<()> {
    let spline = context.spline(unit)?;
    let mut pose = spline
        .compute_position()
        .map_err(super::splines::spline_error)?;
    let flags = spline.flags();
    let facing = spline.facing_words();
    let facing = |index: usize| {
        facing[index].ok_or_else(|| refuse("execution.spline.facing_union_read_undefined"))
    };
    if let Some(transport) = owner.pathfinder_passengers()?.transport(unit)? {
        owner.spline_transport_offset(unit, SourcePose(pose))?;
        pose = benilla_sim::source_transports::passenger_position(
            pose,
            context.objects.pose(transport)?.0,
        )
        .map_err(|_| refuse("execution.spline.passenger_position_numeric_domain_11039"))?;
    }
    let mut faced = false;
    if flags & FINAL_TARGET != 0 {
        let target = SourceGuid::from_raw(u64::from(facing(0)?) | (u64::from(facing(1)?) << 32));
        let map = context
            .objects
            .get(unit)?
            .map
            .ok_or_else(|| refuse("execution.spline.map_owner_11047"))?;
        if let Some(target) = context.objects.lookup_world_object(map, target) {
            let point = context.objects.pose(target)?.0.map(f32::from_bits);
            pose[3] = owner
                .spline_get_angle(unit, [point[0], point[1]], context)?
                .to_bits();
            faced = true;
        }
    } else if flags & FINAL_POINT != 0 {
        pose[3] = owner
            .spline_get_angle(unit, [facing(0)?, facing(1)?].map(f32::from_bits), context)?
            .to_bits();
        faced = true;
    } else if flags & FINAL_ANGLE != 0 {
        pose[3] = facing(0)?;
        faced = true;
    }
    if !faced {
        pose[3] =
            benilla_sim::source_spline::relocation_orientation(context.objects.pose(unit)?.0, pose)
                .map_err(super::splines::spline_error)?;
    }
    let now = owner.spline_current_clock_ms(unit, context)?;
    context
        .units
        .get_mut(context.objects, unit)?
        .spline_last_move_ms = now;
    if !relocate_only && context.objects.get(unit)?.kind == SourceObjectKind::Player {
        return Err(refuse("execution.spline.player_set_position_owner_11086"));
    }
    owner.spline_relocate(
        SplineRelocation {
            unit,
            pose: SourcePose(pose),
            relocate_only,
        },
        context,
    )
}
/// Unit.cpp:11091-11095. Clear the flags before marking the spline done.
pub fn source_disable_spline(fields: &mut SourceUnitFields) -> Result<()> {
    let flags = fields
        .movement
        .as_mut()
        .ok_or_else(|| refuse("execution.spline.disable_movement_info_owner_11094"))?
        .flags
        .as_mut()
        .ok_or_else(|| refuse("execution.spline.disable_movement_flags_owner_11094"))?;
    *flags &= !0x0040_0001;
    fields.movespline.interrupt();
    Ok(())
}

/// Finalization disables movement before relocation, then MotionMaster runs in Unit::Update.
pub fn source_update_spline_movement(
    unit: ObjectHandle,
    diff: u32,
    context: &mut MotionContext<'_>,
    owner: &mut (impl MotionExecutionOwner + ?Sized),
) -> Result<()> {
    if context.spline_finalized(unit)? {
        return Ok(());
    }
    let fields = context.units.get_mut(context.objects, unit)?;
    fields
        .movespline
        .update_state(diff, |_| {})
        .map_err(super::splines::spline_error)?;
    let arrived = fields.movespline.finalized();
    if arrived {
        source_disable_spline(fields)?;
    }
    fields.spline_position_expiry = fields.spline_position_expiry.saturating_sub(diff);
    if fields.spline_position_expiry == 0 || arrived {
        fields.spline_position_expiry = 400;
        source_update_spline_position(unit, false, context, owner)?;
    }
    Ok(())
}
pub fn source_stop_moving(
    unit: ObjectHandle,
    force_send: bool,
    context: &mut MotionContext<'_>,
    owner: &mut (impl MotionExecutionOwner + ?Sized),
) -> Result<()> {
    let fields = context.units.get_mut(context.objects, unit)?;
    if fields.state & 0x0055_0000 == 0 && !force_send {
        return Ok(());
    }
    fields.state &= !0x0055_0000;
    if !context.objects.get(unit)?.in_world {
        return Ok(());
    }
    SourceMoveSplineInit::construct(unit, context)?.stop(unit, force_send, context, owner)
}
pub fn source_interrupt_moving(
    unit: ObjectHandle,
    force_send: bool,
    context: &mut MotionContext<'_>,
    owner: &mut (impl MotionExecutionOwner + ?Sized),
) -> Result<()> {
    let moving = !context.spline_finalized(unit)?;
    if moving {
        source_update_spline_position(unit, true, context, owner)?;
        context
            .units
            .get_mut(context.objects, unit)?
            .movespline
            .interrupt();
    }
    source_stop_moving(unit, force_send || moving, context, owner)
}
