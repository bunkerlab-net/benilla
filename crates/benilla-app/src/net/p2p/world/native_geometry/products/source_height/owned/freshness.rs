// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded test-only source observation. No native query or certification crosses threads.
use super::super::super::super::core::JobLease;
use super::super::{preparation_counts, CompositionError};
use super::OwnedSourceProducts;
use benilla_world_runtime::native::AuthorityRequest;
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, TrySendError};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnqueueError {
    Busy,
    Inactive,
    Stopped,
}

pub(crate) enum FreshnessPoll {
    Idle,
    Pending,
    Ready(Result<FreshnessObservation, CompositionError>),
    Stopped,
}

/// A single-use observation of one allocation for one original Work. It grants
/// no geometry facts, source authority, or exemption from synchronous rechecks.
pub(crate) struct FreshnessObservation {
    products: Arc<OwnedSourceProducts>,
    lease: JobLease,
    cancelled: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}
impl FreshnessObservation {
    pub(in super::super::super::super) fn consume(
        self,
        products: &Arc<OwnedSourceProducts>,
        lease: &JobLease,
    ) -> Result<(), CompositionError> {
        if !Arc::ptr_eq(&self.products, products) || !self.lease.same_job(lease) {
            return Err(super::super::super::Error::Identity.into());
        }
        self.check_active()
    }

    /// Resume exactly once on the caller thread. This is a live scheduling guard,
    /// not freshness authority: the callback must perform all current source,
    /// installed-owner and physics checks and must not publish or acknowledge.
    pub(in super::super::super::super) fn with_current<T, E: From<CompositionError>>(
        self,
        products: &Arc<OwnedSourceProducts>,
        lease: &JobLease,
        invocation_deadline: Instant,
        resume: impl FnOnce(&AuthorityRequest, Instant, &dyn Fn() -> bool) -> Result<T, E>,
    ) -> Result<T, E> {
        self.with_current_borrowed(products, lease, invocation_deadline, resume)
    }

    /// Preserve the complete original revocation guard across an owner-local yield.
    pub(in super::super::super::super) fn prepare_current<T, E: From<CompositionError>>(
        self,
        products: &Arc<OwnedSourceProducts>,
        lease: &JobLease,
        invocation_deadline: Instant,
        prepare: impl FnOnce(&AuthorityRequest, Instant, &dyn Fn() -> bool) -> Result<T, E>,
    ) -> Result<(Self, T), E> {
        let prepared = self.with_current_borrowed(products, lease, invocation_deadline, prepare)?;
        Ok((self, prepared))
    }

    fn with_current_borrowed<T, E: From<CompositionError>>(
        &self,
        products: &Arc<OwnedSourceProducts>,
        lease: &JobLease,
        invocation_deadline: Instant,
        resume: impl FnOnce(&AuthorityRequest, Instant, &dyn Fn() -> bool) -> Result<T, E>,
    ) -> Result<T, E> {
        if !Arc::ptr_eq(&self.products, products) || !self.lease.same_job(lease) {
            return Err(E::from(super::super::super::Error::Identity.into()));
        }
        let deadline = invocation_deadline.min(self.lease.deadline());
        let cancelled = || self.check_active().is_err() || Instant::now() >= deadline;
        if cancelled() {
            return Err(E::from(CompositionError::Cancelled));
        }
        let result = resume(self.lease.request(), deadline, &cancelled)?;
        // Retain the revocation tokens throughout certification, not merely until
        // polling. A successful callback cannot emit after retirement or expiry.
        if cancelled() {
            return Err(E::from(CompositionError::Cancelled));
        }
        Ok(result)
    }

    fn check_active(&self) -> Result<(), CompositionError> {
        if self.cancelled.load(Ordering::Acquire)
            || self.stop.load(Ordering::Acquire)
            || !self.lease.is_active()
        {
            Err(CompositionError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Original logical callback guard, not a renewed source observation. Only the
/// owner may use this non-Clone handle; the worker receives its exact owned tokens.
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(crate) struct F05Guard {
    observation: FreshnessObservation,
    callback_deadline: Instant,
    submitted: std::cell::Cell<bool>,
    operation: Arc<()>,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
impl FreshnessObservation {
    pub(in super::super::super::super) fn into_f05_guard(
        self,
        products: &Arc<OwnedSourceProducts>,
        lease: &JobLease,
        callback_deadline: Instant,
    ) -> Result<F05Guard, CompositionError> {
        if !Arc::ptr_eq(&self.products, products) || !self.lease.same_job(lease) {
            return Err(super::super::super::Error::Identity.into());
        }
        let callback_deadline = callback_deadline.min(self.lease.deadline());
        let guard = F05Guard {
            observation: self,
            callback_deadline,
            submitted: std::cell::Cell::new(false),
            operation: Arc::new(()),
        };
        guard.check_success()?;
        Ok(guard)
    }
}
#[cfg(all(test, not(benilla_headless_geometry)))]
impl F05Guard {
    pub(in super::super::super::super) fn matches(
        &self,
        products: &Arc<OwnedSourceProducts>,
        lease: &JobLease,
    ) -> bool {
        Arc::ptr_eq(&self.observation.products, products) && self.observation.lease.same_job(lease)
    }
    pub(in super::super::super::super) fn callback_deadline(&self) -> Instant {
        self.callback_deadline
    }
    pub(in super::super::super::super) fn request(&self) -> &AuthorityRequest {
        self.observation.lease.request()
    }
    pub(in super::super::super::super) fn lease(&self) -> &JobLease {
        &self.observation.lease
    }
    pub(in super::super::super::super) fn cancelled(&self) -> bool {
        self.observation.check_active().is_err() || Instant::now() >= self.callback_deadline
    }
    pub(in super::super::super::super) fn cancel(&self) {
        self.observation.cancelled.store(true, Ordering::Release);
    }
    /// Only call after successful logical callback completion. Raw F05 results
    /// and owed F59 source entry must never be replaced with this postguard.
    pub(in super::super::super::super) fn check_success(&self) -> Result<(), CompositionError> {
        if self.cancelled() {
            Err(CompositionError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[cfg(all(test, not(benilla_headless_geometry)))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum F05EnqueueError {
    Busy,
    Inactive,
    Stopped,
    Identity,
    AlreadySubmitted,
}
/// Opaque single-operation identity; neither a source receipt nor a reusable job.
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(crate) struct F05Slot {
    identity: Arc<()>,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(crate) struct F05Completion {
    identity: F05ProofIdentity,
    result: Result<(), CompositionError>,
    diagnostics: F05Diagnostics,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
struct F05ProofIdentity {
    operation: Arc<()>,
    products: Arc<OwnedSourceProducts>,
    lease: JobLease,
    cancelled: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    callback_deadline: Instant,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
impl F05ProofIdentity {
    fn matches_guard(&self, guard: &F05Guard) -> bool {
        Arc::ptr_eq(&self.operation, &guard.operation)
            && Arc::ptr_eq(&self.products, &guard.observation.products)
            && self.lease.same_job(&guard.observation.lease)
            && Arc::ptr_eq(&self.cancelled, &guard.observation.cancelled)
            && Arc::ptr_eq(&self.stop, &guard.observation.stop)
            && self.callback_deadline == guard.callback_deadline
            && guard.submitted.get()
    }
}
/// Diagnostic-only data may be copied; it cannot construct a completion or proof.
#[cfg(all(test, not(benilla_headless_geometry)))]
#[derive(Clone, Debug, Default)]
pub(crate) struct F05Diagnostics {
    pub(crate) trace: super::super::freshness_timings::SourceTrace,
    pub(crate) counts: Box<super::super::freshness_timings::Counts>,
    pub(crate) cancel_checks: usize,
    pub(crate) first_cancelled_check: Option<usize>,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(crate) enum F05Outcome {
    SourceError(CompositionError, F05Diagnostics),
    Success(F05VerifiedSuccess),
}
/// Unforgeable success of exactly one actual original-guard worker source call.
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(crate) struct F05VerifiedSuccess {
    identity: F05ProofIdentity,
    diagnostics: F05Diagnostics,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
impl F05VerifiedSuccess {
    pub(in super::super::super::super) fn matches_guard(&self, guard: &F05Guard) -> bool {
        self.identity.matches_guard(guard)
    }
    pub(in super::super::super::super) fn diagnostics(&self) -> &F05Diagnostics {
        &self.diagnostics
    }
    pub(in super::super::super::super) fn into_diagnostics(self) -> F05Diagnostics {
        self.diagnostics
    }
}
#[cfg(all(test, not(benilla_headless_geometry)))]
impl F05Completion {
    /// Diagnostic label only; this cannot expose or replace source-success proof.
    pub(in super::super::super::super) fn diagnostic_outcome(&self) -> String {
        match &self.result {
            Ok(()) => "Ok(())".to_owned(),
            Err(error) => format!("{error:?}"),
        }
    }
    pub(in super::super::super::super) fn diagnostics(&self) -> &F05Diagnostics {
        &self.diagnostics
    }
    /// Match original operation custody, deliberately without a liveness rewrite.
    pub(in super::super::super::super) fn matches_guard(&self, guard: &F05Guard) -> bool {
        self.identity.matches_guard(guard)
    }
    /// Only a matched raw success can mint the non-Clone source-success token.
    /// Even already-cancelled original operations preserve raw error precedence.
    pub(in super::super::super::super) fn into_outcome(
        self,
        guard: &F05Guard,
    ) -> Result<F05Outcome, CompositionError> {
        if !self.matches_guard(guard) {
            return Err(super::super::super::Error::Identity.into());
        }
        let Self {
            identity,
            result,
            diagnostics,
        } = self;
        Ok(match result {
            Ok(()) => F05Outcome::Success(F05VerifiedSuccess {
                identity,
                diagnostics,
            }),
            Err(error) => F05Outcome::SourceError(error, diagnostics),
        })
    }
}
#[cfg(all(test, not(benilla_headless_geometry)))]
pub(crate) enum F05Poll {
    Pending,
    Ready(F05Completion),
    Mismatch,
    Stopped,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
struct F05Task {
    identity: Arc<()>,
    products: Arc<OwnedSourceProducts>,
    lease: JobLease,
    cancelled: Arc<AtomicBool>,
    callback_deadline: Instant,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
struct F05Result {
    identity: Arc<()>,
    completion: F05Completion,
}

enum WorkerTask {
    Observe(Job),
    #[cfg(all(test, not(benilla_headless_geometry)))]
    ContinueF05(F05Task),
}
enum WorkerResult {
    Observation(Result<FreshnessObservation, CompositionError>),
    #[cfg(all(test, not(benilla_headless_geometry)))]
    F05(F05Result),
}

struct Job {
    products: Arc<OwnedSourceProducts>,
    lease: JobLease,
    cancelled: Arc<AtomicBool>,
}

/// Construct once. The occupied slot includes queued, running and completed but
/// undrained work; cancellation never releases it or starts another thread.
pub(crate) struct CaptureFreshnessWorker {
    requests: Sender<WorkerTask>,
    completed: Receiver<WorkerResult>,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    f05: Option<Arc<()>>,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    wake: Sender<()>,
    #[cfg(all(test, not(benilla_headless_geometry)))]
    awakened: Receiver<()>,
    occupied: bool,
    latest: Option<Arc<AtomicBool>>,
    stop: Arc<AtomicBool>,
}
impl CaptureFreshnessWorker {
    pub(crate) fn new() -> io::Result<Self> {
        Self::with_hooks(WorkerHooks::default())
    }

    /// Deterministic native scheduler tests use the existing owned channel gate;
    /// no callback, borrowed source or alternate worker lifecycle is introduced.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn with_test_gate(
        entered: Sender<thread::ThreadId>,
        release: Receiver<()>,
        published: Option<Sender<()>>,
    ) -> io::Result<Self> {
        Self::with_hooks(WorkerHooks {
            before_verify: Some(Gate { entered, release }),
            published,
            ..WorkerHooks::default()
        })
    }

    /// Hold only the actual continued F05 source effect, never an initial
    /// observation. The owning fixture supplies channels, not worker callbacks.
    #[cfg(all(test, not(benilla_headless_geometry)))]
    pub(crate) fn with_f05_test_gate(
        entered: Sender<()>,
        release: Receiver<()>,
    ) -> io::Result<Self> {
        Self::with_hooks(WorkerHooks {
            f05_before_verify: Some(F05Gate { entered, release }),
            ..WorkerHooks::default()
        })
    }

    fn with_hooks(hooks: WorkerHooks) -> io::Result<Self> {
        let (requests, receive) = bounded(1);
        let (complete, completed) = bounded(1);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        #[cfg(all(test, not(benilla_headless_geometry)))]
        let (wake, awakened) = bounded(1);
        #[cfg(all(test, not(benilla_headless_geometry)))]
        let worker_wake = wake.clone();
        // Dropping the handle detaches. Destruction must not join blocked source I/O.
        let _thread = thread::Builder::new()
            .name("capture-freshness".into())
            .spawn(move || {
                #[cfg(not(benilla_headless_geometry))]
                run(receive, complete, worker_stop, hooks, worker_wake);
                #[cfg(benilla_headless_geometry)]
                run(receive, complete, worker_stop, hooks);
            })?;
        Ok(Self {
            requests,
            completed,
            occupied: false,
            latest: None,
            stop,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            f05: None,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            wake,
            #[cfg(all(test, not(benilla_headless_geometry)))]
            awakened,
        })
    }

    /// Only owned captures and an existing read-only lease enter the worker.
    /// No filesystem operation or borrowed cancellation callback runs here.
    pub(in super::super::super::super) fn try_enqueue(
        &mut self,
        products: Arc<OwnedSourceProducts>,
        lease: JobLease,
    ) -> Result<(), EnqueueError> {
        if self.stop.load(Ordering::Acquire) {
            return Err(EnqueueError::Stopped);
        }
        if self.occupied {
            return Err(EnqueueError::Busy);
        }
        if !lease.is_active() {
            return Err(EnqueueError::Inactive);
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        match self.requests.try_send(WorkerTask::Observe(Job {
            products,
            lease,
            cancelled: Arc::clone(&cancelled),
        })) {
            Ok(()) => {
                // Draining releases capacity, not the last observation's token.
                // A new accepted request revokes an unconsumed older observation.
                self.cancel();
                self.latest = Some(cancelled);
                self.occupied = true;
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(EnqueueError::Busy),
            Err(TrySendError::Disconnected(_)) => {
                self.stop.store(true, Ordering::Release);
                Err(EnqueueError::Stopped)
            }
        }
    }

    pub(crate) fn cancel(&self) {
        if let Some(cancelled) = &self.latest {
            cancelled.store(true, Ordering::Release);
        }
        #[cfg(all(test, not(benilla_headless_geometry)))]
        if self.f05.is_some() {
            let _ = self.wake.try_send(());
        }
    }

    pub(crate) fn try_poll(&mut self) -> FreshnessPoll {
        #[cfg(all(test, not(benilla_headless_geometry)))]
        if self.f05.is_some() {
            // Only the matching raw-slot API may drain a continuation result.
            return FreshnessPoll::Pending;
        }
        if self.stop.load(Ordering::Acquire) {
            return FreshnessPoll::Stopped;
        }
        if !self.occupied {
            return FreshnessPoll::Idle;
        }
        match self.completed.try_recv() {
            Ok(WorkerResult::Observation(result)) => {
                self.occupied = false;
                // Preserve actual verification errors even if cancellation happened
                // afterward. Only successful observations need a late liveness guard.
                FreshnessPoll::Ready(result.and_then(|observation| {
                    observation.check_active()?;
                    Ok(observation)
                }))
            }
            #[cfg(all(test, not(benilla_headless_geometry)))]
            Ok(WorkerResult::F05(_)) => {
                unreachable!("continuation slot is never polled as observation")
            }
            Err(TryRecvError::Empty) => FreshnessPoll::Pending,
            Err(TryRecvError::Disconnected) => {
                self.stop.store(true, Ordering::Release);
                FreshnessPoll::Stopped
            }
        }
    }
}
#[cfg(all(test, not(benilla_headless_geometry)))]
impl CaptureFreshnessWorker {
    /// Continue only the original worker's drained observation. Admission does
    /// not revoke that observation or allocate a replacement cancellation token.
    pub(in super::super::super::super) fn try_continue_f05(
        &mut self,
        guard: &F05Guard,
    ) -> Result<F05Slot, F05EnqueueError> {
        if self.stop.load(Ordering::Acquire) {
            return Err(F05EnqueueError::Stopped);
        }
        if self.occupied {
            return Err(F05EnqueueError::Busy);
        }
        if !Arc::ptr_eq(&self.stop, &guard.observation.stop)
            || !self
                .latest
                .as_ref()
                .is_some_and(|latest| Arc::ptr_eq(latest, &guard.observation.cancelled))
        {
            return Err(F05EnqueueError::Identity);
        }
        if guard.submitted.get() {
            return Err(F05EnqueueError::AlreadySubmitted);
        }
        if guard.cancelled() {
            return Err(F05EnqueueError::Inactive);
        }
        let identity = Arc::clone(&guard.operation);
        let task = F05Task {
            identity: Arc::clone(&identity),
            products: Arc::clone(&guard.observation.products),
            lease: guard.observation.lease.clone(),
            cancelled: Arc::clone(&guard.observation.cancelled),
            callback_deadline: guard.callback_deadline,
        };
        match self.requests.try_send(WorkerTask::ContinueF05(task)) {
            Ok(()) => {
                guard.submitted.set(true);
                self.f05 = Some(Arc::clone(&identity));
                self.occupied = true;
                Ok(F05Slot { identity })
            }
            Err(TrySendError::Full(_)) => Err(F05EnqueueError::Busy),
            Err(TrySendError::Disconnected(_)) => {
                self.stop.store(true, Ordering::Release);
                Err(F05EnqueueError::Stopped)
            }
        }
    }

    /// Result arbitration precedes liveness arbitration. A completed raw error
    /// (or success) is never rewritten because its original guard became inactive.
    pub(in super::super::super::super) fn try_poll_f05(&mut self, slot: &F05Slot) -> F05Poll {
        if !self
            .f05
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, &slot.identity))
        {
            return F05Poll::Mismatch;
        }
        match self.completed.try_recv() {
            Ok(WorkerResult::F05(result)) => {
                if !Arc::ptr_eq(&result.identity, &slot.identity) {
                    // Internal protocol violation: abandon, do not reinterpret
                    // this result or release the occupied original tombstone.
                    self.stop.store(true, Ordering::Release);
                    return F05Poll::Mismatch;
                }
                self.occupied = false;
                self.f05 = None;
                F05Poll::Ready(result.completion)
            }
            Ok(WorkerResult::Observation(_)) => {
                self.stop.store(true, Ordering::Release);
                F05Poll::Mismatch
            }
            Err(TryRecvError::Empty) => F05Poll::Pending,
            Err(TryRecvError::Disconnected) => {
                // Keep the occupied tombstone: disconnect is abandonment, not a
                // fabricated source outcome or permission to reuse this slot.
                self.stop.store(true, Ordering::Release);
                F05Poll::Stopped
            }
        }
    }

    /// Coalesced notification only. The owning driver may select this alongside
    /// its independent control channel and original deadline; it must still poll.
    pub(in super::super::super::super) fn completion_wake(&self) -> Receiver<()> {
        self.awakened.clone()
    }
}

impl Drop for CaptureFreshnessWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.cancel();
        // Fields disconnect both channels; no wait, join or replacement worker.
    }
}

fn run(
    requests: Receiver<WorkerTask>,
    completed: Sender<WorkerResult>,
    stop: Arc<AtomicBool>,
    hooks: WorkerHooks,
    #[cfg(all(test, not(benilla_headless_geometry)))] wake: Sender<()>,
) {
    while let Ok(task) = requests.recv() {
        if stop.load(Ordering::Acquire) {
            break;
        }
        if !hooks.before_verify.as_ref().is_none_or(Gate::wait) {
            break;
        }
        #[cfg(benilla_headless_geometry)]
        let WorkerTask::Observe(job) = task;
        #[cfg(not(benilla_headless_geometry))]
        let job = match task {
            WorkerTask::Observe(job) => job,
            WorkerTask::ContinueF05(job) => {
                if !run_f05(job, &completed, &stop, &hooks, &wake) {
                    break;
                }
                continue;
            }
        };
        // Construct the cancellation closure here from owned state only. The view
        // is local to verify_sources and is gone before any subsequent hook or yield.
        let cancelled = || {
            stop.load(Ordering::Acquire)
                || job.cancelled.load(Ordering::Acquire)
                || !job.lease.is_active()
        };
        let result = if let Some(counts) = &hooks.preparation_counts {
            let (result, measured) =
                preparation_counts::measure(|| job.products.verify_sources(cancelled));
            let _ = counts.try_send(measured);
            result
        } else {
            job.products.verify_sources(cancelled)
        };
        if result.is_ok() && !hooks.after_success.as_ref().is_none_or(Gate::wait) {
            break;
        }
        let result = result.map(|()| FreshnessObservation {
            products: job.products,
            lease: job.lease,
            cancelled: job.cancelled,
            stop: Arc::clone(&stop),
        });
        if completed
            .try_send(WorkerResult::Observation(result))
            .is_err()
        {
            break;
        }
        if let Some(published) = &hooks.published {
            let _ = published.try_send(());
        }
    }
    if let Some(exited) = hooks.exited {
        let _ = exited.try_send(());
    }
}

#[cfg(all(test, not(benilla_headless_geometry)))]
fn run_f05(
    job: F05Task,
    completed: &Sender<WorkerResult>,
    stop: &Arc<AtomicBool>,
    hooks: &WorkerHooks,
    wake: &Sender<()>,
) -> bool {
    if !hooks.f05_before_verify.as_ref().is_none_or(F05Gate::wait) {
        return false;
    }
    // Exact original observation cancellation order, followed by the saved
    // callback-effective deadline. Root-local context expiry is not this guard.
    let cancel_checks = std::cell::Cell::new(0usize);
    let first_cancelled_check = std::cell::Cell::new(None);
    let cancelled = || {
        let result = job.cancelled.load(Ordering::Acquire)
            || stop.load(Ordering::Acquire)
            || !job.lease.is_active()
            || Instant::now() >= job.callback_deadline;
        let count = cancel_checks.get() + 1;
        cancel_checks.set(count);
        if result && first_cancelled_check.get().is_none() {
            first_cancelled_check.set(Some(count));
        }
        result
    };
    let ((result, counts), trace) = super::super::freshness_timings::trace_sources(|| {
        super::super::freshness_timings::measure_mode(
            super::super::freshness_timings::Mode::CountOnly,
            || job.products.verify_sources(cancelled),
        )
    });
    if result.is_ok() && !hooks.after_success.as_ref().is_none_or(Gate::wait) {
        return false;
    }
    if completed
        .try_send(WorkerResult::F05(F05Result {
            identity: Arc::clone(&job.identity),
            completion: F05Completion {
                identity: F05ProofIdentity {
                    operation: job.identity,
                    products: job.products,
                    lease: job.lease,
                    cancelled: job.cancelled,
                    stop: Arc::clone(stop),
                    callback_deadline: job.callback_deadline,
                },
                result,
                diagnostics: F05Diagnostics {
                    trace,
                    counts: Box::new(counts),
                    cancel_checks: cancel_checks.get(),
                    first_cancelled_check: first_cancelled_check.get(),
                },
            },
        }))
        .is_err()
    {
        return false;
    }
    // Publication happens-before this wake; a full wake channel already holds
    // a notification. No result or currentness authority is carried by the wake.
    let _ = wake.try_send(());
    if let Some(published) = &hooks.published {
        let _ = published.try_send(());
    }
    true
}

/// Owned channel gates are test instrumentation, never submitted callbacks.
struct Gate {
    entered: Sender<thread::ThreadId>,
    release: Receiver<()>,
}
impl Gate {
    fn wait(&self) -> bool {
        self.entered.send(thread::current().id()).is_ok() && self.release.recv().is_ok()
    }
}
#[cfg(all(test, not(benilla_headless_geometry)))]
struct F05Gate {
    entered: Sender<()>,
    release: Receiver<()>,
}
#[cfg(all(test, not(benilla_headless_geometry)))]
impl F05Gate {
    fn wait(&self) -> bool {
        self.entered.send(()).is_ok() && self.release.recv().is_ok()
    }
}

#[derive(Default)]
struct WorkerHooks {
    #[cfg(all(test, not(benilla_headless_geometry)))]
    f05_before_verify: Option<F05Gate>,
    before_verify: Option<Gate>,
    after_success: Option<Gate>,
    preparation_counts: Option<Sender<preparation_counts::Counts>>,
    published: Option<Sender<()>>,
    exited: Option<Sender<()>>,
}

#[cfg(test)]
mod tests;
