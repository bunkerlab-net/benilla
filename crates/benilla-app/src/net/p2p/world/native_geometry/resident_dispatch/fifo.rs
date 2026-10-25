// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only selection in the actual FIFO. Inspection never grants publication.
use super::super::super::products::source_height::{
    owned::freshness::{CaptureFreshnessWorker, FreshnessObservation, FreshnessPoll},
    CompositionError,
};
use super::*;

pub(crate) enum ResidentStep {
    Pending,
    Observed,
    Complete(Box<CertifiedResidentResult>),
}
struct Selected {
    started: Instant,
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
    /// Read-only timing witness for the frame-cadenced component fixture. This
    /// observes the exact already-selected retained Work, never renews its lease.
    pub(crate) fn selected_retained_deadline_for_test(&self) -> Option<Instant> {
        let selected = self.selected.as_ref()?;
        match &selected.validation.request.query {
            Query::SourceMotion(MotionQuery::Route(route)) if route.request.prior_cache.is_some() => {
                Some(selected.validation.lease.deadline())
            }
            _ => None,
        }
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
        ) -> Result<Option<CertifiedResidentResult>, Failure>,
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
        ) -> Result<Option<CertifiedResidentResult>, Failure>,
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
            let started = Instant::now();
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
                started,
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
                eprintln!(
                    "resident Route freshness ready: tick={} ticket={} elapsed_ms={} original_expired={}",
                    work.request.binding.tick,
                    work.ticket,
                    selected.started.elapsed().as_millis(),
                    Instant::now() >= work.deadline,
                );
                // This borrow and observation cannot survive the callback. A
                // consumed result is never returned to the queue as clearance.
                let result = job
                    .require_resident(resident)
                    .and_then(|()| resume(resident, job, result));
                self.selected = None;
                match result? {
                    None => Ok(ResidentStep::Observed),
                    Some(result) => {
                        // Preserve a genuine seal if Work expired after minting.
                        // The shared final live-Work guard must retire it as unsent.
                        result.validate_staged_result(resident, &work.lease())?;
                        Ok(ResidentStep::Complete(Box::new(result)))
                    }
                }
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
            Some(&mut |work: &Work, slice| {
                self.advance(mailbox, work, resident, slice, &mut |owner, job, result| {
                    resume(owner, job, result);
                    Ok(None)
                })
            }),
        )
    }
    /// The explicit paired transport alone accepts a result minted by OwnedResident.
    pub(in super::super::super) fn serve_results(
        &mut self,
        mailbox: &Mailbox,
        resident: &mut OwnedResident,
        limit: usize,
        deadline: Instant,
        resume: impl for<'call> FnMut(
            &mut OwnedResident,
            ResidentDispatchJob<'call>,
            Result<FreshnessObservation, CompositionError>,
        ) -> Result<CertifiedResidentResult, Failure>,
        ordinary: impl for<'call> FnMut(ServiceQuery<'call>, Instant) -> Result<ServiceStep, Failure>,
    ) -> usize {
        struct Attempt<'a, F> {
            selection: &'a mut ResidentFifoSelection,
            mailbox: &'a Mailbox,
            owner: &'a mut OwnedResident,
            resume: F,
        }
        impl<F> ResidentAttemptHook for Attempt<'_, F>
        where
            F: for<'call> FnMut(
                &mut OwnedResident,
                ResidentDispatchJob<'call>,
                Result<FreshnessObservation, CompositionError>,
            ) -> Result<CertifiedResidentResult, Failure>,
        {
            fn advance(
                &mut self,
                work: &Work,
                slice: Instant,
            ) -> Option<Result<ResidentStep, Failure>> {
                if matches!(
                    work.request.query,
                    Query::SourceMotion(MotionQuery::Route(_))
                ) && self.mailbox.resident.as_ref().is_none_or(|selected| {
                    !Arc::ptr_eq(selected, &self.owner.transport_selection())
                }) {
                    return Some(Err(Failure::Binding));
                }
                self.selection.advance(
                    self.mailbox,
                    work,
                    self.owner,
                    slice,
                    &mut |owner, job, result| (self.resume)(owner, job, result).map(Some),
                )
            }
            fn retire(&mut self, proof: &UndeliveredResidentResult) -> Result<(), Failure> {
                self.owner.retire_undelivered_result(proof)
            }
        }
        self.maintain();
        mailbox.serve_service_query_slice_selected(
            limit,
            true,
            deadline,
            ordinary,
            Some(&mut Attempt {
                selection: self,
                mailbox,
                owner: resident,
                resume,
            }),
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
