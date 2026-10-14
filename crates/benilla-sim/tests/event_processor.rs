// SPDX-License-Identifier: GPL-3.0-or-later
// Original synthetic callbacks only; no runtime admission, database or assets.
use benilla_sim::event_processor::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write};

#[derive(Clone)]
struct Callback {
    behavior: u64,
    arg: u64,
    deletable: bool,
}
fn drain(
    tx: &mut EventTransaction<'_>,
    callbacks: &mut BTreeMap<u64, Callback>,
    trace: &mut String,
) -> Result<(), EventError> {
    while let Some(event) = tx.pop_due()? {
        let id = event.event_id;
        let callback = callbacks.get_mut(&id).unwrap();
        let delete = match event.callback {
            CallbackKind::Execute => {
                writeln!(
                    trace,
                    "E {id} {} {} {} {}",
                    event.now_ms,
                    event.diff_ms.unwrap(),
                    event.add_time_ms,
                    event.exec_time_ms
                )
                .unwrap();
                match callback.behavior {
                    1 | 6 if callback.arg > 0 => {
                        callback.arg -= 1;
                        tx.re_add(id, event.now_ms + u64::from(callback.behavior == 6), false)?;
                        false
                    }
                    2 => false,
                    3 => {
                        tx.modify_event_time(callback.arg, event.now_ms)?;
                        true
                    }
                    4 => {
                        if tx.event(callback.arg).is_some() {
                            tx.set_aborted(callback.arg, true)?;
                        }
                        true
                    }
                    5 => {
                        let new_id = callback.arg;
                        tx.add_event(new_id, EventKind::AiNotify, event.now_ms)?;
                        callbacks.insert(
                            new_id,
                            Callback {
                                behavior: 0,
                                arg: 0,
                                deletable: true,
                            },
                        );
                        true
                    }
                    7 => {
                        assert!(tx.kill_event(id)?.is_none());
                        true
                    }
                    8 => {
                        assert!(!tx.modify_event_time(id, event.now_ms + 99)?);
                        true
                    }
                    9 => {
                        let target = callback.arg;
                        if tx.kill_event(target)?.is_some() {
                            writeln!(trace, "D {target}").unwrap();
                            callbacks.remove(&target);
                        }
                        true
                    }
                    _ => true,
                }
            }
            CallbackKind::Abort => {
                writeln!(
                    trace,
                    "A {id} {} {} {}",
                    event.now_ms, event.add_time_ms, event.exec_time_ms
                )
                .unwrap();
                let (behavior, arg) = (callback.behavior, callback.arg);
                if behavior == 10 {
                    callback.deletable = !callback.deletable;
                }
                let deletable = callback.deletable;
                match behavior {
                    3 => {
                        tx.modify_event_time(arg, event.now_ms)?;
                    }
                    4 => {
                        if tx.event(arg).is_some() {
                            tx.set_aborted(arg, true)?;
                        }
                    }
                    5 => {
                        tx.add_event(arg, EventKind::AiNotify, event.now_ms)?;
                        callbacks.insert(
                            arg,
                            Callback {
                                behavior: 0,
                                arg: 0,
                                deletable: true,
                            },
                        );
                    }
                    9 if tx.kill_event(arg)?.is_some() => {
                        writeln!(trace, "D {arg}").unwrap();
                        callbacks.remove(&arg);
                    }
                    _ => {}
                }
                deletable
            }
        };
        match event.callback {
            CallbackKind::Execute => tx.finish_execute(delete)?,
            CallbackKind::Abort => tx.finish_abort(delete)?,
        }
        if tx.event(id).is_none() {
            writeln!(trace, "D {id}").unwrap();
            callbacks.remove(&id);
        }
    }
    Ok(())
}
fn dump(queue: &EventProcessor, trace: &mut String) {
    let s = queue.snapshot();
    writeln!(
        trace,
        "S {} {} {} {}",
        s.clock_ms,
        u8::from(s.aborting),
        s.queued.len(),
        s.detached.len()
    )
    .unwrap();
    for event in s.queued {
        writeln!(
            trace,
            "Q {} {} {} {} {}",
            event.event_id,
            event.queue.unwrap().deadline_ms,
            event.add_time_ms,
            event.exec_time_ms,
            u8::from(event.to_abort)
        )
        .unwrap();
    }
    for event in s.detached {
        writeln!(
            trace,
            "H {} {} {} {}",
            event.event_id,
            event.add_time_ms,
            event.exec_time_ms,
            u8::from(event.to_abort)
        )
        .unwrap();
    }
}
fn replay_commands(commands: &[String], reopen_after_every_command: bool) -> String {
    let mut queue = EventProcessor::new();
    let mut callbacks = BTreeMap::new();
    let mut trace = String::new();
    for command in commands {
        writeln!(trace, ">{command}").unwrap();
        let mut words = command.split_whitespace();
        let op = words.next().unwrap();
        let values: Vec<u64> = words.map(|word| word.parse().unwrap()).collect();
        match op {
            "P" => {
                queue = EventProcessor::new();
                callbacks.clear();
            }
            "N" => {
                queue
                    .add_event(values[0], EventKind::AiNotify, values[1])
                    .unwrap();
                callbacks.insert(
                    values[0],
                    Callback {
                        behavior: values[2],
                        arg: values[3],
                        deletable: values[4] != 0,
                    },
                );
            }
            "M" => {
                queue.modify_event_time(values[0], values[1]).unwrap();
            }
            "B" => {
                if queue.event(values[0]).is_some() {
                    queue.set_aborted(values[0], values[1] != 0).unwrap();
                }
            }
            "L" => {
                if let Some(c) = callbacks.get_mut(&values[0]) {
                    c.deletable = values[1] != 0;
                }
            }
            "T" => {
                if let Some(c) = callbacks.get_mut(&values[0]) {
                    c.behavior = values[1];
                    c.arg = values[2];
                }
            }
            "R" => queue.re_add(values[0], values[1], values[2] != 0).unwrap(),
            "K" => {
                if queue.kill_event(values[0]).is_some() {
                    writeln!(trace, "D {}", values[0]).unwrap();
                    callbacks.remove(&values[0]);
                }
            }
            "U" | "X" => {
                // World/callback state and effects MUST stage beside the queue.
                let mut staged_callbacks = callbacks.clone();
                let mut staged_trace = String::new();
                let mut tx = if op == "U" {
                    queue.begin_update(values[0] as u32).unwrap()
                } else {
                    queue.begin_kill_all_events(values[0] != 0)
                };
                drain(&mut tx, &mut staged_callbacks, &mut staged_trace).unwrap();
                tx.commit().unwrap();
                callbacks = staged_callbacks;
                trace.push_str(&staged_trace);
            }
            _ => panic!("unexpected command {op}"),
        }
        if reopen_after_every_command {
            let bytes = serde_json::to_vec(&queue.snapshot()).unwrap();
            queue = EventProcessor::restore(serde_json::from_slice(&bytes).unwrap()).unwrap();
        }
        dump(&queue, &mut trace);
    }
    trace
}
#[test]
fn exact_pinned_cpp_differential_and_snapshot_replay() {
    let expected = oracle_text(include_str!("fixtures/event_processor_cmangos.tsv"));
    let commands: Vec<String> = expected
        .lines()
        .filter_map(|line| line.strip_prefix('>'))
        .map(str::to_owned)
        .collect();
    assert_eq!(replay_commands(&commands, false), expected);
    assert_eq!(replay_commands(&commands, true), expected);
}
// Git may check textual oracle fixtures out with CRLF on Windows. Canonicalize
// only that transport spelling; keep every row byte and final newline exact.
fn oracle_text(input: &str) -> String {
    let canonical = input.replace("\r\n", "\n");
    assert!(!canonical.contains('\r'), "bare CR in oracle fixture");
    canonical
}
#[test]
fn exact_source_oracle_accepts_crlf_checkout_without_changing_rows() {
    let lf = oracle_text(include_str!("fixtures/event_processor_cmangos.tsv"));
    let crlf = lf.replace('\n', "\r\n");
    assert_eq!(oracle_text(&crlf), lf);
    let commands: Vec<String> = crlf
        .lines()
        .filter_map(|line| line.strip_prefix('>'))
        .map(str::to_owned)
        .collect();
    assert_eq!(replay_commands(&commands, false), oracle_text(&crlf));
    assert_eq!(replay_commands(&commands, true), oracle_text(&crlf));
    assert_ne!(oracle_text("Q 1 5\r\n"), oracle_text("Q 1  5\r\n"));
    assert_ne!(oracle_text("Q 1 5\r\n"), oracle_text("Q 1 5"));
}
#[test]
#[should_panic(expected = "bare CR in oracle fixture")]
fn exact_source_oracle_rejects_bare_cr() {
    oracle_text("Q 1\r5\n");
}
fn generated_commands(seed: u32) -> Vec<String> {
    let mut commands = vec!["P".to_owned()];
    let mut state = seed + 1;
    let mut draw = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        state
    };
    for event in 1..=32 {
        commands.push(format!("N {event} {} 0 0 {}", draw() % 33, draw() % 2));
    }
    for _ in 0..96 {
        let action = draw() % 5;
        let event = 1 + draw() % 32;
        commands.push(match action {
            0 => format!("M {event} {}", draw() % 40),
            1 => format!("B {event} {}", draw() % 2),
            2 => format!("K {event}"),
            3 => format!("U {}", draw() % 3),
            _ => "X 0".to_owned(),
        });
    }
    commands.push("U 100".to_owned());
    commands.push("X 1".to_owned());
    commands
}
#[test]
fn generated_exact_source_traces_and_every_command_reopening_match() {
    for line in include_str!("fixtures/event_processor_cmangos_digests.tsv").lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let seed = fields[0].parse().unwrap();
        let commands = generated_commands(seed);
        assert_eq!(commands.len(), fields[1].parse::<usize>().unwrap());
        for reopen in [false, true] {
            let trace = replay_commands(&commands, reopen);
            assert_eq!(trace.lines().count(), fields[2].parse::<usize>().unwrap());
            assert_eq!(format!("{:x}", Sha256::digest(trace.as_bytes())), fields[3], "source trace seed={seed}, reopen={reopen}; regenerate with --trace-output for complete C++ traces");
        }
    }
}
fn initial() -> EventProcessor {
    let mut p = EventProcessor::new();
    p.add_event(1, EventKind::AiNotify, 5).unwrap();
    p.add_event(2, EventKind::SpellUpdate { cast_id: 90 }, 5)
        .unwrap();
    p
}
#[test]
fn now_whole_diff_deadline_insertion_kind_and_add_time_are_distinct() {
    let mut p = initial();
    p.modify_event_time(1, 5).unwrap();
    let mut tx = p.begin_update(100).unwrap();
    let e = tx.pop_due().unwrap().unwrap();
    assert_eq!(
        (
            e.event_id,
            e.now_ms,
            e.diff_ms,
            e.scheduled_ms,
            e.insertion_id,
            e.add_time_ms,
            e.exec_time_ms
        ),
        (2, 100, Some(100), 5, 2, 0, 5)
    );
    assert_eq!(e.kind, EventKind::SpellUpdate { cast_id: 90 });
    tx.re_add(2, 100, false).unwrap();
    tx.finish_execute(false).unwrap();
    let e = tx.pop_due().unwrap().unwrap();
    assert_eq!((e.event_id, e.insertion_id), (1, 3));
    tx.finish_execute(true).unwrap();
    let e = tx.pop_due().unwrap().unwrap();
    assert_eq!(
        (e.event_id, e.insertion_id, e.add_time_ms, e.exec_time_ms),
        (2, 4, 0, 100)
    );
    tx.re_add(2, 101, true).unwrap();
    tx.finish_execute(false).unwrap();
    tx.commit().unwrap();
    assert_eq!(p.event(2).unwrap().add_time_ms, 100);
    assert_eq!(p.event(2).unwrap().exec_time_ms, 101);
}
#[test]
fn mutation_errors_poison_and_cannot_publish_prior_callbacks() {
    let mut p = initial();
    let before = p.snapshot();
    let mut tx = p.begin_update(10).unwrap();
    tx.pop_due().unwrap();
    tx.finish_execute(true).unwrap();
    assert!(tx.re_add(2, 10, false).is_err());
    assert_eq!(tx.pop_due(), Err(EventError::PoisonedTransaction));
    assert_eq!(tx.commit(), Err(EventError::PoisonedTransaction));
    assert_eq!(p.snapshot(), before);
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct StagedHost {
    processor: EventProcessor,
    counter: u64,
    outputs: Vec<u64>,
}
impl StagedHost {
    fn update(&mut self, repeat_at_now: bool) -> Result<(), EventError> {
        // A minimal enclosing host transaction, not a production spell adapter.
        let mut staged = self.clone();
        let mut tx = staged.processor.begin_update(100)?;
        while let Some(event) = tx.pop_due()? {
            staged.counter += 1;
            staged.outputs.push(event.event_id);
            if repeat_at_now {
                tx.re_add(event.event_id, event.now_ms, false)?;
            }
            tx.finish_execute(!repeat_at_now)?;
        }
        tx.commit()?;
        *self = staged;
        Ok(())
    }
}
#[test]
fn execution_budget_rolls_back_queue_clock_callbacks_and_outputs() {
    let mut host = StagedHost {
        processor: initial(),
        counter: 7,
        outputs: vec![9],
    };
    let before = host.clone();
    assert_eq!(host.update(true), Err(EventError::ExecutionBudget));
    assert_eq!(host, before);
    // The same host really does publish successful callbacks and the new clock.
    host.update(false).unwrap();
    assert_eq!((host.counter, host.outputs), (9, vec![9, 1, 2]));
    assert_eq!(host.processor.clock_ms(), 100);
    assert!(host.processor.snapshot().queued.is_empty());
}
#[test]
fn incomplete_and_dropped_updates_do_not_advance_clock_or_delete_events() {
    let mut p = initial();
    let before = p.snapshot();
    assert_eq!(
        p.begin_update(10).unwrap().commit(),
        Err(EventError::IncompleteTransaction)
    );
    assert_eq!(p.snapshot(), before);
    {
        let mut tx = p.begin_update(10).unwrap();
        tx.pop_due().unwrap();
    }
    assert_eq!(p.snapshot(), before);
    let mut tx = p.begin_update(10).unwrap();
    tx.pop_due().unwrap();
    assert_eq!(tx.commit(), Err(EventError::IncompleteTransaction));
    assert_eq!(p.snapshot(), before);
}
#[test]
fn retained_detached_is_owned_bounded_and_replayable() {
    let mut p = initial();
    let mut tx = p.begin_update(10).unwrap();
    while tx.pop_due().unwrap().is_some() {
        tx.finish_execute(false).unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(p.snapshot().detached.len(), 2);
    assert!(p.kill_event(1).is_none());
    assert!(!p.modify_event_time(1, 0).unwrap());
    p = EventProcessor::restore(p.snapshot()).unwrap();
    p.re_add(1, 0, false).unwrap();
    assert_eq!(p.snapshot().queued[0].add_time_ms, 0);
}
#[test]
fn source_lifetime_and_iterator_misuse_fail_explicitly_and_atomically() {
    let mut p = initial();
    let before = p.snapshot();
    assert!(matches!(
        p.add_event(1, EventKind::AiNotify, 1),
        Err(EventError::Unsupported(_))
    ));
    assert!(matches!(
        p.re_add(1, 1, false),
        Err(EventError::Unsupported(_))
    ));
    assert_eq!(p.snapshot(), before);
    let mut tx = p.begin_update(10).unwrap();
    let e = tx.pop_due().unwrap().unwrap();
    tx.re_add(e.event_id, 11, false).unwrap();
    assert!(matches!(
        tx.finish_execute(true),
        Err(EventError::Unsupported(_))
    ));
    assert_eq!(tx.commit(), Err(EventError::PoisonedTransaction));
    assert_eq!(p.snapshot(), before);
    let mut tx = p.begin_kill_all_events(false);
    tx.pop_due().unwrap();
    assert!(matches!(
        tx.add_event(3, EventKind::AiNotify, 1),
        Err(EventError::Unsupported(_))
    ));
    assert_eq!(tx.commit(), Err(EventError::PoisonedTransaction));
    assert_eq!(p.snapshot(), before);
    let mut tx = p.begin_update(10).unwrap();
    tx.pop_due().unwrap();
    assert!(tx.kill_event(1).unwrap().is_none());
    tx.re_add(1, 11, false).unwrap();
    assert!(matches!(tx.kill_event(1), Err(EventError::Unsupported(_))));
    assert_eq!(tx.commit(), Err(EventError::PoisonedTransaction));
    assert_eq!(p.snapshot(), before);
}
#[test]
fn abort_flag_is_deferred_kill_is_immediate_and_nondeletable_due_still_deletes() {
    let mut p = initial();
    p.set_aborted(1, true).unwrap();
    p.kill_event(2).unwrap();
    let mut tx = p.begin_update(4).unwrap();
    assert!(tx.pop_due().unwrap().is_none());
    tx.commit().unwrap();
    let mut tx = p.begin_update(1).unwrap();
    assert_eq!(tx.pop_due().unwrap().unwrap().callback, CallbackKind::Abort);
    tx.finish_abort(false).unwrap();
    tx.commit().unwrap();
    assert!(p.snapshot().queued.is_empty());
}
#[test]
fn overflow_is_fail_closed_not_wrapping_or_partial() {
    let mut s = initial().snapshot();
    s.clock_ms = u64::MAX;
    let mut p = EventProcessor::restore(s).unwrap();
    let before = p.snapshot();
    assert!(matches!(p.begin_update(1), Err(EventError::Overflow(_))));
    assert!(matches!(p.calculate_time(1), Err(EventError::Overflow(_))));
    assert_eq!(p.snapshot(), before);
    let mut s = initial().snapshot();
    s.next_insertion_id = u64::MAX;
    let mut p = EventProcessor::restore(s).unwrap();
    let before = p.snapshot();
    assert!(matches!(
        p.add_event(3, EventKind::AiNotify, 0),
        Err(EventError::Overflow(_))
    ));
    assert!(matches!(
        p.modify_event_time(1, 0),
        Err(EventError::Overflow(_))
    ));
    assert_eq!(p.snapshot(), before);
    let mut tx = p.begin_update(5).unwrap();
    tx.pop_due().unwrap();
    assert!(matches!(
        tx.re_add(1, 0, false),
        Err(EventError::Overflow(_))
    ));
    assert_eq!(tx.commit(), Err(EventError::PoisonedTransaction));
    assert_eq!(p.snapshot(), before);
}
#[test]
fn full_queue_rejects_new_events_without_modifying_counter() {
    let mut p = EventProcessor::new();
    for id in 1..=MAX_EVENTS {
        p.add_event(id as u64, EventKind::AiNotify, 0).unwrap();
    }
    let before = p.snapshot();
    assert_eq!(
        p.add_event(MAX_EVENTS as u64 + 1, EventKind::AiNotify, 0),
        Err(EventError::EventLimit)
    );
    assert_eq!(p.snapshot(), before);
    let mut tx = p.begin_update(1).unwrap();
    tx.pop_due().unwrap();
    assert_eq!(
        tx.add_event(MAX_EVENTS as u64 + 1, EventKind::AiNotify, 0),
        Err(EventError::EventLimit)
    );
    assert_eq!(tx.commit(), Err(EventError::PoisonedTransaction));
    assert_eq!(p.snapshot(), before);
}
#[test]
fn malicious_snapshots_reject_counts_aliases_clocks_order_and_unknown_fields() {
    let original = initial().snapshot();
    type SnapshotMutation = Box<dyn Fn(&mut EventProcessorSnapshot)>;
    let mutate: Vec<SnapshotMutation> = vec![
        Box::new(|s| s.version += 1),
        Box::new(|s| s.next_insertion_id = 0),
        Box::new(|s| s.next_event_id = 0),
        Box::new(|s| s.next_event_id = 2),
        Box::new(|s| s.queued[0].event_id = 0),
        Box::new(|s| s.queued[0].event_id = s.queued[1].event_id),
        Box::new(|s| s.queued[0].kind = EventKind::SpellUpdate { cast_id: 0 }),
        Box::new(|s| s.queued[0].add_time_ms = 1),
        Box::new(|s| s.queued[0].exec_time_ms = 3),
        Box::new(|s| s.queued[0].queue.as_mut().unwrap().insertion_id = 0),
        Box::new(|s| s.queued[0].queue.as_mut().unwrap().insertion_id = s.next_insertion_id),
        Box::new(|s| s.queued[1].queue.as_mut().unwrap().insertion_id = 1),
        Box::new(|s| s.queued.swap(0, 1)),
        Box::new(|s| s.queued[0].queue = None),
        Box::new(|s| s.detached.push(s.queued[0].clone())),
        Box::new(|s| s.queued = vec![s.queued[0].clone(); MAX_EVENTS + 1]),
        Box::new(|s| s.detached = vec![s.queued[0].clone(); MAX_EVENTS]),
    ];
    for change in mutate {
        let mut s = original.clone();
        change(&mut s);
        assert!(EventProcessor::restore(s).is_err());
    }
    let mut value = serde_json::to_value(&original).unwrap();
    value["surprise"] = true.into();
    assert!(serde_json::from_value::<EventProcessorSnapshot>(value).is_err());
    let mut oversized = original.clone();
    oversized.queued = vec![oversized.queued[0].clone(); MAX_EVENTS + 1];
    assert!(serde_json::from_slice::<EventProcessorSnapshot>(
        &serde_json::to_vec(&oversized).unwrap()
    )
    .is_err());
}

#[test]
fn detached_metadata_and_insertion_history_are_revalidated() {
    let mut p = initial();
    let mut tx = p.begin_update(5).unwrap();
    while tx.pop_due().unwrap().is_some() {
        tx.finish_execute(false).unwrap();
    }
    tx.commit().unwrap();
    let s = p.snapshot();
    assert!(EventProcessor::restore(s.clone()).is_ok());
    let mut invalid = s.clone();
    invalid.detached[0].exec_time_ms = invalid.clock_ms + 1;
    assert!(EventProcessor::restore(invalid).is_err());
    let mut invalid = s.clone();
    invalid.next_insertion_id = 2;
    assert!(EventProcessor::restore(invalid).is_err());
    let mut invalid = s;
    invalid.detached.swap(0, 1);
    assert!(EventProcessor::restore(invalid).is_err());
}

#[test]
fn lifetime_ids_do_not_alias_replacements_or_replay() {
    let mut p = initial();
    p.kill_event(1).unwrap();
    p = EventProcessor::restore(p.snapshot()).unwrap();
    let before = p.snapshot();
    assert!(matches!(
        p.add_event(1, EventKind::AiNotify, 5),
        Err(EventError::Unsupported(_))
    ));
    assert_eq!(p.snapshot(), before);
    let mut tx = p.begin_update(5).unwrap();
    let old = tx.pop_due().unwrap().unwrap();
    let fresh_id = tx.next_event_id();
    tx.add_event(fresh_id, EventKind::AiNotify, 6).unwrap();
    tx.finish_execute(true).unwrap();
    tx.commit().unwrap();
    assert!(p.event(old.event_id).is_none());
    assert_eq!(p.event(fresh_id).unwrap().kind, EventKind::AiNotify);
    assert_eq!(p.next_event_id(), fresh_id + 1);
    assert!(p.kill_event(old.event_id).is_none());
    let before = p.snapshot();
    assert!(matches!(
        p.add_event(u64::MAX, EventKind::AiNotify, 7),
        Err(EventError::Overflow(_))
    ));
    assert_eq!(p.snapshot(), before);
}

#[test]
fn virgin_allocator_histories_must_agree() {
    let mut s = EventProcessor::new().snapshot();
    s.next_insertion_id = 99;
    assert!(EventProcessor::restore(s).is_err());
    let mut s = EventProcessor::new().snapshot();
    s.next_event_id = 99;
    assert!(EventProcessor::restore(s).is_err());
}
#[test]
fn wrong_callback_and_double_pop_poison_the_whole_update() {
    let mut p = initial();
    let before = p.snapshot();
    let mut tx = p.begin_update(5).unwrap();
    tx.pop_due().unwrap();
    assert!(tx.finish_abort(true).is_err());
    assert_eq!(tx.commit(), Err(EventError::PoisonedTransaction));
    assert_eq!(p.snapshot(), before);
    let mut tx = p.begin_update(5).unwrap();
    tx.pop_due().unwrap();
    assert_eq!(tx.pop_due(), Err(EventError::IncompleteTransaction));
    assert_eq!(tx.commit(), Err(EventError::PoisonedTransaction));
    assert_eq!(p.snapshot(), before);
}
