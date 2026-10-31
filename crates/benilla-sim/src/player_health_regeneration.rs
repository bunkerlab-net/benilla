// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Upstream GPL-2.0-or-later; Rust translation GPL-3.0-or-later.
//! Stateless standing-mage RegenerateHealth subset, not a scheduler or rate owner.
//! Caller must establish alive state, the shared source regeneration cadence,
//! standing, mage identity and absence of health-regeneration auras (88/116/161).
//! Inputs do not constitute evidence of those dependencies or native admission.
use crate::classic_spell::SpellFloat;
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round, Status, StatusAnd,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Input {
    pub health: u32,
    pub max_health: u32,
    pub spirit: u32,
    pub diff_ms: u32,
    /// Explicit authoritative rate supplied by the host; no default is inferred.
    pub health_rate: SpellFloat,
    pub in_combat: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub before: u32,
    pub after: u32,
    pub requested: i32,
    pub gained: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Numeric,
    UnsupportedRate,
    /// Positive ModifyHealth cannot portably interpret u32 health as signed i32.
    UnsupportedHealthDomain,
}
const R: Round = Round::NearestTiesToEven;
fn checked<T>(v: StatusAnd<T>) -> Result<T, Error> {
    if !(v.status & !Status::INEXACT).is_empty() {
        return Err(Error::Numeric);
    }
    Ok(v.value)
}
fn integer(value: u32) -> Result<Single, Error> {
    checked(Single::from_u128_r(u128::from(value), R))
}
/// Source formula and signed health mutation only. In the closed no-aura subset
/// combat computes zero and ModifyHealth(0) does not inspect signed health.
pub fn regenerate(input: Input) -> Result<Outcome, Error> {
    let unchanged = Outcome {
        before: input.health,
        after: input.health,
        requested: 0,
        gained: 0,
    };
    if input.health >= input.max_health || input.in_combat {
        return Ok(unchanged);
    }
    // Supported-rate restriction, not an upstream gameplay denial. Signed zero
    // remains valid; unused numeric operands behind early guards are not evaluated.
    if input.health_rate.bits() & 0x8000_0000 != 0 && input.health_rate.bits() & 0x7fff_ffff != 0 {
        return Err(Error::UnsupportedRate);
    }
    let spirit = integer(input.spirit)?;
    let wide: Double = checked(spirit.convert_r(R, &mut false))?;
    // Unsuffixed C++ 0.11 and 1 promote the already-rounded float Spirit to double.
    let wide = checked(wide.mul_r(Double::from_bits(0x3fbc_28f5_c28f_5c29), R))?;
    let wide = checked(wide.add_r(Double::from_bits(0x3ff0_0000_0000_0000), R))?;
    let base: Single = checked(wide.convert_r(R, &mut false))?;
    let rate = Single::from_bits(u128::from(input.health_rate.bits()));
    let mut addvalue = checked(base.mul_r(rate, R))?;
    // Source unconditional GetTotalAuraModifier(HEALTH_REGEN_IN_COMBAT)==0.
    addvalue = checked(addvalue.add_r(Single::ZERO, R))?;
    if addvalue < Single::ZERO {
        addvalue = Single::ZERO;
    }
    let scaled = checked(addvalue.mul_r(integer(input.diff_ms)?, R))?;
    let scaled = checked(scaled.div_r(integer(1000)?, R))?;
    let requested = checked(scaled.to_i128_r(32, Round::TowardZero, &mut false))?;
    let requested = i32::try_from(requested).map_err(|_| Error::Numeric)?;
    if requested == 0 {
        return Ok(unchanged);
    }
    let current = i32::try_from(input.health).map_err(|_| Error::UnsupportedHealthDomain)?;
    let maximum = i32::try_from(input.max_health).map_err(|_| Error::UnsupportedHealthDomain)?;
    // C++ dVal + curHealth is signed: cap must not hide an overflowing addition.
    let sum = current.checked_add(requested).ok_or(Error::Numeric)?;
    let after = u32::try_from(sum.min(maximum)).map_err(|_| Error::Numeric)?;
    Ok(Outcome {
        before: input.health,
        after,
        requested,
        gained: after - input.health,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Input {
        Input {
            health: 100,
            max_health: 1000,
            spirit: 37,
            diff_ms: 2000,
            health_rate: SpellFloat::ONE,
            in_combat: false,
        }
    }
    #[test]
    fn source_extracted_binary32_binary64_formula_and_modify_health() {
        let mut count = 0;
        for line in include_str!("../tests/fixtures/player_health_regeneration_cmangos.tsv")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let cells: Vec<u32> = line.split('\t').map(|v| v.parse().unwrap()).collect();
            assert_eq!(cells.len(), 9);
            let result = regenerate(Input {
                health: cells[0],
                max_health: cells[1],
                spirit: cells[2],
                diff_ms: cells[3],
                health_rate: SpellFloat::from_bits(cells[4]).unwrap(),
                in_combat: cells[5] != 0,
            })
            .unwrap();
            assert_eq!(
                (result.after, result.requested as u32, result.gained),
                (cells[6], cells[7], cells[8]),
                "{line}"
            );
            count += 1;
        }
        assert_eq!(count, 3062);
    }
    #[test]
    fn full_health_and_no_aura_combat_precede_unneeded_numeric_operands() {
        for in_combat in [false, true] {
            let i = Input {
                health: u32::MAX,
                max_health: u32::MAX,
                spirit: u32::MAX,
                diff_ms: u32::MAX,
                health_rate: SpellFloat::from_bits(1_000_000_f32.to_bits()).unwrap(),
                in_combat,
            };
            assert_eq!(regenerate(i).unwrap().after, u32::MAX);
        }
        let i = Input {
            health: 3_000_000_000,
            max_health: u32::MAX,
            in_combat: true,
            ..input()
        };
        assert_eq!(regenerate(i).unwrap().requested, 0);
    }
    #[test]
    fn rate_domain_cap_and_signed_domain_are_not_saturating_casts() {
        for rate in [0, 0x8000_0000] {
            let result = regenerate(Input {
                health_rate: SpellFloat::from_bits(rate).unwrap(),
                ..input()
            })
            .unwrap();
            assert_eq!(result.after, 100);
        }
        assert_eq!(
            regenerate(Input {
                health_rate: SpellFloat::from_bits((-1.0_f32).to_bits()).unwrap(),
                ..input()
            }),
            Err(Error::UnsupportedRate)
        );
        assert_eq!(
            regenerate(Input {
                health: 999,
                ..input()
            })
            .unwrap()
            .after,
            1000
        );
        let overflow = Input {
            health: i32::MAX as u32 - 1,
            max_health: i32::MAX as u32,
            ..input()
        };
        assert_eq!(regenerate(overflow), Err(Error::Numeric));
        assert_eq!(
            regenerate(Input {
                health: 3_000_000_000,
                max_health: u32::MAX,
                ..input()
            }),
            Err(Error::UnsupportedHealthDomain)
        );
        assert_eq!(
            regenerate(Input {
                spirit: u32::MAX,
                diff_ms: u32::MAX,
                ..input()
            }),
            Err(Error::Numeric)
        );
        // ModifyHealth(0) returns before the unsupported signed health conversion.
        assert!(regenerate(Input {
            health: 3_000_000_000,
            max_health: u32::MAX,
            diff_ms: 0,
            ..input()
        })
        .is_ok());
        for invalid in [0x7f80_0000, 0x7fc0_0000, 1] {
            assert!(SpellFloat::from_bits(invalid).is_err());
        }
    }
}
