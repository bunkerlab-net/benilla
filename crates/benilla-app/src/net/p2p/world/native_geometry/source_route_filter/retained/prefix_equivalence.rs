// SPDX-License-Identifier: GPL-3.0-or-later
//! A1 root/predecessor prefix only, not the remaining G1 predicates or suspension.
use super::super::super::super::products::{
    source_height::freshness_timings, vmap::installed_source_fixture as input,
};
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::PathBuf,
};

// Literal HEAD predicates and source effects: neither oracle calls extracted helpers.
fn old_root_check_current(
    root: &ResidentRetainedRoot,
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    context: &AuthorityRequest,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
) -> Result<Instant, ResidentFilterError> {
    if !Rc::ptr_eq(&root.provider, &provider.identity) || root.context != *context {
        return Err(Failure::Binding.into());
    }
    let saved = provider.dispatch_for(&root.context, deadline, &cancelled)?;
    let current = provider.dispatch_for(context, deadline, &cancelled)?;
    if !saved.same_live_dispatch(current) {
        return Err(Failure::Binding.into());
    }
    let deadline = deadline.min(root.expires);
    provider.check_context(context, deadline, &cancelled)?;
    provider
        .height
        .verify_unchanged(&cancelled)
        .map_err(ResidentFilterError::Source)?;
    provider.check_context(context, deadline, &cancelled)?;
    Ok(deadline)
}

fn old_check_retained_predecessor(
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    prior: &ResidentJoinedColdDependency,
    context: &AuthorityRequest,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
) -> Result<(), ResidentFilterError> {
    provider.require_first_retained_calculation()?;
    provider.check_context(context, deadline, &cancelled)?;
    let previous = prior.retained_cold().historical_observation();
    if !Rc::ptr_eq(&provider.identity, &previous.identity) {
        return Err(Failure::Binding.into());
    }
    provider.check_context(&previous.context, deadline, &cancelled)?;
    let old = provider.dispatch_for(&previous.context, deadline, &cancelled)?;
    let current = provider.dispatch_for(context, deadline, &cancelled)?;
    let Query::SourceMotion(MotionQuery::Route(old_route)) = &previous.context.query else {
        return Err(Failure::Binding.into());
    };
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::UnsupportedQuery.into());
    };
    if !old.first_construction()
        || current.first_construction()
        || old.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
        || current.constructor_kind() != old.constructor_kind()
        || current.construction_binding() != old.construction_binding()
        || current.configuration_binding() != old.configuration_binding()
        || current.npc() != old.npc()
        || route.owner != old_route.owner
        || route.request.binding.query_id <= old_route.request.binding.query_id
        || route.request.binding.now_ms <= old_route.request.binding.now_ms
        || route.request.kind != motion::RouteKind::ChaseReplan
        || route.request.point_limit != motion::PATHFINDER_CACHE_POINT_LIMIT
        || old_route.request.prior_cache.is_some()
        || !prior.retained_cold().historical_calculate_ready()
    {
        return Err(Failure::Binding.into());
    }
    let native = prior.retained_cold().native_terminal().compatibility();
    if native.is_retained()
        || native.storage().len() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT) + 1
        || native.point_limit() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
        || prior.retained_cold().native_terminal().query_capacity() != 1024
        || native.before_active_len() != 0
        || native.before_storage() != [0; motion::PATHFINDER_CACHE_POINT_LIMIT as usize]
        || native.filter() != previous.filter
        || prior.retained_cold().native_terminal().path_type() != 1
        || native.active_len() != 1
        || prior.retained_cold().native_terminal().config()
            != (SourceSamePolygonConfig {
                normalize_z: false,
                ignore_normalization: false,
            })
    {
        return Err(Failure::Binding.into());
    }
    let cache = provider.retained_native_cache(
        prior.retained_cold().native_terminal(),
        &previous.context,
        1,
    )?;
    if route.request.prior_cache.as_deref() != Some(&cache) {
        return Err(Failure::Binding.into());
    }
    provider
        .height
        .verify_unchanged(&cancelled)
        .map_err(ResidentFilterError::Source)?;
    provider.check_context(context, deadline, &cancelled)
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    RootProvider,
    RootContext,
    RootExpiry,
    Deadline,
    PriorContext,
    SourceAt(usize),
    SourceThenCancel(usize, usize),
    CancelAtPoll(usize),
    PanicAtPoll(usize),
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

/// Diagnostic callbacks operate on the genuine un-staged retained result and
/// ACK-selected cold predecessor supplied by the existing ledger fixture.
pub(crate) fn compare(
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    dependency: &ResidentJoinedRetainedDependency,
    prior: &ResidentJoinedColdDependency,
    context: &AuthorityRequest,
    deadline: Instant,
    install: &input::Install,
) {
    let mut faults = vec![
        Fault::None,
        Fault::RootProvider,
        Fault::RootContext,
        Fault::RootExpiry,
        Fault::Deadline,
        Fault::PriorContext,
        Fault::SourceAt(1),
        Fault::SourceAt(2),
    ];
    let mut aliases: Option<[(SourceLocation, SourceLocation); 2]> = None;
    let mut index = 0;
    while index < faults.len() {
        let fault = faults[index];
        index += 1;
        let mut evidence = Vec::new();
        for old in [true, false] {
            let restore = RestoreSource {
                path: install.path().to_path_buf(),
                bytes: install.bytes(),
            };
            // Private rejection-only metadata. Genuine success always uses
            // dependency.root itself; no terminal or dependency is fabricated.
            let mut counterfeit = matches!(
                fault,
                Fault::RootProvider | Fault::RootContext | Fault::RootExpiry | Fault::PriorContext
            )
            .then(|| ResidentRetainedRoot {
                identity: Rc::clone(&dependency.root.identity),
                provider: Rc::clone(&dependency.root.provider),
                predecessor: Rc::clone(&dependency.root.predecessor),
                context: dependency.root.context.clone(),
                expires: dependency.root.expires,
            });
            let mut selected_context = context.clone();
            let mut selected_deadline = deadline;
            match fault {
                Fault::RootProvider => counterfeit.as_mut().unwrap().provider = Rc::new(()),
                Fault::RootContext => counterfeit.as_mut().unwrap().context.binding.tick += 1,
                Fault::RootExpiry => counterfeit.as_mut().unwrap().expires = Instant::now(),
                Fault::Deadline => selected_deadline = Instant::now(),
                Fault::PriorContext => {
                    selected_context = prior
                        .retained_cold()
                        .historical_observation()
                        .context
                        .clone();
                    counterfeit.as_mut().unwrap().context = selected_context.clone();
                }
                _ => {}
            }
            let root = counterfeit.as_ref().unwrap_or(&dependency.root);
            let expected_deadline = selected_deadline.min(root.expires);
            let polls = RefCell::new(Vec::new());
            let mutated = Cell::new(false);
            let cancelled = || {
                let slot = freshness_timings::current_source_trace_len();
                polls.borrow_mut().push(slot);
                if matches!(fault, Fault::PanicAtPoll(at) if polls.borrow().len() >= at) {
                    panic!("prefix oracle original cancellation-poll unwind");
                }
                let source_fault = match fault {
                    Fault::SourceAt(at) | Fault::SourceThenCancel(at, _) => slot >= at,
                    _ => false,
                };
                if source_fault && !mutated.replace(true) {
                    let mut entries = input::navigation_entries();
                    entries
                        .iter_mut()
                        .find(|(name, _)| name == input::WDT)
                        .unwrap()
                        .1[20] ^= 1;
                    install.write(&entries);
                }
                matches!(fault, Fault::CancelAtPoll(at) | Fault::SourceThenCancel(_, at)
                    if polls.borrow().len() >= at)
            };
            let ((outcome, counts), trace) = freshness_timings::trace_sources(|| {
                freshness_timings::measure_mode(freshness_timings::Mode::CountOnly, || {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        if old {
                            old_root_check_current(
                                root,
                                provider,
                                &selected_context,
                                selected_deadline,
                                cancelled,
                            )
                            .and_then(|deadline| {
                                old_check_retained_predecessor(
                                    provider,
                                    prior,
                                    &selected_context,
                                    deadline,
                                    cancelled,
                                )?;
                                Ok(deadline)
                            })
                        } else {
                            provider.check_retained_prefix(
                                root,
                                prior,
                                &selected_context,
                                selected_deadline,
                                &cancelled,
                            )
                        }
                    }))
                })
            });
            drop(restore);
            assert_eq!(
                mutated.get(),
                matches!(fault, Fault::SourceAt(_) | Fault::SourceThenCancel(_, _))
            );
            if matches!(fault, Fault::PanicAtPoll(_)) {
                assert!(outcome.is_err());
                evidence.push((
                    "original poll panic".to_owned(),
                    trace,
                    polls.into_inner(),
                    counts,
                ));
                continue;
            }
            let result = outcome.unwrap();
            if counterfeit.is_some() {
                assert!(
                    result.is_err(),
                    "counterfeit roots never establish successful evidence"
                );
            }
            match fault {
                Fault::None => {
                    assert_eq!(result.as_ref().unwrap(), &expected_deadline);
                    assert_eq!(trace.len(), 2);
                }
                Fault::RootProvider | Fault::RootContext => {
                    assert!(matches!(
                        &result,
                        Err(ResidentFilterError::Context(Failure::Binding))
                    ));
                    assert!(trace.is_empty());
                }
                Fault::RootExpiry | Fault::Deadline => {
                    assert!(matches!(
                        &result,
                        Err(ResidentFilterError::Context(Failure::Deadline))
                    ));
                    assert!(trace.is_empty());
                }
                Fault::PriorContext => {
                    assert!(matches!(
                        &result,
                        Err(ResidentFilterError::Context(Failure::Binding))
                    ));
                    assert_eq!(
                        trace.len(),
                        1,
                        "root passed; predecessor refuses cold dispatch"
                    );
                }
                Fault::SourceAt(at) | Fault::SourceThenCancel(at, _) => {
                    assert!(matches!(
                        &result,
                        Err(ResidentFilterError::Source(
                            source_height::CompositionError::TerrainSource
                        ))
                    ));
                    assert_eq!(trace.len(), at);
                }
                Fault::PanicAtPoll(_) => unreachable!("handled original-poll unwind above"),
                Fault::CancelAtPoll(_) => {
                    assert!(matches!(
                        &result,
                        Err(ResidentFilterError::Source(
                            source_height::CompositionError::Cancelled
                        ))
                    ));
                }
            }
            if let Fault::SourceThenCancel(_, cutoff) = fault {
                assert_eq!(
                    polls.borrow().len() + 1,
                    cutoff,
                    "source error skips later poll"
                );
            }
            evidence.push((format!("{result:?}"), trace, polls.into_inner(), counts));
        }
        if let Fault::SourceAt(slot) = fault {
            faults.push(Fault::SourceThenCancel(slot, evidence[0].2.len() + 1));
        }
        if matches!(fault, Fault::None) {
            let old = &evidence[0].1;
            let current = &evidence[1].1;
            assert_ne!(old[0], old[1]);
            assert_ne!(current[0], current[1]);
            for location in old {
                assert!(std::path::Path::new(location.0)
                    .ends_with("source_route_filter/retained/prefix_equivalence.rs"));
            }
            for location in current {
                assert!(
                    std::path::Path::new(location.0).ends_with("source_route_filter/retained.rs")
                );
            }
            let oracle_source = include_str!("prefix_equivalence.rs");
            let current_source = include_str!("../retained.rs");
            assert_effect_location(
                old[0],
                oracle_source,
                "fn old_root_check_current(",
                "fn old_check_retained_predecessor(",
            );
            assert_effect_location(
                old[1],
                oracle_source,
                "fn old_check_retained_predecessor(",
                "#[derive(Clone, Copy, Debug)]",
            );
            assert_effect_location(
                current[0],
                current_source,
                "impl RootCurrentState {",
                "struct PredecessorState {",
            );
            assert_effect_location(
                current[1],
                current_source,
                "impl PredecessorState {",
                "enum RetainedPrefixState {",
            );
            aliases = Some([(old[0], current[0]), (old[1], current[1])]);
            // First and last original cancellation poll around each source slot,
            // including pre-source predicates. This is not an all-predicate sweep.
            let polls = &evidence[0].2;
            let mut cutoffs = Vec::new();
            for slot in 0..=2 {
                cutoffs.push(polls.iter().position(|at| *at == slot).unwrap() + 1);
                cutoffs.push(polls.iter().rposition(|at| *at == slot).unwrap() + 1);
            }
            cutoffs.sort_unstable();
            cutoffs.dedup();
            faults.extend(cutoffs.into_iter().map(Fault::CancelAtPoll));
            for slot in 1..=2 {
                faults.push(Fault::PanicAtPoll(
                    polls.iter().position(|at| *at == slot).unwrap() + 1,
                ));
            }
        }
        let normalized: Vec<_> = evidence[0]
            .1
            .iter()
            .map(|location| {
                aliases
                    .as_ref()
                    .unwrap()
                    .iter()
                    .find_map(|(old, current)| (location == old).then_some(*current))
                    .unwrap_or(*location)
            })
            .collect();
        assert_eq!(evidence[0].0, evidence[1].0, "{fault:?}");
        assert_eq!(
            normalized, evidence[1].1,
            "only two exact source-callsite aliases: {fault:?}"
        );
        assert_eq!(
            evidence[0].2, evidence[1].2,
            "ordered original cancellation polls: {fault:?}"
        );
        assert!(evidence[0].3.same_counts(&evidence[1].3), "{fault:?}");
    }
    // The original standalone entry points still serve water/LOS and later
    // retained callers. Exercise each shared kernel without constructing again.
    for root_only in [true, false] {
        let mut evidence = Vec::new();
        for old in [true, false] {
            let polls = RefCell::new(Vec::new());
            let cancelled = || {
                polls
                    .borrow_mut()
                    .push(freshness_timings::current_source_trace_len());
                false
            };
            let ((result, counts), trace) = freshness_timings::trace_sources(|| {
                freshness_timings::measure_mode(freshness_timings::Mode::CountOnly, || {
                    match (root_only, old) {
                        (true, true) => old_root_check_current(
                            &dependency.root,
                            provider,
                            context,
                            deadline,
                            cancelled,
                        ),
                        (true, false) => dependency
                            .root
                            .check_current(provider, context, deadline, cancelled),
                        (false, true) => old_check_retained_predecessor(
                            provider, prior, context, deadline, cancelled,
                        )
                        .map(|()| deadline),
                        (false, false) => provider
                            .check_retained_predecessor(prior, context, deadline, cancelled)
                            .map(|()| deadline),
                    }
                })
            });
            assert!(result.is_ok());
            assert_eq!(trace.len(), 1);
            evidence.push((result.unwrap(), trace, polls.into_inner(), counts));
        }
        let alias = aliases.as_ref().unwrap()[usize::from(!root_only)];
        assert_eq!(evidence[0].1, vec![alias.0]);
        assert_eq!(evidence[1].1, vec![alias.1]);
        assert_eq!(evidence[0].0, evidence[1].0);
        assert_eq!(evidence[0].2, evidence[1].2);
        assert!(evidence[0].3.same_counts(&evidence[1].3));
    }
}
