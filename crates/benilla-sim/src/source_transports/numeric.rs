// SPDX-License-Identifier: GPL-3.0-or-later
// Correctly rounded sqrtf, using exact integer squares and no host floating point.
use super::{finite, Error, Result};
use crate::facing::soft::S;

pub(super) fn sqrt_single(value: S) -> Result<S> {
    let bits = finite(value.raw())?.raw();
    if bits & 0x7fff_ffff == 0 {
        return Ok(value);
    }
    if bits >> 31 != 0 {
        return Err(Error::NumericDomain);
    }
    let field = ((bits >> 23) & 255) as i32;
    let mantissa = u128::from((bits & 0x7f_ffff) | if field == 0 { 0 } else { 1 << 23 });
    let exponent = if field == 0 { -149 } else { field - 127 - 23 };
    let highest = 127 - mantissa.leading_zeros() as i32;
    let mut output_exponent = (exponent + highest).div_euclid(2);
    let square = mantissa << (exponent + 46 - 2 * output_exponent);
    let mut root = square.isqrt();
    let middle_square = (2 * root + 1) * (2 * root + 1);
    if 4 * square > middle_square || (4 * square == middle_square && root & 1 != 0) {
        root += 1;
    }
    if root == 1 << 24 {
        root >>= 1;
        output_exponent += 1;
    }
    Ok(S::bits(
        ((output_exponent + 127) as u32) << 23 | (root as u32 & 0x7f_ffff),
    ))
}
