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
        secret: true,
        locked: false,
    }];
    settings.select(0);
    assert!(settings.edit.password);
    settings.edit.password = false;
    settings.select(0);
    assert!(settings.edit.password);
}
#[test]
fn invalid_cvar_draft_never_changes_registry() {
    let mut cvars = Cvars::default();
    let before = cvars.get("MasterVolume").unwrap().to_owned();
    for value in ["NaN", "inf", "not a number"] {
        assert!(cvars
            .apply_settings(&BTreeMap::from([("MasterVolume".into(), value.into())]))
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
        .init_resource::<super::super::LoginForm>()
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
    cvars.apply_settings(&changes).unwrap();
    assert_eq!(cvars.get("MasterVolume"), Some("0.25"));
    assert!(std::fs::read_to_string(temp.join("config.toml"))
        .unwrap()
        .contains("0.25"));
    let blocked = temp.join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    let _blocked = EnvGuard::set("BENILLA_HOME", blocked.to_str().unwrap());
    assert!(cvars
        .apply_settings(&BTreeMap::from([("MasterVolume".into(), "0.75".into())]))
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
