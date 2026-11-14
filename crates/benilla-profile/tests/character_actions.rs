mod char_select {
    pub(crate) use crate::recovery_model;
}
#[path = "../../benilla-app/src/char_select/recovery_model.rs"]
pub(crate) mod recovery_model;
// Execute the production app character actions without linking Bevy.
#[path = "../../benilla-app/src/net/p2p/character_actions.rs"]
mod character_actions;
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

fn glue_record(guid: u64, name: &str) -> benilla_ui::script::GlueCharacter {
    benilla_ui::script::GlueCharacter {
        guid,
        name: name.into(),
        race: "Human".into(),
        class: "Warrior".into(),
        level: 1,
        zone: None,
        file_string: "Human".into(),
        gender: 0,
        flags: 0,
        model: false,
    }
}

#[test]
fn stock_lua_confirmation_preserves_snapshot_and_durable_delete() {
    use benilla_ui::script::{GlueCall, UiScript};
    let fixture = Fixture::new();
    let mut store = ProfileStore::create(&fixture.0).unwrap();
    let first = store.create_character(input("First")).unwrap();
    let second = store.create_character(input("Second")).unwrap();
    let guid = character_actions::character_guid(store.character(first).unwrap());
    let other_guid = character_actions::character_guid(store.character(second).unwrap());
    let rows = vec![
        glue_record(guid, "First"),
        glue_record(other_guid, "Second"),
    ];
    let mut missing_dialog = UiScript::new_glue().unwrap();
    missing_dialog.load_glue_roster(rows.clone());
    missing_dialog.take_glue_calls();
    assert!(missing_dialog
        .install_local_character_delete_guard()
        .is_err());
    missing_dialog.run("DeleteCharacter(1)").unwrap();
    assert!(
        missing_dialog.take_glue_calls().is_empty(),
        "installation failure must disable raw local deletion"
    );
    let mut s = UiScript::new_glue().unwrap();
    s.register_cvars([("lastCharacterIndex", "0")]);
    s.load_glue_roster(rows.clone());
    s.run(
        r#"
        typed = '' DELETE_CONFIRM_STRING = 'DELETE'
        CharacterDeleteDialog = CreateFrame('Frame', 'CharacterDeleteDialog')
        CharacterDeleteDialog:Hide()
        CharacterDeleteEditBox = { GetText = function() return typed end }
        shown = 0 hidden = 0
        CharacterDeleteDialog:SetScript('OnShow', function() shown = shown + 1 end)
        CharacterDeleteDialog:SetScript('OnHide', function() hidden = hidden + 1 end)
    "#,
    )
    .unwrap();
    // A saved raw binding must not bypass a later local-backend guard.
    s.run("OldDelete = DeleteCharacter").unwrap();
    s.install_local_character_delete_guard().unwrap();
    s.take_glue_calls();
    s.run("OldDelete(1) DeleteCharacter(1)").unwrap();
    assert!(s.take_glue_calls().is_empty());
    for typed in ["", "DEL", "DELETE "] {
        s.run("CharacterDeleteDialog:Show()").unwrap();
        s.lua().globals().set("typed", typed).unwrap();
        s.run("DeleteCharacter(1)").unwrap();
        assert!(s.take_glue_calls().is_empty());
        s.run("CharacterDeleteDialog:Hide()").unwrap();
    }
    // Cancel consumes the intent even when the text is valid.
    s.run("typed = 'DELETE' CharacterDeleteDialog:Show() CharacterDeleteDialog:Hide() DeleteCharacter(1)").unwrap();
    assert!(s.take_glue_calls().is_empty());
    // Selection drift cannot retarget or resurrect the original intent.
    s.run("CharacterDeleteDialog:Show() SelectCharacter(2) SelectCharacter(1) DeleteCharacter(1)")
        .unwrap();
    assert!(s.take_glue_calls().is_empty());
    s.run("CharacterDeleteDialog:Hide() CharacterDeleteDialog:Show()")
        .unwrap();
    // Even equal-byte roster replacement invalidates a recycled GUID's old confirmation.
    s.load_glue_roster(rows.clone());
    s.take_glue_calls();
    s.run("DeleteCharacter(1)").unwrap();
    assert!(s.take_glue_calls().is_empty());
    s.run("CharacterDeleteDialog:Hide() CharacterDeleteDialog:Show() typed = 'delete' DeleteCharacter(1) DeleteCharacter(1)").unwrap();
    assert!(
        matches!(s.take_glue_calls().as_slice(), [GlueCall::DeleteCharacter(actual)] if *actual == guid)
    );
    assert!(s.eval::<f64>("return shown").unwrap() > 0.0);
    assert!(s.eval::<f64>("return hidden").unwrap() > 0.0);
    character_actions::delete(
        &mut store,
        guid,
        |event| {
            assert_eq!(action_code(event), 0x39);
            Ok(())
        },
        |_| Ok(()),
    )
    .unwrap();
    assert!(store.character(first).is_none());
    assert!(store.character(second).is_some());
    drop(store);
    let reopened = ProfileStore::open(&fixture.0).unwrap();
    assert!(reopened.character(first).is_none());
    assert!(reopened.character(second).is_some());
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

// This harness compiles the action implementation, but not the Bevy profile worker.
// Guard its production dispatch too: testing the isolated action alone missed E0425.
#[test]
fn profile_worker_routes_recovery_to_the_compiled_recovery_module() {
    let worker = include_str!("../../benilla-app/src/net/p2p/profile.rs");
    let arm = worker
        .split_once("super::super::CharRequest::Recover(request) => {")
        .expect("profile worker must handle character recovery")
        .1
        .split_once("// Rename remains unsupported")
        .expect("recovery arm must precede rename handling")
        .0;
    assert!(
        arm.contains("super::recovery_actions::recover(&mut store, request,"),
        "production recovery must call the same recovery_actions module compiled by this harness"
    );
    assert!(!arm.contains("character_actions::recover"));
}
