// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::near_point::primary::{target_eye_bits, vertical_acceptance_bits, Error};

#[test]
fn original_candidate_is_strict_and_keeps_left_associated_binary32_size_order() {
    let one = 1f32.to_bits();
    assert_eq!(vertical_acceptance_bits(0, one, one, 0, 0), Ok(false));
    assert_eq!(vertical_acceptance_bits(0, one - 1, one, 0, 0), Ok(true));
    assert_eq!(vertical_acceptance_bits(0, one + 1, one, 0, 0), Ok(false));
    // Each +1 rounds back to 2^24; reassociating 2^24+(1+1) would accept equality.
    let large = 16_777_216f32.to_bits();
    assert_eq!(
        vertical_acceptance_bits(0, large, large, one, one),
        Ok(false)
    );
    // Source finite operands may overflow the size sum; do not add an invented cap.
    assert_eq!(
        vertical_acceptance_bits(0, one, f32::MAX.to_bits(), f32::MAX.to_bits(), one),
        Ok(true)
    );
}
#[test]
fn target_eye_adds_only_target_height_and_retains_raw_xy_and_source_zero_signs() {
    assert_eq!(
        target_eye_bits([0x8000_0000, 1, 7f32.to_bits()], 2f32.to_bits()),
        Ok([0x8000_0000, 1, 9f32.to_bits()])
    );
    assert_eq!(
        target_eye_bits([0, 0, 0x8000_0000], 0x8000_0000),
        Ok([0, 0, 0x8000_0000])
    );
}
#[test]
fn nonfinite_and_negative_dimensions_are_explicitly_outside_the_numeric_contract() {
    for invalid in [
        f32::NAN.to_bits(),
        f32::INFINITY.to_bits(),
        f32::NEG_INFINITY.to_bits(),
    ] {
        for field in 0..5 {
            let mut args = [0, 0, 1f32.to_bits(), 0, 0];
            args[field] = invalid;
            assert_eq!(
                vertical_acceptance_bits(args[0], args[1], args[2], args[3], args[4]),
                Err(Error::NonFinite)
            );
        }
        assert_eq!(target_eye_bits([0, 0, invalid], 0), Err(Error::NonFinite));
        assert_eq!(target_eye_bits([0; 3], invalid), Err(Error::NonFinite));
    }
    for field in 2..5 {
        let mut args = [0; 5];
        args[field] = (-1f32).to_bits();
        assert_eq!(
            vertical_acceptance_bits(args[0], args[1], args[2], args[3], args[4]),
            Err(Error::NegativeSize)
        );
    }
    assert_eq!(
        target_eye_bits([0; 3], (-1f32).to_bits()),
        Err(Error::NegativeSize)
    );
    assert_eq!(
        target_eye_bits([0, 0, f32::MAX.to_bits()], f32::MAX.to_bits()),
        Err(Error::NonFinite)
    );
}
