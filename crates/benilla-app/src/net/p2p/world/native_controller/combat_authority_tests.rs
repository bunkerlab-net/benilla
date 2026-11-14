// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact controller/client/store path; source-shaped rows are synthetic only.
use super::*;
use n::synthetic_combat_fixture as combat;
struct Faultable {
    fail: Arc<AtomicBool>,
    inner: combat::SpatialOnly,
}
impl n::Authority for Faultable {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.inner.resolve(request)
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.inner.fail_post_damage = self.fail.load(Ordering::SeqCst);
        self.inner.observe_derived(reply)
    }
}
#[test]
fn source_melee_late_failure_retains_input_then_checkpoints_and_publishes_once() {
    let h = Harness::new();
    let data = combat::profile();
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
                inner: Default::default(),
            },
        )
        .unwrap();
    c.initial_events().unwrap();
    pump_to(&mut c, 1);
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
        .step_batch(2, inputs, &mut combat::SpatialOnly::default())
        .unwrap();
    fail.store(false, Ordering::SeqCst);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored combat save fault",
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
    assert_eq!(c.world.state_digest(), expected.state_digest());
    assert_eq!(c.world.state().next_sequence, 1);
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
        .enter(
            Entry::Open { world: [8; 16] },
            data,
            combat::SpatialOnly::default(),
        )
        .unwrap();
    assert_eq!(reopened.world.state_digest(), digest);
    reopened.initial_events().unwrap();
    pump_to(&mut reopened, 3);
    expected
        .step(3, None, &mut combat::SpatialOnly::default())
        .unwrap();
    assert_eq!(reopened.world.state_digest(), expected.state_digest());
    reopened.shutdown().unwrap();
}
