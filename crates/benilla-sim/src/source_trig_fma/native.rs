// SPDX-License-Identifier: GPL-3.0-or-later
use super::core::Arithmetic;
pub(super) struct Native;
impl Arithmetic for Native {
    type Double = f64;
    fn double(bits: u64) -> f64 {
        f64::from_bits(bits)
    }
    fn single(bits: u32) -> f64 {
        f64::from(f32::from_bits(bits))
    }
    fn integer(value: i64) -> f64 {
        value as f64
    }
    fn truncate(value: f64) -> i32 {
        value as i32
    }
    fn narrow(value: f64) -> u32 {
        (value as f32).to_bits()
    }
    fn fma(x: f64, y: f64, z: f64) -> f64 {
        x.mul_add(y, z)
    }
}
