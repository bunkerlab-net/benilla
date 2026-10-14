// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS Project contributors; see ../../licenses/AUTHORS.md.
// Source GPL-2.0-or-later; Rust translation GPL-3.0-or-later. WITHOUT ANY WARRANTY.
//! Bounded per-Unit EventProcessor, not a spell scheduler or AI visitor.
//! See ../../EVENT-PROCESSOR.md for the source contract and unsupported lifetimes.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

mod snapshot;
pub use snapshot::{BasicEventSnapshot, EventProcessorSnapshot, QueueEntry};

pub const EVENT_PROCESSOR_POLICY_ID: &str = "cmangos-8ec338a1-event-processor-owned-v1";
pub const MAX_EVENTS: usize = 1024;
pub const MAX_CALLBACKS_PER_UPDATE: usize = 4096;

/// The enclosing Unit snapshot binds its owner and generation. A spell identity
/// binds to that Unit's separately validated cast state; it is not a raw pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EventKind {
    AiNotify,
    SpellUpdate { cast_id: u64 },
}
impl EventKind {
    fn validate(self) -> Result<(), EventError> {
        if matches!(self, Self::SpellUpdate { cast_id: 0 }) {
            return Err(EventError::Invalid("zero cast identity"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventError {
    Invalid(&'static str),
    Unsupported(&'static str),
    Overflow(&'static str),
    EventLimit,
    ExecutionBudget,
    PoisonedTransaction,
    IncompleteTransaction,
}
impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source event processor: {self:?}")
    }
}
impl std::error::Error for EventError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventProcessor {
    clock_ms: u64,
    next_insertion_id: u64,
    next_event_id: u64,
    aborting: bool,
    events: BTreeMap<u64, BasicEventSnapshot>,
    queue: BTreeMap<(u64, u64), u64>,
}
impl Default for EventProcessor {
    fn default() -> Self {
        Self::new()
    }
}
impl EventProcessor {
    pub fn new() -> Self {
        Self {
            clock_ms: 0,
            next_insertion_id: 1,
            next_event_id: 1,
            aborting: false,
            events: BTreeMap::new(),
            queue: BTreeMap::new(),
        }
    }
    pub fn clock_ms(&self) -> u64 {
        self.clock_ms
    }
    /// Persisted per-processor lifetime allocator; IDs are never reused.
    pub fn next_event_id(&self) -> u64 {
        self.next_event_id
    }
    pub fn calculate_time(&self, offset_ms: u64) -> Result<u64, EventError> {
        self.clock_ms
            .checked_add(offset_ms)
            .ok_or(EventError::Overflow("CalculateTime"))
    }
    pub fn event(&self, event_id: u64) -> Option<&BasicEventSnapshot> {
        self.events.get(&event_id)
    }
    /// A fresh BasicEvent's first add initializes m_addTime. Passing false on
    /// that first add would leave an uninitialized source value and is excluded.
    pub fn add_event(
        &mut self,
        event_id: u64,
        kind: EventKind,
        deadline_ms: u64,
    ) -> Result<(), EventError> {
        kind.validate()?;
        if event_id == 0 {
            return Err(EventError::Invalid("zero event identity"));
        }
        if self.events.contains_key(&event_id) {
            return Err(EventError::Unsupported(
                "duplicate live BasicEvent identity",
            ));
        }
        if event_id < self.next_event_id {
            return Err(EventError::Unsupported(
                "event identity reused or regressed",
            ));
        }
        let next_event_id = event_id
            .checked_add(1)
            .ok_or(EventError::Overflow("event identity"))?;
        if self.events.len() >= MAX_EVENTS {
            return Err(EventError::EventLimit);
        }
        let insertion_id = self.take_insertion_id()?;
        self.next_event_id = next_event_id;
        self.events.insert(
            event_id,
            BasicEventSnapshot {
                event_id,
                kind,
                add_time_ms: self.clock_ms,
                exec_time_ms: deadline_ms,
                to_abort: false,
                queue: Some(QueueEntry {
                    deadline_ms,
                    insertion_id,
                }),
            },
        );
        self.queue.insert((deadline_ms, insertion_id), event_id);
        Ok(())
    }
    /// Re-add an erased/retained event. A second queue entry for a live pointer
    /// permits source dangling-pointer/double-delete behavior and is rejected.
    pub fn re_add(
        &mut self,
        event_id: u64,
        deadline_ms: u64,
        set_add_time: bool,
    ) -> Result<(), EventError> {
        let event = self
            .events
            .get(&event_id)
            .ok_or(EventError::Invalid("unknown event"))?;
        if event.queue.is_some() {
            return Err(EventError::Unsupported(
                "multiple queue entries for one BasicEvent",
            ));
        }
        let insertion_id = self.take_insertion_id()?;
        let event = self.events.get_mut(&event_id).expect("validated event");
        if set_add_time {
            event.add_time_ms = self.clock_ms;
        }
        event.exec_time_ms = deadline_ms;
        event.queue = Some(QueueEntry {
            deadline_ms,
            insertion_id,
        });
        self.queue.insert((deadline_ms, insertion_id), event_id);
        Ok(())
    }
    /// Source ModifyEventTime reinserts after existing equal-deadline entries.
    /// A missing or detached pointer has no queued entry and is a no-op.
    pub fn modify_event_time(
        &mut self,
        event_id: u64,
        deadline_ms: u64,
    ) -> Result<bool, EventError> {
        let Some(old) = self.events.get(&event_id).and_then(|e| e.queue) else {
            return Ok(false);
        };
        let insertion_id = self.take_insertion_id()?;
        self.queue.remove(&(old.deadline_ms, old.insertion_id));
        self.queue.insert((deadline_ms, insertion_id), event_id);
        let event = self.events.get_mut(&event_id).expect("queued event");
        event.exec_time_ms = deadline_ms;
        event.queue = Some(QueueEntry {
            deadline_ms,
            insertion_id,
        });
        Ok(true)
    }
    /// Source to_Abort is a flag: cancellation stays queued until its deadline.
    pub fn set_aborted(&mut self, event_id: u64, aborted: bool) -> Result<(), EventError> {
        self.events
            .get_mut(&event_id)
            .ok_or(EventError::Invalid("unknown event"))?
            .to_abort = aborted;
        Ok(())
    }
    /// Source KillEvent deletes queued instances without invoking Abort.
    /// Event-specific destructor effects belong to the enclosing staged host.
    pub fn kill_event(&mut self, event_id: u64) -> Option<BasicEventSnapshot> {
        let old = self.events.get(&event_id)?.queue?;
        self.queue.remove(&(old.deadline_ms, old.insertion_id));
        self.events.remove(&event_id)
    }
    fn take_insertion_id(&mut self) -> Result<u64, EventError> {
        let next = self
            .next_insertion_id
            .checked_add(1)
            .ok_or(EventError::Overflow("insertion identity"))?;
        let id = self.next_insertion_id;
        self.next_insertion_id = next;
        Ok(id)
    }
    /// Nothing is published until commit. Drop, any mutation error, incomplete
    /// dispatch, and budget exhaustion leave this processor byte-for-byte intact.
    /// The caller must stage its callback/world effects in the same transaction.
    pub fn begin_update(&mut self, diff_ms: u32) -> Result<EventTransaction<'_>, EventError> {
        let mut staged = self.clone();
        staged.clock_ms = staged.calculate_time(u64::from(diff_ms))?;
        Ok(EventTransaction::new(
            self,
            staged,
            Mode::Update { diff_ms },
        ))
    }
    /// Abort callbacks run in queue order; IsDeletable is supplied AFTER Abort.
    /// Structural callback mutations are explicitly outside this iterator model.
    pub fn begin_kill_all_events(&mut self, force: bool) -> EventTransaction<'_> {
        let mut staged = self.clone();
        staged.aborting = true;
        let remaining = staged.queue.values().copied().collect();
        EventTransaction::new(self, staged, Mode::KillAll { force, remaining })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallbackKind {
    Execute,
    Abort,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DueEvent {
    pub event_id: u64,
    pub kind: EventKind,
    pub callback: CallbackKind,
    pub now_ms: u64,
    /// Execute receives the entire Unit update diff, never lateness or remainder.
    /// KillAllEvents has no update interval and reports None.
    pub diff_ms: Option<u32>,
    pub scheduled_ms: u64,
    pub insertion_id: u64,
    pub add_time_ms: u64,
    pub exec_time_ms: u64,
}
enum Mode {
    Update {
        diff_ms: u32,
    },
    KillAll {
        force: bool,
        remaining: VecDeque<u64>,
    },
}
#[must_use = "dispatch and commit the staged transaction, or drop it to roll back"]
pub struct EventTransaction<'a> {
    target: &'a mut EventProcessor,
    staged: EventProcessor,
    mode: Mode,
    pending: Option<DueEvent>,
    callbacks: usize,
    poisoned: bool,
}
impl<'a> EventTransaction<'a> {
    fn new(target: &'a mut EventProcessor, staged: EventProcessor, mode: Mode) -> Self {
        Self {
            target,
            staged,
            mode,
            pending: None,
            callbacks: 0,
            poisoned: false,
        }
    }
    pub fn clock_ms(&self) -> u64 {
        self.staged.clock_ms()
    }
    pub fn next_event_id(&self) -> u64 {
        self.staged.next_event_id()
    }
    pub fn event(&self, event_id: u64) -> Option<&BasicEventSnapshot> {
        self.staged.event(event_id)
    }
    pub fn calculate_time(&mut self, offset_ms: u64) -> Result<u64, EventError> {
        let result = self.staged.calculate_time(offset_ms);
        self.checked(result)
    }
    fn checked<T>(&mut self, result: Result<T, EventError>) -> Result<T, EventError> {
        if self.poisoned {
            return Err(EventError::PoisonedTransaction);
        }
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn can_mutate_queue(&mut self) -> Result<(), EventError> {
        let result = if matches!(self.mode, Mode::KillAll { .. }) {
            Err(EventError::Unsupported(
                "structural mutation during KillAllEvents",
            ))
        } else {
            Ok(())
        };
        self.checked(result)
    }
    pub fn add_event(&mut self, id: u64, kind: EventKind, at: u64) -> Result<(), EventError> {
        self.can_mutate_queue()?;
        let result = self.staged.add_event(id, kind, at);
        self.checked(result)
    }
    pub fn re_add(&mut self, id: u64, at: u64, set_add_time: bool) -> Result<(), EventError> {
        self.can_mutate_queue()?;
        let result = self.staged.re_add(id, at, set_add_time);
        self.checked(result)
    }
    pub fn modify_event_time(&mut self, id: u64, at: u64) -> Result<bool, EventError> {
        self.can_mutate_queue()?;
        let result = self.staged.modify_event_time(id, at);
        self.checked(result)
    }
    pub fn set_aborted(&mut self, id: u64, aborted: bool) -> Result<(), EventError> {
        self.checked(Ok(()))?;
        let result = self.staged.set_aborted(id, aborted);
        self.checked(result)
    }
    pub fn kill_event(&mut self, id: u64) -> Result<Option<BasicEventSnapshot>, EventError> {
        self.can_mutate_queue()?;
        if self.pending.is_some_and(|event| event.event_id == id)
            && self
                .staged
                .event(id)
                .is_some_and(|event| event.queue.is_some())
        {
            return self.checked(Err(EventError::Unsupported(
                "delete currently executing event",
            )));
        }
        Ok(self.staged.kill_event(id))
    }
    pub fn pop_due(&mut self) -> Result<Option<DueEvent>, EventError> {
        self.checked(Ok(()))?;
        if self.pending.is_some() {
            return self.checked(Err(EventError::IncompleteTransaction));
        }
        let (event_id, diff_ms) = match &mut self.mode {
            Mode::Update { diff_ms } => {
                let Some((&(deadline, _), &id)) = self.staged.queue.first_key_value() else {
                    return Ok(None);
                };
                if deadline > self.staged.clock_ms {
                    return Ok(None);
                }
                (id, Some(*diff_ms))
            }
            Mode::KillAll { remaining, .. } => {
                let Some(&id) = remaining.front() else {
                    return Ok(None);
                };
                (id, None)
            }
        };
        if self.callbacks >= MAX_CALLBACKS_PER_UPDATE {
            return self.checked(Err(EventError::ExecutionBudget));
        }
        let event = self
            .staged
            .events
            .get_mut(&event_id)
            .expect("owned queued event");
        let queue = event.queue.expect("queued event");
        if let Mode::KillAll { remaining, .. } = &mut self.mode {
            remaining.pop_front();
            event.to_abort = true;
        } else {
            event.queue = None;
            self.staged
                .queue
                .remove(&(queue.deadline_ms, queue.insertion_id));
        }
        let due = DueEvent {
            event_id,
            kind: event.kind,
            callback: if event.to_abort {
                CallbackKind::Abort
            } else {
                CallbackKind::Execute
            },
            now_ms: self.staged.clock_ms,
            diff_ms,
            scheduled_ms: queue.deadline_ms,
            insertion_id: queue.insertion_id,
            add_time_ms: event.add_time_ms,
            exec_time_ms: event.exec_time_ms,
        };
        self.pending = Some(due);
        self.callbacks += 1;
        Ok(Some(due))
    }
    /// Supply Execute's bool. False without re-add retains a detached event.
    pub fn finish_execute(&mut self, delete: bool) -> Result<(), EventError> {
        self.finish_callback(CallbackKind::Execute, delete)
    }
    /// Supply IsDeletable after an Abort callback for non-force KillAllEvents.
    /// Update's aborted-event branch and force KillAll delete unconditionally.
    pub fn finish_abort(&mut self, is_deletable: bool) -> Result<(), EventError> {
        self.finish_callback(CallbackKind::Abort, is_deletable)
    }
    fn finish_callback(&mut self, kind: CallbackKind, result: bool) -> Result<(), EventError> {
        self.checked(Ok(()))?;
        let Some(due) = self.pending else {
            return self.checked(Err(EventError::Invalid("no pending callback")));
        };
        if due.callback != kind {
            return self.checked(Err(EventError::Invalid("wrong callback result")));
        }
        let delete = match self.mode {
            Mode::Update { .. } => kind == CallbackKind::Abort || result,
            Mode::KillAll { force, .. } => force || result,
        };
        if delete {
            let event = self
                .staged
                .events
                .get(&due.event_id)
                .expect("pending event");
            if matches!(self.mode, Mode::Update { .. }) && event.queue.is_some() {
                return self.checked(Err(EventError::Unsupported(
                    "delete after re-add leaves a dangling source pointer",
                )));
            }
            if let Some(q) = event.queue {
                self.staged.queue.remove(&(q.deadline_ms, q.insertion_id));
            }
            self.staged.events.remove(&due.event_id);
        }
        self.pending = None;
        Ok(())
    }
    pub fn commit(self) -> Result<(), EventError> {
        if self.poisoned {
            return Err(EventError::PoisonedTransaction);
        }
        let remaining = match &self.mode {
            Mode::Update { .. } => self
                .staged
                .queue
                .first_key_value()
                .is_some_and(|(&(at, _), _)| at <= self.staged.clock_ms),
            Mode::KillAll { remaining, .. } => !remaining.is_empty(),
        };
        if self.pending.is_some() || remaining {
            return Err(EventError::IncompleteTransaction);
        }
        *self.target = self.staged;
        Ok(())
    }
}
