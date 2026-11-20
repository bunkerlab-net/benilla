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
    fn aura_time(&mut self, thread: u64, raw: u64, pc: u64) -> Result<ActiveClock> {
        let sample = self
            .0
            .pop_front()
            .ok_or_else(|| refuse("execution.spells.aura_calendar_owner"))?;
        require(
            sample.site == [1, 3678789157770927477, pc]
                && sample.module
                && sample.thread == thread
                && sample.raw == raw
                && sample.site_line < sample.time_line,
            "execution.spells.aura_recorded_clock_callsite_mismatch",
        )?;
        Ok(sample)
    }
    pub fn aura_holder(&mut self, thread: u64, raw: u64) -> Result<ActiveClock> {
        // SpellAuras.cpp:5174, recording60477313/60477315, time@plt return PC.
        self.aura_time(thread, raw, 8250845)
    }
    pub fn aura(&mut self, thread: u64, raw: u64) -> Result<ActiveClock> {
        // SpellAuras.cpp:270, recording60477319/60477320, time@plt return PC.
        self.aura_time(thread, raw, 8249833)
    }
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

    pub fn solenor_respawn(&mut self, thread: u64, raw: u64) -> Result<ActiveClock> {
        let sample = self
            .0
            .pop_front()
            .ok_or_else(|| refuse("execution.creature.Creature_801.time_owner"))?;
        // Creature.h:801, emitted at the silithus.cpp:86 call in both Reset bodies.
        let expected = match raw {
            13928 => (60657679, 60657681, 0xb6a5e6),
            13930 => (60672952, 60672954, 0xb6a3ba),
            _ => {
                return Err(refuse(
                    "execution.creature.Creature_801.recorded_clock_callsite_mismatch",
                ))
            }
        };
        require(
            sample.module
                && sample.thread == thread
                && sample.raw == raw
                && (sample.site_line, sample.time_line, sample.site[2]) == expected
                && sample.site[..2] == [1, 3678789157770927477],
            "execution.creature.Creature_801.recorded_clock_callsite_mismatch",
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

#[test]
fn aura_clocks_bind_holder_then_effect_at_the_same_raw_position() {
    let holder = ActiveClock {
        site_line: 60477313,
        time_line: 60477315,
        thread: 6,
        site: [1, 3678789157770927477, 8250845],
        module: true,
        seconds: 1794314190,
        raw: 27080,
    };
    let mut aura = holder.clone();
    aura.site_line = 60477319;
    aura.time_line = 60477320;
    aura.site[2] = 8249833;
    let mut clocks = ActiveClocks([holder.clone(), aura.clone()].into());
    assert_eq!(clocks.aura_holder(6, 27080).unwrap().time_line, 60477315);
    assert_eq!(clocks.aura(6, 27080).unwrap().time_line, 60477320);
    assert!(clocks.0.is_empty());
    for (sample, use_holder, thread, raw) in [
        (holder.clone(), false, 6, 27080),
        (aura, true, 6, 27080),
        (holder.clone(), true, 7, 27080),
        (holder, true, 6, 27081),
    ] {
        let mut clocks = ActiveClocks([sample].into());
        let result = if use_holder {
            clocks.aura_holder(thread, raw)
        } else {
            clocks.aura(thread, raw)
        };
        assert_eq!(
            result.unwrap_err().id,
            "execution.spells.aura_recorded_clock_callsite_mismatch"
        );
    }
}
