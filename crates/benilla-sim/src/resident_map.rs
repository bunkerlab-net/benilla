// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f contributors;
// upstream GPL-2.0-or-later, Rust translation GPL-3.0-or-later.
//! ComputeCellPair plus Map::Add cell bounds. Not map-coordinate validation,
//! loaded-grid/terrain evidence, registration, visibility or native admission.
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round, Status, StatusAnd,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellPair {
    x: u16,
    y: u16,
}
impl CellPair {
    pub fn x(self) -> u16 {
        self.x
    }
    pub fn y(self) -> u16 {
        self.y
    }
    pub fn grid_x(self) -> u16 {
        self.x / 16
    }
    pub fn grid_y(self) -> u16 {
        self.y / 16
    }
    pub fn validate(self) -> bool {
        self.x < 1024 && self.y < 1024
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Numeric,
    OutsideMap,
}
const R: Round = Round::NearestTiesToEven;
fn checked<T>(value: StatusAnd<T>) -> Result<T, Error> {
    if !(value.status & !Status::INEXACT).is_empty() {
        return Err(Error::Numeric);
    }
    Ok(value.value)
}
fn double(value: Single) -> Result<Double, Error> {
    checked(value.convert_r(R, &mut false))
}
fn coordinate(bits: u32) -> Result<u16, Error> {
    let input = Single::from_bits(u128::from(bits));
    if !input.is_finite() {
        return Err(Error::Numeric);
    }
    // Exact binary32 rounding of source 533.33333f. Both macro divisions
    // happen in binary32 before function arguments promote to binary64.
    let grid = Single::from_bits(0x4405_5555);
    let size = checked(grid.div_r(Single::from_u128(16).value, R))?;
    let offset = checked(size.div_r(Single::from_u128(2).value, R))?;
    let relative = checked(double(input)?.sub_r(double(offset)?, R))?;
    let relative = checked(relative.div_r(double(size)?, R))?;
    let centered = checked(relative.add_r(Double::from_u128(512).value, R))?;
    let centered = checked(centered.add_r(Double::from_bits(0x3fe0_0000_0000_0000), R))?;
    let signed = checked(centered.to_i128_r(32, Round::TowardZero, &mut false))?;
    // Source int→uint32 wrapping is followed by unsigned >=1024 rejection.
    // Negative fractions truncated to zero above are intentionally admitted.
    if !(0..1024).contains(&signed) {
        return Err(Error::OutsideMap);
    }
    u16::try_from(signed).map_err(|_| Error::Numeric)
}
/// Raw binary32 coordinates preserve exact signed zero/subnormal inputs.
/// NaN/infinity or source undefined int conversion are refused, never clamped.
pub fn compute_cell_pair(x_bits: u32, y_bits: u32) -> Result<CellPair, Error> {
    Ok(CellPair {
        x: coordinate(x_bits)?,
        y: coordinate(y_bits)?,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_source_oracle_matches_cells_and_rejections() {
        let mut count = 0;
        for line in include_str!("../tests/fixtures/resident_map_cmangos.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
        {
            let v: Vec<_> = line
                .split_whitespace()
                .map(|x| x.parse::<u32>().unwrap())
                .collect();
            assert_eq!(v.len(), 5);
            let actual = compute_cell_pair(v[0], v[1]);
            match v[2] {
                0 => {
                    let cell = actual.unwrap();
                    assert_eq!((cell.x(), cell.y()), (v[3] as u16, v[4] as u16));
                    assert!(cell.validate());
                    assert_eq!(
                        (cell.grid_x(), cell.grid_y()),
                        ((v[3] / 16) as u16, (v[4] / 16) as u16)
                    );
                }
                1 => assert_eq!(actual, Err(Error::OutsideMap)),
                2 => assert_eq!(actual, Err(Error::Numeric)),
                _ => panic!("oracle status"),
            }
            count += 1;
        }
        assert_eq!(count, ORACLE_ROWS);
    }
    const ORACLE_ROWS: usize = 6200;
    #[test]
    fn cell_validation_does_not_claim_loaded_grid_or_valid_map_coordinate() {
        assert_eq!(
            compute_cell_pair(0, 0).unwrap(),
            CellPair { x: 512, y: 512 }
        );
        assert_eq!(
            compute_cell_pair(0x80000000, 1).unwrap(),
            CellPair { x: 512, y: 512 }
        );
        assert_eq!(compute_cell_pair(0xc685_7000, 0).unwrap().x(), 0); // -17080: negative fraction truncates to zero.
        assert_eq!(compute_cell_pair(0x4685_5400, 0).unwrap().x(), 1023); // +17066.
        assert_eq!(compute_cell_pair(0x4685_5600, 0), Err(Error::OutsideMap)); // +17067.
        assert!(!CellPair { x: 1024, y: 0 }.validate());
        assert!(!CellPair { x: 0, y: u16::MAX }.validate());
        for invalid in [0x7fc00000, 0x7f800000, 0xff800000, 0x7f7fffff] {
            assert_eq!(compute_cell_pair(invalid, 0), Err(Error::Numeric));
        }
    }
}
