// SPDX-License-Identifier: GPL-3.0-or-later
//! Original trusted-authority boundary. It does not implement owner PathFinder,
//! GetNearPoint, VMAP LOS, liquids or cell visitors. Missing facts stay missing.
use super::{
    GroundMovementFlags, MotionError, PathFinderCache, Point, Real, RouteCalculation,
    MAX_PATH_POINTS,
};
use crate::creature_ai::movement::{Check, Revision, SegmentChecks};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pose {
    pub position: Point,
    pub orientation: Real,
}
impl Pose {
    pub fn validate(self) -> Result<(), MotionError> {
        self.position.validate()?;
        self.orientation.nonnegative("orientation")?;
        if self.orientation.greater(Real::constant(0x40c9_0fdb)) {
            return Err(MotionError::Invalid("normalized orientation"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub creature: u64,
    pub map: u32,
    pub content: Revision,
    pub geometry: Revision,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryBinding {
    pub identity: Identity,
    pub query_id: u64,
    pub now_ms: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub movement_flags: GroundMovementFlags,
    pub id: u64,
    pub pose: Pose,
    pub combat_reach: Real,
    pub bounding_radius: Real,
    pub collision_height: Real,
}
impl Target {
    pub(super) fn validate(self) -> Result<(), MotionError> {
        self.pose.validate()?;
        if self.id == 0 {
            return Err(MotionError::Invalid("target identity"));
        }
        for v in [
            self.combat_reach,
            self.bounding_radius,
            self.collision_height,
        ] {
            v.nonnegative("target bounds")?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NearPointAngle {
    /// Target.GetAngle(owner), computed from the indicated authoritative poses.
    CurrentTargetToOwner,
    Absolute(Real),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NearPointRequest {
    pub binding: QueryBinding,
    pub owner: Pose,
    pub target: Target,
    pub owner_bounding_radius: Real,
    pub distance: Real,
    pub angle: NearPointAngle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RouteKind {
    ChaseInitial,
    ChaseReplan,
    Backpedal,
    HomeForceDestination,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteRequest {
    pub binding: QueryBinding,
    pub origin: Point,
    /// Source Home computes its route from map origin, then Launch corrects the first
    /// vertex to this computed position. Segment evidence must cover that corrected path.
    pub launch_origin: Point,
    pub goal: Point,
    pub kind: RouteKind,
    pub target: Option<Target>,
    pub owner_collision_height: Real,
    /// Exact bounded request cap; a full producer result beyond it is not truncated.
    pub point_limit: u16,
    /// Exact retained state of this generator before this calculation.
    pub prior_cache: Option<Box<PathFinderCache>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RouteSource {
    /// Source owner PathFinder, including its owner filters, liquid and Z logic.
    OwnerPathFinder,
    /// Existing Detour only; does not claim the owner PathFinder branches above.
    BoundedDryDetour,
    SyntheticFixture,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteRoute {
    pub source: RouteSource,
    /// Opaque stable producer/path identity. This is a binding, not authenticity.
    pub path_identity: [u8; 32],
    pub points: Vec<Point>,
    /// One separately resolved witness for every complete path segment.
    /// A Detour polygon is never by itself evidence for all these predicates.
    pub segments: Vec<SegmentChecks>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RouteOutcome {
    Complete(CompleteRoute),
    NoPath,
    Incomplete,
    Truncated,
    Shortcut,
    Missing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Facing {
    Target(u64),
    Angle(Real),
    Tangent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BearingRequest {
    pub binding: QueryBinding,
    /// Unit::UpdateSplinePosition uses the OLD map pose for GetAngle(target),
    /// before committing the newly computed spline position.
    pub origin: Point,
    pub toward: Point,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollisionRequest {
    pub binding: QueryBinding,
    pub owner: Pose,
    pub victim: u64,
    pub radius_squared: Real,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacingTargetRequest {
    pub binding: QueryBinding,
    pub target: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Query {
    FacingTarget(FacingTargetRequest),
    NearPoint(NearPointRequest),
    Route(RouteRequest),
    Bearing(BearingRequest),
    Collider(CollisionRequest),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Fact {
    ObjectPosition(Option<Point>),
    NearPoint(Point),
    Route(RouteOutcome),
    RouteCalculation(Box<RouteCalculation>),
    Bearing(Real),
    Collider(Option<u64>),
    Missing,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub request: Query,
    pub fact: Fact,
}
/// A read-only proposal service owned by the transaction/replay adapter. Never use
/// an untrusted peer, renderer, empty query result or navmesh success as this proof.
pub trait Authority {
    fn resolve(&mut self, query: &Query) -> Result<Reply, MotionError>;
}
pub(super) fn ask(authority: &mut impl Authority, query: Query) -> Result<Fact, MotionError> {
    let reply = authority.resolve(&query)?;
    if reply.request != query {
        return Err(MotionError::Invalid("motion authority binding"));
    }
    if reply.fact == Fact::Missing {
        return Err(MotionError::Missing("motion spatial fact"));
    }
    Ok(reply.fact)
}
pub(super) fn validate_route(
    request: &RouteRequest,
    path: &CompleteRoute,
) -> Result<(), MotionError> {
    if request.point_limit as usize > MAX_PATH_POINTS
        || !(2..=request.point_limit as usize).contains(&path.points.len())
        || path.points.first() != Some(&request.origin)
        || path.points.last() != Some(&request.goal)
        || path.segments.len() + 1 != path.points.len()
        || path.path_identity == [0; 32]
    {
        return Err(MotionError::Invalid("complete route identity/points/limit"));
    }
    for point in &path.points {
        point.validate()?;
    }
    for c in &path.segments {
        let checks = [
            c.loaded_coverage,
            c.line_of_sight,
            c.body_sweep,
            c.continuous_ground_support,
            c.dry_ground,
        ];
        if checks.contains(&Check::Unknown) {
            return Err(MotionError::Missing("route spatial evidence"));
        }
        if checks.contains(&Check::Failed) {
            return Err(MotionError::Invalid("route with failed spatial evidence"));
        }
    }
    Ok(())
}
