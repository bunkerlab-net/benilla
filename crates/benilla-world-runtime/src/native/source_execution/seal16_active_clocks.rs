// SPDX-License-Identifier: GPL-3.0-or-later
//! Frozen clock inputs, never computed outcome inputs.
use super::*;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub(in super::super) struct ActiveClock {
    pub site_line: u64,
    pub time_line: u64,
    pub thread: u64,
    pub site: [u64; 3],
    pub module: bool,
    pub seconds: i64,
    pub raw: u64,
}
#[derive(Clone, Default)]
pub(in super::super) struct ActiveClocks(pub VecDeque<ActiveClock>);
impl ActiveClocks {
    pub fn creature_respawn(&mut self, thread: u64, raw: u64) -> Result<ActiveClock> {
        let sample = self.0.pop_front().ok_or_else(|| {
            refuse("execution.creature.Creature_1698.loaded_respawn_calendar_owner")
        })?;
        // Pinned Creature.cpp:1698, instrumented :1719. addr2line resolves 0x9b2039;
        // objdump shows time@plt at 0x9b2034, then the respawn-time compare at 0x9b2039.
        // seal16 trace map1:60310427/60310429, map0:60383356/60383360.
        require(
            sample.site == [1, 3678789157770927477, 0x9b2039]
                && sample.module
                && sample.thread == thread
                && sample.raw == raw
                && sample.site_line < sample.time_line,
            "execution.creature.Creature_1698.recorded_clock_callsite_mismatch",
        )?;
        Ok(sample)
    }
}

#[test]
fn active_clock_consumes_matching_site_once_and_rejects_same_module_other_pc() {
    let sample = ActiveClock {
        site_line: 1,
        time_line: 2,
        thread: 7,
        site: [1, 3678789157770927477, 0x9b2039],
        module: true,
        seconds: 42,
        raw: 3,
    };
    let mut clocks = ActiveClocks([sample.clone()].into());
    assert_eq!(clocks.creature_respawn(7, 3).unwrap().seconds, 42);
    assert_eq!(
        clocks.creature_respawn(7, 3).unwrap_err().id,
        "execution.creature.Creature_1698.loaded_respawn_calendar_owner"
    );
    for kind in 0..5 {
        let mut wrong = sample.clone();
        match kind {
            0 => wrong.site[2] = 0x6f33d8,
            1 => wrong.thread = 6,
            2 => wrong.raw = 4,
            3 => wrong.module = false,
            _ => wrong.site_line = 3,
        }
        assert_eq!(
            ActiveClocks([wrong].into())
                .creature_respawn(7, 3)
                .unwrap_err()
                .id,
            "execution.creature.Creature_1698.recorded_clock_callsite_mismatch"
        );
    }
}
