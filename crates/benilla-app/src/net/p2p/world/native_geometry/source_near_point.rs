// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact original-candidate acceptance only. A required fallback is unavailable,
//! never a blocked fact, guessed destination or complete OwnerPathFinder claim.
use super::{
    core::{Failure, Session},
    products::{
        ground_height::{MapHeightSnapshot, SourceHeightConfiguration},
        vmap::{QueryBudget, RayRequest, StaticRay},
    },
    source_ground_height::{self, SourceGroundHeightProvider},
};
use benilla_sim::{creature_ai::Point, creature_motion as motion, near_point::primary};
use benilla_world_runtime::native::{
    AuthorityRequest, Facts, MotionFacts, MotionQuery, Profile, Query,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

/// Explicit effective-source declarations, not authenticated configuration capture.
/// The height declaration must equal the one retained by the loaded snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceNearPointConfiguration {
    pub height: SourceHeightConfiguration,
    pub detect_position_collision: bool,
    pub vmap_line_of_sight_enabled: bool,
}

pub(crate) struct SourceNearPointProvider {
    ground: SourceGroundHeightProvider<'static>,
}

pub(crate) fn geometry_revision(
    snapshot: &MapHeightSnapshot<'_>,
    config: SourceNearPointConfiguration,
) -> Result<[u8; 32], Failure> {
    if config.height != snapshot.configuration() {
        return Err(Failure::Binding);
    }
    if !config.detect_position_collision
        || !config.height.vmap_height_enabled
        || !config.vmap_line_of_sight_enabled
    {
        return Err(Failure::UnsupportedQuery);
    }
    let height = snapshot.binding();
    let vmap = snapshot.static_vmap().binding();
    if height.map != vmap.map
        || height.content != vmap.content
        || height.source_epoch != vmap.source_epoch
        || height.inventory != vmap.inventory
    {
        return Err(Failure::Binding);
    }
    let mut h = Sha256::new();
    h.update(b"benilla-source-near-point-primary-loaded-ground-los-v1");
    h.update(source_ground_height::geometry_revision(height));
    h.update(vmap.policy);
    h.update(config.height.effective_config);
    h.update([1, 1, 1]); // Collision detection, height and LOS all explicitly enabled.
    h.update(primary::POLICY.as_bytes());
    for hash in primary::SOURCE_SHA256 {
        h.update(hash.as_bytes());
    }
    Ok(h.finalize().into())
}

impl SourceNearPointProvider {
    pub(super) fn bind_with_same_polygon(
        session: &Session,
        profile: &Profile,
        snapshot: MapHeightSnapshot<'static>,
        config: SourceNearPointConfiguration,
        binding: super::source_route_same_polygon::SamePolygonBinding,
    ) -> Result<Self, Failure> {
        if session.empty_selector_near_point.is_some() || session.primary_near_point.is_none() {
            return Err(Failure::UnsupportedQuery);
        }
        let ground = SourceGroundHeightProvider::bind_with_same_polygon(
            session, profile, snapshot, config, binding,
        )?;
        Ok(Self { ground })
    }
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        snapshot: MapHeightSnapshot<'static>,
        config: SourceNearPointConfiguration,
    ) -> Result<Self, Failure> {
        if session.empty_selector_near_point.is_some() {
            return Err(Failure::UnsupportedQuery);
        }
        // Typed composition rechecks config and exact snapshot identity rather than
        // accepting an arbitrary replacement geometry-revision digest.
        let ground =
            SourceGroundHeightProvider::bind_with_primary(session, profile, snapshot, config)?;
        Ok(Self { ground })
    }

    pub(crate) fn resolve(
        &self,
        session: &Session,
        request: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<Facts, Failure> {
        let bound = self.ground.begin(session, request)?;
        let Query::SourceMotion(MotionQuery::NearPoint(near)) = &request.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let q = &near.request;
        let mut budget = QueryBudget::new(deadline);
        let candidate = bound.derive_initial_candidate_with_budget(1, &mut budget)?;
        let p = candidate.ground.request.position;
        let to = [p[0], p[1], candidate.ground.allowed_z];
        // The pinned visitor that could populate used areas is commented out.
        // Its fresh selector therefore accepts the original angle unconditionally.
        // Do not impose invented nearby-object membership/crowd checks here.
        if !primary::vertical_acceptance_bits(
            q.target.pose.position.z.as_f32().to_bits(),
            to[2],
            q.distance.as_f32().to_bits(),
            q.owner_bounding_radius.as_f32().to_bits(),
            q.target.bounding_radius.as_f32().to_bits(),
        )
        .map_err(|_| Failure::Invalid)?
        {
            return Err(Failure::UnsupportedNearPointFallback);
        }
        let target = q.target.pose.position;
        let from = primary::target_eye_bits(
            [target.x, target.y, target.z].map(|v| v.as_f32().to_bits()),
            q.target.collision_height.as_f32().to_bits(),
        )
        .map_err(|_| Failure::Invalid)?;
        let vmap = self.ground.static_vmap();
        let ray = RayRequest {
            snapshot: vmap.binding(),
            ticket: 2,
            from,
            to,
            // GetNearPointAt calls IsWithinLOS, whose default does NOT ignore M2.
            ignore_m2: false,
        };
        let result = vmap
            .query_with_budget(ray, &mut budget)
            .map_err(|_| Failure::MissingGeometry)?;
        if result.request != ray {
            return Err(Failure::Binding);
        }
        if !matches!(result.result, StaticRay::Miss) {
            return Err(Failure::UnsupportedNearPointFallback);
        }
        // Independent participant exclusion, required by the ground provider,
        // establishes the empty dynamic tree. Selector emptiness does not.
        let point = exact_point(to)?;
        budget.complete().map_err(|_| Failure::MissingGeometry)?;
        Ok(Facts::SourceMotion(MotionFacts::Spatial(Box::new(
            motion::Reply {
                request: motion::Query::NearPoint(q.clone()),
                fact: motion::Fact::NearPoint(point),
            },
        ))))
    }
}

pub(super) fn exact_point(bits: [u32; 3]) -> Result<Point, Failure> {
    let [x, y, z] = bits.map(f32::from_bits);
    let point = Point::new(x, y, z).map_err(|_| Failure::UnsupportedPointRepresentation)?;
    if [point.x, point.y, point.z].map(|v| v.as_f32().to_bits()) != bits {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    Ok(point)
}

#[cfg(test)]
#[path = "source_near_point/tests.rs"]
mod tests;
