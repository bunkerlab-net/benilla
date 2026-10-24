// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// TargetedMovementGenerator.cpp:502, Object.cpp:1283-1307,1343-1350,1387-1392.
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later.
//! Numeric prerequisites for the source Chase straight-path attempt only.
//! The host must bind the owner's actual combat reach and collision heights,
//! then query real LOS, owner water and target water in that order. Passing
//! these prerequisites establishes neither geometry nor routing/admission.

use crate::facing::soft::S;

pub const POLICY: &str = "cmangos-8ec338a1-chase-straight-gate-soft-rne-v1";
/// Complete LF-normalized source hashes, in chase_gate_source_hashes.sha256 order.
pub const SOURCE_SHA256: [&str; 8] = [
    "eaa569e520a7b7a37d3011822e0381d41ab49b9f2dc6ffa1e43d6344bc14ff15",
    "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b",
    "259cdb1a061258d6a8c3e977c743c8c83848c763353b504ca3e617580f58c128",
    "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "52c6d22fbe9f275196a44bf4f9b39da527071e3c81f7460a4ac03a3c9107b85d",
    "accbf9ecc6f436b952d3d38ee2e506fe3d331a7367c048745334160705760c4d",
    "3a901f22b6d65edae74a93b5cd2be793ebc5c3884eb2c0aa06a37265ef53e54e",
    "acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb",
];

/// Source TerrainInfo::IsInWater requests every liquid category.
pub const LIQUID_MASK: u8 = 15;
/// Source TerrainInfo::getLiquidStatus default, not the unit's collision height.
pub const LIQUID_COLLISION_HEIGHT_BITS: u32 = 0x4002_007e; // 2.03128f
/// Inclusive input coordinate bound shared with current-pose facing.
pub const MAX_COORDINATE_BITS: u32 = 0x46ff_f000; // 32760

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericError {
    NonFinite,
    Coordinate,
    NegativeSize,
}

impl std::fmt::Display for NumericError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source Chase gate numeric domain: {self:?}")
    }
}
impl std::error::Error for NumericError {}

/// First failed numeric predicate, or permission to attempt the real LOS query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericGate {
    Distance,
    Vertical,
    Pass,
}

fn finite(bits: u32) -> Result<S, NumericError> {
    if bits & 0x7f80_0000 == 0x7f80_0000 {
        Err(NumericError::NonFinite)
    } else {
        Ok(S::bits(bits))
    }
}

fn position(bits: [u32; 3]) -> Result<[S; 3], NumericError> {
    let mut result = [S::ZERO; 3];
    for (out, bits) in result.iter_mut().zip(bits) {
        *out = finite(bits)?;
        if bits & 0x7fff_ffff > MAX_COORDINATE_BITS {
            return Err(NumericError::Coordinate);
        }
    }
    Ok(result)
}

/// Source IsWithinDist3d(goal, 200.f), followed by strict abs(owner.z-goal.z)<5.f.
/// Inputs are raw binary32; positions are finite within inclusive ±32760, and
/// owner combat reach is finite/nonnegative. Signed zero and subnormals remain.
/// Reach provenance belongs to the caller: this is not an authoritative receipt.
///
/// Software binary32 nearest/ties-even rounds every subtraction, product and
/// left-associated sum separately. There is no sqrt, FMA, target reach, radius,
/// inclusive comparison, quantization or distance-to-current-target substitution.
pub fn numeric_gate_bits(
    owner_xyz: [u32; 3],
    goal_xyz: [u32; 3],
    owner_combat_reach: u32,
) -> Result<NumericGate, NumericError> {
    let owner = position(owner_xyz)?;
    let goal = position(goal_xyz)?;
    let reach = finite(owner_combat_reach)?;
    if reach < S::ZERO {
        return Err(NumericError::NegativeSize);
    }
    let dx = owner[0] - goal[0];
    let dy = owner[1] - goal[1];
    let dz = owner[2] - goal[2];
    let distance_squared = (dx * dx + dy * dy) + dz * dz;
    let max_distance = S::bits(0x4348_0000) + reach; // 200.f + owner.GetCombatReach()
    if distance_squared >= max_distance * max_distance {
        return Ok(NumericGate::Distance);
    }
    if S::bits(dz.raw() & 0x7fff_ffff) >= S::bits(0x40a0_0000) {
        return Ok(NumericGate::Vertical);
    }
    Ok(NumericGate::Pass)
}

/// Eye endpoints for owner.IsWithinLOS(goal.x, goal.y, goal.z+target.height).
/// Return order is [owner-eye, goal-plus-target-height]. The host must request
/// the real LOS with ignore_m2=false, only after [`numeric_gate_bits`] passes.
/// Heights may be any finite signed value, as in the source model-height method.
/// Nonfinite inputs or overflowing eye sums are outside this adapter's domain.
/// Each eye addition rounds separately; input feet coordinates are never clamped.
pub fn los_endpoints_bits(
    owner_xyz: [u32; 3],
    goal_xyz: [u32; 3],
    owner_collision_height: u32,
    target_collision_height: u32,
) -> Result<[[u32; 3]; 2], NumericError> {
    let owner = position(owner_xyz)?;
    let goal = position(goal_xyz)?;
    // Source evaluates the goal's argument before entering IsWithinLOS.
    let goal_z = goal[2] + finite(target_collision_height)?;
    let owner_z = owner[2] + finite(owner_collision_height)?;
    finite(goal_z.raw())?;
    finite(owner_z.raw())?;
    Ok([
        [owner_xyz[0], owner_xyz[1], owner_z.raw()],
        [goal_xyz[0], goal_xyz[1], goal_z.raw()],
    ])
}

/// Truth conversion of a raw TerrainInfo::getLiquidStatus result, after actual
/// loaded coverage and the mask/default-height query above have been established.
/// All u8 values are accepted; every nonzero value is wet, including source
/// ABOVE_WATER (1), WATER_WALK (2), IN_WATER (4), UNDER_WATER (8), combinations
/// and unknown nonzero bytes. This is not Player::IsInWater: a player uses its
/// genuine runtime-owned cached environment bit, without a fresh terrain query.
pub const fn terrain_is_in_water(raw_status: u8) -> bool {
    raw_status != 0
}
