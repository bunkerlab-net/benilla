// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only selection in the actual FIFO. This module cannot publish resident Facts.
use super::super::super::products::source_height::{
    owned::freshness::{CaptureFreshnessWorker, FreshnessObservation, FreshnessPoll},
    CompositionError,
};
use super::*;

pub(crate) enum ResidentStep {
    Pending,
    Observed,
}
struct Selected {
    validation: ResidentDispatchValidation,
    cancelled: bool,
    occupied: bool,
}
/// Caller-thread sidecar owns no Work, reply sender, world view, or second queue.
/// A cancelled slot remains occupied until the persistent worker is drained.
pub(crate) struct ResidentFifoSelection {
    worker: CaptureFreshnessWorker,
    selected: Option<Selected>,
    attachment: Arc<AtomicBool>,
    claim: Arc<()>,
    stopped: bool,
}
impl ResidentFifoSelection {
    pub(crate) fn new(mailbox: &Mailbox) -> Result<Self, Failure> {
        Self::with_worker(mailbox, CaptureFreshnessWorker::new)
    }
    fn with_worker(
        mailbox: &Mailbox,
        create: impl FnOnce() -> std::io::Result<CaptureFreshnessWorker>,
    ) -> Result<Self, Failure> {
        if !mailbox.active.load(Ordering::Acquire) {
            return Err(Failure::Deadline);
        }
        let claim = Arc::new(());
        // Refuse competition before invoking the worker factory. This token is
        // scheduling exclusivity only, never a substitute for provider identity.
        mailbox
            .resident_selector
            .set(Arc::downgrade(&claim))
            .map_err(|_| Failure::Binding)?;
        let worker = match create() {
            Ok(worker) => worker,
            Err(_) => {
                mailbox.active.store(false, Ordering::Release);
                return Err(Failure::Binding);
            }
        };
        Ok(Self {
            worker,
            selected: None,
            attachment: mailbox.active.clone(),
            claim,
            stopped: false,
        })
    }
    pub(crate) fn cancel(&mut self) {
        if let Some(selected) = &mut self.selected {
            selected.cancelled = true;
        }
        self.worker.cancel();
    }
    fn maintain(&mut self) {
        let Some(selected) = &mut self.selected else {
            return;
        };
        if !selected.cancelled && selected.validation.lease.is_active() {
            return;
        }
        selected.cancelled = true;
        self.worker.cancel();
        if selected.occupied {
            match self.worker.try_poll() {
                FreshnessPoll::Pending => return,
                FreshnessPoll::Ready(_) => selected.occupied = false,
                FreshnessPoll::Stopped | FreshnessPoll::Idle => {
                    selected.occupied = false;
                    self.stopped = true;
                }
            }
        }
        // An explicit cancel must not allow the still-queued original Work to be
        // selected anew after draining. Keep its tombstone until Work is dropped.
        if !selected.validation.lease.is_active() {
            self.selected = None;
        }
    }
    fn advance(
        &mut self,
        mailbox: &Mailbox,
        work: &Work,
        resident: &mut OwnedResident,
        deadline: Instant,
        resume: &mut impl for<'call> FnMut(
            &mut OwnedResident,
            ResidentDispatchJob<'call>,
            Result<FreshnessObservation, CompositionError>,
        ),
    ) -> Option<Result<ResidentStep, Failure>> {
        if !matches!(
            &work.request.query,
            Query::SourceMotion(MotionQuery::Route(_))
        ) {
            return None;
        }
        Some(self.advance_route(mailbox, work, resident, deadline, resume))
    }
    fn advance_route(
        &mut self,
        mailbox: &Mailbox,
        work: &Work,
        resident: &mut OwnedResident,
        deadline: Instant,
        resume: &mut impl for<'call> FnMut(
            &mut OwnedResident,
            ResidentDispatchJob<'call>,
            Result<FreshnessObservation, CompositionError>,
        ),
    ) -> Result<ResidentStep, Failure> {
        self.maintain();
        if !Arc::ptr_eq(&self.attachment, &mailbox.active)
            || !mailbox
                .resident_selector
                .get()
                .and_then(std::sync::Weak::upgrade)
                .is_some_and(|claim| Arc::ptr_eq(&claim, &self.claim))
        {
            return Err(Failure::Binding);
        }
        if self.stopped {
            return Err(Failure::Binding);
        }
        if self.selected.is_none() {
            let (_, _, profile, player) = resident.dispatch_selection();
            let validation = ResidentDispatchValidation::bind(
                mailbox, resident, profile, player, work, deadline,
            )?;
            let job = validation.job(work, mailbox, resident, deadline)?;
            resident.enqueue_observation(job, &mut self.worker).map_err(|error| match error {
                super::super::super::source_route_filter::resident::ResidentFilterError::Context(error) => error,
                _ => Failure::Binding,
            })?;
            self.selected = Some(Selected {
                validation,
                cancelled: false,
                occupied: true,
            });
            // Enqueue never polls in the same attempt, even if already complete.
            return Ok(ResidentStep::Pending);
        }
        let selected = self.selected.as_ref().unwrap();
        if !selected.validation.lease.same_job(&work.lease()) {
            return Err(Failure::QueueCapacity);
        }
        if selected.cancelled {
            return Err(Failure::Binding);
        }
        let job = selected.validation.job(work, mailbox, resident, deadline)?;
        match self.worker.try_poll() {
            FreshnessPoll::Pending => Ok(ResidentStep::Pending),
            FreshnessPoll::Ready(result) => {
                // This borrow and observation cannot survive the callback. A
                // consumed result is never returned to the queue as clearance.
                if job.require_resident(resident).is_ok() {
                    resume(resident, job, result);
                }
                self.selected = None;
                Ok(ResidentStep::Observed)
            }
            FreshnessPoll::Stopped | FreshnessPoll::Idle => {
                self.stopped = true;
                self.selected = None;
                Err(Failure::Binding)
            }
        }
    }
    /// Component callback reacquires current world/physics itself. Its unit return
    /// intentionally cannot enter a successful generic reply or acknowledge state.
    pub(in super::super::super) fn serve(
        &mut self,
        mailbox: &Mailbox,
        resident: &mut OwnedResident,
        limit: usize,
        deadline: Instant,
        mut resume: impl for<'call> FnMut(
            &mut OwnedResident,
            ResidentDispatchJob<'call>,
            Result<FreshnessObservation, CompositionError>,
        ),
        ordinary: impl for<'call> FnMut(ServiceQuery<'call>, Instant) -> Result<ServiceStep, Failure>,
    ) -> usize {
        self.maintain();
        mailbox.serve_service_query_slice_selected(
            limit,
            true,
            deadline,
            ordinary,
            Some(&mut |work, slice| self.advance(mailbox, work, resident, slice, &mut resume)),
        )
    }
}
impl Drop for ResidentFifoSelection {
    fn drop(&mut self) {
        // This explicit component selector owns one attachment's resident
        // lifecycle. Replacement must use a new mailbox, never resurrect its
        // queued Work with a fresh worker on the old live attachment.
        self.attachment.store(false, Ordering::Release);
        self.worker.cancel();
        // Worker Drop disconnects/detaches without joining source I/O.
    }
}
impl Mailbox {
    /// Explicit fixture admission authenticates the genuine resident input, then
    /// creates the one original FIFO Work. Ordinary enqueue is unchanged/closed.
    pub(in super::super::super) fn enqueue_resident_test_request(
        &self,
        resident: &OwnedResident,
        ticket: u64,
        request: AuthorityRequest,
        deadline: Instant,
    ) -> Result<(JobLease, ServiceTestReply), Failure> {
        let (_, _, profile, player) = resident.dispatch_selection();
        ResidentDispatchValidation::validate_request(
            self, resident, profile, player, &request, deadline,
        )?;
        let (reply, receive) = bounded(1);
        let work = Work::new(ticket, request, deadline, reply, self.active.clone());
        let lease = work.lease();
        self.retry
            .try_send(work)
            .map_err(|_| Failure::QueueCapacity)?;
        Ok((lease, ServiceTestReply(receive)))
    }
}

#[path = "fifo/tests.rs"]
#[cfg(test)]
mod tests;
