// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS Project contributors; see ../../licenses/AUTHORS.md.
// Source GPL-2.0-or-later; Rust translation GPL-3.0-or-later. WITHOUT ANY WARRANTY.
//! Source-backed NPC spline progression and chase/home lifecycle. Route and near-
//! point authorities are explicit missing services, not Detour success booleans.
//! See ../../CREATURE-MOTION.md for the bounded domain and exact source branches.
use crate::creature_ai::{AiError, Point, Real};
mod numeric;
mod spline;
pub use numeric::chase_speed;
pub use spline::{LinearSpline, SplineEvent, SplineSnapshot, MAX_PATH_POINTS};

pub const CREATURE_MOTION_POLICY_ID: &str = "cmangos-8ec338a1-ground-linear-home-wander-v5";
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MotionError {
    Invalid(&'static str),
    Missing(&'static str),
    Unsupported(&'static str),
    Arithmetic(AiError),
}
impl From<AiError> for MotionError {
    fn from(error: AiError) -> Self {
        Self::Arithmetic(error)
    }
}
impl std::fmt::Display for MotionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native motion: {self:?}")
    }
}
impl std::error::Error for MotionError {}
mod pathfinder_dispatch;
pub use pathfinder_dispatch::{PathfinderConstructorKind, PathfinderDispatch};
mod pathfinder_cache;
pub use pathfinder_cache::{
    PathFinderCache, PathFinderCacheBinding, RouteCalculation, WanderNoPathReason,
    MAX_PATHFINDER_CACHE_REFS, PATHFINDER_CACHE_POINT_LIMIT,
};
mod protocol;
pub use protocol::{
    Authority, BearingRequest, CollisionRequest, CompleteRoute, Facing, FacingTargetRequest, Fact,
    Identity, NearPointAngle, NearPointRequest, Pose, Query, QueryBinding, Reply, RouteKind,
    RouteOutcome, RouteRequest, RouteSource, Target, WanderRequest,
};
mod state;
pub use state::{
    wander_normalized_word, ActiveSplineSnapshot, ChaseMode, ChaseSnapshot, DefaultMovement, Event,
    Evidence, FrameStart, Generator, GroundSettings, MotionAudit, MotionSnapshot, MotionState,
    SourceAdvance, SourceRelocation, UnitInputs, WanderDraw, WanderSnapshot, WANDER_POLICY_ID,
};
mod notify;
pub use notify::{visitor_radius, AggroNotify, AggroNotifySnapshot};

mod flags;
pub use flags::GroundMovementFlags;

#[cfg(test)]
mod pathfinder_cache_tests;
