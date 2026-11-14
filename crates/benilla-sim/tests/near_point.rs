// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::{
    near_point::{candidate_bits, Error, MAP_CLAMP_BITS, MAX_COORDINATE_BITS},
    source_trig::TAU_BITS,
};

const SIGN: u32 = 0x8000_0000;

#[test]
fn initial_candidate_zero_and_axis_results_follow_source_binary32_operations() {
    assert_eq!(candidate_bits([0, 0], 0x3f80_0000, 0), Ok([0x3f80_0000, 0]));
    assert_eq!(
        candidate_bits([0, SIGN], 0x3f80_0000, SIGN),
        Ok([0x3f80_0000, SIGN])
    );
    assert_eq!(candidate_bits([SIGN, SIGN], SIGN, 0), Ok([SIGN, SIGN]));
    // RNE addition of opposite signed zeros produces positive zero.
    assert_eq!(candidate_bits([SIGN, SIGN], 0, 0), Ok([0, 0]));
    // Do not canonicalize a genuine subnormal candidate through runtime Real.
    assert_eq!(candidate_bits([0, 0], 1, 0), Ok([1, 0]));
    assert_eq!(candidate_bits([0, 0], 0x3f80_0000, 1), Ok([0x3f80_0000, 1]));
}

#[test]
fn source_map_clamp_is_strict_and_applies_after_the_coordinate_sum() {
    let limit = MAP_CLAMP_BITS;
    assert_eq!(
        candidate_bits([limit, limit | SIGN], 0, 0),
        Ok([limit, limit | SIGN])
    );
    assert_eq!(candidate_bits([limit, 0], 0x3f80_0000, 0), Ok([limit, 0]));
    assert_eq!(candidate_bits([0, 0], 0x7f7f_ffff, 0), Ok([limit, 0]));
    // Any finite nonnegative distance is admitted; maximum distance clamps safely.
    for angle in [0, 1, 0x3fc9_0fdb, 0x4049_0fdb, 0x4096_cbe4, TAU_BITS] {
        let result = candidate_bits([limit, limit | SIGN], 0x7f7f_ffff, angle).unwrap();
        assert!(result.into_iter().all(|bits| bits & 0x7fff_ffff <= limit));
    }
    // The admitted input domain is wider than the output clamp. Do not preclamp.
    assert_eq!(candidate_bits([0xc69c_4000, 0], 0x469c_4000, 0), Ok([0, 0]));
    assert_eq!(
        candidate_bits([MAX_COORDINATE_BITS, MAX_COORDINATE_BITS | SIGN], 0, 0),
        Ok([limit, limit | SIGN])
    );
    // A movement inward from the source clamp border is not clamped early.
    assert_eq!(candidate_bits([limit | SIGN, 0], limit, 0), Ok([0, 0]));
}

#[test]
fn invalid_fields_reject_even_when_zero_distance_would_hide_them() {
    for bad in [
        0x7f80_0000,
        0xff80_0000,
        0x7fc0_0000,
        0x7f80_0001,
        0xffc0_1234,
    ] {
        assert_eq!(candidate_bits([bad, 0], 0, 0), Err(Error::NonFinite));
        assert_eq!(candidate_bits([0, bad], 0, 0), Err(Error::NonFinite));
        assert_eq!(candidate_bits([0, 0], bad, 0), Err(Error::NonFinite));
        assert_eq!(candidate_bits([0, 0], 0, bad), Err(Error::NonFinite));
    }
    for sign in [0, SIGN] {
        let outside = (MAX_COORDINATE_BITS + 1) | sign;
        assert_eq!(candidate_bits([outside, 0], 0, 0), Err(Error::Coordinate));
        assert_eq!(candidate_bits([0, outside], 0, 0), Err(Error::Coordinate));
        assert!(candidate_bits([MAX_COORDINATE_BITS | sign, 0], 0, 0).is_ok());
    }
    for negative in [SIGN | 1, SIGN | 0x3f80_0000, SIGN | 0x7f7f_ffff] {
        assert_eq!(candidate_bits([0, 0], negative, 0), Err(Error::Distance));
        assert_eq!(candidate_bits([0, 0], 0, negative), Err(Error::Angle));
    }
    assert_eq!(candidate_bits([0, 0], 0, TAU_BITS + 1), Err(Error::Angle));
    assert!(candidate_bits([0, 0], 0, TAU_BITS).is_ok());
    assert!(candidate_bits([0, 0], SIGN, SIGN).is_ok());
}

#[test]
fn independent_source_near_point_cases_match_exact_bits() {
    let cases = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/candidate-cases.test-fixture");
    let mut count = 0;
    for row in cases.lines().filter(|line| !line.starts_with('#')) {
        let words: Vec<_> = row
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        assert_eq!(words.len(), 6);
        let result = candidate_bits([words[0], words[1]], words[2], words[3]).unwrap();
        assert_eq!(result, words[4..], "{row}");
        count += 1;
    }
    assert_eq!(count, 3012);
}

#[test]
fn independent_source_wrapper_domain_rejects_every_unsupported_case() {
    let cases = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/candidate-rejections.test-fixture");
    let mut count = 0;
    for row in cases.lines().filter(|line| !line.starts_with('#')) {
        let words: Vec<_> = row
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        assert_eq!(words.len(), 4);
        assert!(
            candidate_bits([words[0], words[1]], words[2], words[3]).is_err(),
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 37);
}

#[test]
fn source_products_are_not_fused_and_raw_tiny_angles_remain_observable() {
    // Exact source fixture: separately rounded products cancel to zero. Fusing
    // instead yields 0x31a51400 and 0xb2a25570, respectively.
    assert_eq!(
        candidate_bits([0xbf0a_5141, 0xbf57_6aa6], 0x3f80_0001, 0x3f80_0000),
        Ok([0, 0])
    );
    // Subnormal angle1 cannot survive Real's storage contract; source arithmetic
    // with a large valid distance nevertheless produces an ordinary normal Y.
    assert_eq!(
        candidate_bits([0, 0], 0x7f7f_ffff, 1),
        Ok([MAP_CLAMP_BITS, 0x34ff_ffff])
    );
    // Source cancellation starts outside the output clamp and only then clamps Y.
    assert_eq!(
        candidate_bits([0x46ea_6000, 0xc6ea_6000], 0x46ea_6000, 0x4049_0fdb),
        Ok([0, MAP_CLAMP_BITS | SIGN])
    );
}

#[test]
fn independent_source_candidate_sample_digest() {
    use sha2::{Digest, Sha256};
    let source = include_str!("../../benilla-native-geometry-tests/tools/fixtures/near_point_candidate/candidate-grid.test-fixture");
    let expected: Vec<_> = source
        .lines()
        .find(|line| !line.starts_with('#'))
        .unwrap()
        .split_whitespace()
        .collect();
    let count = expected[0].parse::<usize>().unwrap();
    assert_eq!(count, 32_768);
    let mut state = u32::from_str_radix(expected[1], 16).unwrap();
    assert_eq!(state, 0x4e50_4341);
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let mut digest = Sha256::new();
    for _ in 0..count {
        let [x, y, distance, angle] = [next(), next(), next(), next()];
        let x = (x & SIGN) | ((x & 0x7fff_ffff) % (MAX_COORDINATE_BITS + 1));
        let y = (y & SIGN) | ((y & 0x7fff_ffff) % (MAX_COORDINATE_BITS + 1));
        let distance = (distance & 0x7fff_ffff) % 0x7f80_0000;
        let angle = (angle & 0x7fff_ffff) % (TAU_BITS + 1);
        let [result_x, result_y] = candidate_bits([x, y], distance, angle).unwrap();
        for word in [x, y, distance, angle, result_x, result_y] {
            digest.update(word.to_le_bytes());
        }
    }
    assert_eq!(format!("{:x}", digest.finalize()), expected[2]);
}
