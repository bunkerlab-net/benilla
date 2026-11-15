// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Movement/{spline.cpp,spline.impl.h,MoveSpline.cpp}.
//! Retained source MoveSpline numerical state. Launch, packets and relocation are separate owners.
use crate::facing::soft::{D, S};
use serde::Serialize;

mod movement;
pub use movement::*;
#[cfg(test)]
mod tests;

pub type Result<T, E = Error> = std::result::Result<T, E>;
pub type Point = [u32; 3];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(pub &'static str);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}
pub(crate) fn integer(value: u32) -> S {
    use rustc_apfloat::{ieee::Single, Float, Round};
    S::bits(
        Single::from_u128_r(u128::from(value), Round::NearestTiesToEven)
            .value
            .to_bits() as u32,
    )
}
fn signed(value: i32) -> S {
    D::integer(value).single()
}
fn truncate(value: S, id: &'static str) -> Result<i32> {
    value.double().truncate_i32().ok_or(Error(id))
}

/// MoveSplineInit.cpp:188; G3D g3dmath.h:468-477,576-577.
pub fn wrap_facing(angle: u32) -> Result<u32> {
    let value = S::bits(angle);
    let tau = S::bits(0x40c9_0fdb);
    if value >= S::ZERO && value < tau {
        return Ok(angle);
    }
    let quotient = value / tau;
    let mut floor = truncate(quotient, "facing_wrap_floor_conversion_undefined")?;
    if signed(floor) > quotient {
        floor = floor
            .checked_sub(1)
            .ok_or(Error("facing_wrap_floor_conversion_undefined"))?;
    }
    Ok((value - tau * signed(floor)).raw())
}

/// ByteBuffer.h:421-427. Casts truncate toward zero before the integer masks.
pub fn pack_xyz(point: Point) -> Result<u32> {
    let mut packed = 0u32;
    for (i, value) in point.into_iter().enumerate() {
        let value = truncate(
            S::bits(value) / S::bits(0x3e80_0000),
            "packet_xyz_conversion_undefined",
        )?;
        packed |= (value as u32 & if i == 2 { 0x3ff } else { 0x7ff }) << (i * 11);
    }
    Ok(packed)
}

/// Unit.cpp:11068-11073; Common.h:28,213 selects float atan2 and float 2*pi.
pub fn relocation_orientation(old: [u32; 4], new: [u32; 4]) -> Result<u32> {
    let old = old.map(S::bits);
    let new = new.map(S::bits);
    if new[1] == old[1] && new[0] == old[0] {
        return Ok(old[3].raw());
    }
    let angle = S::bits(
        crate::facing::atan2_bits((new[1] - old[1]).raw(), (new[0] - old[0]).raw())
            .map_err(|_| Error("relocation_atan2_numeric_domain_11072"))?,
    );
    Ok(if angle >= S::ZERO {
        angle
    } else {
        S::bits(0x40c9_0fdb) + angle
    }
    .raw())
}
fn sqrt(value: S) -> S {
    // Reuse the integer sqrtf kernel; IEEE nonfinite results reach the source conversion.
    let bits = value.raw();
    if bits & 0x7fff_ffff >= 0x7f80_0000 {
        return if bits & 0x7fff_ffff == 0x7f80_0000 && bits >> 31 == 0 {
            value
        } else {
            S::bits(bits | 0x0040_0000)
        };
    }
    crate::source_transports::numeric::sqrt_single(value).expect("sum of squares is nonnegative")
}
fn distance(a: Point, b: Point) -> S {
    let d: [S; 3] = std::array::from_fn(|i| S::bits(a[i]) - S::bits(b[i]));
    sqrt((d[0] * d[0] + d[1] * d[1]) + d[2] * d[2])
}
fn lerp(a: Point, b: Point, t: S) -> Point {
    std::array::from_fn(|i| (S::bits(a[i]) + (S::bits(b[i]) - S::bits(a[i])) * t).raw())
}

/// spline.cpp:99-115 and G3D Vector4.cpp:121-129, including the initial +0 addition.
pub(crate) fn catmull_evaluate(points: &[Point], index: usize, t: S, derivative: bool) -> Point {
    let one = integer(1);
    let times = if derivative {
        [integer(3) * t * t, integer(2) * t, one, S::ZERO]
    } else {
        [t * t * t, t * t, t, one]
    };
    let coefficients = [
        [0xbf00_0000, 0x3fc0_0000, 0xbfc0_0000, 0x3f00_0000],
        [0x3f80_0000, 0xc020_0000, 0x4000_0000, 0xbf00_0000],
        [0xbf00_0000, 0, 0x3f00_0000, 0],
        [0, 0x3f80_0000, 0, 0],
    ];
    let mut weights = [S::ZERO; 4];
    for i in 0..4 {
        for j in 0..4 {
            weights[i] = weights[i] + times[j] * S::bits(coefficients[j][i]);
        }
    }
    std::array::from_fn(|axis| {
        let p = |n: usize| S::bits(points[index - 1 + n][axis]);
        (((p(0) * weights[0] + p(1) * weights[1]) + p(2) * weights[2]) + p(3) * weights[3]).raw()
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Mode {
    Linear,
    CatmullRom,
}
/// The source Spline<int32> specialization used by MoveSpline, not the older LinearSpline.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Spline {
    points: Vec<Point>,
    lengths: Vec<i32>,
    first: usize,
    last: usize,
    mode: Option<Mode>,
    cyclic: bool,
}
impl Spline {
    /// spline.cpp:205-219,241-272. Linear also dispatches InitCatmullRom, cpp:50-57.
    pub fn initialize(controls: &[Point], mode: Mode, cyclic: Option<usize>) -> Result<Self> {
        let mut spline = Self::default();
        spline.initialize_controls(controls, mode, cyclic)?;
        Ok(spline)
    }
    fn initialize_controls(
        &mut self,
        controls: &[Point],
        mode: Mode,
        cyclic: Option<usize>,
    ) -> Result<()> {
        let count = controls.len();
        self.mode = Some(mode);
        self.cyclic = cyclic.is_some();
        self.points
            .resize(count + if cyclic.is_some() { 3 } else { 2 }, [0; 3]);
        self.points[1..count + 1].copy_from_slice(controls);
        let first = *controls
            .first()
            .ok_or(Error("spline_control_first_undefined"))?;
        if cyclic == Some(0) {
            self.points[0] = controls[count - 1];
        } else {
            let next = *controls
                .get(1)
                .ok_or(Error("spline_control_pair_undefined"))?;
            self.points[0] = lerp(first, next, signed(-1));
        }
        if let Some(cycle) = cyclic {
            self.points[count + 1] = *controls
                .get(cycle)
                .ok_or(Error("spline_cycle_control_undefined"))?;
            self.points[count + 2] = *controls
                .get(cycle + 1)
                .ok_or(Error("spline_cycle_next_control_undefined"))?;
        } else {
            self.points[count + 1] = controls[count - 1];
        }
        self.first = 1;
        self.last = count + usize::from(cyclic.is_some());
        Ok(())
    }
    pub fn points(&self) -> &[Point] {
        &self.points
    }
    pub fn lengths(&self) -> &[i32] {
        &self.lengths
    }
    pub fn first(&self) -> usize {
        self.first
    }
    pub fn last(&self) -> usize {
        self.last
    }
    pub fn empty(&self) -> bool {
        self.first == self.last
    }
    pub fn cyclic(&self) -> bool {
        self.cyclic
    }
    /// spline.cpp:287-292 and spline.impl.h:91-95 retain mode and cyclic on clear.
    pub fn clear(&mut self) {
        self.first = 0;
        self.last = 0;
        self.points.clear();
        self.lengths.clear();
    }
    fn index(&self, index: usize) -> Result<()> {
        if index < self.first || index >= self.last {
            Err(Error("spline_segment_assertion"))
        } else {
            Ok(())
        }
    }
    /// spline.cpp:117-127,136-146. The source does not clamp the segment parameter.
    pub fn evaluate(&self, index: usize, t: u32, derivative: bool) -> Result<Point> {
        self.index(index)?;
        Ok(match self.mode {
            Some(Mode::Linear) => {
                if derivative {
                    std::array::from_fn(|a| {
                        (S::bits(self.points[index + 1][a]) - S::bits(self.points[index][a])).raw()
                    })
                } else {
                    lerp(self.points[index], self.points[index + 1], S::bits(t))
                }
            }
            Some(Mode::CatmullRom) => catmull_evaluate(&self.points, index, S::bits(t), derivative),
            None => return Err(Error("spline_uninitialized_assertion")),
        })
    }
    /// spline.cpp:155-179. Three binary32 chords accumulate in binary64, then narrow once.
    pub fn segment_length(&self, index: usize) -> Result<u32> {
        self.index(index)?;
        if self.mode == Some(Mode::Linear) {
            return Ok(distance(self.points[index], self.points[index + 1]).raw());
        }
        let mut previous = self.points[index];
        let mut length = D::ZERO;
        for step in 1..=3 {
            let next = self.evaluate(index, (integer(step) / integer(3)).raw(), false)?;
            length = length + distance(next, previous).double();
            previous = next;
        }
        Ok(length.single().raw())
    }
    /// Spline<int32>::initLengths, spline.impl.h:79-89. Conversion follows each float addition.
    pub fn init_lengths(&mut self) -> Result<()> {
        let mut length = 0;
        self.lengths.resize(self.last + 1, 0);
        for i in self.first..self.last {
            length = truncate(
                signed(length) + S::bits(self.segment_length(i)?),
                "spline_length_conversion_undefined",
            )?;
            self.lengths[i + 1] = length;
        }
        Ok(())
    }
    pub fn length(&self, index: usize) -> Result<i32> {
        self.lengths
            .get(index)
            .copied()
            .ok_or(Error("spline_length_index_undefined"))
    }
    pub fn total_length(&self) -> Result<i32> {
        self.length(self.last)
    }
    pub fn segment_time(&self, index: usize) -> Result<i32> {
        self.length(index + 1)?
            .checked_sub(self.length(index)?)
            .ok_or(Error("spline_length_subtraction_undefined"))
    }
    /// spline.impl.h:37-77, including strict '<' at an exact boundary and int32 truncation.
    pub fn compute_index(&self, t: u32) -> Result<(usize, u32)> {
        let t = S::bits(t);
        if !(t >= S::ZERO && t <= integer(1)) {
            return Err(Error("spline_percent_assertion"));
        }
        let length = truncate(
            t * signed(self.total_length()?),
            "spline_index_length_conversion_undefined",
        )?;
        let mut i = self.first;
        while i + 1 < self.last && self.length(i + 1)? < length {
            i += 1;
        }
        self.index(i)?;
        let delta = length
            .checked_sub(self.length(i)?)
            .ok_or(Error("spline_index_subtraction_undefined"))?;
        Ok((i, (signed(delta) / signed(self.segment_time(i)?)).raw()))
    }
}
