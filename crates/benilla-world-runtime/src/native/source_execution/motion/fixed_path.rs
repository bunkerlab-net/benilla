// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1; PathMovementGenerator.cpp:29-319.
use super::super::*;
use benilla_sim::source_time::ShortTimeTracker;
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, Debug, Default)]
pub struct FixedPathOptions {
    pub forced_movement: u32,
    pub flying: bool,
    pub speed: f32,
    pub offset: i32,
    pub cyclic: bool,
    pub target: Option<SourceGuid>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FixedPathProgress {
    pub index: i32,
    pub orientation: Option<f32>,
    pub wait: u32,
    pub cyclic: bool,
    pub first_cycle: bool,
    pub start_point: u32,
    pub speed_changed: bool,
}
/// Read operands of AbstractPathMovementGenerator::Update, never a fabricated spline.
#[derive(Clone, Copy, Debug)]
pub struct FixedPathSplineState {
    pub destination: [f32; 3],
    pub finalized: bool,
    pub index: i32,
}
pub struct SourceFixedPathGenerator {
    // Source copies the loaded map because WaypointManager may reload it, cpp:44-47.
    path: Vec<(u32, SourceWaypointNode)>,
    spline: RefCell<Vec<[f32; 3]>>,
    progress: Cell<FixedPathProgress>,
    pub options: FixedPathOptions,
}
impl SourceFixedPathGenerator {
    pub fn from_path(path: Option<&SourceWaypointPath>, options: FixedPathOptions) -> Self {
        let path = path.map_or_else(Vec::new, |path| {
            path.iter()
                .map(|(&key, &node)| (key, node))
                .collect::<Vec<_>>()
        });
        let cyclic = options.cyclic && !path.iter().any(|(_, n)| n.delay != 0);
        Self {
            spline: RefCell::new(Vec::with_capacity(
                path.len() + usize::from(!path.is_empty()),
            )),
            path,
            progress: Cell::new(FixedPathProgress {
                index: options.offset,
                cyclic,
                ..Default::default()
            }),
            options,
        }
    }
    pub fn from_points(
        points: &[[f32; 3]],
        orientation: Option<f32>,
        options: FixedPathOptions,
    ) -> Self {
        let path = points
            .iter()
            .enumerate()
            .map(|(i, &position)| {
                (
                    i as u32,
                    SourceWaypointNode {
                        position,
                        orientation: if i + 1 == points.len() {
                            orientation
                        } else {
                            Some(0.)
                        },
                        delay: 0,
                        script: 0,
                    },
                )
            })
            .collect();
        Self {
            path,
            spline: RefCell::new(Vec::with_capacity(
                points.len() + usize::from(!points.is_empty()),
            )),
            progress: Cell::new(FixedPathProgress {
                index: options.offset,
                orientation,
                cyclic: options.cyclic,
                ..Default::default()
            }),
            options,
        }
    }
    pub fn load(
        master: &SourceMotionMaster,
        context: &MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
        path_id: i32,
        origin: WaypointOrigin,
        options: FixedPathOptions,
    ) -> Result<Self> {
        let paths = owner.waypoint_paths()?;
        let entry = context.units.get(context.objects, master.unit)?.entry;
        let (_, path) = paths.select(
            entry,
            context.objects.get(master.unit)?.database_guid,
            path_id,
            origin,
        )?;
        Ok(Self::from_path(path.as_deref(), options))
    }
    pub fn progress(&self) -> FixedPathProgress {
        self.progress.get()
    }
    pub fn speed_changed(&self) {
        let mut p = self.progress.get();
        p.speed_changed = true;
        self.progress.set(p);
    }
    pub fn add_pause(&self, delta: i32, force: bool) {
        let mut p = self.progress.get();
        if force {
            p.wait = delta as u32;
            self.spline.borrow_mut().clear();
        } else if p.wait != 0 {
            // uint32 + int32 wraps before the pinned signed conversion, cpp:315.
            let wait = p.wait.wrapping_add(delta as u32) as i32;
            p.wait = if wait > 0 { wait as u32 } else { 1 };
        }
        self.progress.set(p);
    }
    fn prepare_spline(&self) -> Result<()> {
        if self.path.is_empty() {
            return Ok(());
        }
        let mut p = self.progress.get();
        require(
            p.index >= 0 && p.index as usize <= self.path.len(),
            "execution.fixed_path.offset_iterator_undefined",
        )?;
        let mut spline = self.spline.borrow_mut();
        spline.clear();
        spline.push([0.; 3]);
        let offset = p.index as usize;
        if p.cyclic {
            spline.extend(
                self.path[offset..]
                    .iter()
                    .chain(self.path[..offset].iter())
                    .map(|(_, n)| n.position),
            );
        } else {
            for (i, (_, node)) in self.path.iter().enumerate().skip(offset) {
                spline.push(node.position);
                if node.delay != 0 || i + 1 == self.path.len() {
                    p.wait = node.delay;
                    p.orientation = node.orientation;
                    break;
                }
            }
        }
        p.speed_changed = false;
        self.progress.set(p);
        Ok(())
    }
    fn abstract_initialize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.stop_spline(master, context, owner)?;
        self.prepare_spline()
    }
    fn stop_spline(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        if !owner.spline_finalized(master.unit)? {
            let call = if owner.client_controlled(master.unit)? {
                MotionCall::StopMovingForced
            } else {
                MotionCall::InterruptMoving
            };
            owner.motion_call(master, call, context)?;
        }
        Ok(())
    }
    fn inform_node(&self) -> Result<(u32, SourceWaypointNode)> {
        require(
            !self.path.is_empty(),
            "execution.fixed_path.inform_modulo_zero_undefined",
        )?;
        let p = self.progress.get();
        let index = p
            .index
            .checked_add(i32::from(!p.first_cycle))
            .ok_or_else(|| refuse("execution.fixed_path.index_addition_undefined"))?;
        // Negative int32 converts to size_t before modulo, cpp:198.
        Ok(self.path[(index as usize) % self.path.len()])
    }
    fn movement_inform(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        let (index, node) = self.inform_node()?;
        if node.script != 0 {
            owner.fixed_path_script(self, master, context, node.script, self.options.target)?;
        }
        // Owner checks unit AI and the temporary creature/pet summoner AI, cpp:212-223.
        owner.fixed_path_inform(master, context, 3, index)
    }
}
impl SourceMovementGenerator for SourceFixedPathGenerator {
    fn kind(&self) -> u32 {
        3
    }
    fn initialize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        context.units.get_mut(context.objects, master.unit)?.state |= WANDER_ACTIVE;
        self.abstract_initialize(master, context, owner)?;
        if !self.spline.borrow().is_empty() {
            // cpp:274 constructs MoveSplineInit before any path/flag/facing setup.
            // MoveSplineInit.cpp:173-178 reads live movementInfo flags.
            return Err(refuse("execution.fixed_path.f6_move_spline_init_owner"));
        }
        Ok(())
    }
    fn finalize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        context.units.get_mut(context.objects, master.unit)?.state &=
            !(WANDER_ACTIVE | WANDER_MOVING);
        self.stop_spline(master, context, owner)
    }
    fn interrupt(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        context.units.get_mut(context.objects, master.unit)?.state &=
            !(WANDER_ACTIVE | WANDER_MOVING);
        owner.motion_call(master, MotionCall::InterruptMoving, context)
    }
    fn reset(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        context.units.get_mut(context.objects, master.unit)?.state |= WANDER_ACTIVE;
        // Reset explicitly calls Abstract::Initialize, not Fixed::Initialize, cpp:266-270.
        self.abstract_initialize(master, context, owner)
    }
    fn pause(&self, delta: i32, force: bool) -> Result<()> {
        self.add_pause(delta, force);
        Ok(())
    }
    fn update(
        &self,
        master: &mut SourceMotionMaster,
        diff: u32,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<bool> {
        let destination = self.spline.borrow().last().copied();
        if let Some(destination) = destination {
            let state = owner.fixed_path_spline(master.unit)?;
            if state.destination != destination {
                if state.finalized {
                    self.initialize(master, context, owner)?;
                } else {
                    return Ok(false);
                }
            }
            if self.progress.get().cyclic {
                while self.progress.get().index != owner.fixed_path_spline(master.unit)?.index {
                    let p = self.progress.get();
                    if !p.first_cycle || p.index != 0 {
                        self.movement_inform(master, context, owner)?;
                    }
                    let mut p = self.progress.get();
                    let next = p
                        .index
                        .checked_add(1)
                        .ok_or_else(|| refuse("execution.fixed_path.index_addition_undefined"))?;
                    p.index = ((next as usize) % (self.path.len() + 1)) as i32;
                    self.progress.set(p);
                    if owner.spline_finalized(master.unit)? {
                        return Ok(true);
                    }
                }
                let mut p = self.progress.get();
                if p.index as u32 == p.start_point {
                    p.first_cycle = false;
                    self.progress.set(p);
                }
            } else {
                while self.progress.get().index < owner.fixed_path_spline(master.unit)?.index {
                    self.movement_inform(master, context, owner)?;
                    let mut p = self.progress.get();
                    p.index = p
                        .index
                        .checked_add(1)
                        .ok_or_else(|| refuse("execution.fixed_path.index_addition_undefined"))?;
                    self.progress.set(p);
                }
            }
            if !owner.spline_finalized(master.unit)? {
                if self.progress.get().speed_changed {
                    self.initialize(master, context, owner)?;
                }
                return Ok(true);
            }
        }
        let mut p = self.progress.get();
        let mut timer = ShortTimeTracker::new(p.wait);
        timer.update(diff);
        p.wait = timer.get_expiry();
        self.progress.set(p);
        if !timer.passed() {
            return Ok(true);
        }
        if (p.index as usize) < self.path.len() {
            self.initialize(master, context, owner)?;
            return Ok(true);
        }
        Ok(false)
    }
}
#[cfg(test)]
#[path = "fixed_path_tests.rs"]
mod tests;
