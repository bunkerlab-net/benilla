// SPDX-License-Identifier: GPL-3.0-or-later
//! Captured source-shaped capability rows through the actual controller and store.
//! Spatial answers are explicitly authored fixtures, never a native geometry provider.
use super::*;
use benilla_sim::{classic_spell as spell, creature_ai as ai, creature_motion as motion};
use n::{synthetic_motion_fixture as source, CreatureCapabilityRows as Rows};

fn profile() -> n::Profile {
    let mut profile = source::profile();
    profile
        .initialize_creature_capabilities(source::creature_capability_sources(&profile))
        .unwrap();
    profile
}

fn movement(flags: u32, x: f32, orientation: f32) -> ClientCommand {
    ClientCommand::Move {
        kind: MoveKind::Heartbeat,
        flags,
        pos: [x, 0.0, 0.0],
        orientation,
        pitch: 0.0,
        fall_time: 0,
        jump: None,
        transport: None,
    }
}

fn source_rows(profile: &n::Profile) -> Vec<n::CreatureCapabilitySource> {
    let Rows::Complete(rows) = source::creature_capability_sources(profile) else {
        panic!("authored fixture must provide explicit source rows")
    };
    rows
}

#[derive(Default)]
struct ObservedAuthority {
    // 1 refuses NearPoint; 2 returns an incorrectly bound echo.
    fault: Arc<AtomicU64>,
    observed: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    inner: source::SyntheticMotionAuthority,
}
impl n::Authority for ObservedAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.observed.lock().unwrap().push(request.clone());
        let near_point = matches!(
            request.query,
            n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
        );
        if near_point && self.fault.load(Ordering::SeqCst) == 1 {
            return Err(n::NativeError::AuthorityUnavailable(
                "authored capability NearPoint fault",
            ));
        }
        let mut reply = self.inner.resolve(request)?;
        if near_point && self.fault.load(Ordering::SeqCst) == 2 {
            let n::Query::SourceMotion(n::MotionQuery::NearPoint(q)) = &mut reply.request.query
            else {
                unreachable!()
            };
            q.request.owner.position.x = fixture::real(17.0);
        }
        Ok(reply)
    }
}

fn enter(
    harness: &Harness,
    entry: Entry,
    profile: n::Profile,
    authority: ObservedAuthority,
) -> NativeController {
    harness.enter(entry, profile, authority).unwrap()
}

fn create(
    harness: &Harness,
    profile: n::Profile,
    authority: ObservedAuthority,
) -> NativeController {
    enter(
        harness,
        Entry::Create {
            world: [8; 16],
            seed: 7,
        },
        profile,
        authority,
    )
}

fn near_points(requests: &[n::AuthorityRequest]) -> Vec<&n::CreatureNearPointRequest> {
    requests
        .iter()
        .filter_map(|request| match &request.query {
            n::Query::SourceMotion(n::MotionQuery::NearPoint(query)) => Some(query),
            _ => None,
        })
        .collect()
}

fn durable_files(harness: &Harness) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(harness.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.file_name().unwrap() != "world.lock")
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[test]
fn selected_capability_chase_and_moving_replan_bind_current_state_and_replay_receipts() {
    let h = Harness::new();
    let data = profile();
    let observed = Arc::new(Mutex::new(vec![]));
    let mut c = create(
        &h,
        data.clone(),
        ObservedAuthority {
            observed: observed.clone(),
            ..Default::default()
        },
    );
    c.initial_events().unwrap();
    let genesis = c.world.clone();
    let initial_capabilities = c.world.state().npcs[&2]
        .creature_capabilities()
        .unwrap()
        .clone();
    let chase_events = pump_to(&mut c, 2);
    assert_eq!(c.world.state().npcs[&2].ai.phase(), ai::Phase::Combat);
    let npc_guid = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
    assert_eq!(
        chase_events
            .iter()
            .filter(|event| matches!(event,
            SessionEvent::MonsterMove { guid, stop: false, .. } if *guid == npc_guid))
            .count(),
        1
    );
    let receipts = c.world.records().to_vec();
    let requests: Vec<_> = receipts
        .iter()
        .flat_map(|record| &record.replies)
        .map(|reply| reply.request.clone())
        .collect();
    let initial = near_points(&requests);
    assert_eq!(initial.len(), 1);
    let first = initial[0];
    assert_eq!(first.searcher.npc(), c.world.state().npcs[&2].id);
    assert_eq!(first.searcher.inhabit_type(), 1);
    assert_eq!(first.searcher.capability_movement_flags(), 0);
    assert_eq!(&first.searcher, initial_capabilities.witness());
    let capabilities = first.searcher.capabilities().unwrap();
    assert!(capabilities.can_walk());
    assert!(!capabilities.can_swim());
    assert!(!capabilities.can_fly());
    assert_eq!(
        first.searcher.allowed_position_z_branch().unwrap(),
        n::AllowedPositionZBranch::Ground
    );
    assert_eq!(first.request.owner.position, data.npcs[0].ai.spawn);
    assert_eq!(first.request.target.pose.position, data.player.start);
    assert_eq!(
        first.request.target.movement_flags,
        motion::GroundMovementFlags::STOPPED
    );
    assert_eq!(first.request.binding.now_ms, 200);
    assert_eq!(first.request.binding.identity.creature, 2);
    assert_eq!(first.request.binding.identity.map, data.map_id);
    assert_eq!(
        first.request.binding.identity.geometry.0,
        data.geometry_revision
    );
    let wrapped = requests
        .iter()
        .find(|request| {
            matches!(
                request.query,
                n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
            )
        })
        .unwrap();
    assert_eq!(wrapped.binding.world, [8; 16]);
    assert_eq!(wrapped.binding.tick, 2);
    assert_eq!(wrapped.binding.profile, data.digest());
    assert_eq!(wrapped.binding.npc, Some(c.world.state().npcs[&2].id));
    let near_reply = receipts
        .iter()
        .flat_map(|record| &record.replies)
        .find(|reply| &reply.request == wrapped)
        .unwrap();
    assert!(matches!(&near_reply.facts,
        n::Facts::SourceMotion(n::MotionFacts::Spatial(reply))
            if reply.request == motion::Query::NearPoint(first.request.clone())
                && matches!(reply.fact, motion::Fact::NearPoint(_))));
    assert!(observed
        .lock()
        .unwrap()
        .iter()
        .any(|request| request == wrapped));
    assert!(!requests.iter().any(|request| matches!(
        request.query,
        n::Query::SourceMotion(n::MotionQuery::Spatial(motion::Query::NearPoint(_)))
    )));
    assert!(c.world.state().npcs[&2].ai.position().x.as_f32() < 16.0);
    let mut replay = genesis.clone();
    replay.replay(&receipts).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());

    for field in 0..8 {
        let mut substituted = receipts.clone();
        let reply = substituted
            .iter_mut()
            .flat_map(|record| &mut record.replies)
            .find(|reply| {
                matches!(
                    reply.request.query,
                    n::Query::SourceMotion(n::MotionQuery::NearPoint(_))
                )
            })
            .unwrap();
        let n::Query::SourceMotion(n::MotionQuery::NearPoint(query)) = &mut reply.request.query
        else {
            unreachable!()
        };
        match field {
            0 => query.request.owner.position.x = fixture::real(17.0),
            1 => query.request.target.pose.position.y = fixture::real(0.5),
            2 => query.request.binding.query_id += 1,
            3 => reply.request.binding.npc.as_mut().unwrap().generation += 1,
            _ => {
                // A serialized witness is transcript data, not a way to write
                // live capabilities. Replay must reject each altered field.
                let mut value = serde_json::to_value(&query.searcher).unwrap();
                match field {
                    4 => {
                        let byte = value["source_binding"][0].as_u64().unwrap();
                        value["source_binding"][0] = serde_json::json!(byte ^ 1);
                    }
                    5 => value["inhabit_type"] = serde_json::json!(3),
                    6 => {
                        value["capability_movement_flags"] =
                            serde_json::json!(benilla_sim::creature_capabilities::MOVEFLAG_HOVER)
                    }
                    _ => value["npc"]["generation"] = serde_json::json!(1),
                }
                query.searcher = serde_json::from_value(value).unwrap();
            }
        }
        let mut rejected = genesis.clone();
        assert!(rejected.replay(&substituted).is_err());
        assert_eq!(
            rejected.checkpoint().unwrap(),
            genesis.checkpoint().unwrap()
        );
    }

    let initial_source = first.searcher.source_binding();
    let mut replanned = false;
    for _ in 0..12 {
        let x = c.world.state().position.x.as_f32() - 0.7;
        c.receive(movement(1, x, 0.5)).unwrap();
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none());
        let observed = observed.lock().unwrap();
        if let Some(query) = near_points(&observed).last() {
            if query.request.target.pose.position != data.player.start {
                assert_eq!(query.request.target.pose.position, c.world.state().position);
                assert_eq!(query.request.target.movement_flags.bits(), 1);
                assert_eq!(query.searcher.source_binding(), initial_source);
                assert_eq!(query.searcher.capability_movement_flags(), 0);
                assert_eq!(
                    c.world.state().npcs[&2].creature_capabilities(),
                    Some(&initial_capabilities)
                );
                replanned = true;
                break;
            }
        }
    }
    assert!(
        replanned,
        "moving target did not reach the capability-bound replan"
    );
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(
        reopened.world.state().npcs[&2].creature_capabilities(),
        Some(&initial_capabilities)
    );
}

#[test]
fn capability_authority_fault_and_store_failure_retain_exact_batch_then_publish_once() {
    for fault_kind in [1, 2] {
        let h = Harness::new();
        let data = profile();
        let fault = Arc::new(AtomicU64::new(fault_kind));
        let observed = Arc::new(Mutex::new(vec![]));
        let mut c = create(
            &h,
            data.clone(),
            ObservedAuthority {
                fault: fault.clone(),
                observed: observed.clone(),
                ..Default::default()
            },
        );
        c.initial_events().unwrap();
        pump_to(&mut c, 1);
        let before = c.world.checkpoint().unwrap();
        let persisted = durable_files(&h);
        let generation = c.store.generation();
        let mut expected = c.world.clone();
        let mut replay = c.world.clone();
        c.receive(movement(1, -0.25, 0.75)).unwrap();
        let out = c.pump(c.tick_duration());
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(out.pause.is_some());
        assert_eq!(c.world.checkpoint().unwrap(), before);
        assert_eq!(c.store.generation(), generation);
        assert_eq!(durable_files(&h), persisted);
        let inputs = c.pending.as_ref().unwrap().inputs.clone();
        if fault_kind == 2 {
            assert!(matches!(out.pause.unwrap().kind, PauseKind::Simulation));
            assert!(matches!(c.retry_tick(), Err(Error::Paused)));
            assert_eq!(c.pending.as_ref().unwrap().inputs, inputs);
            assert_eq!(c.position_xyz(), [0.0; 3]);
            // A bad binding is not a retryable unavailable-authority result.
            // Final save retains the last committed tick and cancels this input.
            let digest = c.world.state_digest();
            c.shutdown().unwrap();
            drop(c);
            let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
            assert_eq!(reopened.world.state_digest(), digest);
            assert_eq!(reopened.world.state().next_sequence, 0);
            continue;
        }
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        expected
            .step_batch(
                2,
                inputs.clone(),
                &mut source::SyntheticMotionAuthority::default(),
            )
            .unwrap();
        let attempted = (*near_points(&observed.lock().unwrap())[0]).clone();
        c.pump(Duration::from_secs(60));
        assert_eq!(c.pending.as_ref().unwrap().inputs, inputs);
        assert_eq!(c.world.checkpoint().unwrap(), before);
        fault.store(0, Ordering::SeqCst);
        fs::write(
            h.path().join(".world-checkpoint.pending"),
            b"authored capability store fault",
        )
        .unwrap();
        c.retry_tick().unwrap();
        let out = c.pump(Duration::ZERO);
        assert_eq!(out.ticks, 1);
        assert!(out.events.is_empty());
        assert!(matches!(
            out.pause.unwrap().kind,
            PauseKind::Checkpoint {
                requires_reopen: false
            }
        ));
        assert!(c.pending.is_none());
        assert_eq!(
            c.world.checkpoint().unwrap(),
            expected.checkpoint().unwrap()
        );
        assert_eq!(c.world.state().next_sequence, 1);
        assert_eq!(c.position_xyz(), [0.0; 3]);
        assert_eq!(c.store.generation(), generation);
        let observed = observed.lock().unwrap();
        let attempts = near_points(&observed);
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0], &attempted);
        assert_eq!(attempts[1], &attempted);
        drop(observed);
        replay
            .replay(std::slice::from_ref(c.world.last_tick().unwrap()))
            .unwrap();
        assert_eq!(replay.checkpoint().unwrap(), c.world.checkpoint().unwrap());
        let digest = c.world.state_digest();
        fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
        c.retry_checkpoint().unwrap();
        assert_eq!(c.world.state_digest(), digest);
        let out = c.pump(Duration::ZERO);
        assert_eq!(out.ticks, 0);
        assert!(!out.events.is_empty());
        let npc_guid = c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap();
        assert_eq!(
            out.events
                .iter()
                .filter(|event| matches!(event,
            SessionEvent::MonsterMove { guid, stop: false, .. } if *guid == npc_guid))
                .count(),
            1
        );
        assert_eq!(c.position_xyz(), [-0.25, 0.0, 0.0]);
        assert!(c.pump(Duration::ZERO).events.is_empty());
        assert!(c.world.records().is_empty());
        assert_eq!(c.world.state().next_sequence, 1);
        c.shutdown().unwrap();
        drop(c);
        let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
        assert_eq!(reopened.world.state_digest(), digest);
        assert_eq!(reopened.world.state().next_sequence, 1);
    }
}

#[test]
fn missing_capability_rows_and_water_capable_sources_never_mutate_or_create_a_world() {
    let selected = profile();
    let original = selected.digest();
    for missing in 0..7 {
        let mut rows = source_rows(&selected);
        let attempted = match missing {
            0 => Rows::Missing,
            1 => Rows::Complete(vec![]),
            _ => {
                match missing {
                    2 => rows[0].templates = Rows::Missing,
                    3 => rows[0].spawn_templates = Rows::Missing,
                    4 => rows[0].external_features = Rows::Missing,
                    5 => rows[0].requested_spawn_template_entry += 1,
                    _ => {
                        let Rows::Complete(templates) = &mut rows[0].templates else {
                            unreachable!()
                        };
                        templates[0].inhabit_type = 3;
                    }
                }
                Rows::Complete(rows)
            }
        };
        let mut candidate = selected.clone();
        assert!(candidate
            .initialize_creature_capabilities(attempted)
            .is_err());
        assert_eq!(candidate.digest(), original);
        assert_eq!(candidate, selected);
    }

    for malformed in 0..4 {
        let h = Harness::new();
        let mut candidate = selected.clone();
        let mut rows = source_rows(&selected);
        match malformed {
            0 => rows.clear(),
            1 => rows[0].spawn_id += 1,
            2 => {
                rows[0].spawn_entry += 1;
                let Rows::Complete(templates) = &mut rows[0].templates else {
                    unreachable!()
                };
                templates[0].entry += 1;
            }
            _ => {
                let Rows::Complete(templates) = &mut rows[0].templates else {
                    unreachable!()
                };
                // Source CanWalk is true for habitat3, but that does not exclude
                // water or justify a ground-only runtime's source closure.
                templates[0].inhabit_type = 3;
            }
        }
        candidate.creature_capabilities =
            Some(n::CreatureCapabilityCatalog::validate_sources(Rows::Complete(rows)).unwrap());
        let observed = Arc::new(Mutex::new(vec![]));
        let result = h.enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            candidate,
            ObservedAuthority {
                observed: observed.clone(),
                ..Default::default()
            },
        );
        assert!(result.is_err());
        assert!(!h.path().exists());
        assert!(observed.lock().unwrap().is_empty());
    }
}

#[test]
fn missing_or_changed_selected_catalog_cannot_reopen_or_rewrite_durable_capabilities() {
    let h = Harness::new();
    let data = profile();
    let mut c = create(&h, data.clone(), Default::default());
    c.initial_events().unwrap();
    pump_to(&mut c, 2);
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let persisted = durable_files(&h);
    for removed in [true, false] {
        let mut changed = data.clone();
        if removed {
            changed.creature_capabilities = None;
        } else {
            let mut rows = source_rows(&changed);
            let Rows::Complete(spawn_templates) = &mut rows[0].spawn_templates else {
                unreachable!()
            };
            spawn_templates[0].spawn_flags = 0;
            changed
                .initialize_creature_capabilities(Rows::Complete(rows))
                .unwrap();
        }
        assert_ne!(changed.digest(), data.digest());
        let observed = Arc::new(Mutex::new(vec![]));
        assert!(h
            .enter(
                Entry::Open { world: [8; 16] },
                changed,
                ObservedAuthority {
                    observed: observed.clone(),
                    ..Default::default()
                },
            )
            .is_err());
        assert_eq!(durable_files(&h), persisted);
        assert!(observed.lock().unwrap().is_empty());
    }
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
    assert_eq!(reopened.world.state_digest(), digest);
}

#[test]
fn source_death_despawn_and_respawn_rebind_capabilities_to_the_new_generation() {
    let h = Harness::new();
    let mut data = source::profile();
    data.npcs[0].max_health = 1;
    data.npcs[0].ai.reaction = ai::Reaction::Defensive;
    data.npcs[0].ai.corpse_decay_ms = 500;
    let n::AbilityAdmission::Available(spec) = &mut data.abilities[0].admission else {
        unreachable!()
    };
    let spell::MageEffect::SchoolDamage(mut damage) = spec.effects[0] else {
        unreachable!()
    };
    damage.base_points = 99;
    damage.base_dice = 1;
    damage.die_sides = 1;
    spec.effects = [
        spell::MageEffect::SchoolDamage(damage),
        spell::MageEffect::Empty,
        spell::MageEffect::Empty,
    ];
    data.initialize_creature_capabilities(source::creature_capability_sources(&data))
        .unwrap();
    let observed = Arc::new(Mutex::new(vec![]));
    let mut c = create(
        &h,
        data.clone(),
        ObservedAuthority {
            observed: observed.clone(),
            inner: source::SyntheticMotionAuthority {
                critical_chance: Some(0),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    c.initial_events().unwrap();
    pump_to(&mut c, 2);
    let initial_id = c.world.state().npcs[&2].id;
    let initial = c.world.state().npcs[&2]
        .creature_capabilities()
        .unwrap()
        .clone();
    c.receive(cast(&c)).unwrap();
    while c.world.state().npcs[&2].health != 0 {
        assert!(
            c.tick() < 100,
            "source spell never killed the authored target"
        );
        let next = c.tick() + 1;
        pump_to(&mut c, next);
    }
    assert_eq!(c.world.state().npcs[&2].ai.phase(), ai::Phase::Corpse);
    assert_eq!(
        c.world.state().npcs[&2].creature_capabilities(),
        Some(&initial)
    );
    assert!(near_points(&observed.lock().unwrap())
        .iter()
        .any(|query| query.searcher.npc() == initial_id));
    let corpse_digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let mut c = enter(
        &h,
        Entry::Open { world: [8; 16] },
        data.clone(),
        Default::default(),
    );
    assert_eq!(c.world.state_digest(), corpse_digest);
    assert_eq!(
        c.world.state().npcs[&2].creature_capabilities(),
        Some(&initial)
    );
    c.initial_events().unwrap();
    let mut saw_despawn = false;
    while c.world.state().npcs[&2].id == initial_id {
        assert!(c.tick() < 150, "authored source respawn did not complete");
        let next = c.tick() + 1;
        pump_to(&mut c, next);
        saw_despawn |= c.world.state().npcs[&2].ai.phase() == ai::Phase::Despawned;
    }
    assert!(saw_despawn);
    let npc = &c.world.state().npcs[&2];
    assert_eq!(npc.id.spawn, initial_id.spawn);
    assert_eq!(npc.id.generation, initial_id.generation + 1);
    let current = npc.creature_capabilities().unwrap().witness();
    assert_eq!(current.npc(), npc.id);
    assert_eq!(current.source_binding(), initial.witness().source_binding());
    assert_eq!(current.inhabit_type(), initial.witness().inhabit_type());
    assert_eq!(
        current.capability_movement_flags(),
        initial.witness().capability_movement_flags()
    );
    assert_eq!(
        current.capabilities().unwrap(),
        initial.witness().capabilities().unwrap()
    );
    let respawn_digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let reopened = enter(&h, Entry::Open { world: [8; 16] }, data, Default::default());
    assert_eq!(reopened.world.state_digest(), respawn_digest);
    assert_eq!(
        reopened.world.state().npcs[&2]
            .creature_capabilities()
            .unwrap()
            .witness()
            .npc()
            .generation,
        1
    );
}
