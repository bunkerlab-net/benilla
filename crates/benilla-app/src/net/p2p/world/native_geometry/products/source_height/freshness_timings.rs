// SPDX-License-Identifier: GPL-3.0-or-later
//! Scoped test-thread timings; observations never replace a source verification.
//! Categories are inclusive wall times and may overlap; they must not be summed.
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
    context_validation: PhaseCounts,
    concrete_corridor: PhaseCounts,
    full_resident_dispatch: PhaseCounts,
    full_gate_model: PhaseCounts,
    initialized_owner_height: PhaseCounts,
}
#[derive(Clone, Copy)]
pub(in super::super::super) enum Phase {
    Total,
    Terrain,
    Collision,
    ContextValidation,
    // Only the Corridor::certify entry point, not direct enclosure certification.
    ConcreteCorridor,
    // Selected resident/filter, LOS, joined and retained callsites, not all dispatch checks.
    FullResidentDispatch,
    // The full gate_model wrapper (None), not the initialized fast path.
    FullGateModel,
    // Inclusive: may also be inside ContextValidation.
    InitializedOwnerHeight,
}
impl Counts {
    pub(in super::super::super) fn context_validation_calls(&self) -> usize {
        self.context_validation.started
    }
    fn phase(&mut self, phase: Phase) -> &mut PhaseCounts {
        match phase {
            Phase::Total => &mut self.total,
            Phase::Terrain => &mut self.terrain,
            Phase::Collision => &mut self.collision,
            Phase::ContextValidation => &mut self.context_validation,
            Phase::ConcreteCorridor => &mut self.concrete_corridor,
            Phase::FullResidentDispatch => &mut self.full_resident_dispatch,
            Phase::FullGateModel => &mut self.full_gate_model,
            Phase::InitializedOwnerHeight => &mut self.initialized_owner_height,
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
pub(in super::super::super) struct Timer {
    started: Option<Instant>,
    phase: Phase,
    succeeded: bool,
    thread: PhantomData<Rc<()>>,
}
impl Timer {
    pub(in super::super::super) fn start(phase: Phase) -> Self {
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
    pub(in super::super::super) fn finish(mut self, succeeded: bool) {
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
            let (_, phases) = measure(|| {
                Timer::start(Phase::ContextValidation).finish(true);
                Timer::start(Phase::ContextValidation).finish(false);
                Timer::start(Phase::ConcreteCorridor).finish(true);
                drop(Timer::start(Phase::ConcreteCorridor));
            });
            assert_eq!(phases.context_validation_calls(), 2);
            assert_eq!(phases.context_validation.succeeded, 1);
            assert_eq!(phases.context_validation.refused, 1);
            assert_eq!(phases.concrete_corridor.started, 2);
            assert_eq!(phases.concrete_corridor.succeeded, 1);
            assert_eq!(phases.concrete_corridor.refused, 1);
            let (_, validation) = measure(|| {
                for phase in [
                    Phase::FullResidentDispatch,
                    Phase::FullGateModel,
                    Phase::InitializedOwnerHeight,
                ] {
                    Timer::start(phase).finish(true);
                    drop(Timer::start(phase));
                }
            });
            for counts in [
                validation.full_resident_dispatch,
                validation.full_gate_model,
                validation.initialized_owner_height,
            ] {
                assert_eq!(counts.started, 2);
                assert_eq!(counts.succeeded, 1);
                assert_eq!(counts.refused, 1);
            }
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
        assert_eq!(outer.context_validation, PhaseCounts::default());
        assert_eq!(outer.concrete_corridor, PhaseCounts::default());
        assert_eq!(outer.full_resident_dispatch, PhaseCounts::default());
        assert_eq!(outer.full_gate_model, PhaseCounts::default());
        assert_eq!(outer.initialized_owner_height, PhaseCounts::default());
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
