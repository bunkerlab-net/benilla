// SPDX-License-Identifier: GPL-3.0-or-later
//! Private two-turn first-retained certification; never a scene-proof cache.
use super::super::super::super::{
    products::source_height::CompositionError,
    source_route_resident_local::{ledger::Diagnostic, PreparedRetained},
};
use super::transport::CertifiedResidentResult;
use super::*;

pub(crate) struct PreparedResident {
    observation: FreshnessObservation,
    retained: PreparedRetained,
}
pub(crate) enum ResidentPhase {
    Observation(Result<FreshnessObservation, CompositionError>),
    Prepared(Box<PreparedResident>),
}
pub(crate) enum ResidentPhaseResult {
    Prepared(Box<PreparedResident>),
    Complete(Box<CertifiedResidentResult>),
}
impl OwnedResident {
    pub(in super::super::super::super) fn diagnostic(&self) -> Diagnostic {
        self.ledger.diagnostic()
    }
    pub(crate) fn certify_phase_result(
        &mut self,
        phase: ResidentPhase,
        job: ResidentDispatchJob<'_>,
        world: &mut World,
        installed: &NativeInventorySource,
    ) -> Result<ResidentPhaseResult, ResidentFilterError> {
        self.certify_phase_result_after(phase, job, world, installed, || Ok(()))
    }
    pub(in super::super::super::super) fn certify_phase_result_after(
        &mut self,
        phase: ResidentPhase,
        job: ResidentDispatchJob<'_>,
        world: &mut World,
        installed: &NativeInventorySource,
        after: impl FnOnce() -> Result<(), Failure>,
    ) -> Result<ResidentPhaseResult, ResidentFilterError> {
        job.require_resident(self)?;
        self.transport.request(job.request())?;
        match phase {
            ResidentPhase::Observation(observation) => {
                let observation = observation.map_err(ResidentFilterError::Source)?;
                let Query::SourceMotion(MotionQuery::Route(route)) = &job.request().query else {
                    return Err(Failure::Binding.into());
                };
                if route.request.prior_cache.is_none() {
                    return self
                        .certify_observed_result_after(observation, job, world, installed, after)
                        .map(|result| ResidentPhaseResult::Complete(Box::new(result)));
                }
                let products = Arc::clone(&self.products);
                let (observation, retained) = observation.prepare_current(
                    &products,
                    &job.lease(),
                    job.deadline(),
                    |context, deadline, cancelled| {
                        job.require_resident(self)?;
                        if !std::ptr::eq(context, job.request())
                            || job.profile() != &self.state.profile
                        {
                            return Err(Failure::Binding.into());
                        }
                        self.calculate_with_job(
                            installed,
                            deadline,
                            cancelled,
                            Some(&job),
                            |provider, compatibility, ledger| {
                                ledger.prepare_retained(
                                    world,
                                    compatibility,
                                    provider,
                                    context,
                                    201,
                                    deadline,
                                    cancelled,
                                )
                            },
                        )
                    },
                )?;
                Ok(ResidentPhaseResult::Prepared(Box::new(PreparedResident {
                    observation,
                    retained,
                })))
            }
            ResidentPhase::Prepared(prepared) => self
                .certify_result_with(job, |owner, job| {
                    let PreparedResident {
                        observation,
                        retained,
                    } = *prepared;
                    let deadline = job.deadline().min(retained.deadline());
                    let products = Arc::clone(&owner.products);
                    let facts = observation.with_current(
                        &products,
                        &job.lease(),
                        deadline,
                        |context, deadline, cancelled| {
                            job.require_resident(owner)?;
                            if !std::ptr::eq(context, job.request())
                                || job.profile() != &owner.state.profile
                            {
                                return Err(Failure::Binding.into());
                            }
                            owner.calculate_with_job(
                                installed,
                                deadline,
                                cancelled,
                                Some(&job),
                                |provider, compatibility, ledger| {
                                    ledger.finish_retained(
                                        retained,
                                        world,
                                        compatibility,
                                        provider,
                                        context,
                                        deadline,
                                        cancelled,
                                    )
                                },
                            )
                        },
                    )?;
                    after()?;
                    Ok(facts)
                })
                .map(|result| ResidentPhaseResult::Complete(Box::new(result))),
        }
    }
}
