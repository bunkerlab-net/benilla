// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only result seal. Only synchronous genuine resident certification mints it.
use super::super::super::super::{
    core::{self, resident_dispatch::ResidentDispatchValidation, JobLease},
    products::source_height::freshness_timings::{Phase, Timer},
    source_route_resident_local::ledger::ResidentCandidateStamp,
};
use super::*;
use benilla_sim::creature_ai::movement::{Check, SegmentChecks};
use benilla_world_runtime::native::{MotionFacts, MotionQuery, Query};

pub(crate) struct ResidentTransportSelection {
    ledger_identity: Arc<()>,
    session: Session,
    profile: Profile,
    player: PlayerId,
    inventory: [u8; 32],
    source_epoch: [u8; 32],
    first_retained: bool,
}
impl ResidentTransportSelection {
    pub(super) fn new(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        identity: &ResidentProviderIdentity,
        ledger_identity: Arc<()>,
    ) -> Self {
        Self {
            ledger_identity,
            session: session.clone(),
            profile: profile.clone(),
            player,
            inventory: identity.inventory,
            source_epoch: identity.source_epoch,
            first_retained: identity.one_retained_route
                && !identity.two_retained_routes
                && !identity.finite_retained_routes
                && !identity.finite_straight_routes
                && !identity.two_owner_first_retained,
        }
    }
    pub(crate) fn session(&self) -> &Session {
        &self.session
    }
    pub(crate) fn request(&self, request: &AuthorityRequest) -> Result<(), Failure> {
        if !self.first_retained
            || self.profile.npcs.len() != 1
            || self.session.same_polygon_route.is_some()
        {
            return Err(Failure::UnsupportedQuery);
        }
        ResidentDispatchValidation::validate_shape(
            &self.session,
            &self.profile,
            self.player,
            request,
        )
    }
    pub(crate) fn boundary(&self, boundary: &RouteCommitBoundary) -> Result<(), Failure> {
        if !self.first_retained
            || boundary.world() != self.session.world
            || boundary.player() != self.player
            || boundary.profile() != self.profile.digest()
            || boundary.profile() != self.session.profile
            || boundary.geometry() != self.session.geometry
            || boundary.authority() != self.session.policy
            || boundary.heads().len() != self.profile.npcs.len()
        {
            return Err(Failure::Binding);
        }
        for head in boundary.heads() {
            if !self.session.bodies.contains_key(&head.npc().spawn)
                || head.suspended_wander().is_some()
                || !matches!(
                    head.constructor_kind(),
                    None | Some(motion::PathfinderConstructorKind::ChaseLazy)
                )
                || head.cache().is_some() && head.construction().is_none()
                || head.construction().is_some() && head.constructor_kind().is_none()
            {
                return Err(Failure::Binding);
            }
        }
        Ok(())
    }
    fn facts(&self, request: &AuthorityRequest, facts: &Facts) -> Result<(), Failure> {
        self.request(request)?;
        let (
            Query::SourceMotion(MotionQuery::Route(route)),
            Facts::SourceMotion(MotionFacts::Spatial(reply)),
        ) = (&request.query, facts)
        else {
            return Err(Failure::Binding);
        };
        if reply.request != motion::Query::Route(route.request.clone()) {
            return Err(Failure::Binding);
        }
        let motion::Fact::RouteCalculation(calculation) = &reply.fact else {
            return Err(Failure::Binding);
        };
        calculation
            .validate_for(&route.request)
            .map_err(|_| Failure::Binding)?;
        let motion::RouteOutcome::Complete(path) = &calculation.outcome else {
            return Err(Failure::Binding);
        };
        let cache = &calculation.after_cache;
        let include = benilla_sim::route_filter::initial_include_flags(
            route.owner.capabilities().map_err(|_| Failure::Binding)?,
        )
        .ok_or(Failure::Binding)?;
        let revision = if route.request.prior_cache.is_some() {
            2
        } else {
            1
        };
        let passed = SegmentChecks {
            loaded_coverage: Check::Passed,
            line_of_sight: Check::Passed,
            body_sweep: Check::Passed,
            continuous_ground_support: Check::Passed,
            dry_ground: Check::Passed,
        };
        if calculation.source != motion::RouteSource::OwnerPathFinder
            || cache.revision != revision
            || cache.binding.owner_generation
                != request.binding.npc.ok_or(Failure::Binding)?.generation
            || cache.binding.navigation_policy != self.session.geometry
            || cache.binding.inventory != self.inventory
            || cache.binding.source_epoch != self.source_epoch
            || cache.include_flags != include
            || cache.exclude_flags != 0
            || cache.active_len != 1
            || cache.path_type != 1
            || path.source != motion::RouteSource::OwnerPathFinder
            || path.path_identity == [0; 32]
            || path.points != [route.request.origin, cache.requested_end]
            || path.segments != [passed]
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
}

/// Constructed only after the synchronous attempt returned an error, before any result escaped.
pub(in super::super::super::super) struct RejectedResidentCertification {
    selection: Arc<ResidentTransportSelection>,
    request: AuthorityRequest,
    lease: JobLease,
    first_id: u64,
}
impl RejectedResidentCertification {
    pub(in super::super::super::super) fn parts(
        &self,
    ) -> Result<(&Arc<()>, &AuthorityRequest, u64), Failure> {
        // Original job identity remains available even after its deadline expires.
        let original = self.lease.request();
        if original != &self.request
            || !original
                .require_pathfinder_dispatch(&self.selection.profile)
                .map_err(|_| Failure::Binding)?
                .same_live_dispatch(
                    self.request
                        .require_pathfinder_dispatch(&self.selection.profile)
                        .map_err(|_| Failure::Binding)?,
                )
        {
            return Err(Failure::Binding);
        }
        Ok((
            &self.selection.ledger_identity,
            &self.request,
            self.first_id,
        ))
    }
}

pub(crate) struct CertifiedResidentResult {
    selection: Arc<ResidentTransportSelection>,
    lease: JobLease,
    request: AuthorityRequest,
    facts: Facts,
    candidate: ResidentCandidateStamp,
}
impl CertifiedResidentResult {
    pub(in super::super::super::super) fn retirement_parts(
        &self,
    ) -> (&Arc<()>, &ResidentCandidateStamp, &AuthorityRequest, &Facts) {
        (
            &self.selection.ledger_identity,
            &self.candidate,
            &self.request,
            &self.facts,
        )
    }
    pub(in super::super::super::super) fn matches_original_job(
        &self,
        lease: &JobLease,
    ) -> Result<(), Failure> {
        if self.lease.same_job(lease) {
            Ok(())
        } else {
            Err(Failure::Binding)
        }
    }
    pub(crate) fn facts(&self) -> &Facts {
        &self.facts
    }
    pub(in super::super::super::super) fn validate_reply(
        &self,
        selection: &Arc<ResidentTransportSelection>,
        lease: &JobLease,
        request: &AuthorityRequest,
        facts: &Facts,
    ) -> Result<(), Failure> {
        if !Arc::ptr_eq(selection, &self.selection)
            || !self.lease.same_job(lease)
            || self.request != *request
            || self.facts != *facts
        {
            return Err(Failure::Binding);
        }
        // Work has already been dropped after delivery. This is immutable selected
        // evidence; the waiting receiver checks attachment and original deadline.
        let saved = self
            .request
            .require_pathfinder_dispatch(&self.selection.profile)
            .map_err(|_| Failure::Binding)?;
        let current = request
            .require_pathfinder_dispatch(&self.selection.profile)
            .map_err(|_| Failure::Binding)?;
        if !saved.same_live_dispatch(current) {
            return Err(Failure::Binding);
        }
        self.selection.facts(request, facts)
    }
    pub(in super::super::super::super) fn validate_staged_result(
        &self,
        owner: &OwnedResident,
        lease: &JobLease,
    ) -> Result<(), Failure> {
        self.validate_reply(&owner.transport, lease, &self.request, &self.facts)?;
        owner
            .ledger
            .validate_transport_candidate(&self.candidate, &self.request, &self.facts)
    }
    pub(in super::super::super::super) fn validate_service(
        &self,
        owner: &OwnedResident,
        lease: &JobLease,
    ) -> Result<(), Failure> {
        self.validate_staged_result(owner, lease)?;
        if !lease.is_active() {
            return Err(Failure::Deadline);
        }
        Ok(())
    }
}
impl OwnedResident {
    pub(crate) fn transport_selection(&self) -> Arc<ResidentTransportSelection> {
        Arc::clone(&self.transport)
    }
    pub(crate) fn certify_observed_result(
        &mut self,
        observation: FreshnessObservation,
        job: ResidentDispatchJob<'_>,
        world: &mut World,
        installed: &NativeInventorySource,
    ) -> Result<CertifiedResidentResult, ResidentFilterError> {
        self.certify_observed_result_after(observation, job, world, installed, || Ok(()))
    }
    /// Failure-only test injection after genuine certification, never a Facts constructor.
    pub(in super::super::super::super) fn certify_observed_result_after(
        &mut self,
        observation: FreshnessObservation,
        job: ResidentDispatchJob<'_>,
        world: &mut World,
        installed: &NativeInventorySource,
        after: impl FnOnce() -> Result<(), Failure>,
    ) -> Result<CertifiedResidentResult, ResidentFilterError> {
        self.certify_result_with(job, |owner, job| {
            owner
                .certify_observed(observation, job, world, installed)
                .and_then(|facts| {
                    after()?;
                    Ok(facts)
                })
        })
    }

    pub(super) fn certify_result_with(
        &mut self,
        job: ResidentDispatchJob<'_>,
        calculate: impl FnOnce(&mut Self, ResidentDispatchJob<'_>) -> Result<Facts, ResidentFilterError>,
    ) -> Result<CertifiedResidentResult, ResidentFilterError> {
        let prechecks = Timer::start(Phase::ResultPrechecks);
        job.require_resident(self)?;
        let lease = job.lease();
        let request = job.request().clone();
        self.transport.request(&request)?;
        let first_id = self.ledger.next_transport_candidate();
        prechecks.finish(true);
        let callback = Timer::start(Phase::CalculateCallback);
        let calculated = calculate(self, job);
        callback.finish(calculated.is_ok());
        let seal = Timer::start(Phase::ResultSeal);
        let facts = match calculated {
            Ok(facts) => facts,
            Err(error) => {
                let proof = RejectedResidentCertification {
                    selection: Arc::clone(&self.transport),
                    request,
                    lease,
                    first_id,
                };
                self.ledger.retire_rejected_transport_attempt(&proof)?;
                return Err(error);
            }
        };
        let result = (|| {
            self.transport.facts(&request, &facts)?;
            let candidate = self.ledger.staged_transport_candidate(&request, &facts)?;
            let result = CertifiedResidentResult {
                selection: Arc::clone(&self.transport),
                lease: lease.clone(),
                request: request.clone(),
                facts,
                candidate,
            };
            result.validate_service(self, &lease)?;
            Ok::<_, Failure>(result)
        })();
        match result {
            Ok(result) => {
                seal.finish(true);
                Ok(result)
            }
            Err(error) => {
                let proof = RejectedResidentCertification {
                    selection: Arc::clone(&self.transport),
                    request,
                    lease,
                    first_id,
                };
                self.ledger.retire_rejected_transport_attempt(&proof)?;
                Err(error.into())
            }
        }
    }

    /// Retire an actual never-published probe result, preserving monotonic staging IDs.
    pub(in super::super::super::super) fn retire_unpublished_probe_result(
        &mut self,
        result: CertifiedResidentResult,
        first_id: u64,
    ) -> Result<(), Failure> {
        result.validate_staged_result(self, &result.lease)?;
        let proof = RejectedResidentCertification {
            selection: result.selection,
            request: result.request,
            lease: result.lease,
            first_id,
        };
        self.ledger.retire_rejected_transport_attempt(&proof)
    }

    #[cfg(test)]
    pub(in super::super::super::super) fn assert_committed_transport_refusal_for_test(&self) {
        self.ledger.assert_committed_transport_refusal();
    }
    #[cfg(test)]
    pub(in super::super::super::super) fn assert_result_refusals_for_test(
        &self,
        result: &CertifiedResidentResult,
    ) {
        let saved = &self.transport;
        let foreign = Arc::new(ResidentTransportSelection {
            ledger_identity: Arc::clone(&saved.ledger_identity),
            session: saved.session.clone(),
            profile: saved.profile.clone(),
            player: saved.player,
            inventory: saved.inventory,
            source_epoch: saved.source_epoch,
            first_retained: saved.first_retained,
        });
        assert_eq!(
            result.validate_reply(&foreign, &result.lease, &result.request, &result.facts),
            Err(Failure::Binding)
        );
        for mutation in 0..4 {
            let mut facts = result.facts.clone();
            let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &mut facts else {
                unreachable!()
            };
            let motion::Fact::RouteCalculation(calculation) = &mut spatial.fact else {
                unreachable!()
            };
            let motion::RouteOutcome::Complete(path) = &mut calculation.outcome else {
                unreachable!()
            };
            match mutation {
                0 => calculation.after_cache.revision += 1,
                1 => calculation.after_cache.binding.source_epoch[0] ^= 1,
                2 => path.segments[0].body_sweep = Check::Failed,
                _ => {
                    path.points.pop();
                }
            }
            assert_eq!(saved.facts(&result.request, &facts), Err(Failure::Binding));
            assert_eq!(
                result.validate_reply(saved, &result.lease, &result.request, &facts),
                Err(Failure::Binding)
            );
        }
        self.ledger
            .assert_transport_stamp_refusals(&result.request, &result.facts);
    }
    pub(crate) fn retire_undelivered_result(
        &mut self,
        proof: &core::UndeliveredResidentResult,
    ) -> Result<(), Failure> {
        let result = proof.result()?;
        if !Arc::ptr_eq(&self.transport, &result.selection) {
            return Err(Failure::Binding);
        }
        self.ledger.retire_unsent_transport_candidate(proof)
    }
}
