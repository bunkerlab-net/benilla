// SPDX-License-Identifier: GPL-3.0-or-later
//! Original authority adapter contract around native chase/home intents. This is NOT
//! CMaNGOS PathFinder or a substitute for collision/navigation data. See CREATURE-AI.md.
use super::{AiError, Point, Real};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision(pub [u8; 32]);
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MovementPurpose {
    Chase { target: u64 },
    Home,
}

/// A trusted adapter resolves a source-style near-point before issuing a chase segment.
/// `elapsed_ms` is the simulation movement budget, never an arbitrary peer timestamp.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementRequest {
    pub request_id: u64,
    pub command_id: u64,
    pub creature_id: u64,
    pub map_id: u32,
    pub origin: Point,
    pub goal: Point,
    pub speed: Real,
    pub elapsed_ms: u32,
    pub content_revision: Revision,
    pub geometry_revision: Revision,
    pub purpose: MovementPurpose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Check {
    Passed,
    Failed,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentChecks {
    pub loaded_coverage: Check,
    pub line_of_sight: Check,
    pub body_sweep: Check,
    pub continuous_ground_support: Check,
    pub dry_ground: Check,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum BlockReason {
    Obstacle,
    UnsupportedGround,
    Water,
    MissingGeometry,
    UnsupportedNavigation,
    WrongMap,
    StaleGeometry,
    StaleContent,
    StaleRequest,
}
/// Opaque proof emitted only by the source motion kernel after a completed
/// transactional update. It cannot be constructed, serialized or deserialized
/// by a geometry adapter, network peer or checkpoint loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMotion {
    pub(crate) position: Point,
    pub(crate) relocated: bool,
    pub(crate) home_arrived: bool,
    pub(crate) reachable: Option<bool>,
    pub(crate) moving_and_running: bool,
    pub(crate) last_move_ms: u64,
    pub(crate) now_ms: u64,
    pub(crate) stationary_melee_refresh_eligible: bool,
    pub(crate) target_moving_and_running: bool,
}
impl SourceMotion {
    pub fn position(&self) -> Point {
        self.position
    }
    pub fn relocated(&self) -> bool {
        self.relocated
    }
    pub fn home_arrived(&self) -> bool {
        self.home_arrived
    }
    pub fn reachable(&self) -> Option<bool> {
        self.reachable
    }
    pub fn moving_and_running(&self) -> bool {
        self.moving_and_running
    }
    pub fn last_move_ms(&self) -> u64 {
        self.last_move_ms
    }
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MovementResult {
    /// Internal completed source-spline update; never an Authority outcome.
    #[serde(skip)]
    SourceSpline(SourceMotion),
    /// Position is computed by the authority's continuous movement solver. `travelled`
    /// is total path length this interval, including every bend/height adjustment.
    Progress {
        position: Point,
        travelled: Real,
        arrived: bool,
        checks: SegmentChecks,
    },
    /// Known obstruction is different from missing/unsupported geometry.
    Blocked(BlockReason),
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementOutcome {
    pub request_id: u64,
    pub map_id: u32,
    pub content_revision: Revision,
    pub geometry_revision: Revision,
    pub result: MovementResult,
}

/// Only a trusted local/replicated authority implements this. PSK transport authentication
/// alone does not validate claimed positions, path proofs, damage, LOS or terrain coverage.
/// Resolve is a proposal: do not mutate the world until its AI step is committed.
pub trait GeometryAuthority {
    fn resolve(&mut self, request: &MovementRequest) -> Result<MovementOutcome, AiError>;
}

/// No Deserialize implementation and no public field constructor. The validator checks
/// budgets and binding; the authority is responsible for truthful geometric checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedMovement {
    pub(crate) request: MovementRequest,
    pub(crate) result: MovementResult,
}
impl ValidatedMovement {
    pub fn request(&self) -> &MovementRequest {
        &self.request
    }
    pub fn result(&self) -> &MovementResult {
        &self.result
    }
}

pub fn resolve_movement(
    authority: &mut impl GeometryAuthority,
    request: MovementRequest,
) -> Result<ValidatedMovement, AiError> {
    request.origin.validate()?;
    request.goal.validate()?;
    request.speed.nonnegative("movement speed")?;
    if request.request_id == 0
        || request.command_id == 0
        || request.creature_id == 0
        || request.elapsed_ms == 0
        || request.elapsed_ms > 60_000
        || request.speed == Real::ZERO
    {
        return Err(AiError::Invalid("movement request"));
    }
    let outcome = authority.resolve(&request)?;
    if matches!(outcome.result, MovementResult::SourceSpline(_)) {
        return Err(AiError::Invalid("external source motion token"));
    }
    if outcome.request_id != request.request_id {
        return Err(AiError::Movement(BlockReason::StaleRequest));
    }
    if outcome.map_id != request.map_id {
        return Err(AiError::Movement(BlockReason::WrongMap));
    }
    if outcome.content_revision != request.content_revision {
        return Err(AiError::Movement(BlockReason::StaleContent));
    }
    if outcome.geometry_revision != request.geometry_revision {
        return Err(AiError::Movement(BlockReason::StaleGeometry));
    }
    if let MovementResult::Progress {
        position,
        travelled,
        arrived,
        checks,
    } = &outcome.result
    {
        position.validate()?;
        travelled.nonnegative("travelled distance")?;
        let checks = [
            checks.loaded_coverage,
            checks.line_of_sight,
            checks.body_sweep,
            checks.continuous_ground_support,
            checks.dry_ground,
        ];
        if checks.contains(&Check::Unknown) {
            return Err(AiError::Movement(BlockReason::MissingGeometry));
        }
        if checks.contains(&Check::Failed) {
            return Err(AiError::Invalid("progress without passed geometric checks"));
        }
        let budget = request
            .speed
            .mul(Real::integer(request.elapsed_ms))?
            .div(Real::integer(1000))?;
        if travelled.greater(budget)
            || request
                .origin
                .square_distance(*position, true)?
                .greater(travelled.mul(*travelled)?)
        {
            return Err(AiError::Invalid(
                "movement exceeds native speed/path-length budget",
            ));
        }
        if *arrived && *position != request.goal {
            return Err(AiError::Invalid("arrival away from goal"));
        }
        if *position == request.origin && request.goal != request.origin && *travelled == Real::ZERO
        {
            return Err(AiError::Invalid("stationary progress on nonempty movement"));
        }
    }
    Ok(ValidatedMovement {
        request,
        result: outcome.result,
    })
}
