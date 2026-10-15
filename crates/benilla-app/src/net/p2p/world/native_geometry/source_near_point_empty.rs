// SPDX-License-Identifier: GPL-3.0-or-later
//! Separately selected ground-searcher GetNearPointAt with empty source lists.
//! Exhaustion returns the source's restored first XY and carried/adjusted Z;
//! that final return is not a LOS, support, accessibility or path certificate.
use super::{
    core::{EmptySelectorNearPointPolicy, Failure, Session},
    products::{
        ground_height::MapHeightSnapshot,
        vmap::{QueryBudget, RayRequest, StaticRay},
    },
    source_ground_height::{
        EmptySelectorGroundSequence, SequentialGroundPhase as CandidatePhase,
        SequentialGroundReply, SourceGroundHeightProvider,
    },
    source_near_point::{self, SourceNearPointConfiguration},
};
use benilla_sim::{creature_motion as motion, near_point, source_atan, source_trig};
use benilla_world_runtime::native::{
    AuthorityRequest, Facts, MotionFacts, MotionQuery, Profile, Query,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

// Borrowed observations keep the production no-op observer allocation-free.
// Tests inspect actual loaded queries, never replace their answers.
enum QueryObservation<'a> {
    Ground(&'a SequentialGroundReply),
    Ray(&'a RayRequest, &'a StaticRay),
}

pub(crate) struct SourceEmptySelectorNearPointProvider {
    ground: SourceGroundHeightProvider<'static>,
}

pub(crate) fn geometry_revision(
    snapshot: &MapHeightSnapshot<'_>,
    config: SourceNearPointConfiguration,
) -> Result<[u8; 32], Failure> {
    let primary = source_near_point::geometry_revision(snapshot, config)?;
    let mut hash = Sha256::new();
    hash.update(b"benilla-source-near-point-empty-selector-ground-los-v1");
    hash.update(primary);
    for policy in [
        near_point::selector::POLICY,
        near_point::FALLBACK_POLICY,
        source_atan::POLICY,
        source_trig::FALLBACK_SOFT_POLICY,
    ] {
        hash.update((policy.len() as u64).to_le_bytes());
        hash.update(policy.as_bytes());
    }
    for source in near_point::selector::SOURCE_SHA256
        .iter()
        .chain(source_atan::SOURCE_SHA256.iter())
    {
        hash.update(source.as_bytes());
    }
    Ok(hash.finalize().into())
}

impl SourceEmptySelectorNearPointProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        snapshot: MapHeightSnapshot<'static>,
        config: SourceNearPointConfiguration,
    ) -> Result<Self, Failure> {
        if session.primary_near_point.is_some()
            || session.empty_selector_near_point
                != Some(EmptySelectorNearPointPolicy::from_profile(profile)?)
        {
            return Err(Failure::UnsupportedQuery);
        }
        let ground = SourceGroundHeightProvider::bind_with_empty_selector(
            session, profile, snapshot, config,
        )?;
        Ok(Self { ground })
    }

    pub(crate) fn resolve(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<Facts, Failure> {
        self.resolve_observed(session, request, deadline, |_| {})
    }

    fn resolve_observed(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
        observe: impl FnMut(QueryObservation<'_>),
    ) -> Result<Facts, Failure> {
        self.resolve_budget_observed(session, request, &mut QueryBudget::new(deadline), observe)
    }

    fn resolve_budget_observed(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        budget: &mut QueryBudget,
        mut observe: impl FnMut(QueryObservation<'_>),
    ) -> Result<Facts, Failure> {
        session.body(request)?;
        let bound = self.ground.begin(session, request)?;
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let q = &near.request;
        let mut sequence: EmptySelectorGroundSequence<'_, '_> = bound.empty_selector_sequence()?;
        while let Some(candidate) = sequence.next_with_budget(budget)? {
            observe(QueryObservation::Ground(&candidate));
            let position = candidate.ground.request.position;
            let to = [position[0], position[1], candidate.ground.allowed_z];
            if candidate.phase != CandidatePhase::Final {
                if !near_point::primary::vertical_acceptance_bits(
                    q.target.pose.position.z.as_f32().to_bits(),
                    to[2],
                    q.distance.as_f32().to_bits(),
                    q.owner_bounding_radius.as_f32().to_bits(),
                    q.target.bounding_radius.as_f32().to_bits(),
                )
                .map_err(|_| Failure::Invalid)?
                {
                    continue;
                }
                // Source only evaluates eye/LOS after the vertical predicate.
                let target = q.target.pose.position;
                let from = near_point::primary::target_eye_bits(
                    [target.x, target.y, target.z].map(|v| v.as_f32().to_bits()),
                    q.target.collision_height.as_f32().to_bits(),
                )
                .map_err(|_| Failure::Invalid)?;
                let vmap = self.ground.static_vmap();
                let ray = RayRequest {
                    snapshot: vmap.binding(),
                    ticket: candidate.los_ticket.ok_or(Failure::Binding)?,
                    from,
                    to,
                    ignore_m2: false,
                };
                let result = vmap
                    .query_with_budget(ray, budget)
                    .map_err(|_| Failure::MissingGeometry)?;
                if result.request != ray {
                    return Err(Failure::Binding);
                }
                observe(QueryObservation::Ray(&ray, &result.result));
                if !matches!(result.result, StaticRay::Miss) {
                    continue;
                }
            } else if candidate.los_ticket.is_some() {
                return Err(Failure::Binding);
            }
            // The source returns here, even when the final exhausted point is
            // obstructed. Representation failure cannot select another point.
            let point = source_near_point::exact_point(to)?;
            budget.complete().map_err(|_| Failure::MissingGeometry)?;
            return Ok(Facts::SourceMotion(MotionFacts::Spatial(Box::new(
                motion::Reply {
                    request: motion::Query::NearPoint(q.clone()),
                    fact: motion::Fact::NearPoint(point),
                },
            ))));
        }
        Err(Failure::Binding)
    }
}

#[cfg(test)]
#[path = "source_near_point_empty/tests.rs"]
mod tests;
