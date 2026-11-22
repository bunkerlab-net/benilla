// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::{
    near_point::{
        candidate_bits, fallback_candidate_bits as fallback, Error, MAP_CLAMP_BITS,
        MAX_COORDINATE_BITS,
    },
    source_trig::{FALLBACK_MAX_ANGLE_BITS as MAX, FALLBACK_MIN_ANGLE_BITS as MIN, TAU_BITS},
};

const SIGN: u32 = 0x8000_0000;

#[test]
fn fallback_candidate_uses_raw_angles_without_normalization_and_source_clamps() {
    assert_eq!(
        fallback([0, 0], 0x3f80_0000, MAX),
        Ok([0xbf80_0000, 0xb2cc_de2e])
    );
    assert_eq!(
        fallback([0, 0], 0x3f80_0000, MIN),
        Ok([0xbf80_0000, 0x33bb_bd2e])
    );
    assert_eq!(
        fallback([0, 0], 0x7f7f_ffff, SIGN | 1),
        Ok([MAP_CLAMP_BITS, 0xb4ff_ffff])
    );
    assert_eq!(fallback([SIGN, SIGN], SIGN, 0), Ok([SIGN, SIGN]));
    assert_eq!(fallback([SIGN, SIGN], 0, 0), Ok([0, 0]));
    assert_eq!(fallback([0, 0], 1, 0), Ok([1, 0]));
    assert_eq!(fallback([0xc69c_4000, 0], 0x469c_4000, 0), Ok([0, 0]));
    assert_eq!(
        fallback([MAX_COORDINATE_BITS, MAX_COORDINATE_BITS | SIGN], 0, MAX),
        Ok([MAP_CLAMP_BITS, MAP_CLAMP_BITS | SIGN])
    );
    assert_eq!(
        fallback([0xbf0a_5141, 0xbf57_6aa6], 0x3f80_0001, 0x3f80_0000),
        Ok([0, 0])
    );
    for angle in [MIN, SIGN | 1, TAU_BITS + 1, MAX] {
        assert!(fallback([0, 0], 0, angle).is_ok());
        assert_eq!(candidate_bits([0, 0], 0, angle), Err(Error::Angle));
    }
}

#[test]
fn fallback_rejects_every_unsupported_field_even_at_zero_distance() {
    for bad in [
        0x7f80_0000,
        0xff80_0000,
        0x7fc0_0000,
        0x7f80_0001,
        0xffc0_1234,
    ] {
        assert_eq!(fallback([bad, 0], 0, 0), Err(Error::NonFinite));
        assert_eq!(fallback([0, bad], 0, 0), Err(Error::NonFinite));
        assert_eq!(fallback([0, 0], bad, 0), Err(Error::NonFinite));
        assert_eq!(fallback([0, 0], 0, bad), Err(Error::NonFinite));
    }
    for sign in [0, SIGN] {
        let outside = (MAX_COORDINATE_BITS + 1) | sign;
        assert_eq!(fallback([outside, 0], 0, 0), Err(Error::Coordinate));
        assert_eq!(fallback([0, outside], 0, 0), Err(Error::Coordinate));
    }
    for distance in [SIGN | 1, 0xbf80_0000, 0xff7f_ffff] {
        assert_eq!(fallback([0, 0], distance, 0), Err(Error::Distance));
    }
    for angle in [MIN + 1, MAX + 1, TAU_BITS | SIGN, MAX | SIGN, 0x42f0_0000] {
        assert_eq!(fallback([0, 0], 0, angle), Err(Error::Angle));
    }
}

#[test]
fn fallback_candidate_matches_independent_source_cases() {
    let cases = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/fallback-candidate-cases.test-fixture");
    let mut count = 0;
    for row in cases.lines().filter(|line| !line.starts_with('#')) {
        let words: Vec<_> = row
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        assert_eq!(words.len(), 6);
        let actual = fallback([words[0], words[1]], words[2], words[3]).unwrap();
        assert_eq!(actual, words[4..], "{row}");
        assert!(actual
            .into_iter()
            .all(|word| word & !SIGN <= MAP_CLAMP_BITS));
        count += 1;
    }
    assert_eq!(count, 7200);
}

#[test]
fn fallback_candidate_preserves_initial_profile_results_on_their_shared_domain() {
    let cases = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/candidate-cases.test-fixture");
    for row in cases.lines().filter(|line| !line.starts_with('#')) {
        let words: Vec<_> = row
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        assert_eq!(
            fallback([words[0], words[1]], words[2], words[3]).unwrap(),
            words[4..],
            "{row}"
        );
    }
}

#[test]
fn fallback_candidate_rejects_independent_wrapper_domain_vectors() {
    let cases = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/fallback-candidate-rejections.test-fixture");
    let mut count = 0;
    for row in cases.lines().filter(|line| !line.starts_with('#')) {
        let words: Vec<_> = row
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        assert_eq!(words.len(), 4);
        assert!(
            fallback([words[0], words[1]], words[2], words[3]).is_err(),
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 38);
}

#[test]
fn fallback_candidate_matches_independent_asymmetric_sample_digest() {
    use sha2::{Digest, Sha256};
    let source = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/fallback-candidate-grid.test-fixture");
    let expected: Vec<_> = source
        .lines()
        .find(|line| !line.starts_with('#'))
        .unwrap()
        .split_whitespace()
        .collect();
    let count = expected[0].parse::<usize>().unwrap();
    assert_eq!(count, 32_768);
    let mut state = u32::from_str_radix(expected[1], 16).unwrap();
    assert_eq!(state, 0x4e50_4642);
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let mut digest = Sha256::new();
    for _ in 0..count {
        let [x, y, distance, angle] = [next(), next(), next(), next()];
        let x = (x & SIGN) | ((x & !SIGN) % (MAX_COORDINATE_BITS + 1));
        let y = (y & SIGN) | ((y & !SIGN) % (MAX_COORDINATE_BITS + 1));
        let distance = (distance & !SIGN) % 0x7f80_0000;
        let limit = if angle & SIGN != 0 { MIN & !SIGN } else { MAX };
        let angle = (angle & SIGN) | ((angle & !SIGN) % (limit + 1));
        let [result_x, result_y] = fallback([x, y], distance, angle).unwrap();
        for word in [x, y, distance, angle, result_x, result_y] {
            digest.update(word.to_le_bytes());
        }
    }
    assert_eq!(format!("{:x}", digest.finalize()), expected[2]);
}
