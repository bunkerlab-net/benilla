// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2018-2025 Free Software Foundation, Inc.
// Bounded Rust translation, modified 2026-10-15: Benilla fork contributors.
// The source-derived FMA kernel is shared in benilla-sim::source_trig_fma.
// See GLIBC-NOTICE.txt and PROVENANCE.md for the exact source and license.
use super::Error;

/// Source-profile float trig only for admitted Euler degrees, not a general libm.
/// Preserve the original VMap degree conversion and select pinned glibc 2.36 FMA.
pub(super) fn sin_cos_degrees(angle: f32) -> Result<(f32, f32), Error> {
    if !angle.is_finite() || angle.abs() > 1800.0 {
        return Err(Error::Unsupported);
    }
    // ModelInstance.cpp:31 uses G3D::pi() in binary64 before the float cast.
    let radians = (f64::from_bits(0x4009_21fb_5444_2d28) * f64::from(angle) / 180.0) as f32;
    if radians.to_bits() & 0x7fff_ffff > benilla_sim::source_trig::ASSEMBLER_MAX_RADIANS_BITS {
        return Err(Error::Unsupported);
    }
    let [s, c] = benilla_sim::source_trig_fma::sin_cos_bits_native(radians.to_bits());
    Ok((f32::from_bits(s), f32::from_bits(c)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn signed_boundaries_match_pinned_glibc_source() {
        let cases = include_str!(
            "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/trig-cases.tsv"
        );
        let mut count = 0;
        for row in cases.lines().filter(|line| !line.starts_with('#')) {
            let values: Vec<_> = row
                .split('\t')
                .map(|word| u32::from_str_radix(word, 16).unwrap())
                .collect();
            let (s, c) = sin_cos_degrees(f32::from_bits(values[0])).unwrap();
            assert_eq!([s.to_bits(), c.to_bits()], values[1..], "{row}");
            count += 1;
        }
        assert_eq!(count, 244);
    }

    #[test]
    fn entire_angle_domain_samples_match_independent_source_digest() {
        let mut digest = Sha256::new();
        let mut count = 0;
        let bit_grid = (0..=1800.0f32.to_bits()).step_by(4093);
        let linear_grid = (0..=1800 * 1024).map(|step| (step as f32 / 1024.0).to_bits());
        for magnitude in bit_grid.chain(linear_grid) {
            for sign in [0, 0x8000_0000] {
                let bits = magnitude | sign;
                let (s, c) = sin_cos_degrees(f32::from_bits(bits)).unwrap();
                for word in [bits, s.to_bits(), c.to_bits()] {
                    digest.update(word.to_le_bytes());
                }
                count += 1;
            }
        }
        assert_eq!(count, 4_251_072);
        let expected = include_str!("../../../../../../../../benilla-sim/tools/fixtures/source_trig_fma/degrees-grid.sha256").trim();
        assert_eq!(format!("{:x}", digest.finalize()), expected);
    }

    #[test]
    fn authored_degree_fma_discriminators_match_pinned_builder() {
        let cases = include_str!(
            "../../../../../../../../benilla-sim/tools/fixtures/source_trig_fma/degrees.tsv"
        );
        let mut count = 0;
        for row in cases.lines() {
            let values: Vec<u32> = row
                .split_whitespace()
                .map(|word| u32::from_str_radix(word, 16).unwrap())
                .collect();
            assert_eq!(values.len(), 4);
            let degrees = f32::from_bits(values[0]);
            let radians =
                (f64::from_bits(0x4009_21fb_5444_2d28) * f64::from(degrees) / 180.0) as f32;
            assert_eq!(radians.to_bits(), values[1]);
            let (sine, cosine) = sin_cos_degrees(degrees).unwrap();
            assert_eq!([sine.to_bits(), cosine.to_bits()], values[2..], "{row}");
            count += 1;
        }
        assert_eq!(count, 14);
    }

    #[test]
    fn unsupported_angles_never_enter_bounded_reduction() {
        for angle in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            1800.0f32.next_up(),
            (-1800.0f32).next_down(),
            f32::MAX,
            f32::MIN,
        ] {
            assert_eq!(sin_cos_degrees(angle), Err(Error::Unsupported));
        }
        assert_eq!(
            sin_cos_degrees(-0.0).unwrap().0.to_bits(),
            (-0.0f32).to_bits()
        );
    }
}
