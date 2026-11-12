// SPDX-License-Identifier: GPL-3.0-or-later
//! Full binary32 result bits of pinned glibc 2.36 FMA sinf/cosf.
//! Component arithmetic only. Historical execution still needs process dispatch binding.
mod core;
mod native;
use crate::facing::soft::{D, S};
use core::Arithmetic;

pub const POLICY: &str = "glibc-2.36-full-sincos-fma-soft-rne-v1";
pub const SOFT_POLICY: &str = "glibc-2.36-bounded-sincos-fma-soft-rne-v1";
pub const FALLBACK_SOFT_POLICY: &str = "glibc-2.36-near-point-fallback-sincos-fma-soft-rne-v1";
/// glibc-2.36 s_cosf.c, s_sincosf.h, s_sincosf_data.c, s_sinf.c, sincosf_poly.h.
pub const SOURCE_SHA256: [&str; 5] = [
    "2e60b691f1e4826da0f10057daecad243b3aaf2a8538ac7caee5c55bb763110a",
    "8155a4a1a21e947cae204802af76343686d8ede76963353b8192d8b3c4be4186",
    "0feab148a73db57ecbfbf6ae6ef4810ce1b2f0fd9788857ae6253636bc26a355",
    "f0d27b880bc10eab4e27f2f79f19c2af1aaa1bc4cce2e1675c999a1f1f2d698d",
    "df0bea6550598a828872c53d72f9f45473741de51f8e5e5617c1a6e27b86070e",
];
pub const LIBM_SHA256: &str = crate::source_sine::LIBM_SHA256;
pub const SINF_OFFSET: &str = "0x73720";
pub const COSF_OFFSET: &str = "0x73280";

/// Raw [sinf, cosf] results for every binary32 encoding, using software RNE.
/// NaNs retain sign/payload and become quiet. Both infinities return 0xffc00000.
/// This returns result bits only, not C errno or floating-point exception flags.
pub fn sin_cos_bits(bits: u32) -> [u32; 2] {
    core::sin_cos::<Software>(bits)
}

/// Same kernel with native IEEE RNE arithmetic and explicit fused multiply-add.
/// Requires gradual underflow and the caller's existing native FPU-profile guard.
/// No host trigonometric function or implicit compiler contraction is used.
pub fn sin_cos_bits_native(bits: u32) -> [u32; 2] {
    core::sin_cos::<native::Native>(bits)
}

/// FMA results with the existing ±TAU source-caller refusal boundary.
pub fn sin_cos_radians_bits_soft(bits: u32) -> Result<[u32; 2], crate::source_trig::Error> {
    crate::source_trig::validate(bits)?;
    Ok(sin_cos_bits(bits))
}

/// FMA results with the existing asymmetric near-point fallback boundary.
pub fn sin_cos_fallback_radians_bits_soft(
    bits: u32,
) -> Result<[u32; 2], crate::source_trig::Error> {
    crate::source_trig::validate_fallback(bits)?;
    Ok(sin_cos_bits(bits))
}

/// FMA results retaining the existing Euler-profile refusal boundary.
pub fn sin_cos_euler_radians_bits_native(bits: u32) -> Result<[u32; 2], crate::source_trig::Error> {
    crate::source_trig::validate_euler(bits)?;
    Ok(sin_cos_bits_native(bits))
}

struct Software;
impl Arithmetic for Software {
    type Double = D;
    fn double(bits: u64) -> D {
        D::bits(bits)
    }
    fn single(bits: u32) -> D {
        S::bits(bits).double()
    }
    fn integer(value: i64) -> D {
        D::integer_i64(value)
    }
    fn truncate(value: D) -> i32 {
        value
            .truncate_i32()
            .expect("source fast reducer is bounded by 120 radians")
    }
    fn narrow(value: D) -> u32 {
        value.single().raw()
    }
    fn fma(x: D, y: D, z: D) -> D {
        x.fma(y, z)
    }
}
