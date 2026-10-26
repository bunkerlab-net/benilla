// SPDX-License-Identifier: GPL-3.0-or-later
//! Standalone Filter-root LOS seam, not Retained-root/water/environment coverage.
use super::super::super::super::products::{
    source_height::freshness_timings, vmap::installed_source_fixture as input,
};
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::PathBuf,
};

// Literal HEAD los_for, independent of extracted pre/post helpers.
fn old_los_for(
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    dependency: &ResidentChaseLosDependency,
    context: &AuthorityRequest,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
) -> Result<ResidentChaseLosObservation, ResidentFilterError> {
    if !Rc::ptr_eq(&provider.identity, &dependency.identity) || dependency.context != *context {
        return Err(Failure::Binding.into());
    }
    let saved = provider.dispatch_for(&dependency.context, deadline, &cancelled)?;
    let current = provider.dispatch_for(context, deadline, &cancelled)?;
    if !saved.same_live_dispatch(current) {
        return Err(Failure::Binding.into());
    }
    let deadline = deadline.min(dependency.expires);
    provider.check_context(context, deadline, &cancelled)?;
    if provider.chase_model_for(context, dependency.player, deadline, &cancelled)?
        != dependency.model
    {
        return Err(Failure::Binding.into());
    }
    provider
        .height
        .verify_unchanged(&cancelled)
        .map_err(ResidentFilterError::Source)?;
    provider.check_context(context, deadline, &cancelled)?;
    Ok(dependency.observation)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    None,
    Fresh,
    Warm,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Fault {
    None,
    Context,
    Deadline,
    Source,
    SourceThenCancel(usize),
    CancelAtPoll(usize),
    PanicAtPoll(usize),
}

pub(crate) struct Evidence {
    result: Option<Result<ResidentChaseLosObservation, String>>,
    trace: freshness_timings::SourceTrace,
    polls: Vec<usize>,
    counts: freshness_timings::Counts,
}
struct RestoreSource {
    path: PathBuf,
    bytes: BTreeMap<String, Vec<u8>>,
}
impl Drop for RestoreSource {
    fn drop(&mut self) {
        for (name, bytes) in &self.bytes {
            std::fs::write(self.path.join(name), bytes).unwrap();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn arm(
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    dependency: &ResidentChaseLosDependency,
    context: &AuthorityRequest,
    deadline: Instant,
    install: &input::Install,
    mode: Mode,
    fault: Fault,
    old: bool,
) -> Evidence {
    assert!(matches!(&dependency.root, ResidentChaseLosRoot::Filter(_)));
    let restore = RestoreSource {
        path: install.path().to_path_buf(),
        bytes: install.bytes(),
    };
    let mut selected_context = context.clone();
    let selected_deadline = if matches!(fault, Fault::Deadline) {
        Instant::now()
    } else {
        deadline
    };
    if matches!(fault, Fault::Context) {
        selected_context.binding.tick += 1;
    }
    let polls = RefCell::new(Vec::new());
    let mutated = Cell::new(false);
    let cancelled = || {
        let slot = freshness_timings::current_source_trace_len();
        polls.borrow_mut().push(slot);
        if matches!(fault, Fault::PanicAtPoll(at) if polls.borrow().len() >= at) {
            panic!("LOS original cancellation-poll unwind");
        }
        if matches!(fault, Fault::Source | Fault::SourceThenCancel(_))
            && slot == 1
            && !mutated.replace(true)
        {
            let mut entries = input::navigation_entries();
            entries
                .iter_mut()
                .find(|(name, _)| name == input::WDT)
                .unwrap()
                .1[20] ^= 1;
            install.write(&entries);
        }
        matches!(fault, Fault::CancelAtPoll(at) | Fault::SourceThenCancel(at)
            if polls.borrow().len() >= at)
    };
    let ((outcome, counts), trace) = freshness_timings::trace_sources(|| {
        freshness_timings::measure_mode(freshness_timings::Mode::CountOnly, || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if old {
                    old_los_for(
                        provider,
                        dependency,
                        &selected_context,
                        selected_deadline,
                        cancelled,
                    )
                } else {
                    provider.los_for(dependency, &selected_context, selected_deadline, cancelled)
                }
            }))
        })
    });
    drop(restore);
    assert_eq!(
        mutated.get(),
        matches!(fault, Fault::Source | Fault::SourceThenCancel(_))
    );
    let result = if matches!(fault, Fault::PanicAtPoll(_)) {
        assert!(outcome.is_err());
        assert_eq!(trace.len(), 1);
        None
    } else {
        let result = outcome.unwrap();
        match fault {
            Fault::None => {
                assert_eq!(result.as_ref().unwrap(), &dependency.observation);
                assert_eq!(trace.len(), 1);
                assert_eq!(
                    counts.full_gate_model_calls(),
                    usize::from(mode != Mode::Warm)
                );
            }
            Fault::Context => {
                assert!(matches!(
                    &result,
                    Err(ResidentFilterError::Context(Failure::Binding))
                ));
                assert!(trace.is_empty());
            }
            Fault::Deadline => {
                assert!(matches!(
                    &result,
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));
                assert!(trace.is_empty());
            }
            Fault::Source | Fault::SourceThenCancel(_) => {
                assert!(matches!(
                    &result,
                    Err(ResidentFilterError::Source(
                        source_height::CompositionError::TerrainSource
                    ))
                ));
                assert_eq!(trace.len(), 1);
            }
            Fault::CancelAtPoll(_) => assert!(matches!(
                &result,
                Err(ResidentFilterError::Source(
                    source_height::CompositionError::Cancelled
                ))
            )),
            Fault::PanicAtPoll(_) => unreachable!("handled original-poll unwind"),
        }
        Some(result.map_err(|error| format!("{error:?}")))
    };
    if let Fault::SourceThenCancel(cutoff) = fault {
        assert_eq!(
            polls.borrow().len() + 1,
            cutoff,
            "source error skips later poll"
        );
    }
    Evidence {
        result,
        trace,
        polls: polls.into_inner(),
        counts,
    }
}

type SourceLocation = (&'static str, u32, u32);
fn assert_effect_location(location: SourceLocation, source: &str, start: &str, end: &str) {
    let begin = source.find(start).unwrap();
    let finish = begin + source[begin..].find(end).unwrap();
    let begin_line = source[..begin].lines().count() + 1;
    let end_line = source[..finish].lines().count() + 1;
    assert!((begin_line..end_line).contains(&(location.1 as usize)));
    assert_eq!(
        source[begin..finish].matches(".verify_unchanged(").count(),
        1
    );
    let line = source.lines().nth(location.1 as usize - 1).unwrap();
    assert!((1..=line.len() + 1).contains(&(location.2 as usize)));
}

/// Each run callback creates its own ordinary invocation. Old and current never
/// share a freshly populated cache; the Warm mode primes each independently.
pub(crate) fn compare(mut run: impl FnMut(Mode, Fault, bool) -> Evidence) {
    for mode in [Mode::None, Mode::Fresh, Mode::Warm] {
        let mut faults = vec![Fault::None, Fault::Context, Fault::Deadline];
        let mut alias = None;
        let mut index = 0;
        while index < faults.len() {
            let fault = faults[index];
            index += 1;
            let old = run(mode, fault, true);
            let current = run(mode, fault, false);
            if matches!(fault, Fault::None) {
                let old_source = old.trace[0];
                let current_source = current.trace[0];
                assert!(old_source.0.ends_with("source_route_filter/chase_los/equivalence.rs"));
                assert!(current_source.0.ends_with("source_route_filter/chase_los.rs"));
                assert_effect_location(
                    old_source,
                    include_str!("equivalence.rs"),
                    "fn old_los_for(",
                    "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
                );
                assert_effect_location(
                    current_source,
                    include_str!("../chase_los.rs"),
                    "pub(crate) fn los_for(",
                    "\n    }",
                );
                assert_ne!(old_source, current_source);
                alias = Some((old_source, current_source));
                let first_source = old.polls.iter().position(|slot| *slot == 1).unwrap() + 1;
                // The last slot-one poll is the original post-source
                // check_context poll, not a poll inside source verification.
                let post_source_poll = old.polls.iter().rposition(|slot| *slot == 1).unwrap() + 1;
                assert!(first_source > 1);
                faults.extend([
                    Fault::Source,
                    Fault::CancelAtPoll(first_source - 1),
                    Fault::CancelAtPoll(first_source),
                    Fault::CancelAtPoll(post_source_poll),
                    Fault::PanicAtPoll(first_source),
                    Fault::PanicAtPoll(post_source_poll),
                ]);
            }
            if matches!(fault, Fault::Source) {
                faults.push(Fault::SourceThenCancel(old.polls.len() + 1));
            }
            let (old_source, current_source) = alias.unwrap();
            let normalized: Vec<_> = old
                .trace
                .iter()
                .map(|location| {
                    if *location == old_source {
                        current_source
                    } else {
                        *location
                    }
                })
                .collect();
            assert_eq!(old.result, current.result, "{mode:?}/{fault:?}");
            assert_eq!(
                normalized, current.trace,
                "one anchored LOS alias: {mode:?}/{fault:?}"
            );
            assert_eq!(old.polls, current.polls, "ordered polls: {mode:?}/{fault:?}");
            assert!(old.counts.same_counts(&current.counts), "{mode:?}/{fault:?}");
        }
    }
}
