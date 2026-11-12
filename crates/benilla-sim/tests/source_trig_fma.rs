// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::{source_trig as old, source_trig_fma as fma};

#[test]
fn pinned_fma_vectors_match_native_and_software_results() {
    let inputs = include_str!("../tools/fixtures/source_trig_fma/inputs.txt");
    let expected = include_str!("../tools/fixtures/source_trig_fma/expected.tsv");
    let mut count = 0;
    for (input, row) in inputs.lines().zip(expected.lines()) {
        let words: Vec<u32> = row
            .split_whitespace()
            .map(|s| u32::from_str_radix(s, 16).unwrap())
            .collect();
        assert_eq!(words.len(), 3);
        assert_eq!(words[0], u32::from_str_radix(input, 16).unwrap());
        let result = [words[1], words[2]];
        assert_eq!(
            fma::sin_cos_bits(words[0]),
            result,
            "software input {input}"
        );
        assert_eq!(
            fma::sin_cos_bits_native(words[0]),
            result,
            "native input {input}"
        );
        count += 1;
    }
    assert_eq!(inputs.lines().count(), 83);
    assert_eq!(expected.lines().count(), 83);
    assert_eq!(count, 83);
}

#[test]
fn ten_known_assembler_inputs_distinguish_fma_from_the_retained_offline_profile() {
    for sign in [0, 0x8000_0000] {
        for (input, before, after) in [
            (0x418a_3adb, 0xb7b4_f76f, 0xb7b4_f770),
            (0x418a_3adc, 0xb7a4_f76f, 0xb7a4_f770),
            (0x418a_3add, 0xb794_f76f, 0xb794_f770),
            (0x418a_3ade, 0xb784_f76f, 0xb784_f770),
            (0x41bc_76d9, 0xbb80_3f6a, 0xbb80_3f6b),
        ] {
            let input = input | sign;
            assert_eq!(
                old::sin_cos_assembler_radians_bits_native(input).unwrap()[1],
                before
            );
            assert_eq!(fma::sin_cos_bits(input)[1], after);
            assert_eq!(fma::sin_cos_bits_native(input)[1], after);
        }
    }
}

#[test]
fn software_and_native_operations_agree_across_exponents_and_reduction_tables() {
    for bits in (0..=u32::MAX).step_by(65537) {
        assert_eq!(
            fma::sin_cos_bits(bits),
            fma::sin_cos_bits_native(bits),
            "{bits:08x}"
        );
    }
}

#[test]
fn migrated_caller_wrappers_preserve_every_old_domain_refusal() {
    for bits in [
        0,
        1,
        old::TAU_BITS,
        old::TAU_BITS + 1,
        old::FALLBACK_MAX_ANGLE_BITS,
        old::FALLBACK_MAX_ANGLE_BITS + 1,
        old::EULER_MAX_RADIANS_BITS,
        old::EULER_MAX_RADIANS_BITS + 1,
        0x7f80_0000,
        0x7f80_0001,
        0x7fc1_2345,
    ] {
        for sign in [0, 0x8000_0000] {
            let bits = bits | sign;
            assert_eq!(
                fma::sin_cos_radians_bits_soft(bits).map(|_| ()),
                old::sin_cos_radians_bits_soft(bits).map(|_| ())
            );
            assert_eq!(
                fma::sin_cos_fallback_radians_bits_soft(bits).map(|_| ()),
                old::sin_cos_fallback_radians_bits_soft(bits).map(|_| ())
            );
            assert_eq!(
                fma::sin_cos_euler_radians_bits_native(bits).map(|_| ()),
                old::sin_cos_euler_radians_bits_native(bits).map(|_| ())
            );
        }
    }
}
