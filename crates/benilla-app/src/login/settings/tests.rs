use super::*;
#[test]
fn pending_connection_edits_are_cancelled_and_reopening_is_clean() {
    let cvars = Cvars::default();
    let old = "".to_owned();
    let mut settings = Settings::default();
    settings.refresh_fields(&cvars);
    let index = settings
        .fields
        .iter()
        .position(|f| f.name == "wow.data")
        .unwrap();
    settings.fields[index].locked = false;
    settings.select(index);
    settings.edit.set_text("0.25");
    settings.keep_edit();
    assert_eq!(settings.changes["wow.data"], "0.25");
    assert!(settings.config.is_none());
    settings = Settings::default();
    settings.refresh_fields(&cvars);
    assert_eq!(
        settings
            .fields
            .iter()
            .find(|f| f.name == "wow.data")
            .unwrap()
            .value,
        old
    );
}
#[test]
fn refreshing_fields_retains_draft_and_secret_is_masked_again_on_selection() {
    let cvars = Cvars::default();
    let mut settings = Settings::default();
    settings.refresh_fields(&cvars);
    let index = settings
        .fields
        .iter()
        .position(|f| f.name == "wow.data")
        .unwrap();
    settings.fields[index].locked = false;
    settings.select(index);
    settings.edit.set_text("0.4");
    settings.refresh_fields(&cvars);
    settings.refresh_fields(&cvars);
    assert_eq!(
        settings
            .fields
            .iter()
            .find(|f| f.name == "wow.data")
            .unwrap()
            .value,
        "0.4"
    );
    settings.fields = vec![Field {
        name: "p2p.psk".into(),
        value: "private sample passphrase".into(),
        saved_value: "private sample passphrase".into(),
        secret: true,
        locked: false,
    }];
    settings.select(0);
    assert!(settings.edit.password);
    settings.edit.password = false;
    settings.selected = None;
    settings.select(0);
    assert!(settings.edit.password);
}
#[test]
fn invalid_cvar_draft_never_changes_registry() {
    let mut cvars = Cvars::default();
    let before = cvars.get("MasterVolume").unwrap().to_owned();
    for value in ["NaN", "inf", "not a number"] {
        assert!(cvars
            .apply_finite_settings_draft_for_test(&BTreeMap::from([(
                "MasterVolume".into(),
                value.into()
            )]))
            .is_err());
        assert_eq!(cvars.get("MasterVolume"), Some(before.as_str()));
    }
}
#[test]
fn every_startup_field_is_reachable() {
    let cvars = Cvars::default();
    let mut settings = Settings::default();
    settings.refresh_fields(&cvars);
    assert_eq!(settings.fields.len(), STARTUP.len());
}
#[test]
fn recovery_disables_network_plugin() {
    let mut app = App::new();
    app.insert_resource(ConfigRecovery("invalid config".into()));
    app.add_plugins(crate::net::NetPlugin { connect: true });
    assert!(app.world().contains_resource::<crate::net::NetOffline>());
}
#[test]
fn recovery_diagnostics_do_not_spawn_a_worker() {
    let mut settings = Settings {
        recovery: true,
        ..default()
    };
    diagnostics::start(&mut settings, false);
    assert!(settings.diagnostic.is_none());
    diagnostics::start(&mut settings, true);
    assert!(settings.diagnostic.is_none());
}
#[test]
fn headless_panel_reopen_and_close_leave_no_duplicate_or_secret_text() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Settings>()
        .init_resource::<crate::glue::art::GlueArt>()
        .add_message::<bevy::input::mouse::MouseWheel>()
        .add_systems(
            Update,
            (render, super::super::screen::refresh_boxes).chain(),
        );
    app.world_mut()
        .spawn((Window::default(), bevy::window::PrimaryWindow));
    let cvars = Cvars::default();
    for _ in 0..3 {
        {
            let mut settings = app.world_mut().resource_mut::<Settings>();
            settings.open = true;
            settings.refresh_fields(&cvars);
            settings.fields[1].value = "private sample passphrase".into();
            settings.fields[1].saved_value = "private sample passphrase".into();
            settings.select(1);
        }
        app.update();
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<view::Root>>()
                .iter(app.world())
                .count(),
            1
        );
        assert!(app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .all(|t| !t.0.contains("private sample passphrase")));
        *app.world_mut().resource_mut::<Settings>() = Settings::default();
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<view::Root>>()
                .iter(app.world())
                .count(),
            0
        );
    }
}
#[test]
fn game_save_writes_then_applies_and_failed_save_keeps_registry() {
    use crate::local_state::test_env::{EnvGuard, ENV_LOCK};
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let temp = std::env::temp_dir().join(format!("benilla-login-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let _capture = EnvGuard::unset("WOW_CAPTURE");
    let _home = EnvGuard::set("BENILLA_HOME", temp.to_str().unwrap());
    let mut cvars = Cvars::default();
    let changes = BTreeMap::from([("MasterVolume".into(), "0.25".into())]);
    cvars
        .apply_finite_settings_draft_for_test(&changes)
        .unwrap();
    assert_eq!(cvars.get("MasterVolume"), Some("0.25"));
    assert!(std::fs::read_to_string(temp.join("config.toml"))
        .unwrap()
        .contains("0.25"));
    let blocked = temp.join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    let _blocked = EnvGuard::set("BENILLA_HOME", blocked.to_str().unwrap());
    assert!(cvars
        .apply_finite_settings_draft_for_test(&BTreeMap::from([(
            "MasterVolume".into(),
            "0.75".into()
        )]))
        .is_err());
    assert_eq!(cvars.get("MasterVolume"), Some("0.25"));
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn interrupted_login_removes_settings_overlay() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = App::new();
    app.init_resource::<Settings>();
    app.world_mut().spawn(view::Root);
    app.world_mut().resource_mut::<Settings>().open = true;
    app.world_mut().run_system_once(close).unwrap();
    assert!(!app.world().resource::<Settings>().open);
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<view::Root>>()
            .iter(app.world())
            .count(),
        0
    );
}

#[test]
fn enabling_p2p_with_an_untouched_empty_passphrase_writes_an_explicit_empty_value() {
    let path =
        std::env::temp_dir().join(format!("benilla-empty-phrase-{}.toml", std::process::id()));
    let mut settings = Settings {
        config: Some(ConfigEditor::open(&path).unwrap()),
        ..default()
    };
    settings.changes.insert("p2p.enabled".into(), "true".into());
    let editor = settings.prepare_config().unwrap();
    assert_eq!(editor.get("p2p", "psk").as_deref(), Some(""));
    assert!(!path.exists(), "preparing the draft must not write a file");
}

#[test]
fn classic_capture_path_uses_existing_optional_text_field_and_cancel_discards_it() {
    let cvars = Cvars::default();
    let mut settings = Settings::default();
    settings.refresh_fields(&cvars);
    let index = settings
        .fields
        .iter()
        .position(|f| f.name == "p2p.classic_start_captures")
        .unwrap();
    assert_eq!(settings.fields[index].value, "");
    settings.fields[index].locked = false;
    settings.select(index);
    settings.edit.set_text("benilla-config/classic-starts");
    settings.keep_edit();
    assert_eq!(
        settings.changes["p2p.classic_start_captures"],
        "benilla-config/classic-starts"
    );
    let mut reopened = Settings::default();
    reopened.refresh_fields(&cvars);
    assert_eq!(reopened.fields[index].value, "");
}

#[test]
fn legacy_internet_values_are_visible_under_iroh_and_disable_removes_the_table() {
    let path =
        std::env::temp_dir().join(format!("benilla-iroh-settings-{}.toml", std::process::id()));
    let original = "[p2p]\npsk = \"\"\n[p2p.internet]\nport_mapping = true\nrelay_urls = []\n";
    std::fs::write(&path, original).unwrap();
    let mut settings = Settings {
        config: Some(ConfigEditor::open(&path).unwrap()),
        ..default()
    };
    settings.refresh_fields(&Cvars::default());
    assert!(settings
        .fields
        .iter()
        .all(|field| !field.name.starts_with("p2p.internet")));
    for name in ["p2p.iroh.enabled", "p2p.iroh.port_mapping"] {
        assert_eq!(
            settings
                .fields
                .iter()
                .find(|field| field.name == name)
                .unwrap()
                .value,
            "true"
        );
    }
    let draft = settings.prepare_config().unwrap();
    assert!(draft.has_section("p2p.iroh"));
    assert!(!draft.has_section("p2p.internet"));
    settings
        .changes
        .insert("p2p.iroh.enabled".into(), "false".into());
    let draft = settings.prepare_config().unwrap();
    assert!(!draft.has_section("p2p.iroh"));
    assert!(!draft.has_section("p2p.internet"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        original,
        "draft edits and opening do not write the migration"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn enabling_iroh_creates_only_the_canonical_table_with_mapping_disabled() {
    let path = std::env::temp_dir().join(format!(
        "benilla-new-iroh-settings-{}.toml",
        std::process::id()
    ));
    let mut settings = Settings {
        config: Some(ConfigEditor::open(&path).unwrap()),
        ..default()
    };
    settings.changes.insert("p2p.enabled".into(), "true".into());
    settings
        .changes
        .insert("p2p.iroh.enabled".into(), "true".into());
    let draft = settings.prepare_config().unwrap();
    assert_eq!(
        draft.get("p2p.iroh", "port_mapping").as_deref(),
        Some("false")
    );
    assert!(!draft.has_section("p2p.internet"));
    assert!(!path.exists());
}

#[test]
fn groups_cover_all_fields_without_pages_and_remember_selection_and_drafts() {
    let mut settings = Settings::default();
    settings.refresh_fields(&Cvars::default());
    for field in &mut settings.fields {
        field.locked = false;
    }
    for group in FieldGroup::ALL {
        let count = settings
            .fields
            .iter()
            .filter(|field| FieldGroup::for_field(&field.name) == group)
            .count();
        assert!((1..=7).contains(&count));
    }
    let network = settings
        .fields
        .iter()
        .position(|field| field.name == "p2p.network")
        .unwrap();
    settings.select(network);
    settings.edit.set_text("draft-network");
    settings.edit.cursor = 3;
    settings.select(network);
    assert_eq!(settings.edit.cursor, 3, "repeated row click keeps caret");
    settings.select_group(FieldGroup::Iroh);
    let iroh = settings.selected;
    settings.select_group(FieldGroup::World);
    settings.select_group(FieldGroup::Iroh);
    assert_eq!(settings.selected, iroh);
    settings.select_group(FieldGroup::Connection);
    assert_eq!(settings.selected, Some(network));
    assert_eq!(settings.edit.text, "draft-network");
    assert_eq!(settings.changes["p2p.network"], "draft-network");
}

#[test]
fn boolean_toggles_are_typed_draft_changes_and_respect_read_only_fields() {
    let mut settings = Settings::default();
    settings.refresh_fields(&Cvars::default());
    for name in [
        "p2p.enabled",
        "p2p.iroh.enabled",
        "p2p.iroh.port_mapping",
        "p2p.encounter.enabled",
    ] {
        let index = settings
            .fields
            .iter()
            .position(|field| field.name == name)
            .unwrap();
        settings.toggle_boolean(index);
        assert_eq!(settings.fields[index].value, "false");
        settings.fields[index].locked = false;
        settings.toggle_boolean(index);
        assert_eq!(settings.fields[index].value, "true");
        assert_eq!(settings.changes[name], "true");
        settings.toggle_boolean(index);
        assert_eq!(settings.fields[index].value, "false");
        assert!(!settings.changes.contains_key(name));
    }
    let network = settings
        .fields
        .iter()
        .position(|field| field.name == "p2p.network")
        .unwrap();
    settings.fields[network].locked = false;
    settings.toggle_boolean(network);
    assert_eq!(settings.fields[network].value, "benilla");
    settings.toggle_boolean(usize::MAX);
}

#[test]
fn successful_save_keeps_group_field_caret_and_other_changes_then_failure_keeps_draft() {
    let path =
        std::env::temp_dir().join(format!("benilla-save-context-{}.toml", std::process::id()));
    let mut settings = Settings {
        config: Some(ConfigEditor::open(&path).unwrap()),
        ..default()
    };
    let mut cvars = Cvars::default();
    settings.refresh_fields(&cvars);
    let xp = settings
        .fields
        .iter()
        .position(|field| field.name == "ruleset.xp_rate")
        .unwrap();
    let drop = settings
        .fields
        .iter()
        .position(|field| field.name == "ruleset.drop_rate")
        .unwrap();
    settings.select(xp);
    settings.edit.set_text("1.25");
    settings.select(drop);
    settings.edit.set_text("1.5");
    settings.edit.cursor = 1;
    let edit = settings.edit.clone();
    settings.save(&mut cvars).unwrap();
    assert_eq!(settings.group, FieldGroup::World);
    assert_eq!(settings.selected, Some(drop));
    assert_eq!(settings.edit, edit);
    assert!(settings.changes.is_empty());
    assert_eq!(
        settings
            .config
            .as_ref()
            .unwrap()
            .get("ruleset", "xp_rate")
            .as_deref(),
        Some("1.25")
    );
    let saved = std::fs::read_to_string(&path).unwrap();
    settings.edit.set_text("not-a-number");
    assert!(settings.save(&mut cvars).is_err());
    assert_eq!(settings.group, FieldGroup::World);
    assert_eq!(settings.selected, Some(drop));
    assert_eq!(settings.edit.text, "not-a-number");
    assert_eq!(settings.changes["ruleset.drop_rate"], "not-a-number");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
    assert!(settings.has_unsaved_changes());
    settings = Settings::default(); // Escape/Close discard only the unsaved in-memory draft.
    assert!(!settings.has_unsaved_changes());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn restoring_original_value_clears_dirty_state_without_losing_other_edits() {
    let mut settings = Settings::default();
    settings.refresh_fields(&Cvars::default());
    for field in &mut settings.fields {
        field.locked = false;
    }
    let network = settings
        .fields
        .iter()
        .position(|field| field.name == "p2p.network")
        .unwrap();
    let listen = settings
        .fields
        .iter()
        .position(|field| field.name == "p2p.listen")
        .unwrap();
    settings.select(network);
    settings.edit.set_text("other-network");
    settings.keep_edit();
    assert!(settings.has_unsaved_changes());
    settings.edit.set_text("benilla");
    assert!(
        !settings.has_unsaved_changes(),
        "undo is visible before committing the edit buffer"
    );
    settings.keep_edit();
    assert!(settings.changes.is_empty());
    settings.select(listen);
    settings.edit.set_text("0.0.0.0:7475");
    settings.select(network);
    settings.edit.set_text("temporary");
    settings.keep_edit();
    settings.edit.set_text("benilla");
    settings.keep_edit();
    assert!(!settings.changes.contains_key("p2p.network"));
    assert_eq!(settings.changes["p2p.listen"], "0.0.0.0:7475");
    assert!(settings.has_unsaved_changes());
}

#[test]
fn ticket_preview_is_invalidated_when_endpoint_changes_or_apply_starts() {
    let mut settings = Settings {
        ticket_preview: Some("old endpoint".into()),
        ..default()
    };
    settings.sync_ticket_preview(Some("old endpoint"));
    assert_eq!(settings.ticket_preview.as_deref(), Some("old endpoint"));
    assert!(!settings.refresh);
    settings.sync_ticket_preview(Some("new endpoint"));
    assert!(settings.ticket_preview.is_none());
    assert!(settings.refresh);
    settings.ticket_preview = Some("current endpoint".into());
    settings.refresh = false;
    settings.sync_ticket_preview(None); // Pending/Applied transitions or missing current endpoint.
    assert!(settings.ticket_preview.is_none());
    assert!(settings.refresh);
}
