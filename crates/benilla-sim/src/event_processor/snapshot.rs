// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded data-only checkpoints; restoring always revalidates queue ownership.
use super::*;
use serde::de::{Error as _, SeqAccess, Visitor};
use std::{collections::BTreeSet, fmt, marker::PhantomData};

pub const SNAPSHOT_VERSION: u32 = 1;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueEntry {
    pub deadline_ms: u64,
    pub insertion_id: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasicEventSnapshot {
    pub event_id: u64,
    pub kind: EventKind,
    pub add_time_ms: u64,
    pub exec_time_ms: u64,
    pub to_abort: bool,
    pub queue: Option<QueueEntry>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventProcessorSnapshot {
    pub version: u32,
    pub clock_ms: u64,
    pub next_insertion_id: u64,
    pub next_event_id: u64,
    /// Source sets this on KillAllEvents but AddEvent never consults it.
    pub aborting: bool,
    /// Canonical ascending (deadline, insertion). Identity/kind travel with it.
    #[serde(deserialize_with = "bounded_events")]
    pub queued: Vec<BasicEventSnapshot>,
    /// Execute(false) without re-add remains alive, sorted by stable identity.
    #[serde(deserialize_with = "bounded_events")]
    pub detached: Vec<BasicEventSnapshot>,
}
fn bounded_events<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BasicEventSnapshot>, D::Error> {
    struct Bounded(PhantomData<BasicEventSnapshot>);
    impl<'de> Visitor<'de> for Bounded {
        type Value = Vec<BasicEventSnapshot>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "at most {MAX_EVENTS} BasicEvents")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_EVENTS) {
                return Err(A::Error::custom("event snapshot count limit"));
            }
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(MAX_EVENTS));
            while let Some(event) = seq.next_element()? {
                if out.len() == MAX_EVENTS {
                    return Err(A::Error::custom("event snapshot count limit"));
                }
                out.push(event);
            }
            Ok(out)
        }
    }
    deserializer.deserialize_seq(Bounded(PhantomData))
}
impl EventProcessor {
    pub fn snapshot(&self) -> EventProcessorSnapshot {
        EventProcessorSnapshot {
            version: SNAPSHOT_VERSION,
            clock_ms: self.clock_ms,
            next_insertion_id: self.next_insertion_id,
            next_event_id: self.next_event_id,
            aborting: self.aborting,
            queued: self
                .queue
                .values()
                .map(|id| self.events[id].clone())
                .collect(),
            detached: self
                .events
                .values()
                .filter(|event| event.queue.is_none())
                .cloned()
                .collect(),
        }
    }
    pub fn restore(snapshot: EventProcessorSnapshot) -> Result<Self, EventError> {
        if snapshot.version != SNAPSHOT_VERSION
            || snapshot.next_insertion_id == 0
            || snapshot.next_event_id == 0
        {
            return Err(EventError::Invalid("snapshot version/insertion counter"));
        }
        if (snapshot.next_event_id == 1) != (snapshot.next_insertion_id == 1) {
            return Err(EventError::Invalid("snapshot allocator histories disagree"));
        }
        if snapshot.queued.len() > MAX_EVENTS
            || snapshot.detached.len() > MAX_EVENTS - snapshot.queued.len()
        {
            return Err(EventError::EventLimit);
        }
        if snapshot.next_insertion_id <= (snapshot.queued.len() + snapshot.detached.len()) as u64 {
            return Err(EventError::Invalid(
                "snapshot insertion history shorter than live set",
            ));
        }
        let mut restored = Self {
            clock_ms: snapshot.clock_ms,
            next_insertion_id: snapshot.next_insertion_id,
            next_event_id: snapshot.next_event_id,
            aborting: snapshot.aborting,
            events: BTreeMap::new(),
            queue: BTreeMap::new(),
        };
        let mut insertions = BTreeSet::new();
        let mut previous_queue = None;
        let mut previous_detached = 0;
        for (queued, events) in [(true, snapshot.queued), (false, snapshot.detached)] {
            for event in events {
                event.kind.validate()?;
                if event.event_id == 0
                    || event.event_id >= snapshot.next_event_id
                    || event.add_time_ms > snapshot.clock_ms
                    || restored.events.contains_key(&event.event_id)
                {
                    return Err(EventError::Invalid("snapshot identity/add time"));
                }
                match event.queue {
                    Some(entry) if queued => {
                        let key = (entry.deadline_ms, entry.insertion_id);
                        if entry.insertion_id == 0
                            || entry.insertion_id >= snapshot.next_insertion_id
                            || entry.deadline_ms != event.exec_time_ms
                            || !insertions.insert(entry.insertion_id)
                            || previous_queue.is_some_and(|previous| previous >= key)
                        {
                            return Err(EventError::Invalid("snapshot queue order/time/insertion"));
                        }
                        previous_queue = Some(key);
                        restored.queue.insert(key, event.event_id);
                    }
                    None if !queued => {
                        if event.event_id <= previous_detached
                            || event.exec_time_ms > snapshot.clock_ms
                        {
                            return Err(EventError::Invalid("snapshot detached order"));
                        }
                        previous_detached = event.event_id;
                    }
                    _ => return Err(EventError::Invalid("snapshot queued/detached ownership")),
                }
                restored.events.insert(event.event_id, event);
            }
        }
        Ok(restored)
    }
}
