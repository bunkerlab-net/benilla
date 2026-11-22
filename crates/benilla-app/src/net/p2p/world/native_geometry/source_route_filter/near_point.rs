// SPDX-License-Identifier: GPL-3.0-or-later
//! Initial Ground NearPoint on the same intrinsically bound resident context.
//! Unsupported fallback never becomes a guessed point or a movement certificate.
use super::super::super::{
    core::PrimaryNearPointPolicy,
    products::{
        ground_height::{allowed_ground_z, map_height},
        source_height::HeightRequest,
        vmap::{QueryBudget, RayReply, StaticRay},
    },
    source_ground_height::initial_candidate_position,
    source_near_point::exact_point,
    source_player_environment::resident::{
        ResidentEnvironmentError, ResidentPlayerEnvironmentProvider,
    },
};
use super::*;
use benilla_sim::near_point::primary;
use benilla_world_runtime::{
    native::{AllowedPositionZBranch, Facts, MotionFacts},
    PlayerId,
};

#[derive(Clone, Debug)]
pub(crate) enum NearPointObservation {
    Ground {
        angle: u32,
        request: HeightRequest,
        height: u32,
        allowed_z: u32,
    },
    Ray(RayReply),
}
impl<'a, 'terrain, 'bundle, 'source> ResidentFilterProvider<'a, 'terrain, 'bundle, 'source> {
    /// Compose the narrower source-only responder with this already-validated
    /// selected geometry/profile context. No caller may provide a replacement revision.
    pub(crate) fn player_environment(
        &self,
        player: PlayerId,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<
        ResidentPlayerEnvironmentProvider<'a, 'terrain, 'bundle, 'source>,
        ResidentEnvironmentError,
    > {
        if self.profile.geometry_revision
            != self
                .geometry_policy
                .revision(self.height.binding(), self.query.binding())
                .map_err(ResidentEnvironmentError::Context)?
        {
            return Err(ResidentEnvironmentError::Context(Failure::Binding));
        }
        ResidentPlayerEnvironmentProvider::bind(
            &self.session,
            &self.profile,
            player,
            self.height,
            deadline,
            cancelled,
        )
    }
    pub(crate) fn resolve_primary_near_point(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        self.primary_inner(session, request, deadline, cancelled, |_| {})
    }
    #[cfg(test)]
    pub(crate) fn resolve_primary_near_point_observed(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(&NearPointObservation),
    ) -> Result<Facts, ResidentFilterError> {
        self.primary_inner(session, request, deadline, cancelled, observe)
    }
    fn primary_inner(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observe: impl FnMut(&NearPointObservation),
    ) -> Result<Facts, ResidentFilterError> {
        let poll = || -> Result<(), ResidentFilterError> {
            if cancelled() {
                return Err(ResidentFilterError::Source(
                    source_height::CompositionError::Cancelled,
                ));
            }
            if Instant::now() >= deadline {
                return Err(Failure::Deadline.into());
            }
            Ok(())
        };
        poll()?;
        // The independent participant witness establishes an empty dynamic tree;
        // the source's empty NearPoint selector cannot establish that property.
        session.check_participants()?;
        self.session.check_participants()?;
        let expected = PrimaryNearPointPolicy::from_profile(&self.profile)?;
        // The installed dry revision authorizes only its distinct Swimming algorithm.
        if self.geometry_policy.dry_primary
            || session.world != self.session.world
            || session.profile != self.session.profile
            || session.policy != self.session.policy
            || session.content != self.session.content
            || session.geometry != self.session.geometry
            || session.map != self.session.map
            || session.bodies != self.session.bodies
            || session.player != self.session.player
            || session.static_los != self.session.static_los
            || session.source_location != self.session.source_location
            || session.player_environment != self.session.player_environment
            || session.same_polygon_route != self.session.same_polygon_route
            || session.primary_near_point.as_ref() != Some(&expected)
            || self.session.primary_near_point.as_ref() != Some(&expected)
            || session.empty_selector_near_point.is_some()
            || self.session.empty_selector_near_point.is_some()
            || session.geometry
                != self
                    .geometry_policy
                    .revision(self.height.binding(), self.query.binding())?
        {
            return Err(Failure::Binding.into());
        }
        session.body(request)?;
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if near
            .searcher
            .allowed_position_z_branch()
            .map_err(|_| Failure::Binding)?
            != AllowedPositionZBranch::Ground
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        poll()?;
        let q = &near.request;
        let (angle, position) = initial_candidate_position(q)?;
        let mut budget = QueryBudget::new(deadline);
        let ground_request = HeightRequest {
            binding: self.height.binding(),
            ticket: 1,
            position,
            max_search: 10.0f32.to_bits(),
        };
        let ground = map_height(
            position.map(f32::from_bits),
            false,
            |p, enabled, search| {
                if !enabled || search.to_bits() != 10.0f32.to_bits() {
                    return Err(super::super::super::products::Error::Unsupported);
                }
                self.height
                    .query_with_budget(
                        HeightRequest {
                            position: p.map(f32::to_bits),
                            max_search: search.to_bits(),
                            ..ground_request
                        },
                        &mut budget,
                    )
                    .map(|reply| f32::from_bits(reply.height))
            },
            |_, _| Ok(f32::NEG_INFINITY),
        )
        .map_err(Failure::product)?;
        let allowed_z = allowed_ground_z(f32::from_bits(position[2]), ground).to_bits();
        observe(&NearPointObservation::Ground {
            angle,
            request: ground_request,
            height: ground.to_bits(),
            allowed_z,
        });
        poll()?;
        if !primary::vertical_acceptance_bits(
            q.target.pose.position.z.bits(),
            allowed_z,
            q.distance.bits(),
            q.owner_bounding_radius.bits(),
            q.target.bounding_radius.bits(),
        )
        .map_err(|_| Failure::Invalid)?
        {
            return Err(Failure::UnsupportedNearPointFallback.into());
        }
        let target = q.target.pose.position;
        let from = primary::target_eye_bits(
            [target.x.bits(), target.y.bits(), target.z.bits()],
            q.target.collision_height.bits(),
        )
        .map_err(|_| Failure::Invalid)?;
        let ray = self
            .height
            .source_ray_with_budget(
                2,
                from,
                [position[0], position[1], allowed_z],
                &mut budget,
                &cancelled,
            )
            .map_err(ResidentFilterError::Source)?;
        observe(&NearPointObservation::Ray(ray));
        poll()?;
        if !matches!(ray.result, StaticRay::Miss) {
            return Err(Failure::UnsupportedNearPointFallback.into());
        }
        let point = exact_point([position[0], position[1], allowed_z])?;
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        budget.complete().map_err(Failure::product)?;
        poll()?;
        Ok(Facts::SourceMotion(MotionFacts::Spatial(Box::new(
            motion::Reply {
                request: motion::Query::NearPoint(q.clone()),
                fact: motion::Fact::NearPoint(point),
            },
        ))))
    }
}
