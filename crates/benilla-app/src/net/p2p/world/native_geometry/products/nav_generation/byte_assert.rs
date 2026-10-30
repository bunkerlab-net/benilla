// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact byte checks with a constant-size failure report, including length-only differences.
use sha2::{Digest, Sha256};

pub(crate) fn assert_navigation_bytes(actual: &[u8], expected: &[u8], label: &str) {
    if actual == expected {
        return;
    }
    let offsets: Vec<_> = (0..actual.len().max(expected.len()))
        .filter(|&offset| actual.get(offset) != expected.get(offset))
        .take(8)
        .collect();
    panic!(
        "{label}: byte mismatch; actual_len={} expected_len={} actual_sha256={:x} expected_sha256={:x} first_offsets={offsets:?}",
        actual.len(),
        expected.len(),
        Sha256::digest(actual),
        Sha256::digest(expected),
    );
}

#[test]
fn byte_diagnostics_are_bounded_without_weakening_equality() {
    assert_navigation_bytes(&[], &[], "empty");
    assert_navigation_bytes(&[1, 2], &[1, 2], "equal");
    for (actual, expected, offsets) in [
        (
            vec![0; 1_000_000],
            vec![1; 1_000_000],
            "[0, 1, 2, 3, 4, 5, 6, 7]",
        ),
        (vec![0; 1_000_000], vec![0; 999_999], "[999999]"),
        (vec![], vec![1], "[0]"),
    ] {
        let panic =
            std::panic::catch_unwind(|| assert_navigation_bytes(&actual, &expected, "test"))
                .expect_err("every unequal byte sequence must fail");
        let message = panic.downcast_ref::<String>().unwrap();
        assert!(message.len() < 400);
        assert!(message.contains("actual_len=") && message.contains("expected_sha256="));
        assert!(message.contains(&format!("first_offsets={offsets}")));
    }
}
