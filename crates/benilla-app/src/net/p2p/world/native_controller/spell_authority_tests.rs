// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact controller/client/store path; source-shaped rows are synthetic only.
use super::*;
use n::synthetic_spell_fixture as source;
struct Faultable {
    fail: Arc<AtomicBool>,
    inner: source::SpatialOnly,
}
impl n::Authority for Faultable {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.inner.resolve(request)
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.inner.fail_phase = self
            .fail
            .load(Ordering::SeqCst)
            .then_some(n::SourceSpellPhase::ReleaseHit);
        self.inner.observe_derived(reply)
    }
}
#[test]
fn source_spell_release_failure_retains_input_then_checkpoints_and_publishes_once() {
    let h = Harness::new();
    let data = source::profile();
    let fail = Arc::new(AtomicBool::new(false));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            data.clone(),
            Faultable {
                fail: fail.clone(),
                inner: source::spatial_only(),
            },
        )
        .unwrap();
    c.initial_events().unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 133,
        target: Some(c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap()),
    })
    .unwrap();
    pump_to(&mut c, 2);
    c.receive(move_to([0.0, 0.0, 0.0])).unwrap();
    let before = c.world.checkpoint().unwrap();
    let mut expected = c.world.clone();
    fail.store(true, Ordering::SeqCst);
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 0);
    assert!(out.events.is_empty());
    assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
    assert_eq!(c.world.checkpoint().unwrap(), before);
    let inputs = c.pending.as_ref().unwrap().inputs.clone();
    expected
        .step_batch(3, inputs, &mut source::spatial_only())
        .unwrap();
    fail.store(false, Ordering::SeqCst);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored spell save fault",
    )
    .unwrap();
    c.retry_tick().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(
        matches!(
            out.pause.as_ref().unwrap().kind,
            PauseKind::Checkpoint {
                requires_reopen: false
            }
        ),
        "{:?}",
        out.pause
    );
    assert_eq!(c.world.state_digest(), expected.state_digest());
    assert_eq!(c.world.state().next_sequence, 2);
    let digest = c.world.state_digest();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert!(!out.events.is_empty());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state_digest(), digest);
    c.shutdown().unwrap();
    drop(c);
    let mut reopened = h
        .enter(Entry::Open { world: [8; 16] }, data, source::spatial_only())
        .unwrap();
    assert_eq!(reopened.world.state_digest(), digest);
    reopened.initial_events().unwrap();
    pump_to(&mut reopened, 4);
    expected.step(4, None, &mut source::spatial_only()).unwrap();
    assert_eq!(reopened.world.state_digest(), expected.state_digest());
    reopened.shutdown().unwrap();
}

#[test]
fn real_static_los_missing_and_changed_geometry_pause_without_publication_then_resume() {
    use crate::net::p2p::native_geometry::{
        authored_vmap, authored_vmap_revision as geometry_revision,
        core::{Body, NonRenderParticipants, PlayerMotion, Session, StaticLosPolicy},
        SourceVmapProvider,
    };
    struct Spatial {
        provider: SourceVmapProvider,
        session: Session,
        mode: Arc<AtomicU64>,
        other: source::SpatialOnly,
    }
    impl n::Authority for Spatial {
        fn resolve(
            &mut self,
            request: &n::AuthorityRequest,
        ) -> Result<n::AuthorityReply, n::NativeError> {
            if matches!(
                request.query,
                n::Query::SourceSpell(n::SourceSpellQuery::LineOfSight(_))
            ) {
                let mut session = self.session.clone();
                match self.mode.load(Ordering::SeqCst) {
                    1 => {
                        return Err(n::NativeError::AuthorityUnavailable(
                            "authored missing snapshot",
                        ));
                    }
                    2 => session.geometry = [99; 32],
                    _ => {}
                }
                let facts = self
                    .provider
                    .line_of_sight(
                        &session,
                        request,
                        std::time::Instant::now() + Duration::from_secs(5),
                    )
                    .map_err(|_| {
                        n::NativeError::AuthorityUnavailable("bound static LOS unavailable")
                    })?;
                return Ok(n::AuthorityReply {
                    request: request.clone(),
                    facts,
                });
            }
            // All non-LOS mechanics remain explicitly synthetic in this component test.
            self.other.resolve(request)
        }
        fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
            self.other.observe_derived(reply)
        }
    }
    let h = Harness::new();
    let mut data = source::profile();
    let snapshot = authored_vmap(data.map_id);
    data.content_digest = snapshot.binding().content;
    for npc in &mut data.npcs {
        npc.ai.content_revision = benilla_sim::creature_ai::movement::Revision(data.content_digest);
    }
    let raw = [41; 32];
    data.geometry_revision = geometry_revision(raw, snapshot.binding());
    let body = |height| Body {
        radius: benilla_sim::creature_ai::Real::new(0.5).unwrap(),
        height,
    };
    let motion = data.source_motion.as_ref().unwrap();
    let session = Session {
        world: [8; 16],
        profile: data.digest(),
        policy: data.authority_policy,
        content: data.content_digest,
        geometry: data.geometry_revision,
        map: data.map_id,
        bodies: std::collections::BTreeMap::from([(
            data.npcs[0].ai.creature_id,
            body(
                motion
                    .npc(data.npcs[0].ai.creature_id)
                    .unwrap()
                    .npc_policy
                    .settings
                    .collision_height,
            ),
        )]),
        player: Some(PlayerMotion {
            body: body(motion.player_collision_height),
            speed: benilla_sim::creature_ai::Real::new(7.).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            data.content_digest,
            data.map_id,
        )),
        static_los: Some(StaticLosPolicy::from_profile(&data, true).unwrap()),
        primary_near_point: None,
        empty_selector_near_point: None,
        source_location: None,
        player_environment: None,
        same_polygon_route: None,
    };
    assert!(StaticLosPolicy::from_profile(&data, false).is_err());
    let mode = Arc::new(AtomicU64::new(1));
    let authority = Spatial {
        provider: SourceVmapProvider::bind(&session, snapshot, raw).unwrap(),
        session,
        mode: mode.clone(),
        other: source::spatial_only(),
    };
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            data.clone(),
            authority,
        )
        .unwrap();
    c.initial_events().unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 133,
        target: Some(c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap()),
    })
    .unwrap();
    let before = c.world.checkpoint().unwrap();
    for fault in [1, 2] {
        mode.store(fault, Ordering::SeqCst);
        if fault == 2 {
            c.retry_tick().unwrap();
        }
        let out = c.pump(if fault == 1 {
            c.tick_duration()
        } else {
            Duration::ZERO
        });
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(c.world.checkpoint().unwrap(), before);
    }
    mode.store(0, Ordering::SeqCst);
    c.retry_tick().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 1);
    assert!(out.pause.is_none());
    assert!(out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellStart { spell_id: 133, .. })));
    assert_eq!(c.world.state().next_sequence, 1);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    let digest = c.world.state_digest();
    c.shutdown().unwrap();
    drop(c);
    let reopened = h
        .enter(Entry::Open { world: [8; 16] }, data, source::spatial_only())
        .unwrap();
    assert_eq!(reopened.world.state_digest(), digest);
}

fn captured_cast(c: &mut NativeController) {
    c.receive(ClientCommand::CastSpell {
        spell_id: 133,
        target: Some(c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap()),
    })
    .unwrap();
}

fn captured_phase(c: &NativeController, phase: usize) -> bool {
    use benilla_sim::classic_spell::CastState;
    let state = c.world.state();
    let aura = state.npcs[&2]
        .auras
        .values()
        .find_map(|target| target.aura());
    match phase {
        0 => state
            .casts
            .values()
            .any(|cast| matches!(cast.execution.state(), CastState::Casting { .. })),
        1 => state
            .casts
            .values()
            .any(|cast| matches!(cast.execution.state(), CastState::Traveling { .. })),
        2 => aura.is_some_and(|a| a.ticks == 0),
        3 => aura.is_some_and(|a| a.ticks == 1),
        4 => state.tick > 60 && aura.is_none() && state.casts.is_empty(),
        _ => unreachable!(),
    }
}

#[test]
fn captured_fireball_controller_reopens_every_phase_without_republishing_events() {
    for phase in 0..5 {
        let h = Harness::new();
        let control_home = Harness::new();
        let data = n::synthetic_captured_spell_fixture::captured_fireball_profile();
        let mut c = h
            .enter(
                Entry::Create {
                    world: [8; 16],
                    seed: 7,
                },
                data.clone(),
                source::spatial_only(),
            )
            .unwrap();
        let mut control = NativeController::enter_component_test(
            &control_home.root,
            &h.profile,
            &h.character,
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            data.clone(),
            &Catalog,
            source::spatial_only(),
        )
        .unwrap();
        let initial = c.initial_events().unwrap();
        assert!(initial.iter().any(
            |e| matches!(e, SessionEvent::SpellBook { spell_ids, .. } if spell_ids.contains(&133))
        ));
        control.initial_events().unwrap();
        captured_cast(&mut c);
        captured_cast(&mut control);
        for _ in 0..100 {
            let tick = c.tick() + 1;
            let actual = pump_to(&mut c, tick);
            let expected = pump_to(&mut control, tick);
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
            assert_eq!(
                c.world.checkpoint().unwrap(),
                control.world.checkpoint().unwrap()
            );
            if captured_phase(&c, phase) {
                break;
            }
        }
        assert!(captured_phase(&c, phase), "phase {phase}");
        let checkpoint = c.world.checkpoint().unwrap();
        c.shutdown().unwrap();
        drop(c);
        let mut resumed = h
            .enter(Entry::Open { world: [8; 16] }, data, source::spatial_only())
            .unwrap();
        assert_eq!(resumed.world.checkpoint().unwrap(), checkpoint);
        let projection = resumed.initial_events().unwrap();
        assert_eq!(
            projection
                .iter()
                .filter(|e| matches!(e, SessionEvent::SpellStart { .. }))
                .count(),
            usize::from(phase == 0)
        );
        if phase == 0 {
            let benilla_sim::classic_spell::CastState::Casting { remaining_ms } =
                resumed.world.state().casts[&1].execution.state()
            else {
                unreachable!()
            };
            assert!(projection.iter().any(|e| matches!(e,
                SessionEvent::SpellStart { spell_id: 133, cast_time_ms, target: Some(_), .. }
                    if *cast_time_ms == remaining_ms)));
        }
        assert!(!projection.iter().any(|e| matches!(
            e,
            SessionEvent::SpellGo { .. }
                | SessionEvent::SpellDamageLog(_)
                | SessionEvent::PeriodicAuraLog(_)
        )));
        for _ in 0..80 {
            let tick = resumed.tick() + 1;
            let mut reference = control.world.clone();
            reference
                .step(tick, None, &mut source::spatial_only())
                .unwrap();
            let actual = pump_to(&mut resumed, tick);
            let expected = pump_to(&mut control, tick);
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
            let checkpoint = resumed.world.checkpoint().unwrap();
            assert_eq!(checkpoint, control.world.checkpoint().unwrap());
            assert_eq!(checkpoint, reference.checkpoint().unwrap());
            let record = reference.last_tick().unwrap();
            assert_eq!(checkpoint.identity().history_tip, record.digest());
            // A durable acknowledgement clears the bounded journal, not the committed history.
            for controller in [&resumed, &control] {
                if let Some(retained) = controller.world.last_tick() {
                    assert_eq!(retained, record);
                } else {
                    assert_eq!(controller.saved_tick, tick);
                }
            }
        }
        resumed.shutdown().unwrap();
        control.shutdown().unwrap();
    }
}

#[test]
fn captured_fireball_release_retry_commits_once_and_changed_bindings_cannot_open() {
    let h = Harness::new();
    let data = n::synthetic_captured_spell_fixture::captured_fireball_profile();
    let fail = Arc::new(AtomicBool::new(false));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            data.clone(),
            Faultable {
                fail: fail.clone(),
                inner: source::spatial_only(),
            },
        )
        .unwrap();
    c.initial_events().unwrap();
    captured_cast(&mut c);
    pump_to(&mut c, 1);
    let benilla_sim::classic_spell::CastState::Casting { remaining_ms } =
        c.world.state().casts[&1].execution.state()
    else {
        panic!("not casting")
    };
    let release_tick = c.tick() + u64::from(remaining_ms).div_ceil(u64::from(data.tick_ms));
    pump_to(&mut c, release_tick - 1);
    // An unchanged pose forces save-before-publication without interrupting this cast.
    c.receive(move_to([0.0, 0.0, 0.0])).unwrap();
    let before = c.world.checkpoint().unwrap();
    let mut expected = c.world.clone();
    fail.store(true, Ordering::SeqCst);
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 0);
    assert!(out.events.is_empty());
    assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
    assert_eq!(c.world.checkpoint().unwrap(), before);
    assert!(c.pending.as_ref().unwrap().moving);
    expected
        .step_batch(
            release_tick,
            c.pending.as_ref().unwrap().inputs.clone(),
            &mut source::spatial_only(),
        )
        .unwrap();
    fail.store(false, Ordering::SeqCst);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored captured spell fault",
    )
    .unwrap();
    c.retry_tick().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 1);
    assert!(c.outbox.as_ref().is_some_and(|batch| batch.saved.is_none()
        && batch
            .events
            .iter()
            .any(|e| matches!(e, SessionEvent::SpellGo { .. }))));
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert_eq!(
        c.world.checkpoint().unwrap(),
        expected.checkpoint().unwrap()
    );
    let committed = c.world.checkpoint().unwrap();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    let published = c.pump(Duration::ZERO);
    assert_eq!(published.ticks, 0);
    assert_eq!(
        published
            .events
            .iter()
            .filter(|e| matches!(e, SessionEvent::SpellGo { .. }))
            .count(),
        1
    );
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.checkpoint().unwrap(), committed);
    c.shutdown().unwrap();
    drop(c);
    for field in 0..3 {
        let (mut changed, initial, mut input) =
            n::synthetic_captured_spell_fixture::captured_fireball_inputs();
        if field == 0 {
            input.spells[0].cast_time.minimum_ms -= 1;
        }
        if field == 1 {
            input.spells[0].bonus_coefficients[0] = 0;
        }
        changed.initialize_spell_authority(&initial, input).unwrap();
        source::initialize_authored_location(&mut changed);
        if field == 2 {
            changed
                .source_motion
                .as_mut()
                .unwrap()
                .spell_hits
                .get_mut(&133)
                .unwrap()
                .attributes[0] = 0;
        }
        assert!(h
            .enter(
                Entry::Open { world: [8; 16] },
                changed,
                source::spatial_only()
            )
            .is_err());
    }
    let mut resumed = h
        .enter(Entry::Open { world: [8; 16] }, data, source::spatial_only())
        .unwrap();
    assert_eq!(resumed.world.checkpoint().unwrap(), committed);
    resumed.initial_events().unwrap();
    let next = resumed.tick() + 1;
    pump_to(&mut resumed, next);
    expected
        .step(next, None, &mut source::spatial_only())
        .unwrap();
    assert_eq!(
        resumed.world.checkpoint().unwrap(),
        expected.checkpoint().unwrap()
    );
    resumed.shutdown().unwrap();
}
