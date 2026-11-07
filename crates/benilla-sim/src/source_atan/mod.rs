// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected glibc 2.41 binary64 atan of an exactly widened binary32 ratio.
//! Software RNE arithmetic, including the original non-FMA double-length macros.
//! The translated core and table retain LGPL-2.1-or-later; see ../../SOURCE-ATAN.md.
mod core;
mod table;

use crate::facing::soft::S;

pub const POLICY: &str = "glibc-2.41-atan-binary64-from-binary32-soft-rne-non-fma-v1";
pub const GLIBC_REVISION: &str = "74f59e9271cbb4071671e5a474e7d4f1622b186f";
/// Full-file hashes of s_atan.c, dla.h, mydefs.h, uatan.tbl and atnat.h, in order.
pub const SOURCE_SHA256: [&str; 5] = [
    "786f7cf3abc44135498c253da678569247f40a13e7158886b9ad065185e7ca5f",
    "f06aa3a847e7fdbb1e9f0969a8a3d32ce8cb33165612e8434aadb1f3ee3a9f6e",
    "7d8a1e3c8a0769ca4e778704fc32f0517207ec7ce79cc494217927933fdef9c5",
    "dcc2d24c0e5c83de489cae4143e4753f73702b04ed905b183c0da493647bf4ab",
    "a4c3b10f0b81fbf902f636801f871b53763c5f3b0e5df6c68334dd3197c162de",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceAtanError {
    Negative,
    NaN,
}

impl std::fmt::Display for SourceAtanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source atan numeric domain: {self:?}")
    }
}

impl std::error::Error for SourceAtanError {}

/// Raw values at the source binary32 -> binary64 atan -> binary32 boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceAtanResult {
    pub input_f64_bits: u64,
    pub atan_f64_bits: u64,
    pub atan_f32_bits: u32,
}

/// Evaluate the pinned binary64 atan after exact widening of a binary32 ratio.
/// Accepts nonnegative finite values, +infinity and both zeros. Negative zero
/// is deliberately preserved, matching the source tiny-input return. Rejects
/// negative nonzero values and every NaN before any arithmetic. All operations
/// and final narrowing use software round-to-nearest/ties-to-even; no host atan.
pub fn atan_nonnegative_f32_bits(input_bits: u32) -> Result<SourceAtanResult, SourceAtanError> {
    let magnitude = input_bits & 0x7fff_ffff;
    if magnitude > 0x7f80_0000 {
        return Err(SourceAtanError::NaN);
    }
    if input_bits & 0x8000_0000 != 0 && magnitude != 0 {
        return Err(SourceAtanError::Negative);
    }
    let input = S::bits(input_bits).double();
    let result = core::atan(input);
    Ok(SourceAtanResult {
        input_f64_bits: input.raw(),
        atan_f64_bits: result.raw(),
        atan_f32_bits: result.single().raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::table::CIJ;
    use sha2::{Digest, Sha256};

    #[test]
    fn table_words_match_both_original_endian_tables() {
        // The independent generator parses both hash-pinned original endian
        // sections, requires all 1687 words to agree, then hashes u64 LE bytes.
        let fixture = include_str!("../../tools/fixtures/source_atan/coefficients.sha256");
        let expected: Vec<_> = fixture.split_whitespace().collect();
        assert_eq!(expected.len(), 2);
        assert_eq!(expected[1], "cij_u64_le_241x7");
        let mut digest = Sha256::new();
        for row in CIJ {
            for word in row {
                digest.update(word.to_le_bytes());
            }
        }
        assert_eq!(format!("{:x}", digest.finalize()), expected[0]);
    }
}
