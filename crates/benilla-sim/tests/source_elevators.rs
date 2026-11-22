// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::source_transports::{elevator_position, Error};

#[test]
fn authored_pinned_builder_lerp_quaternion_sign_flip_and_origin_bits() {
    // G3D normalizes before the left-vector matrix product; Matrix3.cpp:117-136, Matrix3.h:349-356 at 8ec338a1.
    let corpus = include_str!("data/source_elevators.tsv");
    let mut count = 0;
    for line in corpus.lines() {
        let words: Vec<u32> = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        assert_eq!(words.len(), 19);
        let actual = elevator_position(
            words[..3].try_into().unwrap(),
            words[3..6].try_into().unwrap(),
            words[6],
            words[7],
            words[8],
            words[9..13].try_into().unwrap(),
            words[13..16].try_into().unwrap(),
        )
        .unwrap();
        assert_eq!(actual, words[16..19], "case {count}");
        count += 1;
    }
    assert_eq!(count, 105);
}

#[test]
fn equal_nodes_bypass_zero_denominator_but_distinct_nodes_do_not() {
    // Equal coordinates skip the float quotient; Transports.cpp:468-474 at 8ec338a1.
    let p = [1.0_f32.to_bits(), 2.0_f32.to_bits(), 3.0_f32.to_bits()];
    let q = [0, 0, 0, 1.0_f32.to_bits()];
    assert!(elevator_position(p, p, 1, 0, 0, q, [0; 3]).is_ok());
    assert_eq!(
        elevator_position(p, [0; 3], 1, 0, 0, q, [0; 3]),
        Err(Error::NumericDomain)
    );
}
