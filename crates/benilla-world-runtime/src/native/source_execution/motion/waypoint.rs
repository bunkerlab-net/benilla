// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1; WaypointMovementGenerator.cpp:35-254,305-614.
use super::super::*;
use benilla_sim::source_time::ShortTimeTracker;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

pub const WAYPOINT_PAUSED: u32 = 0x0800_0000;
#[derive(Clone, Copy, Debug, Default)]
pub struct WaypointPathRequest {
    pub path_id: i32,
    pub origin: WaypointOrigin,
    pub delay: u32,
    pub overwrite_entry: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaypointProgress {
    pub current: u32,
    pub last_reached: u32,
    pub path_id: u32,
    pub origin: WaypointOrigin,
    pub wait: u32,
    pub script_time: i32,
    pub path_duration: i32,
    pub speed_changed: bool,
}
/// Component-only state. No successful spline/path or native continuation is implied.
pub struct SourceWaypointGenerator {
    // The C++ constructor leaves i_path indeterminate, header:48-57,74-77.
    loaded: Cell<bool>,
    path: RefCell<Option<Rc<SourceWaypointPath>>>,
    cursor: Cell<Option<u32>>,
    progress: Cell<WaypointProgress>,
    reset_point: Cell<[f32; 4]>,
    node_indexes: RefCell<VecDeque<i32>>,
    pub forced_movement: Cell<u32>,
    pub target_guid: Cell<Option<SourceGuid>>,
}
impl Default for SourceWaypointGenerator {
    fn default() -> Self {
        Self {
            loaded: Cell::new(false),
            path: RefCell::new(None),
            cursor: Cell::new(None),
            progress: Cell::new(WaypointProgress::default()),
            // Position::Position, Object.h:288.
            reset_point: Cell::new([0.; 4]),
            node_indexes: RefCell::new(VecDeque::new()),
            forced_movement: Cell::new(0),
            target_guid: Cell::new(None),
        }
    }
}
impl SourceWaypointGenerator {
    pub fn progress(&self) -> WaypointProgress {
        self.progress.get()
    }
    fn path(&self) -> Result<Option<Rc<SourceWaypointPath>>> {
        require(
            self.loaded.get(),
            "execution.waypoint.path_uninitialized_undefined",
        )?;
        Ok(self.path.borrow().clone())
    }
    /// LoadPath does not actually change the single node's delay. `auto modNode`
    /// copies the const_cast reference, cpp:70-82. Preserve this source quirk.
    pub fn load_path(
        &self,
        paths: &SourceWaypointPaths,
        entry: u32,
        guid: u32,
        path_id: i32,
        origin: WaypointOrigin,
    ) -> Result<()> {
        let (origin, path) = paths.select(entry, guid, path_id, origin)?;
        self.loaded.set(true);
        let mut p = self.progress.get();
        // GetDefaultPath leaves the previous origin untouched on a miss.
        if origin != WaypointOrigin::None {
            p.origin = origin;
        }
        p.path_id = path_id as u32;
        if let Some(path) = &path {
            if let Some((&first, _)) = path.first_key_value() {
                p.current = first;
                p.last_reached = 0;
                self.cursor.set(Some(first));
            }
        }
        self.progress.set(p);
        *self.path.borrow_mut() = path;
        Ok(())
    }
    pub fn initialize_path(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
        request: WaypointPathRequest,
    ) -> Result<()> {
        let WaypointPathRequest {
            path_id,
            origin,
            delay,
            overwrite_entry,
        } = request;
        let paths = owner.waypoint_paths()?;
        let entry = if overwrite_entry == 0 {
            context.units.get(context.objects, master.unit)?.entry
        } else {
            overwrite_entry
        };
        self.load_path(
            &paths,
            entry,
            context.objects.get(master.unit)?.database_guid,
            path_id,
            origin,
        )?;
        self.stop(delay);
        let Some(path) = self.path()? else {
            return Ok(());
        };
        // uint32 startPoint=-1 satisfies >=0. It selects UINT_MAX if present.
        // cpp:104-128. Empty begin()->first is undefined, not a missing path.
        require(
            !path.is_empty(),
            "execution.waypoint.empty_path_iterator_undefined",
        )?;
        let first = *path.first_key_value().unwrap().0;
        let mut p = self.progress.get();
        p.current = if path.contains_key(&u32::MAX) {
            u32::MAX
        } else {
            first
        };
        p.last_reached = path.range(..p.current).next_back().map_or(0, |(&k, _)| k);
        self.cursor.set(Some(p.current));
        self.progress.set(p);
        self.send_next(master, context, owner)?;
        self.reset_point.set(owner.waypoint_position(master.unit)?);
        Ok(())
    }
    fn stop(&self, wait: u32) {
        let mut p = self.progress.get();
        p.wait = wait;
        self.progress.set(p);
    }
    fn stopped(&self, state: u32) -> bool {
        self.progress.get().wait != 0 || state & WAYPOINT_PAUSED != 0
    }
    fn can_move(&self, diff: u32, state: u32) -> bool {
        let mut timer = ShortTimeTracker::new(self.progress.get().wait);
        // cpp:506 narrows to int32; Timer::Update converts back to uint32.
        timer.update(diff);
        if timer.passed() && state & WAYPOINT_PAUSED != 0 {
            timer.reset(1);
        }
        self.stop(timer.get_expiry());
        timer.passed() && state & WAYPOINT_PAUSED == 0
    }
    /// All additions have a uint32 operand. Wrap first, then pinned int32
    /// conversion and minimum-one clamp, cpp:211,569-584; no signed overflow.
    pub fn add_pause(&self, delta: i32, force: bool) {
        let mut p = self.progress.get();
        if force {
            p.wait = delta as u32;
        } else if p.wait != 0 {
            let new_wait = p.wait.wrapping_add(delta as u32) as i32;
            p.wait = if new_wait > 0 { new_wait as u32 } else { 1 };
        } else {
            p.script_time = delta;
        }
        self.progress.set(p);
    }
    pub fn set_next_waypoint(&self, mut point: u32) -> Result<bool> {
        let Some(path) = self.path()? else {
            return Ok(false);
        };
        if path.is_empty() {
            return Ok(false);
        }
        // cpp:593 uses size(), not greatest map key. Noncontiguous keys can miss.
        point = (u64::from(point) % path.len() as u64) as u32;
        if !path.contains_key(&point) {
            return Ok(false);
        }
        let mut p = self.progress.get();
        p.wait = 1;
        p.current = point;
        p.last_reached = if point > 0 {
            point - 1
        } else {
            *path.last_key_value().unwrap().0
        };
        self.progress.set(p);
        self.cursor.set(Some(point));
        Ok(true)
    }
    pub fn speed_changed(&self) {
        let mut p = self.progress.get();
        p.speed_changed = true;
        self.progress.set(p);
    }
    fn informs(
        &self,
        unit: ObjectHandle,
        has_ai: bool,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<bool> {
        let p = self.progress.get();
        Ok(p.origin == WaypointOrigin::External
            || (p.origin == WaypointOrigin::WaypointPath
                && has_ai
                && p.path_id == owner.waypoint_ai_path(unit)?))
    }
    fn send_next(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.node_indexes.borrow_mut().clear();
        let fields = context.units.get(context.objects, master.unit)?;
        if !context.objects.get(master.unit)?.alive
            || fields.immobilized()
            || owner.waypoint_casting_for_movement(master.unit)?
        {
            if !owner.spline_finalized(master.unit)? {
                let call = if owner.client_controlled(master.unit)? {
                    MotionCall::StopMovingForced
                } else {
                    MotionCall::InterruptMoving
                };
                owner.motion_call(master, call, context)?;
            }
            return Ok(());
        }
        context.units.get_mut(context.objects, master.unit)?.state |= WANDER_MOVING;
        let mut p = self.progress.get();
        p.speed_changed = false;
        self.progress.set(p);
        let path = self
            .path()?
            .ok_or_else(|| refuse("execution.waypoint.null_path_iterator_undefined"))?;
        require(
            self.cursor.get().is_some_and(|key| path.contains_key(&key)),
            "execution.waypoint.empty_path_iterator_undefined",
        )?;
        let has_ai = context.units.get(context.objects, master.unit)?.has_ai;
        if self.informs(master.unit, has_ai, owner)? && has_ai {
            let finished = path.first_key_value().is_some_and(|(&k, _)| k == p.current)
                && path
                    .last_key_value()
                    .is_some_and(|(&k, _)| k == p.last_reached);
            owner.waypoint_inform(
                self,
                master,
                context,
                if finished { 20 } else { 19 },
                p.current,
            )?;
            let object = context.objects.get(master.unit)?;
            if !object.alive || !object.in_world {
                return Ok(());
            }
        }
        // cpp:354 obtains live world pose before the first spline query.
        context.objects.pose(master.unit)?;
        if !owner.spline_finalized(master.unit)? {
            // cpp:357-359 needs transport transform and ComputePosition first.
            return Err(refuse(
                "execution.waypoint.f6_spline_compute_position_owner",
            ));
        }
        // cpp:264 constructs PathFinder before calculate/launch or RNG draws.
        Err(refuse("execution.waypoint.f6_pathfinder_constructor_owner"))
    }
    fn arrived(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        let mut p = self.progress.get();
        if p.last_reached == p.current {
            return Ok(());
        }
        let path = self
            .path()?
            .ok_or_else(|| refuse("execution.waypoint.null_path_iterator_undefined"))?;
        let cursor = self
            .cursor
            .get()
            .ok_or_else(|| refuse("execution.waypoint.empty_path_iterator_undefined"))?;
        let node = *path
            .get(&cursor)
            .ok_or_else(|| refuse("execution.waypoint.empty_path_iterator_undefined"))?;
        p.last_reached = cursor;
        self.progress.set(p);
        if node.script != 0 {
            owner.waypoint_script(self, master, context, node.script, self.target_guid.get())?;
        }
        let has_ai = context.units.get(context.objects, master.unit)?.has_ai;
        if has_ai {
            let external = self.informs(master.unit, has_ai, owner)?;
            owner.waypoint_inform(
                self,
                master,
                context,
                if external { 18 } else { 2 },
                self.progress.get().current,
            )?;
        }
        self.reset_point.set(owner.waypoint_position(master.unit)?);
        p = self.progress.get();
        let wait = node.delay.wrapping_add(p.script_time as u32) as i32;
        let cursor = self
            .cursor
            .get()
            .ok_or_else(|| refuse("execution.waypoint.empty_path_iterator_undefined"))?;
        let next = path
            .range((
                std::ops::Bound::Excluded(cursor),
                std::ops::Bound::Unbounded,
            ))
            .next()
            .or_else(|| path.first_key_value())
            .map(|(&k, _)| k)
            .ok_or_else(|| refuse("execution.waypoint.empty_path_iterator_undefined"))?;
        // In C++ the iterator changes before i_currentNode, cpp:214,252.
        // Keep i_currentNode until callbacks finish, including a restart.
        self.cursor.set(Some(next));
        if wait > 0 {
            context.units.get_mut(context.objects, master.unit)?.state &= !WANDER_MOVING;
            self.stop(wait as u32);
            if !owner.spline_finalized(master.unit)? && node.delay == 0 && p.script_time != 0 {
                owner.motion_call(master, MotionCall::StopSpline, context)?;
            }
        } else if wait == 0 && node.delay != 0 {
            // SendNext uses the next iterator even while current still names arrival.
            self.send_next(master, context, owner)?;
        }
        if !self.node_indexes.borrow().is_empty()
            && !self.stopped(context.units.get(context.objects, master.unit)?.state)
        {
            let has_ai = context.units.get(context.objects, master.unit)?.has_ai;
            // cpp:243 dereferences AI without a null check for waypoint_path.
            if p.origin == WaypointOrigin::WaypointPath {
                require(has_ai, "execution.waypoint.null_ai_path_query_undefined")?;
            }
            if self.informs(master.unit, has_ai, owner)? && has_ai {
                owner.waypoint_inform(self, master, context, 19, next)?;
            }
        }
        p = self.progress.get();
        p.current = next;
        p.script_time = 0;
        self.progress.set(p);
        Ok(())
    }
    fn restore_walk(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        let walk = context.units.get(context.objects, master.unit)?.state & 0x0444_0000 == 0;
        owner.motion_call(master, MotionCall::RestoreWalk(walk), context)
    }
}
impl SourceMovementGenerator for SourceWaypointGenerator {
    fn kind(&self) -> u32 {
        2
    }
    fn initialize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        _: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        let fields = context.units.get_mut(context.objects, master.unit)?;
        fields.state = (fields.state | WANDER_ACTIVE) & !WAYPOINT_PAUSED;
        Ok(())
    }
    fn reset(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        let fields = context.units.get_mut(context.objects, master.unit)?;
        fields.state |= WANDER_ACTIVE;
        if self.path()?.is_none() || self.stopped(fields.state) {
            return Ok(());
        }
        self.send_next(master, context, owner)
    }
    fn finalize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        context.units.get_mut(context.objects, master.unit)?.state &=
            !(WANDER_ACTIVE | WANDER_MOVING);
        self.restore_walk(master, context, owner)
    }
    fn interrupt(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if context.units.get(context.objects, master.unit)?.state & WANDER_MOVING != 0 {
            self.reset_point.set(owner.waypoint_position(master.unit)?);
        }
        owner.motion_call(master, MotionCall::InterruptMoving, context)?;
        context.units.get_mut(context.objects, master.unit)?.state &=
            !(WANDER_ACTIVE | WANDER_MOVING);
        self.restore_walk(master, context, owner)
    }
    fn update(
        &self,
        master: &mut SourceMotionMaster,
        diff: u32,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<bool> {
        let fields = context.units.get_mut(context.objects, master.unit)?;
        if fields.state & MOTION_NOT_MOVE != 0 {
            fields.state &= !WANDER_MOVING;
            return Ok(true);
        }
        if self.path()?.is_none_or(|p| p.is_empty()) {
            fields.state &= !WANDER_MOVING;
            return Ok(true);
        }
        if self.stopped(fields.state) || self.progress.get().speed_changed {
            if !owner.spline_finalized(master.unit)? {
                owner.motion_call(master, MotionCall::StopSpline, context)?;
            }
            let state = context.units.get(context.objects, master.unit)?.state;
            if self.can_move(diff, state) {
                self.send_next(master, context, owner)?;
            }
        } else if fields.state & 0x0055_0000 == 0 {
            if owner.waypoint_casting_for_movement(master.unit)? {
                return Ok(true);
            }
            self.stop(1000);
        } else if owner.spline_finalized(master.unit)? {
            self.arrived(master, context, owner)?;
            if !self.stopped(context.units.get(context.objects, master.unit)?.state) {
                self.send_next(master, context, owner)?;
            }
        } else {
            // cpp:479-486 subtracts an unsigned diff and converts to int32.
            let mut p = self.progress.get();
            if p.path_duration <= 0 {
                let path = self.path()?.unwrap();
                let key = self
                    .cursor
                    .get()
                    .ok_or_else(|| refuse("execution.waypoint.empty_path_iterator_undefined"))?;
                if path[&key].delay == 0 {
                    self.send_next(master, context, owner)?;
                }
            } else {
                p.path_duration = (p.path_duration as u32).wrapping_sub(diff) as i32;
                self.progress.set(p);
            }
            if !self.node_indexes.borrow().is_empty() {
                return Err(refuse("execution.waypoint.f6_spline_current_index_owner"));
            }
        }
        Ok(true)
    }
    fn reset_position(&self) -> Option<[f32; 4]> {
        Some(self.reset_point.get())
    }
}
#[cfg(test)]
#[path = "waypoint_tests.rs"]
mod tests;
