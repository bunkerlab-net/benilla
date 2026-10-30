// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in, bounded component diagnostics. Timestamps never grant authority.
use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub(crate) enum Event {
    EnqueueBefore,
    EnqueueAfter,
    FirstDequeue,
    F05EnqueueBefore,
    F05EnqueueAfter,
    WorkerEntered,
    VerifyBefore,
    VerifyAfter,
    PublishBefore,
    PublishAfter,
    CompletionReceivedUnvalidated,
    FinalizeBefore,
    FinalizeAfter,
    StagedValidated,
    SliceGuard,
    ActiveGuardBefore,
    ActiveGuardAfter,
    SendBefore,
    SendAfter,
    ReceiveSelected,
    ReceiveGuard,
    ReceiveReturned,
}
const EVENTS: [Event; 22] = [
    Event::EnqueueBefore,
    Event::EnqueueAfter,
    Event::FirstDequeue,
    Event::F05EnqueueBefore,
    Event::F05EnqueueAfter,
    Event::WorkerEntered,
    Event::VerifyBefore,
    Event::VerifyAfter,
    Event::PublishBefore,
    Event::PublishAfter,
    Event::CompletionReceivedUnvalidated,
    Event::FinalizeBefore,
    Event::FinalizeAfter,
    Event::StagedValidated,
    Event::SliceGuard,
    Event::ActiveGuardBefore,
    Event::ActiveGuardAfter,
    Event::SendBefore,
    Event::SendAfter,
    Event::ReceiveSelected,
    Event::ReceiveGuard,
    Event::ReceiveReturned,
];

#[derive(Debug)]
pub(crate) struct RouteLatency {
    started: Instant,
    deadline: Instant,
    tick: u64,
    ticket: u64,
    stamps: [OnceLock<Instant>; EVENTS.len()],
    pub(crate) receive_outcome: OnceLock<&'static str>,
    pub(crate) send_outcome: OnceLock<&'static str>,
}
impl RouteLatency {
    pub(crate) fn new(started: Instant, deadline: Instant, tick: u64, ticket: u64) -> Self {
        Self {
            started,
            deadline,
            tick,
            ticket,
            stamps: std::array::from_fn(|_| OnceLock::new()),
            receive_outcome: OnceLock::new(),
            send_outcome: OnceLock::new(),
        }
    }
    pub(crate) fn mark(&self, event: Event) {
        self.at(event, Instant::now());
    }
    pub(crate) fn at(&self, event: Event, at: Instant) {
        // First occurrence only. Requeues neither reset the clock nor grow storage.
        let _ = self.stamps[event as usize].set(at);
    }
    fn get(&self, event: Event) -> Option<Instant> {
        self.stamps[event as usize].get().copied()
    }
    fn interval(&self, before: Event, after: Event) -> Option<Duration> {
        self.get(after)?.checked_duration_since(self.get(before)?)
    }
    fn publication_to_drain(&self) -> Option<(Option<Duration>, Duration)> {
        let drain = self.get(Event::CompletionReceivedUnvalidated)?;
        let upper = drain.checked_duration_since(self.get(Event::PublishBefore)?)?;
        // The owner may drain before the worker records send-after. Never wait
        // for that stamp, and do not mistake its absence for zero latency.
        let lower = self
            .get(Event::PublishAfter)
            .map(|after| drain.saturating_duration_since(after));
        Some((lower, upper))
    }
    fn print(&self) {
        let stamps: Vec<_> = EVENTS
            .iter()
            .map(|&event| {
                (
                    event,
                    self.get(event).map(|at| {
                        (
                            at.saturating_duration_since(self.started).as_micros(),
                            at >= self.deadline,
                        )
                    }),
                )
            })
            .collect();
        eprintln!(
            "F05 bounded first-occurrence latency (not final-attempt stamps): tick={} ticket={} budget_us={} stamps=(event, optional(offset_us, expired))={stamps:?} receive={:?} send={:?}; queue_upper={:?} worker_queue_upper={:?} verify={:?} publication_to_channel_receive_bounds={:?} channel_receive_to_finalize={:?} finalize={:?}; send/receive are observations, not authority; missing stamps remain missing",
            self.tick,
            self.ticket,
            self.deadline
                .saturating_duration_since(self.started)
                .as_micros(),
            self.receive_outcome.get(),
            self.send_outcome.get(),
            self.interval(Event::EnqueueBefore, Event::FirstDequeue),
            self.interval(Event::F05EnqueueBefore, Event::WorkerEntered),
            self.interval(Event::VerifyBefore, Event::VerifyAfter),
            self.publication_to_drain(),
            self.interval(Event::CompletionReceivedUnvalidated, Event::FinalizeBefore),
            self.interval(Event::FinalizeBefore, Event::FinalizeAfter)
        );
    }
}
/// Only the cold and first retained Route are relevant to the paired fixture.
/// No mutable queue, result channel, proof token or control wake is introduced.
#[derive(Default)]
pub(crate) struct Capture {
    records: [OnceLock<Arc<RouteLatency>>; 2],
}
impl Capture {
    pub(crate) fn insert(&self, index: usize, record: Arc<RouteLatency>) {
        if let Some(slot) = self.records.get(index) {
            let _ = slot.set(record);
        }
    }
    pub(crate) fn report_on_drop(self: &Arc<Self>) -> CaptureReport {
        CaptureReport(Arc::clone(self))
    }
    fn print(&self) {
        for record in self.records.iter().filter_map(OnceLock::get) {
            record.print();
        }
    }
}

/// Owned outside the measured thread scope: also prints on assertion/join unwind.
/// Reporting never waits for a missing timestamp or changes a control decision.
pub(crate) struct CaptureReport(Arc<Capture>);
impl Drop for CaptureReport {
    fn drop(&mut self) {
        self.0.print();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_bounds_handle_racing_and_missing_after_stamp() {
        let start = Instant::now();
        let record = RouteLatency::new(start, start + Duration::from_millis(500), 38, 1);
        record.at(Event::PublishBefore, start + Duration::from_millis(10));
        record.at(
            Event::CompletionReceivedUnvalidated,
            start + Duration::from_millis(12),
        );
        assert_eq!(
            record.publication_to_drain(),
            Some((None, Duration::from_millis(2)))
        );
        record.at(Event::PublishAfter, start + Duration::from_millis(13));
        assert_eq!(
            record.publication_to_drain(),
            Some((Some(Duration::ZERO), Duration::from_millis(2)))
        );
        record.at(
            Event::CompletionReceivedUnvalidated,
            start + Duration::from_millis(99),
        );
        assert_eq!(
            record.get(Event::CompletionReceivedUnvalidated),
            Some(start + Duration::from_millis(12))
        );
    }
    #[test]
    fn absent_events_and_bounded_capture_do_not_fabricate_evidence() {
        let start = Instant::now();
        let record = Arc::new(RouteLatency::new(
            start,
            start + Duration::from_millis(500),
            38,
            1,
        ));
        assert_eq!(record.publication_to_drain(), None);
        let capture = Capture::default();
        capture.insert(0, Arc::clone(&record));
        capture.insert(2, record);
        assert_eq!(
            capture
                .records
                .iter()
                .filter(|slot| slot.get().is_some())
                .count(),
            1
        );
    }
}
