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
    Wander,
    /// Source clear/updateFilter/SetCurrentNavMesh before endpoint RNG.
    WanderPrepare,
}
/// Source ComputePathToRandomPoint inputs, preserving the angle then linear-radius draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WanderRequest {
    pub prepared: QueryBinding,
    pub center: Point,
    pub radius: Real,
    pub angle_draw: Real,
    pub radius_draw: Real,
}
impl WanderRequest {
    pub fn endpoint(self) -> Result<Point, MotionError> {
        self.sampled_point()
    }
    pub fn sampled_point(self) -> Result<Point, MotionError> {
        self.center.validate()?;
        self.radius.nonnegative("Wander radius")?;
        for draw in [self.angle_draw, self.radius_draw] {
            draw.nonnegative("Wander normalized draw")?;
            if !draw.less(Real::ONE) {
                return Err(MotionError::Invalid("Wander normalized draw"));
            }
        }
        // PathFinder.cpp:1367-1374: (draw * 2) * PI, linear radius, separate products/adds.
        let angle = self
            .angle_draw
            .mul(Real::constant(0x4000_0000))?
            .mul(Real::constant(0x4049_0fdb))?;
        let range = self.radius_draw.mul(self.radius)?;
        let [sin, cos] = crate::source_trig::sin_cos_radians_bits_soft(angle.bits())
            .map_err(|_| MotionError::Unsupported("Wander source trigonometry"))?;
        let point = Point {
            x: self.center.x.add(range.mul(Real::from_bits(cos)?)?)?,
            y: self.center.y.add(range.mul(Real::from_bits(sin)?)?)?,
            z: self.center.z,
        };
        point.validate()?;
        Ok(point)
    }
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
    /// Present only for the separately modeled ComputePathToRandomPoint dispatch.
    pub wander: Option<Box<WanderRequest>>,
    pub target: Option<Target>,
    pub owner_collision_height: Real,
    /// Exact bounded request cap; a full producer result beyond it is not truncated.
    pub point_limit: u16,
    /// Exact retained state of this generator before this calculation.
    pub prior_cache: Option<Box<PathFinderCache>>,
    /// Actual dispatch boundary, independent of calculation/cache success. Home
    /// owns a separate stack-local PathFinder whose constructor is not modeled here.
    #[serde(deserialize_with = "super::pathfinder_dispatch::required_option")]
    pub pathfinder_dispatch: Option<super::PathfinderDispatch>,
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
    /// Exact echoed prepare request binds the already-executed source prefix.
    WanderPrepared,
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
    fn wander_draw(&mut self, _purpose: super::WanderDraw) -> Result<u32, MotionError> {
        Err(MotionError::Missing("Wander RNG owner"))
    }
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
        || !request.accepts_route_end(path.points.last().copied())
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

impl RouteRequest {
    pub fn validate_wander(&self) -> Result<(), MotionError> {
        match (self.kind, self.wander.as_deref()) {
            (RouteKind::Wander, Some(wander))
                if self.target.is_none()
                    && self.launch_origin == self.origin
                    && wander.sampled_point()? == self.goal
                    && wander.prepared.identity == self.binding.identity
                    && wander.prepared.now_ms == self.binding.now_ms
                    && wander.prepared.query_id > 0
                    && wander.prepared.query_id.checked_add(1) == Some(self.binding.query_id) =>
            {
                Ok(())
            }
            (RouteKind::WanderPrepare, None)
                if self.target.is_none() && self.launch_origin == self.origin =>
            {
                Ok(())
            }
            (RouteKind::WanderPrepare, _) => Err(MotionError::Invalid("Wander prepare inputs")),
            (RouteKind::Wander, _) => Err(MotionError::Invalid("Wander route inputs")),
            (_, None) => Ok(()),
            (_, Some(_)) => Err(MotionError::Invalid("Wander inputs on another generator")),
        }
    }
    pub(super) fn accepts_route_end(&self, end: Option<Point>) -> bool {
        match (self.kind, end) {
            (RouteKind::Wander, Some(end)) => self.goal.x == end.x && self.goal.y == end.y,
            (_, Some(end)) => self.goal == end,
            (_, None) => false,
        }
    }
}

#[cfg(test)]
#[path = "protocol_wander_tests.rs"]
mod protocol_wander_tests;
