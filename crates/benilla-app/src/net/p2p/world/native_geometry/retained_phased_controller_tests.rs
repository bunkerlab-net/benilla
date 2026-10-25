// SPDX-License-Identifier: GPL-3.0-or-later
//! Two owner invocations in the actual FIFO, with unchanged synchronous comparisons.
use super::{
    products::source_height::freshness_timings,
    resident_result_controller_tests::{run, run_profiled, Mode, RunEvidence},
};

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

/// Incremental activation cost in one instrumented binary, not a pre-patch baseline.
/// OFF leaves hooks present but inactive; no guessed overhead is subtracted.
#[test]
#[ignore = "runner-sensitive original 500ms Route budget; run explicitly in release profile"]
fn resident_observer_off_count_clocked_comparison() {
    use freshness_timings::{
        Mode::{Clocked, CountOnly, Off},
        Phase,
    };
    let mut outcomes = Vec::new();
    eprintln!(
        "resident observer control: same instrumented binary; OFF hooks remain present but counters/clocks are inactive, not a zero-overhead or pre-patch baseline"
    );
    for mode in [Off, CountOnly, Clocked] {
        let started = std::time::Instant::now();
        // Reserve outside every Route lease, and retain bounded records even
        // when the original deadline makes the fixture unwind.
        let mut phases_on_failure = Vec::with_capacity(4);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_profiled(mode, &mut phases_on_failure)
        }));
        eprintln!(
            "resident observer arm: mode={mode:?} success={} full_fixture_wall={:?}; unchanged500ms, no retry or overhead subtraction",
            outcome.is_ok(),
            started.elapsed(),
        );
        if let Ok(evidence) = &outcome {
            // Report after this arm's Work, save and ACK, before another arm can
            // fail. Formatting detailed records never enters a Route lease.
            eprintln!(
                "resident observer completed records: mode={mode:?} phases={:?}; inner guard/source categories are inclusive, outer intervals are disjoint",
                evidence.profile_phases,
            );
            eprintln!(
                "resident observer completed: mode={mode:?} cold_service={:?} cold_prepare_to_final={:?} global_service={:?}",
                evidence.max_cold_owner_service,
                evidence.cold_latency,
                evidence.max_owner_service,
            );
        } else {
            // Scoped controller threads and their Work have already unwound.
            eprintln!(
                "resident observer failed records: mode={mode:?} phases={phases_on_failure:?}; failure retained without retry"
            );
        }
        outcomes.push(outcome);
    }
    // Every arm is attempted exactly once, even if an observer pushes a previous
    // arm over its original deadline. Any such failure remains a failed test.
    assert!(
        outcomes.iter().all(Result::is_ok),
        "observer comparison failed an original500ms arm; inspect all three reported outcomes"
    );
    let mut outcomes = outcomes.into_iter().map(Result::unwrap);
    let off = outcomes.next().unwrap();
    let counted = outcomes.next().unwrap();
    let clocked = outcomes.next().unwrap();
    for evidence in [&off, &counted, &clocked] {
        assert_eq!(evidence.cold_trace.len(), 77);
        assert_eq!(evidence.retained_trace.len(), 93);
        assert_eq!(evidence.profile_phases.len(), 4);
    }
    assert_eq!(off.cold_trace, counted.cold_trace);
    assert_eq!(off.cold_trace, clocked.cold_trace);
    assert_eq!(off.retained_trace, counted.retained_trace);
    assert_eq!(off.retained_trace, clocked.retained_trace);
    for ((off, counted), clocked) in off
        .profile_phases
        .iter()
        .zip(&counted.profile_phases)
        .zip(&clocked.profile_phases)
    {
        assert!(off.succeeded && counted.succeeded && clocked.succeeded);
        assert_eq!((off.cold, off.finishing), (counted.cold, counted.finishing));
        assert_eq!((off.cold, off.finishing), (clocked.cold, clocked.finishing));
        assert_eq!(off.callback, Default::default());
        assert_eq!(off.collision, Default::default());
        assert_eq!(off.outer, Default::default());
        assert!(counted.callback.started(Phase::ProviderGuard) > 0);
        assert_eq!(counted.callback.started(Phase::PhaseEntryPrechecks), 1);
        assert!(counted.collision.verify.started > 0);
        assert!(counted.collision.open_chain.started > 0);
        assert!(counted.collision.receipt_probe.started > 0);
        let final_count = usize::from(counted.finishing);
        assert_eq!(
            counted.callback.started(Phase::ResultPrechecks),
            final_count
        );
        assert_eq!(
            counted.callback.started(Phase::CalculateCallback),
            final_count
        );
        assert_eq!(counted.callback.started(Phase::ResultSeal), final_count);
        assert_eq!(counted.outer.started(Phase::FifoDelivery), final_count);
        assert!(counted.callback.same_counts(&clocked.callback));
        assert!(counted.collision.same_counts(&clocked.collision));
        assert!(counted.outer.same_counts(&clocked.outer));
        eprintln!(
            "resident observer paired phase: cold={} final={} off_wall={:?} count_wall={:?} clocked_wall={:?}; inclusive child categories are not additive",
            off.cold,
            off.finishing,
            off.service_wall,
            counted.service_wall,
            clocked.service_wall,
        );
    }
}
