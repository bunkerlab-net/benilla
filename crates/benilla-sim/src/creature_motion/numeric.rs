// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS/G3D source-derived arithmetic; see ../../CREATURE-MOTION.md.
use super::{MotionError, Point, Real};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round, Status,
};
const R: Round = Round::NearestTiesToEven;

fn widen(value: Real) -> Double {
    let result: rustc_apfloat::StatusAnd<Double> = value.value().convert_r(R, &mut false);
    result.value
}
fn narrow(value: Double) -> Result<Real, MotionError> {
    let result: rustc_apfloat::StatusAnd<Single> = value.convert_r(R, &mut false);
    if !(result.status & !Status::INEXACT).is_empty() {
        return Err(MotionError::Invalid("binary32 conversion"));
    }
    Ok(Real::from_bits(result.value.to_bits() as u32)?)
}
/// Correctly rounded sqrtf of a bounded binary32 squared length, using exact
/// binary64 midpoint squares (<=50 significant bits), never a host sqrt/libm.
pub(super) fn length(a: Point, b: Point) -> Result<Real, MotionError> {
    let square = a.distance_squared(b)?;
    if square == Real::ZERO {
        return Ok(Real::ZERO);
    }
    let wide = widen(square);
    let mut lo = 0u32;
    let mut hi = 0x4b00_0000u32;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        let y = widen(Real::from_bits(mid)?);
        if y.mul_r(y, R).value <= wide {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let low = widen(Real::from_bits(lo)?);
    let high = widen(Real::from_bits(hi)?);
    let mid = low
        .add_r(high, R)
        .value
        .mul_r(Double::from_bits(0x3fe0_0000_0000_0000), R)
        .value;
    let middle_squared = mid.mul_r(mid, R).value;
    Ok(Real::from_bits(
        if wide < middle_squared || (wide == middle_squared && lo & 1 == 0) {
            lo
        } else {
            hi
        },
    )?)
}
pub(super) fn trunc_ms(value: Real) -> Result<u32, MotionError> {
    let result = value.value().to_u128_r(31, Round::TowardZero, &mut false);
    if !(result.status & !Status::INEXACT).is_empty() {
        return Err(MotionError::Invalid("spline duration overflow"));
    }
    Ok(result.value as u32)
}
pub(super) fn interpolate(a: Point, b: Point, u: Real) -> Result<Point, MotionError> {
    Ok(Point {
        x: a.x.add(b.x.sub(a.x)?.mul(u)?)?,
        y: a.y.add(b.y.sub(a.y)?.mul(u)?)?,
        z: a.z.add(b.z.sub(a.z)?.mul(u)?)?,
    })
}
/// Componentwise enclosure of every successful source-linear interpolation at a
/// representable parameter in [0, 1]. For the fixed rounded delta, multiplication
/// is monotone (possibly decreasing); addition and software-RNE rounding preserve
/// that ordering. Exact reconstruction of both endpoints bounds the composition.
/// This is numeric evidence only: it neither certifies geometry nor promises that
/// every intermediate operation satisfies Real's restricted arithmetic domain.
pub fn linear_segment_enclosure(a: Point, b: Point) -> Result<[Point; 2], MotionError> {
    a.validate()?;
    b.validate()?;
    if interpolate(a, b, Real::ZERO)? != a || interpolate(a, b, Real::ONE)? != b {
        return Err(MotionError::Unsupported("linear endpoint reconstruction"));
    }
    let ordered = |a: Real, b: Real| if a.less(b) { (a, b) } else { (b, a) };
    let (x0, x1) = ordered(a.x, b.x);
    let (y0, y1) = ordered(a.y, b.y);
    let (z0, z1) = ordered(a.z, b.z);
    Ok([
        Point {
            x: x0,
            y: y0,
            z: z0,
        },
        Point {
            x: x1,
            y: y1,
            z: z1,
        },
    ])
}

/// Creature::IsSlowedInCombat and ChaseMovementGenerator::DispatchSplineToPosition.
/// Inputs are current transactional Unit values, not an admission-time health claim.
/// Source 1.67 is binary64; all health arithmetic before it is binary32.
pub fn chase_speed(
    base: Real,
    walking: bool,
    health: u32,
    maximum: u32,
    no_wounded_slowdown: bool,
) -> Result<Real, MotionError> {
    base.nonnegative("movement speed")?;
    if base == Real::ZERO || health == 0 || maximum == 0 || health > maximum {
        return Err(MotionError::Invalid("live movement health/speed"));
    }
    if walking || no_wounded_slowdown {
        return Ok(base);
    }
    let percent = Real::integer(health)
        .mul(Real::integer(100))?
        .div(Real::integer(maximum))?;
    let thirty = Real::integer(30);
    if !percent.less(thirty) {
        return Ok(base);
    }
    let difference = widen(thirty.sub(percent)?);
    let slowdown = difference
        .mul_r(Double::from_bits(0x3ffa_b851_eb85_1eb8), R)
        .value
        .div_r(Double::from_bits(0x4059_0000_0000_0000), R)
        .value;
    let multiplier = narrow(
        Double::from_bits(0x3ff0_0000_0000_0000)
            .sub_r(slowdown, R)
            .value,
    )?;
    Ok(base.mul(multiplier)?)
}
