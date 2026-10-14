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
