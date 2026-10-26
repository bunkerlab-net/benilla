// SPDX-License-Identifier: GPL-3.0-or-later
//! Characterize the two original wrapper rules against real authored source reads.
//! The F05 pattern below is not a replacement for retained_for or its predicates.
use super::super::super::vmap::installed_source_fixture as input;
use super::super::freshness_timings::trace_sources;
use super::*;
use benilla_native_vmap_source::{MapBundleError, SourceError};
use std::{cell::Cell, time::Duration};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

// F05's source-result-first rule, with the same poll implementation on each side.
// F04 is exercised by calling the actual inventory::fresh, never a copied body.
fn source_first(
    view: &PreparedSourceHeight<'_, '_, '_>,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<(), CompositionError> {
    poll(deadline, cancelled)?;
    view.verify_unchanged(|| cancelled() || Instant::now() >= deadline)?;
    poll(deadline, cancelled)
}

fn run(
    view: &PreparedSourceHeight<'_, '_, '_>,
    source_first_rule: bool,
    cutoff: usize,
    deadline: Instant,
) -> (Result<(), CompositionError>, usize, usize) {
    let polls = Cell::new(0);
    let cancelled = || {
        polls.set(polls.get() + 1);
        polls.get() >= cutoff
    };
    let (result, trace) = trace_sources(|| {
        if source_first_rule {
            source_first(view, deadline, &cancelled)
        } else {
            fresh(view, deadline, &cancelled)
        }
    });
    (result, polls.get(), trace.len())
}

fn with_view(run: impl FnOnce(input::Install, &PreparedSourceHeight<'_, '_, '_>)) {
    let install = input::install();
    let terrain =
        benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles()).unwrap();
    let discovery = benilla_native_vmap_source::prepare(install.path(), 0, || false).unwrap();
    let collision = discovery
        .into_map_bundle(input::profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &collision, || false).unwrap();
    run(install, &view);
}

#[test]
fn inventory_f04_and_f05_poll_order_on_live_sources() {
    with_view(|_install, view| {
        let (result, total, trace) = run(view, false, usize::MAX, deadline());
        result.unwrap();
        assert_eq!(trace, 1);
        assert!(total > 3);
        let (result, source_first_total, trace) = run(view, true, usize::MAX, deadline());
        result.unwrap();
        assert_eq!((source_first_total, trace), (total, 1));
        // Entry, source-entry, first terrain checkpoint, last source checkpoint,
        // wrapper postpoll, and the first unobserved cutoff. Not every I/O cutoff.
        for cutoff in [1, 2, 3, total - 1, total, total + 1] {
            for source_first_rule in [false, true] {
                let (result, polls, trace) = run(view, source_first_rule, cutoff, deadline());
                if cutoff > total {
                    result.unwrap();
                    assert_eq!(polls, total);
                } else {
                    assert!(matches!(result, Err(CompositionError::Cancelled)));
                    // F04 polls once more even after source cancellation.
                    let expected =
                        cutoff + usize::from(!source_first_rule && cutoff > 1 && cutoff < total);
                    assert_eq!(polls, expected);
                }
                assert_eq!(trace, usize::from(cutoff > 1));
            }
        }
    });
}

#[test]
fn inventory_f04_postpoll_masks_real_source_fault_but_f05_preserves_it() {
    #[derive(Clone, Copy, Debug)]
    enum Fault {
        CollisionMember,
        TerrainMember,
        MissingArchive,
    }
    for fault in [
        Fault::CollisionMember,
        Fault::TerrainMember,
        Fault::MissingArchive,
    ] {
        with_view(|install, view| {
            match fault {
                // The exterior ADT is captured by collision, not the terrain guards.
                Fault::CollisionMember => install.write(&input::overlapping_entries()),
                Fault::TerrainMember => {
                    let original = input::entries();
                    let mut changed = original.clone();
                    let wdt = &mut changed
                        .iter_mut()
                        .find(|(name, _)| name == input::WDT)
                        .unwrap()
                        .1;
                    wdt[20] ^= 1;
                    let differences: Vec<_> = original
                        .iter()
                        .zip(&changed)
                        .filter(|(before, after)| before != after)
                        .map(|(_, (name, _))| name.as_str())
                        .collect();
                    assert_eq!(differences, [input::WDT]);
                    install.write(&changed);
                }
                Fault::MissingArchive => drop(install),
            }
            let assert_source_error = |result: &Result<(), CompositionError>| {
                let expected = match fault {
                    Fault::CollisionMember => matches!(
                        result,
                        Err(CompositionError::Collision(PrepareError::Source(
                            MapBundleError::Source(SourceError::Changed)
                        )))
                    ),
                    Fault::TerrainMember | Fault::MissingArchive => {
                        matches!(result, Err(CompositionError::TerrainSource))
                    }
                };
                assert!(expected, "{fault:?}: {result:?}");
            };
            // Measure each genuine source failure before selecting its postpoll.
            let (result, baseline_polls, trace) = run(view, false, usize::MAX, deadline());
            assert_source_error(&result);
            assert_eq!(trace, 1);
            assert!(baseline_polls > 2);
            let (result, polls, trace) = run(view, true, usize::MAX, deadline());
            assert_source_error(&result);
            assert_eq!((polls, trace), (baseline_polls - 1, 1));

            let (result, polls, trace) = run(view, false, baseline_polls, deadline());
            assert!(
                matches!(result, Err(CompositionError::Cancelled)),
                "{fault:?}: {result:?}"
            );
            assert_eq!((polls, trace), (baseline_polls, 1));
            let (result, polls, trace) = run(view, true, baseline_polls, deadline());
            assert_source_error(&result);
            assert_eq!((polls, trace), (baseline_polls - 1, 1));
        });
    }
}

#[test]
fn inventory_f04_prefix_cancel_and_deadline_skip_source() {
    with_view(|_install, view| {
        for source_first_rule in [false, true] {
            let expired = Instant::now();
            let (result, polls, trace) = run(view, source_first_rule, usize::MAX, expired);
            assert!(matches!(
                result,
                Err(CompositionError::Validation(Error::Deadline))
            ));
            assert_eq!((polls, trace), (1, 0));
            let (result, polls, trace) = run(view, source_first_rule, 1, expired);
            assert!(matches!(result, Err(CompositionError::Cancelled)));
            assert_eq!((polls, trace), (1, 0));
            let (result, polls, trace) = run(view, source_first_rule, 1, deadline());
            assert!(matches!(result, Err(CompositionError::Cancelled)));
            assert_eq!((polls, trace), (1, 0));
        }
    });
}
