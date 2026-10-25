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
