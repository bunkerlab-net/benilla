// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded test-only source observation. No native query or certification crosses threads.
use super::super::super::super::core::JobLease;
use super::super::{preparation_counts, CompositionError};
use super::OwnedSourceProducts;
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, TrySendError};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
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
    pub(crate) fn consume(
        self,
        products: &Arc<OwnedSourceProducts>,
        lease: &JobLease,
    ) -> Result<(), CompositionError> {
        if !Arc::ptr_eq(&self.products, products) || !self.lease.same_job(lease) {
            return Err(super::super::super::Error::Identity.into());
        }
        self.check_active()
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

struct Job {
    products: Arc<OwnedSourceProducts>,
    lease: JobLease,
    cancelled: Arc<AtomicBool>,
}

/// Construct once. The occupied slot includes queued, running and completed but
/// undrained work; cancellation never releases it or starts another thread.
pub(crate) struct CaptureFreshnessWorker {
    requests: Sender<Job>,
    completed: Receiver<Result<FreshnessObservation, CompositionError>>,
    occupied: bool,
    latest: Option<Arc<AtomicBool>>,
    stop: Arc<AtomicBool>,
}
impl CaptureFreshnessWorker {
    pub(crate) fn new() -> io::Result<Self> {
        Self::with_hooks(WorkerHooks::default())
    }

    fn with_hooks(hooks: WorkerHooks) -> io::Result<Self> {
        let (requests, receive) = bounded(1);
        let (complete, completed) = bounded(1);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        // Dropping the handle detaches. Destruction must not join blocked source I/O.
        let _thread = thread::Builder::new()
            .name("capture-freshness".into())
            .spawn(move || run(receive, complete, worker_stop, hooks))?;
        Ok(Self {
            requests,
            completed,
            occupied: false,
            latest: None,
            stop,
        })
    }

    /// Only owned captures and an existing read-only lease enter the worker.
    /// No filesystem operation or borrowed cancellation callback runs here.
    pub(crate) fn try_enqueue(
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
        match self.requests.try_send(Job {
            products,
            lease,
            cancelled: Arc::clone(&cancelled),
        }) {
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
    }

    pub(crate) fn try_poll(&mut self) -> FreshnessPoll {
        if self.stop.load(Ordering::Acquire) {
            return FreshnessPoll::Stopped;
        }
        if !self.occupied {
            return FreshnessPoll::Idle;
        }
        match self.completed.try_recv() {
            Ok(result) => {
                self.occupied = false;
                // Preserve actual verification errors even if cancellation happened
                // afterward. Only successful observations need a late liveness guard.
                FreshnessPoll::Ready(result.and_then(|observation| {
                    observation.check_active()?;
                    Ok(observation)
                }))
            }
            Err(TryRecvError::Empty) => FreshnessPoll::Pending,
            Err(TryRecvError::Disconnected) => {
                self.stop.store(true, Ordering::Release);
                FreshnessPoll::Stopped
            }
        }
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
    requests: Receiver<Job>,
    completed: Sender<Result<FreshnessObservation, CompositionError>>,
    stop: Arc<AtomicBool>,
    hooks: WorkerHooks,
) {
    while let Ok(job) = requests.recv() {
        if stop.load(Ordering::Acquire) {
            break;
        }
        if !hooks.before_verify.as_ref().is_none_or(Gate::wait) {
            break;
        }
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
        if completed.try_send(result).is_err() {
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
#[derive(Default)]
struct WorkerHooks {
    before_verify: Option<Gate>,
    after_success: Option<Gate>,
    preparation_counts: Option<Sender<preparation_counts::Counts>>,
    published: Option<Sender<()>>,
    exited: Option<Sender<()>>,
}

#[cfg(test)]
mod tests;
