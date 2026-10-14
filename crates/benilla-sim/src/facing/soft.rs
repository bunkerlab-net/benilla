// SPDX-License-Identifier: GPL-3.0-or-later
//! IEEE arithmetic for the explicitly selected source profile. Every operation
//! uses software round-to-nearest/ties-even with gradual underflow. No host FPU.
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round,
};
use std::ops::{Add, Div, Mul, Neg, Sub};
const R: Round = Round::NearestTiesToEven;

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub(super) struct D(Double);
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub(super) struct S(Single);
impl D {
    pub const ZERO: Self = Self(Double::ZERO);
    pub fn one() -> Self {
        Self::bits(0x3ff0_0000_0000_0000)
    }
    pub fn bits(bits: u64) -> Self {
        Self(Double::from_bits(u128::from(bits)))
    }
    pub fn raw(self) -> u64 {
        self.0.to_bits() as u64
    }
    pub fn fma(self, other: Self, add: Self) -> Self {
        Self(self.0.mul_add_r(other.0, add.0, R).value)
    }
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
    pub fn single(self) -> S {
        let out: rustc_apfloat::StatusAnd<Single> = self.0.convert_r(R, &mut false);
        S(out.value)
    }
}
impl S {
    pub const ZERO: Self = Self(Single::ZERO);
    pub fn bits(bits: u32) -> Self {
        Self(Single::from_bits(u128::from(bits)))
    }
    pub fn raw(self) -> u32 {
        self.0.to_bits() as u32
    }
    pub fn double(self) -> D {
        let out: rustc_apfloat::StatusAnd<Double> = self.0.convert_r(R, &mut false);
        D(out.value)
    }
    pub fn fmod(self, rhs: Self) -> Self {
        Self(self.0.c_fmod(rhs.0).value)
    }
}
macro_rules! operations {
    ($t:ident) => {
        impl Add for $t {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self(self.0.add_r(rhs.0, R).value)
            }
        }
        impl Sub for $t {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self(self.0.sub_r(rhs.0, R).value)
            }
        }
        impl Mul for $t {
            type Output = Self;
            fn mul(self, rhs: Self) -> Self {
                Self(self.0.mul_r(rhs.0, R).value)
            }
        }
        impl Div for $t {
            type Output = Self;
            fn div(self, rhs: Self) -> Self {
                Self(self.0.div_r(rhs.0, R).value)
            }
        }
        impl Neg for $t {
            type Output = Self;
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }
    };
}
operations!(D);
operations!(S);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_fma_has_one_rounding_and_ordinary_product_does_not_fuse() {
        // Exact (1 + 2^-27)(1 - 2^-27) - 1 = -2^-54. Rounding the
        // intermediate product first instead yields 1, then zero.
        let x = D::bits(0x3ff0_0000_0200_0000);
        let y = D::bits(0x3fef_ffff_fc00_0000);
        assert_eq!(x.fma(y, -D::one()).raw(), 0xbc90_0000_0000_0000);
        assert_eq!((x * y - D::one()).raw(), 0);
    }

    #[test]
    fn software_profile_preserves_gradual_underflow_and_ties_even() {
        let two = S::bits(0x4000_0000);
        assert_eq!((S::bits(0x0080_0000) / two).raw(), 0x0040_0000);
        assert_eq!((S::bits(1) / two).raw(), 0);
        assert_eq!((S::bits(3) / two).raw(), 2);
        assert_eq!((-S::bits(1) / two).raw(), 0x8000_0000);
        let two = D::bits(0x4000_0000_0000_0000);
        assert_eq!(
            (D::bits(0x0010_0000_0000_0000) / two).raw(),
            0x0008_0000_0000_0000
        );
    }
}
