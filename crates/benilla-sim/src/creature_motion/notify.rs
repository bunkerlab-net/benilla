// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Unit::ScheduleAINotify, OnRelocated, EnterCombat and respawn lifecycle.
use super::MotionError;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggroNotifySnapshot {
    pub event_clock_ms: u64,
    pub pending_at_ms: Option<u64>,
    pub can_aggro: bool,
}
/// The notification clock advances in Unit's event phase, BEFORE spline/AI.
/// Execution emits a visitor obligation; it does not assert visibility, LOS,
/// accessibility, hostility, membership of nearby cells, or actual aggro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggroNotify {
    state: AggroNotifySnapshot,
}
impl Default for AggroNotify {
    fn default() -> Self {
        Self::new()
    }
}
impl AggroNotify {
    pub fn new() -> Self {
        Self {
            state: AggroNotifySnapshot {
                event_clock_ms: 0,
                pending_at_ms: None,
                can_aggro: false,
            },
        }
    }
    pub fn snapshot(&self) -> AggroNotifySnapshot {
        self.state.clone()
    }
    pub fn restore(state: AggroNotifySnapshot) -> Result<Self, MotionError> {
        if state
            .pending_at_ms
            .is_some_and(|at| at < state.event_clock_ms)
        {
            return Err(MotionError::Invalid("past AI notify"));
        }
        Ok(Self { state })
    }
    pub fn can_aggro(&self) -> bool {
        self.state.can_aggro
    }
    pub fn schedule(&mut self, delay_ms: u32, forced: bool) -> Result<(), MotionError> {
        if self.state.pending_at_ms.is_none() || forced {
            let at = self
                .state
                .event_clock_ms
                .checked_add(u64::from(delay_ms))
                .ok_or(MotionError::Invalid("AI notify clock"))?;
            self.state.pending_at_ms = Some(at);
        }
        Ok(())
    }
    /// Creature constructor starts CanAggro=false; its initial add schedules
    /// the configured ordinary-creature delay, just as respawn does.
    pub fn added_to_world(&mut self, configured_delay_ms: u32) -> Result<(), MotionError> {
        self.schedule(configured_delay_ms, false)
    }
    pub fn visibility_changed(&mut self) -> Result<(), MotionError> {
        self.schedule(0, false)
    }
    pub fn relocated(&mut self, configured_delay_ms: u32) -> Result<(), MotionError> {
        self.schedule(configured_delay_ms, false)
    }
    pub fn respawned(&mut self, configured_delay_ms: u32) -> Result<(), MotionError> {
        let mut staged = self.clone();
        staged.state.can_aggro = false;
        staged.schedule(configured_delay_ms, true)?;
        *self = staged;
        Ok(())
    }
    pub fn entered_combat(&mut self) {
        if !self.state.can_aggro {
            self.state.can_aggro = true;
            self.state.pending_at_ms = None;
        }
    }
    pub fn abort(&mut self) {
        self.state.pending_at_ms = None;
    }
    /// Returns true exactly when the source nearby-object visitor must execute.
    /// Caller resolves the actual current visitor set before committing the frame.
    pub fn advance_events(&mut self, elapsed_ms: u32, alive: bool) -> Result<bool, MotionError> {
        let now = self
            .state
            .event_clock_ms
            .checked_add(u64::from(elapsed_ms))
            .ok_or(MotionError::Invalid("AI event clock"))?;
        self.state.event_clock_ms = now;
        if self.state.pending_at_ms.is_some_and(|at| at <= now) {
            self.state.pending_at_ms = None;
            self.state.can_aggro = alive;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// UnitVisitObjectsInRangeNotifyEvent::Execute: integer max before binary32
/// multiplication; this is not the map's visibility distance.
pub fn visitor_radius(
    detection_range: u32,
    aggro_rate: crate::creature_ai::Real,
) -> Result<crate::creature_ai::Real, MotionError> {
    aggro_rate.nonnegative("AI notify aggro rate")?;
    Ok(crate::creature_ai::Real::integer(detection_range.max(45)).mul(aggro_rate)?)
}
