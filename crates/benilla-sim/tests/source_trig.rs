// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::source_trig::{
    sin_cos_radians_bits_native as native, sin_cos_radians_bits_soft as soft, Error, TAU_BITS,
};

#[test]
fn tiny_source_branches_preserve_signed_zero_and_subnormal_bits() {
    for magnitude in [0, 1, 2, 0x007f_ffff, 0x0080_0000, 0x397f_ffff] {
        for sign in [0, 0x8000_0000] {
            let angle = magnitude | sign;
            assert_eq!(native(angle), Ok([angle, 0x3f80_0000]));
            assert_eq!(soft(angle), Ok([angle, 0x3f80_0000]));
        }
    }
}

#[test]
fn bounded_reducer_rejects_nonfinite_and_outside_angles() {
    for bad in [
        0x7f80_0000,
        0xff80_0000,
        0x7fc0_0000,
        0x7f80_0001,
        0xffc0_1234,
    ] {
        assert_eq!(native(bad), Err(Error::NonFinite));
        assert_eq!(soft(bad), Err(Error::NonFinite));
    }
    for sign in [0, 0x8000_0000] {
        for magnitude in [TAU_BITS + 1, 0x42f0_0000, 0x7f7f_ffff] {
            assert_eq!(native(magnitude | sign), Err(Error::Angle));
            assert_eq!(soft(magnitude | sign), Err(Error::Angle));
        }
        assert!(native(TAU_BITS | sign).is_ok());
        assert!(soft(TAU_BITS | sign).is_ok());
    }
}

#[test]
fn signed_reduction_and_polynomial_boundaries_agree_between_adapters() {
    // Source abstop12 thresholds, adjacent quadrants, negative reduction and TAU.
    for magnitude in [
        0x397f_ffff,
        0x3980_0000,
        0x3980_0001,
        0x3f3f_ffff,
        0x3f40_0000,
        0x3f40_0001,
        0x3fc9_0fda,
        0x3fc9_0fdb,
        0x3fc9_0fdc,
        0x4016_cbe3,
        0x4016_cbe4,
        0x4016_cbe5,
        0x4049_0fda,
        0x4049_0fdb,
        0x4049_0fdc,
        0x4096_cbe3,
        0x4096_cbe4,
        0x4096_cbe5,
        TAU_BITS - 1,
        TAU_BITS,
    ] {
        for sign in [0, 0x8000_0000] {
            let angle = magnitude | sign;
            assert_eq!(native(angle), soft(angle), "radians {angle:08x}");
        }
    }
}

#[test]
fn independent_pinned_source_radian_cases_match_both_adapters() {
    let cases = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/trig-cases.test-fixture");
    let mut count = 0;
    for row in cases.lines().filter(|line| !line.starts_with('#')) {
        let words: Vec<_> = row
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        assert_eq!(words.len(), 3);
        assert_eq!(native(words[0]).unwrap(), words[1..], "native {row}");
        assert_eq!(soft(words[0]).unwrap(), words[1..], "software {row}");
        count += 1;
    }
    assert_eq!(count, 258);
}

#[test]
fn independent_pinned_source_radian_domain_rejections() {
    let cases = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/trig-rejections.test-fixture");
    let mut count = 0;
    for row in cases.lines().filter(|line| !line.starts_with('#')) {
        let angle = u32::from_str_radix(row, 16).unwrap();
        assert!(native(angle).is_err(), "native {row}");
        assert!(soft(angle).is_err(), "software {row}");
        count += 1;
    }
    assert_eq!(count, 9);
}

#[test]
fn both_adapters_match_independent_radian_sample_digest() {
    use sha2::{Digest, Sha256};
    let source = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/trig-grid.test-fixture");
    let expected: Vec<_> = source
        .lines()
        .find(|line| !line.starts_with('#'))
        .unwrap()
        .split_whitespace()
        .collect();
    let mut native_digest = Sha256::new();
    let mut software_digest = Sha256::new();
    let mut count = 0;
    let bit_grid = (0..=TAU_BITS).step_by(4093);
    let linear_grid = (0..=102_943).map(|step| (step as f32 / 16_384.0).to_bits());
    for magnitude in bit_grid.chain(linear_grid).chain([TAU_BITS]) {
        for sign in [0, 0x8000_0000] {
            let angle = magnitude | sign;
            let [s, c] = native(angle).unwrap();
            for word in [angle, s, c] {
                native_digest.update(word.to_le_bytes());
            }
            let [s, c] = soft(angle).unwrap();
            for word in [angle, s, c] {
                software_digest.update(word.to_le_bytes());
            }
            count += 1;
        }
    }
    assert_eq!(count, 737_002);
    assert_eq!(count, expected[0].parse::<usize>().unwrap());
    assert_eq!(format!("{:x}", native_digest.finalize()), expected[1]);
    assert_eq!(format!("{:x}", software_digest.finalize()), expected[1]);
}

#[test]
fn reference_fma_libm_grid_or_explicit_absence() {
    use benilla_sim::source_trig::{
        sin_cos_assembler_radians_bits_native, sin_cos_fallback_radians_bits_soft,
        FALLBACK_MAX_ANGLE_BITS, FALLBACK_MIN_ANGLE_BITS,
    };
    use std::io::{BufRead, BufReader};
    let Ok(path) = std::env::var("BENILLA_F4_LIBM_TRIG_GRID") else {
        println!("SKIP reference glibc 2.36 FMA grid: set BENILLA_F4_LIBM_TRIG_GRID");
        return;
    };
    let reader = BufReader::new(std::fs::File::open(path).unwrap());
    let mut rows = 0;
    let mut native_differences = 0;
    let mut software_differences = 0;
    let mut fallback_differences = 0;
    for line in reader.lines() {
        let line = line.unwrap();
        let words: Vec<u32> = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        assert_eq!(words.len(), 3);
        let angle = words[0];
        let expected = [words[1], words[2]];
        native_differences +=
            usize::from(sin_cos_assembler_radians_bits_native(angle).unwrap() != expected);
        if angle & 0x7fff_ffff <= TAU_BITS {
            software_differences += usize::from(soft(angle).unwrap() != expected);
        }
        let limit = if angle >> 31 == 0 {
            FALLBACK_MAX_ANGLE_BITS
        } else {
            FALLBACK_MIN_ANGLE_BITS & 0x7fff_ffff
        };
        if angle & 0x7fff_ffff <= limit {
            fallback_differences +=
                usize::from(sin_cos_fallback_radians_bits_soft(angle).unwrap() != expected);
        }
        rows += 1;
    }
    println!("Reference FMA grid: {rows} inputs, {native_differences} native, {software_differences} software and {fallback_differences} fallback differences");
    assert_eq!(rows, 540_926);
    assert_eq!(native_differences, 0);
    assert_eq!(software_differences, 0);
    assert_eq!(fallback_differences, 0);
}
