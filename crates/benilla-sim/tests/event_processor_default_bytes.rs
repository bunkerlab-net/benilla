use benilla_sim::event_processor::{EventKind, EventProcessor};

#[test]
fn default_event_snapshot_keeps_pre_f5_06b_bytes() {
    let mut queue: EventProcessor = EventProcessor::new();
    for (id, kind) in [
        (1, EventKind::AiNotify),
        (2, EventKind::ReenableHelp { occurrence: 7 }),
        (3, EventKind::AggroLinking { occurrence: 8 }),
        (4, EventKind::SpellUpdate { cast_id: 9 }),
    ] {
        queue.add_event(id, kind, id * 10).unwrap();
    }
    queue.set_aborted(2, true).unwrap();
    queue.begin_update(5).unwrap().commit().unwrap();
    // Captured against the unchanged default-kind owner before genericization.
    assert_eq!(
        serde_json::to_string(&queue.snapshot()).unwrap(),
        r#"{"version":1,"clock_ms":5,"next_insertion_id":5,"next_event_id":5,"aborting":false,"queued":[{"event_id":1,"kind":"AiNotify","add_time_ms":0,"exec_time_ms":10,"to_abort":false,"queue":{"deadline_ms":10,"insertion_id":1}},{"event_id":2,"kind":{"ReenableHelp":{"occurrence":7}},"add_time_ms":0,"exec_time_ms":20,"to_abort":true,"queue":{"deadline_ms":20,"insertion_id":2}},{"event_id":3,"kind":{"AggroLinking":{"occurrence":8}},"add_time_ms":0,"exec_time_ms":30,"to_abort":false,"queue":{"deadline_ms":30,"insertion_id":3}},{"event_id":4,"kind":{"SpellUpdate":{"cast_id":9}},"add_time_ms":0,"exec_time_ms":40,"to_abort":false,"queue":{"deadline_ms":40,"insertion_id":4}}],"detached":[]}"#
    );
}

#[test]
fn default_kind_still_rejects_source_only_despawn_input() {
    assert!(serde_json::from_str::<EventKind>(r#"{"ForcedDespawn":{"only_alive":true}}"#).is_err());
}

#[test]
fn data_only_private_payload_uses_existing_abort_and_deletability_rules() {
    use benilla_sim::event_processor::{CallbackKind, EventError, EventPayload};
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct PrivateKind(u32);
    impl EventPayload for PrivateKind {
        fn validate(self) -> Result<(), EventError> {
            Ok(())
        }
    }
    let mut queue = EventProcessor::<PrivateKind>::with_payload();
    queue.add_event(1, PrivateKind(7), 1).unwrap();
    let mut cleanup = queue.begin_kill_all_events(false);
    let due = cleanup.pop_due().unwrap().unwrap();
    assert_eq!(due.kind, PrivateKind(7));
    assert_eq!(due.callback, CallbackKind::Abort);
    cleanup.finish_abort(false).unwrap();
    cleanup.commit().unwrap();
    assert!(queue.event(1).unwrap().to_abort);
    let mut update = queue.begin_update(1).unwrap();
    assert_eq!(
        update.pop_due().unwrap().unwrap().callback,
        CallbackKind::Abort
    );
    update.finish_abort(false).unwrap();
    update.commit().unwrap();
    assert!(queue.event(1).is_none());
}
