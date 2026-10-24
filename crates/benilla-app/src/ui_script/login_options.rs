//! The real game Options frame before login. This is a separate, short-lived FrameXML VM:
//! no PLAYER_LOGIN, world feeds, third-party addons, or server preference writes.
use benilla_ui::script::UiScript;
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct LoginOptions {
    pub(crate) requested: bool,
    pub(crate) active: bool,
    pub(crate) error: Option<String>,
}

struct ParkedLoginVm(UiScript);
const SHELL: &str = include_str!("login_options.lua");

pub(crate) fn active(options: Option<Res<LoginOptions>>) -> bool {
    options.is_some_and(|o| o.active)
}

/// Exclusive lifecycle in PreUpdate, ahead of all VM consumers. Closing is observed one frame
/// after Lua hides the panel, so that the Escape which closes it cannot also quit native login.
pub(super) fn drive(world: &mut World) {
    let Some(options) = world.get_resource::<LoginOptions>() else {
        return;
    };
    let in_login = world
        .get_resource::<State<crate::char_select::ClientState>>()
        .is_some_and(|s| *s.get() == crate::char_select::ClientState::Login);
    if options.active {
        let shown = world
            .get_non_send_resource::<UiScript>()
            .is_some_and(|s| s.frame_visible("BenillaOptionsFrame"));
        if !in_login || !shown {
            close(world);
        }
        return;
    }
    if !options.requested {
        return;
    }
    world.resource_mut::<LoginOptions>().requested = false;
    if !in_login {
        return;
    }
    match open(world) {
        Ok(()) => {
            let mut options = world.resource_mut::<LoginOptions>();
            options.active = true;
            options.error = None;
        }
        Err(()) => {
            restore(world);
            world.resource_mut::<LoginOptions>().error = Some(
                "Game Options could not load. Check your Classic 1.12.1 game data in Connection setup. No settings were saved.".into());
        }
    }
}

fn open(world: &mut World) -> Result<(), ()> {
    // Never substitute a raw CVar table when the actual game UI cannot load.
    if super::reference_ui::core().is_none() || !super::manifest::layer_enabled() {
        return Err(());
    }
    crate::cvars::fold_dying_vm_cvars(world);
    if let Some(vm) = world.remove_non_send_resource::<UiScript>() {
        world.insert_non_send_resource(ParkedLoginVm(vm));
    }
    let mut script = super::lifecycle::new_vm(world, UiScript::new).ok_or(())?;
    if let Some(cvars) = world.get_resource::<crate::cvars::Cvars>() {
        script.set_cvar_saved_base(cvars.orphans());
        script.seed_cvars(cvars.vm_seed());
    }
    crate::bindings::seed_bindings_for_vm(world, &mut script);
    script.load_binding_set(1);
    super::lifecycle::seat_raster_seam_for_load(world, &mut script);
    super::addons::register_login_options_addons(&mut script);
    script.set_instruction_budget(super::addons::LOAD_INSTRUCTION_BUDGET);
    let result = super::manifest::silenced_ui_load(&mut script, |script| {
        if !super::load_default_ui(script).is_empty() {
            return Err(());
        }
        script.set_instruction_budget(super::addons::LOAD_INSTRUCTION_BUDGET);
        crate::ui_saved::load_saved_variables(script, |_| {});
        script.run(SHELL).map_err(|_| ())?;
        if !script.frame_visible("BenillaOptionsFrame") {
            return Err(());
        }
        Ok(())
    });
    script.clear_instruction_budget();
    result?;
    world.insert_non_send_resource(script);
    Ok(())
}

fn restore(world: &mut World) {
    if let Some(ParkedLoginVm(mut script)) = world.remove_non_send_resource::<ParkedLoginVm>() {
        world.remove_non_send_resource::<UiScript>();
        // Re-seed the parked VM before its next CVar mirror pass, never replay its old values.
        if let Some(cvars) = world.get_resource::<crate::cvars::Cvars>() {
            script.set_cvar_saved_base(cvars.orphans());
            script.seed_cvars(cvars.vm_seed());
        }
        world.insert_non_send_resource(script);
    }
}

/// Called before the world-entry park and on quit as well as the normal Close button path.
/// The game's OnHide discards pending Apply edits; committed immediate edits retain the exact
/// same semantics as the in-world Options window.
pub(super) fn close(world: &mut World) {
    if !world
        .get_resource::<LoginOptions>()
        .is_some_and(|o| o.active)
    {
        return;
    }
    if let Some(mut script) = world.get_non_send_resource_mut::<UiScript>() {
        let _ = script.run("BenillaOptionsFrame:Hide()");
        crate::ui_saved::save(&mut script);
    }
    crate::cvars::fold_dying_vm_cvars(world);
    if let Some(mut cvars) = world.get_resource_mut::<crate::cvars::Cvars>() {
        cvars.save();
    }
    restore(world);
    let mut options = world.resource_mut::<LoginOptions>();
    options.active = false;
    options.requested = false;
}

/// Search boxes and keybinding capture get first refusal, exactly as they do in-world.
pub(super) fn escape(
    options: Res<LoginOptions>,
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<super::UiKeyboardCapture>,
    script: Option<NonSend<UiScript>>,
) {
    if options.active
        && keys.just_pressed(KeyCode::Escape)
        && !capture.typing
        && !capture.consumed.contains(&KeyCode::Escape)
    {
        if let Some(script) = script {
            let _ = script.run("BenillaOptionsFrame:Hide()");
        }
    }
}

#[cfg(test)]
#[path = "login_options_tests.rs"]
mod shell_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_open_without_a_parked_vm_never_removes_login_vm() {
        let mut world = World::new();
        let script = UiScript::new().unwrap();
        let session = script.session();
        world.insert_non_send_resource(script);
        restore(&mut world);
        assert_eq!(world.non_send_resource::<UiScript>().session(), session);
    }

    #[test]
    fn repeated_close_is_safe_and_restores_the_original_login_vm() {
        let mut world = World::new();
        let original = UiScript::new().unwrap();
        let session = original.session();
        world.insert_non_send_resource(ParkedLoginVm(original));
        let options = UiScript::new().unwrap();
        options
            .run("CreateFrame('Frame', 'BenillaOptionsFrame')")
            .unwrap();
        world.insert_non_send_resource(options);
        world.insert_resource(LoginOptions {
            active: true,
            ..default()
        });
        close(&mut world);
        close(&mut world);
        assert!(!world.resource::<LoginOptions>().active);
        assert_eq!(world.non_send_resource::<UiScript>().session(), session);
        assert!(world.get_non_send_resource::<ParkedLoginVm>().is_none());
    }

    #[test]
    fn navigation_away_closes_options_and_drops_pending_reopen() {
        let mut world = World::new();
        world.insert_resource(State::new(crate::char_select::ClientState::CharSelect));
        let original = UiScript::new().unwrap();
        let session = original.session();
        world.insert_non_send_resource(ParkedLoginVm(original));
        let options = UiScript::new().unwrap();
        options
            .run("CreateFrame('Frame', 'BenillaOptionsFrame')")
            .unwrap();
        world.insert_non_send_resource(options);
        world.insert_resource(LoginOptions {
            active: true,
            requested: true,
            error: None,
        });
        drive(&mut world);
        assert!(!world.resource::<LoginOptions>().active);
        assert!(!world.resource::<LoginOptions>().requested);
        assert_eq!(world.non_send_resource::<UiScript>().session(), session);
        drive(&mut world);
        assert_eq!(world.non_send_resource::<UiScript>().session(), session);
    }

    #[test]
    fn actual_options_bootstrap_opens_every_category_without_a_player_session() {
        let _data = benilla_formats::wow_data_or_skip!();
        let mut world = World::new();
        world.insert_resource(crate::cvars::Cvars::default());
        world.insert_resource(LoginOptions::default());
        world.insert_non_send_resource(UiScript::new().unwrap());
        open(&mut world).expect("production pre-login Options bootstrap");
        let script = world.non_send_resource::<UiScript>();
        for category in [
            "Controls",
            "Interface",
            "ActionBars",
            "Combat",
            "Chat",
            "Keybindings",
            "Nameplates",
            "Graphics",
            "Audio",
        ] {
            script
                .run(&format!("BenillaOptionsFrame_SelectCategory('{category}')"))
                .unwrap();
            assert!(script.frame_visible("BenillaOptionsFrame"));
            assert!(!script.frame_visible("MainMenuBar"));
        }
        assert!(script.errors().is_empty(), "{:?}", script.errors());
    }
}
