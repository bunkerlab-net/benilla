// SPDX-License-Identifier: GPL-3.0-or-later
//! Logical threads preserve queue choices without imposing host completion order.
use super::*;

pub(super) struct FifoJobs<T>(VecDeque<(u64, T)>);
impl<T> Default for FifoJobs<T> {
    fn default() -> Self {
        Self(VecDeque::new())
    }
}
impl<T> FifoJobs<T> {
    pub(super) fn push(&mut self, id: u64, value: T) {
        self.0.push_back((id, value));
    }
    // Validate the next recipient choice against the actual queue; ProducerConsumerQueue.h:64-78.
    pub(super) fn pop(&mut self, expected: u64) -> Result<T> {
        require(
            self.0.front().is_some_and(|(id, _)| *id == expected),
            "execution.fifo_dequeue",
        )?;
        self.pop_next()
    }
    pub(super) fn front(&self) -> Option<&T> {
        self.0.front().map(|(_, value)| value)
    }
    pub(super) fn pop_next(&mut self) -> Result<T> {
        self.0
            .pop_front()
            .map(|(_, value)| value)
            .ok_or_else(|| refuse("execution.queue_empty"))
    }
}
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadRole {
    MapWorker,
    StartupMap(MapIdentity),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadBirth {
    pub thread: u64,
    pub creator: u64,
    pub ordinal: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapJob {
    pub id: u64,
    pub map: MapIdentity,
    pub diff: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleAction {
    Birth(ThreadBirth, ThreadRole),
    Submit(MapJob),
    Dispatch(MapJob),
    Complete(MapJob),
    Barrier,
    Exit,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleEdge {
    pub id: u64,
    pub thread: u64,
    pub causes: Vec<u64>,
    pub action: ScheduleAction,
}
struct Thread {
    role: ThreadRole,
    birth: u64,
    running: Option<(MapJob, u64)>,
    alive: bool,
}
#[derive(Default)]
pub struct SourceScheduler {
    threads: BTreeMap<u64, Thread>,
    births: BTreeSet<(u64, u64)>,
    queue: FifoJobs<(MapJob, u64)>,
    jobs: BTreeSet<u64>,
    pending: BTreeSet<u64>,
    completions: Vec<u64>,
    previous_pop: Option<u64>,
    sequence: u64,
    last_thread_event: BTreeMap<u64, u64>,
}
impl SourceScheduler {
    fn edge(&mut self, thread: u64, mut causes: Vec<u64>, action: ScheduleAction) -> ScheduleEdge {
        if let Some(prior) = self.last_thread_event.get(&thread) {
            causes.push(*prior);
        }
        causes.sort_unstable();
        causes.dedup();
        self.sequence += 1;
        self.last_thread_event.insert(thread, self.sequence);
        ScheduleEdge {
            id: self.sequence,
            thread,
            causes,
            action,
        }
    }
    // Logical births outlive OS-thread ID reuse; MapUpdater.cpp:24-25,33-34.
    pub fn birth(&mut self, birth: ThreadBirth, role: ThreadRole) -> Result<ScheduleEdge> {
        require(
            birth.thread > 0
                && birth.creator > 0
                && birth.ordinal > 0
                && !self.threads.contains_key(&birth.thread)
                && !self.births.contains(&(birth.creator, birth.ordinal)),
            "execution.thread_birth",
        )?;
        self.births.insert((birth.creator, birth.ordinal));
        let event = self.edge(birth.creator, vec![], ScheduleAction::Birth(birth, role));
        self.threads.insert(
            birth.thread,
            Thread {
                role,
                birth: event.id,
                running: None,
                alive: true,
            },
        );
        Ok(event)
    }
    // Increment pending before publishing the FIFO entry; MapUpdater.cpp:74-79.
    pub fn submit(&mut self, thread: u64, job: MapJob) -> Result<ScheduleEdge> {
        require(
            thread > 0
                && job.id > 0
                && job.id < u64::MAX - 1
                && job.map.creation_generation > 0
                && !self.jobs.contains(&job.id),
            "execution.job_submit",
        )?;
        self.jobs.insert(job.id);
        self.pending.insert(job.id);
        let event = self.edge(thread, vec![], ScheduleAction::Submit(job));
        self.queue.push(job.id, (job, event.id));
        Ok(event)
    }
    // A recorded recipient may take only the next eligible queue job; MapUpdater.cpp:88-96.
    pub fn dequeue(&mut self, thread: u64, job_id: u64) -> Result<ScheduleEdge> {
        let worker = self
            .threads
            .get(&thread)
            .ok_or_else(|| refuse("execution.worker_missing"))?;
        require(
            worker.alive && worker.role == ThreadRole::MapWorker && worker.running.is_none(),
            "execution.worker_busy_or_dead",
        )?;
        let (job, submit) = self
            .queue
            .front()
            .copied()
            .ok_or_else(|| refuse("execution.queue_empty"))?;
        require(job.id == job_id, "execution.fifo_dequeue")?;
        let mut causes = vec![submit, worker.birth];
        if let Some(previous) = self.previous_pop {
            causes.push(previous);
        }
        self.queue.pop(job_id)?;
        let event = self.edge(thread, causes, ScheduleAction::Dispatch(job));
        self.previous_pop = Some(event.id);
        self.threads.get_mut(&thread).unwrap().running = Some((job, event.id));
        Ok(event)
    }
    // std::async starts each continent directly, not through MapUpdater's queue; MapManager.cpp:96-106.
    pub fn start_async(&mut self, thread: u64, job: MapJob) -> Result<ScheduleEdge> {
        let owner = self
            .threads
            .get(&thread)
            .ok_or_else(|| refuse("execution.startup_thread"))?;
        require(
            owner.alive
                && owner.role == ThreadRole::StartupMap(job.map)
                && owner.running.is_none()
                && job.id > 0
                && !self.jobs.contains(&job.id),
            "execution.startup_owner",
        )?;
        let birth = owner.birth;
        self.jobs.insert(job.id);
        self.pending.insert(job.id);
        let event = self.edge(thread, vec![birth], ScheduleAction::Dispatch(job));
        self.threads.get_mut(&thread).unwrap().running = Some((job, event.id));
        Ok(event)
    }
    // Completion follows actual execution, not submission; MapWorkers.h:50-53.
    pub fn complete(&mut self, thread: u64, job_id: u64) -> Result<ScheduleEdge> {
        let (job, dispatch) = self
            .threads
            .get(&thread)
            .and_then(|t| t.running)
            .ok_or_else(|| refuse("execution.completion_without_dispatch"))?;
        require(job.id == job_id, "execution.completion_job")?;
        self.threads.get_mut(&thread).unwrap().running = None;
        self.pending.remove(&job.id);
        let event = self.edge(thread, vec![dispatch], ScheduleAction::Complete(job));
        self.completions.push(event.id);
        Ok(event)
    }
    // Wait returns only after every pending request completes; MapUpdater.cpp:47-52.
    pub fn barrier(&mut self, thread: u64) -> Result<ScheduleEdge> {
        require(
            thread > 0 && self.pending.is_empty(),
            "execution.barrier_pending",
        )?;
        let causes = std::mem::take(&mut self.completions);
        Ok(self.edge(thread, causes, ScheduleAction::Barrier))
    }
    // Join cannot destroy a running logical worker; MapUpdater.cpp:37-58.
    pub fn exit(&mut self, thread: u64) -> Result<ScheduleEdge> {
        let owner = self
            .threads
            .get(&thread)
            .ok_or_else(|| refuse("execution.thread_missing"))?;
        require(
            owner.alive
                && owner.running.is_none()
                && (owner.role != ThreadRole::MapWorker || self.pending.is_empty()),
            "execution.thread_exit_pending",
        )?;
        let birth = owner.birth;
        let startup = matches!(owner.role, ThreadRole::StartupMap(_));
        self.threads.get_mut(&thread).unwrap().alive = false;
        let event = self.edge(thread, vec![birth], ScheduleAction::Exit);
        if startup {
            self.completions.push(event.id);
        }
        Ok(event)
    }
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}

impl SourceScheduler {
    pub fn worker_count(&self) -> usize {
        self.threads
            .values()
            .filter(|t| t.alive && t.role == ThreadRole::MapWorker)
            .count()
    }
}
