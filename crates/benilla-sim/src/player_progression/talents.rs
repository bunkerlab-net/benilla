// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Player.cpp:2771/2803/19284; World.cpp:456/2420.
// Source GPL-2.0-or-later; this bounded translation GPL-3.0-or-later.
//! Unspent UpdateFreeTalentPoints: source binary32 multiply then uint32 truncate.
use super::{Error, Result, MAX_SUPPORTED_LEVEL};
use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};

pub(super) const POLICY_ID: &str = "cmangos-8ec338a1-unspent-talent-rate-binary32-v1";
fn checked<T>(v: StatusAnd<T>) -> Result<T> {
    if !(v.status & !Status::INEXACT).is_empty() {
        return Err(Error::Numeric("talent binary32 conversion"));
    }
    Ok(v.value)
}
pub(super) fn validate_rate(bits: u32) -> Result<()> {
    let magnitude = bits & 0x7fff_ffff;
    // setConfigPos permits zero, including -0. Other negative rates would have
    // been replaced by the source config loader; callers supply its result.
    // Nonfinite/subnormal inputs are this fork's fail-closed numeric domain.
    if magnitude >= 0x7f80_0000
        || (magnitude != 0 && magnitude < 0x0080_0000)
        || (bits & 0x8000_0000 != 0 && magnitude != 0)
    {
        return Err(Error::Numeric("resolved Rate.Talent"));
    }
    Ok(())
}
pub(super) fn points(level: u32, bits: u32) -> Result<u32> {
    validate_rate(bits)?;
    if !(1..=MAX_SUPPORTED_LEVEL).contains(&level) {
        return Err(Error::Unsupported("mage talent level outside 1–60"));
    }
    let base = checked(Single::from_u128_r(
        level.saturating_sub(9).into(),
        Round::NearestTiesToEven,
    ))?;
    let product = checked(base.mul_r(Single::from_bits(bits.into()), Round::NearestTiesToEven))?;
    u32::try_from(checked(product.to_u128_r(
        32,
        Round::TowardZero,
        &mut false,
    ))?)
    .map_err(|_| Error::Numeric("talent uint32 conversion"))
}
