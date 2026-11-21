// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::source_rng::Mt19937;
use std::collections::BTreeMap;
fn map(id: u32) -> MapIdentity {
    MapIdentity {
        map: id,
        instance: 0,
        creation_generation: 1,
    }
}
fn birth(thread: u64) -> ThreadBirth {
    ThreadBirth {
        thread,
        creator: 1,
        ordinal: thread,
    }
}
fn job(id: u64, map_id: u32) -> MapJob {
    MapJob {
        id,
        map: map(map_id),
        diff: 100,
    }
}

#[test]
fn fifo_recipients_and_completion_barrier_are_independent() {
    let mut scheduler = SourceScheduler::default();
    scheduler.birth(birth(2), ThreadRole::MapWorker).unwrap();
    scheduler.birth(birth(3), ThreadRole::MapWorker).unwrap();
    assert!(scheduler.complete(2, 1).is_err());
    let a = scheduler.submit(1, job(1, 0)).unwrap();
    let b = scheduler.submit(1, job(2, 1)).unwrap();
    assert!(scheduler.dequeue(2, 2).is_err());
    let da = scheduler.dequeue(3, 1).unwrap();
    assert!(da.causes.contains(&a.id));
    assert!(scheduler.dequeue(3, 2).is_err());
    let db = scheduler.dequeue(2, 2).unwrap();
    assert!(db.causes.contains(&b.id) && db.causes.contains(&da.id));
    assert!(scheduler.barrier(1).is_err());
    let cb = scheduler.complete(2, 2).unwrap();
    assert!(scheduler.barrier(1).is_err());
    let ca = scheduler.complete(3, 1).unwrap();
    let barrier = scheduler.barrier(1).unwrap();
    assert!(barrier.causes.contains(&ca.id) && barrier.causes.contains(&cb.id));
    scheduler.exit(2).unwrap();
    scheduler.submit(1, job(3, 0)).unwrap();
    assert!(scheduler.dequeue(2, 3).is_err());
    scheduler.dequeue(3, 3).unwrap();
    assert!(scheduler.exit(3).is_err());
    scheduler.complete(3, 3).unwrap();
    scheduler.exit(3).unwrap();
    assert!(scheduler.birth(birth(3), ThreadRole::MapWorker).is_err());
    scheduler.birth(birth(4), ThreadRole::MapWorker).unwrap();
}

#[derive(Default)]
struct Work {
    phases: Vec<WorldPhase>,
    jobs: Vec<MapJob>,
    unload_diffs: Vec<u32>,
    leave_pending: bool,
}
impl WorldWork for Work {
    fn phase(&mut self, phase: WorldPhase, _: u64, _: u32) -> Result<()> {
        self.phases.push(phase);
        Ok(())
    }
    fn run_map_jobs(
        &mut self,
        _: u64,
        jobs: &[MapJob],
        scheduler: &mut SourceScheduler,
    ) -> Result<()> {
        self.jobs.extend_from_slice(jobs);
        if !self.leave_pending {
            for job in jobs {
                scheduler.dequeue(2, job.id)?;
                scheduler.complete(2, job.id)?;
            }
        }
        Ok(())
    }
    fn run_map_inline(&mut self, _: u64, job: MapJob) -> Result<()> {
        self.jobs.push(job);
        Ok(())
    }
    fn unload_maps(&mut self, diff: u32, maps: &mut SourceMapManager) -> Result<()> {
        assert_eq!(maps.current(), i64::from(diff));
        self.unload_diffs.push(diff);
        Ok(())
    }
}
#[test]
fn world_source_phases_and_map_accumulation() {
    let expected = vec![
        WorldPhase::ClockSnapshot,
        WorldPhase::Timers,
        WorldPhase::GameTime,
        WorldPhase::Messages,
        WorldPhase::MassMail,
        WorldPhase::WeeklyQuests,
        WorldPhase::Auctions,
        WorldPhase::Sessions,
        WorldPhase::Uptime,
        WorldPhase::Maps,
        WorldPhase::Battlegrounds,
        WorldPhase::OutdoorPvp,
        WorldPhase::WorldState,
        WorldPhase::Groups,
        WorldPhase::DeleteCharacters,
        WorldPhase::SqlCallbacks,
        WorldPhase::Corpses,
        WorldPhase::GameEvents,
        WorldPhase::DelayedRemovals,
        WorldPhase::PersistentState,
        WorldPhase::Maintenance,
        WorldPhase::Cli,
        WorldPhase::Terrain,
    ];
    for threaded in [false, true] {
        let mut world = SourceWorld::new(1, 100).unwrap();
        if threaded {
            world
                .scheduler
                .birth(birth(2), ThreadRole::MapWorker)
                .unwrap();
        }
        world.maps.register(map(1)).unwrap();
        world.maps.register(map(0)).unwrap();
        let mut work = Work::default();
        world.update(50, &mut work).unwrap();
        assert_eq!(work.phases, expected);
        assert!(work.jobs.is_empty());
        world.update(49, &mut work).unwrap();
        assert_eq!(world.maps.current(), 99);
        world.update(1, &mut work).unwrap();
        assert_eq!(
            work.jobs
                .iter()
                .map(|j| (j.map.map, j.diff))
                .collect::<Vec<_>>(),
            vec![(0, 100), (1, 100)]
        );
        world.update(250, &mut work).unwrap();
        assert_eq!(work.unload_diffs, vec![100, 250]);
        assert_eq!(world.maps.current(), 0);
        assert_eq!(work.jobs[2].diff, 250);
        assert_eq!(work.jobs.len(), 4);
    }
}
#[test]
fn barrier_blocks_all_post_map_phases() {
    let mut world = SourceWorld::new(1, 100).unwrap();
    world.maps.register(map(0)).unwrap();
    world
        .scheduler
        .birth(birth(2), ThreadRole::MapWorker)
        .unwrap();
    let mut work = Work {
        leave_pending: true,
        ..Work::default()
    };
    assert_eq!(
        world.update(100, &mut work).unwrap_err().id,
        "execution.barrier_pending"
    );
    assert_eq!(work.phases.last(), Some(&WorldPhase::Maps));
    assert!(work.unload_diffs.is_empty());
    assert_eq!(world.maps.current(), 100);
    assert_eq!(
        world.update(100, &mut work).unwrap_err().id,
        "execution.world_stopped"
    );
}
#[test]
fn startup_async_maps_have_separate_births_and_join_edges() {
    let mut startup = StartupMaps::default();
    let mut maps = SourceMapManager::new(100);
    let mut scheduler = SourceScheduler::default();
    assert!(startup
        .launch(&mut maps, &mut scheduler, birth(2), map(1))
        .is_err());
    let a = startup
        .launch(&mut maps, &mut scheduler, birth(2), map(0))
        .unwrap();
    let b = startup
        .launch(&mut maps, &mut scheduler, birth(3), map(1))
        .unwrap();
    assert!(startup.wait(&mut scheduler, 1).is_err());
    let ScheduleAction::Dispatch(jb) = b[1].action else {
        panic!()
    };
    let ScheduleAction::Dispatch(ja) = a[1].action else {
        panic!()
    };
    let cb = startup.complete(&mut scheduler, 3, jb.id).unwrap();
    assert!(startup.wait(&mut scheduler, 1).is_err());
    let ca = startup.complete(&mut scheduler, 2, ja.id).unwrap();
    let joined = startup.wait(&mut scheduler, 1).unwrap();
    assert!(joined.causes.contains(&ca[0].id) && joined.causes.contains(&cb[0].id));
    assert!(scheduler.start_async(2, ja).is_err());
    scheduler.birth(birth(4), ThreadRole::MapWorker).unwrap();
}

struct Handlers {
    world: SourceMessager,
    map_queue: SourceMessager,
    calls: Vec<(u64, u32)>,
    rng: BTreeMap<u64, Mt19937>,
    draws: Vec<u32>,
    creature: bool,
}
impl Handlers {
    fn new() -> Self {
        Self {
            world: SourceMessager::new(MessageOwner::World),
            map_queue: SourceMessager::new(MessageOwner::Map(map(0))),
            calls: vec![],
            rng: BTreeMap::new(),
            draws: vec![],
            creature: true,
        }
    }
}
impl MessageWork for Handlers {
    fn begin_message(&mut self, _: &MessageEnvelope) -> Result<()> {
        Ok(())
    }
    fn forward_game_event(&mut self, _: u64, maps: &[EventMapGuids], spawn: bool) -> Result<()> {
        self.map_queue.enqueue(MessageEnvelope {
            id: 1,
            cause: 10,
            payload: SourceMessage::MapGameEvent {
                creatures: maps
                    .first()
                    .map_or_else(Vec::new, |map| map.creatures.clone()),
                gameobjects: vec![],
                spawn,
            },
        })?;
        self.world.enqueue(MessageEnvelope {
            id: 2,
            cause: 10,
            payload: SourceMessage::ForwardGameEvent {
                maps: vec![],
                spawn,
            },
        })
    }
    fn map_game_event(
        &mut self,
        thread: u64,
        _: MapIdentity,
        creatures: &[u32],
        _: &[u32],
        _: bool,
    ) -> Result<()> {
        for guid in creatures {
            self.calls.push((thread, *guid));
        }
        self.map_queue.enqueue(MessageEnvelope {
            id: 2,
            cause: 11,
            payload: SourceMessage::UpdateCreatureEvent {
                update: authored_event_update(),
            },
        })
    }
    fn creature_present(&self, _: MapIdentity, _: u64) -> bool {
        self.creature
    }
    fn update_creature_event(
        &mut self,
        thread: u64,
        _: MapIdentity,
        _: EventCreatureUpdate,
    ) -> Result<()> {
        self.draws.push(
            self.rng
                .entry(thread)
                .or_insert_with(|| Mt19937::from_seed(5489))
                .next_u32(),
        );
        Ok(())
    }
}
#[test]
fn event_world_map_batches_detach_and_nested_enqueue_waits() {
    let mut handlers = Handlers::new();
    handlers
        .world
        .enqueue(MessageEnvelope {
            id: 1,
            cause: 9,
            payload: SourceMessage::ForwardGameEvent {
                maps: vec![EventMapGuids {
                    map: 0,
                    creatures: vec![7, 8],
                    gameobjects: vec![],
                }],
                spawn: true,
            },
        })
        .unwrap();
    let world_batch = handlers.world.take_batch(MessageOwner::World).unwrap();
    let done = world_batch
        .execute(MessageOwner::World, 1, &mut handlers)
        .unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(handlers.world.pending(), 1);
    assert!(handlers.calls.is_empty());
    let batch = handlers
        .map_queue
        .take_batch(MessageOwner::Map(map(0)))
        .unwrap();
    batch
        .execute(MessageOwner::Map(map(0)), 2, &mut handlers)
        .unwrap();
    assert_eq!(handlers.calls, vec![(2, 7), (2, 8)]);
    assert!(handlers.draws.is_empty());
    let batch = handlers
        .map_queue
        .take_batch(MessageOwner::Map(map(0)))
        .unwrap();
    batch
        .execute(MessageOwner::Map(map(0)), 2, &mut handlers)
        .unwrap();
    assert_eq!(handlers.draws, vec![3499211612]);
}
#[test]
fn stale_lookup_is_owner_specific_and_worker_rng_continues_across_maps() {
    let mut handlers = Handlers::new();
    let mut scheduler = SourceScheduler::default();
    scheduler.birth(birth(2), ThreadRole::MapWorker).unwrap();
    for (id, map_id) in [(1, 0), (2, 1), (3, 0)] {
        scheduler.submit(1, job(id, map_id)).unwrap();
        let dispatched = scheduler.dequeue(2, id).unwrap();
        handlers
            .update_creature_event(dispatched.thread, map(map_id), authored_event_update())
            .unwrap();
        scheduler.complete(2, id).unwrap();
    }
    assert_eq!(handlers.draws, vec![3499211612, 581869302, 3890346734]);
    handlers.creature = false;
    let mut messages = SourceMessager::new(MessageOwner::Map(map(0)));
    messages
        .enqueue(MessageEnvelope {
            id: 1,
            cause: 1,
            payload: SourceMessage::UpdateCreatureEvent {
                update: authored_event_update(),
            },
        })
        .unwrap();
    let wrong = MapIdentity {
        creation_generation: 2,
        ..map(0)
    };
    assert!(messages.take_batch(MessageOwner::Map(wrong)).is_err());
    assert_eq!(messages.pending(), 1);
    let done = messages
        .execute(MessageOwner::Map(map(0)), 2, &mut handlers)
        .unwrap();
    assert_eq!(done[0].outcome, MessageOutcome::CreatureAbsent);
    assert_eq!(handlers.draws.len(), 3);
    let mut calendar = Calendar::new(CalendarOwner::Map(map(0)), 0);
    assert!(calendar
        .update(
            CalendarOwner::Map(wrong),
            ClockSample::authored(ClockKind::WallSeconds, 1)
        )
        .is_err());
}

fn authored_event_update() -> EventCreatureUpdate {
    EventCreatureUpdate {
        definition: EventSpawnDefinition {
            map: 0,
            cell: benilla_sim::resident_map::CellPair::from_indices(512, 512).unwrap(),
            entry: 1,
            guid: 7,
        },
        data: GameEventSubstitution {
            guid: 7,
            entry: 0,
            model: 0,
            equipment: 0,
            start_spell: 0,
            end_spell: 0,
        },
        activate: true,
    }
}
