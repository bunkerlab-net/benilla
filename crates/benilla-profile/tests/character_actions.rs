mod char_select {
    pub(crate) use crate::recovery_model;
}
#[path = "../../benilla-app/src/char_select/recovery_model.rs"]
pub(crate) mod recovery_model;
// Execute the production app character actions without linking Bevy.
#[path = "../../benilla-app/src/net/p2p/character_actions.rs"]
mod character_actions;
#[path = "../../benilla-app/src/char_select/delete_intent.rs"]
mod delete_intent;
#[path = "../../benilla-app/src/net/p2p/recovery_actions.rs"]
mod recovery_actions;

use benilla_profile::{Appearance, NewCharacter, ProfileStore};
use benilla_protocol::{CharAction, SessionEvent};
use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        // macOS temp paths can traverse /var's symlink; storage rejects symlink ancestors.
        let base = std::env::temp_dir().canonicalize().unwrap();
        Self(base.join(format!(
            "benilla-delete-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn input(name: &str) -> NewCharacter {
    NewCharacter {
        name: name.into(),
        race: 1,
        class: 1,
        gender: 0,
        appearance: Appearance::default(),
    }
}
fn action_code(event: SessionEvent) -> u8 {
    match event {
        SessionEvent::CharActionResult {
            action: CharAction::Delete,
            code,
        } => code,
        _ => panic!("unexpected event"),
    }
}

#[test]
fn explicit_confirmation_consumes_exact_snapshot_and_cancel_wins() {
    use delete_intent::{armed, decide, Decision};
    assert!(!armed("", "DELETE"));
    assert!(!armed("", ""));
    assert!(!armed("DELETE ", "DELETE"));
    assert!(!armed("DEL", "DELETE"));
    assert!(armed("delete", "DELETE"));
    let mut target = Some((41, "First"));
    assert!(matches!(
        decide(&mut target, false, true, false),
        Decision::Pending
    ));
    assert_eq!(target, Some((41, "First")));
    // A later UI selection is irrelevant to the snapshotted GUID and name.
    let _new_selected_row = (92, "Second");
    assert!(matches!(
        decide(&mut target, true, true, false),
        Decision::Confirmed((41, "First"))
    ));
    assert!(matches!(
        decide(&mut target, true, true, false),
        Decision::Pending
    ));
    for submit in [false, true] {
        let mut target = Some(41);
        assert!(matches!(
            decide(&mut target, true, submit, true),
            Decision::Cancelled
        ));
        assert!(target.is_none());
        assert!(matches!(
            decide(&mut target, true, true, false),
            Decision::Pending
        ));
    }
}

#[test]
fn production_delete_maps_owner_guid_and_commits_before_roster_and_success() {
    let fixture = Fixture::new();
    let mut store = ProfileStore::create(&fixture.0).unwrap();
    let first = store.create_character(input("First")).unwrap();
    let second = store.create_character(input("Second")).unwrap();
    let guid = character_actions::character_guid(store.character(first).unwrap());
    let other = store.character(second).unwrap().clone();
    let key = store.public_key();
    let original = store.character(first).unwrap().clone();
    let order = RefCell::new(Vec::new());
    character_actions::delete(
        &mut store,
        guid,
        |event| {
            assert_eq!(action_code(event), 0x39);
            order.borrow_mut().push("success");
            Ok(())
        },
        |saved| {
            assert_eq!(saved.characters(), std::slice::from_ref(&other));
            assert_eq!(saved.selected_character(), Some(&other));
            let disk: serde_json::Value =
                serde_json::from_slice(&fs::read(fixture.0.join("profile.json")).unwrap()).unwrap();
            assert_eq!(
                disk["payload"]["state"]["characters"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                disk["payload"]["state"]["archived"][0]["id"],
                serde_json::to_value(first).unwrap()
            );
            order.borrow_mut().push("roster");
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(*order.borrow(), ["roster", "success"]);
    let bytes = fs::read(fixture.0.join("profile.json")).unwrap();
    for stale in [guid, 0, u64::MAX] {
        character_actions::delete(
            &mut store,
            stale,
            |event| {
                assert_eq!(action_code(event), 0x3a);
                Ok(())
            },
            |_| panic!("stale target must not emit a changed roster"),
        )
        .unwrap();
        assert_eq!(fs::read(fixture.0.join("profile.json")).unwrap(), bytes);
    }
    drop(store);
    let mut store = ProfileStore::open(&fixture.0).unwrap();
    assert_eq!(store.public_key(), key);
    assert_eq!(store.characters(), &[other]);
    assert_eq!(store.archived_characters(), std::slice::from_ref(&original));
    store.restore_character(store.owner_id(), first).unwrap();
    assert_eq!(store.character(first), Some(&original));
    assert_eq!(
        character_actions::character_guid(store.character(first).unwrap()),
        guid
    );
}

#[test]
fn a_copied_id_owned_by_someone_else_does_not_delete_the_local_character() {
    let fixture = Fixture::new();
    let mut store = ProfileStore::create(&fixture.0).unwrap();
    let id = store.create_character(input("Alice")).unwrap();
    let mut other_owner = store.character(id).unwrap().clone();
    other_owner.owner_id[0] ^= 1;
    let foreign_guid = character_actions::character_guid(&other_owner);
    assert_ne!(
        foreign_guid,
        character_actions::character_guid(store.character(id).unwrap())
    );
    character_actions::delete(
        &mut store,
        foreign_guid,
        |event| {
            assert_eq!(action_code(event), 0x3a);
            Ok(())
        },
        |_| panic!("foreign owner has no matching local target"),
    )
    .unwrap();
    assert!(store.character(id).is_some());
}

#[test]
fn failed_save_emits_failure_never_success_or_changed_roster() {
    let fixture = Fixture::new();
    let mut store = ProfileStore::create(&fixture.0).unwrap();
    let id = store.create_character(input("Alice")).unwrap();
    let guid = character_actions::character_guid(store.character(id).unwrap());
    let before = fs::read(fixture.0.join("profile.json")).unwrap();
    fs::remove_file(fixture.0.join("profile.previous.json")).unwrap();
    fs::create_dir(fixture.0.join("profile.previous.json")).unwrap();
    let codes = RefCell::new(Vec::new());
    assert!(character_actions::delete(
        &mut store,
        guid,
        |event| {
            codes.borrow_mut().push(action_code(event));
            Ok(())
        },
        |_| panic!("failed write must not change roster")
    )
    .is_err());
    assert_eq!(*codes.borrow(), [0x3a]);
    assert!(store.character(id).is_some());
    assert!(store.archived_characters().is_empty());
    assert_eq!(fs::read(fixture.0.join("profile.json")).unwrap(), before);
}

#[test]
fn a_lost_roster_delivery_keeps_durable_removal_and_does_not_emit_success() {
    let fixture = Fixture::new();
    let mut store = ProfileStore::create(&fixture.0).unwrap();
    let id = store.create_character(input("Alice")).unwrap();
    let guid = character_actions::character_guid(store.character(id).unwrap());
    assert!(character_actions::delete(
        &mut store,
        guid,
        |_| panic!("no success after failed roster delivery"),
        |_| Err(anyhow::anyhow!("synthetic disconnected event queue"))
    )
    .is_err());
    drop(store);
    let store = ProfileStore::open(&fixture.0).unwrap();
    assert!(store.characters().is_empty());
    assert_eq!(store.archived_characters()[0].id, id);
}

#[test]
fn recovery_lists_confirms_and_restores_once_without_changing_identity() {
    let fixture = Fixture::new();
    let mut store = ProfileStore::create(&fixture.0).unwrap();
    let id = store.create_character(input("Alice")).unwrap();
    let original = store.character(id).unwrap().clone();
    store.archive_character(store.owner_id(), id).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    recovery_actions::recover(&mut store, recovery_model::Request::List(tx), |_| {
        panic!("list is read only")
    })
    .unwrap();
    let rows = rx.recv().unwrap().unwrap();
    assert_eq!(rows, std::slice::from_ref(&original));
    let mut confirmation = recovery_model::Confirmation(Some(original.clone()));
    let target = confirmation.confirm(&rows).unwrap();
    assert!(confirmation.confirm(&rows).is_none());
    let (reply, rx) = std::sync::mpsc::channel();
    recovery_actions::recover(
        &mut store,
        recovery_model::Request::Restore { target, reply },
        |saved| {
            assert_eq!(saved.character(id), Some(&original));
            assert!(rx.try_recv().is_err(), "no success before roster delivery");
            Ok(())
        },
    )
    .unwrap();
    assert!(rx.recv().unwrap().unwrap().is_empty());
    let (reply, rx) = std::sync::mpsc::channel();
    recovery_actions::recover(
        &mut store,
        recovery_model::Request::Restore {
            target: original.clone(),
            reply,
        },
        |_| panic!("duplicate restore"),
    )
    .unwrap();
    assert!(rx.recv().unwrap().is_err());
    drop(store);
    let store = ProfileStore::open(&fixture.0).unwrap();
    assert_eq!(store.character(id), Some(&original));
}

#[test]
fn recovery_rejects_foreign_stale_and_name_conflicting_targets() {
    let fixture = Fixture::new();
    let mut store = ProfileStore::create(&fixture.0).unwrap();
    let id = store.create_character(input("Alice")).unwrap();
    let original = store.character(id).unwrap().clone();
    store.archive_character(store.owner_id(), id).unwrap();
    let mut foreign = original.clone();
    foreign.owner_id[0] ^= 1;
    let mut stale = original.clone();
    stale.id[0] ^= 1;
    for target in [foreign, stale] {
        let (reply, rx) = std::sync::mpsc::channel();
        recovery_actions::recover(
            &mut store,
            recovery_model::Request::Restore { target, reply },
            |_| panic!("invalid target"),
        )
        .unwrap();
        assert!(rx.recv().unwrap().is_err());
    }
    store.create_character(input("Alice")).unwrap();
    let before = fs::read(fixture.0.join("profile.json")).unwrap();
    let (reply, rx) = std::sync::mpsc::channel();
    recovery_actions::recover(
        &mut store,
        recovery_model::Request::Restore {
            target: original.clone(),
            reply,
        },
        |_| panic!("name conflict"),
    )
    .unwrap();
    assert!(rx.recv().unwrap().unwrap_err().contains("name"));
    assert_eq!(fs::read(fixture.0.join("profile.json")).unwrap(), before);
    let mut confirmation = recovery_model::Confirmation(Some(original));
    assert!(confirmation.confirm(&[]).is_none());
    assert!(confirmation.0.is_none());
}

#[test]
fn realm_recovery_requests_are_explicitly_rejected() {
    let (tx, rx) = std::sync::mpsc::channel();
    recovery_model::Request::List(tx).reject();
    assert!(rx.recv().unwrap().unwrap_err().contains("only for local"));
}

#[test]
fn recovery_save_failure_closes_lobby_without_false_success() {
    let fixture = Fixture::new();
    let mut store = ProfileStore::create(&fixture.0).unwrap();
    let id = store.create_character(input("Alice")).unwrap();
    let target = store.character(id).unwrap().clone();
    store.archive_character(store.owner_id(), id).unwrap();
    fs::remove_file(fixture.0.join("profile.previous.json")).unwrap();
    fs::create_dir(fixture.0.join("profile.previous.json")).unwrap();
    let (reply, rx) = std::sync::mpsc::channel();
    assert!(recovery_actions::recover(
        &mut store,
        recovery_model::Request::Restore { target, reply },
        |_| panic!("failed save cannot refresh roster")
    )
    .is_err());
    assert!(rx.recv().unwrap().is_err());
    assert!(store.character(id).is_none());
    assert_eq!(store.archived_characters().len(), 1);
}

#[test]
fn recovery_lost_roster_delivery_does_not_acknowledge_success() {
    let fixture = Fixture::new();
    let mut store = ProfileStore::create(&fixture.0).unwrap();
    let id = store.create_character(input("Alice")).unwrap();
    let target = store.character(id).unwrap().clone();
    store.archive_character(store.owner_id(), id).unwrap();
    let (reply, rx) = std::sync::mpsc::channel();
    assert!(recovery_actions::recover(
        &mut store,
        recovery_model::Request::Restore { target, reply },
        |_| Err(anyhow::anyhow!("disconnected event queue"))
    )
    .is_err());
    assert!(rx.recv().is_err());
    drop(store);
    assert!(ProfileStore::open(&fixture.0)
        .unwrap()
        .character(id)
        .is_some());
}

#[test]
fn recovery_full_roster_refuses_without_replacing_characters() {
    let fixture = Fixture::new();
    let mut store = ProfileStore::create(&fixture.0).unwrap();
    let id = store.create_character(input("Archived")).unwrap();
    let target = store.character(id).unwrap().clone();
    store.archive_character(store.owner_id(), id).unwrap();
    for name in [
        "Alice", "Beryl", "Cathy", "Derek", "Edith", "Frank", "Grace", "Henry", "Irene", "James",
    ] {
        store.create_character(input(name)).unwrap();
    }
    assert_eq!(store.characters().len(), benilla_profile::MAX_CHARACTERS);
    let (reply, rx) = std::sync::mpsc::channel();
    recovery_actions::recover(
        &mut store,
        recovery_model::Request::Restore { target, reply },
        |_| panic!("full roster"),
    )
    .unwrap();
    assert!(rx.recv().unwrap().unwrap_err().contains("full"));
    assert_eq!(store.characters().len(), benilla_profile::MAX_CHARACTERS);
    assert!(store.character(id).is_none());
    assert_eq!(store.archived_characters().len(), 1);
}
