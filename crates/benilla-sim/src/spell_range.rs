// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Spell.cpp and Object.cpp; GPL-2.0-or-later,
// Copyright CMaNGOS contributors. See ../SPELL-RANGE.md.
//! Ordinary explicit-unit spell range with portable software binary32 RNE.
//! This numeric mechanism does not validate a spell, target identity or phase.
use rustc_apfloat::{ieee::Single, Float, Round};

pub const POLICY: &str = "cmangos-8ec338a1-spell-range-soft-rne-v1";
/// LF-normalized pinned Spell.cpp and Object.cpp, respectively.
pub const SOURCE_SHA256: [&str; 2] = [
    "ebcabd3073f188541e6a1abcc82a8d0ee6d48d360dc5ab16d9e63b3e87d75201",
    "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b",
];
const R: Round = Round::NearestTiesToEven;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NonFinite,
    Coordinate,
    Range,
    Reach,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source spell range numeric domain: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeResult {
    Within,
    TooFar,
    TooClose,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub minimum: u32,
    /// Includes the non-strict release tolerance when requested.
    pub maximum: u32,
    pub distance_squared: u32,
    pub result: RangeResult,
}

fn bounded(bits: u32, bound: u32, error: Error) -> Result<Single, Error> {
    if bits & 0x7f80_0000 == 0x7f80_0000 {
        return Err(Error::NonFinite);
    }
    if bits & 0x7fff_ffff > bound {
        return Err(error);
    }
    Ok(Single::from_bits(u128::from(bits)))
}
fn positive(bits: u32, bound: u32, error: Error) -> Result<Single, Error> {
    let value = bounded(bits, bound, error)?;
    if value < Single::ZERO {
        return Err(error);
    }
    Ok(value)
}
fn point(bits: [u32; 3]) -> Result<[Single; 3], Error> {
    Ok([
        bounded(bits[0], 0x46ff_f000, Error::Coordinate)?,
        bounded(bits[1], 0x46ff_f000, Error::Coordinate)?,
        bounded(bits[2], 0x46ff_f000, Error::Coordinate)?,
    ])
}
fn add(a: Single, b: Single) -> Single {
    a.add_r(b, R).value
}
fn mul(a: Single, b: Single) -> Single {
    a.mul_r(b, R).value
}

/// Spell::GetMinMaxRange/CheckRange for distinct unit targets and zero range flags.
/// Coordinates are inclusive ±32760, ranges 0..=1e6, reaches 0..=binary32(1e12),
/// and minimum <= maximum. Finite subnormal input and intermediate values retain
/// IEEE gradual underflow. Native typed inputs impose their existing stricter
/// normal-or-zero domain. These bounds preclude arithmetic overflow.
/// No moving-player leeway, melee/ranged flags, spellmods or instant-cast bypass.
/// Strict comparisons make both endpoints inclusive; TooFar precedes TooClose.
pub fn check_bits(
    origin: [u32; 3],
    target: [u32; 3],
    minimum: u32,
    maximum: u32,
    caster_reach: u32,
    target_reach: u32,
    strict: bool,
) -> Result<Outcome, Error> {
    let origin = point(origin)?;
    let target = point(target)?;
    let minimum = positive(minimum, 0x4974_2400, Error::Range)?;
    let maximum = positive(maximum, 0x4974_2400, Error::Range)?;
    if minimum > maximum {
        return Err(Error::Range);
    }
    let caster_reach = positive(caster_reach, 0x5368_d4a5, Error::Reach)?;
    let target_reach = positive(target_reach, 0x5368_d4a5, Error::Reach)?;
    // Preserve each source binary32 operation, with no reassociation or FMA.
    let mut minimum = add(minimum, Single::ZERO); // row minimum + meleeRange(0.f)
    let reach = add(caster_reach, target_reach);
    if minimum > Single::ZERO {
        minimum = add(minimum, reach);
    }
    let mut maximum = add(maximum, reach);
    if !strict {
        let scaled = mul(maximum, Single::from_bits(0x3dcc_cccd)); // 0.1f
        let cap = Single::from_bits(0x4040_0000); // 3.f
        maximum = add(maximum, if scaled < cap { scaled } else { cap });
    }
    let dx = origin[0].sub_r(target[0], R).value;
    let dy = origin[1].sub_r(target[1], R).value;
    let mut distance = add(mul(dx, dx), mul(dy, dy));
    let dz = origin[2].sub_r(target[2], R).value;
    distance = add(distance, mul(dz, dz));
    let result = if distance > mul(maximum, maximum) {
        RangeResult::TooFar
    } else if !minimum.is_zero() && distance < mul(minimum, minimum) {
        RangeResult::TooClose
    } else {
        RangeResult::Within
    };
    Ok(Outcome {
        minimum: minimum.to_bits() as u32,
        maximum: maximum.to_bits() as u32,
        distance_squared: distance.to_bits() as u32,
        result,
    })
}
