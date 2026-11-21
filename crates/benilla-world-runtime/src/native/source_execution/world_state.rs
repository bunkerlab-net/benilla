// SPDX-License-Identifier: GPL-3.0-or-later
use super::{refuse, Result, SourceWorldClock};

pub type WorldStateRow<'a> = (u32, Option<&'a [u8]>);

#[derive(Default)]
struct AhnQiraj {
    phase: u32,
    timer_ms: u64,
    counters: [u32; 30],
    phase2_tier: u32,
    killed_bosses: u32,
}

struct ScourgeInvasion {
    state: u32,
    timers_ms: [i64; 8],
    battles_won: u32,
    last_attack_zone: Option<u32>,
    remaining: [u32; 6],
}

pub(super) struct SourceWorldState {
    dragon_mask: u8,
    dragon_timer_ms: u32,
    dragon_positions: [u32; 4],
    aq: AhnQiraj,
    love_counters: [u32; 8],
    invasion: ScourgeInvasion,
}

impl Default for SourceWorldState {
    fn default() -> Self {
        // WorldState.cpp:95-97,1058-1060; WorldState.h:204-217,286-292,455.
        Self {
            dragon_mask: 0xF,
            dragon_timer_ms: 0,
            dragon_positions: [0; 4],
            aq: AhnQiraj::default(),
            love_counters: [0; 8],
            invasion: ScourgeInvasion {
                state: 0,
                timers_ms: [0; 8],
                battles_won: 0,
                // CreationPolicy.h:35-38 does not zero fields omitted by WorldState.cpp:1058.
                last_attack_zone: None,
                remaining: [0; 6],
            },
        }
    }
}

impl SourceWorldState {
    // WorldState.cpp:108-214 visits rows in result order. Field.h:49-52 reads a C string.
    pub(super) fn load_rows(
        &mut self,
        rows: &[WorldStateRow<'_>],
        clock: &SourceWorldClock,
    ) -> Result<()> {
        for &(id, raw) in rows {
            let raw = raw.unwrap_or_default();
            let text = raw.split(|&byte| byte == 0).next().unwrap_or_default();
            if text.is_empty() {
                // WorldState.cpp:127,145,171,183-184,188 leaves only Love reset on empty text.
                if id == 2 {
                    self.love_counters.fill(0);
                }
                continue;
            }
            let mut stream = SaveStream {
                rest: text,
                failed: false,
            };
            match id {
                // WorldState.cpp:123-141, WorldState.h:441-443.
                0 => {
                    let mask = stream.u32().unwrap_or(0xF);
                    let respawn_s = stream.unsigned(u64::MAX);
                    self.dragon_mask = mask as u8;
                    stream.fill(&mut self.dragon_positions);
                    let respawn_s = respawn_s.ok_or_else(|| {
                        refuse("execution.world.WorldState_134.uninitialized_dragon_time")
                    })?;
                    self.dragon_timer_ms = if respawn_s == 0 {
                        0
                    } else {
                        remaining_millis(
                            respawn_s,
                            clock,
                            "execution.world.WorldState_136.dragon_clock_overflow",
                        )? as u32
                    };
                }
                // WorldState.cpp:143-166, WorldState.h:204-217.
                1 => {
                    stream.assign(&mut self.aq.phase);
                    let respawn_s = stream.unsigned(u64::MAX).ok_or_else(|| {
                        refuse("execution.world.WorldState_152.uninitialized_aq_time")
                    })?;
                    if respawn_s != 0 {
                        self.aq.timer_ms = remaining_millis(
                            respawn_s,
                            clock,
                            "execution.world.WorldState_154.aq_clock_overflow",
                        )? as u64;
                    }
                    stream.fill(&mut self.aq.counters);
                    stream.assign(&mut self.aq.phase2_tier);
                    stream.assign(&mut self.aq.killed_bosses);
                }
                // WorldState.cpp:169-185, WorldState.h:306-321.
                2 => stream.fill(&mut self.love_counters),
                // WorldState.cpp:186-210, WorldState.h:222-228,242-264,286-292.
                3 => {
                    self.invasion.state = stream.u32().ok_or_else(|| {
                        refuse("execution.world.WorldState_193.uninitialized_invasion_state")
                    })?;
                    for timer in &mut self.invasion.timers_ms {
                        let time = stream.unsigned(u64::MAX).ok_or_else(|| {
                            refuse("execution.world.WorldState_198.uninitialized_invasion_time")
                        })?;
                        *timer = epoch_millis(
                            time,
                            "execution.world.WorldState_198.invasion_clock_overflow",
                        )?;
                    }
                    stream.assign(&mut self.invasion.battles_won);
                    if let Some(zone) = stream.u32() {
                        self.invasion.last_attack_zone = Some(zone);
                    }
                    stream.fill(&mut self.invasion.remaining);
                }
                _ => (),
            }
        }
        Ok(())
    }

    pub(super) fn war_effort(&self) -> (u32, u32) {
        (self.aq.phase, self.aq.killed_bosses)
    }
}

// WorldState.cpp:691-710 dispatches event starts for active war effort phases.
pub(super) fn start_war_effort_event(
    phase: u32,
    killed_bosses: u32,
    mut start: impl FnMut(u16) -> Result<()>,
) -> Result<()> {
    if let 1..=5 = phase {
        start(119 + phase as u16)?;
        if phase == 4 {
            for (bit, id) in [(1, 125), (2, 126), (4, 127)] {
                if killed_bosses & bit != 0 {
                    start(id)?;
                }
            }
        }
    }
    Ok(())
}

fn epoch_millis(seconds: u64, undefined: &'static str) -> Result<i64> {
    // Common.h:58-59 and libstdc++ chrono.h:1257-1261 convert through native nanoseconds.
    (seconds as i64)
        .checked_mul(1_000_000_000)
        .map(|ns| ns / 1_000_000)
        .ok_or_else(|| refuse(undefined))
}

fn remaining_millis(
    seconds: u64,
    clock: &SourceWorldClock,
    undefined: &'static str,
) -> Result<i64> {
    // World.h:581 returns the existing cached clock.
    epoch_millis(seconds, undefined)?
        .checked_sub(clock.current_ms())
        .ok_or_else(|| refuse(undefined))
}

struct SaveStream<'a> {
    rest: &'a [u8],
    failed: bool,
}

impl SaveStream<'_> {
    // WorldState.cpp:120 uses a classic-locale istringstream. A failed extraction may assign
    // zero or saturate once; later extractions leave their destination unchanged.
    fn unsigned(&mut self, max: u64) -> Option<u64> {
        if self.failed {
            return None;
        }
        while let Some((&(b' ' | b'\t'..=b'\r'), tail)) = self.rest.split_first() {
            self.rest = tail;
        }
        let (&first, digits) = self.rest.split_first()?;
        let (negative, digits) = match first {
            b'-' => (true, digits),
            b'+' => (false, digits),
            _ => (false, self.rest),
        };
        let mut value = 0u64;
        let mut len = 0;
        for &byte in digits.iter().take_while(|b| b.is_ascii_digit()) {
            len += 1;
            let digit = u64::from(byte - b'0');
            if value > (max - digit) / 10 {
                self.failed = true;
                return Some(max);
            }
            value = value * 10 + digit;
        }
        self.rest = &digits[len..];
        self.failed = len == 0;
        Some(if negative {
            value.wrapping_neg() & max
        } else {
            value
        })
    }

    fn u32(&mut self) -> Option<u32> {
        self.unsigned(u64::from(u32::MAX)).map(|value| value as u32)
    }

    fn assign(&mut self, target: &mut u32) {
        if let Some(value) = self.u32() {
            *target = value;
        }
    }

    fn fill(&mut self, fields: &mut [u32]) {
        for field in fields {
            self.assign(field);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_state_load_restores_all_four_saved_row_types() {
        let mut world = SourceWorldState::default();
        let clock = SourceWorldClock::default();
        assert_eq!(world.dragon_mask, 0xF);
        assert_eq!(world.dragon_timer_ms, 0);
        assert_eq!(world.dragon_positions, [0; 4]);
        assert_eq!(world.war_effort(), (0, 0));
        assert_eq!(world.invasion.last_attack_zone, None);
        world
            .load_rows(
                &[
                    (0, Some(b"256 4294968 14890 14888 14887 14889")),
                    (
                        1,
                        Some(b"4 2 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 5 7"),
                    ),
                    (2, Some(b"10 20 30 40 50 60 70 80\0discard")),
                    (3, Some(b"1 0 1 2 3 4 5 6 7 9 139 1 2 3 4 5 6")),
                    (4, Some(b"ignored")),
                ],
                &clock,
            )
            .unwrap();
        assert_eq!(world.dragon_mask, 0);
        assert_eq!(world.dragon_timer_ms, 704);
        assert_eq!(world.dragon_positions, [14890, 14888, 14887, 14889]);
        assert_eq!(world.war_effort(), (4, 7));
        assert_eq!(world.aq.timer_ms, 2_000);
        assert_eq!(world.aq.counters, core::array::from_fn(|i| i as u32 + 1));
        assert_eq!(world.aq.phase2_tier, 5);
        assert_eq!(world.love_counters, [10, 20, 30, 40, 50, 60, 70, 80]);
        assert_eq!(world.invasion.state, 1);
        assert_eq!(
            world.invasion.timers_ms,
            [0, 1000, 2000, 3000, 4000, 5000, 6000, 7000]
        );
        assert_eq!(world.invasion.battles_won, 9);
        assert_eq!(world.invasion.last_attack_zone, Some(139));
        assert_eq!(world.invasion.remaining, [1, 2, 3, 4, 5, 6]);

        world
            .load_rows(
                &[(0, None), (1, Some(b"4 0")), (2, None), (3, Some(b""))],
                &clock,
            )
            .unwrap();
        assert_eq!(world.dragon_timer_ms, 704);
        assert_eq!(world.aq.timer_ms, 2_000);
        assert_eq!(world.love_counters, [0; 8]);
        assert_eq!(world.invasion.state, 1);
    }

    #[test]
    fn world_state_load_keeps_defined_partial_extraction() {
        let mut world = SourceWorldState::default();
        let clock = SourceWorldClock::default();
        world
            .load_rows(&[(2, Some(b"1 2 3 4 5 6 7 8"))], &clock)
            .unwrap();
        world
            .load_rows(&[(2, Some(b"-1\x0b+9 nope 88"))], &clock)
            .unwrap();
        assert_eq!(world.love_counters, [u32::MAX, 9, 0, 4, 5, 6, 7, 8]);
        world
            .load_rows(&[(2, Some(b"4 4294967296 6"))], &clock)
            .unwrap();
        assert_eq!(world.love_counters, [4, u32::MAX, 0, 4, 5, 6, 7, 8]);
        world
            .load_rows(&[(3, Some(b"1 0 1 2 3 4 5 6 7 9"))], &clock)
            .unwrap();
        assert_eq!(world.invasion.battles_won, 9);
        assert_eq!(world.invasion.last_attack_zone, None);
        world.load_rows(&[(0, Some(b"7 x 99"))], &clock).unwrap();
        assert_eq!(
            (
                world.dragon_mask,
                world.dragon_timer_ms,
                world.dragon_positions
            ),
            (7, 0, [0; 4])
        );
    }

    #[test]
    fn world_state_load_refuses_undefined_times_at_the_source_site() {
        let mut world = SourceWorldState::default();
        let clock = SourceWorldClock::default();
        world
            .load_rows(&[(0, Some(b"7 9223372036"))], &clock)
            .unwrap();
        for (id, text, refusal) in [
            (
                0,
                &b"7"[..],
                "execution.world.WorldState_134.uninitialized_dragon_time",
            ),
            (
                0,
                &b"7 9223372037"[..],
                "execution.world.WorldState_136.dragon_clock_overflow",
            ),
            (
                1,
                &b"4 "[..],
                "execution.world.WorldState_152.uninitialized_aq_time",
            ),
            (
                1,
                &b"4 9223372037"[..],
                "execution.world.WorldState_154.aq_clock_overflow",
            ),
            (
                3,
                &b" "[..],
                "execution.world.WorldState_193.uninitialized_invasion_state",
            ),
            (
                3,
                &b"1 0"[..],
                "execution.world.WorldState_198.uninitialized_invasion_time",
            ),
            (
                3,
                &b"1 9223372037"[..],
                "execution.world.WorldState_198.invasion_clock_overflow",
            ),
        ] {
            assert_eq!(
                world.load_rows(&[(id, Some(text))], &clock).unwrap_err().id,
                refusal
            );
        }
    }

    #[test]
    fn world_state_starts_war_effort_events_in_source_order() {
        for (phase, bosses, expected) in [
            (0, 7, &[][..]),
            (1, 7, &[120][..]),
            (2, 7, &[121][..]),
            (3, 7, &[122][..]),
            (4, 0, &[123][..]),
            (4, 7, &[123, 125, 126, 127][..]),
            (5, 7, &[124][..]),
            (6, 7, &[][..]),
        ] {
            let mut calls = Vec::new();
            start_war_effort_event(phase, bosses, |id| {
                calls.push(id);
                Ok(())
            })
            .unwrap();
            assert_eq!(calls, expected);
        }
    }
}
