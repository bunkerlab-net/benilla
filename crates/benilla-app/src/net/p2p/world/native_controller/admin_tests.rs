// SPDX-License-Identifier: GPL-3.0-or-later
//! Real AdminBackend -> app controller -> runtime -> exclusive NativeStore, without Bevy.
use super::*;
use benilla_ui::script::admin as ui;
use ui::AdminBackend;

fn data(h: &Harness) -> n::Profile {
    let mut data = quests::profile();
    data.admin = Some(n::AdminPolicy {
        world: [8; 16],
        world_owner: Selection::from_profile(&h.profile, &h.character)
            .unwrap()
            .player
            .owner,
        allowlist: Default::default(),
        revision: 7,
    });
    data
}
fn enter(h: &Harness, open: bool) -> NativeController {
    h.enter(
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        data(h),
        quests::QuestAuthority::default(),
    )
    .unwrap()
}
fn request(
    b: &impl AdminBackend,
    sequence: u64,
    operation: ui::AdminOperation,
) -> ui::AdminRequest {
    let snapshot = b.snapshot();
    ui::AdminRequest {
        id: ui::AdminRequestId {
            context: snapshot.context.unwrap(),
            sequence,
        },
        catalog_revision: snapshot.revision,
        operation,
    }
}
fn committed(b: &impl AdminBackend, r: &ui::AdminRequest) -> ui::AdminResult {
    let result = b.poll(&r.id).expect("durable admin result");
    assert!(
        matches!(&result.outcome, ui::AdminOutcome::Committed { receipt } if !receipt.is_empty())
    );
    result
}
fn inject(h: &Harness) {
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored recoverable fault",
    )
    .unwrap();
}
fn repair(h: &Harness) {
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
}

#[test]
fn admin_adapter_only_publishes_durable_hooks_and_exact_duplicate_receipt() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = c.admin_backend();
    let s = b.snapshot();
    assert_eq!(
        s.level,
        Some(ui::AdminLevel {
            current: 1,
            maximum: 3
        })
    );
    assert_eq!(s.xp_enabled, Some(true));
    assert!(s.status.contains("configured cap 60"));
    assert_eq!(
        s.availability
            .iter()
            .filter(|a| matches!(a, ui::AdminAvailability::Available))
            .count(),
        2
    );
    let r = request(&b, 1, ui::AdminOperation::GainOneLevel);
    b.submit(r.clone()).unwrap();
    b.submit(r.clone()).unwrap();
    assert!(b.poll(&r.id).is_none());
    assert_eq!(c.world.player_level(), 1);
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert!(out.saved.is_some());
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::LevelUp(_)))
            .count(),
        1
    );
    let receipt = committed(&b, &r);
    assert_eq!(b.snapshot().level.unwrap().current, 2);
    let sequence = c.world.state().next_sequence;
    b.submit(r.clone()).unwrap();
    c.pump(Duration::ZERO);
    assert_eq!(b.poll(&r.id), Some(receipt));
    assert_eq!(c.world.state().next_sequence, sequence);
    let mut conflict = r.clone();
    conflict.operation = ui::AdminOperation::SetXpGain { enabled: false };
    assert!(b.submit(conflict).is_err());
    let mut stale = request(&b, 2, ui::AdminOperation::GainOneLevel);
    stale.catalog_revision -= 1;
    assert!(b.submit(stale).is_err());
    let mut wrong = request(&b, 2, ui::AdminOperation::GainOneLevel);
    wrong.id.context.0.push('x');
    assert!(b.submit(wrong).is_err());
    let unsupported = request(&b, 2, ui::AdminOperation::GrantGold { copper: 1 });
    assert!(b.submit(unsupported).is_err());
    assert_eq!(c.world.player_level(), 2);
}
#[test]
fn admin_failed_save_withholds_result_and_snapshot_until_exact_retry() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = c.admin_backend();
    let r = request(&b, 1, ui::AdminOperation::GainOneLevel);
    b.submit(r.clone()).unwrap();
    inject(&h);
    let out = c.pump(c.tick_duration());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(out.events.is_empty());
    assert!(b.poll(&r.id).is_none());
    assert_eq!(c.world.player_level(), 2);
    assert_eq!(b.snapshot().level.unwrap().current, 1);
    let digest = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    b.submit(r.clone()).unwrap();
    let second = request(&b, 2, ui::AdminOperation::GainOneLevel);
    assert!(b.submit(second).is_err());
    repair(&h);
    c.retry_checkpoint().unwrap();
    assert!(b.poll(&r.id).is_none());
    let out = c.pump(Duration::ZERO);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::LevelUp(_)))
            .count(),
        1
    );
    committed(&b, &r);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.world.state().next_sequence, sequence);
    assert_eq!(b.snapshot().level.unwrap().current, 2);
    assert!(c.pump(Duration::ZERO).events.is_empty());
}
#[test]
fn reopened_store_reconciles_retained_ids_without_resubmission() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = c.admin_backend();
    let r = request(&b, 1, ui::AdminOperation::GainOneLevel);
    b.submit(r.clone()).unwrap();
    inject(&h);
    assert!(c.pump(c.tick_duration()).pause.is_some());
    repair(&h);
    c.retry_checkpoint().unwrap(); // Saved, but delivery is deliberately lost.
    assert!(b.poll(&r.id).is_none());
    drop(c);
    let mut c = enter(&h, true);
    c.reconcile_admin_backend(&b).unwrap();
    committed(&b, &r);
    assert_eq!(c.world.player_level(), 2);
    assert_eq!(b.snapshot().level.unwrap().current, 2);
    let new = c.admin_backend();
    assert_ne!(new.snapshot().context, Some(r.id.context.clone()));
    assert!(new.submit(r.clone()).is_err());
    let sequence = c.world.state().next_sequence;
    c.pump(Duration::ZERO);
    assert_eq!(c.world.state().next_sequence, sequence);
    assert!(!c
        .initial_events()
        .unwrap()
        .iter()
        .any(|e| matches!(e, SessionEvent::LevelUp(_))));
}
#[test]
fn lost_unsaved_request_is_explicitly_reconciled_as_uncommitted_on_reopen() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = c.admin_backend();
    let r = request(&b, 1, ui::AdminOperation::GainOneLevel);
    b.submit(r.clone()).unwrap();
    inject(&h);
    assert!(c.pump(c.tick_duration()).pause.is_some());
    assert!(c.reconcile_admin_backend(&b).is_err());
    drop(c);
    repair(&h);
    let mut c = enter(&h, true);
    assert_eq!(c.world.player_level(), 1);
    c.reconcile_admin_backend(&b).unwrap();
    assert!(matches!(
        b.poll(&r.id).unwrap().outcome,
        ui::AdminOutcome::Rejected { .. }
    ));
    let b2 = c.admin_backend();
    let r2 = request(&b2, 1, ui::AdminOperation::GainOneLevel);
    b2.submit(r2.clone()).unwrap();
    assert!(c.pump(c.tick_duration()).pause.is_none());
    committed(&b2, &r2);
    assert_eq!(c.world.player_level(), 2);
}
#[test]
fn xp_off_and_on_round_trip_through_real_ui_backend_and_reopen() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = c.admin_backend();
    let off = request(&b, 1, ui::AdminOperation::SetXpGain { enabled: false });
    b.submit(off.clone()).unwrap();
    assert!(c.pump(c.tick_duration()).pause.is_none());
    committed(&b, &off);
    assert_eq!(b.snapshot().xp_enabled, Some(false));
    let giver = c.projection.npc_guid(c.world.state().npcs[&3].id).unwrap();
    c.receive(ClientCommand::QuestgiverAccept {
        npc: giver,
        quest: 70001,
    })
    .unwrap();
    assert!(c.pump(c.tick_duration()).pause.is_none());
    c.receive(cast(&c)).unwrap();
    let mut events = vec![];
    for _ in 0..25 {
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
        events.extend(out.events);
        if c.world.state().npcs[&2].health == 0 {
            break;
        }
    }
    assert_eq!(c.world.state().npcs[&2].health, 0);
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestObjectiveKill { .. })));
    assert!(!events
        .iter()
        .any(|e| matches!(e, SessionEvent::XpGain(_) | SessionEvent::LevelUp(_))));
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc: giver,
        quest: 70001,
        choice: 0,
    })
    .unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert!(out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestComplete(reward) if reward.xp == 0)));
    assert_eq!(
        c.world.state().quest_progression.as_ref().unwrap().money,
        18
    );
    drop(c);
    let mut c = enter(&h, true);
    let b = c.admin_backend();
    assert_eq!(b.snapshot().xp_enabled, Some(false));
    assert_eq!(c.world.player_level(), 1);
    let on = request(&b, 1, ui::AdminOperation::SetXpGain { enabled: true });
    b.submit(on.clone()).unwrap();
    assert!(c.pump(c.tick_duration()).pause.is_none());
    committed(&b, &on);
    assert_eq!(b.snapshot().xp_enabled, Some(true));
    assert_eq!(
        c.world
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .player
            .view()
            .xp,
        0
    );
}
#[test]
fn full_mailbox_and_missing_permission_do_not_stop_normal_ticks() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = c.admin_backend();
    for sequence in 0..MAX_ACTIONS as u64 {
        b.submit(request(
            &b,
            sequence,
            ui::AdminOperation::SetXpGain { enabled: false },
        ))
        .unwrap();
    }
    assert!(b
        .submit(request(&b, 99, ui::AdminOperation::GainOneLevel))
        .is_err());
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(c.world.player_level(), 1);
    assert_eq!(c.world.state().next_sequence, MAX_ACTIONS as u64);
    assert!(c.pump(c.tick_duration()).pause.is_none());
    drop(c);
    let h = Harness::new();
    let mut p = data(&h);
    p.admin.as_mut().unwrap().world_owner = [99; 16];
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            p,
            quests::QuestAuthority::default(),
        )
        .unwrap();
    let b = c.admin_backend();
    assert!(b.snapshot().context.is_none());
    assert!(c.pump(c.tick_duration()).pause.is_none());
    assert_eq!(c.world.player_level(), 1);
}

#[test]
fn actual_lua_vm_reload_uses_fresh_ids_and_preserves_previous_receipts() {
    use benilla_ui::script::UiScript;
    use std::rc::Rc;
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = Rc::new(c.admin_backend());
    let first = UiScript::new().unwrap();
    first.set_admin_backend(Some(b.clone())).unwrap();
    first.run("BenillaAdminGetView()").unwrap();
    assert!(first
        .eval::<bool>("return BenillaAdminSubmit('level', '', '')")
        .unwrap());
    assert!(c.pump(c.tick_duration()).pause.is_none());
    assert!(first
        .eval::<String>("return BenillaAdminGetView().message")
        .unwrap()
        .contains("Committed."));
    assert_eq!(c.world.player_level(), 2);
    drop(first);
    let b2 = Rc::new(c.admin_backend());
    assert_ne!(b.snapshot().context, b2.snapshot().context);
    let second = UiScript::new().unwrap();
    second.set_admin_backend(Some(b2)).unwrap();
    second.run("BenillaAdminGetView()").unwrap();
    assert!(second
        .eval::<bool>("return BenillaAdminSubmit('level', '', '')")
        .unwrap());
    assert!(c.pump(c.tick_duration()).pause.is_none());
    assert!(second
        .eval::<String>("return BenillaAdminGetView().message")
        .unwrap()
        .contains("Committed."));
    assert_eq!(c.world.player_level(), 3);
    assert_eq!(c.world.state().next_sequence, 2);
}
#[test]
fn lua_reload_during_failed_save_waits_for_old_request_without_duplicate_grant() {
    use benilla_ui::script::UiScript;
    use std::rc::Rc;
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = Rc::new(c.admin_backend());
    let first = UiScript::new().unwrap();
    first.set_admin_backend(Some(b.clone())).unwrap();
    first.run("BenillaAdminGetView()").unwrap();
    assert!(first
        .eval::<bool>("return BenillaAdminSubmit('level', '', '')")
        .unwrap());
    inject(&h);
    assert!(c.pump(c.tick_duration()).pause.is_some());
    let b2 = Rc::new(c.admin_backend());
    let second = UiScript::new().unwrap();
    second.set_admin_backend(Some(b2)).unwrap();
    second.run("BenillaAdminGetView()").unwrap();
    assert!(!second
        .eval::<bool>("return BenillaAdminSubmit('level', '', '')")
        .unwrap());
    assert_eq!(c.world.player_level(), 2);
    repair(&h);
    c.retry_checkpoint().unwrap();
    c.pump(Duration::ZERO);
    assert!(first
        .eval::<String>("return BenillaAdminGetView().message")
        .unwrap()
        .contains("Committed."));
    assert_eq!(c.world.state().next_sequence, 1);
    assert!(second
        .eval::<bool>("return BenillaAdminGetView().level.enabled")
        .unwrap());
}

#[test]
fn recovered_xp_receipt_and_old_ui_snapshot_describe_the_same_durable_world() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = c.admin_backend();
    let off = request(&b, 1, ui::AdminOperation::SetXpGain { enabled: false });
    b.submit(off.clone()).unwrap();
    inject(&h);
    assert!(c.pump(c.tick_duration()).pause.is_some());
    assert_eq!(b.snapshot().xp_enabled, Some(true));
    repair(&h);
    c.retry_checkpoint().unwrap();
    drop(c);
    let c = enter(&h, true);
    c.reconcile_admin_backend(&b).unwrap();
    committed(&b, &off);
    assert_eq!(b.snapshot().xp_enabled, Some(false));
}

#[test]
fn accepted_request_retry_stays_pending_during_nonblocking_mailbox_contention() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = c.admin_backend();
    let r = request(&b, 1, ui::AdminOperation::GainOneLevel);
    let new = request(&b, 2, ui::AdminOperation::GainOneLevel);
    b.submit(r.clone()).unwrap();
    // Controller-side shared ownership is locked; the UI thread must neither wait nor
    // report an accepted request as definitively unadmitted.
    let bridge = c.admin_bridge.as_ref().unwrap().clone();
    bridge.with_locked_for_test(|| {
        assert!(b.submit(r.clone()).is_ok());
        assert!(b.submit(new).is_err());
        assert_eq!(b.snapshot().context, Some(r.id.context.clone()));
        assert!(b.poll(&r.id).is_none());
    });
    assert!(c.pump(c.tick_duration()).pause.is_none());
    committed(&b, &r);
    assert_eq!(c.world.player_level(), 2);
}

#[test]
fn dropping_controller_retires_new_admission_without_inventing_pending_outcomes() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = c.admin_backend();
    let pending = request(&b, 1, ui::AdminOperation::GainOneLevel);
    b.submit(pending.clone()).unwrap();
    drop(c);
    assert!(b.submit(pending.clone()).is_ok());
    assert!(b.poll(&pending.id).is_none());
    let new = request(&b, 2, ui::AdminOperation::GainOneLevel);
    assert!(b.submit(new).is_err());
    let c = enter(&h, true);
    c.reconcile_admin_backend(&b).unwrap();
    assert!(matches!(
        b.poll(&pending.id).unwrap().outcome,
        ui::AdminOutcome::Rejected { .. }
    ));
    assert_eq!(c.world.player_level(), 1);
}

#[test]
fn admin_catalog_snapshot_reuses_rc_until_the_committed_view_changes() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let b = c.admin_backend();
    let before = b.snapshot();
    assert!(std::rc::Rc::ptr_eq(&before.items, &b.snapshot().items));
    let request = request(&b, 1, ui::AdminOperation::GainOneLevel);
    b.submit(request).unwrap();
    let output = c.pump(c.tick_duration());
    assert!(output.pause.is_none(), "{:?}", output.pause);
    let after = b.snapshot();
    assert_ne!(before.revision, after.revision);
    assert!(!std::rc::Rc::ptr_eq(&before.items, &after.items));
    assert!(std::rc::Rc::ptr_eq(&after.items, &b.snapshot().items));
}
