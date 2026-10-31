// SPDX-License-Identifier: GPL-3.0-or-later
//! Retained PathFinder polygon state, not smoothing scratch or loaded authority.
//! CMaNGOS 8ec338a1 PathFinder.cpp:306–380, 510–789 and PathFinder.h::clear.
use super::{
    Identity, MotionError, Point, Real, RouteKind, RouteOutcome, RouteRequest, RouteSource,
};
use serde::{Deserialize, Serialize};

/// Ordinary Chase does not call setPathLengthLimit. Dungeon growth is unrepresented.
pub const PATHFINDER_CACHE_POINT_LIMIT: u16 = 74;
/// Operational memory bound only: an oversized proposal fails without truncation.
pub const MAX_PATHFINDER_CACHE_REFS: usize = 4096;

/// Equality/provenance binding only. Serialized refs cannot rehydrate loaded authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathFinderCacheBinding {
    pub identity: Identity,
    /// Supplied and checked by the native owner adapter; simulation has no generation.
    pub owner_generation: u64,
    pub inventory: [u8; 32],
    pub source_epoch: [u8; 32],
    pub navigation_policy: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathFinderCache {
    pub binding: PathFinderCacheBinding,
    /// Operational calculation sequence, not a source PathFinder field.
    pub revision: u64,
    pub source: RouteSource,
    /// Complete m_pathPolyRefs backing storage, including inactive slots and tail.
    /// Source clear changes active_len only; smoothing uses separate scratch refs.
    pub poly_refs: Vec<u64>,
    pub active_len: u16,
    pub point_limit: u16,
    pub path_type: u32,
    pub requested_end: Point,
    pub include_flags: u16,
    pub exclude_flags: u16,
}
impl PathFinderCache {
    pub fn validate(&self) -> Result<(), MotionError> {
        let identity = self.binding.identity;
        if identity.creature == 0
            || identity.content.0 == [0; 32]
            || identity.geometry.0 == [0; 32]
            || [
                self.binding.inventory,
                self.binding.source_epoch,
                self.binding.navigation_policy,
            ]
            .contains(&[0; 32])
            || self.revision == 0
            || !matches!(
                self.source,
                RouteSource::OwnerPathFinder | RouteSource::SyntheticFixture
            )
            || self.point_limit != PATHFINDER_CACHE_POINT_LIMIT
            || self.poly_refs.len() < usize::from(self.point_limit)
            || self.poly_refs.len() > MAX_PATHFINDER_CACHE_REFS
            || self.active_len > self.point_limit
            || self.path_type & !0x3f != 0
            // Initial failed Wander caches are admitted contextually by the
            // exact calculation reason; saved Chase still requires NORMAL.
            || (self.source == RouteSource::OwnerPathFinder && self.revision == 1
                && !(self.path_type == 1 && self.active_len > 0)
                && !(self.path_type == 8 && self.active_len == 0
                    && (self.poly_refs.len() == usize::from(self.point_limit)
                        || (self.poly_refs.len() == usize::from(self.point_limit) + 1
                            && self.poly_refs.last() != Some(&0)))
                    && self.poly_refs[..usize::from(self.point_limit)].iter().all(|reference|*reference == 0)))
            // Shape only: the host binds the exact 1/13 choice to its capability owner.
            || !matches!(self.include_flags, 1 | 13)
            || self.exclude_flags != 0
        {
            return Err(MotionError::Invalid("retained pathfinder cache domain"));
        }
        self.requested_end.validate()?;
        Ok(())
    }
    pub fn validate_for(&self, identity: Identity) -> Result<(), MotionError> {
        self.validate()?;
        if self.binding.identity != identity {
            return Err(MotionError::Invalid("retained pathfinder cache identity"));
        }
        Ok(())
    }
}

/// Exact ordinary ComputePathToRandomPoint return branch. Other failures remain
/// outside the supported native domain, not converted into retryable NoPath.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum WanderNoPathReason {
    MinimumDistance,
    EndpointMissing,
    EndpointHeight,
}

/// A completed calculation carries state even when no spline can be launched.
/// Missing/unsupported calculations never commit a partial candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteCalculation {
    pub wander_no_path: Option<WanderNoPathReason>,
    pub source: RouteSource,
    pub outcome: RouteOutcome,
    pub prior_cache: Option<Box<PathFinderCache>>,
    pub after_cache: PathFinderCache,
}
impl RouteCalculation {
    pub fn validate_for(&self, request: &RouteRequest) -> Result<(), MotionError> {
        request.validate_wander()?;
        if request.kind == RouteKind::HomeForceDestination {
            request.validate_pathfinder_dispatch()?;
            if self.after_cache.revision != 1 || !matches!(self.outcome, RouteOutcome::Complete(_))
            {
                return Err(MotionError::Invalid("Home local calculation terminal"));
            }
        }
        if self.prior_cache != request.prior_cache {
            return Err(MotionError::Invalid("pathfinder calculation prior state"));
        }
        if self.source == RouteSource::OwnerPathFinder {
            request.validate_source_cache_dispatch()?;
        }
        self.after_cache.validate_for(request.binding.identity)?;
        let wander_no_path =
            request.kind == RouteKind::Wander && self.outcome == RouteOutcome::NoPath;
        if wander_no_path != self.wander_no_path.is_some() {
            return Err(MotionError::Invalid("Wander no-path reason presence"));
        }
        if let Some(reason) = self.wander_no_path {
            let old_end = request.prior_cache.as_ref().map_or(
                Point {
                    x: Real::ZERO,
                    y: Real::ZERO,
                    z: Real::ZERO,
                },
                |cache| cache.requested_end,
            );
            let cold = vec![0; usize::from(request.point_limit)];
            let old_refs = request
                .prior_cache
                .as_ref()
                .map_or(cold.as_slice(), |cache| cache.poly_refs.as_slice());
            let after_refs = self.after_cache.poly_refs.as_slice();
            let near_zero = request
                .origin
                .distance_squared(request.goal)?
                .less(Real::constant(0x3c23_d70a));
            let exact = match reason {
                WanderNoPathReason::MinimumDistance => {
                    near_zero && self.after_cache.requested_end == old_end && after_refs == old_refs
                }
                WanderNoPathReason::EndpointMissing => {
                    !near_zero
                        && self.after_cache.requested_end == request.goal
                        && after_refs == old_refs
                }
                WanderNoPathReason::EndpointHeight => {
                    !near_zero
                        && self.after_cache.requested_end.x == request.goal.x
                        && self.after_cache.requested_end.y == request.goal.y
                        && ((after_refs == old_refs
                            && old_refs.iter().any(|reference| *reference != 0))
                            || (after_refs.len() == old_refs.len() + 1
                                && &after_refs[..old_refs.len()] == old_refs
                                && after_refs.last() != Some(&0)))
                }
            };
            if !exact || self.after_cache.path_type != 8 || self.after_cache.active_len != 0 {
                return Err(MotionError::Invalid("Wander no-path source transition"));
            }
        }
        if self.source != self.after_cache.source
            || (!wander_no_path && !request.accepts_route_end(Some(self.after_cache.requested_end)))
            || self.after_cache.point_limit != request.point_limit
        {
            return Err(MotionError::Invalid("pathfinder calculation binding"));
        }
        match &request.prior_cache {
            Some(before) => {
                before.validate_for(request.binding.identity)?;
                if before.binding != self.after_cache.binding
                    || before.source != self.source
                    || before.point_limit != self.after_cache.point_limit
                    || before.include_flags != self.after_cache.include_flags
                    || before.exclude_flags != self.after_cache.exclude_flags
                    || before.revision.checked_add(1) != Some(self.after_cache.revision)
                    // No clear/replacement/cut branch shrinks the backing vector.
                    || self.after_cache.poly_refs.len() < before.poly_refs.len()
                {
                    return Err(MotionError::Invalid("pathfinder calculation transition"));
                }
            }
            None if self.source == RouteSource::SyntheticFixture
                && self.after_cache.revision == 1 => {}
            None if self.source == RouteSource::OwnerPathFinder
                && self.after_cache.revision == 1
                && (matches!(self.outcome, RouteOutcome::Complete(_)) || wander_no_path) => {}
            None => {
                return Err(MotionError::Invalid("initial pathfinder calculation"));
            }
        }
        match &self.outcome {
            RouteOutcome::Complete(path)
                if path.source != self.source
                    || self.after_cache.path_type != 1
                    || self.after_cache.active_len == 0 =>
            {
                Err(MotionError::Invalid("pathfinder complete outcome/source"))
            }
            RouteOutcome::Complete(path) => {
                if path.points.last() != Some(&self.after_cache.requested_end) {
                    return Err(MotionError::Invalid("pathfinder actual endpoint"));
                }
                super::protocol::validate_route(request, path)
            }
            RouteOutcome::NoPath if self.after_cache.path_type != 8 => {
                Err(MotionError::Invalid("pathfinder no-path outcome/type"))
            }
            _ => Ok(()),
        }
    }
}
