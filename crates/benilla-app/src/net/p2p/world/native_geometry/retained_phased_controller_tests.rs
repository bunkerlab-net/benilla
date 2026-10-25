// SPDX-License-Identifier: GPL-3.0-or-later
//! Two owner invocations in the actual FIFO, with unchanged synchronous comparisons.
use super::resident_result_controller_tests::{run, Mode, RunEvidence};

#[test]
fn retained_phased_fifo_saves_after_distinct_owner_turns() {
    run(Mode::Phased);
}

#[test]
#[ignore = "runner-sensitive default Route budget; run explicitly in release profile"]
fn retained_phased_default_budget_comparison() {
    compare(Mode::DefaultBudget, Mode::PhasedDefaultBudget);
}

#[test]
#[ignore = "runner-sensitive default Route budget; run explicitly in release profile"]
fn retained_phased_frame_cadenced_comparison() {
    compare(Mode::FrameCadenced, Mode::PhasedFrameCadenced);
}

fn compare(original: Mode, phased: Mode) {
    // Preserve either failure, but a runner-sensitive baseline must not hide the
    // changed path's independent deadline and contiguous owner-service evidence.
    let original = std::panic::catch_unwind(|| run(original));
    let phased = std::panic::catch_unwind(|| run(phased));
    let (Ok(original), Ok(phased)) = (original, phased) else {
        panic!("both baseline and phased runs must pass the original Work contract");
    };
    assert_original_trace(original, phased);
}

fn assert_original_trace(original: RunEvidence, phased: RunEvidence) {
    assert!(
        phased.max_owner_service < original.max_owner_service,
        "no measured owner-service reduction: baseline={:?}, phased={:?}",
        original.max_owner_service,
        phased.max_owner_service,
    );
    assert_eq!(original.retained_trace.len(), 87);
    assert_eq!(phased.retained_trace.len(), 93);
    let mut remaining = phased.retained_trace.iter();
    for check in original.retained_trace {
        assert!(
            remaining.any(|current| *current == check),
            "original source-check order changed at {check:?}",
        );
    }
}

#[test]
fn cold_phased_fifo_saves_after_distinct_owner_turns() {
    run(Mode::ColdPhased);
}

#[test]
#[ignore = "runner-sensitive default Route budget; run explicitly in release profile"]
fn cold_phased_default_budget_comparison() {
    compare_cold(Mode::PhasedDefaultBudget, Mode::ColdPhasedDefaultBudget);
}

#[test]
#[ignore = "runner-sensitive default Route budget; run explicitly in release profile"]
fn cold_phased_frame_cadenced_comparison() {
    compare_cold(Mode::PhasedFrameCadenced, Mode::ColdPhasedFrameCadenced);
}

fn compare_cold(original: Mode, phased: Mode) {
    // Both arms retain the existing retained split and original 500 ms Work lease.
    let original = std::panic::catch_unwind(|| run(original));
    let phased = std::panic::catch_unwind(|| run(phased));
    let (Ok(original), Ok(phased)) = (original, phased) else {
        panic!("both cold comparison arms must pass the original Work contract");
    };
    assert_eq!(original.cold_trace.len(), 71);
    assert_eq!(original.retained_trace, phased.retained_trace);
    let mut original_checks = original.cold_trace.iter().peekable();
    for (index, check) in phased.cold_trace.iter().enumerate() {
        let original_check = original_checks.peek().is_some_and(|next| *next == check);
        if original_check {
            original_checks.next();
        }
        eprintln!(
            "cold freshness comparison: index={index} original={original_check} check={check:?}"
        );
    }
    assert!(
        original_checks.next().is_none(),
        "original cold trace order changed"
    );
    eprintln!(
        "cold paired evidence: baseline_service={:?} split_service={:?} baseline_prepare_to_final={:?} split_prepare_to_final={:?} baseline_global={:?} split_global={:?} baseline_checks={} split_checks={}",
        original.max_cold_owner_service,
        phased.max_cold_owner_service,
        original.cold_latency,
        phased.cold_latency,
        original.max_owner_service,
        phased.max_owner_service,
        original.cold_trace.len(),
        phased.cold_trace.len(),
    );
    // Retained final is unchanged (203.530 ms in the motivating run). A cold
    // service improvement neither establishes a global improvement nor frame time.
    assert!(
        phased.max_cold_owner_service < original.max_cold_owner_service,
        "no measured cold owner-service reduction",
    );
}
