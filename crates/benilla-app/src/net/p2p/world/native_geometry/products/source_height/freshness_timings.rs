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
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    total: PhaseCounts,
    terrain: PhaseCounts,
    collision: PhaseCounts,
    context_validation: PhaseCounts,
    concrete_corridor: PhaseCounts,
    full_resident_dispatch: PhaseCounts,
    full_gate_model: PhaseCounts,
    initialized_owner_height: PhaseCounts,
    invocation_match: PhaseCounts,
    provider_guard: PhaseCounts,
    provider_active: PhaseCounts,
    provider_lease: PhaseCounts,
    provider_identity: PhaseCounts,
    provider_session: PhaseCounts,
    provider_profile: PhaseCounts,
    provider_request: PhaseCounts,
    phase_entry_prechecks: PhaseCounts,
    result_prechecks: PhaseCounts,
    calculate_callback: PhaseCounts,
    result_seal: PhaseCounts,
    fifo_delivery: PhaseCounts,
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
    InvocationMatch,
    ProviderGuard,
    ProviderActive,
    ProviderLease,
    ProviderIdentity,
    ProviderSession,
    ProviderProfile,
    ProviderRequest,
    // The following outer intervals are disjoint, not an exhaustive partition.
    // All preceding validation/source categories are inclusive children.
    EntryPrechecks,
    ResultPrechecks,
    CalculateCallback,
    ResultSeal,
    FifoDelivery,
}
impl Phase {
    fn detailed(self) -> bool {
        !matches!(
            self,
            Self::Total
                | Self::Terrain
                | Self::Collision
                | Self::ContextValidation
                | Self::ConcreteCorridor
                | Self::FullResidentDispatch
                | Self::FullGateModel
                | Self::InitializedOwnerHeight
        )
    }
}
// Legacy measurements retain their original output shape and category set.
impl std::fmt::Debug for Counts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut output = formatter.debug_struct("Counts");
        output.field("total", &self.total);
        output.field("terrain", &self.terrain);
        output.field("collision", &self.collision);
        output.field("context_validation", &self.context_validation);
        output.field("concrete_corridor", &self.concrete_corridor);
        output.field("full_resident_dispatch", &self.full_resident_dispatch);
        output.field("full_gate_model", &self.full_gate_model);
        output.field("initialized_owner_height", &self.initialized_owner_height);
        if self.invocation_match.started != 0 {
            output.field("invocation_match", &self.invocation_match);
        }
        if self.provider_guard.started != 0 {
            output.field("provider_guard", &self.provider_guard);
        }
        if self.provider_active.started != 0 {
            output.field("provider_active", &self.provider_active);
        }
        if self.provider_lease.started != 0 {
            output.field("provider_lease", &self.provider_lease);
        }
        if self.provider_identity.started != 0 {
            output.field("provider_identity", &self.provider_identity);
        }
        if self.provider_session.started != 0 {
            output.field("provider_session", &self.provider_session);
        }
        if self.provider_profile.started != 0 {
            output.field("provider_profile", &self.provider_profile);
        }
        if self.provider_request.started != 0 {
            output.field("provider_request", &self.provider_request);
        }
        if self.phase_entry_prechecks.started != 0 {
            output.field("phase_entry_prechecks", &self.phase_entry_prechecks);
        }
        if self.result_prechecks.started != 0 {
            output.field("result_prechecks", &self.result_prechecks);
        }
        if self.calculate_callback.started != 0 {
            output.field("calculate_callback", &self.calculate_callback);
        }
        if self.result_seal.started != 0 {
            output.field("result_seal", &self.result_seal);
        }
        if self.fifo_delivery.started != 0 {
            output.field("fifo_delivery", &self.fifo_delivery);
        }
        output.finish()
    }
}
impl Counts {
    pub(crate) fn legacy_only(mut self) -> Self {
        self.invocation_match = PhaseCounts::default();
        self.provider_guard = PhaseCounts::default();
        self.provider_active = PhaseCounts::default();
        self.provider_lease = PhaseCounts::default();
        self.provider_identity = PhaseCounts::default();
        self.provider_session = PhaseCounts::default();
        self.provider_profile = PhaseCounts::default();
        self.provider_request = PhaseCounts::default();
        self.phase_entry_prechecks = PhaseCounts::default();
        self.result_prechecks = PhaseCounts::default();
        self.calculate_callback = PhaseCounts::default();
        self.result_seal = PhaseCounts::default();
        self.fifo_delivery = PhaseCounts::default();
        self
    }

    pub(crate) fn same_counts(&self, other: &Self) -> bool {
        let normalize = |mut counts: Self| {
            counts.total.elapsed = Duration::ZERO;
            counts.terrain.elapsed = Duration::ZERO;
            counts.collision.elapsed = Duration::ZERO;
            counts.context_validation.elapsed = Duration::ZERO;
            counts.concrete_corridor.elapsed = Duration::ZERO;
            counts.full_resident_dispatch.elapsed = Duration::ZERO;
            counts.full_gate_model.elapsed = Duration::ZERO;
            counts.initialized_owner_height.elapsed = Duration::ZERO;
            counts.invocation_match.elapsed = Duration::ZERO;
            counts.provider_guard.elapsed = Duration::ZERO;
            counts.provider_active.elapsed = Duration::ZERO;
            counts.provider_lease.elapsed = Duration::ZERO;
            counts.provider_identity.elapsed = Duration::ZERO;
            counts.provider_session.elapsed = Duration::ZERO;
            counts.provider_profile.elapsed = Duration::ZERO;
            counts.provider_request.elapsed = Duration::ZERO;
            counts.phase_entry_prechecks.elapsed = Duration::ZERO;
            counts.result_prechecks.elapsed = Duration::ZERO;
            counts.calculate_callback.elapsed = Duration::ZERO;
            counts.result_seal.elapsed = Duration::ZERO;
            counts.fifo_delivery.elapsed = Duration::ZERO;
            counts
        };
        normalize(*self) == normalize(*other)
    }

    pub(in super::super::super) fn context_validation_calls(&self) -> usize {
        self.context_validation.started
    }
    pub(in super::super::super) fn full_resident_dispatch_calls(&self) -> usize {
        self.full_resident_dispatch.started
    }
    pub(in super::super::super) fn full_gate_model_calls(&self) -> usize {
        self.full_gate_model.started
    }
    pub(in super::super::super) fn initialized_owner_height_calls(&self) -> usize {
        self.initialized_owner_height.started
    }
    pub(in super::super::super) fn started(&self, phase: Phase) -> usize {
        let mut counts = *self;
        counts.phase(phase).started
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
            Phase::InvocationMatch => &mut self.invocation_match,
            Phase::ProviderGuard => &mut self.provider_guard,
            Phase::ProviderActive => &mut self.provider_active,
            Phase::ProviderLease => &mut self.provider_lease,
            Phase::ProviderIdentity => &mut self.provider_identity,
            Phase::ProviderSession => &mut self.provider_session,
            Phase::ProviderProfile => &mut self.provider_profile,
            Phase::ProviderRequest => &mut self.provider_request,
            Phase::EntryPrechecks => &mut self.phase_entry_prechecks,
            Phase::ResultPrechecks => &mut self.result_prechecks,
            Phase::CalculateCallback => &mut self.calculate_callback,
            Phase::ResultSeal => &mut self.result_seal,
            Phase::FifoDelivery => &mut self.fifo_delivery,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Off,
    CountOnly,
    Clocked,
}
#[derive(Clone, Copy)]
struct Active {
    mode: Mode,
    detailed: bool,
    counts: Counts,
}
thread_local! {
    static ACTIVE: Cell<Option<Active>> = const { Cell::new(None) };
}

/// Nested scopes report their own calls, then restore the outer counts. An outer
/// timer's wall duration still includes nested work. Unwinding also restores state.
pub(crate) fn measure<T>(run: impl FnOnce() -> T) -> (T, Counts) {
    measure_scope(Mode::Clocked, false, run)
}

/// Off performs no observer clock reads, record allocations or I/O. CountOnly
/// records the identical fixed categories without reading observer clocks.
pub(crate) fn measure_mode<T>(mode: Mode, run: impl FnOnce() -> T) -> (T, Counts) {
    measure_scope(mode, true, run)
}

fn measure_scope<T>(mode: Mode, detailed: bool, run: impl FnOnce() -> T) -> (T, Counts) {
    struct Reset(Option<Active>);
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.set(self.0));
        }
    }
    let current = (mode != Mode::Off).then_some(Active {
        mode,
        detailed,
        counts: Counts::default(),
    });
    let previous = ACTIVE.with(|active| active.replace(current));
    let reset = Reset(previous);
    let result = run();
    let counts = ACTIVE.with(|active| active.get().map_or(Counts::default(), |v| v.counts));
    drop(reset);
    (result, counts)
}

/// Evaluates one original short-circuit mismatch operand exactly once, in place.
/// A false operand is a successful comparison, not permission to omit later work.
pub(in super::super::super) fn mismatch(phase: Phase, compare: impl FnOnce() -> bool) -> bool {
    let timer = Timer::start(phase);
    let mismatch = compare();
    timer.finish(!mismatch);
    mismatch
}

/// Non-Send guard remains on the measured thread. Disabled scopes read no clock.
pub(in super::super::super) struct Timer {
    started: Option<Instant>,
    active: bool,
    phase: Phase,
    succeeded: bool,
    thread: PhantomData<Rc<()>>,
}
impl Timer {
    pub(in super::super::super) fn start(phase: Phase) -> Self {
        let mode = ACTIVE.with(|active| {
            let mut current = active.get()?;
            if phase.detailed() && !current.detailed {
                return None;
            }
            current.counts.phase(phase).started += 1;
            active.set(Some(current));
            Some(current.mode)
        });
        let started = (mode == Some(Mode::Clocked)).then(Instant::now);
        Self {
            started,
            active: mode.is_some(),
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
        if !self.active {
            return;
        }
        let elapsed = self
            .started
            .map_or(Duration::ZERO, |started| started.elapsed());
        ACTIVE.with(|active| {
            if let Some(mut current) = active.get() {
                let phase = current.counts.phase(self.phase);
                phase.elapsed += elapsed;
                if std::thread::panicking() {
                    phase.unwound += 1;
                } else if self.succeeded {
                    phase.succeeded += 1;
                } else {
                    phase.refused += 1;
                }
                active.set(Some(current));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_preserve_outcomes_and_short_circuit_without_clocking_counts() {
        let run = || {
            let mut visited = Vec::new();
            let refused = mismatch(Phase::ProviderLease, || {
                visited.push(1);
                false
            }) || mismatch(Phase::ProviderIdentity, || {
                visited.push(2);
                true
            }) || mismatch(Phase::ProviderProfile, || {
                visited.push(3);
                false
            });
            assert!(refused);
            assert_eq!(visited, [1, 2]);
        };
        let (_, legacy) = measure(|| {
            let timer = Timer::start(Phase::ProviderGuard);
            assert!(!timer.active && timer.started.is_none());
            Timer::start(Phase::Total).finish(true);
        });
        assert_eq!(legacy.provider_guard, PhaseCounts::default());
        assert_eq!(legacy.total.started, 1);
        let phases = [
            Phase::InvocationMatch,
            Phase::ProviderGuard,
            Phase::ProviderActive,
            Phase::ProviderLease,
            Phase::ProviderIdentity,
            Phase::ProviderSession,
            Phase::ProviderProfile,
            Phase::ProviderRequest,
            Phase::EntryPrechecks,
            Phase::ResultPrechecks,
            Phase::CalculateCallback,
            Phase::ResultSeal,
            Phase::FifoDelivery,
        ];
        let (_, detail) = measure_mode(Mode::CountOnly, || {
            for phase in phases {
                Timer::start(phase).finish(true);
            }
        });
        for phase in phases {
            assert_eq!(detail.started(phase), 1);
        }
        assert_eq!(detail.legacy_only(), Counts::default());
        let (_, off) = measure_mode(Mode::Off, run);
        assert_eq!(off, Counts::default());
        let (_, counted) = measure_mode(Mode::CountOnly, || {
            let timer = Timer::start(Phase::Total);
            assert!(timer.started.is_none());
            assert!(timer.active);
            timer.finish(true);
            run();
        });
        let (_, clocked) = measure_mode(Mode::Clocked, || {
            Timer::start(Phase::Total).finish(true);
            run();
        });
        assert!(counted.same_counts(&clocked));
        assert_eq!(counted.total.elapsed, Duration::ZERO);
        assert_eq!(counted.provider_identity.refused, 1);
        assert_eq!(counted.provider_profile.started, 0);
        let (_, outer) = measure_mode(Mode::CountOnly, || {
            Timer::start(Phase::ProviderGuard).finish(true);
            let (_, disabled) = measure_mode(Mode::Off, || {
                let timer = Timer::start(Phase::ProviderGuard);
                assert!(!timer.active && timer.started.is_none());
            });
            assert_eq!(disabled, Counts::default());
            assert!(std::panic::catch_unwind(|| {
                measure_mode(Mode::Clocked, || panic!("restore count-only parent"));
            })
            .is_err());
            let timer = Timer::start(Phase::ProviderGuard);
            assert!(timer.started.is_none());
            timer.finish(true);
            std::thread::spawn(|| {
                assert!(!Timer::start(Phase::ProviderGuard).active);
            })
            .join()
            .unwrap();
        });
        assert_eq!(outer.provider_guard.succeeded, 2);
        assert_eq!(outer.provider_guard.elapsed, Duration::ZERO);
        assert!(!Timer::start(Phase::Total).active);
    }

    #[test]
    fn scopes_isolate_nested_threads_and_restore_after_unwind() {
        assert!(Timer::start(Phase::Total).started.is_none());
        assert_eq!(current_source_trace_len(), 0);
        let (_, trace) = trace_sources(|| {
            trace_source_check();
            assert_eq!(current_source_trace_len(), 1);
            let (_, inner) = trace_sources(|| {
                assert_eq!(current_source_trace_len(), 0);
                trace_source_check();
                assert_eq!(current_source_trace_len(), 1);
            });
            assert_eq!(inner.len(), 1);
            assert_eq!(current_source_trace_len(), 1);
            std::thread::spawn(|| assert_eq!(current_source_trace_len(), 0))
                .join()
                .unwrap();
            assert!(std::panic::catch_unwind(|| trace_sources(|| {
                trace_source_check();
                panic!("trace scope must restore on unwind");
            }))
            .is_err());
            assert_eq!(current_source_trace_len(), 1);
            trace_source_check();
        });
        assert_eq!(trace.len(), 2);
        assert!(SOURCE_TRACE.with(|trace| trace.borrow().is_none()));
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
            assert_eq!(validation.full_resident_dispatch_calls(), 2);
            assert_eq!(validation.initialized_owner_height_calls(), 2);
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

/// Ordered callsite witnesses only; never source success or cached authority.
pub(crate) type SourceTrace = Vec<(&'static str, u32, u32)>;
thread_local! {
    static SOURCE_TRACE: std::cell::RefCell<Option<SourceTrace>> = const { std::cell::RefCell::new(None) };
}
#[track_caller]
pub(super) fn trace_source_check() {
    let caller = std::panic::Location::caller();
    SOURCE_TRACE.with(|trace| {
        if let Some(trace) = trace.borrow_mut().as_mut() {
            trace.push((caller.file(), caller.line(), caller.column()));
        }
    });
}
/// Read-only diagnostic for authored cancellation/source-mutation cutoffs.
/// It neither starts a check nor alters its result or checkpoints.
pub(crate) fn current_source_trace_len() -> usize {
    SOURCE_TRACE.with(|trace| trace.borrow().as_ref().map_or(0, Vec::len))
}

pub(crate) fn trace_sources<T>(run: impl FnOnce() -> T) -> (T, SourceTrace) {
    struct Reset(Option<SourceTrace>);
    impl Drop for Reset {
        fn drop(&mut self) {
            SOURCE_TRACE.with(|trace| {
                trace.replace(self.0.take());
            });
        }
    }
    let reset = Reset(SOURCE_TRACE.with(|trace| trace.replace(Some(Vec::new()))));
    let result = run();
    let trace = SOURCE_TRACE.with(|trace| trace.borrow_mut().take().unwrap());
    drop(reset);
    (result, trace)
}
