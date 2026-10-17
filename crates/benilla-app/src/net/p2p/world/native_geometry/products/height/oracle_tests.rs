// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact source selection/call schedule only. Scripted callbacks are not VMap geometry.
use super::{select_height, Error, INVALID_HEIGHT_VALUE};

#[test]
fn complete_source_selector_matches_exact_height_and_ordered_query_bits() {
    let mut cases = 0;
    let mut by_call_count = [0usize; 5];
    for line in include_str!(
        "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/static-height.tsv"
    )
    .lines()
    .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 20, "{line}");
        assert_eq!(row[0].parse::<usize>().unwrap(), cases);
        let bits = |i| u32::from_str_radix(row[i], 16).unwrap();
        let value = |i| f32::from_bits(bits(i));
        let map = match row[2] {
            "0" => INVALID_HEIGHT_VALUE,
            "1" => value(3),
            _ => panic!("invalid map presence: {line}"),
        };
        let expected_count = row[10].parse::<usize>().unwrap();
        assert!((1..=4).contains(&expected_count), "{line}");
        let mut calls = 0;
        let result = select_height(map, value(4), value(5), |z, range| {
            assert!(calls < expected_count, "unexpected query: {line}");
            assert_eq!(
                [z.to_bits(), range.to_bits()],
                [bits(11 + calls * 2), bits(12 + calls * 2)],
                "query {calls}: {line}"
            );
            let reply = value(6 + calls);
            calls += 1;
            Ok(reply)
        })
        .unwrap();
        assert_eq!(calls, expected_count, "omitted source query: {line}");
        assert_eq!(result.to_bits(), bits(19), "result: {line}");
        for unused in calls..4 {
            assert_eq!([bits(11 + unused * 2), bits(12 + unused * 2)], [0, 0]);
        }
        by_call_count[calls] += 1;
        cases += 1;
    }
    assert_eq!(cases, 242);
    assert!(by_call_count[1..].iter().all(|count| *count > 0));
}

#[test]
fn a_callback_failure_stops_selection_at_each_possible_query_stage() {
    // This checks the Rust adapter's fail-closed behavior, not source C++ errors.
    // The original source methods return scalar heights; real callbacks can fail
    // due to coverage, range limits or a shared traversal/deadline budget.
    for error in [Error::Missing, Error::Unsupported, Error::Budget] {
        for fail_at in 0..4 {
            let mut calls = 0;
            let result = select_height(100.0, 0.0, 50.0, |_, _| {
                let current = calls;
                calls += 1;
                if current == fail_at {
                    Err(error)
                } else {
                    Ok(INVALID_HEIGHT_VALUE)
                }
            });
            assert_eq!(result, Err(error), "stage {fail_at}");
            assert_eq!(calls, fail_at + 1, "stage {fail_at}");
        }
    }
}
