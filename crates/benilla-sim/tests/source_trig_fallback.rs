// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::source_trig::{
    sin_cos_fallback_radians_bits_soft as fallback, sin_cos_radians_bits_native as native,
    sin_cos_radians_bits_soft as initial, Error, FALLBACK_MAX_ANGLE_BITS as MAX,
    FALLBACK_MIN_ANGLE_BITS as MIN, TAU_BITS,
};

const SIGN: u32 = 0x8000_0000;

#[test]
fn fallback_domain_is_asymmetric_and_does_not_expand_existing_trig_apis() {
    for angle in [MIN, MIN - 1, MAX - 1, MAX, 0, SIGN, 1, SIGN | 1, TAU_BITS] {
        assert!(fallback(angle).is_ok(), "{angle:08x}");
    }
    for angle in [MIN + 1, MAX + 1, TAU_BITS | SIGN, MAX | SIGN, 0x42f0_0000] {
        assert_eq!(fallback(angle), Err(Error::Angle), "{angle:08x}");
    }
    for angle in [TAU_BITS + 1, MAX] {
        assert_eq!(initial(angle), Err(Error::Angle));
        assert_eq!(native(angle), Err(Error::Angle));
    }
    // The older signed ±TAU domain is wider on the negative side.
    assert!(initial(TAU_BITS | SIGN).is_ok());
    assert!(native(TAU_BITS | SIGN).is_ok());
    for angle in [0, SIGN, 1, SIGN | 1, 0x007f_ffff, 0x807f_ffff] {
        assert_eq!(fallback(angle), Ok([angle, 0x3f80_0000]));
    }
}

#[test]
fn fallback_trig_matches_independent_original_glibc_boundary_vectors() {
    let cases = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/fallback-trig-cases.test-fixture");
    let mut count = 0;
    for row in cases.lines().filter(|line| !line.starts_with('#')) {
        let words: Vec<_> = row
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        assert_eq!(words.len(), 3);
        assert_eq!(fallback(words[0]).unwrap(), words[1..], "{row}");
        if words[0] & 0x7fff_ffff <= TAU_BITS {
            assert_eq!(fallback(words[0]), initial(words[0]), "shared core {row}");
        }
        count += 1;
    }
    assert_eq!(count, 251);
    // Wrapping 3*PI to PI first would produce a different sine bit pattern.
    assert_eq!(fallback(MAX), Ok([0xb2cc_de2e, 0xbf80_0000]));
    assert_eq!(fallback(MIN), Ok([0x33bb_bd2e, 0xbf80_0000]));
}

#[test]
fn fallback_trig_rejects_independent_wrapper_domain_vectors() {
    let cases = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/fallback-trig-rejections.test-fixture");
    let mut count = 0;
    for row in cases.lines().filter(|line| !line.starts_with('#')) {
        let angle = u32::from_str_radix(row, 16).unwrap();
        let expected = if angle & 0x7fff_ffff >= 0x7f80_0000 {
            Error::NonFinite
        } else {
            Error::Angle
        };
        assert_eq!(fallback(angle), Err(expected), "{row}");
        count += 1;
    }
    assert_eq!(count, 11);
}

#[test]
fn fallback_trig_matches_independent_asymmetric_sample_digest() {
    use sha2::{Digest, Sha256};
    let source = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/fallback-trig-grid.test-fixture");
    let expected: Vec<_> = source
        .lines()
        .find(|line| !line.starts_with('#'))
        .unwrap()
        .split_whitespace()
        .collect();
    let mut digest = Sha256::new();
    let mut count = 0;
    for (sign, limit, steps) in [(0, MAX, 38_603), (SIGN, MIN & !SIGN, 12_867)] {
        let bit_grid = (0..=limit).step_by(65_521);
        let linear_grid = (0..=steps).map(|step| (step as f32 / 4_096.0).to_bits());
        for magnitude in bit_grid.chain(linear_grid).chain([limit]) {
            let angle = magnitude | sign;
            let [s, c] = fallback(angle).unwrap();
            for word in [angle, s, c] {
                digest.update(word.to_le_bytes());
            }
            count += 1;
        }
    }
    assert_eq!(count, 84_602);
    assert_eq!(count, expected[0].parse::<usize>().unwrap());
    assert_eq!(format!("{:x}", digest.finalize()), expected[1]);
}
