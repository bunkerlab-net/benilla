// SPDX-License-Identifier: GPL-3.0-or-later
// Pinned source and bounded branches: ../../PLAYER-SLOW.md.
use super::{MoveType, SlowError, SpeedRates};
use crate::creature_ai::Real;
use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};
const R: Round = Round::NearestTiesToEven;
const FLOOR: u32 = 0x3c23_d70a; // source 0.01f

fn checked(value: StatusAnd<Single>) -> Result<Single, SlowError> {
    if !(value.status & !Status::INEXACT).is_empty() {
        return Err(SlowError::Numeric("software binary32 exception"));
    }
    Ok(value.value)
}
fn out(value: Single) -> Result<Real, SlowError> {
    Real::from_bits(value.to_bits() as u32)
        .map_err(|_| SlowError::Numeric("bounded binary32 output"))
}
pub(super) fn rate(amount: i32) -> Result<Real, SlowError> {
    let one = Single::from_bits(0x3f80_0000);
    let mut speed = one;
    if amount != 0 {
        let hundred = Single::from_bits(0x42c8_0000);
        let slow = checked(Single::from_i128_r(i128::from(amount), R))?;
        let numerator = checked(hundred.add_r(slow, R))?;
        let multiplier = checked(numerator.div_r(hundred, R))?;
        speed = checked(speed.mul_r(multiplier, R))?;
    }
    // UpdateSpeed passes speed * ratio; the admitted source ratio is exactly1.
    speed = checked(speed.mul_r(one, R))?;
    out(speed.max(Single::from_bits(u128::from(FLOOR))))
}
pub(super) fn speed(rates: SpeedRates, mode: MoveType) -> Result<Real, SlowError> {
    for rate in [rates.run, rates.run_back, rates.swim] {
        if rate.bits() < FLOOR || rate.bits() > Real::ONE.bits() {
            return Err(SlowError::Invalid("unmodified player slow rate domain"));
        }
    }
    let (rate, base) = match mode {
        MoveType::Walk => (Real::ONE, 0x4020_0000),
        MoveType::Run => (rates.run, 0x40e0_0000),
        MoveType::RunBack => (rates.run_back, 0x4090_0000),
        MoveType::Swim => (rates.swim, 0x4097_1c71), // source 4.722222f
        MoveType::SwimBack => (Real::ONE, 0x4020_0000),
    };
    out(checked(
        Single::from_bits(u128::from(rate.bits())).mul_r(Single::from_bits(base), R),
    )?)
}
