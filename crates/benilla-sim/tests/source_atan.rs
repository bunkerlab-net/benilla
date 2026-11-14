// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::source_atan::{
    atan_nonnegative_f32_bits as atan, SourceAtanError, GLIBC_REVISION, POLICY, SOURCE_SHA256,
};
use std::collections::BTreeMap;

const CASES: &str = include_str!("../tools/fixtures/source_atan/atan.tsv");

fn cases() -> BTreeMap<&'static str, (u32, u64, u32)> {
    CASES
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let words: Vec<_> = line.split_whitespace().collect();
            assert_eq!(words.len(), 4, "fixture shape: {line}");
            (
                words[0],
                (
                    u32::from_str_radix(words[1], 16).unwrap(),
                    u64::from_str_radix(words[2], 16).unwrap(),
                    u32::from_str_radix(words[3], 16).unwrap(),
                ),
            )
        })
        .collect()
}

// Independently derive the exact widening from IEEE exponent/significand fields.
fn widened(bits: u32) -> u64 {
    let sign = u64::from(bits >> 31) << 63;
    let exponent = (bits >> 23) & 0xff;
    let fraction = bits & 0x007f_ffff;
    match (exponent, fraction) {
        (0, 0) => sign,
        (0, _) => {
            let shift = fraction.leading_zeros() - 8;
            sign | (u64::from(897 - shift) << 52)
                | (u64::from((fraction << shift) & 0x007f_ffff) << 29)
        }
        (255, 0) => sign | 0x7ff0_0000_0000_0000,
        (255, _) => panic!("NaN is outside the accepted fixture domain"),
        _ => sign | (u64::from(exponent + 896) << 52) | (u64::from(fraction) << 29),
    }
}

#[test]
fn independent_original_c_oracle_matches_binary64_and_final_binary32() {
    let cases = cases();
    assert_eq!(cases.len(), 2_224);
    assert_eq!(
        cases.len(),
        CASES
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .count(),
        "fixture names must be unique"
    );
    for (name, (input, binary64, binary32)) in cases {
        let actual = atan(input).unwrap();
        assert_eq!(actual.input_f64_bits, widened(input), "input: {name}");
        assert_eq!(actual.atan_f64_bits, binary64, "binary64: {name}");
        assert_eq!(actual.atan_f32_bits, binary32, "binary32: {name}");
    }
}

#[test]
fn independent_original_c_sweep_matches_both_result_widths() {
    use sha2::{Digest, Sha256};
    let source = include_str!("../tools/fixtures/source_atan/sample-digests.tsv");
    let expected: Vec<_> = source
        .lines()
        .find(|line| line.starts_with("atan "))
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(expected.len(), 4);
    assert_eq!(expected[2], "4093");
    let mut digest = Sha256::new();
    let mut count = 0;
    for input in (0..=0x7f80_0000_u32)
        .step_by(4093)
        .chain([0x7f80_0000, 0x8000_0000])
    {
        let actual = atan(input).unwrap();
        digest.update(input.to_le_bytes());
        digest.update(actual.atan_f64_bits.to_le_bytes());
        digest.update(actual.atan_f32_bits.to_le_bytes());
        count += 1;
    }
    assert_eq!(count, 522_625);
    assert_eq!(count, expected[1].parse::<usize>().unwrap());
    assert_eq!(format!("{:x}", digest.finalize()), expected[3]);
}

#[test]
fn zeros_tiny_values_and_positive_infinity_keep_source_bits() {
    for input in [
        0,
        0x8000_0000,
        1,
        2,
        0x007f_ffff,
        0x0080_0000,
        0x0080_0001,
        0x325d_b3cf,
    ] {
        let actual = atan(input).unwrap();
        assert_eq!(actual.input_f64_bits, widened(input));
        assert_eq!(actual.atan_f64_bits, widened(input));
        assert_eq!(actual.atan_f32_bits, input);
    }
    let infinite = atan(0x7f80_0000).unwrap();
    assert_eq!(infinite.input_f64_bits, 0x7ff0_0000_0000_0000);
    assert_eq!(infinite.atan_f64_bits, 0x3ff9_21fb_5444_2d18);
    assert_eq!(infinite.atan_f32_bits, 0x3fc9_0fdb);
}

#[test]
fn negative_nonzero_and_all_nan_classes_are_rejected_before_arithmetic() {
    for input in [
        0x8000_0001,
        0x807f_ffff,
        0x8080_0000,
        0xbf80_0000,
        0xff7f_ffff,
        0xff80_0000,
    ] {
        assert_eq!(atan(input), Err(SourceAtanError::Negative));
    }
    for magnitude in [0x7f80_0001, 0x7fbf_ffff, 0x7fc0_0000, 0x7fff_ffff] {
        for sign in [0, 0x8000_0000] {
            assert_eq!(atan(magnitude | sign), Err(SourceAtanError::NaN));
        }
    }
}

// Exact integer RNE conversion for the positive rational table boundaries.
// The only inputs here have results in [1/16, 16], so no subnormal/overflow case.
fn rational_bits(numerator: u64, denominator: u64) -> u32 {
    let mut exponent = 0_i32;
    let (mut n, mut d) = (numerator, denominator);
    while n < d {
        n *= 2;
        exponent -= 1;
    }
    while n >= d * 2 {
        d *= 2;
        exponent += 1;
    }
    let numerator = n << 23;
    let quotient = numerator / d;
    let remainder = numerator % d;
    let increment = remainder * 2 > d || (remainder * 2 == d && quotient & 1 != 0);
    let significand = quotient + u64::from(increment);
    let fraction = u32::try_from(significand - (1 << 23)).unwrap();
    (u32::try_from(exponent + 127).unwrap() << 23) + fraction
}

#[test]
fn oracle_covers_every_branch_and_direct_and_reciprocal_table_transition() {
    let cases = cases();
    for (label, center) in [
        ("A", 0x325d_b3d0_u32),
        ("B", 0x3d80_0000),
        ("C", 0x3f80_0000),
        ("D", 0x4180_0000),
        ("E", 0x59a4_ff90),
    ] {
        for delta in [-1_i32, 0, 1] {
            let name = format!("boundary_{label}_{delta}");
            assert_eq!(
                cases.get(name.as_str()).unwrap().0,
                center.checked_add_signed(delta).unwrap(),
                "{name}"
            );
        }
    }
    for n in 16..=255 {
        for (label, center) in [
            ("direct", rational_bits(2 * n + 1, 512)),
            ("reciprocal", rational_bits(512, 2 * n + 1)),
        ] {
            for delta in [-1_i32, 0, 1] {
                let name = format!("table_{label}_{n}_{delta}");
                assert_eq!(
                    cases.get(name.as_str()).unwrap().0,
                    center.checked_add_signed(delta).unwrap(),
                    "{name}"
                );
            }
        }
    }
}

#[test]
fn selected_source_profile_and_license_are_explicit() {
    use sha2::{Digest, Sha256};
    assert_eq!(GLIBC_REVISION, "74f59e9271cbb4071671e5a474e7d4f1622b186f");
    assert_eq!(
        POLICY,
        "glibc-2.41-atan-binary64-from-binary32-soft-rne-non-fma-v1"
    );
    assert_eq!(SOURCE_SHA256.len(), 5);
    for hash in SOURCE_SHA256 {
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|c| c.is_ascii_hexdigit()));
    }
    let manifest = include_str!("../tools/source_atan_source_hashes.sha256");
    let numerical_sources: Vec<_> = manifest
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .take(5)
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(numerical_sources, SOURCE_SHA256);
    let notice = include_str!("../src/source_atan/GLIBC-NOTICE.txt");
    assert!(notice.contains("Copyright (C) 2001-2025 Free Software Foundation, Inc."));
    assert!(notice.contains("either version 2.1 of the License"));
    let license = include_bytes!("../src/source_atan/GLIBC-LICENSE.txt");
    assert_eq!(
        format!("{:x}", Sha256::digest(license)),
        "dc626520dcd53a22f727af3ee42c770e56c97a64fe3adb063799d8ab032fe551"
    );
}
