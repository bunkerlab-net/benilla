// SPDX-License-Identifier: GPL-3.0-or-later
//! The sole proposal-to-Route seam takes the concrete production LocalCorridor.
//! A generic Corridor implementation cannot supply this conversion.
use super::{
    core::{Body, Corridor, Failure, RouteJob, ServiceStep, Session, ValidatedRequest},
    source_route_same_polygon::{
        SamePolygonError, SamePolygonProposal, SourceSamePolygonRouteProvider,
    },
    LocalCorridor,
};
use benilla_sim::{
    creature_ai::movement::{Check, SegmentChecks},
    creature_motion as motion,
};
use benilla_world_runtime::native::{AuthorityRequest, Facts, MotionFacts, MotionQuery, Query};
use sha2::{Digest, Sha256};
use std::time::Instant;

pub(super) fn resolve(
    provider: &mut SourceSamePolygonRouteProvider,
    session: &Session,
    context: &AuthorityRequest,
    corridor: &mut LocalCorridor<'_, '_, '_>,
    first_ticket: u64,
    deadline: Instant,
) -> Result<Facts, SamePolygonError> {
    // Preserve raw-entry validation and its structural-error precedence.
    first_ticket.checked_add(6).ok_or(Failure::Invalid)?;
    session.with_validated_request(context, |validated| {
        resolve_validated(provider, validated, corridor, first_ticket, deadline)
    })?
}

/// Accept only the exact objects already checked in this synchronous attempt.
/// Calculate still validates provider binding, live dispatch and all loaded inputs.
pub(super) fn resolve_validated(
    provider: &mut SourceSamePolygonRouteProvider,
    validated: ValidatedRequest<'_>,
    corridor: &mut LocalCorridor<'_, '_, '_>,
    first_ticket: u64,
    deadline: Instant,
) -> Result<Facts, SamePolygonError> {
    let segment_ticket = first_ticket.checked_add(6).ok_or(Failure::Invalid)?;
    let session = validated.session();
    let context = validated.request();
    let body = validated.body();
    if corridor.map != session.map || corridor.revision != provider.raw_geometry() {
        return Err(Failure::Binding.into());
    }
    let proposal = provider.calculate(session, context, first_ticket, deadline)?;
    let result = certify_proposal(
        provider,
        &proposal,
        corridor,
        CertificationInput {
            session,
            context,
            body,
            segment_ticket,
            deadline,
        },
        false,
    )?;
    provider.certify(session, &proposal, &result)?;
    #[cfg(test)]
    proposal.report_timing();
    Ok(result)
}

/// Staged native work never carries a concrete clearance proof across invocations.
pub(super) fn resolve_job(
    provider: &mut SourceSamePolygonRouteProvider,
    job: RouteJob<'_>,
    corridor: &mut LocalCorridor<'_, '_, '_>,
    first_ticket: u64,
    deadline: Instant,
) -> Result<ServiceStep, SamePolygonError> {
    let segment_ticket = first_ticket.checked_add(6).ok_or(Failure::Invalid)?;
    if corridor.map != job.session().map || corridor.revision != provider.raw_geometry() {
        return Err(Failure::Binding.into());
    }
    if !provider.advance_job(&job, first_ticket, deadline)? {
        return Ok(ServiceStep::Pending);
    }
    let proposal = provider.terminal_for_job(&job)?;
    let facts = certify_proposal(
        provider,
        proposal,
        corridor,
        CertificationInput {
            session: job.session(),
            context: job.request(),
            body: job.body(),
            segment_ticket,
            deadline,
        },
        true,
    )?;
    provider.certify_job(&job, &facts)?;
    Ok(ServiceStep::Complete(Box::new(facts)))
}

struct CertificationInput<'a> {
    session: &'a Session,
    context: &'a AuthorityRequest,
    body: Body,
    segment_ticket: u64,
    deadline: Instant,
}
fn certify_proposal(
    provider: &SourceSamePolygonRouteProvider,
    proposal: &SamePolygonProposal,
    corridor: &mut LocalCorridor<'_, '_, '_>,
    input: CertificationInput<'_>,
    staged: bool,
) -> Result<Facts, SamePolygonError> {
    let CertificationInput {
        session,
        context,
        body,
        segment_ticket,
        deadline,
    } = input;
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::UnsupportedQuery.into());
    };
    let q = &route.request;
    let points: Vec<_> = proposal
        .terminal()
        .points()
        .iter()
        .copied()
        .map(super::source_near_point::exact_point)
        .collect::<Result<_, _>>()?;
    if points != [q.origin, q.goal] || proposal.after_cache().requested_end != q.goal {
        return Err(Failure::Binding.into());
    }
    let [from, to] = [q.origin, q.goal].map(|p| [p.x.as_f32(), p.y.as_f32(), p.z.as_f32()]);
    if Instant::now() >= deadline {
        #[cfg(test)]
        eprintln!(
            "native geometry local route deadline failure: tick={} checkpoint=before corridor.certify overdue={:?}",
            context.binding.tick,
            Instant::now().saturating_duration_since(deadline),
        );
        return Err(Failure::Deadline.into());
    }
    // Real raw coverage, membership, body sweep, elevated LOS and whole swept
    // rectangle support/dry proof, including exact feet at both endpoints.
    corridor.certify(from, to, body)?;
    // This is separate from the Chase gate's owner-height / target-height ray.
    // Every route segment uses the owner's height at BOTH ends and includes M2.
    let sight = if staged {
        provider.segment_line_of_sight_prepared(proposal, segment_ticket, deadline)
    } else {
        provider.segment_line_of_sight(session, proposal, segment_ticket, deadline)
    };
    #[cfg(test)]
    if let Err(error) = &sight {
        eprintln!(
            "native geometry local route failure: tick={} stage=segment_line_of_sight error={error:?} deadline_expired={}",
            context.binding.tick,
            Instant::now() >= deadline,
        );
    }
    sight?;
    if Instant::now() >= deadline {
        #[cfg(test)]
        eprintln!(
            "native geometry local route deadline failure: tick={} checkpoint=after segment_line_of_sight overdue={:?}",
            context.binding.tick,
            Instant::now().saturating_duration_since(deadline),
        );
        return Err(Failure::Deadline.into());
    }
    let mut identity = Sha256::new();
    identity.update(b"benilla-source-owned-same-polygon-real-corridor-route-v1");
    identity.update(session.geometry);
    identity.update(session.world);
    identity.update(session.profile);
    identity.update(
        context
            .binding
            .npc
            .ok_or(Failure::Binding)?
            .generation
            .to_le_bytes(),
    );
    identity.update(q.binding.identity.creature.to_le_bytes());
    identity.update(q.binding.query_id.to_le_bytes());
    identity.update(q.binding.now_ms.to_le_bytes());
    identity.update(proposal.after_cache().revision.to_le_bytes());
    for point in &points {
        for value in [point.x, point.y, point.z] {
            identity.update(value.bits().to_le_bytes());
        }
    }
    for poly in &proposal.after_cache().poly_refs {
        identity.update(poly.to_le_bytes());
    }
    let calculation = motion::RouteCalculation {
        source: motion::RouteSource::OwnerPathFinder,
        outcome: motion::RouteOutcome::Complete(motion::CompleteRoute {
            source: motion::RouteSource::OwnerPathFinder,
            path_identity: identity.finalize().into(),
            points,
            segments: vec![SegmentChecks {
                loaded_coverage: Check::Passed,
                line_of_sight: Check::Passed,
                body_sweep: Check::Passed,
                continuous_ground_support: Check::Passed,
                dry_ground: Check::Passed,
            }],
        }),
        prior_cache: q.prior_cache.clone(),
        after_cache: proposal.after_cache().clone(),
    };
    calculation.validate_for(q).map_err(|_| Failure::Binding)?;
    // This immutable candidate remains staged. Neither certification nor returning
    // facts acknowledges an outer step_batch commit or prunes rollback ancestors.
    let facts = Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
        request: motion::Query::Route(q.clone()),
        fact: motion::Fact::RouteCalculation(Box::new(calculation)),
    })));
    Ok(facts)
}

#[cfg(test)]
#[path = "source_route_same_polygon_local/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "source_route_same_polygon_local/two_owner_tests.rs"]
mod two_owner_tests;

#[cfg(test)]
#[path = "source_route_same_polygon_local/quest_loop_tests.rs"]
mod quest_loop_tests;
