// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic Object.cpp/MapManager.h, Copyright CMaNGOS contributors,
// GPL-2.0-or-later, revision 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
//! Bounded, source-profile numeric bearing and arc comparisons. No map, LOS,
//! visibility, object identity inference, or admission. See ../../FACING.md.
mod atan2;
mod soft;
use soft::S;

pub const POLICY: &str = "cmangos-8ec338a1-facing-glibc-2.41-soft-rne-v1";
/// LF-normalized pinned e_atan2f.c, Object.cpp, MapManager.h, respectively.
pub const SOURCE_SHA256: [&str; 3] = [
    "96f9c81b6e870c256cc0757f6d88f5290ed35db8d5b640b9e757d6feca96ae38",
    "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b",
    "bb5ff0741f5c78a295a96d708a0875da5881547386aea63ecfda9d30f66f05ad",
];
pub const PI_BITS: u32 = 0x4049_0fdb;
const TAU_BITS: u32 = 0x40c9_0fdb;
const MAX_COORDINATE_BITS: u32 = 0x46ff_f000; // 32760
const MAX_ANGLE_BITS: u32 = 0x41c9_0fdb; // float(4 * TAU)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NonFinite,
    Coordinate,
    Angle,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source facing numeric domain: {self:?}")
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
fn point(words: [u32; 2]) -> Result<[S; 2], Error> {
    let values = [finite(words[0])?, finite(words[1])?];
    if words
        .iter()
        .any(|bits| bits & 0x7fff_ffff > MAX_COORDINATE_BITS)
    {
        return Err(Error::Coordinate);
    }
    Ok(values)
}
fn angle(bits: u32) -> Result<S, Error> {
    let value = finite(bits)?;
    if bits & 0x7fff_ffff > MAX_ANGLE_BITS {
        return Err(Error::Angle);
    }
    Ok(value)
}

/// Pinned glibc/CORE-MATH atan2f with software IEEE RNE, preserving signed zero
/// and subnormals. This raw helper accepts all finite binary32 inputs.
pub fn atan2_bits(y: u32, x: u32) -> Result<u32, Error> {
    Ok(atan2::atan2(finite(y)?, finite(x)?).raw())
}

fn bearing(origin: [S; 2], toward: [S; 2]) -> S {
    let dx = toward[0] - origin[0];
    let dy = toward[1] - origin[1];
    let a = atan2::atan2(dy, dx);
    if a >= S::ZERO {
        a
    } else {
        S::bits(TAU_BITS) + a
    }
}

/// WorldObject::GetAngleAt. Coordinates are inclusive ±32760, native XY.
/// Coincident coordinates do not imply object identity. Negative zero is kept.
pub fn bearing_bits(origin: [u32; 2], toward: [u32; 2]) -> Result<u32, Error> {
    Ok(bearing(point(origin)?, point(toward)?).raw())
}
fn normalize(value: S) -> S {
    let tau = S::bits(TAU_BITS);
    if value < S::ZERO {
        -((-value).fmod(tau)) + tau
    } else {
        value.fmod(tau)
    }
}
/// MapManager::NormalizeOrientation in the bounded ±float(4*TAU) domain.
/// Positive whole turns become zero; negative whole turns become TAU.
pub fn normalize_bits(bits: u32) -> Result<u32, Error> {
    Ok(normalize(angle(bits)?).raw())
}

/// WorldObject::HasInArcAt with source inclusive borders. The caller supplies
/// actual identity equality; coordinate equality never takes the self shortcut.
/// Domain validation precedes that shortcut, including otherwise unused fields.
pub fn has_in_arc_bits(
    origin: [u32; 2],
    orientation: u32,
    target: [u32; 2],
    arc: u32,
    same_object: bool,
) -> Result<bool, Error> {
    let origin = point(origin)?;
    let target = point(target)?;
    let orientation = angle(orientation)?;
    let arc = angle(arc)?;
    if same_object {
        return Ok(true);
    }
    let arc = normalize(arc);
    let mut a = normalize(bearing(origin, target) - orientation);
    if a > S::bits(PI_BITS) {
        a = a - S::bits(TAU_BITS);
    }
    let border = arc / S::bits(0x4000_0000);
    Ok(a >= -border && a <= border)
}
