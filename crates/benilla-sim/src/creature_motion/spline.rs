// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS MoveSpline.cpp and spline.cpp, pinned in CREATURE-MOTION.md.
use super::{numeric, MotionError, Point, Real};
use serde::{Deserialize, Serialize};
pub const MAX_PATH_POINTS: usize = 74;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplineSnapshot {
    pub points: Vec<Point>,
    pub velocity: Real,
    pub timestamps_ms: Vec<u32>,
    pub elapsed_ms: u32,
    pub segment: usize,
    pub finalized: bool,
}
/// Non-cyclic, non-falling, non-smooth source linear spline. There is no public
/// deserializer: restore recomputes timestamps and checks progress invariants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinearSpline {
    state: SplineSnapshot,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SplineEvent {
    NextSegment { segment: usize },
    Arrived,
}
impl LinearSpline {
    pub fn launch(
        mut points: Vec<Point>,
        current: Point,
        velocity: Real,
    ) -> Result<Self, MotionError> {
        current.validate()?;
        if !(2..=MAX_PATH_POINTS).contains(&points.len()) || velocity == Real::ZERO {
            return Err(MotionError::Invalid("spline path/speed"));
        }
        velocity.nonnegative("spline speed")?;
        for point in &points {
            point.validate()?;
        }
        // MoveSplineInit::Launch replaces the first vertex with ComputePosition
        // when the old spline is running, otherwise the current map position.
        points[0] = current;
        let inverse = Real::integer(1000).div(velocity)?;
        let mut time = 1u32;
        let mut timestamps_ms = vec![0];
        for pair in points.windows(2) {
            let duration = numeric::length(pair[0], pair[1])?.mul(inverse)?;
            // C++ int32 += float rounds the running integer to float BEFORE
            // addition, then truncates the combined value after every segment.
            time = numeric::trunc_ms(Real::integer(time).add(duration)?)?;
            timestamps_ms.push(time);
        }
        if time == 0 {
            *timestamps_ms.last_mut().unwrap() = 1;
        }
        Ok(Self {
            state: SplineSnapshot {
                points,
                velocity,
                timestamps_ms,
                elapsed_ms: 0,
                segment: 0,
                finalized: false,
            },
        })
    }
    pub fn restore(state: SplineSnapshot) -> Result<Self, MotionError> {
        let first = *state
            .points
            .first()
            .ok_or(MotionError::Invalid("empty saved spline"))?;
        let rebuilt = Self::launch(state.points.clone(), first, state.velocity)?;
        if rebuilt.state.timestamps_ms != state.timestamps_ms
            || state.elapsed_ms > rebuilt.duration_ms()
            || state.segment >= state.points.len() - 1
        {
            return Err(MotionError::Invalid("saved spline timing"));
        }
        if state.finalized {
            if state.elapsed_ms != rebuilt.duration_ms() || state.segment != state.points.len() - 2
            {
                return Err(MotionError::Invalid("saved spline arrival"));
            }
        } else if state.elapsed_ms < state.timestamps_ms[state.segment]
            || (state.elapsed_ms > state.timestamps_ms[state.segment + 1]
                || (state.elapsed_ms == state.timestamps_ms[state.segment + 1]
                    && state.timestamps_ms[state.segment + 1] > state.timestamps_ms[state.segment]))
        {
            // Unadvanced, zero-duration segments are source-valid immediately at launch.
            if state.elapsed_ms != 0 || state.segment != 0 {
                return Err(MotionError::Invalid("saved spline segment"));
            }
        }
        Ok(Self { state })
    }
    pub fn snapshot(&self) -> SplineSnapshot {
        self.state.clone()
    }
    pub fn finalized(&self) -> bool {
        self.state.finalized
    }
    pub fn duration_ms(&self) -> u32 {
        *self.state.timestamps_ms.last().unwrap()
    }
    pub fn elapsed_ms(&self) -> u32 {
        self.state.elapsed_ms
    }
    pub fn destination(&self) -> Point {
        *self.state.points.last().unwrap()
    }
    pub fn velocity(&self) -> Real {
        self.state.velocity
    }
    pub fn position(&self) -> Result<Point, MotionError> {
        let i = self.state.segment;
        let duration = self.state.timestamps_ms[i + 1] - self.state.timestamps_ms[i];
        let u = if duration == 0 {
            Real::ONE
        } else {
            Real::integer(self.state.elapsed_ms - self.state.timestamps_ms[i])
                .div(Real::integer(duration))?
        };
        numeric::interpolate(self.state.points[i], self.state.points[i + 1], u)
    }
    /// Source updateState consumes each complete segment and discards overshoot
    /// after final arrival. A newly launched spline receives no previous overshoot.
    pub fn advance(&mut self, elapsed_ms: u32) -> Vec<SplineEvent> {
        let mut events = vec![];
        if self.state.finalized {
            return events;
        }
        let mut remaining = elapsed_ms;
        loop {
            let until_next =
                self.state.timestamps_ms[self.state.segment + 1] - self.state.elapsed_ms;
            let step = remaining.min(until_next);
            self.state.elapsed_ms += step;
            remaining -= step;
            if self.state.elapsed_ms >= self.state.timestamps_ms[self.state.segment + 1] {
                self.state.segment += 1;
                if self.state.segment == self.state.points.len() - 1 {
                    self.state.segment -= 1;
                    self.state.finalized = true;
                    self.state.elapsed_ms = self.duration_ms();
                    events.push(SplineEvent::Arrived);
                    break;
                }
                events.push(SplineEvent::NextSegment {
                    segment: self.state.segment,
                });
            }
            if remaining == 0 {
                break;
            }
        }
        events
    }
}
