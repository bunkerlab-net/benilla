// SPDX-License-Identifier: GPL-3.0-or-later
// GNU libstdc++ GCC 12.2 translation, copyright FSF 2009-2022;
// upstream GPL-3.0-or-later WITH GCC-exception-3.1; see SOURCE-RNG.md.
use super::{Error, Operation, RawGenerator, Rejection, SourceRandom, TraceEvent};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round,
};
const R: Round = Round::NearestTiesToEven;

/// Apply the source double-to-float conversion with software IEEE rounding; Util.cpp:116-119.
pub fn double_to_float_bits(bits: u64) -> u32 {
    let value: Single = Double::from_bits(u128::from(bits))
        .convert_r(R, &mut false)
        .value;
    value.to_bits() as u32
}

impl<E: RawGenerator, S: FnMut(TraceEvent)> SourceRandom<'_, E, S> {
    /// Preserve uint64 inclusive bounds and every scaling branch; uniform_int_dist.h:284-362.
    pub fn uniform_u64(&mut self, min: u64, max: u64) -> Result<u64, Error> {
        self.operation(Operation::UniformU64, [min, max, 0], |this| {
            if min > max {
                return Err(Error::InvalidRange);
            }
            Ok(this.integer_offset(max - min)?.wrapping_add(min))
        })
    }

    pub fn uniform_u32(&mut self, min: u32, max: u32) -> Result<u32, Error> {
        self.operation(
            Operation::UniformU32,
            [u64::from(min), u64::from(max), 0],
            |this| {
                if min > max {
                    return Err(Error::InvalidRange);
                }
                Ok(this
                    .integer_offset(u64::from(max - min))?
                    .wrapping_add(u64::from(min)) as u32)
            },
        )
    }

    pub fn uniform_i64(&mut self, min: i64, max: i64) -> Result<i64, Error> {
        self.operation(Operation::UniformI64, [min as u64, max as u64, 0], |this| {
            if min > max {
                return Err(Error::InvalidRange);
            }
            let width = (max as u64).wrapping_sub(min as u64);
            Ok(this.integer_offset(width)?.wrapping_add(min as u64) as i64)
        })
    }

    pub fn uniform_i32(&mut self, min: i32, max: i32) -> Result<i32, Error> {
        self.operation(Operation::UniformI32, [min as u64, max as u64, 0], |this| {
            if min > max {
                return Err(Error::InvalidRange);
            }
            let width = (i64::from(max) - i64::from(min)) as u64;
            Ok(this.integer_offset(width)?.wrapping_add(min as u64) as i32)
        })
    }

    fn integer_offset(&mut self, upper: u64) -> Result<u64, Error> {
        let engine_range = E::MAX - E::MIN;
        if engine_range == upper {
            return Ok(self.raw()? - E::MIN);
        }
        if engine_range > upper {
            let range = upper + 1;
            if engine_range == u64::MAX {
                let mut begin = self.cursor().raw;
                let mut product = u128::from(self.raw()?) * u128::from(range);
                let mut low = product as u64;
                if low < range {
                    let threshold = range.wrapping_neg() % range;
                    while low < threshold {
                        self.rejected(begin, Rejection::Lemire64);
                        begin = self.cursor().raw;
                        product = u128::from(self.raw()?) * u128::from(range);
                        low = product as u64;
                    }
                }
                return Ok((product >> 64) as u64);
            }
            if engine_range == u64::from(u32::MAX) {
                let range = range as u32;
                let mut begin = self.cursor().raw;
                let mut product = self.raw()?.wrapping_mul(u64::from(range));
                let mut low = product as u32;
                if low < range {
                    let threshold = range.wrapping_neg() % range;
                    while low < threshold {
                        self.rejected(begin, Rejection::Lemire32);
                        begin = self.cursor().raw;
                        product = self.raw()?.wrapping_mul(u64::from(range));
                        low = product as u32;
                    }
                }
                return Ok(product >> 32);
            }
            let scaling = engine_range / range;
            let past = range * scaling;
            loop {
                let begin = self.cursor().raw;
                let word = self.raw()? - E::MIN;
                if word < past {
                    return Ok(word / scaling);
                }
                self.rejected(begin, Rejection::Fallback);
            }
        }
        let base = engine_range + 1;
        loop {
            let begin = self.cursor().raw;
            let high = base.wrapping_mul(self.uniform_u64(0, upper / base)?);
            let value = high.wrapping_add(self.raw()? - E::MIN);
            if value <= upper && value >= high {
                return Ok(value);
            }
            self.rejected(begin, Rejection::Upscale);
        }
    }

    /// Preserve canonical word order, rounding and endpoint repair; random.tcc:3354-3385.
    fn canonical<F: Float>(&mut self) -> Result<F, Error> {
        let range = u128::from(E::MAX) - u128::from(E::MIN) + 1;
        if !range.is_power_of_two() {
            return Err(Error::UnreachedRealGenerator);
        }
        let bits = range.trailing_zeros() as usize;
        let count = F::PRECISION.div_ceil(bits).max(1);
        let radix = F::from_u128(range).value;
        let one = F::from_u128(1).value;
        let mut sum = F::ZERO;
        let mut factor = one;
        for _ in 0..count {
            let digit = F::from_u128(u128::from(self.raw()? - E::MIN)).value;
            let product = digit.mul_r(factor, R).value;
            sum = sum.add_r(product, R).value;
            factor = factor.mul_r(radix, R).value;
        }
        let value = sum.div_r(factor, R).value;
        Ok(if value >= one {
            one.next_down().value
        } else {
            value
        })
    }

    pub fn canonical_f32(&mut self) -> Result<f32, Error> {
        self.operation(Operation::CanonicalF32, [E::MIN, E::MAX, 24], |this| {
            Ok(f32::from_bits(this.canonical::<Single>()?.to_bits() as u32))
        })
    }

    pub fn canonical_f64(&mut self) -> Result<f64, Error> {
        self.operation(Operation::CanonicalF64, [E::MIN, E::MAX, 53], |this| {
            Ok(f64::from_bits(this.canonical::<Double>()?.to_bits() as u64))
        })
    }

    /// Round the subtraction, multiplication and addition separately; random.h:1864-1873.
    pub fn uniform_f32(&mut self, min: f32, max: f32) -> Result<f32, Error> {
        self.operation(
            Operation::UniformF32,
            [u64::from(min.to_bits()), u64::from(max.to_bits()), 0],
            |this| {
                let min = Single::from_bits(u128::from(min.to_bits()));
                let max = Single::from_bits(u128::from(max.to_bits()));
                let width = max.sub_r(min, R).value;
                if !min.is_finite() || !max.is_finite() || min > max || !width.is_finite() {
                    return Err(Error::InvalidRealRange);
                }
                let unit = Single::from_bits(u128::from(this.canonical_f32()?.to_bits()));
                let product = unit.mul_r(width, R).value;
                Ok(f32::from_bits(product.add_r(min, R).value.to_bits() as u32))
            },
        )
    }

    pub fn uniform_f64(&mut self, min: f64, max: f64) -> Result<f64, Error> {
        self.operation(
            Operation::UniformF64,
            [min.to_bits(), max.to_bits(), 0],
            |this| {
                let min = Double::from_bits(u128::from(min.to_bits()));
                let max = Double::from_bits(u128::from(max.to_bits()));
                let width = max.sub_r(min, R).value;
                if !min.is_finite() || !max.is_finite() || min > max || !width.is_finite() {
                    return Err(Error::InvalidRealRange);
                }
                let unit = Double::from_bits(u128::from(this.canonical_f64()?.to_bits()));
                let product = unit.mul_r(width, R).value;
                Ok(f64::from_bits(product.add_r(min, R).value.to_bits() as u64))
            },
        )
    }
}
