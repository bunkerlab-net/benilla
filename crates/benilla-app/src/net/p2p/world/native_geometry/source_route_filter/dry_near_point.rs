// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in first-candidate Swimming NearPoint over fresh resident NO_WATER points.
//! Aura state is unobserved: exact NoWater returns ground for either predicate.
//! This provider has no Route conversion, movement certificate or admission API.
use super::super::super::{
    core::PrimaryNearPointPolicy,
    products::{
        ground_height::{allowed_ground_z, map_height},
        source_height::{HeightRequest, ResidentInitializationConfiguration},
        vmap::{QueryBudget, RayReply, StaticRay},
    },
    source_ground_height::initial_candidate_position,
    source_near_point::exact_point,
};
use super::*;
use benilla_sim::near_point::primary;
use benilla_world_runtime::native::{
    AllowedPositionZBranch, CreatureInitializationSnapshot, Facts, MotionFacts,
};

/// Intrinsic, deliberately separate from the resident Ground-v2 policy.
/// Configuration is fixed to enabled height/LOS; bind refuses missing/disabled.
pub(crate) fn geometry_revision(height: source_height::Binding) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"benilla-resident-primary-dry-swimming-v1");
    hash.update(b"height50;empty-dynamic;fresh-static50-vmap-gridmap;mask15;default4002007e;exact-nowater;aura-unobserved;selected-model-bound;valid-ground-only;strict-clamp;independent-los;shared-budget");
    hash.update(height.map.to_le_bytes());
    hash.update(height.revision);
    hash.update(b"52c6d22fbe9f275196a44bf4f9b39da527071e3c81f7460a4ac03a3c9107b85d");
    hash.update(b"03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0");
    for policy in [
        primary::POLICY,
        benilla_sim::near_point::POLICY,
        benilla_sim::source_trig::SOFT_POLICY,
        benilla_sim::facing::POLICY,
    ] {
        hash.update((policy.len() as u64).to_le_bytes());
        hash.update(policy.as_bytes());
    }
    for source in primary::SOURCE_SHA256
        .iter()
        .chain(benilla_sim::near_point::SOURCE_SHA256.iter())
        .chain(benilla_sim::source_trig::SOURCE_SHA256.iter())
        .chain(benilla_sim::facing::SOURCE_SHA256.iter())
    {
        hash.update(source.as_bytes());
    }
    hash.finalize().into()
}

#[derive(Clone, Debug)]
pub(crate) enum Observation {
    Ground { request: HeightRequest, height: u32 },
    Liquid(PointNoWaterReply),
    AllowedZ(u32),
    Ray(RayReply),
}

pub(crate) struct ResidentDryNearPointProvider<'a, 'terrain, 'bundle, 'source> {
    session: Session,
    player: benilla_world_runtime::PlayerId,
    profile: Profile,
    height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
    initialization: CreatureInitializationSnapshot,
}
impl<'a, 'terrain, 'bundle, 'source> ResidentDryNearPointProvider<'a, 'terrain, 'bundle, 'source> {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        player: benilla_world_runtime::PlayerId,
        height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
        configuration: Option<ResidentInitializationConfiguration>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, ResidentFilterError> {
        poll(deadline, &cancelled)?;
        profile.validate().map_err(|_| Failure::Binding)?;
        let initialization = height
            .initialization_binding(profile.content_digest, configuration, deadline, &cancelled)
            .map_err(ResidentFilterError::Source)?;
        let provider = Self {
            session: session.clone(),
            player,
            profile: profile.clone(),
            height,
            initialization,
        };
        provider.check_session(session)?;
        poll(deadline, &cancelled)?;
        Ok(provider)
    }
    fn check_session(&self, session: &Session) -> Result<(), ResidentFilterError> {
        session.check_participants()?;
        self.session.check_participants()?;
        let expected = PrimaryNearPointPolicy::from_profile(&self.profile)?;
        if self.player.owner == [0; 16]
            || self.player.character == [0; 16]
            || session.world == [0; 16]
            || session.world != self.session.world
            || session.profile != self.profile.digest()
            || session.policy != self.profile.authority_policy
            || session.content != self.profile.content_digest
            || session.geometry != self.profile.geometry_revision
            || session.geometry != geometry_revision(self.height.binding())
            || session.map != self.profile.map_id
            || session.map != self.height.binding().map
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
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub(crate) fn resolve(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        let mut budget = QueryBudget::new(deadline);
        self.resolve_inner(session, request, deadline, &cancelled, &mut budget, |_| {})
    }
    #[cfg(test)]
    pub(crate) fn resolve_observed(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        work_limit: usize,
        observe: impl FnMut(&Observation),
    ) -> Result<Facts, ResidentFilterError> {
        let mut budget = QueryBudget::new(deadline);
        budget.restrict_work_for_test(work_limit);
        self.resolve_inner(session, request, deadline, &cancelled, &mut budget, observe)
    }
    fn resolve_inner(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
        cancelled: &impl Fn() -> bool,
        budget: &mut QueryBudget,
        mut observe: impl FnMut(&Observation),
    ) -> Result<Facts, ResidentFilterError> {
        poll(deadline, cancelled)?;
        self.check_session(session)?;
        session.body(request)?;
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
            return Err(Failure::UnsupportedQuery.into());
        };
        if near
            .searcher
            .allowed_position_z_branch()
            .map_err(|_| Failure::Binding)?
            != AllowedPositionZBranch::Swimming
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        if self.profile.player_collision_height.is_some() {
            near.require_target_collision_height(&self.profile, session.world, self.player)
                .map_err(|_| Failure::Binding)?;
        } else if near.target_player_height.is_some() {
            return Err(Failure::Binding.into());
        }
        // validate_context above validates the generation's actual selected model,
        // source and collision height. Preserve the operand even though NoWater
        // bypasses depth arithmetic; never substitute classification's default.
        let initial = near.searcher.initialization().ok_or(Failure::Binding)?;
        if initial.request.snapshot != self.initialization {
            return Err(Failure::Binding.into());
        }
        initial
            .request
            .validate_for(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let selected_height = f32::from_bits(initial.request.collision_height.bits());
        if !selected_height.is_finite() || selected_height <= 0.0 || selected_height > 100.0 {
            return Err(Failure::UnresolvedBody.into());
        }
        self.height
            .verify_unchanged(cancelled)
            .map_err(ResidentFilterError::Source)?;
        poll(deadline, cancelled)?;
        let q = &near.request;
        let (_, position) = initial_candidate_position(q)?;
        let ground_request = HeightRequest {
            binding: self.height.binding(),
            ticket: 1,
            position,
            max_search: 50.0f32.to_bits(),
        };
        let ground = map_height(
            position.map(f32::from_bits),
            true,
            |p, enabled, search| {
                if !enabled || search.to_bits() != 50.0f32.to_bits() {
                    return Err(super::super::super::products::Error::Unsupported);
                }
                self.height
                    .query_with_budget(
                        HeightRequest {
                            position: p.map(f32::to_bits),
                            max_search: search.to_bits(),
                            ..ground_request
                        },
                        budget,
                    )
                    .map(|reply| f32::from_bits(reply.height))
            },
            |_, _| Ok(f32::NEG_INFINITY),
        )
        .map_err(Failure::product)?;
        observe(&Observation::Ground {
            request: ground_request,
            height: ground.to_bits(),
        });
        poll(deadline, cancelled)?;
        // Invalid-ground continuation is outside this resident adapter's domain.
        if !valid_ground(ground) {
            return Err(Failure::UnsupportedQuery.into());
        }
        let liquid_request = PointNoWaterRequest {
            binding: self.height.binding(),
            ticket: 2,
            position: [position[0], position[1], ground.to_bits()],
            mask: 15,
        };
        let liquid = self
            .height
            .point_no_water_with_budget(liquid_request, budget, cancelled)
            .map_err(ResidentFilterError::Source)?;
        if liquid.request() != liquid_request {
            return Err(Failure::Binding.into());
        }
        observe(&Observation::Liquid(liquid));
        poll(deadline, cancelled)?;
        // Exact NoWater returns OUTER Map groundZ for both aura predicate values,
        // never the nested static height/effective floor. Strict comparisons retain
        // equal signed-zero input bits, unlike an unconditional replacement.
        let allowed_z = allowed_ground_z(f32::from_bits(position[2]), ground).to_bits();
        observe(&Observation::AllowedZ(allowed_z));
        poll(deadline, cancelled)?;
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
                3,
                from,
                [position[0], position[1], allowed_z],
                budget,
                cancelled,
            )
            .map_err(ResidentFilterError::Source)?;
        observe(&Observation::Ray(ray));
        poll(deadline, cancelled)?;
        if !matches!(ray.result, StaticRay::Miss) {
            return Err(Failure::UnsupportedNearPointFallback.into());
        }
        let point = exact_point([position[0], position[1], allowed_z])?;
        self.height
            .verify_unchanged(cancelled)
            .map_err(ResidentFilterError::Source)?;
        budget.complete().map_err(Failure::product)?;
        poll(deadline, cancelled)?;
        Ok(Facts::SourceMotion(MotionFacts::Spatial(Box::new(
            motion::Reply {
                request: motion::Query::NearPoint(q.clone()),
                fact: motion::Fact::NearPoint(point),
            },
        ))))
    }
}
fn poll(deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), ResidentFilterError> {
    if cancelled() {
        return Err(ResidentFilterError::Source(
            source_height::CompositionError::Cancelled,
        ));
    }
    if Instant::now() >= deadline {
        return Err(Failure::Deadline.into());
    }
    Ok(())
}

fn valid_ground(ground: f32) -> bool {
    ground.is_finite() && ground > -100_000.0 && ground.abs() <= 100_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_swimming_valid_ground_domain_and_strict_clamp_keep_source_bits() {
        for ground in [-200_000.0, -100_000.0, 100_001.0, f32::INFINITY, f32::NAN] {
            assert!(!valid_ground(ground));
        }
        for ground in [-99_999.0, -0.0, 0.0, 100_000.0] {
            assert!(valid_ground(ground));
            for z in [ground - 1.0, ground, ground + 1.0] {
                assert_eq!(allowed_ground_z(z, ground).to_bits(), ground.to_bits());
            }
        }
        assert_eq!(allowed_ground_z(-0.0, 0.0).to_bits(), (-0.0f32).to_bits());
        assert_eq!(allowed_ground_z(0.0, -0.0).to_bits(), 0.0f32.to_bits());
    }
}
