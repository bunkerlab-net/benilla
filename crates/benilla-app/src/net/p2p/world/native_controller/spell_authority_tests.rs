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
                        ))
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
            body(motion.npc_policy.settings.collision_height),
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
