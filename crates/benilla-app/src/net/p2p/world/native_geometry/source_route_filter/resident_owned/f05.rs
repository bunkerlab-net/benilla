// SPDX-License-Identifier: GPL-3.0-or-later
//! One original F05 source slot. Private custody is not source or scene authority.
use super::super::super::super::{
    core::{resident_dispatch::fifo::ResidentFifoSelection, JobLease},
    products::source_height::{
        owned::{
            f05_scope,
            freshness::{F05Completion, F05Guard, F05Outcome, F05VerifiedSuccess},
        },
        CompositionError,
    },
    source_route_resident_local::ledger::ParkedRetainedF05,
};
use super::transport::{CertifiedResidentResult, ResidentAttempt};
use super::*;

/// Sealed owner-custody input; FIFO cannot create a stand-alone continuation.
pub(in super::super::super::super) struct DriverCustody {
    identity: Rc<()>,
    cancel_after_staging: bool,
}

// Fields and constructors are private to this actual owning driver. Neither
// capability can be made from serialized provenance or a caller-supplied bool.
pub(in super::super::super::super) struct F05ResumePermit {
    products: Arc<OwnedSourceProducts>,
    custody: Rc<()>,
    invocation: Rc<()>,
    guard: Rc<F05Guard>,
}
pub(in super::super::super::super) struct InactiveF05Proof {
    products: Arc<OwnedSourceProducts>,
    custody: Rc<()>,
    invocation: Rc<()>,
    guard: Rc<F05Guard>,
}
impl F05ResumePermit {
    pub(in super::super::super::super) fn matches(
        &self,
        products: &Arc<OwnedSourceProducts>,
        custody: &Rc<()>,
        invocation: &Rc<()>,
        guard: &Rc<F05Guard>,
    ) -> bool {
        Arc::ptr_eq(&self.products, products)
            && Rc::ptr_eq(&self.custody, custody)
            && Rc::ptr_eq(&self.invocation, invocation)
            && Rc::ptr_eq(&self.guard, guard)
    }
}
impl InactiveF05Proof {
    pub(in super::super::super::super) fn matches(
        &self,
        products: &Arc<OwnedSourceProducts>,
        custody: &Rc<()>,
        invocation: &Rc<()>,
        guard: &Rc<F05Guard>,
    ) -> bool {
        Arc::ptr_eq(&self.products, products)
            && Rc::ptr_eq(&self.custody, custody)
            && Rc::ptr_eq(&self.invocation, invocation)
            && Rc::ptr_eq(&self.guard, guard)
            && guard.cancelled()
    }
}

struct InvocationAnchor {
    identity: ResidentProviderIdentity,
    lease: JobLease,
    operands: InvocationOperands,
}

/// No borrowed view, Work, native terminal clone, Facts, or candidate crosses park.
pub(in super::super::super::super) struct ParkedF05 {
    attempt: ResidentAttempt,
    guard: Rc<F05Guard>,
    height: f05_scope::F05HeightFrame,
    inventory: f05_scope::F05InventoryFrame,
    custody: Rc<()>,
    invocation: Rc<()>,
    anchor: InvocationAnchor,
    retained: ParkedRetainedF05,
    installed: NativeInventorySource,
}
impl ParkedF05 {
    pub(in super::super::super::super) fn guard(&self) -> &F05Guard {
        &self.guard
    }

    /// Raw source error does not reconstruct a job/provider or run root postcheck.
    fn source_error(
        self,
        owner: &mut OwnedResident,
        error: CompositionError,
    ) -> Result<CertifiedResidentResult, ResidentFilterError> {
        drop(self.retained);
        drop(self.anchor);
        drop(self.inventory);
        let result = self.height.finish(Err(ResidentFilterError::Source(error)));
        owner.finish_attempt(self.attempt, result)
    }

    /// The original non-resettable guard must already be inactive. No generic
    /// callback or active-context capability is exposed by this error-only path.
    fn cancelled_success(
        self,
        success: F05VerifiedSuccess,
        owner: &mut OwnedResident,
        custody: &DriverCustody,
    ) -> Result<CertifiedResidentResult, ResidentFilterError> {
        let inner = (|| {
            if !self.guard.cancelled() || !Rc::ptr_eq(&self.custody, &custody.identity) {
                return Err(Failure::Binding.into());
            }
            let proof = InactiveF05Proof {
                products: Arc::clone(&owner.products),
                custody: Rc::clone(&custody.identity),
                invocation: Rc::clone(&self.invocation),
                guard: Rc::clone(&self.guard),
            };
            f05_scope::with_inactive_f05_views(
                &self.height,
                &self.inventory,
                &owner.inventory,
                &proof,
                |height, _| {
                    let provider = ResidentFilterProvider {
                        state: ResidentStateStorage::Borrowed(&mut owner.state),
                        height,
                        invocation: None,
                    };
                    Err(self.retained.unwind_cancelled_f05(
                        success,
                        &proof,
                        &provider,
                        self.guard.request(),
                        || self.guard.cancelled(),
                    ))
                },
            )
        })();
        drop(self.anchor);
        drop(self.inventory);
        let result = self.height.finish(inner);
        owner.finish_attempt(self.attempt, result)
    }
}

/// Owns resident and selector continuously. There is no mutable-owner getter,
/// view-returning callback, or replacement API while a source slot is occupied.
pub(in super::super::super::super) struct ResidentF05Driver {
    owner: OwnedResident,
    selection: ResidentFifoSelection,
    custody: DriverCustody,
}

impl OwnedResident {
    pub(in super::super::super::super) fn begin_f05(
        &mut self,
        prepared: PreparedResident,
        job: ResidentDispatchJob<'_>,
        world: &mut World,
        installed: &NativeInventorySource,
        custody: &DriverCustody,
    ) -> Result<ParkedF05, ResidentFilterError> {
        // Original Prepared entry checks precede the original transport attempt.
        job.require_resident(self)?;
        self.transport.request(job.request())?;
        let attempt = self.begin_attempt(&job)?;
        let began = (|| {
            let (observation, prepared) = prepared.into_f05_retained()?;
            let deadline = job.deadline().min(prepared.deadline());
            let guard =
                Rc::new(observation.into_f05_guard(&self.products, &job.lease(), deadline)?);
            job.require_resident(self)?;
            if !std::ptr::eq(guard.request(), job.request()) || job.profile() != &self.state.profile
            {
                return Err(Failure::Binding.into());
            }
            let invocation = Rc::new(());
            let identity = self.identity.clone();
            let (height, inventory, (retained, operands)) = f05_scope::begin(
                Arc::clone(&self.products),
                Rc::clone(&self.inventory),
                Rc::clone(&custody.identity),
                Rc::clone(&invocation),
                Rc::clone(&guard),
                |height, compatibility| {
                    let operands = InvocationOperands::default();
                    let mut provider = ResidentFilterProvider {
                        state: ResidentStateStorage::Borrowed(&mut self.state),
                        height,
                        invocation: None,
                    };
                    provider.verify_installed_owner(compatibility, installed, deadline, || {
                        guard.cancelled()
                    })?;
                    provider.invocation =
                        Some(InvocationContext::bind(&job, &provider, &operands)?);
                    let retained = self.ledger.begin_retained_f05(
                        prepared,
                        world,
                        compatibility,
                        &mut provider,
                        guard.request(),
                        deadline,
                        || guard.cancelled(),
                    )?;
                    drop(provider);
                    Ok::<_, ResidentFilterError>((retained, operands))
                },
            )?;
            Ok((
                guard, invocation, identity, height, inventory, retained, operands,
            ))
        })();
        match began {
            Ok((guard, invocation, identity, height, inventory, retained, operands)) => {
                Ok(ParkedF05 {
                    attempt,
                    anchor: InvocationAnchor {
                        identity,
                        lease: job.lease(),
                        operands,
                    },
                    guard,
                    height,
                    inventory,
                    custody: Rc::clone(&custody.identity),
                    invocation,
                    retained,
                    installed: installed.clone(),
                })
            }
            Err(error) => match self.finish_attempt(attempt, Err(error)) {
                Err(error) => Err(error),
                Ok(_) => unreachable!("an error cannot mint a transport result"),
            },
        }
    }
}

impl ParkedF05 {
    fn resume_success(
        self,
        success: F05VerifiedSuccess,
        owner: &mut OwnedResident,
        job: ResidentDispatchJob<'_>,
        custody: &DriverCustody,
        geometry: &mut super::super::super::super::real_corridor_fixture::installed_source::InstalledFixture,
    ) -> (Result<CertifiedResidentResult, ResidentFilterError>, bool) {
        let reached_scene = std::cell::Cell::new(false);
        // Every normal reentry error remains inside the saved F59 debt.
        let inner = (|| {
            job.require_resident(owner)?;
            if !Rc::ptr_eq(&custody.identity, &self.custody)
                || !self.anchor.lease.same_job(&job.lease())
                || !self.guard.matches(&owner.products, &job.lease())
                || !std::ptr::eq(self.guard.request(), job.request())
                || !self.anchor.identity.same_allocation(&owner.identity)
            {
                return Err(Failure::Binding.into());
            }
            let permit = F05ResumePermit {
                products: Arc::clone(&owner.products),
                custody: Rc::clone(&custody.identity),
                invocation: Rc::clone(&self.invocation),
                guard: Rc::clone(&self.guard),
            };
            f05_scope::with_f05_views(
                &self.height,
                &self.inventory,
                &owner.inventory,
                &permit,
                |height, compatibility| {
                    let mut provider = ResidentFilterProvider {
                        state: ResidentStateStorage::Borrowed(&mut owner.state),
                        height,
                        invocation: None,
                    };
                    if !self.anchor.identity.matches(&provider) {
                        return Err(Failure::Binding.into());
                    }
                    // Explicit extra reentry eligibility, not a repeated bind.
                    job.require_provider(&provider, &provider.session, &provider.profile)?;
                    provider.invocation = Some(InvocationContext {
                        job: &job,
                        lease: self.anchor.lease.clone(),
                        operands: &self.anchor.operands,
                    });
                    let cancelled = || self.guard.cancelled();
                    let deadline = self.guard.callback_deadline();
                    let ready = ResidentLedger::resume_retained_f05(
                        self.retained,
                        success,
                        &permit,
                        &mut provider,
                        job.request(),
                        cancelled,
                    )?;
                    // No scene proof exists yet. This authored NON-frame schedule
                    // owns no resident/state/ledger resource or driver callback.
                    reached_scene.set(true);
                    geometry.advance_physics();
                    let facts = owner.ledger.finish_retained_f05(
                        ready,
                        geometry.app.world_mut(),
                        compatibility,
                        &mut provider,
                        job.request(),
                        cancelled,
                    )?;
                    if custody.cancel_after_staging {
                        self.guard.cancel();
                    }
                    // Original F58 uses the original outer installed operand.
                    provider.verify_installed_owner(
                        compatibility,
                        &self.installed,
                        deadline,
                        cancelled,
                    )?;
                    Ok(facts)
                },
            )
        })();
        drop(self.anchor);
        drop(self.inventory);
        let calculated = self.height.finish(inner).and_then(|facts| {
            self.guard.check_success()?;
            Ok(facts)
        });
        (
            owner.finish_attempt(self.attempt, calculated),
            reached_scene.get(),
        )
    }
}

pub(in super::super::super::super) fn is_f05_retained(prepared: &PreparedResident) -> bool {
    prepared.is_f05_retained()
}

impl ParkedF05 {
    pub(in super::super::super::super) fn reentry_error(
        self,
        owner: &mut OwnedResident,
        error: ResidentFilterError,
    ) -> Result<CertifiedResidentResult, ResidentFilterError> {
        drop(self.retained);
        drop(self.anchor);
        drop(self.inventory);
        let result = self.height.finish(Err(error));
        owner.finish_attempt(self.attempt, result)
    }
}

impl ResidentF05Driver {
    pub(in super::super::super::super) fn new(
        owner: OwnedResident,
        mailbox: &super::super::super::super::core::Mailbox,
    ) -> Result<Self, Failure> {
        Ok(Self {
            owner,
            selection: ResidentFifoSelection::new(mailbox)?,
            custody: DriverCustody {
                identity: Rc::new(()),
                cancel_after_staging: false,
            },
        })
    }
    pub(in super::super::super::super) fn initialize(
        &mut self,
        boundary: &RouteCommitBoundary,
    ) -> Result<(), Failure> {
        if !self.selection.f05_idle() {
            return Err(Failure::QueueCapacity);
        }
        self.owner.initialize(boundary)
    }
    pub(in super::super::super::super) fn acknowledge(
        &mut self,
        notice: &RouteCommitNotice,
    ) -> Result<(), Failure> {
        if !self.selection.f05_idle() {
            return Err(Failure::QueueCapacity);
        }
        self.owner.acknowledge(notice)
    }
    pub(in super::super::super::super) fn diagnostic(
        &self,
    ) -> super::super::super::super::source_route_resident_local::ledger::Diagnostic {
        self.owner.diagnostic()
    }
    pub(in super::super::super::super) fn pending(&self) -> bool {
        self.selection.f05_pending()
    }
    pub(in super::super::super::super) fn cancel(&mut self) {
        self.selection.cancel();
    }
    pub(in super::super::super::super) fn completion_wake(
        &self,
    ) -> crossbeam_channel::Receiver<()> {
        self.selection.f05_wake()
    }
    pub(in super::super::super::super) fn serve(
        &mut self,
        mailbox: &super::super::super::super::core::Mailbox,
        geometry: &mut super::super::super::super::real_corridor_fixture::installed_source::InstalledFixture,
        limit: usize,
        deadline: Instant,
        ordinary: impl for<'call> FnMut(
            super::super::super::super::core::ServiceQuery<'call>,
            Instant,
        ) -> Result<
            super::super::super::super::core::ServiceStep,
            Failure,
        >,
    ) -> usize {
        self.selection.serve_f05(
            mailbox,
            &mut self.owner,
            &self.custody,
            geometry,
            limit,
            deadline,
            ordinary,
        )
    }
    pub(in super::super::super::super) fn constructor_identity(
        &mut self,
        installed: &NativeInventorySource,
        context: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<(usize, benilla_native_nav::SourceMapQuerySlot, usize), ResidentFilterError> {
        if !self.selection.f05_idle() {
            return Err(Failure::QueueCapacity.into());
        }
        self.owner
            .constructor_identity(installed, context, deadline)
    }
}

#[cfg(test)]
#[path = "tests/f05_equivalence.rs"]
mod tests;

impl ResidentF05Driver {
    pub(in super::super::super::super) fn f05_counts(&self) -> (usize, usize) {
        self.selection.f05_counts()
    }
    pub(in super::super::super::super) fn f05_trace(
        &self,
    ) -> Option<&super::super::super::super::products::source_height::freshness_timings::SourceTrace>
    {
        self.selection.f05_trace()
    }
}

impl ParkedF05 {
    pub(in super::super::super::super) fn complete_inactive(
        self,
        owner: &mut OwnedResident,
        completion: F05Completion,
        custody: &DriverCustody,
    ) -> Result<CertifiedResidentResult, ResidentFilterError> {
        match completion.into_outcome(&self.guard) {
            Err(error) => self.reentry_error(owner, error.into()),
            Ok(F05Outcome::SourceError(error, _diagnostics)) => self.source_error(owner, error),
            Ok(F05Outcome::Success(success)) => self.cancelled_success(success, owner, custody),
        }
    }

    pub(in super::super::super::super) fn complete_active<'work>(
        self,
        owner: &mut OwnedResident,
        completion: F05Completion,
        custody: &DriverCustody,
        make_job: impl FnOnce(&OwnedResident) -> Result<ResidentDispatchJob<'work>, Failure>,
        geometry: &mut super::super::super::super::real_corridor_fixture::installed_source::InstalledFixture,
    ) -> (Result<CertifiedResidentResult, ResidentFilterError>, bool) {
        match completion.into_outcome(&self.guard) {
            Err(error) => (self.reentry_error(owner, error.into()), false),
            Ok(F05Outcome::SourceError(error, _diagnostics)) => {
                (self.source_error(owner, error), false)
            }
            Ok(F05Outcome::Success(success)) => {
                if self.guard.cancelled() {
                    return (self.cancelled_success(success, owner, custody), false);
                }
                match make_job(owner) {
                    Ok(job) => self.resume_success(success, owner, job, custody, geometry),
                    Err(error) => (self.reentry_error(owner, error.into()), false),
                }
            }
        }
    }
}

impl ResidentF05Driver {
    pub(in super::super::super::super) fn new_with_f05_gate(
        owner: OwnedResident,
        mailbox: &super::super::super::super::core::Mailbox,
    ) -> Result<
        (
            Self,
            crossbeam_channel::Receiver<()>,
            crossbeam_channel::Sender<()>,
        ),
        Failure,
    > {
        let (entered_send, entered) = crossbeam_channel::bounded(1);
        let (release, release_receive) = crossbeam_channel::bounded(1);
        let selection =
            ResidentFifoSelection::with_f05_test_gate(mailbox, entered_send, release_receive)?;
        Ok((
            Self {
                owner,
                selection,
                custody: DriverCustody {
                    identity: Rc::new(()),
                    cancel_after_staging: false,
                },
            },
            entered,
            release,
        ))
    }
    pub(in super::super::super::super) fn pending_request(&self) -> Option<&AuthorityRequest> {
        self.selection.pending_f05_request()
    }
}

impl ResidentF05Driver {
    pub(in super::super::super::super) fn f05_outcome(&self) -> (bool, Option<&str>) {
        self.selection.f05_outcome()
    }
}

impl ResidentF05Driver {
    pub(in super::super::super::super) fn pending_deadline(&self) -> Option<Instant> {
        self.selection.pending_f05_deadline()
    }
}

impl ResidentF05Driver {
    pub(in super::super::super::super) fn f05_raw_outcome(&self) -> Option<&str> {
        self.selection.f05_raw_outcome()
    }
}

impl ResidentF05Driver {
    pub(in super::super::super::super) fn new_with_f05_gate_cancel_after_staging(
        owner: OwnedResident,
        mailbox: &super::super::super::super::core::Mailbox,
    ) -> Result<
        (
            Self,
            crossbeam_channel::Receiver<()>,
            crossbeam_channel::Sender<()>,
        ),
        Failure,
    > {
        let (mut driver, entered, release) = Self::new_with_f05_gate(owner, mailbox)?;
        driver.custody.cancel_after_staging = true;
        Ok((driver, entered, release))
    }
}

impl ResidentF05Driver {
    pub(in super::super::super::super) fn ledger_counts(&self) -> ((usize, usize), u64) {
        (
            self.owner.ledger.counts(),
            self.owner.ledger.next_transport_candidate(),
        )
    }
}

impl ResidentF05Driver {
    pub(in super::super::super::super) fn f05_reentry_jobs(&self) -> usize {
        self.selection.f05_reentry_jobs()
    }
}
