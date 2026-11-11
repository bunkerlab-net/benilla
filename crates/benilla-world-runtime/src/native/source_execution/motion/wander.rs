// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; RandomMovementGenerator.cpp:25-211.
use super::super::*;
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};
use benilla_sim::source_time::ShortTimeTracker;
use std::cell::Cell;

pub const WANDER_ACTIVE: u32 = 0x8000;
pub const WANDER_MOVING: u32 = 0x10000;
const NO_FREE_MOVE: u32 = 0x2020_219e;
const RUNNING_STATE: u32 = 0x0444_0000;

/// Numerical state only, not proof that F6 dispatched a movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceWanderTimer {
    timer: ShortTimeTracker,
    move_count: u32,
}
impl Default for SourceWanderTimer {
    fn default() -> Self {
        Self {
            timer: ShortTimeTracker::new(0),
            move_count: 1,
        }
    }
}
impl SourceWanderTimer {
    pub fn expiry(self) -> u32 {
        self.timer.get_expiry()
    }
    pub fn move_count(self) -> u32 {
        self.move_count
    }
    pub fn update(&mut self, diff: u32) {
        self.timer.update(diff);
    }
    /// RandomMovementGenerator.cpp:78 resets the timer, not the move count.
    pub fn reset(&mut self) {
        self.timer.reset(0);
    }
    /// The addition is uint32; the pinned LP64 conversion to int32 precedes clamp.
    /// RandomMovementGenerator.cpp:200-211; this is not signed overflow.
    pub fn add_pause(&mut self, delta: i32, force: bool) {
        if force {
            self.timer.reset(delta as u32);
        } else if !self.timer.passed() {
            let wait = self.expiry().wrapping_add(delta as u32) as i32;
            self.timer.reset(if wait > 0 { wait as u32 } else { 1 });
        }
    }
    /// Kernel-only post-launch arithmetic, RandomMovementGenerator.cpp:101-112.
    /// A caller must obtain the real spline result before using this kernel.
    pub fn location_result(
        &mut self,
        duration: i32,
        player_controlled: bool,
        random: &mut impl CreatureRandom,
    ) -> Result<()> {
        if duration != 0 {
            if self.move_count > 1 {
                self.move_count -= 1;
            } else {
                self.move_count = random.creature_urand(1, 3)?;
                self.timer.reset(random.creature_urand(3000, 10000)?);
            }
        } else {
            self.timer.reset(if player_controlled { 100 } else { 500 });
        }
        Ok(())
    }
}

/// Kernel-only F3 draws inside F6, PathFinder.cpp:1366-1367.
/// No point is constructed; updateFilter/SetCurrentNavMesh must execute first.
pub fn wander_destination_draws<E: RawGenerator, S: FnMut(TraceEvent)>(
    random: &mut SourceRandom<'_, E, S>,
    radius: f32,
) -> Result<[f32; 2]> {
    let angle = random
        .rand_norm_f()
        .map_err(|_| refuse("execution.motion.wander_rand_norm_f"))?
        * 2.
        * std::f32::consts::PI;
    let range = random
        .rand_norm_f()
        .map_err(|_| refuse("execution.motion.wander_rand_norm_f"))?
        * radius;
    Ok([angle, range])
}

/// Kernel-only walk choice after a path exists, RandomMovementGenerator.cpp:135-138.
pub fn wander_walk_choice(
    random_run: bool,
    walk: bool,
    random: &mut impl CreatureRandom,
) -> Result<bool> {
    if random_run {
        Ok(random.creature_urand(0, 99)? >= 15)
    } else {
        Ok(walk)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WanderParameters {
    pub center: [f32; 3],
    pub radius: f32,
    pub vertical_z: f32,
    pub walk: bool,
    pub random_run: bool,
}
pub struct SourceWanderGenerator {
    parameters: WanderParameters,
    timer: Cell<SourceWanderTimer>,
}
impl SourceWanderGenerator {
    /// Explicit-location constructor, RandomMovementGenerator.cpp:165-175.
    pub fn new(
        center: [f32; 3],
        radius: f32,
        vertical_z: f32,
        walk: bool,
        random_run: bool,
    ) -> Self {
        Self {
            parameters: WanderParameters {
                center,
                radius,
                vertical_z,
                walk,
                random_run,
            },
            timer: Cell::new(SourceWanderTimer::default()),
        }
    }
    /// RandomMovementGenerator.cpp:177-182; GetRespawnCoord, Creature.cpp:2493-2506.
    pub fn from_creature(
        unit: ObjectHandle,
        context: &MotionContext<'_>,
        radius: f32,
    ) -> Result<Self> {
        let object = context.objects.get(unit)?;
        require(
            matches!(
                object.kind,
                SourceObjectKind::Creature | SourceObjectKind::Pet
            ),
            "execution.motion.wander_not_creature",
        )?;
        let random_run = context.units.get(context.objects, unit)?.run_during_wander;
        let pose = object
            .respawn_pose
            .ok_or_else(|| refuse("execution.motion.respawn_position_absent"))?
            .0
            .map(f32::from_bits);
        // GridDefines.h:38,56-57,177-189. Orientation and radius are not asserted.
        let limit = (533.333_3_f32 * 64. / 2.) as f64 - 0.5;
        require(
            pose[..2]
                .iter()
                .all(|v| v.is_finite() && f64::from(v.abs()) <= limit)
                && pose[2].is_finite(),
            "execution.motion.respawn_coord_assertion",
        )?;
        Ok(Self::new(
            [pose[0], pose[1], pose[2]],
            radius,
            0.,
            true,
            random_run,
        ))
    }
    pub fn parameters(&self) -> WanderParameters {
        self.parameters
    }
    pub fn timer(&self) -> SourceWanderTimer {
        self.timer.get()
    }
    pub fn add_to_random_pause_time(&self, delta: i32, force: bool) {
        let mut timer = self.timer.get();
        timer.add_pause(delta, force);
        self.timer.set(timer);
    }
    fn restore_walk(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if matches!(
            context.objects.get(master.unit)?.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ) {
            let walk = context.units.get(context.objects, master.unit)?.state & RUNNING_STATE == 0;
            owner.motion_call(master, MotionCall::RestoreWalk(walk), context)?;
        }
        Ok(())
    }
}
impl SourceMovementGenerator for SourceWanderGenerator {
    fn kind(&self) -> u32 {
        1
    }
    fn initialize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        _: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        context.units.get_mut(context.objects, master.unit)?.state |= WANDER_ACTIVE;
        // RandomMovementGenerator.cpp:29; PathFinder.cpp:47-53 queries F6 already.
        // Do not draw or execute the later client-control/target/spline operations.
        Err(refuse(
            "execution.motion.f6_wander_pathfinder_constructor_owner",
        ))
    }
    fn reset(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        let mut timer = self.timer.get();
        timer.reset();
        self.timer.set(timer);
        self.initialize(master, context, owner)
    }
    fn interrupt(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        owner.motion_call(master, MotionCall::InterruptMoving, context)?;
        context.units.get_mut(context.objects, master.unit)?.state &= !WANDER_MOVING;
        self.restore_walk(master, context, owner)
    }
    fn finalize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        let fields = context.units.get_mut(context.objects, master.unit)?;
        fields.state &= !(WANDER_ACTIVE | WANDER_MOVING);
        // GetClientControlling is not GetControllingPlayer, Unit.cpp:6523-6548.
        let player_controlled = fields.flags & 8 != 0;
        if let Some(player) = if player_controlled {
            owner.client_controlling(master.unit)?
        } else {
            None
        } {
            owner.motion_call(master, MotionCall::RestoreClientControl(player), context)?;
        }
        if !owner.spline_finalized(master.unit)? {
            let call = if owner.client_controlled(master.unit)? {
                MotionCall::StopMovingForced
            } else {
                MotionCall::InterruptMoving
            };
            owner.motion_call(master, call, context)?;
        }
        self.restore_walk(master, context, owner)
    }
    fn update(
        &self,
        master: &mut SourceMotionMaster,
        diff: u32,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<bool> {
        if !context.objects.get(master.unit)?.alive {
            return Ok(false);
        }
        let fields = context.units.get_mut(context.objects, master.unit)?;
        if fields.state & (NO_FREE_MOVE & !WANDER_ACTIVE) != 0 {
            let mut timer = self.timer.get();
            timer.update(diff);
            self.timer.set(timer);
            fields.state &= !WANDER_MOVING;
            return Ok(true);
        }
        if owner.spline_finalized(master.unit)? {
            let mut timer = self.timer.get();
            timer.update(diff);
            self.timer.set(timer);
            if timer.expiry() == 0 {
                // RandomMovementGenerator.cpp:127. No authored path can bypass F6.
                return Err(refuse("execution.motion.f6_wander_random_path_owner"));
            }
        }
        Ok(true)
    }
}
