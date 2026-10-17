// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic ObjectPosSelector.cpp/.h, Copyright CMaNGOS contributors,
// GPL-2.0-or-later, revision 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
//! Empty used-area lists only, as in the pinned GetNearPointAt caller whose
//! population visitor is commented out. These angles are not geometry facts.
use crate::{
    facing::soft::{D, S},
    source_atan,
};

pub const POLICY: &str = "cmangos-8ec338a1-empty-selector-glibc-2.41-atan64-soft-rne-v1";
/// LF-normalized ObjectPosSelector.cpp/.h, ObjectDefines.h and Common.h.
pub const SOURCE_SHA256: [&str; 4] = [
    "e4d85d02d78f975e5bf1912bb1a9d7677b5920ecee9fdf9c5928e4f6a5458b8b",
    "777f09cec2af7a5222383de444740a6c594fc5af4140362489d2556bef1b02f7",
    "8a5433945cf1a8016afa7ce9618cd0b2140721961acf05c44dee2b47cbff3491",
    "5d07c692b1a729cf8e4045812c5633e096c8d47c81b2530c641b2ee4c0aefc32",
];
pub const PI_BITS: u32 = 0x4049_0fdb;
pub const DEFAULT_SIZE_BITS: u32 = 0x3ec7_2b02;
/// At most 314 offsets per side under this nonnegative, finite input profile.
pub const MAX_OFFSETS: usize = 628;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NonFinite,
    Negative,
    Atan,
    BaseAngle,
    Offset,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source empty selector numeric domain: {self:?}")
    }
}
impl std::error::Error for Error {}

fn repaired_size(bits: u32) -> Result<S, Error> {
    if bits & 0x7fff_ffff >= 0x7f80_0000 {
        return Err(Error::NonFinite);
    }
    let value = S::bits(bits);
    if value < S::ZERO {
        return Err(Error::Negative);
    }
    // Both signed zeros compare equal in the original constructor.
    Ok(if value == S::ZERO {
        S::bits(DEFAULT_SIZE_BITS)
    } else {
        value
    })
}

/// Fresh source selector followed by InitializeAngle, with actual empty lists.
/// Zero repairs affect selector spacing only, not candidate distance or radii.
#[derive(Clone)]
pub struct EmptySelector {
    ratio: u32,
    half: S,
    steps: [S; 2],
}
impl EmptySelector {
    pub fn new(searcher_size: u32, distance: u32) -> Result<Self, Error> {
        let size = repaired_size(searcher_size)?;
        let distance = repaired_size(distance)?;
        // Original constructor: two separately rounded binary32 operations,
        // then global atan(double), then narrowing to its float member.
        let ratio = (S::bits(0x3fe6_6666) * size) / distance;
        let atan = source_atan::atan_nonnegative_f32_bits(ratio.raw()).map_err(|_| Error::Atan)?;
        let half = S::bits(atan.atan_f32_bits);
        // This subtraction is not unary negation when half is positive zero.
        let initial = S::ZERO - half;
        Ok(Self {
            ratio: ratio.raw(),
            half,
            steps: [initial; 2],
        })
    }
    pub fn ratio_bits(&self) -> u32 {
        self.ratio
    }
    pub fn half_angle_bits(&self) -> u32 {
        self.half.raw()
    }
    pub fn step_bits(&self) -> [u32; 2] {
        self.steps.map(S::raw)
    }
}
impl Iterator for EmptySelector {
    type Item = u32;
    fn next(&mut self) -> Option<Self::Item> {
        let pi = S::bits(PI_BITS);
        loop {
            // Exact NextAngle branch order: plus wins equal-step ties.
            let side = if self.steps[0] < pi && self.steps[0] <= self.steps[1] {
                0
            } else if self.steps[1] < pi {
                1
            } else {
                return None;
            };
            // The source's unsuffixed 0.01 promotes BOTH additions to double.
            let increment = self.half.double() + D::bits(0x3f84_7ae1_47ae_147b);
            self.steps[side] = (self.steps[side].double() + increment).single();
            if self.steps[side] > pi {
                continue;
            }
            // Both source lists are empty; NextSideAngle returns immediately.
            let sign = if side == 0 { 0x3f80_0000 } else { 0xbf80_0000 };
            return Some((self.steps[side] * S::bits(sign)).raw());
        }
    }
}
impl std::iter::FusedIterator for EmptySelector {}

/// GetNearPointAt supplies raw binary32(absAngle + angle), without normalization.
/// The final asymmetric angle domain is handled by fallback_candidate_bits.
pub fn candidate_angle_bits(base: u32, offset: u32) -> Result<u32, Error> {
    if base & 0x7fff_ffff >= 0x7f80_0000 || offset & 0x7fff_ffff >= 0x7f80_0000 {
        return Err(Error::NonFinite);
    }
    let base = S::bits(base);
    if base < S::ZERO || base > S::bits(crate::source_trig::TAU_BITS) {
        return Err(Error::BaseAngle);
    }
    if offset & 0x7fff_ffff > PI_BITS {
        return Err(Error::Offset);
    }
    Ok((base + S::bits(offset)).raw())
}
