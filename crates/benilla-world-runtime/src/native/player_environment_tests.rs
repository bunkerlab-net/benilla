// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored component evidence, not real-world admission or full SetPosition parity.
use super::{
    synthetic_fixture as f, synthetic_motion_fixture as motion,
    synthetic_player_model_fixture as models, *,
};
use environment::{
    BoundLiquidResult, EnvironmentSnapshotBinding, LiquidCatalog, LiquidData, LiquidStatus,
};

fn catalog_bytes() -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [2_u32, 4, 16, 1, 1, 1, 3, 0, 2, 1, 3, 123] {
        bytes.extend(word.to_le_bytes());
    }
    bytes.push(0);
    bytes
}
fn catalog() -> LiquidCatalog {
    LiquidCatalog::from_wdbc(&catalog_bytes()).unwrap()
}
fn binding(p: &Profile) -> EnvironmentSnapshotBinding {
    EnvironmentSnapshotBinding {
        map: p.map_id,
        content: p.content_digest,
        source_epoch: [10; 32],
        inventory: [11; 32],
        policy: [12; 32],
        liquid_catalog: catalog().digest(),
    }
}
fn profile() -> Profile {
    let mut p = models::profile();
    p.npcs[0].ai.reaction = ai::Reaction::Passive;
    p.tick_ms = 25;
    p.initialize_player_environment(binding(&p), catalog())
        .unwrap();
    p
}
fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 1234).unwrap()
}
fn pose(x: f32, angle: f32) -> Command {
    Command::MovementPose {
        destination: f::point(x),
        orientation: Orientation::new(angle).unwrap(),
        movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
    }
}
fn inputs(w: &NativeWorld, commands: Vec<Command>) -> Vec<OwnedInput> {
    commands
        .into_iter()
        .enumerate()
        .map(|(i, command)| OwnedInput {
            authenticated_owner: w.state().player.owner,
            input: Input {
                world: w.state().world,
                player: w.state().player,
                tick: w.state().tick + 1,
                sequence: w.state().next_sequence + i as u64,
                command,
            },
        })
        .collect()
}
#[derive(Default)]
struct AuthorityFixture {
    motion: motion::SyntheticMotionAuthority,
    seen: Vec<AuthorityRequest>,
    status: Option<LiquidStatus>,
    entry: u32,
    type_flags: u32,
    reject_movement: bool,
    fail_late: bool,
    forged: Option<serde_json::Value>,
}
impl Authority for AuthorityFixture {
    fn resolve(&mut self, q: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.seen.push(q.clone());
        if let Query::PlayerEnvironment(query) = &q.query {
            let status = self.status.unwrap_or(LiquidStatus::NoWater);
            let facts = if let Some(value) = &self.forged {
                serde_json::from_value(value.clone()).unwrap()
            } else {
                PlayerEnvironmentFacts::new(
                    query,
                    BoundLiquidResult {
                        query: query.liquid_query(),
                        status,
                        data: (status != LiquidStatus::NoWater).then_some(LiquidData {
                            entry: self.entry,
                            type_flags: self.type_flags,
                            level: 0.0_f32.to_bits(),
                            depth_level: (-10.0_f32).to_bits(),
                        }),
                    },
                )?
            };
            return Ok(AuthorityReply {
                request: q.clone(),
                facts: Facts::PlayerEnvironment(facts),
            });
        }
        if matches!(q.query, Query::PlayerMovement { .. }) && self.reject_movement {
            return Ok(AuthorityReply {
                request: q.clone(),
                facts: Facts::PlayerMovement { accepted: false },
            });
        }
        if self.fail_late
            && matches!(&q.query, Query::SourceMotion(MotionQuery::Visitor(v)) if matches!(v.owner, VisitorOwner::Creature(_)))
        {
            return Err(NativeError::AuthorityUnavailable("late NPC callback"));
        }
        self.motion.resolve(q)
    }
}
fn step(
    w: &mut NativeWorld,
    commands: Vec<Command>,
    a: &mut AuthorityFixture,
) -> Result<(), NativeError> {
    let i = inputs(w, commands);
    w.step_batch(w.state().tick + 1, i, a).map(|_| ())
}
fn env_queries(a: &AuthorityFixture) -> Vec<&PlayerEnvironmentQuery> {
    a.seen
        .iter()
        .filter_map(|r| {
            if let Query::PlayerEnvironment(q) = &r.query {
                Some(q.as_ref())
            } else {
                None
            }
        })
        .collect()
}
fn phases(w: &NativeWorld) -> Vec<&PlayerEnvironmentReceipt> {
    w.last_tick()
        .unwrap()
        .events
        .iter()
        .filter_map(|e| {
            if let Event::PlayerEnvironment(r) = e {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
#[test]
fn selection_is_explicit_atomic_complete_and_requires_owned_height() {
    let mut p = models::unowned_profile();
    let before = p.clone();
    assert!(p
        .initialize_player_environment(binding(&p), catalog())
        .is_err());
    assert_eq!(p, before);
    let mut p = models::profile();
    for field in 0..7 {
        let mut b = binding(&p);
        match field {
            0 => b.map += 1,
            1 => b.content = [13; 32],
            2 => b.source_epoch = [0; 32],
            3 => b.inventory = [0; 32],
            4 => b.policy = [0; 32],
            5 => b.liquid_catalog = [9; 32],
            _ => {}
        }
        let c = if field == 6 {
            LiquidCatalog::from_legacy_wdbc(&catalog_bytes()).unwrap()
        } else {
            catalog()
        };
        let before = p.clone();
        assert!(p.initialize_player_environment(b, c).is_err(), "{field}");
        assert_eq!(p, before);
    }
    let old = p.digest();
    p.initialize_player_environment(binding(&p), catalog())
        .unwrap();
    assert_ne!(p.digest(), old);
    let before = p.clone();
    p.initialize_player_environment(binding(&p), catalog())
        .unwrap();
    assert_eq!(p, before);
    let mut changed = binding(&p);
    changed.source_epoch = [99; 32];
    assert!(p.initialize_player_environment(changed, catalog()).is_err());
    assert_eq!(p, before);
    p.player_collision_height = None;
    assert!(p.validate().is_err());
}
#[test]
fn poses_run_in_order_after_relocation_and_countdown_runs_once_after_commands() {
    let mut w = world(profile());
    let mut a = AuthorityFixture::default();
    step(
        &mut w,
        vec![pose(1.0, 0.0), pose(2.0, 0.5), pose(2.0, 0.5)],
        &mut a,
    )
    .unwrap();
    let q = env_queries(&a);
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].pose().position, f::point(1.0));
    assert_eq!(
        q[0].height_witness().height().bits(),
        w.state()
            .player_collision_height()
            .unwrap()
            .witness()
            .height()
            .bits()
    );
    assert_eq!(
        q[0].liquid_query().collision_height,
        q[0].height_witness().height().bits()
    );
    q[0].validate_for(w.profile(), w.state().world, w.state().player)
        .unwrap();
    let cache = w.state().player_environment().unwrap();
    assert_eq!(cache.sequence(), 4);
    assert_eq!(cache.environment().timer_ms(), 75);
    assert_eq!(
        cache.environment().cached_query().unwrap().position,
        [1.0_f32.to_bits(), 0, 0]
    );
    assert_eq!(
        cache.environment().last_position().unwrap().position,
        [2.0_f32.to_bits(), 0, 0]
    );
    let r = phases(&w);
    assert_eq!(r.len(), 4);
    assert!(matches!(
        r[0].phase,
        PlayerEnvironmentPhase::Position {
            refreshed: true,
            ..
        }
    ));
    assert!(matches!(
        r[1].phase,
        PlayerEnvironmentPhase::Position {
            refreshed: false,
            ..
        }
    ));
    assert!(matches!(
        r[2].phase,
        PlayerEnvironmentPhase::Position {
            refreshed: false,
            ..
        }
    ));
    assert!(matches!(
        r[3].phase,
        PlayerEnvironmentPhase::Update { diff_ms: 25 }
    ));
    for pair in r.windows(2) {
        assert_eq!(pair[0].after, pair[1].before);
        assert_eq!(pair[0].sequence + 1, pair[1].sequence);
    }
    for _ in 0..4 {
        step(&mut w, vec![], &mut a).unwrap();
    }
    assert_eq!(env_queries(&a).len(), 1);
    assert_eq!(
        w.state()
            .player_environment()
            .unwrap()
            .environment()
            .timer_ms(),
        0
    );
    step(&mut w, vec![pose(2.0, 0.5)], &mut a).unwrap();
    assert_eq!(env_queries(&a).len(), 2);
    for _ in 0..3 {
        step(&mut w, vec![], &mut a).unwrap();
    }
    step(&mut w, vec![pose(2.0, 0.75)], &mut a).unwrap();
    assert_eq!(env_queries(&a).len(), 3);
    assert_eq!(env_queries(&a)[2].pose().orientation, f::real(0.75));
}
#[test]
fn timer_one_is_checked_before_update_and_rejected_or_dead_poses_have_no_phase() {
    let mut p = profile();
    p.tick_ms = 99;
    let mut w = world(p);
    let mut a = AuthorityFixture::default();
    step(&mut w, vec![pose(0.0, 0.0)], &mut a).unwrap();
    assert_eq!(
        w.state()
            .player_environment()
            .unwrap()
            .environment()
            .timer_ms(),
        1
    );
    step(&mut w, vec![pose(1.0, 0.0)], &mut a).unwrap();
    assert_eq!(env_queries(&a).len(), 1);
    assert_eq!(
        w.state()
            .player_environment()
            .unwrap()
            .environment()
            .timer_ms(),
        0
    );
    a.reject_movement = true;
    step(&mut w, vec![pose(2.0, 0.0)], &mut a).unwrap();
    assert_eq!(env_queries(&a).len(), 1);
    assert_eq!(phases(&w).len(), 1);
    a.reject_movement = false;
    step(&mut w, vec![pose(1.0, 0.0)], &mut a).unwrap();
    assert_eq!(env_queries(&a).len(), 2);
    // Private mutation constructs a dead command boundary only; no dead checkpoint claim.
    w.state.health = 0;
    let sequence = w.state().player_environment().unwrap().sequence();
    step(&mut w, vec![pose(3.0, 0.0)], &mut a).unwrap();
    assert_eq!(env_queries(&a).len(), 2);
    assert_eq!(phases(&w).len(), 1);
    assert_eq!(
        w.state().player_environment().unwrap().sequence(),
        sequence + 1
    );
}
#[test]
fn unsupported_environment_results_and_late_npc_errors_rollback_whole_frame() {
    for (status, flags, entry) in [
        (LiquidStatus::InWater, 8, 1),
        (LiquidStatus::InWater, 0, 0),
        (LiquidStatus::UnderWater, 8, 1),
        (LiquidStatus::AboveWater, 0x12, 1),
        (LiquidStatus::AboveWater, 1, 1),
        (LiquidStatus::WaterWalk, 4, 1),
        (LiquidStatus::AboveWater, 8, 2),
    ] {
        let mut w = world(profile());
        let before = w.checkpoint().unwrap();
        let mut a = AuthorityFixture {
            status: Some(status),
            type_flags: flags,
            entry,
            ..Default::default()
        };
        assert!(
            step(&mut w, vec![pose(1.0, 0.3)], &mut a).is_err(),
            "{status:?} {flags}"
        );
        assert_eq!(w.checkpoint().unwrap(), before);
        assert!(w.records().is_empty());
    }
    let mut p = profile();
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.source_motion
        .as_mut()
        .unwrap()
        .creature_initial_aggro_delay_ms = 0;
    let mut w = world(p);
    let before = w.checkpoint().unwrap();
    let mut a = AuthorityFixture {
        fail_late: true,
        ..Default::default()
    };
    assert!(step(&mut w, vec![pose(0.0, 0.5)], &mut a).is_err());
    assert_eq!(env_queries(&a).len(), 1);
    assert!(
        matches!(&a.seen.last().unwrap().query, Query::SourceMotion(MotionQuery::Visitor(v)) if matches!(v.owner, VisitorOwner::Creature(_)))
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    assert!(w.records().is_empty());
}
#[test]
fn callback_free_surface_is_supported_but_combined_dry_clear_is_not() {
    for status in [LiquidStatus::AboveWater, LiquidStatus::WaterWalk] {
        let mut w = world(profile());
        let mut a = AuthorityFixture {
            status: Some(status),
            type_flags: 8,
            entry: 1,
            ..Default::default()
        };
        step(&mut w, vec![pose(0.0, 0.0)], &mut a).unwrap();
        assert_eq!(
            w.state()
                .player_environment()
                .unwrap()
                .environment()
                .flags(),
            0x40
        );
        for _ in 0..3 {
            step(&mut w, vec![], &mut a).unwrap();
        }
        a.status = Some(LiquidStatus::NoWater);
        let before = w.checkpoint().unwrap();
        assert!(matches!(
            step(&mut w, vec![pose(0.0, 0.0)], &mut a),
            Err(NativeError::PlayerEnvironment(
                environment::EnvironmentError::CallbacksUnavailable(
                    environment::EnvironmentCallback::UpdateThreatTables
                )
            ))
        ));
        assert_eq!(w.checkpoint().unwrap(), before);
    }
}
#[test]
fn native_roundtrip_replay_and_rehashed_cache_forgeries_fail_closed() {
    let p = profile();
    let mut w = world(p.clone());
    let initial = w.checkpoint().unwrap();
    let mut a = AuthorityFixture::default();
    step(&mut w, vec![pose(1.0, 0.5), pose(2.0, 0.75)], &mut a).unwrap();
    for _ in 0..4 {
        step(&mut w, vec![], &mut a).unwrap();
    }
    let checkpoint = w.checkpoint().unwrap();
    let decoded = Checkpoint::decode(&checkpoint.encode().unwrap()).unwrap();
    assert_eq!(decoded, checkpoint);
    let restored = NativeWorld::restore_synthetic_component_test(p.clone(), decoded).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
    let records = decode_records(&encode_records(w.records()).unwrap()).unwrap();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(p.clone(), initial.clone()).unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), checkpoint);
    for field in [
        "source",
        "world",
        "profile",
        "height",
        "flags",
        "timer",
        "sequence",
        "latest_pose",
        "query_height",
        "query_snapshot",
        "mirrors",
        "missing",
    ] {
        let mut c = checkpoint.clone();
        let mut json = serde_json::to_value(&c.state).unwrap();
        let v = &mut json["source_player_environment"];
        match field {
            "source" => v["owner"]["source"][0] = 99.into(),
            "world" | "profile" => v["owner"]["height"][field][0] = 99.into(),
            "height" => v["owner"]["height"]["height"] = 3.0_f32.to_bits().into(),
            "flags" => v["state"]["flags"] = 0x48.into(),
            "timer" => v["state"]["timer_ms"] = 101.into(),
            "sequence" => v["sequence"] = 99999.into(),
            "latest_pose" => v["last_pose"]["position"]["x"] = 3.0_f32.to_bits().into(),
            "query_height" => {
                v["state"]["cached_query"]["collision_height"] = 3.0_f32.to_bits().into()
            }
            "query_snapshot" => {
                v["state"]["cached_query"]["snapshot"]["source_epoch"][0] = 99.into()
            }
            "mirrors" => v["state"]["mirrors"][0]["scale"] = 10.into(),
            "missing" => *v = serde_json::Value::Null,
            _ => unreachable!(),
        }
        c.state = serde_json::from_value(json).unwrap();
        c.digest = crate::digest(b"benilla-native-state-v39\0", &c.state);
        c.checksum = crate::digest(
            b"benilla-native-checkpoint-v39\0",
            &(&c.state, c.digest, c.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(p.clone(), c).is_err(),
            "{field}"
        );
    }
    for field in ["before", "sequence", "pose", "receipt"] {
        let mut forged = records.clone();
        if field == "receipt" {
            let Event::PlayerEnvironment(r) = forged[0]
                .events
                .iter_mut()
                .find(|e| matches!(e, Event::PlayerEnvironment(_)))
                .unwrap()
            else {
                unreachable!()
            };
            r.sequence += 1;
        } else {
            let reply = forged[0]
                .replies
                .iter_mut()
                .find(|r| matches!(r.request.query, Query::PlayerEnvironment(_)))
                .unwrap();
            let mut value = serde_json::to_value(&*reply).unwrap();
            let q = &mut value["request"]["query"]["PlayerEnvironment"];
            match field {
                "before" => q["before"][0] = 99.into(),
                "sequence" => q["sequence"] = 99.into(),
                "pose" => q["pose"]["orientation"] = 1.0_f32.to_bits().into(),
                _ => unreachable!(),
            }
            *reply = serde_json::from_value(value).unwrap();
        }
        let mut replay =
            NativeWorld::restore_synthetic_component_test(p.clone(), initial.clone()).unwrap();
        assert!(replay.replay(&forged).is_err(), "{field}");
        assert_eq!(replay.checkpoint().unwrap(), initial);
    }
}
#[test]
fn every_facts_binding_is_checked_before_state_can_commit() {
    let mut baseline = world(profile());
    let mut a = AuthorityFixture::default();
    step(&mut baseline, vec![pose(0.0, 0.0)], &mut a).unwrap();
    let q = env_queries(&a)[0];
    let facts = PlayerEnvironmentFacts::new(
        q,
        BoundLiquidResult {
            query: q.liquid_query(),
            status: LiquidStatus::NoWater,
            data: None,
        },
    )
    .unwrap();
    for field in [
        "world",
        "player",
        "content",
        "profile",
        "geometry",
        "map",
        "source",
        "epoch",
        "inventory",
        "policy",
        "catalog",
        "sequence",
        "before",
        "pose",
        "height",
        "ticket",
        "mask",
        "status",
        "data",
    ] {
        let mut forged = serde_json::to_value(&facts).unwrap();
        match field {
            "world" | "content" | "profile" | "geometry" => {
                forged["query"]["owner"]["height"][field][0] = 99.into()
            }
            "player" => forged["query"]["owner"]["height"]["player"]["owner"][0] = 99.into(),
            "map" => forged["query"]["owner"]["height"]["map"] = 99.into(),
            "source" => forged["query"]["owner"]["source"][0] = 99.into(),
            "epoch" => forged["result"]["query"]["snapshot"]["source_epoch"][0] = 99.into(),
            "inventory" | "policy" => forged["result"]["query"]["snapshot"][field][0] = 99.into(),
            "catalog" => forged["result"]["query"]["snapshot"]["liquid_catalog"][0] = 99.into(),
            "sequence" => forged["query"]["sequence"] = 99.into(),
            "before" => forged["query"]["before"][0] = 99.into(),
            "pose" => forged["query"]["pose"]["position"]["x"] = 1.0_f32.to_bits().into(),
            "height" => forged["result"]["query"]["collision_height"] = 1.0_f32.to_bits().into(),
            "ticket" => forged["result"]["query"]["ticket"] = 99.into(),
            "mask" => forged["result"]["query"]["required_type"] = 1.into(),
            "status" => forged["result"]["status"] = "AboveWater".into(),
            "data" => {
                forged["result"]["data"] =
                    serde_json::json!({"entry":1,"type_flags":8,"level":0,"depth_level":0})
            }
            _ => unreachable!(),
        }
        let mut w = world(profile());
        let before = w.checkpoint().unwrap();
        let mut a = AuthorityFixture {
            forged: Some(forged),
            ..Default::default()
        };
        assert!(
            step(&mut w, vec![pose(0.0, 0.0)], &mut a).is_err(),
            "{field}"
        );
        assert_eq!(w.checkpoint().unwrap(), before);
    }
}
#[test]
fn optional_cache_fields_and_format26_are_not_silently_repaired() {
    for p in [models::profile(), profile()] {
        let w = world(p);
        let checkpoint = w.checkpoint().unwrap();
        let mut json = serde_json::to_value(&checkpoint).unwrap();
        assert!(json["state"]
            .as_object_mut()
            .unwrap()
            .remove("source_player_environment")
            .is_some());
        assert!(serde_json::from_value::<Checkpoint>(json).is_err());
        let mut json = serde_json::to_value(&checkpoint).unwrap();
        json["version"] = 26.into();
        assert!(Checkpoint::decode(&serde_json::to_vec(&json).unwrap()).is_err());
        if w.profile().player_environment.is_some() {
            let mut json = serde_json::to_value(&checkpoint).unwrap();
            json["state"]["source_player_environment"]
                .as_object_mut()
                .unwrap()
                .remove("last_pose");
            assert!(serde_json::from_value::<Checkpoint>(json).is_err());
        }
    }
}
#[test]
fn more_than_component_journal_limit_phases_use_only_native_journal() {
    let mut w = world(profile());
    let mut a = AuthorityFixture::default();
    for _ in 0..33 {
        step(&mut w, vec![pose(0.0, 0.0); 64], &mut a).unwrap();
    }
    assert_eq!(w.records().len(), 33);
    assert_eq!(w.state().player_environment().unwrap().sequence(), 33 * 65);
    assert!(
        w.state().player_environment().unwrap().sequence()
            > environment::MAX_ENVIRONMENT_RECORDS as u64
    );
    let c = w.checkpoint().unwrap();
    w.checkpoint_committed(&c).unwrap();
    assert!(w.records().is_empty());
    let mut restored =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), c).unwrap();
    step(&mut restored, vec![pose(0.0, 0.0)], &mut a).unwrap();
    assert_eq!(
        restored.state().player_environment().unwrap().sequence(),
        33 * 65 + 2
    );
}
#[test]
fn chase_carries_stale_cached_query_and_current_target_without_resampling() {
    let mut p = profile();
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    let mut w = world(p);
    let mut a = AuthorityFixture::default();
    step(&mut w, vec![pose(1.0, 0.0), pose(2.0, 0.5)], &mut a).unwrap();
    let count = env_queries(&a).len();
    while !a
        .seen
        .iter()
        .any(|q| matches!(&q.query, Query::SourceMotion(MotionQuery::Route(_))))
    {
        assert!(w.state().tick < 10);
        step(&mut w, vec![], &mut a).unwrap();
    }
    assert_eq!(env_queries(&a).len(), count);
    let route = a
        .seen
        .iter()
        .find_map(|q| {
            if let Query::SourceMotion(MotionQuery::Route(r)) = &q.query {
                Some(r.as_ref())
            } else {
                None
            }
        })
        .unwrap();
    let water = route
        .require_target_water(w.profile(), w.state().world, w.state().player)
        .unwrap();
    assert!(!water.in_water());
    assert_eq!(water.moment(), PlayerEnvironmentMoment::AfterCountdown);
    let record = w
        .records()
        .iter()
        .find(|record| record.tick == water.tick())
        .unwrap();
    let timer = record
        .events
        .iter()
        .filter_map(|event| {
            if let Event::PlayerEnvironment(receipt) = event {
                Some(receipt)
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    assert_eq!(timer.after, water.cache_digest());
    assert_eq!(timer.sequence, water.phase_sequence());
    assert_eq!(water.target().pose.position, f::point(2.0));
    let value = serde_json::to_value(water).unwrap();
    assert_eq!(
        value["snapshot"]["state"]["cached_query"]["position"][0],
        serde_json::json!(1.0_f32.to_bits())
    );
    for field in [
        "water", "sequence", "digest", "target", "source", "tick", "moment",
    ] {
        let mut json = serde_json::to_value(route).unwrap();
        let v = &mut json["target_player_water"];
        match field {
            "moment" => v["moment"] = "BeforeCountdown".into(),
            "tick" => v["tick"] = 999.into(),
            "water" => v["in_water"] = true.into(),
            "sequence" => v["snapshot"]["sequence"] = 0.into(),
            "digest" => v["digest"][0] = 99.into(),
            "target" => v["target"]["pose"]["position"]["x"] = 7.0_f32.to_bits().into(),
            "source" => v["snapshot"]["owner"]["source"][0] = 99.into(),
            _ => unreachable!(),
        }
        let forged: CreatureRouteRequest = serde_json::from_value(json).unwrap();
        assert!(
            forged
                .require_target_water(w.profile(), w.state().world, w.state().player)
                .is_err(),
            "{field}"
        );
    }
    // A valid old cache at the same target pose cannot be transplanted into a later tick.
    let mut later = route.clone();
    later.request.binding.now_ms += u64::from(w.profile().tick_ms);
    assert!(later
        .require_target_water(w.profile(), w.state().world, w.state().player)
        .is_err());
    assert_eq!(
        water.tick() * u64::from(w.profile().tick_ms) + w.profile().clock_origin_ms,
        route.request.binding.now_ms
    );
    let mut forged = route.clone();
    forged.request.kind = benilla_sim::creature_motion::RouteKind::Backpedal;
    assert!(forged
        .require_target_water(w.profile(), w.state().world, w.state().player)
        .is_err());
    let mut json = serde_json::to_value(route).unwrap();
    json.as_object_mut().unwrap().remove("target_player_water");
    assert!(serde_json::from_value::<CreatureRouteRequest>(json).is_err());
}

#[test]
fn synchronous_player_notify_chase_observes_pre_countdown_cache_and_replays() {
    let mut p = profile();
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.source_motion.as_mut().unwrap().relocation_aggro_delay_ms = 125;
    let mut w = world(p.clone());
    let initial = w.checkpoint().unwrap();
    let mut a = AuthorityFixture::default();
    a.motion.visits = Some(false);
    for _ in 0..8 {
        step(&mut w, vec![], &mut a).unwrap();
    }
    step(&mut w, vec![pose(1.0, 0.0)], &mut a).unwrap();
    for _ in 0..3 {
        step(&mut w, vec![], &mut a).unwrap();
    }
    assert_eq!(
        w.state()
            .player_environment()
            .unwrap()
            .environment()
            .timer_ms(),
        0
    );
    a.motion.visits = Some(true);
    step(&mut w, vec![pose(1.0, 0.0)], &mut a).unwrap();
    let route = a
        .seen
        .iter()
        .find_map(|q| {
            if let Query::SourceMotion(MotionQuery::Route(route)) = &q.query {
                Some(route.as_ref())
            } else {
                None
            }
        })
        .unwrap();
    let water = route
        .require_target_water(w.profile(), w.state().world, w.state().player)
        .unwrap();
    assert_eq!(water.tick(), 13);
    assert_eq!(water.moment(), PlayerEnvironmentMoment::BeforeCountdown);
    let json = serde_json::to_value(water).unwrap();
    assert_eq!(json["snapshot"]["updated_tick"], 12);
    assert_eq!(json["snapshot"]["state"]["timer_ms"], 100);
    let events = &w.last_tick().unwrap().events;
    let position = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::PlayerEnvironment(PlayerEnvironmentReceipt {
                    phase: PlayerEnvironmentPhase::Position {
                        refreshed: true,
                        ..
                    },
                    ..
                })
            )
        })
        .unwrap();
    let motion = events
        .iter()
        .position(|e| matches!(e, Event::NpcMotion { .. }))
        .unwrap();
    let update = events
        .iter()
        .position(|e| {
            matches!(
                e,
                Event::PlayerEnvironment(PlayerEnvironmentReceipt {
                    phase: PlayerEnvironmentPhase::Update { .. },
                    ..
                })
            )
        })
        .unwrap();
    assert!(position < motion && motion < update);
    let Event::PlayerEnvironment(receipt) = &events[position] else {
        unreachable!()
    };
    assert_eq!(receipt.after, water.cache_digest());
    let mut forged = serde_json::to_value(route).unwrap();
    forged["target_player_water"]["moment"] = "AfterCountdown".into();
    let forged: CreatureRouteRequest = serde_json::from_value(forged).unwrap();
    assert!(forged
        .require_target_water(w.profile(), w.state().world, w.state().player)
        .is_err());
    let mut replay = NativeWorld::restore_synthetic_component_test(p, initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
}
