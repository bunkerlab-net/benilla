// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Movement/MoveSplineInit.cpp:27-190.
use super::*;
use benilla_sim::source_spline::{self as spline, Args};

const WALK: u32 = 0x100;
const FORWARD_SPLINE: u32 = 0x0040_0001;
const ROOT: u32 = 0x0800_0000;
const RUNMODE: u32 = 0x100;
const FACING: u32 = spline::FINAL_POINT | spline::FINAL_TARGET | spline::FINAL_ANGLE;

/// One instance belongs to one recorded execution thread, not one creature or map.
#[derive(Debug, Serialize)]
pub struct SourceSplineThread {
    pub thread: u64,
    next: u32,
}
impl SourceSplineThread {
    pub fn new(thread: u64) -> Self {
        Self { thread, next: 1 }
    }
    /// MoveSplineInit.cpp:27,102,152. uint32 wraps, including through zero.
    pub fn take_id(&mut self) -> u32 {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        id
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SplineOutbound {
    pub unit: ObjectHandle,
    pub guid: SourceGuid,
    pub transport: Option<SourceGuid>,
    pub excluded: Option<SourceGuid>,
    /// SMSG_MONSTER_MOVE body after the packed Unit/transport GUIDs.
    pub movement: Vec<u8>,
}
#[derive(Debug)]
pub struct SourceMoveSplineInit {
    pub args: Args,
    pub slowed: f32,
}
fn movement<'a>(
    context: &'a MotionContext<'_>,
    unit: ObjectHandle,
) -> Result<&'a SourceCreatureMovement> {
    context
        .units
        .get(context.objects, unit)?
        .movement
        .as_ref()
        .ok_or_else(|| refuse("execution.spline.movement_info_owner_176"))
}
fn flags(context: &MotionContext<'_>, unit: ObjectHandle) -> Result<u32> {
    movement(context, unit)?
        .flags
        .ok_or_else(|| refuse("execution.spline.movement_flags_owner_176"))
}
fn write_flags(context: &mut MotionContext<'_>, unit: ObjectHandle, flags: u32) -> Result<()> {
    context
        .units
        .get_mut(context.objects, unit)?
        .movement
        .as_mut()
        .ok_or_else(|| refuse("execution.spline.movement_info_owner_104"))?
        .flags = Some(flags);
    Ok(())
}
impl SourceMoveSplineInit {
    pub fn construct(unit: ObjectHandle, context: &MotionContext<'_>) -> Result<Self> {
        let movement = flags(context, unit)?;
        Ok(Self {
            args: Args {
                path: Vec::with_capacity(16),
                facing: spline::facing_from_target(0),
                flags: if movement & WALK == 0 { RUNMODE } else { 0 }
                    | if movement & (0x0080_0000 | 0x4000_0000 | 0x800 | 0x400) != 0 {
                        spline::FLYING
                    } else {
                        0
                    },
                path_index_offset: 0,
                velocity: 0,
                spline_id: 0,
            },
            slowed: 0.,
        })
    }
    pub fn move_by_path(&mut self, path: &[[f32; 3]], offset: i32) {
        self.args.path_index_offset = offset;
        self.args.path.clear();
        self.args
            .path
            .extend(path.iter().map(|p| p.map(f32::to_bits)));
    }
    /// MoveSplineInit.h:127-132, the non-PathFinder overload.
    pub fn move_to(&mut self, destination: [f32; 3]) {
        self.args.path_index_offset = 0;
        self.args.path.resize(2, [0; 3]);
        self.args.path[1] = destination.map(f32::to_bits);
    }
    pub fn set_walk(&mut self, walk: bool) {
        if walk {
            self.args.flags &= !RUNMODE;
        } else {
            self.args.flags |= RUNMODE;
        }
    }
    pub fn set_fly(&mut self) {
        self.args.flags |= spline::FLYING;
    }
    pub fn set_cyclic(&mut self) {
        self.args.flags |= spline::CYCLIC;
    }
    pub fn set_fall(&mut self) {
        self.args.flags |= spline::FALLING;
    }
    pub fn set_velocity(&mut self, velocity: f32) {
        self.args.velocity = velocity.to_bits();
    }
    pub fn set_facing_target(&mut self, target: SourceGuid) {
        self.args.flags = (self.args.flags & !FACING) | spline::FINAL_TARGET;
        self.args.facing[0] = Some(target.raw() as u32);
        self.args.facing[1] = Some((target.raw() >> 32) as u32);
    }
    pub fn set_facing_point(&mut self, point: [f32; 3]) {
        self.args.facing = point.map(|value| Some(value.to_bits()));
        self.args.flags = (self.args.flags & !FACING) | spline::FINAL_POINT;
    }
    pub fn set_facing(&mut self, angle: f32) -> Result<()> {
        self.args.facing[0] =
            Some(spline::wrap_facing(angle.to_bits()).map_err(super::splines::spline_error)?);
        self.args.flags = (self.args.flags & !FACING) | spline::FINAL_ANGLE;
        Ok(())
    }
    /// MoveSpline.cpp:177-180 deliberately does NOT call _checkPathBounds.
    pub fn validate(&self) -> bool {
        self.args.path.len() > 1 && f32::from_bits(self.args.velocity) > 0.
    }
    pub fn check_bounds(&self) -> bool {
        if self.args.flags & spline::FLYING == 0 && self.args.path.len() > 2 {
            let first = self.args.path[0].map(f32::from_bits);
            let last = self.args.path.last().unwrap().map(f32::from_bits);
            let middle: [f32; 3] = std::array::from_fn(|i| (first[i] + last[i]) / 2.);
            for p in &self.args.path[1..self.args.path.len() - 1] {
                if (0..3).any(|i| ((f32::from_bits(p[i]) - middle[i]) / 0.25).abs() >= 1024.) {
                    return false;
                }
            }
        }
        true
    }
    fn real_position(
        &self,
        unit: ObjectHandle,
        context: &MotionContext<'_>,
        owner: &(impl MotionExecutionOwner + ?Sized),
    ) -> Result<([u32; 4], Option<ObjectHandle>)> {
        let transport = owner.pathfinder_passengers()?.movement(unit)?.transport;
        let mut pose = context.objects.pose(unit)?.0;
        if let Some(transport) = transport {
            pose = benilla_sim::source_transports::passenger_offset(
                pose,
                context.objects.pose(transport)?.0,
            )
            .map_err(|_| refuse("execution.spline.passenger_offset_numeric_domain_59"))?;
        }
        if !context.spline_finalized(unit)? {
            pose = context
                .spline(unit)?
                .compute_position()
                .map_err(super::splines::spline_error)?;
        }
        Ok((pose, transport))
    }
    fn output(
        &self,
        unit: ObjectHandle,
        transport: Option<ObjectHandle>,
        stop: Option<[u32; 4]>,
        context: &mut MotionContext<'_>,
        owner: &mut (impl MotionExecutionOwner + ?Sized),
    ) -> Result<()> {
        let guid = context
            .objects
            .get(unit)?
            .guid
            .ok_or_else(|| refuse("execution.spline.unit_guid_owner_108"))?;
        let transport = transport
            .map(|t| {
                context
                    .objects
                    .get(t)?
                    .guid
                    .ok_or_else(|| refuse("execution.spline.transport_guid_owner_113"))
            })
            .transpose()?;
        let movement = if let Some(pose) = stop {
            let mut packet = Vec::with_capacity(17);
            for value in &pose[..3] {
                packet.extend(value.to_le_bytes());
            }
            packet.extend(context.spline(unit)?.id().to_le_bytes());
            packet.push(1);
            packet
        } else {
            monster_move(context.spline(unit)?)?
        };
        let output = SplineOutbound {
            unit,
            guid,
            transport,
            excluded: None,
            movement,
        };
        context
            .units
            .get_mut(context.objects, unit)?
            .spline_outbound
            .push(output);
        // Unit.cpp:11214. Out-of-world sends do not visit a recipient set.
        if context.objects.get(unit)?.in_world {
            let packet = context
                .units
                .get(context.objects, unit)?
                .spline_outbound
                .last()
                .expect("The packet was appended before sending");
            owner.send_spline(packet, context)?;
        }
        Ok(())
    }
    pub fn launch(
        &mut self,
        unit: ObjectHandle,
        context: &mut MotionContext<'_>,
        owner: &mut (impl MotionExecutionOwner + ?Sized),
    ) -> Result<i32> {
        // Object.cpp:1173 supplies the actual constructor state, not an assumed false query.
        if context.units.get(context.objects, unit)?.spline_debug_path {
            for (index, &point) in self.args.path.iter().enumerate() {
                owner.spline_debug_spawn(unit, point, index as u32, context)?;
            }
        }
        let (pose, transport) = self.real_position(unit, context, owner)?;
        let empty = self.args.path.is_empty();
        if empty {
            self.move_to([pose[0], pose[1], pose[2]].map(f32::from_bits));
        }
        self.args.path[0] = [pose[0], pose[1], pose[2]];
        if self.args.flags & spline::CYCLIC != 0 {
            self.args.flags |= spline::ENTER_CYCLE;
        } else {
            self.args.flags &= !spline::ENTER_CYCLE;
        }
        let mut move_flags = flags(context, unit)?;
        if self.args.flags & RUNMODE != 0 {
            move_flags &= !WALK;
        } else {
            move_flags |= WALK;
        }
        move_flags |= FORWARD_SPLINE;
        if f32::from_bits(self.args.velocity) == 0. {
            let movement = movement(context, unit)?;
            self.args.velocity = movement
                .speed_for_flags(move_flags & !(0x800 | 0x20_0000))?
                .to_bits();
            if self.slowed != 0. {
                self.args.velocity = (f32::from_bits(self.args.velocity) * self.slowed).to_bits();
            }
        }
        if !self.validate() {
            return Ok(0);
        }
        if move_flags & ROOT != 0 && !empty {
            return Ok(0);
        }
        self.args.spline_id = owner.spline_thread()?.take_id();
        write_flags(context, unit, move_flags)?;
        context
            .units
            .get_mut(context.objects, unit)?
            .movespline
            .initialize(&self.args)
            .map_err(super::splines::spline_error)?;
        self.output(unit, transport, None, context, owner)?;
        Ok(context
            .spline(unit)?
            .duration()
            .map_err(super::splines::spline_error)? as i32)
    }
    pub fn stop(
        &mut self,
        unit: ObjectHandle,
        force_send: bool,
        context: &mut MotionContext<'_>,
        owner: &mut (impl MotionExecutionOwner + ?Sized),
    ) -> Result<()> {
        if !force_send && context.spline_finalized(unit)? {
            return Ok(());
        }
        let (pose, transport) = self.real_position(unit, context, owner)?;
        if self.args.path.is_empty() {
            self.move_to([pose[0], pose[1], pose[2]].map(f32::from_bits));
        }
        self.args.path[0] = [pose[0], pose[1], pose[2]];
        self.args.spline_id = owner.spline_thread()?.take_id();
        self.args.flags = spline::DONE;
        write_flags(context, unit, flags(context, unit)? & !FORWARD_SPLINE)?;
        context
            .units
            .get_mut(context.objects, unit)?
            .movespline
            .initialize(&self.args)
            .map_err(super::splines::spline_error)?;
        self.output(unit, transport, Some(pose), context, owner)
    }
}
fn word(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}
/// packet_builder.cpp:35-146. Packed XYZ conversions refuse at ByteBuffer.h:424-426.
fn monster_move(s: &spline::MoveSpline) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(64);
    let points = s.spline().points();
    let first = points
        .get(s.spline().first())
        .ok_or_else(|| refuse("execution.spline.packet_first_point_read_undefined_45"))?;
    for &value in first {
        word(&mut out, value);
    }
    word(&mut out, s.id());
    let facing = |index| s.facing_word(index).map_err(super::splines::spline_error);
    match s.flags() & FACING {
        spline::FINAL_TARGET => {
            out.push(3);
            let target = u64::from(facing(0)?) | (u64::from(facing(1)?) << 32);
            out.extend(target.to_le_bytes());
        }
        spline::FINAL_ANGLE => {
            out.push(4);
            word(&mut out, facing(0)?);
        }
        spline::FINAL_POINT => {
            out.push(2);
            for index in 0..3 {
                word(&mut out, facing(index)?);
            }
        }
        _ => out.push(0),
    }
    word(&mut out, (s.flags() & !(FACING | spline::DONE)) | RUNMODE);
    word(
        &mut out,
        s.duration().map_err(super::splines::spline_error)?,
    );
    if s.flags() & spline::FLYING != 0 {
        let count = points.len()
            - if s.flags() & spline::CYCLIC != 0 {
                4
            } else {
                3
            };
        word(&mut out, count as u32);
        for p in &points[2..2 + count] {
            for &value in p {
                word(&mut out, value);
            }
        }
    } else {
        let last = s.spline().last();
        let dest = points[last].map(f32::from_bits);
        let count_pos = out.len();
        word(&mut out, 0);
        for value in points[last] {
            word(&mut out, value);
        }
        let mut count = 1u32;
        for p in &points[s.spline().first() + 1..last] {
            let delta: [f32; 3] = std::array::from_fn(|i| dest[i] - f32::from_bits(p[i]));
            let square = (delta[0] * delta[0] + delta[1] * delta[1]) + delta[2] * delta[2];
            if square < 0.5 {
                continue;
            }
            word(
                &mut out,
                spline::pack_xyz(delta.map(f32::to_bits)).map_err(super::splines::spline_error)?,
            );
            count += 1;
        }
        out[count_pos..count_pos + 4].copy_from_slice(&count.to_le_bytes());
    }
    Ok(out)
}

#[cfg(test)]
#[path = "spline_init_tests.rs"]
mod tests;
