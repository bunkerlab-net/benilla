// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared controller assertions for the app-only loaded Service fixture. This file
//! supplies no geometry answers: the app passes its actual authority and commit sink.
use super::*;
use benilla_sim::creature_motion as motion;
use benilla_world_store::native::reconstruction::{Recipe, RecipeStore};
use std::collections::BTreeMap;

#[path = "loaded_quest_loop_fixture/restart.rs"]
mod restart;

#[derive(Clone, Default)]
pub(crate) struct Observed {
    controls: Controls,
    pub(crate) loaded: Arc<Mutex<Vec<n::AuthorityReply>>>,
    pub(crate) committed: Arc<Mutex<Vec<RouteCommitNotice>>>,
    pub(crate) attempted: Arc<Mutex<Vec<RouteCommitNotice>>>,
    pub(crate) lose_ack: Arc<AtomicBool>,
    fail_after_route: Arc<AtomicBool>,
}

/// Route/near-point/environment requests have exactly one destination. All other
/// spatial witnesses are explicitly authored; selected semantic escapes fail closed.
pub(crate) struct Authority<A> {
    local: A,
    other: loop_fixture::CompleteAuthority,
    observed: Observed,
    last_route_tick: Option<u64>,
}
impl<A> Authority<A> {
    pub(crate) fn new(local: A, observed: Observed) -> Self {
        Self {
            local,
            other: Default::default(),
            observed,
            last_route_tick: None,
        }
    }
}
impl<A: n::Authority> n::Authority for Authority<A> {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.observed.controls.calls.fetch_add(1, Ordering::SeqCst);
        if matches!(
            request.query,
            n::Query::PlayerEnvironment(_)
                | n::Query::SourceMotion(n::MotionQuery::NearPoint(_) | n::MotionQuery::Route(_))
        ) {
            let reply = self.local.resolve(request)?;
            if matches!(
                request.query,
                n::Query::SourceMotion(n::MotionQuery::Route(_))
            ) {
                assert_eq!(
                    calculation(&reply).source,
                    motion::RouteSource::OwnerPathFinder
                );
                self.last_route_tick = Some(request.binding.tick);
            }
            self.observed.loaded.lock().unwrap().push(reply.clone());
            Ok(reply)
        } else {
            self.observed
                .controls
                .queries
                .lock()
                .unwrap()
                .push(request.clone());
            self.other.resolve(request)
        }
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.observed.controls.calls.fetch_add(1, Ordering::SeqCst);
        self.observed
            .controls
            .derived
            .lock()
            .unwrap()
            .push(reply.clone());
        self.other.observe_derived(reply)?;
        if reply
            .request
            .binding
            .npc
            .is_some_and(|id| id.spawn == self.observed.controls.fail_owner.load(Ordering::SeqCst))
            && matches!(&reply.request.query,
                n::Query::SourceMotion(n::MotionQuery::Combat(n::CombatQuery::NpcSemantics(request)))
                    if request.phase == n::CombatPhase::UnitPrelude)
            && (!self.observed.fail_after_route.load(Ordering::SeqCst)
                || self.last_route_tick == Some(reply.request.binding.tick))
        {
            return Err(n::NativeError::AuthorityUnavailable(
                "authored late giver after loaded geometry",
            ));
        }
        Ok(())
    }
}
pub(crate) fn calculation(reply: &n::AuthorityReply) -> &motion::RouteCalculation {
    let n::Facts::SourceMotion(n::MotionFacts::Spatial(inner)) = &reply.facts else {
        panic!("actual loaded Route facts");
    };
    let motion::Fact::RouteCalculation(calculation) = &inner.fact else {
        panic!("actual loaded Route calculation");
    };
    calculation
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Heads {
    pub(crate) committed: BTreeMap<runtime::NpcId, (usize, motion::PathFinderCache)>,
    pub(crate) candidates: Vec<usize>,
}

pub(crate) struct Fixture {
    harness: Harness,
    profile: n::Profile,
}
impl Fixture {
    pub(crate) fn new(profile: n::Profile) -> Self {
        Self {
            harness: Harness::new(),
            profile,
        }
    }
    pub(crate) fn player(&self) -> PlayerId {
        Selection::from_profile(&self.harness.profile, &self.harness.character)
            .unwrap()
            .player
    }
    pub(crate) fn run(
        self,
        authority: impl n::Authority + Send + 'static,
        sink: impl RouteCommitSink + 'static,
        observed: Observed,
        inspect: impl Fn(&RouteCommitBoundary, &[n::AuthorityReply], bool) -> Heads,
        faults: bool,
    ) {
        let mut driver =
            self.open_accepted(authority, sink, observed.clone(), inspect, faults, false);
        let mode = if faults {
            FinishMode::AuthorityAndSave
        } else {
            FinishMode::Uninterrupted
        };
        driver.finish(&observed, mode);
        // A's death can clear its cache; the legacy path still never reopens it.
        driver.controller.shutdown().unwrap();
    }
    #[allow(clippy::too_many_arguments)]
    fn open_accepted(
        self,
        authority: impl n::Authority + Send + 'static,
        sink: impl RouteCommitSink + 'static,
        observed: Observed,
        inspect: impl Fn(&RouteCommitBoundary, &[n::AuthorityReply], bool) -> Heads,
        faults: bool,
        recipe_capture: bool,
    ) -> Driver {
        let h = self.harness;
        let profile = self.profile;
        let (hostile, giver) = role_ids(&profile);
        let catalog = LoopCatalog::new(&profile);
        let mut c = NativeController::enter_component_test(
            &h.root,
            &h.profile,
            &h.character,
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            profile.clone(),
            &catalog,
            authority,
        )
        .unwrap();
        let path = if recipe_capture {
            assert_eq!(c.tick(), 0);
            assert!(c.world.records().is_empty());
            c.world.validate_saved_session_reopen().unwrap();
            let path = h.path().with_extension("quest-recipe");
            let (recipe, receipt) = RecipeStore::create(&path, &c.world).unwrap();
            receipt.acknowledge(&mut c.world).unwrap();
            c.store = store::Store::Recipe(Box::new(recipe));
            path
        } else {
            h.path()
        };
        c.initial_events().unwrap();
        c.attach_route_commit_sink(sink).unwrap();
        let b = c.world.state().npcs[&giver].id;
        let b_guid = guid(&c, giver);
        let a = c.world.state().npcs[&hostile].id;
        assert_eq!(c.world.state().npcs.len(), 2);
        let boundary = |c: &NativeController| c.route_commit.as_ref().unwrap().boundary.clone();
        let mut rolled_back = false;
        let mut journal_refusal = false;
        if faults {
            observed.controls.fail_owner.store(giver, Ordering::SeqCst);
            observed.fail_after_route.store(true, Ordering::SeqCst);
        }
        // No cast, quest, direct state edit, hidden actor or permissive semantic
        // response starts the chase: A's selected sight update acquires this cache.
        while inspect(&boundary(&c), &[], true).committed.is_empty() {
            assert!(
                c.tick() < 10,
                "natural hostile sight must acquire a loaded cache"
            );
            let before = c.world.clone();
            let checkpoint = c.world.checkpoint().unwrap();
            let generation = c.store.generation();
            let count = observed.committed.lock().unwrap().len();
            let first = observed.loaded.lock().unwrap().len();
            let out = c.pump(c.tick_duration());
            if let Some(pause) = out.pause {
                assert!(faults && !rolled_back,
                    "unexpected loaded quest pause before intended rollback: tick={}, faults={faults}, rolled_back={rolled_back}, pause={pause:?}", c.tick());
                assert!(pause.message.contains("authored late giver after loaded geometry"),
                    "loaded quest paused before the injected late-giver failure: tick={}, loaded_replies={}, pause={pause:?}", c.tick(), observed.loaded.lock().unwrap().len() - first);
                assert_eq!(out.ticks, 0);
                assert!(out.events.is_empty());
                assert_eq!(pause.kind, PauseKind::Authority);
                assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
                assert_eq!(c.world.state(), before.state());
                assert_eq!(c.store.generation(), generation);
                assert_eq!(observed.committed.lock().unwrap().len(), count);
                let attempted: Vec<_> = observed.loaded.lock().unwrap()[first..]
                    .iter()
                    .filter(|r| {
                        matches!(
                            r.request.query,
                            n::Query::SourceMotion(n::MotionQuery::Route(_))
                        )
                    })
                    .cloned()
                    .collect();
                assert_eq!(attempted.len(), 1,
                    "injected late-giver rollback must follow exactly one loaded Route: tick={}, pause={pause:?}", c.tick());
                assert_eq!(attempted[0].request.binding.npc, Some(a));
                let heads = inspect(&boundary(&c), &attempted, false);
                assert!(heads.committed.is_empty());
                let pending = c.pending.as_ref().unwrap().inputs.clone();
                assert!(pending.is_empty());
                observed.controls.fail_owner.store(0, Ordering::SeqCst);
                observed.fail_after_route.store(false, Ordering::SeqCst);
                // Also withhold durable publication on the exact successful retry.
                inject(&path);
                c.retry_tick().unwrap();
                assert_eq!(c.pending.as_ref().unwrap().inputs, pending);
                let retry_first = observed.loaded.lock().unwrap().len();
                let out = c.pump(Duration::ZERO);
                let retried: Vec<_> = observed.loaded.lock().unwrap()[retry_first..]
                    .iter()
                    .filter(|r| {
                        matches!(
                            r.request.query,
                            n::Query::SourceMotion(n::MotionQuery::Route(_))
                        )
                    })
                    .cloned()
                    .collect();
                assert_eq!(retried, attempted,
                    "loaded quest retry must reach the exact loaded Route before durable publication refusal: tick={}, retry_loaded_replies={}, ticks={}, pause={:?}",
                    c.tick(), observed.loaded.lock().unwrap().len() - retry_first, out.ticks, out.pause);
                assert!(!attempted[0]
                    .request
                    .require_pathfinder_dispatch(c.world.profile())
                    .unwrap()
                    .same_live_dispatch(
                        retried[0]
                            .request
                            .require_pathfinder_dispatch(c.world.profile())
                            .unwrap()
                    ));
                assert_eq!(inspect(&boundary(&c), &retried, false), heads);
                let record = c.world.last_tick().unwrap().clone();
                let mut replay = before.clone();
                replay.replay(&[record]).unwrap();
                assert_eq!(replay.state(), c.world.state());
                assert_eq!(replay.state().rng_counter, c.world.state().rng_counter);
                assert_eq!(c.store.generation(), generation);
                assert_eq!(observed.committed.lock().unwrap().len(), count);
                assert!(!recipe_capture);
                reject_journal_reopen(&h, &before, &c.world);
                journal_refusal = true;
                flush(&path, &mut c, &observed, out, false);
                rolled_back = true;
            } else {
                assert_eq!(out.ticks, 1);
            }
        }
        assert_eq!(rolled_back, faults);
        assert_eq!(journal_refusal, faults);
        let heads = inspect(&boundary(&c), &[], true);
        assert_eq!(heads.committed.len(), 1);
        let cache = &heads.committed[&a].1;
        assert_eq!(cache.source, motion::RouteSource::OwnerPathFinder);
        assert_eq!(cache.binding.identity.creature, hostile);
        assert_eq!(cache.binding.owner_generation, a.generation);
        assert_eq!(cache.revision, 1);
        assert!(progress(&c).quests.progress(loop_fixture::QUEST).is_none());
        assert!(progress(&c).credited_generations.is_empty());
        assert_giver(&c, giver, b, b_guid);
        if !recipe_capture {
            reject_checkpoint_reopen(&h, &c.world, "before-accept", true);
        }
        assert!(matches!(
            &c.receive(ClientCommand::QuestgiverHello { npc: b_guid })
                .unwrap()
                .events[..],
            [SessionEvent::QuestGreeting(_)]
        ));
        assert!(matches!(
            &c.receive(ClientCommand::QuestgiverQuery {
                npc: b_guid,
                quest: loop_fixture::QUEST
            })
            .unwrap()
            .events[..],
            [SessionEvent::QuestDetail(_)]
        ));
        if faults {
            c.receive(ClientCommand::QuestgiverAccept {
                npc: b_guid,
                quest: loop_fixture::QUEST,
            })
            .unwrap();
            let before = c.world.clone();
            let checkpoint = c.world.checkpoint().unwrap();
            let heads = inspect(&boundary(&c), &[], true);
            let generation = c.store.generation();
            let committed = observed.committed.lock().unwrap().len();
            observed.controls.fail_owner.store(giver, Ordering::SeqCst);
            let failed = c.pump(c.tick_duration());
            assert_eq!(failed.ticks, 0);
            assert!(failed.events.is_empty());
            assert_eq!(failed.pause.unwrap().kind, PauseKind::Authority);
            assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
            assert_eq!(c.world.state(), before.state());
            assert_eq!(c.store.generation(), generation);
            assert_eq!(inspect(&boundary(&c), &[], true), heads);
            assert_eq!(observed.committed.lock().unwrap().len(), committed);
            assert!(progress(&c).quests.progress(loop_fixture::QUEST).is_none());
            let inputs = c.pending.as_ref().unwrap().inputs.clone();
            assert_eq!(inputs.len(), 1);
            observed.controls.fail_owner.store(0, Ordering::SeqCst);
            inject(&path);
            c.retry_tick().unwrap();
            assert_eq!(c.pending.as_ref().unwrap().inputs, inputs);
            let out = c.pump(Duration::ZERO);
            let mut replay = before;
            replay
                .replay(&[c.world.last_tick().unwrap().clone()])
                .unwrap();
            assert_eq!(replay.state(), c.world.state());
            assert!(progress(&c).quests.progress(loop_fixture::QUEST).is_some());
            flush(&path, &mut c, &observed, out, false);
        } else {
            accept(&mut c, giver);
        }
        assert_eq!(
            progress(&c)
                .quests
                .progress(loop_fixture::QUEST)
                .unwrap()
                .status,
            q::Status::Incomplete
        );
        if !recipe_capture {
            reject_checkpoint_reopen(&h, &c.world, "accepted", false);
        }
        assert_giver(&c, giver, b, b_guid);
        Driver {
            controller: c,
            path,
            harness: h,
        }
    }
}

struct Driver {
    // Drop the controller's exclusive store lease before deleting fixture files.
    controller: NativeController,
    path: PathBuf,
    harness: Harness,
}

#[derive(Clone, Copy)]
enum FinishMode {
    Uninterrupted,
    AuthorityAndSave,
    Reconstructed { retry_faults: bool },
}
impl Driver {
    fn finish(&mut self, observed: &Observed, mode: FinishMode) {
        let path = &self.path;
        let c = &mut self.controller;
        let (hostile, giver) = role_ids(c.world.profile());
        let a = c.world.state().npcs[&hostile].id;
        let b = c.world.state().npcs[&giver].id;
        let b_guid = guid(c, giver);
        let mut first_tick = true;
        let mut settle_tick = |c: &mut NativeController| {
            if std::mem::take(&mut first_tick)
                && matches!(mode, FinishMode::Reconstructed { retry_faults: true })
            {
                progress_tick(path, c, observed, mode);
            } else {
                step(c);
            }
        };
        // Finish the source spline before aiming. The only authored pose input is
        // an in-place player turn, not a replacement NPC path or collision result.
        while c.world.state().npcs[&hostile]
            .source_motion
            .as_ref()
            .unwrap()
            .motion
            .is_moving()
        {
            assert!(c.tick() < 50);
            settle_tick(c);
        }
        let player = c.world.state().position;
        let target = c.world.state().npcs[&hostile].ai.position();
        let angle = (target.y.as_f32() - player.y.as_f32())
            .atan2(target.x.as_f32() - player.x.as_f32())
            .rem_euclid(std::f32::consts::TAU);
        c.receive(ClientCommand::Move {
            kind: MoveKind::Heartbeat,
            flags: 0,
            pos: [player.x.as_f32(), player.y.as_f32(), player.z.as_f32()],
            orientation: angle,
            pitch: 0.,
            fall_time: 0,
            jump: None,
            transport: None,
        })
        .unwrap();
        settle_tick(c);
        let mut killed = begin_cast(c, hostile).events;
        while c.world.state().npcs[&hostile].health > 0 {
            assert!(c.tick() < 100, "selected spell133 must kill A");
            if c.world.state().casts.is_empty() {
                c.receive(ClientCommand::CastSpell {
                    spell_id: loop_fixture::SPELL,
                    target: Some(guid(c, hostile)),
                })
                .unwrap();
            }
            killed.extend(progress_tick(path, c, observed, mode));
        }
        assert_eq!(
            killed
                .iter()
                .filter(|event| matches!(event,
                    SessionEvent::XpGain(gain) if gain.kill
                ))
                .count(),
            1
        );
        assert_eq!(
            killed
                .iter()
                .filter(|event| matches!(event, SessionEvent::QuestObjectiveKill { count: 1, .. }))
                .count(),
            1
        );
        let position =
            |predicate: fn(&SessionEvent) -> bool| killed.iter().position(predicate).unwrap();
        assert!(
            position(|e| matches!(e, SessionEvent::SpellStart { spell_id: 133, .. }))
                < position(|e| matches!(e, SessionEvent::SpellGo { spell_id: 133, .. }))
        );
        assert!(
            position(|e| matches!(e, SessionEvent::SpellDamageLog(_)))
                < position(|e| matches!(e, SessionEvent::XpGain(x) if x.kill))
        );
        assert!(
            position(|e| matches!(e, SessionEvent::XpGain(x) if x.kill))
                < position(|e| matches!(e, SessionEvent::QuestObjectiveKill { count: 1, .. }))
        );
        assert_eq!(
            c.world.state().npcs[&hostile].original_loot_recipient,
            Some(c.world.state().player)
        );
        assert_eq!(
            progress(c).credited_generations.get(&hostile),
            Some(&a.generation)
        );
        assert_eq!(
            progress(c)
                .quests
                .progress(loop_fixture::QUEST)
                .unwrap()
                .status,
            q::Status::Incomplete
        );
        assert_eq!(inventory(c).item_count(loop_fixture::ITEM).unwrap(), 0);
        assert_eq!(progress(c).money, 11);
        assert_giver(c, giver, b, b_guid);
        let loot = open_loot(c, hostile);
        assert!(loot.events.iter().any(|e| matches!(e, SessionEvent::LootResponse { gold: 3, items, .. } if items.len() == 1 && items[0].item_id == loop_fixture::ITEM)));
        c.receive(ClientCommand::LootMoney).unwrap();
        c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
            .unwrap();
        let collected = progress_tick(path, c, observed, mode);
        assert_eq!(
            collected
                .iter()
                .filter(|event| matches!(
                    event,
                    SessionEvent::QuestObjectiveItem {
                        item_id: 70001,
                        count: 1
                    }
                ))
                .count(),
            1
        );
        assert_eq!(
            collected
                .iter()
                .filter(|event| matches!(event, SessionEvent::ItemCreate { .. }))
                .count(),
            1
        );
        let credit = collected
            .iter()
            .position(|e| {
                matches!(
                    e,
                    SessionEvent::QuestObjectiveItem {
                        item_id: 70001,
                        count: 1
                    }
                )
            })
            .unwrap();
        let stored = collected
            .iter()
            .position(|e| matches!(e, SessionEvent::ItemCreate { .. }))
            .unwrap();
        assert!(credit < stored);
        assert_eq!(inventory(c).item_count(loop_fixture::ITEM).unwrap(), 1);
        assert_eq!(progress(c).money, 14);
        assert_eq!(
            progress(c)
                .quests
                .progress(loop_fixture::QUEST)
                .unwrap()
                .status,
            q::Status::Complete
        );
        choose_reward(c, giver);
        let reward = progress_tick(path, c, observed, mode);
        assert_eq!(
            reward
                .iter()
                .filter(|event| matches!(event, SessionEvent::QuestComplete(_)))
                .count(),
            1
        );
        assert_eq!(
            reward
                .iter()
                .filter(|event| matches!(event,
                    SessionEvent::XpGain(gain) if !gain.kill
                ))
                .count(),
            1
        );
        let destroyed = reward
            .iter()
            .position(|e| matches!(e, SessionEvent::ObjectDestroyed(_)))
            .unwrap();
        let rewarded = reward
            .iter()
            .position(|e| matches!(e, SessionEvent::QuestComplete(_)))
            .unwrap();
        assert!(destroyed < rewarded);
        assert!(
            progress(c)
                .quests
                .progress(loop_fixture::QUEST)
                .unwrap()
                .rewarded
        );
        assert_eq!(progress(c).player.view().level, 3);
        assert_eq!(inventory(c).item_count(loop_fixture::ITEM).unwrap(), 0);
        assert_eq!(progress(c).money, 21);
        assert_giver(c, giver, b, b_guid);
        choose_reward(c, giver);
        no_transient_progress(&step(c).events);
        assert_eq!(progress(c).money, 21);
        assert_source_hosts(&observed.controls);
        let queries = observed.controls.queries.lock().unwrap();
        assert!(
            queries.iter().any(|r| matches!(&r.query,
            n::Query::SourceMotion(n::MotionQuery::SceneVisitor(scene))
                if scene.creatures.len() == 2 && scene.creatures.iter().any(|npc| npc.id == b)
                    && scene.owner_order.contains(&hostile) && scene.owner_order.contains(&giver)))
        );
        drop(queries);
        let loaded = observed.loaded.lock().unwrap();
        let required = if matches!(mode, FinishMode::Reconstructed { .. }) {
            1
        } else {
            3
        };
        for kind in 0..required {
            assert!(loaded.iter().any(|r| matches!(
                (kind, &r.request.query),
                (0, n::Query::PlayerEnvironment(_))
                    | (1, n::Query::SourceMotion(n::MotionQuery::NearPoint(_)))
                    | (2, n::Query::SourceMotion(n::MotionQuery::Route(_)))
            )));
        }
        assert!(observed
            .controls
            .queries
            .lock()
            .unwrap()
            .iter()
            .all(|r| !matches!(
                r.query,
                n::Query::PlayerEnvironment(_)
                    | n::Query::SourceMotion(
                        n::MotionQuery::NearPoint(_) | n::MotionQuery::Route(_)
                    )
            )));
        assert!(
            loaded
                .iter()
                .filter(|r| matches!(
                    r.request.query,
                    n::Query::SourceMotion(n::MotionQuery::Route(_))
                ))
                .all(|r| r.request.binding.npc == Some(a)),
            "neutral B was never removed or attacked to obtain quest access"
        );
        drop(loaded);
    }
}

fn role_ids(profile: &n::Profile) -> (u64, u64) {
    let hostile = profile
        .npcs
        .iter()
        .find(|npc| npc.entry == loop_fixture::HOSTILE_ENTRY)
        .unwrap()
        .ai
        .creature_id;
    let giver = profile
        .npcs
        .iter()
        .find(|npc| npc.entry == loop_fixture::GIVER_ENTRY)
        .unwrap()
        .ai
        .creature_id;
    (hostile, giver)
}

fn assert_giver(c: &NativeController, giver: u64, id: runtime::NpcId, guid: u64) {
    let npc = &c.world.state().npcs[&giver];
    assert_eq!(npc.id, id);
    assert_eq!(npc.ai.phase(), ai::Phase::Idle);
    assert!(npc.health > 0);
    assert_eq!(npc.events.clock_ms(), c.world.state().now_ms);
    assert_eq!(npc.ai.next_sequence(), Some(c.tick() + 1));
    assert_eq!(c.projection.npc_guid(id), Some(guid));
    assert!(matches!(
        npc.source_motion.as_ref().unwrap().motion.generator(),
        motion::Generator::Idle
    ));
}
fn inject(path: &Path) {
    fs::write(
        path.join(".world-checkpoint.pending"),
        b"authored loaded-loop fault",
    )
    .unwrap();
}
fn progress_tick(
    path: &Path,
    c: &mut NativeController,
    observed: &Observed,
    mode: FinishMode,
) -> Vec<SessionEvent> {
    if matches!(
        mode,
        FinishMode::AuthorityAndSave | FinishMode::Reconstructed { retry_faults: true }
    ) {
        let giver = c
            .world
            .profile()
            .npcs
            .iter()
            .find(|npc| npc.entry == loop_fixture::GIVER_ENTRY)
            .unwrap()
            .ai
            .creature_id;
        let before = c.world.clone();
        let checkpoint = c.world.checkpoint().unwrap();
        let records = c.world.records().to_vec();
        let generation = c.store.generation();
        let committed = observed.committed.lock().unwrap().len();
        let bytes = fs::read(path.join("world.checkpoint")).unwrap();
        observed.controls.fail_owner.store(giver, Ordering::SeqCst);
        let out = c.pump(c.tick_duration());
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert_eq!(out.pause.unwrap().kind, PauseKind::Authority);
        assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
        assert_eq!(c.world.state(), before.state());
        assert_eq!(c.world.records(), records);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(observed.committed.lock().unwrap().len(), committed);
        assert_eq!(fs::read(path.join("world.checkpoint")).unwrap(), bytes);
        let inputs = c.pending.as_ref().unwrap().inputs.clone();
        observed.controls.fail_owner.store(0, Ordering::SeqCst);
        inject(path);
        c.retry_tick().unwrap();
        assert_eq!(c.pending.as_ref().unwrap().inputs, inputs);
        let out = c.pump(Duration::ZERO);
        assert_eq!(fs::read(path.join("world.checkpoint")).unwrap(), bytes);
        assert_eq!(c.store.generation(), generation);
        let mut replay = before;
        replay
            .replay(&[c.world.last_tick().unwrap().clone()])
            .unwrap();
        assert_eq!(replay.state(), c.world.state());
        assert_eq!(replay.state().rng_counter, c.world.state().rng_counter);
        flush(
            path,
            c,
            observed,
            out,
            matches!(mode, FinishMode::Reconstructed { .. }),
        )
    } else {
        step(c).events
    }
}
fn flush(
    path: &Path,
    c: &mut NativeController,
    observed: &Observed,
    out: Pump,
    lose_ack: bool,
) -> Vec<SessionEvent> {
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(c.pending.is_none());
    let checkpoint = c.world.checkpoint().unwrap();
    let state = c.world.state().clone();
    let calls = observed.controls.calls.load(Ordering::SeqCst);
    let committed = observed.committed.lock().unwrap().len();
    let generation = c.store.generation();
    let events = c.outbox.as_ref().unwrap().events.len();
    let projected = format!("{:?}", c.outbox.as_ref().unwrap().events);
    let records = c.world.records().to_vec();
    let record = c.world.last_tick().unwrap().clone();
    let notice = c
        .route_commit
        .as_ref()
        .unwrap()
        .pending
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone();
    let unsaved_bytes = fs::read(path.join("world.checkpoint")).unwrap();
    assert!(c.outbox.as_ref().unwrap().saved.is_none());
    assert!(c.retry_tick().is_err());
    assert!(c.retry_route_commit().is_err());
    assert!(c.retry_checkpoint().is_err());
    assert_eq!(
        fs::read(path.join("world.checkpoint")).unwrap(),
        unsaved_bytes
    );
    assert_eq!(c.world.records(), records);
    assert_eq!(c.world.last_tick(), Some(&record));
    assert!(c.pump(Duration::from_secs(600)).events.is_empty());
    assert_eq!(observed.committed.lock().unwrap().len(), committed);
    fs::remove_file(path.join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    assert_eq!(c.store.generation(), generation + 1);
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    assert!(c.world.records().is_empty());
    assert!(c.world.last_tick().is_none());
    assert_eq!(
        c.route_commit
            .as_ref()
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap(),
        &notice
    );
    assert_eq!(observed.committed.lock().unwrap().len(), committed);
    let attempts = observed.attempted.lock().unwrap().len();
    if lose_ack {
        observed.lose_ack.store(true, Ordering::SeqCst);
    }
    let published = c.pump(Duration::ZERO);
    if lose_ack {
        assert_eq!(
            published.pause.as_ref().unwrap().kind,
            PauseKind::CommittedAwaitingRouteAck
        );
        restart::assert_ack_retry(path, c, observed, &notice, attempts);
    } else {
        assert!(published.pause.is_none(), "{:?}", published.pause);
    }
    assert_eq!(published.ticks, 0);
    assert_eq!(published.events.len(), events);
    assert_eq!(format!("{:?}", published.events), projected);
    assert_eq!(observed.committed.lock().unwrap().len(), committed + 1);
    assert_eq!(observed.committed.lock().unwrap().last(), Some(&notice));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state(), &state);
    assert_eq!(observed.controls.calls.load(Ordering::SeqCst), calls);
    published.events
}
fn copy_durable_store(h: &Harness, world: &n::NativeWorld, name: &str) -> PathBuf {
    let path = h.root.join(name);
    let genesis = n::NativeWorld::synthetic_component_test(
        world.profile().clone(),
        world.state().world,
        world.state().player,
        7,
    )
    .unwrap();
    // Let the real store establish its private directory and file modes, then
    // replace only the fixture payloads with the actual published envelopes.
    drop(NativeStore::create(&path, &genesis).unwrap());
    for file in ["world.checkpoint", "world.previous"] {
        fs::copy(h.path().join(file), path.join(file)).unwrap();
        assert_eq!(
            fs::read(path.join(file)).unwrap(),
            fs::read(h.path().join(file)).unwrap(),
            "copied {file} must contain the original durable bytes"
        );
    }
    path
}

#[test]
fn copied_durable_fixture_reopens_exact_published_bytes() {
    let h = Harness::new();
    let mut c = enter(&h, loop_fixture::profile(), false, Controls::default());
    step(&mut c);
    let checkpoint = c.world.checkpoint().unwrap();
    let path = copy_durable_store(&h, &c.world, "copy-regression");
    let published: Vec<_> = ["world.checkpoint", "world.previous"]
        .map(|file| {
            let bytes = fs::read(h.path().join(file)).unwrap();
            assert_eq!(fs::read(path.join(file)).unwrap(), bytes);
            (file, bytes)
        })
        .into();
    let mut restored_base = false;
    let result = NativeStore::open(&path, Identity::of(&checkpoint), |base| {
        let world =
            n::NativeWorld::restore_synthetic_component_test(c.world.profile().clone(), base);
        restored_base |= world.is_ok();
        world
    });
    assert!(
        result.is_ok(),
        "copied fixture must reach runtime restore, got {:?}",
        result.as_ref().err()
    );
    let (_store, restored) = result.unwrap();
    assert!(restored_base);
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
    for (file, bytes) in published {
        assert_eq!(fs::read(path.join(file)).unwrap(), bytes);
    }
}
fn reject_checkpoint_reopen(
    h: &Harness,
    world: &n::NativeWorld,
    name: &str,
    base_must_restore: bool,
) {
    let checkpoint = world.checkpoint().unwrap();
    assert_eq!(
        world.validate_saved_session_reopen(),
        Err(n::NativeError::UnsupportedMechanic(
            "saved native PathFinder cache rehydration"
        ))
    );
    assert!(n::NativeWorld::restore_synthetic_component_test(
        world.profile().clone(),
        checkpoint.clone()
    )
    .is_err());
    let path = copy_durable_store(h, world, name);
    reject_open(
        &path,
        Identity::of(&checkpoint),
        world.profile(),
        base_must_restore,
    );
}
fn reject_journal_reopen(h: &Harness, before: &n::NativeWorld, after: &n::NativeWorld) {
    assert_eq!(before.validate_saved_session_reopen(), Ok(()));
    assert!(!after.records().is_empty());
    let path = copy_durable_store(h, before, "loaded-from-journal");
    let identity = Identity::of(&before.checkpoint().unwrap());
    let (mut store, restored) = NativeStore::open(&path, identity.clone(), |checkpoint| {
        n::NativeWorld::restore_synthetic_component_test(before.profile().clone(), checkpoint)
    })
    .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), before.checkpoint().unwrap());
    store.commit(store.prepare(after).unwrap()).unwrap();
    drop(store);
    reject_open(&path, identity, after.profile(), true);
}
fn reject_open(path: &Path, identity: Identity, profile: &n::Profile, base_must_restore: bool) {
    let bytes = fs::read(path.join("world.checkpoint")).unwrap();
    let previous = fs::read(path.join("world.previous")).unwrap();
    for _ in 0..2 {
        let mut restored_base = false;
        let result = NativeStore::open(path, identity.clone(), |checkpoint| {
            let result =
                n::NativeWorld::restore_synthetic_component_test(profile.clone(), checkpoint);
            restored_base |= result.is_ok();
            result
        });
        if base_must_restore {
            assert!(matches!(
                &result,
                Err(benilla_world_store::native::Error::Runtime(
                    n::NativeError::UnsupportedMechanic(
                        "saved native PathFinder cache rehydration"
                    )
                ))
            ), "cached journal must reach the post-replay reopen guard; restored_base={restored_base}, error={:?}", result.as_ref().err());
        } else {
            assert!(matches!(
                &result,
                Err(benilla_world_store::native::Error::Runtime(
                    n::NativeError::InvalidCheckpoint
                ))
            ), "cached base must fail runtime restoration; restored_base={restored_base}, error={:?}", result.as_ref().err());
        }
        assert_eq!(restored_base, base_must_restore);
        assert_eq!(fs::read(path.join("world.checkpoint")).unwrap(), bytes);
        assert_eq!(fs::read(path.join("world.previous")).unwrap(), previous);
    }
}
