// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored-fixture runtime freshness diagnostics, never source authority.
//! Fixed categories are inclusive wall durations: parent and child must not be summed.
use std::{
    cell::Cell,
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    CountOnly,
    Clocked,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperationCounts {
    pub started: usize,
    pub succeeded: usize,
    /// Operation error/false predicate or early return, not a verification failure
    /// count. Healthy absent archive metadata returns NotFound and lands here.
    pub refused: usize,
    pub unwound: usize,
    pub elapsed: Duration,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub verify: OperationCounts,
    pub open_chain: OperationCounts,
    pub directory_scan: OperationCounts,
    pub metadata: OperationCounts,
    /// The existing bounded archive-open API is opaque; no decoder subphase claim.
    pub archive_open_decode: OperationCounts,
    pub receipt_probe: OperationCounts,
    pub member_profile: OperationCounts,
    pub payload_read: OperationCounts,
    pub payload_hash: OperationCounts,
    /// A false receipt predicate is refused, distinct from an I/O error.
    pub receipt_compare: OperationCounts,
}
impl Counts {
    /// Compare operation/outcome counts, not clock overhead or elapsed time.
    pub fn same_counts(&self, other: &Self) -> bool {
        let normalized = |counts: &Self| {
            let mut counts = *counts;
            for phase in Phase::ALL {
                counts.phase(phase).elapsed = Duration::ZERO;
            }
            counts
        };
        normalized(self) == normalized(other)
    }
    fn phase(&mut self, phase: Phase) -> &mut OperationCounts {
        match phase {
            Phase::Verify => &mut self.verify,
            Phase::OpenChain => &mut self.open_chain,
            Phase::DirectoryScan => &mut self.directory_scan,
            Phase::Metadata => &mut self.metadata,
            Phase::ArchiveOpenDecode => &mut self.archive_open_decode,
            Phase::ReceiptProbe => &mut self.receipt_probe,
            Phase::MemberProfile => &mut self.member_profile,
            Phase::PayloadRead => &mut self.payload_read,
            Phase::PayloadHash => &mut self.payload_hash,
            Phase::ReceiptCompare => &mut self.receipt_compare,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Phase {
    Verify,
    OpenChain,
    DirectoryScan,
    Metadata,
    ArchiveOpenDecode,
    ReceiptProbe,
    MemberProfile,
    PayloadRead,
    PayloadHash,
    ReceiptCompare,
}
impl Phase {
    const ALL: [Self; 10] = [
        Self::Verify,
        Self::OpenChain,
        Self::DirectoryScan,
        Self::Metadata,
        Self::ArchiveOpenDecode,
        Self::ReceiptProbe,
        Self::MemberProfile,
        Self::PayloadRead,
        Self::PayloadHash,
        Self::ReceiptCompare,
    ];
}
#[derive(Clone, Copy)]
struct Active {
    mode: Mode,
    counts: Counts,
}
thread_local! {
    static ACTIVE: Cell<Option<Active>> = const { Cell::new(None) };
    static RUNTIME: Cell<bool> = const { Cell::new(false) };
}

/// Measurement alone cannot instrument discovery. Only the runtime map freshness
/// entry enables operations. Fixed stack/TLS storage contains no member/path data.
pub fn measure<T>(mode: Mode, run: impl FnOnce() -> T) -> (T, Counts) {
    struct Reset(Option<Active>);
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.set(self.0));
        }
    }
    let next = (mode != Mode::Off).then_some(Active {
        mode,
        counts: Counts::default(),
    });
    let reset = Reset(ACTIVE.with(|active| active.replace(next)));
    let result = run();
    let counts = ACTIVE.with(|active| active.get().map_or(Counts::default(), |a| a.counts));
    drop(reset);
    (result, counts)
}

/// Private, thread-bound marker is installed only by the runtime map API.
pub(crate) struct RuntimeScope {
    previous: bool,
    thread: PhantomData<Rc<()>>,
}
impl RuntimeScope {
    pub(crate) fn enter() -> Self {
        Self {
            previous: RUNTIME.with(|runtime| runtime.replace(true)),
            thread: PhantomData,
        }
    }
}
impl Drop for RuntimeScope {
    fn drop(&mut self) {
        RUNTIME.with(|runtime| runtime.set(self.previous));
    }
}

pub(crate) struct Timer {
    active: bool,
    started: Option<Instant>,
    phase: Phase,
    succeeded: bool,
    thread: PhantomData<Rc<()>>,
}
impl Timer {
    pub(crate) fn start(phase: Phase) -> Self {
        let mode = if RUNTIME.with(Cell::get) {
            ACTIVE.with(|active| {
                let mut state = active.get()?;
                state.counts.phase(phase).started += 1;
                active.set(Some(state));
                Some(state.mode)
            })
        } else {
            None
        };
        Self {
            active: mode.is_some(),
            started: (mode == Some(Mode::Clocked)).then(Instant::now),
            phase,
            succeeded: false,
            thread: PhantomData,
        }
    }
    pub(crate) fn finish(mut self, succeeded: bool) {
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
            if let Some(mut state) = active.get() {
                let phase = state.counts.phase(self.phase);
                phase.elapsed += elapsed;
                if std::thread::panicking() {
                    phase.unwound += 1;
                } else if self.succeeded {
                    phase.succeeded += 1;
                } else {
                    phase.refused += 1;
                }
                active.set(Some(state));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_scopes_are_bounded_nested_unwind_safe_and_thread_local() {
        for mode in [Mode::Off, Mode::CountOnly, Mode::Clocked] {
            let (_, unmarked) = measure(mode, || Timer::start(Phase::Verify).finish(true));
            assert_eq!(unmarked, Counts::default());
        }
        let (_, off) = measure(Mode::Off, || {
            let _runtime = RuntimeScope::enter();
            let timer = Timer::start(Phase::Verify);
            assert!(!timer.active);
            assert!(timer.started.is_none());
            timer.finish(true);
        });
        assert_eq!(off, Counts::default());
        let (_, counts) = measure(Mode::CountOnly, || {
            let _runtime = RuntimeScope::enter();
            let timer = Timer::start(Phase::Verify);
            assert!(timer.active);
            assert!(timer.started.is_none());
            timer.finish(true);
            let (_, inner) = measure(Mode::Clocked, || {
                let _inner = RuntimeScope::enter();
                Timer::start(Phase::PayloadRead).finish(false);
            });
            assert_eq!(inner.payload_read.refused, 1);
            std::thread::spawn(|| {
                let (_, counts) = measure(Mode::Clocked, || {
                    Timer::start(Phase::Verify).finish(true);
                });
                assert_eq!(counts, Counts::default());
            })
            .join()
            .unwrap();
            assert!(std::panic::catch_unwind(|| {
                measure(Mode::Clocked, || {
                    let _inner = RuntimeScope::enter();
                    let _timer = Timer::start(Phase::PayloadRead);
                    panic!("nested runtime measurement must restore");
                });
            })
            .is_err());
            Timer::start(Phase::Verify).finish(true);
        });
        assert_eq!(counts.verify.started, 2);
        assert_eq!(counts.verify.succeeded, 2);
        assert_eq!(counts.verify.elapsed, Duration::ZERO);
        assert_eq!(counts.payload_read, OperationCounts::default());
        assert!(!RUNTIME.with(Cell::get));
        assert!(ACTIVE.with(Cell::get).is_none());
        let (_, unwound) = measure(Mode::CountOnly, || {
            assert!(std::panic::catch_unwind(|| {
                let _runtime = RuntimeScope::enter();
                let _timer = Timer::start(Phase::Verify);
                panic!("unwind is not an ordinary refusal");
            })
            .is_err());
            assert!(!RUNTIME.with(Cell::get));
        });
        assert_eq!(unwound.verify.unwound, 1);
        assert_eq!(unwound.verify.refused, 0);
    }
}
