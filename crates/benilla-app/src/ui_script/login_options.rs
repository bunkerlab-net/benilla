//! The real game Options frame before login. This is a separate, short-lived FrameXML VM:
//! no PLAYER_LOGIN, world feeds, third-party addons, or server preference writes.
use benilla_ui::script::origin::{within, Origin};
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

/// Exclusive lifecycle in PreUpdate, ahead of all VM consumers. Closing is observed one frame
/// after Lua hides the panel, so that the Escape which closes it cannot also quit native login.
pub(super) fn drive(world: &mut World) {
    let Some(options) = world.get_resource::<LoginOptions>() else {
        return;
    };
    let in_login = world
        .get_resource::<State<crate::client_state::ClientState>>()
        .is_some_and(|s| *s.get() == crate::client_state::ClientState::Login);
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
    // The fork's pre-login interface mirrors UI_Init's own VARIABLES_LOADED (`0x49156a`).
    let result = within(Origin::Allowed("ui_script::login_options::open"), || {
        crate::bindings::seed_bindings_for_vm(&mut script);
        crate::bindings::register_binding_readers(world, &mut script);
        script.load_binding_set(1);
        super::lifecycle::seat_raster_seam_for_load(world, &mut script);
        super::addons::register_login_options_addons(&mut script);
        script.set_instruction_budget(super::addons::LOAD_INSTRUCTION_BUDGET);
        super::manifest::silenced_ui_load(&mut script, |script| {
            crate::script_calls::lent_by_world(world, script, |script| {
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
            })
        })
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
    crate::script_calls::with_hosted_vm(world, |script| {
        if let Some(script) = script {
            let _ = script.run("BenillaOptionsFrame:Hide()");
            crate::ui_saved::save(script);
        }
    });
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
    mut vm: crate::script_calls::HostedVm,
) {
    if options.active
        && keys.just_pressed(KeyCode::Escape)
        && !capture.typing
        && !capture.consumed.contains(&KeyCode::Escape)
    {
        vm.run(|script| {
            if let Some(script) = script {
                let _ = script.run("BenillaOptionsFrame:Hide()");
            }
        });
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
        world.insert_resource(State::new(crate::client_state::ClientState::CharSelect));
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
        let mut app = crate::game_plugins::schedule_tests::headless_client_unrun();
        let world = app.world_mut();
        world.insert_resource(State::new(crate::client_state::ClientState::Login));
        assert!(world.resource::<crate::net::SelfGuid>().0.is_none());
        world.insert_resource(crate::vplates::VPlateMode {
            enemies: true,
            friends: true,
        });
        world.insert_non_send_resource(UiScript::new().unwrap());
        let _origin = benilla_ui::script::origin::arm(
            super::super::raise_origin::RAISES_AT_THEIR_OWN_PLACE,
            benilla_ui::script::origin::Failure::Panic,
        );
        open(world).expect("production pre-login Options bootstrap");
        let script = world.non_send_resource::<UiScript>();
        assert!(script.errors().is_empty(), "{:?}", script.errors());
        let (enemies, friends) = script
            .eval::<(bool, bool)>(
                "return NAMEPLATES_ON and true or false, FRIENDNAMEPLATES_ON and true or false",
            )
            .unwrap();
        assert_eq!(
            *world.resource::<crate::vplates::VPlateMode>(),
            crate::vplates::VPlateMode { enemies, friends },
            "VARIABLES_LOADED applies the nameplate verbs through the real host"
        );
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
