// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded selected glibc 2.41 sinf/cosf profile, not host-libm equivalence.
//! Native and software adapters share one private source-derived core.
//! The core retains its separate LGPL-2.1-or-later grant; see ../../SOURCE-TRIG.md.
mod core;
mod native;
mod software;

pub const NATIVE_POLICY: &str = "glibc-2.41-bounded-sincos-native-unfused-rne-v1";
pub const SOFT_POLICY: &str = "glibc-2.41-bounded-sincos-soft-unfused-rne-v1";
pub const TAU_BITS: u32 = 0x40c9_0fdb;
/// Pinned s_cosf.c, s_sincosf.h, s_sincosf_data.c, s_sinf.c, sincosf_poly.h.
pub const SOURCE_SHA256: [&str; 5] = [
    "a6e8c6c2da36b0b859bd83d4c6725667f55ca2d23488dcafb803c23023e55694",
    "40c282f82dae4225bccdb6444624cbd2d9030ebfeca98ad5b248dc9d9e168448",
    "2ad440b58b5baac791c82c70d0d8b61788875f4ecfecb05a5f5e80977c161514",
    "d2781d7f1f91557d24f342b25c3cc2356fec8f0cf1dfaccce63917e7900ae440",
    "1c42f6f9a22fb814ccb879d25ac6f277440b5f7264bbeec07c1fddcbf0a9542f",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NonFinite,
    Angle,
    Reduction,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source trig numeric domain: {self:?}")
    }
}
impl std::error::Error for Error {}

fn validate(angle: u32) -> Result<(), Error> {
    let magnitude = angle & 0x7fff_ffff;
    if magnitude >= 0x7f80_0000 {
        return Err(Error::NonFinite);
    }
    if magnitude > TAU_BITS {
        return Err(Error::Angle);
    }
    Ok(())
}

/// Raw [sine, cosine] bits for finite radians within inclusive ±binary32 TAU.
/// This adapter retains native unfused binary64 operations under the native IEEE
/// RNE/gradual-underflow profile. It never calls host sin/cos or changes the FPU.
pub fn sin_cos_radians_bits_native(angle: u32) -> Result<[u32; 2], Error> {
    validate(angle)?;
    core::sin_cos::<native::Native>(angle)
}

/// Raw [sine, cosine] bits with software IEEE RNE and gradual underflow throughout.
/// Accepts finite radians within inclusive ±binary32 TAU; signed zero is preserved.
/// The bounded source reducer truncates to integer toward zero, as C requires.
pub fn sin_cos_radians_bits_soft(angle: u32) -> Result<[u32; 2], Error> {
    validate(angle)?;
    core::sin_cos::<software::Software>(angle)
}
