// SPDX-License-Identifier: GPL-3.0-or-later
//! Private two-turn cold/first-retained certification; never a scene-proof cache.
use super::super::super::super::{
    products::source_height::{
        freshness_timings::{Phase, Timer},
        CompositionError,
    },
    source_route_resident_local::{
        ledger::{Diagnostic, PreparedCold},
        PreparedRetained,
    },
};
use super::transport::CertifiedResidentResult;
use super::*;

pub(crate) struct PreparedResident {
    observation: FreshnessObservation,
    dependency: PreparedDependency,
}
enum PreparedDependency {
    Cold(Box<PreparedCold>),
    Retained(Box<PreparedRetained>),
}
impl PreparedDependency {
    fn deadline(&self) -> Instant {
        match self {
            Self::Cold(prepared) => prepared.deadline(),
            Self::Retained(prepared) => prepared.deadline(),
        }
    }
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
    pub(crate) fn certify_phase_result_retained_only(
        &mut self,
        phase: ResidentPhase,
        job: ResidentDispatchJob<'_>,
        world: &mut World,
        installed: &NativeInventorySource,
    ) -> Result<ResidentPhaseResult, ResidentFilterError> {
        self.certify_phase_result_inner(phase, job, world, installed, || Ok(()), false)
    }
    pub(in super::super::super::super) fn certify_phase_result_retained_only_after(
        &mut self,
        phase: ResidentPhase,
        job: ResidentDispatchJob<'_>,
        world: &mut World,
        installed: &NativeInventorySource,
        after: impl FnOnce() -> Result<(), Failure>,
    ) -> Result<ResidentPhaseResult, ResidentFilterError> {
        self.certify_phase_result_inner(phase, job, world, installed, after, false)
    }
    pub(in super::super::super::super) fn certify_phase_result_after(
        &mut self,
        phase: ResidentPhase,
        job: ResidentDispatchJob<'_>,
        world: &mut World,
        installed: &NativeInventorySource,
        after: impl FnOnce() -> Result<(), Failure>,
    ) -> Result<ResidentPhaseResult, ResidentFilterError> {
        self.certify_phase_result_inner(phase, job, world, installed, after, true)
    }
    #[allow(clippy::too_many_arguments)]
    fn certify_phase_result_inner(
        &mut self,
        phase: ResidentPhase,
        job: ResidentDispatchJob<'_>,
        world: &mut World,
        installed: &NativeInventorySource,
        after: impl FnOnce() -> Result<(), Failure>,
        cold_split: bool,
    ) -> Result<ResidentPhaseResult, ResidentFilterError> {
        let prechecks = Timer::start(Phase::EntryPrechecks);
        job.require_resident(self)?;
        self.transport.request(job.request())?;
        prechecks.finish(true);
        match phase {
            ResidentPhase::Observation(observation) => {
                let observation = observation.map_err(ResidentFilterError::Source)?;
                let Query::SourceMotion(MotionQuery::Route(route)) = &job.request().query else {
                    return Err(Failure::Binding.into());
                };
                if !cold_split && route.request.prior_cache.is_none() {
                    return self
                        .certify_observed_result_after(observation, job, world, installed, after)
                        .map(|result| ResidentPhaseResult::Complete(Box::new(result)));
                }
                let products = Arc::clone(&self.products);
                let cold = route.request.prior_cache.is_none();
                let player = self.player;
                let (observation, dependency) = observation.prepare_current(
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
                                if cold {
                                    let constructor =
                                        provider.observe(context, 101, deadline, cancelled)?;
                                    let los = provider.observe_chase_los(
                                        &constructor,
                                        context,
                                        player,
                                        103,
                                        deadline,
                                        cancelled,
                                    )?;
                                    let water = provider.observe_chase_water(
                                        &los, context, 104, deadline, cancelled,
                                    )?;
                                    return ledger
                                        .prepare_cold(
                                            world,
                                            compatibility,
                                            provider,
                                            context,
                                            constructor,
                                            water,
                                            105,
                                            deadline,
                                            cancelled,
                                        )
                                        .map(|prepared| {
                                            PreparedDependency::Cold(Box::new(prepared))
                                        });
                                }
                                ledger
                                    .prepare_retained(
                                        world,
                                        compatibility,
                                        provider,
                                        context,
                                        201,
                                        deadline,
                                        cancelled,
                                    )
                                    .map(|prepared| {
                                        PreparedDependency::Retained(Box::new(prepared))
                                    })
                            },
                        )
                    },
                )?;
                Ok(ResidentPhaseResult::Prepared(Box::new(PreparedResident {
                    observation,
                    dependency,
                })))
            }
            ResidentPhase::Prepared(prepared) => self
                .certify_result_with(job, |owner, job| {
                    let PreparedResident {
                        observation,
                        dependency,
                    } = *prepared;
                    let deadline = job.deadline().min(dependency.deadline());
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
                                |provider, compatibility, ledger| match dependency {
                                    PreparedDependency::Cold(prepared) => ledger.finish_cold(
                                        *prepared,
                                        world,
                                        compatibility,
                                        provider,
                                        context,
                                        deadline,
                                        cancelled,
                                    ),
                                    PreparedDependency::Retained(prepared) => ledger
                                        .finish_retained(
                                            *prepared,
                                            world,
                                            compatibility,
                                            provider,
                                            context,
                                            deadline,
                                            cancelled,
                                        ),
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

impl PreparedResident {
    pub(super) fn is_f05_retained(&self) -> bool {
        matches!(self.dependency, PreparedDependency::Retained(_))
    }

    pub(super) fn into_f05_retained(
        self,
    ) -> Result<(FreshnessObservation, PreparedRetained), ResidentFilterError> {
        match self.dependency {
            PreparedDependency::Retained(prepared) => Ok((self.observation, *prepared)),
            PreparedDependency::Cold(_) => Err(Failure::Binding.into()),
        }
    }
}
