// SPDX-License-Identifier: GPL-3.0-or-later
//! Unfused native IEEE adapter, preserving the existing VMap arithmetic profile.
use super::core::Arithmetic;

pub(super) struct Native;
impl Arithmetic for Native {
    type Double = f64;
    fn binary64(bits: u64) -> f64 {
        f64::from_bits(bits)
    }
    fn binary32(bits: u32) -> f64 {
        f64::from(f32::from_bits(bits))
    }
    fn integer(value: i32) -> f64 {
        f64::from(value)
    }
    fn single(value: f64) -> u32 {
        (value as f32).to_bits()
    }
    fn truncate(value: f64) -> Option<i32> {
        // Keep invalid conversion an error even if this private adapter is reused.
        (value >= f64::from(i32::MIN) && value < 2_147_483_648.0).then_some(value as i32)
    }
}
