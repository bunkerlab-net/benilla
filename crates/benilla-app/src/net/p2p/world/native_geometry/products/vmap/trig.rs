// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2018-2025 Free Software Foundation, Inc.
// Bounded Rust translation, modified 2026-10-15: Benilla fork contributors.
// The source-derived reducer/polynomial is shared in benilla-sim::source_trig.
// See GLIBC-NOTICE.txt and PROVENANCE.md for the exact source and license.
use super::Error;

/// Source-profile float trig only for admitted Euler degrees, not a general libm.
/// Preserve the original VMap degree conversion and native unfused arithmetic.
pub(super) fn sin_cos_degrees(angle: f32) -> Result<(f32, f32), Error> {
    if !angle.is_finite() || angle.abs() > 360.0 {
        return Err(Error::Unsupported);
    }
    let radians = (std::f64::consts::PI * f64::from(angle) / 180.0) as f32;
    let [s, c] = benilla_sim::source_trig::sin_cos_radians_bits_native(radians.to_bits())
        .map_err(|_| Error::Unsupported)?;
    Ok((f32::from_bits(s), f32::from_bits(c)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn signed_boundaries_match_pinned_glibc_source() {
        let cases = include_str!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/trig-cases.tsv");
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
        assert_eq!(count, 202);
    }

    #[test]
    fn entire_angle_domain_samples_match_independent_source_digest() {
        let mut digest = Sha256::new();
        let mut count = 0;
        let bit_grid = (0..=360.0f32.to_bits()).step_by(4093);
        let linear_grid = (0..=360 * 1024).map(|step| (step as f32 / 1024.0).to_bits());
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
        assert_eq!(count, 1_292_314);
        let expected = include_str!("../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/trig-grid.sha256").trim();
        assert_eq!(format!("{:x}", digest.finalize()), expected);
    }

    #[test]
    fn unsupported_angles_never_enter_bounded_reduction() {
        for angle in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            360.0f32.next_up(),
            (-360.0f32).next_down(),
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
