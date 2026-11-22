// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::spell_range::{check_bits, Error, RangeResult};

fn at(x: f32, z: f32) -> [u32; 3] {
    [x.to_bits(), 0, z.to_bits()]
}
fn check(x: f32, minimum: f32, maximum: f32, strict: bool) -> RangeResult {
    check_bits(
        at(0.0, 0.0),
        at(x, 0.0),
        minimum.to_bits(),
        maximum.to_bits(),
        1.5f32.to_bits(),
        1.5f32.to_bits(),
        strict,
    )
    .unwrap()
    .result
}
#[test]
fn source_endpoints_and_adjacent_binary32_values_are_inclusive() {
    for (distance, result) in [
        (f32::from_bits(8.0f32.to_bits() - 1), RangeResult::TooClose),
        (8.0, RangeResult::Within),
        (f32::from_bits(8.0f32.to_bits() + 1), RangeResult::Within),
        (f32::from_bits(33.0f32.to_bits() - 1), RangeResult::Within),
        (33.0, RangeResult::Within),
        (f32::from_bits(33.0f32.to_bits() + 1), RangeResult::TooFar),
    ] {
        assert_eq!(check(distance, 5.0, 30.0, true), result);
    }
    assert_eq!(check(0.0, 0.0, 30.0, true), RangeResult::Within);
    assert_eq!(check(36.0, 5.0, 30.0, false), RangeResult::Within);
    assert_eq!(
        check(f32::from_bits(36.0f32.to_bits() + 1), 5.0, 30.0, false),
        RangeResult::TooFar
    );
    assert_eq!(check(22.0, 0.0, 17.0, false), RangeResult::Within);
    assert_eq!(
        check(f32::from_bits(22.0f32.to_bits() + 1), 0.0, 17.0, false),
        RangeResult::TooFar
    );
}
#[test]
fn source_distance_is_3d_squared_and_coincident_units_keep_the_minimum() {
    let outcome = check_bits(
        at(0.0, 0.0),
        at(0.0, 34.0),
        0,
        30.0f32.to_bits(),
        1.5f32.to_bits(),
        1.5f32.to_bits(),
        true,
    )
    .unwrap();
    assert_eq!(outcome.distance_squared, (34.0f32 * 34.0).to_bits());
    assert_eq!(outcome.result, RangeResult::TooFar);
    assert_eq!(check(0.0, 5.0, 30.0, true), RangeResult::TooClose);
}
#[test]
fn tiny_normal_inputs_keep_source_subnormal_intermediates_and_gradual_underflow() {
    let distance = check_bits([0; 3], [0x1c80_0000, 0, 0], 0, 0x0080_0000, 0, 0, true).unwrap();
    assert_eq!(distance.distance_squared, 0x0000_0200);
    assert_eq!(distance.result, RangeResult::TooFar);
    let tolerance = check_bits([0; 3], [0; 3], 0, 0x0080_0000, 0, 0, false).unwrap();
    assert!(tolerance.maximum > 0x0080_0000);
    // Source computes a positive minimum whose square rounds to zero; no clamp.
    assert_eq!(
        check_bits([0; 3], [0; 3], 0x0080_0000, 0x0080_0000, 0, 0, true)
            .unwrap()
            .result,
        RangeResult::Within
    );
}
#[test]
fn explicit_input_bounds_reject_overflow_nonfinite_negative_and_unordered_cases() {
    assert_eq!(
        check_bits([0; 3], [0x7f7f_ffff, 0, 0], 0, 0, 0, 0, true),
        Err(Error::Coordinate)
    );
    assert_eq!(
        check_bits([0; 3], [0; 3], 0, 0x7f7f_ffff, 0, 0, true),
        Err(Error::Range)
    );
    assert_eq!(
        check_bits([0; 3], [0; 3], 0, 0, 0x7f7f_ffff, 0, true),
        Err(Error::Reach)
    );
    assert_eq!(
        check_bits([0; 3], [0; 3], 0, 0x7f80_0000, 0, 0, true),
        Err(Error::NonFinite)
    );
    assert_eq!(
        check_bits([0; 3], [0; 3], 0, 0x7fc0_0000, 0, 0, true),
        Err(Error::NonFinite)
    );
    assert_eq!(
        check_bits([0; 3], [0; 3], 0xbf80_0000, 0, 0, 0, true),
        Err(Error::Range)
    );
    assert_eq!(
        check_bits([0; 3], [0; 3], 0, 0, 0xbf80_0000, 0, true),
        Err(Error::Reach)
    );
    assert_eq!(
        check_bits([0; 3], [0; 3], 0x3f80_0000, 0, 0, 0, true),
        Err(Error::Range)
    );
}
