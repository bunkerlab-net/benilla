// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Util.cpp/Util.h translation, GPL-2.0-or-later;
// Rust modifications GPL-3.0-or-later; see PROVENANCE.md.
use super::{
    double_to_float_bits, Error, EventKind, NoDraw, Operation, RawGenerator, SourceRandom,
    TraceEvent,
};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round, Status,
};
const R: Round = Round::NearestTiesToEven;

/// Clamp with std::max's NaN ordering, multiply in float, then roundf; Util.h:92-95.
pub fn chance_u(chance: f32) -> Result<u32, Error> {
    let chance = Single::from_bits(u128::from(chance.to_bits()));
    let positive = if chance > Single::ZERO {
        chance
    } else {
        Single::ZERO
    };
    let scaled = positive.mul_r(Single::from_u128(100).value, R).value;
    let rounded = scaled.round_to_integral(Round::NearestTiesToAway).value;
    let integer = rounded.to_u128_r(32, Round::TowardZero, &mut false);
    if integer
        .status
        .intersects(Status::INVALID_OP | Status::OVERFLOW)
    {
        return Err(Error::UndefinedChanceConversion);
    }
    Ok(integer.value as u32)
}

/// Preserve uint8 side count, weights and supplied-value reduction; Util.h:105-130.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Die<const SIDES: usize> {
    weights: [u32; SIDES],
    default: u8,
}
impl<const SIDES: usize> Die<SIDES> {
    pub fn new(default: u8) -> Result<Self, Error> {
        if SIDES > usize::from(u8::MAX) {
            return Err(Error::InvalidDieSides);
        }
        Ok(Self {
            weights: [0; SIDES],
            default,
        })
    }
    pub fn set(&mut self, side: u8, chance: f32) -> Result<(), Error> {
        if usize::from(side) < SIDES {
            self.weights[usize::from(side)] = chance_u(chance)?;
        }
        Ok(())
    }
    pub fn weights(&self) -> &[u32; SIDES] {
        &self.weights
    }
}

impl<E: RawGenerator, S: FnMut(TraceEvent)> SourceRandom<'_, E, S> {
    /// Use an inclusive signed distribution without an equal-bound shortcut; Util.cpp:68-72.
    pub fn irand(&mut self, min: i32, max: i32) -> Result<i32, Error> {
        self.operation(Operation::Irand, [min as u64, max as u64, 0], |this| {
            this.uniform_i32(min, max)
        })
    }
    pub fn urand(&mut self, min: u32, max: u32) -> Result<u32, Error> {
        self.operation(
            Operation::Urand,
            [u64::from(min), u64::from(max), 0],
            |this| this.uniform_u32(min, max),
        )
    }
    /// Use the actual default distribution bounds, zero through INT32_MAX; Util.cpp:86-90.
    pub fn irand_default(&mut self) -> Result<i32, Error> {
        self.operation(Operation::IrandDefault, [0; 3], |this| {
            this.uniform_i32(0, i32::MAX)
        })
    }
    pub fn urand_default(&mut self) -> Result<u32, Error> {
        self.operation(Operation::UrandDefault, [0; 3], |this| {
            this.uniform_u32(0, u32::MAX)
        })
    }
    pub fn frand(&mut self, min: f32, max: f32) -> Result<f32, Error> {
        self.operation(
            Operation::Frand,
            [u64::from(min.to_bits()), u64::from(max.to_bits()), 0],
            |this| this.uniform_f32(min, max),
        )
    }
    pub fn rand_norm(&mut self) -> Result<f64, Error> {
        self.operation(Operation::RandNorm, [0; 3], |this| {
            this.uniform_f64(0.0, 1.0)
        })
    }
    pub fn rand_norm_f(&mut self) -> Result<f32, Error> {
        self.operation(Operation::RandNormF, [0; 3], |this| {
            this.uniform_f32(0.0, 1.0)
        })
    }
    pub fn rand_chance(&mut self) -> Result<f64, Error> {
        self.operation(Operation::RandChance, [0; 3], |this| {
            this.uniform_f64(0.0, 100.0)
        })
    }
    /// Draw a double distribution before narrowing, not a float distribution; Util.cpp:116-119.
    pub fn rand_chance_f(&mut self) -> Result<f32, Error> {
        self.operation(Operation::RandChanceF, [0; 3], |this| {
            Ok(f32::from_bits(double_to_float_bits(
                this.uniform_f64(0.0, 100.0)?.to_bits(),
            )))
        })
    }
    /// Compare the widened float chance with the double roll, even for NaN; Util.h:80-84.
    pub fn roll_chance_f(&mut self, chance: f32) -> Result<bool, Error> {
        self.operation(
            Operation::RollChanceF,
            [u64::from(chance.to_bits()), 0, 0],
            |this| {
                let chance: Double = Single::from_bits(u128::from(chance.to_bits()))
                    .convert_r(R, &mut false)
                    .value;
                let roll = Double::from_bits(u128::from(this.rand_chance()?.to_bits()));
                Ok(chance > roll)
            },
        )
    }
    pub fn roll_chance_i(&mut self, chance: i32) -> Result<bool, Error> {
        self.operation(Operation::RollChanceI, [chance as u64, 0, 0], |this| {
            Ok(chance > this.irand(0, 99)?)
        })
    }
    pub fn chance_u(&mut self, chance: f32) -> Result<u32, Error> {
        self.operation(
            Operation::ChanceU,
            [u64::from(chance.to_bits()), 0, 0],
            |_| chance_u(chance),
        )
    }
    /// Skip the engine only when the premultiplied chance is zero; Util.h:98-103.
    pub fn roll_chance_combat(&mut self, chance: f32) -> Result<bool, Error> {
        self.operation(
            Operation::RollChanceCombat,
            [u64::from(chance.to_bits()), 0, 0],
            |this| {
                let scaled = this.chance_u(chance)?;
                if scaled == 0 {
                    this.no_draw(NoDraw::CombatRoundedZero);
                    return Ok(false);
                }
                Ok(scaled > this.urand(1, 10000)?)
            },
        )
    }
    /// Reduce a supplied value without taking another raw word; Util.h:111-123.
    pub fn die_roll<const SIDES: usize>(
        &mut self,
        die: &Die<SIDES>,
        supplied: u32,
    ) -> Result<u8, Error> {
        self.operation(
            Operation::DieRoll,
            [u64::from(supplied), SIDES as u64, u64::from(die.default)],
            |this| {
                this.no_draw(NoDraw::SuppliedDie);
                let mut cumulative = 0_u32;
                for (side, &weight) in die.weights.iter().enumerate() {
                    if weight != 0 {
                        cumulative = cumulative.wrapping_add(weight);
                        this.emit(EventKind::DieSide {
                            span: this.span(),
                            side: side as u8,
                            weight,
                            cumulative,
                        });
                        if supplied <= cumulative {
                            return Ok(side as u8);
                        }
                    }
                }
                Ok(die.default)
            },
        )
    }
    /// Select an index through urand, including a one-value draw; Util.h:133-137.
    pub fn pick_random_value<'v, T>(&mut self, values: &'v [T]) -> Result<&'v T, Error> {
        let index = self.operation(
            Operation::PickRandomValue,
            [values.len() as u64, 0, 0],
            |this| {
                if values.is_empty() {
                    return Err(Error::EmptySelection);
                }
                this.urand(0, (values.len() - 1) as u32)
            },
        )?;
        Ok(&values[index as usize])
    }
}
