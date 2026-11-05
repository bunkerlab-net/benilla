// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f contributors;
// upstream GPL-2.0-or-later, Rust translation GPL-3.0-or-later.
//! Pure pinned LP64 timer arithmetic from explicit millisecond observations.
//! No host clock, persistence, map execution, or native-world admission.

/// Refuse undefined signed addition/subtraction without mutation; Timer.h:84,92,136.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    SignedOverflow,
}

/// Narrow an already observed relative millisecond count; Util.cpp:60-64.
pub const fn narrow_milliseconds(elapsed_ms: i64) -> u32 {
    elapsed_ms as u32
}

/// Use the source's minimum backward/wrap difference; Timer.h:50-60.
pub fn world_timer_diff(old_ms: u32, new_ms: u32) -> u32 {
    if old_ms > new_ms {
        ((u32::MAX - old_ms) + new_ms).min(old_ms - new_ms)
    } else {
        new_ms - old_ms
    }
}

/// Store narrowed current/previous observations without sampling a clock; Util.cpp:42-57.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldTimer {
    current: u32,
    previous: u32,
}

impl WorldTimer {
    /// Advance from an explicit relative millisecond observation; Util.cpp:48-64.
    pub fn tick(&mut self, elapsed_ms: i64) -> u32 {
        self.previous = self.current;
        self.current = narrow_milliseconds(elapsed_ms);
        world_timer_diff(self.previous, self.current)
    }

    /// Return the current narrowed sample; Util.cpp:45.
    pub fn tick_time(&self) -> u32 {
        self.current
    }

    /// Return the previous narrowed sample; Util.cpp:46.
    pub fn tick_prev_time(&self) -> u32 {
        self.previous
    }
}

/// Preserve signed LP64 time_t interval/current values and zero defaults; Timer.h:77-103.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IntervalTimer {
    interval: i64,
    current: i64,
}

impl IntervalTimer {
    /// Add a signed diff before clamping negative current to zero; Timer.h:82-87.
    pub fn update(&mut self, diff: i64) -> Result<(), Error> {
        let current = self
            .current
            .checked_add(diff)
            .ok_or(Error::SignedOverflow)?;
        self.current = current.max(0);
        Ok(())
    }

    /// Include equality in the interval threshold; Timer.h:88.
    pub fn passed(&self) -> bool {
        self.current >= self.interval
    }

    /// Subtract exactly one interval only when passed; Timer.h:89-93.
    pub fn reset(&mut self) -> Result<(), Error> {
        if self.passed() {
            self.current = self
                .current
                .checked_sub(self.interval)
                .ok_or(Error::SignedOverflow)?;
        }
        Ok(())
    }

    /// Assign current directly without a clamp; Timer.h:95.
    pub fn set_current(&mut self, current: i64) {
        self.current = current;
    }

    /// Assign the signed interval directly; Timer.h:96.
    pub fn set_interval(&mut self, interval: i64) {
        self.interval = interval;
    }

    /// Return the signed LP64 interval; Timer.h:97.
    pub fn get_interval(&self) -> i64 {
        self.interval
    }

    /// Return the signed LP64 accumulator; Timer.h:98.
    pub fn get_current(&self) -> i64 {
        self.current
    }
}

/// Keep signed LP64 expiry, including values below zero; Timer.h:132-143.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeTracker {
    expiry: i64,
}

impl TimeTracker {
    /// Initialize expiry without clamping; Timer.h:135.
    pub fn new(expiry: i64) -> Self {
        Self { expiry }
    }

    /// Subtract a signed diff without saturating at zero; Timer.h:136.
    pub fn update(&mut self, diff: i64) -> Result<(), Error> {
        self.expiry = self.expiry.checked_sub(diff).ok_or(Error::SignedOverflow)?;
        Ok(())
    }

    /// Treat zero and negative expiry as passed; Timer.h:137.
    pub fn passed(&self) -> bool {
        self.expiry <= 0
    }

    /// Replace expiry with the signed interval; Timer.h:138.
    pub fn reset(&mut self, interval: i64) {
        self.expiry = interval;
    }

    /// Return the signed LP64 expiry; Timer.h:139.
    pub fn get_expiry(&self) -> i64 {
        self.expiry
    }
}

/// Keep uint32 expiry with zero saturation, not signed TimeTracker semantics; Timer.h:145-162.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShortTimeTracker {
    expiry: u32,
}

impl ShortTimeTracker {
    /// Initialize an unsigned expiry; Timer.h:148.
    pub fn new(expiry: u32) -> Self {
        Self { expiry }
    }

    /// Clamp expiry to zero when the diff reaches or exceeds it; Timer.h:149-155.
    pub fn update(&mut self, diff: u32) {
        self.expiry = self.expiry.saturating_sub(diff);
    }

    /// Treat only zero unsigned expiry as passed; Timer.h:156.
    pub fn passed(&self) -> bool {
        self.expiry == 0
    }

    /// Replace expiry with the unsigned interval; Timer.h:157.
    pub fn reset(&mut self, interval: u32) {
        self.expiry = interval;
    }

    /// Return the uint32 expiry; Timer.h:158.
    pub fn get_expiry(&self) -> u32 {
        self.expiry
    }
}

/// Compute one map dispatch diff, not map execution or worker ordering; MapManager.cpp:194-227.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapIntervalAccumulator {
    timer: IntervalTimer,
}

impl MapIntervalAccumulator {
    /// Widen the configured uint32 interval into time_t; MapManager.cpp:40, MapManager.h:180.
    pub fn new(interval: u32) -> Self {
        let mut timer = IntervalTimer::default();
        timer.set_interval(i64::from(interval));
        Self { timer }
    }

    /// Return the whole current narrowed to uint32 once, then clear it; MapManager.cpp:194-227.
    pub fn update(&mut self, diff: u32) -> Result<Option<u32>, Error> {
        self.timer.update(i64::from(diff))?;
        if !self.timer.passed() {
            return Ok(None);
        }
        let diff = self.timer.get_current() as u32;
        self.timer.set_current(0);
        Ok(Some(diff))
    }

    /// Expose the untruncated signed accumulator for arithmetic comparison; Timer.h:98.
    pub fn get_current(&self) -> i64 {
        self.timer.get_current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_diff_uses_source_backward_and_wrap_formula() {
        assert_eq!(world_timer_diff(0xffff_fff0, 5), 20);
        assert_eq!(world_timer_diff(100, 99), 1);
        assert_eq!(world_timer_diff(u32::MAX, 0), 0);
        assert_eq!(world_timer_diff(0, u32::MAX), u32::MAX);
        assert_eq!(world_timer_diff(100, 150), 50);
        assert_eq!(world_timer_diff(100, 100), 0);
        assert_eq!(world_timer_diff(0x8000_0000, 0), 0x7fff_ffff);
    }

    #[test]
    fn milliseconds_narrow_to_uint32_without_clamping() {
        assert_eq!(narrow_milliseconds(0), 0);
        assert_eq!(narrow_milliseconds(4_294_967_296), 0);
        assert_eq!(narrow_milliseconds(4_294_967_301), 5);
        assert_eq!(narrow_milliseconds(-1), u32::MAX);
        assert_eq!(narrow_milliseconds(i64::MIN), 0);
        assert_eq!(narrow_milliseconds(i64::MAX), u32::MAX);
    }

    #[test]
    fn world_tick_keeps_previous_and_current_narrowed_samples() {
        let mut timer = WorldTimer::default();
        assert_eq!((timer.tick_prev_time(), timer.tick_time()), (0, 0));
        assert_eq!(timer.tick(4_294_967_280), 0xffff_fff0);
        assert_eq!(
            (timer.tick_prev_time(), timer.tick_time()),
            (0, 0xffff_fff0)
        );
        assert_eq!(timer.tick(4_294_967_301), 20);
        assert_eq!(
            (timer.tick_prev_time(), timer.tick_time()),
            (0xffff_fff0, 5)
        );
        assert_eq!(timer.tick(4_294_967_301), 0);
        assert_eq!((timer.tick_prev_time(), timer.tick_time()), (5, 5));
        assert_eq!(timer.tick(4_294_967_300), 1);
    }

    #[test]
    fn interval_defaults_and_threshold_equality_match_source() {
        let mut timer = IntervalTimer::default();
        assert_eq!((timer.get_interval(), timer.get_current()), (0, 0));
        assert!(timer.passed());
        timer.set_interval(100);
        assert!(!timer.passed());
        timer.update(0).unwrap();
        assert_eq!(timer.get_current(), 0);
        timer.update(99).unwrap();
        assert!(!timer.passed());
        timer.update(1).unwrap();
        assert!(timer.passed());
        timer.reset().unwrap();
        assert_eq!(timer.get_current(), 0);
        assert!(!timer.passed());
    }

    #[test]
    fn interval_reset_subtracts_only_one_interval_when_passed() {
        let mut timer = IntervalTimer::default();
        timer.set_interval(100);
        timer.set_current(250);
        timer.reset().unwrap();
        assert_eq!(timer.get_current(), 150);
        assert!(timer.passed());
        timer.reset().unwrap();
        assert_eq!(timer.get_current(), 50);
        assert!(!timer.passed());
        timer.reset().unwrap();
        assert_eq!(timer.get_current(), 50);
        timer.set_current(0);
        assert_eq!(timer.get_current(), 0);
    }

    #[test]
    fn interval_clamps_only_after_update_and_accepts_negative_diffs() {
        let mut timer = IntervalTimer::default();
        timer.set_interval(10);
        timer.set_current(20);
        timer.update(-5).unwrap();
        assert_eq!(timer.get_current(), 15);
        timer.update(-30).unwrap();
        assert_eq!(timer.get_current(), 0);
        timer.set_current(-20);
        assert_eq!(timer.get_current(), -20);
        assert!(!timer.passed());
        timer.update(25).unwrap();
        assert_eq!(timer.get_current(), 5);
        timer.set_current(-20);
        timer.update(0).unwrap();
        assert_eq!(timer.get_current(), 0);
        timer.set_interval(-10);
        assert!(timer.passed());
        timer.reset().unwrap();
        assert_eq!(timer.get_current(), 10);
    }

    #[test]
    fn interval_refuses_undefined_signed_addition_before_clamping() {
        let mut timer = IntervalTimer::default();
        timer.set_current(i64::MAX);
        assert_eq!(timer.update(1), Err(Error::SignedOverflow));
        assert_eq!(timer.get_current(), i64::MAX);
        timer.set_current(i64::MIN);
        assert_eq!(timer.update(-1), Err(Error::SignedOverflow));
        assert_eq!(timer.get_current(), i64::MIN);
        timer.update(0).unwrap();
        assert_eq!(timer.get_current(), 0);
        timer.update(i64::MIN).unwrap();
        assert_eq!(timer.get_current(), 0);
    }

    #[test]
    fn interval_reset_refuses_undefined_signed_subtraction() {
        let mut timer = IntervalTimer::default();
        timer.set_current(i64::MAX);
        timer.set_interval(-1);
        assert_eq!(timer.reset(), Err(Error::SignedOverflow));
        assert_eq!(timer.get_current(), i64::MAX);
        assert_eq!(timer.get_interval(), -1);
        timer.set_current(i64::MIN);
        timer.set_interval(1);
        timer.reset().unwrap();
        assert_eq!(timer.get_current(), i64::MIN);
    }

    #[test]
    fn signed_expiry_crosses_zero_and_negative_diffs_increase_it() {
        let mut timer = TimeTracker::new(10);
        assert!(!timer.passed());
        timer.update(0).unwrap();
        assert_eq!(timer.get_expiry(), 10);
        timer.update(10).unwrap();
        assert!(timer.passed());
        assert_eq!(timer.get_expiry(), 0);
        timer.update(7).unwrap();
        assert_eq!(timer.get_expiry(), -7);
        assert!(timer.passed());
        timer.update(-10).unwrap();
        assert_eq!(timer.get_expiry(), 3);
        assert!(!timer.passed());
        timer.reset(-20);
        assert_eq!(timer.get_expiry(), -20);
        assert!(timer.passed());
        assert!(TimeTracker::new(0).passed());
    }

    #[test]
    fn signed_expiry_refuses_undefined_subtraction_without_mutation() {
        let mut timer = TimeTracker::new(i64::MIN);
        assert_eq!(timer.update(1), Err(Error::SignedOverflow));
        assert_eq!(timer.get_expiry(), i64::MIN);
        timer.reset(i64::MAX);
        assert_eq!(timer.update(-1), Err(Error::SignedOverflow));
        assert_eq!(timer.get_expiry(), i64::MAX);
        timer.update(i64::MAX).unwrap();
        assert_eq!(timer.get_expiry(), 0);
        assert_eq!(timer.update(i64::MIN), Err(Error::SignedOverflow));
        assert_eq!(timer.get_expiry(), 0);
    }

    #[test]
    fn short_expiry_is_unsigned_and_saturates_instead_of_going_negative() {
        let mut timer = ShortTimeTracker::default();
        assert_eq!(timer.get_expiry(), 0);
        assert!(timer.passed());
        timer.update(u32::MAX);
        assert_eq!(timer.get_expiry(), 0);
        timer.reset(10);
        timer.update(0);
        assert_eq!(timer.get_expiry(), 10);
        assert!(!timer.passed());
        timer.update(9);
        assert_eq!(timer.get_expiry(), 1);
        timer.update(1);
        assert_eq!(timer.get_expiry(), 0);
        assert!(timer.passed());
        let mut timer = ShortTimeTracker::new(10);
        timer.update(11);
        assert_eq!(timer.get_expiry(), 0);
        timer.reset(u32::MAX);
        timer.update(1);
        assert_eq!(timer.get_expiry(), 4_294_967_294);
    }

    #[test]
    fn map_accumulator_dispatches_whole_diff_once_then_clears_remainder() {
        let mut timer = MapIntervalAccumulator::new(100);
        assert_eq!(timer.update(50), Ok(None));
        assert_eq!(timer.get_current(), 50);
        assert_eq!(timer.update(49), Ok(None));
        assert_eq!(timer.get_current(), 99);
        assert_eq!(timer.update(0), Ok(None));
        assert_eq!(timer.update(1), Ok(Some(100)));
        assert_eq!(timer.get_current(), 0);
        assert_eq!(timer.update(0), Ok(None));
        assert_eq!(timer.update(250), Ok(Some(250)));
        assert_eq!(timer.get_current(), 0);
        assert_eq!(timer.update(0), Ok(None));
        assert_eq!(timer.update(99), Ok(None));
        assert_eq!(timer.update(1), Ok(Some(100)));
    }

    #[test]
    fn map_accumulator_uses_lp64_current_then_truncates_dispatch_to_uint32() {
        let mut timer = MapIntervalAccumulator::new(u32::MAX);
        assert_eq!(timer.update(u32::MAX - 1), Ok(None));
        assert_eq!(timer.get_current(), 4_294_967_294);
        assert_eq!(timer.update(u32::MAX), Ok(Some(4_294_967_293)));
        assert_eq!(timer.get_current(), 0);
        assert_eq!(timer.update(0), Ok(None));
    }

    #[test]
    fn zero_map_interval_dispatches_even_a_zero_diff() {
        let mut timer = MapIntervalAccumulator::new(0);
        assert_eq!(timer.update(0), Ok(Some(0)));
        assert_eq!(timer.update(7), Ok(Some(7)));
        assert_eq!(timer.get_current(), 0);
    }

    #[test]
    fn public_integer_types_match_uint32_and_lp64_time_t() {
        let _: fn(u32, u32) -> u32 = world_timer_diff;
        let _: fn(i64) -> u32 = narrow_milliseconds;
        let _: fn(&mut WorldTimer, i64) -> u32 = WorldTimer::tick;
        let _: fn(&WorldTimer) -> u32 = WorldTimer::tick_time;
        let _: fn(&WorldTimer) -> u32 = WorldTimer::tick_prev_time;
        let _: fn(&mut IntervalTimer, i64) -> Result<(), Error> = IntervalTimer::update;
        let _: fn(&mut IntervalTimer, i64) = IntervalTimer::set_current;
        let _: fn(&mut IntervalTimer, i64) = IntervalTimer::set_interval;
        let _: fn(&IntervalTimer) -> i64 = IntervalTimer::get_current;
        let _: fn(&IntervalTimer) -> i64 = IntervalTimer::get_interval;
        let _: fn(i64) -> TimeTracker = TimeTracker::new;
        let _: fn(&mut TimeTracker, i64) -> Result<(), Error> = TimeTracker::update;
        let _: fn(&mut TimeTracker, i64) = TimeTracker::reset;
        let _: fn(&TimeTracker) -> i64 = TimeTracker::get_expiry;
        let _: fn(u32) -> ShortTimeTracker = ShortTimeTracker::new;
        let _: fn(&mut ShortTimeTracker, u32) = ShortTimeTracker::update;
        let _: fn(&mut ShortTimeTracker, u32) = ShortTimeTracker::reset;
        let _: fn(&ShortTimeTracker) -> u32 = ShortTimeTracker::get_expiry;
        let _: fn(u32) -> MapIntervalAccumulator = MapIntervalAccumulator::new;
        let _: fn(&mut MapIntervalAccumulator, u32) -> Result<Option<u32>, Error> =
            MapIntervalAccumulator::update;
        let _: fn(&MapIntervalAccumulator) -> i64 = MapIntervalAccumulator::get_current;
        let mut interval = IntervalTimer::default();
        interval.set_interval(4_294_967_296);
        interval.update(4_294_967_295).unwrap();
        assert!(!interval.passed());
        interval.update(1).unwrap();
        assert_eq!(interval.get_interval(), 4_294_967_296);
        assert_eq!(interval.get_current(), 4_294_967_296);
        assert!(interval.passed());
        let mut expiry = TimeTracker::new(4_294_967_296);
        expiry.update(1).unwrap();
        assert_eq!(expiry.get_expiry(), 4_294_967_295);
    }
}
