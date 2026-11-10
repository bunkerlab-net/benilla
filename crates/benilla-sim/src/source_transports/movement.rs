// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Transports.cpp:349-440; translation GPL-3.0-or-later.
use super::*;
use crate::source_time::ShortTimeTracker;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportStep {
    Unchanged,
    Position { parameter: u32, pose: [u32; 4] },
    Transfer { current: usize, next: usize },
}
#[derive(Clone, Debug)]
pub struct TransportMotion {
    progress: u32,
    movement_started: u32,
    current: usize,
    next: usize,
    moving: bool,
    pending_stop: bool,
    position_timer: ShortTimeTracker,
}
impl TransportMotion {
    // The constructor initializes progress from the map snapshot, not elapsed diff; Transports.cpp:174-177, Transports.h:33.
    pub fn new(snapshot: u32) -> Self {
        Self {
            progress: snapshot,
            movement_started: 0,
            current: 0,
            next: 1,
            moving: true,
            pending_stop: false,
            position_timer: ShortTimeTracker::default(),
        }
    }
    pub fn progress(&self) -> u32 {
        self.progress
    }
    pub fn current_frame(&self) -> usize {
        self.current
    }
    pub fn next_frame(&self) -> usize {
        self.next
    }
    pub fn is_moving(&self) -> bool {
        self.moving
    }
    pub fn position_expiry(&self) -> u32 {
        self.position_timer.get_expiry()
    }
    // Unsigned subtraction precedes the ordinary >= guard, including wrap suppression; Transports.cpp:356-363.
    pub fn update(
        &mut self,
        template: &TransportTemplate,
        definition: MoTransport,
        map: u32,
        snapshot: u32,
        _supplied_diff: u32,
    ) -> Result<TransportStep> {
        if template.key_frames.len() <= 1 {
            return Ok(TransportStep::Unchanged);
        }
        let current = snapshot.wrapping_sub(self.movement_started);
        if self.progress >= current {
            return Ok(TransportStep::Unchanged);
        }
        let diff = current.wrapping_sub(self.progress);
        if self.moving || !self.pending_stop {
            self.progress = current;
        }
        if template.path_time == 0 {
            return Err(Error::SourceUndefined("period_modulo_zero"));
        }
        let progress = self.progress % template.path_time;
        loop {
            let frame = template
                .key_frames
                .get(self.current)
                .ok_or(Error::SourceUndefined("current_frame"))?;
            if progress >= frame.arrive_time && progress < frame.departure_time {
                self.moving = false;
                break;
            }
            self.moving = true;
            if progress >= frame.departure_time && progress < frame.next_arrive_time {
                break;
            }
            self.current = self.next;
            self.next += 1;
            if self.next == template.key_frames.len() {
                self.next = 0;
            }
            let frame = &template.key_frames[self.current];
            if template.nodes[frame.node].map != map || frame.teleport {
                return Ok(TransportStep::Transfer {
                    current: self.current,
                    next: self.next,
                });
            }
        }
        self.position_timer.update(diff);
        if !self.position_timer.passed() {
            return Ok(TransportStep::Unchanged);
        }
        self.position_timer.reset(50);
        if !self.moving || progress == 0 {
            return Ok(TransportStep::Unchanged);
        }
        let frame = &template.key_frames[self.current];
        let parameter = segment_position(template, definition, frame, progress)?;
        let spline = frame
            .spline
            .and_then(|i| template.splines.get(i))
            .ok_or(Error::SourceUndefined("movement_spline_null"))?;
        let pos = spline.evaluate(frame.index, parameter)?;
        let dir = spline.derivative(frame.index, parameter)?;
        // UpdatePosition stores the unnormalized float atan2 + double M_PI; Transports.cpp:406, Object.cpp:1198-1203.
        let angle = crate::facing::atan2_bits(dir[1], dir[0]).map_err(|_| Error::NumericDomain)?;
        let angle = scalar((S::bits(angle).double() + D::bits(0x4009_21fb_5444_2d18)).single())?;
        Ok(TransportStep::Position {
            parameter,
            pose: [pos[0], pos[1], pos[2], angle],
        })
    }
}
// All operations remain binary32 and the final quotient is unclamped; Transports.cpp:411-439.
pub fn segment_position(
    template: &TransportTemplate,
    definition: MoTransport,
    frame: &KeyFrame,
    progress: u32,
) -> Result<u32> {
    let now = integer(progress) * S::bits(0x3a83_126f);
    let elapsed = now - (S::bits(0x3f80_0000) / integer(1000)) * integer(frame.departure_time);
    let since = finite(frame.time_from)? + elapsed;
    let until = finite(frame.time_to)? - elapsed;
    let accel_time = finite(template.accel_time)?;
    let distance = |time: S| -> Result<S> {
        Ok(if time < accel_time {
            S::bits(0x3f00_0000) * integer(definition.acceleration) * time * time
        } else {
            finite(template.accel_dist)? + (time - accel_time) * integer(definition.speed)
        })
    };
    let position = if since < until {
        distance(since)? - finite(frame.dist_since_stop)?
    } else {
        finite(frame.dist_until_stop)? - distance(until)?
    };
    scalar(position / finite(frame.next_dist_from_prev)?)
}
