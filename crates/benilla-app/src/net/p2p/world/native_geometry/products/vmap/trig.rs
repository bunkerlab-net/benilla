// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2018-2025 Free Software Foundation, Inc.
// Bounded Rust translation, modified 2026-10-14: Benilla fork contributors.
// Derived from GNU C Library 2.41 sinf/cosf, sinf_poly and reduce_fast.
// See GLIBC-NOTICE.txt and PROVENANCE.md for the exact source and license.
use super::Error;

// Original hexadecimal binary64 coefficients, with no decimal conversion.
const HPI_INV: f64 = f64::from_bits(0x4164_5f30_6dc9_c883); // 2/pi * 2^24
const HPI: f64 = f64::from_bits(0x3ff9_21fb_5444_2d18);
const COS: [f64; 5] = [
    1.0,
    f64::from_bits(0xbfdf_ffff_fd0c_621c),
    f64::from_bits(0x3fa5_5553_e106_8f19),
    f64::from_bits(0xbf56_c087_e89a_359d),
    f64::from_bits(0x3ef9_9343_027b_f8c3),
];
const SIN: [f64; 3] = [
    f64::from_bits(0xbfc5_5554_5995_a603),
    f64::from_bits(0x3f81_1076_0523_0bc4),
    f64::from_bits(0xbf29_94eb_3774_cf24),
];

/// Source-profile float trig only for admitted Euler degrees, not a general libm.
/// Ordinary Rust operations preserve the unfused source binary64 expression order.
pub(super) fn sin_cos_degrees(angle: f32) -> Result<(f32, f32), Error> {
    if !angle.is_finite() || angle.abs() > 360.0 {
        return Err(Error::Unsupported);
    }
    let radians = (std::f64::consts::PI * f64::from(angle) / 180.0) as f32;
    let top = (radians.to_bits() >> 20) & 0x7ff;
    let x = f64::from(radians);
    // Source abstop12 branches, including its signed-zero and tiny-input behavior.
    if top < 0x3f4 {
        if top < 0x398 {
            return Ok((radians, 1.0));
        }
        return Ok((
            polynomial(x, x * x, 0, false),
            polynomial(x, x * x, 1, false),
        ));
    }
    // All admitted inputs have |radians| <= float(2*pi), well below 120.
    // Pin the source's TOINT_INTRINSICS=0 reduction on every architecture.
    let n = ((x * HPI_INV) as i32 + 0x80_0000) >> 24;
    let x = x - f64::from(n) * HPI;
    let sign = [1.0, -1.0, -1.0, 1.0][(n & 3) as usize];
    Ok((
        polynomial(x * sign, x * x, n, n & 2 != 0),
        polynomial(x * sign, x * x, n ^ 1, n & 2 != 0),
    ))
}

fn polynomial(x: f64, x2: f64, n: i32, negate_cos: bool) -> f32 {
    if n & 1 == 0 {
        let x3 = x * x2;
        let s1 = SIN[1] + x2 * SIN[2];
        let x7 = x3 * x2;
        let s = x + x3 * SIN[0];
        (s + x7 * s1) as f32
    } else {
        // Source negates all cosine coefficients before evaluating the polynomial.
        let c = COS.map(|c| if negate_cos { -c } else { c });
        let x4 = x2 * x2;
        let c2 = c[3] + x2 * c[4];
        let c1 = c[0] + x2 * c[1];
        let x6 = x4 * x2;
        let c = c1 + x4 * c[2];
        (c + x6 * c2) as f32
    }
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
