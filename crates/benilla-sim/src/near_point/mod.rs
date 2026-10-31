// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic Object.cpp/GridDefines.h, Copyright CMaNGOS contributors,
// GPL-2.0-or-later, revision 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
//! WorldObject::GetNearPoint2dAt candidates under separate bounded numeric profiles.
//! No collision search, radii, height, LOS, visibility, PathFinder or admission.
//! The caller supplies the actual raw radians, never an angle inferred from Real.
use crate::{facing::soft::S, source_trig};

pub const POLICY: &str = "cmangos-8ec338a1-initial-near-point-glibc-2.41-soft-rne-v1";
pub const FALLBACK_POLICY: &str = "cmangos-8ec338a1-fallback-near-point-glibc-2.41-soft-rne-v1";
/// LF-normalized pinned Object.cpp and GridDefines.h, respectively.
pub const SOURCE_SHA256: [&str; 2] = [
    "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b",
    "5f5413657f84c5f5964f2a69d342874d8d1f7f5ed2bab969e1d78b0650141d82",
];
/// Inclusive native input coordinate bound, shared with current-pose facing.
pub const MAX_COORDINATE_BITS: u32 = 0x46ff_f000; // 32760
/// Source MAP_HALFSIZE - 0.5, exactly binary32 17066.166015625.
pub const MAP_CLAMP_BITS: u32 = 0x4685_5455;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NonFinite,
    Coordinate,
    Distance,
    Angle,
    Trig(source_trig::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source near-point numeric domain: {self:?}")
    }
}
impl std::error::Error for Error {}

fn finite(bits: u32) -> Result<S, Error> {
    if bits & 0x7f80_0000 == 0x7f80_0000 {
        Err(Error::NonFinite)
    } else {
        Ok(S::bits(bits))
    }
}
fn coordinate(bits: u32) -> Result<S, Error> {
    let value = finite(bits)?;
    if bits & 0x7fff_ffff > MAX_COORDINATE_BITS {
        Err(Error::Coordinate)
    } else {
        Ok(value)
    }
}
fn normalize_coordinate(value: S) -> S {
    let limit = S::bits(MAP_CLAMP_BITS);
    // GridDefines.h::NormalizeMapCoord uses strict comparisons; zero is unchanged.
    if value > limit {
        limit
    } else if value < -limit {
        -limit
    } else {
        value
    }
}

/// Initial near-point XY candidate from raw target XY, distance and angle bits.
/// Coordinates must be finite and inside inclusive ±32760. Distance
/// accepts every finite nonnegative binary32 value; angle accepts [0, binary32
/// TAU]. All three inputs retain signed zero and subnormals. Validation precedes
/// computation even for zero distance. The caller must supply the actual angle.
///
/// Pinned source trig is software RNE. Each binary32 distance product rounds
/// before its separate coordinate addition; no FMA is used. Source clamping
/// to ±17066.166015625 follows both additions. Input coordinates are never
/// preclamped. This is only a candidate, with no spatial guarantee.
pub fn candidate_bits(target_xy: [u32; 2], distance: u32, angle: u32) -> Result<[u32; 2], Error> {
    let target = [coordinate(target_xy[0])?, coordinate(target_xy[1])?];
    let distance = finite(distance)?;
    if distance < S::ZERO {
        return Err(Error::Distance);
    }
    let angle_value = finite(angle)?;
    if angle_value < S::ZERO || angle & 0x7fff_ffff > source_trig::TAU_BITS {
        return Err(Error::Angle);
    }
    let [sin, cos] = source_trig::sin_cos_radians_bits_soft(angle).map_err(Error::Trig)?;
    Ok(candidate_from_trig(target, distance, [sin, cos]))
}

/// Fallback near-point XY candidate from the already-rounded raw source angle.
/// XY and distance have exactly the same domains as [`candidate_bits`]. The
/// finite angle domain is [-binary32 PI, RNE32(binary32 TAU + binary32 PI)].
/// The caller must supply source RNE32(base-angle + selector-offset) directly;
/// there is no modulo, normalization, clamping or angle reconstruction here.
/// This numeric helper does not select an offset or implement fallback search.
pub fn fallback_candidate_bits(
    target_xy: [u32; 2],
    distance: u32,
    angle: u32,
) -> Result<[u32; 2], Error> {
    let target = [coordinate(target_xy[0])?, coordinate(target_xy[1])?];
    let distance = finite(distance)?;
    if distance < S::ZERO {
        return Err(Error::Distance);
    }
    let angle_value = finite(angle)?;
    if angle_value < S::bits(source_trig::FALLBACK_MIN_ANGLE_BITS)
        || angle_value > S::bits(source_trig::FALLBACK_MAX_ANGLE_BITS)
    {
        return Err(Error::Angle);
    }
    let trig = source_trig::sin_cos_fallback_radians_bits_soft(angle).map_err(Error::Trig)?;
    Ok(candidate_from_trig(target, distance, trig))
}

fn candidate_from_trig(target: [S; 2], distance: S, [sin, cos]: [u32; 2]) -> [u32; 2] {
    // Object.cpp::GetNearPoint2dAt: x = posX + (distance2d * cosf(absAngle)).
    let dx = distance * S::bits(cos);
    let x = target[0] + dx;
    let dy = distance * S::bits(sin);
    let y = target[1] + dy;
    [normalize_coordinate(x).raw(), normalize_coordinate(y).raw()]
}

/// Source original-candidate acceptance arithmetic; actual geometry is separate.
pub mod primary;

/// Bounded source selector arithmetic; actual candidate geometry is separate.
pub mod selector;
