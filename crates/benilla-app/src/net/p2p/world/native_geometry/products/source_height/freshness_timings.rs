// SPDX-License-Identifier: GPL-3.0-or-later
//! Scoped test-thread timings; observations never replace a source verification.
use std::{
    cell::Cell,
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PhaseCounts {
    started: usize,
    succeeded: usize,
    refused: usize,
    unwound: usize,
    elapsed: Duration,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    total: PhaseCounts,
    terrain: PhaseCounts,
    collision: PhaseCounts,
}
#[derive(Clone, Copy)]
pub(super) enum Phase {
    Total,
    Terrain,
    Collision,
}
impl Counts {
    fn phase(&mut self, phase: Phase) -> &mut PhaseCounts {
        match phase {
            Phase::Total => &mut self.total,
            Phase::Terrain => &mut self.terrain,
            Phase::Collision => &mut self.collision,
        }
    }
}
thread_local! {
    static ACTIVE: Cell<Option<Counts>> = const { Cell::new(None) };
}

/// Nested scopes report their own calls, then restore the outer counts. An outer
/// timer's wall duration still includes nested work. Unwinding also restores state.
pub(crate) fn measure<T>(run: impl FnOnce() -> T) -> (T, Counts) {
    struct Reset(Option<Counts>);
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.set(self.0));
        }
    }
    let previous = ACTIVE.with(|active| active.replace(Some(Counts::default())));
    let reset = Reset(previous);
    let result = run();
    let counts = ACTIVE.with(|active| active.get().expect("measurement is active"));
    drop(reset);
    (result, counts)
}

/// Non-Send guard remains on the measured thread. Disabled scopes read no clock.
pub(super) struct Timer {
    started: Option<Instant>,
    phase: Phase,
    succeeded: bool,
    thread: PhantomData<Rc<()>>,
}
impl Timer {
    pub(super) fn start(phase: Phase) -> Self {
        let started = ACTIVE.with(|active| {
            let mut counts = active.get()?;
            counts.phase(phase).started += 1;
            active.set(Some(counts));
            Some(Instant::now())
        });
        Self {
            started,
            phase,
            succeeded: false,
            thread: PhantomData,
        }
    }
    pub(super) fn finish(mut self, succeeded: bool) {
        self.succeeded = succeeded;
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        let Some(started) = self.started else {
            return;
        };
        let elapsed = started.elapsed();
        ACTIVE.with(|active| {
            if let Some(mut counts) = active.get() {
                let phase = counts.phase(self.phase);
                phase.elapsed += elapsed;
                if std::thread::panicking() {
                    phase.unwound += 1;
                } else if self.succeeded {
                    phase.succeeded += 1;
                } else {
                    phase.refused += 1;
                }
                active.set(Some(counts));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_isolate_nested_threads_and_restore_after_unwind() {
        assert!(Timer::start(Phase::Total).started.is_none());
        let (_, outer) = measure(|| {
            Timer::start(Phase::Total).finish(true);
            let (_, inner) = measure(|| {
                drop(Timer::start(Phase::Terrain));
                Timer::start(Phase::Collision).finish(true);
            });
            assert_eq!(inner.terrain.started, 1);
            assert_eq!(inner.terrain.refused, 1);
            assert_eq!(inner.collision.succeeded, 1);
            std::thread::spawn(|| {
                assert!(Timer::start(Phase::Total).started.is_none());
            })
            .join()
            .unwrap();
            assert!(std::panic::catch_unwind(|| {
                measure(|| {
                    let _timer = Timer::start(Phase::Total);
                    panic!("measurement must restore its parent");
                });
            })
            .is_err());
            Timer::start(Phase::Total).finish(true);
        });
        assert_eq!(outer.total.started, 2);
        assert_eq!(outer.total.succeeded, 2);
        assert_eq!(outer.total.refused, 0);
        assert_eq!(outer.total.unwound, 0);
        assert_eq!(outer.terrain, PhaseCounts::default());
        assert_eq!(outer.collision, PhaseCounts::default());
        assert!(Timer::start(Phase::Total).started.is_none());
        let (_, failed) = measure(|| {
            let _ = std::panic::catch_unwind(|| {
                let _timer = Timer::start(Phase::Total);
                panic!("unwind is not a normal refusal");
            });
        });
        assert_eq!(failed.total.started, 1);
        assert_eq!(failed.total.unwound, 1);
        assert_eq!(failed.total.refused, 0);
    }
}
