// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! General EventAI lifecycle reducers. No action execution or TEXT duplication.
use super::*;

/// CreatureEventAIMgr.cpp:266-278 narrows before capping. Zero remains loaded.
pub fn loaded_chance(raw: u32) -> u8 {
    (raw as u8).min(100)
}

/// CreatureEventAI.cpp:35-48. Reversal disables without changing the timer.
pub fn update_holder_timer<E>(
    timer: &mut u32,
    enabled: &mut bool,
    range: [u32; 2],
    draw: &mut impl FnMut(u32, u32) -> Result<u32, E>,
) -> Result<bool, E> {
    if range[0] == range[1] {
        *timer = range[0];
    } else if range[1] > range[0] {
        *timer = draw(range[0], range[1])?;
    } else {
        *enabled = false;
        return Ok(false);
    }
    Ok(true)
}

/// CreatureEventAI.cpp:165-191, not the larger IsTimerBasedEvent set.
pub fn timer_executed_event(kind: u32) -> bool {
    matches!(kind, 0..=3 | 9 | 12..=16 | 18 | 23 | 24 | 27..=29 | 31..=33)
}

/// CreatureEventAI.cpp:209-241, including callback events with reset timers.
pub fn timer_based_event(kind: u32) -> bool {
    matches!(kind, 0..=3 | 5 | 8..=10 | 12..=18 | 23..=29 | 31..=34)
}

/// Shared unsigned debt wraps as uint32 upstream, CreatureEventAI.cpp:1929-1975.
/// Completion is separate because ProcessEvents precedes resetting the clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct EventClock {
    pub remaining: u32,
    pub debt: u32,
}
impl EventClock {
    pub const fn constructed() -> Self {
        Self {
            remaining: 0,
            debt: 0,
        }
    }
    pub fn reset(&mut self) {
        self.remaining = EVENT_UPDATE_MS;
        self.debt = 0;
    }
    pub fn begin_update(&mut self, diff: u32) -> Option<u32> {
        self.debt = self.debt.wrapping_add(diff);
        if self.remaining < diff {
            Some(self.debt)
        } else {
            self.remaining -= diff;
            None
        }
    }
}
