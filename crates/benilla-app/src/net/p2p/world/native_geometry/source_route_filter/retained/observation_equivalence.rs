// SPDX-License-Identifier: GPL-3.0-or-later
//! F11 diagnostic observation seam only; earlier A1/prerequisite helpers are shared.
use super::super::super::super::products::{
    source_height::freshness_timings, vmap::installed_source_fixture as input,
};
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::PathBuf,
};

// Literal prior retained_for; changed F11 tail remains independent of new helpers.
fn old_retained_for(
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    dependency: &ResidentJoinedRetainedDependency,
    prior: &ResidentJoinedColdDependency,
    context: &AuthorityRequest,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
) -> Result<ResidentJoinedRetainedObservation, ResidentFilterError> {
    let deadline =
        provider.check_retained_prefix(&dependency.root, prior, context, deadline, &cancelled)?;
    if !Rc::ptr_eq(
        &dependency.root.predecessor,
        &prior
            .retained_cold()
            .historical_observation()
            .observation_identity,
    ) || !prior.retained_chase().same_water_model(&dependency.chase)
    {
        return Err(Failure::Binding.into());
    }
    let (_, water_ticket) = dependency.chase.retained_prerequisites(
        provider,
        &dependency.root,
        context,
        deadline,
        &cancelled,
    )?;
    let tickets = dependency
        .observations
        .each_ref()
        .map(|reply| reply.request().ticket);
    let masks = dependency
        .observations
        .each_ref()
        .map(|reply| reply.request().mask);
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::UnsupportedQuery.into());
    };
    let terminal = &dependency.terminal;
    let native = terminal.compatibility();
    let prior_native = prior.retained_cold().native_terminal().compatibility();
    let bits = |point: Point| [point.x.bits(), point.y.bits(), point.z.bits()];
    if &dependency.owner_ignore
        != route
            .require_owner_ignore_pathfinding(&provider.profile)
            .map_err(|_| Failure::Binding)?
        || dependency.owner_ignore.ignore_pathfinding()
        || !dependency.tiles.both_present()
        || tickets[0] != water_ticket.checked_add(1).ok_or(Failure::Invalid)?
        || tickets[1] != tickets[0].checked_add(1).ok_or(Failure::Invalid)?
        || masks != [15, 10]
        || dependency.observations.iter().any(|reply| {
            reply.request().binding != provider.height.binding()
                || reply.request().position != bits(route.owner_position)
        })
        || !native.is_retained()
        || native.before_storage() != prior_native.storage()
        || native.before_active_len() != prior_native.active_len()
        || native.filter() != prior_native.filter()
        || native.initial_storage() != prior_native.initial_storage()
        || native.initial_active_len() != prior_native.initial_active_len()
        || terminal.slot() != prior.retained_cold().native_terminal().slot()
        || terminal.query_capacity() != prior.retained_cold().native_terminal().query_capacity()
        || terminal.query_capacity() != 1024
        || terminal.config() != prior.retained_cold().native_terminal().config()
        || terminal.points() != [bits(route.request.origin), bits(route.request.goal)]
        || terminal.actual_end() != bits(route.request.goal)
        || terminal.path_type() != 1
        || native.active_len() != 1
        || native.point_limit() != usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
    {
        return Err(Failure::Binding.into());
    }
    let npc = context.binding.npc.ok_or(Failure::Binding)?;
    let body = Body {
        radius: provider
            .session
            .bodies
            .get(&npc.spawn)
            .ok_or(Failure::UnresolvedBody)?
            .radius,
        height: provider
            .initialized_height_for(context, deadline, &cancelled)?
            .ok_or(Failure::UnresolvedBody)?,
    };
    provider
        .height
        .verify_unchanged(&cancelled)
        .map_err(ResidentFilterError::Source)?;
    provider.check_context(context, deadline, &cancelled)?;
    Ok(ResidentJoinedRetainedObservation {
        terminal: ResidentSamePolygonObservation {
            points: terminal.points().try_into().map_err(|_| Failure::Binding)?,
            liquid_tickets: tickets,
            liquid_masks: masks,
            actual_end: terminal.actual_end(),
            path_type: terminal.path_type(),
            active_len: native.active_len(),
            filter: native.filter(),
            query_capacity: terminal.query_capacity(),
            before_storage_zero: native.before_active_len() == 0
                && native.before_storage().len()
                    == usize::from(motion::PATHFINDER_CACHE_POINT_LIMIT)
                && native.before_storage().iter().all(|id| *id == 0),
            compatibility_id: native.compatibility_id(),
            compatibility_version: native.compatibility_version(),
        },
        body,
    })
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    Context,
    QueryId,
    Deadline,
    Source,
    SourceThenCancel(usize),
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

/// Observe construction has already primed the real Some(job) invocation cache.
/// Both paired paths use that same warm cache; None remains the uncached path.
/// No reset, replacement dependency, native clone, staging or authority escapes.
pub(crate) fn compare(
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    dependency: &ResidentJoinedRetainedDependency,
    prior: &ResidentJoinedColdDependency,
    context: &AuthorityRequest,
    deadline: Instant,
    install: &input::Install,
) {
    let mut faults = vec![Fault::None, Fault::Context, Fault::QueryId, Fault::Deadline];
    let mut alias: Option<(SourceLocation, SourceLocation)> = None;
    let mut final_slot = 0;
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
            let mut selected_context = context.clone();
            let mut selected_deadline = deadline;
            match fault {
                Fault::Context => selected_context.binding.tick += 1,
                Fault::QueryId => {
                    let Query::SourceMotion(MotionQuery::Route(route)) = &mut selected_context.query
                    else {
                        unreachable!()
                    };
                    route.request.binding.query_id += 1;
                }
                Fault::Deadline => selected_deadline = Instant::now(),
                _ => {}
            }
            let polls = RefCell::new(Vec::new());
            let mutated = Cell::new(false);
            let cancelled = || {
                let slot = freshness_timings::current_source_trace_len();
                polls.borrow_mut().push(slot);
                if matches!(fault, Fault::PanicAtPoll(at) if polls.borrow().len() >= at) {
                    panic!("F11 original cancellation-poll unwind");
                }
                if matches!(fault, Fault::Source | Fault::SourceThenCancel(_))
                    && slot >= final_slot
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
                            old_retained_for(
                                provider,
                                dependency,
                                prior,
                                &selected_context,
                                selected_deadline,
                                cancelled,
                            )
                        } else {
                            provider.retained_for(
                                dependency,
                                prior,
                                &selected_context,
                                selected_deadline,
                                cancelled,
                            )
                        }
                    }))
                })
            });
            drop(restore);
            assert_eq!(
                mutated.get(),
                matches!(fault, Fault::Source | Fault::SourceThenCancel(_))
            );
            if matches!(fault, Fault::PanicAtPoll(_)) {
                assert!(outcome.is_err());
                assert_eq!(trace.len(), final_slot);
                evidence.push((
                    None,
                    "original poll panic".to_owned(),
                    trace,
                    polls.into_inner(),
                    counts,
                ));
                continue;
            }
            let result = outcome.unwrap();
            match fault {
                Fault::None => {
                    assert!(result.is_ok());
                    // This authored ClearConditional fixture has environment
                    // present: root, predecessor, root, LOS, environment x2, F11.
                    // Seven is not asserted for arbitrary absent-environment input.
                    assert_eq!(trace.len(), 7);
                }
                Fault::Context | Fault::QueryId => {
                    assert!(matches!(
                        &result,
                        Err(ResidentFilterError::Context(Failure::Binding))
                    ));
                    assert!(
                        trace.is_empty(),
                        "reachable context rejection, not a late ticket guard"
                    );
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
                    assert_eq!(trace.len(), final_slot);
                }
                Fault::CancelAtPoll(_) => {
                    assert!(matches!(
                        &result,
                        Err(ResidentFilterError::Source(
                            source_height::CompositionError::Cancelled
                        ))
                    ));
                    assert!((final_slot - 1..=final_slot).contains(&trace.len()));
                }
                Fault::PanicAtPoll(_) => unreachable!("handled original-poll unwind"),
            }
            if let Fault::SourceThenCancel(cutoff) = fault {
                assert_eq!(
                    polls.borrow().len() + 1,
                    cutoff,
                    "source error skips later poll"
                );
            }
            evidence.push((
                result.as_ref().ok().cloned(),
                format!("{result:?}"),
                trace,
                polls.into_inner(),
                counts,
            ));
        }
        if matches!(fault, Fault::None) {
            let old = &evidence[0].2;
            let current = &evidence[1].2;
            assert_eq!(old.len(), current.len());
            final_slot = old.len();
            assert_eq!(&old[..final_slot - 1], &current[..final_slot - 1]);
            let old_final = old[final_slot - 1];
            let current_final = current[final_slot - 1];
            assert!(old_final
                .0
                .ends_with("source_route_filter/retained/observation_equivalence.rs"));
            assert!(current_final.0.ends_with("source_route_filter/retained.rs"));
            assert_effect_location(
                old_final,
                include_str!("observation_equivalence.rs"),
                "fn old_retained_for(",
                "#[derive(Clone, Copy, Debug)]",
            );
            assert_effect_location(
                current_final,
                include_str!("../retained.rs"),
                "impl RetainedObservationState",
                "/// Own fresh prerequisites",
            );
            assert_ne!(old_final, current_final);
            alias = Some((old_final, current_final));
            let polls = &evidence[0].3;
            let first_final = polls.iter().position(|slot| *slot == final_slot).unwrap() + 1;
            let last_final = polls.iter().rposition(|slot| *slot == final_slot).unwrap() + 1;
            assert!(first_final > 1);
            faults.extend([
                Fault::Source,
                Fault::CancelAtPoll(first_final - 1),
                Fault::CancelAtPoll(first_final),
                Fault::CancelAtPoll(last_final),
                Fault::PanicAtPoll(first_final),
                Fault::PanicAtPoll(last_final),
            ]);
        }
        if matches!(fault, Fault::Source) {
            faults.push(Fault::SourceThenCancel(evidence[0].3.len() + 1));
        }
        let (old_final, current_final) = alias.unwrap();
        let normalized: Vec<_> = evidence[0]
            .2
            .iter()
            .map(|location| {
                if *location == old_final {
                    current_final
                } else {
                    *location
                }
            })
            .collect();
        assert_eq!(
            evidence[0].0, evidence[1].0,
            "complete diagnostic observation: {fault:?}"
        );
        assert_eq!(evidence[0].1, evidence[1].1, "full error payload: {fault:?}");
        assert_eq!(
            normalized, evidence[1].2,
            "one anchored F11 alias only: {fault:?}"
        );
        assert_eq!(
            evidence[0].3, evidence[1].3,
            "ordered original cancellation polls: {fault:?}"
        );
        assert!(evidence[0].4.same_counts(&evidence[1].4), "{fault:?}");
    }
}
