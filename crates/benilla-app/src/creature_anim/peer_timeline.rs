//! P2P-only deterministic M2 variation clock. Realm animation keeps the shared CRT stream.
//! The owner supplies a generation and elapsed duration, never a sequence or asset path.

/// Incremental timeline evaluation is bounded even for a late join or malformed local model.
pub(super) struct Timeline {
    seed: u32,
    start: f64,
    end: f64,
    index: usize,
}

impl Timeline {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            seed,
            start: 0.0,
            end: 0.0,
            index: 0,
        }
    }

    fn draw(&mut self) -> u16 {
        // Same CRT weighted/replay draws as AnimRng; isolated only for the fork's shared emote.
        self.seed = self.seed.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.seed >> 16) & 0x7fff) as u16
    }

    pub(super) fn sample(
        &mut self,
        elapsed: f64,
        mut pick: impl FnMut(u16, u16) -> (usize, f64),
    ) -> Option<(usize, f64)> {
        if !elapsed.is_finite() || elapsed < 0.0 {
            return None;
        }
        for _ in 0..256 {
            if elapsed < self.end {
                return Some((self.index, elapsed - self.start));
            }
            let variation = self.draw();
            let replay = self.draw();
            let (index, span) = pick(variation, replay);
            if !span.is_finite() || span <= 0.0 {
                return None;
            }
            self.index = index;
            self.start = self.end;
            self.end += span;
        }
        None
    }
}

/// A sender-relative clock: duplicate/older snapshots never rewind; no shared OS clock needed.
pub(super) struct Clock {
    pub(super) generation: u64,
    origin: f64,
}
impl Clock {
    pub(super) fn new(generation: u64, elapsed: f64, now: f64) -> Self {
        Self {
            generation,
            origin: now - elapsed,
        }
    }
    pub(super) fn observe(&mut self, generation: u64, elapsed: f64, now: f64) {
        if generation == self.generation {
            self.origin = self.origin.min(now - elapsed);
        }
    }
    pub(super) fn elapsed(&self, now: f64) -> f64 {
        (now - self.origin).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pick(a: u16, b: u16) -> (usize, f64) {
        let variant = usize::from(a >= 16384);
        (variant, (variant as f64 + 1.0) * f64::from(1 + b / 16384))
    }
    #[test]
    fn different_frame_rates_and_late_join_choose_same_segment_and_phase() {
        let mut fast = Timeline::new(42);
        let mut slow = Timeline::new(42);
        for i in 0..2000 {
            fast.sample(i as f64 / 100.0, pick);
        }
        let expected = fast.sample(20.0, pick);
        assert_eq!(slow.sample(20.0, pick), expected);
        for i in 21..200 {
            assert_eq!(fast.sample(i as f64, pick), slow.sample(i as f64, pick));
        }
    }
    #[test]
    fn latency_duplicates_and_out_of_order_snapshots_do_not_rewind() {
        let mut c = Clock::new(5, 2.0, 102.1);
        assert!((c.elapsed(103.1) - 3.0).abs() < 1e-9);
        c.observe(5, 2.0, 103.0); // duplicate
        c.observe(5, 1.0, 103.1); // older sample
        c.observe(4, 100.0, 103.1); // old generation
        assert!((c.elapsed(103.1) - 3.0).abs() < 1e-9);
        c.observe(5, 4.0, 104.02); // a lower latency snapshot improves the origin
        assert!((c.elapsed(104.02) - 4.0).abs() < 1e-9);
    }
    #[test]
    fn catch_up_is_bounded_but_eventually_reaches_the_same_timeline() {
        let mut t = Timeline::new(1);
        let mut calls = 0;
        assert!(t
            .sample(1000.5, |_, _| {
                calls += 1;
                (0, 1.0)
            })
            .is_none());
        assert_eq!(calls, 256);
        let mut result = None;
        for _ in 0..4 {
            result = t.sample(1000.5, |_, _| (0, 1.0));
        }
        assert_eq!(result, Some((0, 0.5)));
        assert!(t.sample(f64::NAN, pick).is_none());
        assert!(Timeline::new(1).sample(1.0, |_, _| (0, 0.0)).is_none());
    }
}
