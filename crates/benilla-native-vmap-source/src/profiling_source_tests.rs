// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source operations remain identical with diagnostics off/counting/timed.
use super::*;
use crate::profiling::{self, Counts, Mode, RuntimeScope};
use std::cell::Cell;

fn verify(
    source: &CapturedSource,
    mode: Mode,
    cutoff: usize,
) -> (Result<()>, usize, Counts) {
    let calls = Cell::new(0);
    let (result, counts) = profiling::measure(mode, || {
        let _runtime = RuntimeScope::enter();
        source.verify_unchanged(&|| {
            calls.set(calls.get() + 1);
            calls.get() == cutoff
        })
    });
    (result, calls.get(), counts)
}

#[test]
fn runtime_diagnostics_preserve_reads_receipts_and_every_cancellation_checkpoint() {
    let temp = Temp::new();
    let data = temp.data();
    store(
        &data,
        "model.MPQ",
        &[entry(MAP, 7), raw_entry("Dummy", vec![1])],
    );
    let (source, unmarked) = profiling::measure(Mode::Clocked, || {
        let mut source = CapturedSource::open(&data, &|| false).unwrap();
        assert!(matches!(source.read(MAP, &|| false).unwrap(), Lookup::Present(_)));
        assert!(matches!(source.read("Missing", &|| false).unwrap(), Lookup::Absent));
        assert!(matches!(source.read("Dummy", &|| false).unwrap(), Lookup::Dummy {}));
        source.verify_unchanged(&|| false).unwrap();
        source
    });
    // Discovery and its direct source replay do not become runtime measurements.
    assert_eq!(unmarked, Counts::default());
    let (original, total, disabled) = verify(&source, Mode::Off, usize::MAX);
    assert_eq!(original, Ok(()));
    assert_eq!(disabled, Counts::default());
    let (count_result, count_calls, counts) = verify(&source, Mode::CountOnly, usize::MAX);
    let (clock_result, clock_calls, clocked) = verify(&source, Mode::Clocked, usize::MAX);
    assert_eq!((count_result, count_calls), (original, total));
    assert_eq!((clock_result, clock_calls), (original, total));
    assert!(counts.same_counts(&clocked));
    assert_eq!(counts.verify.started, 1);
    assert_eq!(counts.verify.succeeded, 1);
    assert_eq!(counts.open_chain.succeeded, 1);
    assert_eq!(counts.directory_scan.succeeded, 1);
    assert_eq!(counts.metadata.started, MAX_ARCHIVES);
    assert_eq!(counts.archive_open_decode.succeeded, 1);
    assert_eq!(counts.receipt_probe.succeeded, 3);
    assert_eq!(counts.member_profile.succeeded, 1);
    assert_eq!(counts.payload_read.succeeded, 1);
    assert_eq!(counts.payload_hash.succeeded, 1);
    assert_eq!(counts.receipt_compare.succeeded, 3);
    assert_eq!(counts.verify.elapsed, std::time::Duration::ZERO);
    assert_eq!(counts.payload_read.elapsed, std::time::Duration::ZERO);
    for cutoff in 1..=total + 1 {
        let (off_result, off_calls, _) = verify(&source, Mode::Off, cutoff);
        let (count_result, count_calls, counts) = verify(&source, Mode::CountOnly, cutoff);
        let (clock_result, clock_calls, clocked) = verify(&source, Mode::Clocked, cutoff);
        assert_eq!((off_result, off_calls), (count_result, count_calls));
        assert_eq!((off_result, off_calls), (clock_result, clock_calls));
        assert!(counts.same_counts(&clocked));
        if cutoff <= total {
            assert_eq!(off_result, Err(SourceError::Cancelled));
        }
    }
    store(
        &data,
        "model.MPQ",
        &[entry(MAP, 9), raw_entry("Dummy", vec![1])],
    );
    for mode in [Mode::Off, Mode::CountOnly, Mode::Clocked] {
        let (result, _, counts) = verify(&source, mode, usize::MAX);
        assert_eq!(result, Err(SourceError::Changed));
        if mode != Mode::Off {
            assert_eq!(counts.verify.refused, 1);
            assert_eq!(counts.receipt_compare.refused, 1);
        }
    }
}
