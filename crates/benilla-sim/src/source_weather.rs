// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Weather.cpp translation, GPL-2.0-or-later;
// Rust modifications GPL-3.0-or-later; see PROVENANCE.md.
//! Weather arithmetic and packet payloads, without a clock or private RNG.
use crate::source_rng::{self, RawGenerator, SourceRandom, TraceEvent};

pub const FINE: u32 = 0;
pub const RAIN: u32 = 1;
pub const SNOW: u32 = 2;
pub const STORM: u32 = 3;

/// Correct individual operands, never their sum; Weather.cpp:458-480.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chances(pub [[u32; 3]; 4]);
impl Chances {
    pub fn corrected(mut values: [[u32; 3]; 4]) -> Self {
        for season in &mut values {
            for value in season {
                if *value > 100 {
                    *value = 25;
                }
            }
        }
        Self(values)
    }
}

/// Consume the dispatching thread's F3 helpers, not a weather-owned engine.
pub trait Random {
    type Error;
    fn urand(&mut self, low: u32, high: u32) -> Result<u32, Self::Error>;
    fn rand_norm_f(&mut self) -> Result<f32, Self::Error>;
}
impl<E: RawGenerator, S: FnMut(TraceEvent)> Random for SourceRandom<'_, E, S> {
    type Error = source_rng::Error;
    fn urand(&mut self, low: u32, high: u32) -> Result<u32, Self::Error> {
        SourceRandom::urand(self, low, high)
    }
    fn rand_norm_f(&mut self) -> Result<f32, Self::Error> {
        SourceRandom::rand_norm_f(self)
    }
}
/// Read the World game-time snapshot only after the first roll; Weather.cpp:103-112.
pub trait Context: Random {
    fn year_day(&mut self) -> Result<u32, Self::Error>;
}
/// All valid localtime day indices, including leap day; Weather.cpp:112.
pub fn season(year_day: u32) -> usize {
    (((year_day + 287) / 91) % 4) as usize
}

/// Store binary32 bits without JSON float conversion; Weather.h:94-99.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Weather {
    pub kind: u32,
    pub grade_bits: u32,
    pub permanent: bool,
}
impl Weather {
    pub fn grade(self) -> f32 {
        f32::from_bits(self.grade_bits)
    }
    fn grade_mut(&mut self, grade: f32) {
        self.grade_bits = grade.to_bits();
    }
    /// Preserve negative/one boundary replacements and NaN comparisons; Weather.cpp:291-296.
    pub fn normalize(&mut self) {
        let grade = self.grade();
        if grade >= 1.0 {
            self.grade_mut(0.9999);
        } else if grade < 0.0 {
            self.grade_mut(0.0001);
        }
    }
    /// Mutate permanent even on an unchanged value pair; Weather.cpp:241-250.
    pub fn set(&mut self, kind: u32, grade: f32, permanent: bool) -> bool {
        self.permanent = permanent;
        if self.kind == kind && self.grade() == grade {
            return false;
        }
        self.kind = kind;
        self.grade_mut(grade);
        true
    }
    /// Preserve early returns before normalization; Weather.cpp:81-204.
    pub fn regenerate<C: Context>(
        &mut self,
        chances: Option<&Chances>,
        context: &mut C,
    ) -> Result<bool, C::Error> {
        if self.permanent {
            return Ok(false);
        }
        let old = *self;
        let Some(chances) = chances else {
            self.kind = FINE;
            self.grade_mut(0.0);
            return Ok(old.kind != self.kind || old.grade() != self.grade());
        };
        let u = context.urand(0, 99)?;
        if u < 30 {
            return Ok(false);
        }
        let season = season(context.year_day()?);
        if u < 60 && self.grade() < 0.33333334 {
            self.kind = FINE;
            self.grade_mut(0.0);
        }
        if u < 60 && self.kind != FINE {
            self.grade_mut(self.grade() - 0.33333334);
            return Ok(true);
        }
        if u < 90 && self.kind != FINE {
            self.grade_mut(self.grade() + 0.33333334);
            return Ok(true);
        }
        if self.kind != FINE {
            if self.grade() < 0.33333334 {
                self.grade_mut(0.9999);
                return Ok(true);
            }
            if self.grade() > 0.6666667 && context.urand(0, 99)? < 50 {
                self.grade_mut(self.grade() - 0.6666667);
                return Ok(true);
            }
            self.kind = FINE;
            self.grade_mut(0.0);
        }
        let [rain, snow, storm] = chances.0[season];
        let draw = context.urand(1, 100)?;
        self.kind = if draw <= rain {
            RAIN
        } else if draw <= rain.wrapping_add(snow) {
            SNOW
        } else if draw <= rain.wrapping_add(snow).wrapping_add(storm) {
            STORM
        } else {
            FINE
        };
        let grade = if self.kind == FINE {
            0.0
        } else if u < 90 {
            context.rand_norm_f()? * 0.3333
        } else if context.urand(0, 99)? < 50 {
            context.rand_norm_f()? * 0.3333 + 0.3334
        } else {
            context.rand_norm_f()? * 0.3333 + 0.6667
        };
        self.grade_mut(grade);
        self.normalize();
        Ok(self.kind != old.kind || self.grade() != old.grade())
    }
    /// Rain alone suppresses sound below 0.27; Weather.cpp:389-427.
    pub fn sound(self) -> u32 {
        let grade = self.grade();
        let base = match self.kind {
            RAIN if grade < 0.27 => return 0,
            RAIN => 8533,
            SNOW => 8536,
            STORM => 8556,
            _ => return 0,
        };
        base + if grade < 0.40 {
            0
        } else if grade < 0.70 {
            1
        } else {
            2
        }
    }
    /// Normalize before producing the 13-byte smooth-change payload; Weather.cpp:207-229.
    pub fn packet(&mut self) -> [u8; 13] {
        self.normalize();
        let mut bytes = [0; 13];
        bytes[..4].copy_from_slice(&self.kind.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.grade_bits.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.sound().to_le_bytes());
        bytes
    }
    /// Logging state has a shared 0.27 cutoff unlike sound; Weather.cpp:254-288.
    pub fn state(self) -> u32 {
        if self.grade() < 0.27 {
            return 0;
        }
        let states = match self.kind {
            RAIN => [3, 4, 5],
            SNOW => [6, 7, 8],
            STORM => [22, 41, 42],
            _ => return 0,
        };
        states[if self.grade() < 0.40 {
            0
        } else if self.grade() < 0.70 {
            1
        } else {
            2
        }]
    }
}

#[cfg(test)]
#[path = "source_weather_tests.rs"]
mod tests;
