//! Fork connection entry and recovery overlays. Stock GlueXML owns realm login and characters.
mod screen;
pub(crate) mod settings;
use crate::client_state::ClientState;
use bevy::prelude::*;

pub(crate) struct LoginPlugin;
impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<settings::Settings>()
            .init_resource::<crate::char_select::recovery::RecoveryPanel>()
            .add_systems(OnExit(ClientState::Login), settings::close)
            .add_systems(
                OnEnter(ClientState::InWorld),
                (
                    screen::close,
                    settings::close,
                    crate::char_select::recovery::close,
                ),
            )
            .add_systems(
                Update,
                (
                    screen::materialize_screen,
                    login_input.after(crate::game_tip::drive_game_tip),
                    settings::recover,
                    settings::input,
                    settings::render.before(crate::cvars::sync_cvars),
                    screen::refresh_boxes,
                    screen::refresh_listen_addresses,
                    crate::char_select::recovery::drive,
                    crate::glue::dialog::drive_glue_dialog,
                    crate::glue::dialog::refresh_dialog_box,
                )
                    .chain()
                    .after(crate::ui_script::UiTick)
                    .before(crate::ui_script::UiDrain)
                    .after(benilla_world::schedule::WorldStage::Net)
                    .before(crate::glue::GlueVisuals)
                    .before(crate::video::apply_present_mode)
                    .before(crate::video::apply_window_mode)
                    .run_if(not(in_state(ClientState::InWorld))),
            );
    }
}

/// Native overlays own their keyboard and pointer, not the stock frames underneath them.
pub(crate) fn blocks_stock_input(world: &mut World) -> bool {
    let pointer_on_controls = world
        .query_filtered::<&Interaction, With<LoginAction>>()
        .iter(world)
        .any(|i| *i != Interaction::None);
    pointer_on_controls
        || world
            .get_resource::<settings::Settings>()
            .is_some_and(|s| s.open)
        || world
            .get_resource::<crate::char_select::recovery::RecoveryPanel>()
            .is_some_and(|s| s.open)
        || world
            .get_resource::<crate::glue::dialog::GlueDialog>()
            .is_some_and(|s| s.is_open())
        || (world
            .get_resource::<State<ClientState>>()
            .is_some_and(|s| *s.get() == ClientState::Login)
            && world.contains_resource::<crate::net::P2pSettings>()
            && !world
                .get_resource::<crate::ui_script::login_options::LoginOptions>()
                .is_some_and(|o| o.active))
}

#[derive(Component, Clone, Copy)]
pub(super) enum LoginAction {
    Start,
    Settings,
    ConnectionSetup,
    Recover,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn login_input(
    buttons: Query<(Entity, &LoginAction)>,
    clicks: Res<crate::glue::GlueClicks>,
    mut settings: ResMut<settings::Settings>,
    cvars: Res<crate::cvars::Cvars>,
    recovery: Option<Res<settings::ConfigRecovery>>,
    mut options: ResMut<crate::ui_script::login_options::LoginOptions>,
    mut start: Option<ResMut<crate::net::P2pStart>>,
    catalog: Option<Res<crate::entities::CharCreate>>,
    emotes: Option<Res<crate::sound::EmoteSounds>>,
    mut dialog: ResMut<crate::glue::dialog::GlueDialog>,
    pick: Res<crate::net::CharPick>,
    mut panel: ResMut<crate::char_select::recovery::RecoveryPanel>,
) {
    if settings.open || panel.open || options.active || options.requested || dialog.is_open() {
        return;
    }
    if let Some(error) = options.error.take() {
        dialog.open_error(&error);
        return;
    }
    for (entity, action) in &buttons {
        if !clicks.hit(entity) {
            continue;
        }
        match action {
            LoginAction::Settings => options.requested = true,
            LoginAction::ConnectionSetup => settings.open(&cvars, recovery.as_deref()),
            LoginAction::Recover => panel.open(&pick),
            LoginAction::Start => {
                if recovery.is_some() {
                    settings.open(&cvars, recovery.as_deref());
                    continue;
                }
                match start.as_deref_mut() {
                    Some(start) if start.can_start() => {
                        if let Err(error) = start.start(
                            catalog.as_deref().map(|c| &c.0),
                            emotes.as_deref().map(|e| &e.0),
                        ) {
                            dialog.open_error(&error);
                        }
                    }
                    Some(_) => {
                        dialog.open_error("Characters are already open or the session is closing.")
                    }
                    None => dialog.open_error("Solo/P2P is unavailable. Check Connection setup."),
                }
            }
        }
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(State::new(ClientState::Login));
        world.insert_resource(settings::Settings::default());
        world.insert_resource(crate::ui_script::login_options::LoginOptions::default());
        world
    }

    #[test]
    fn connection_draft_blocks_underlying_stock_keyboard_and_pointer() {
        let mut world = world();
        assert!(!blocks_stock_input(&mut world));
        world.resource_mut::<settings::Settings>().open = true;
        assert!(blocks_stock_input(&mut world));
        world.resource_mut::<settings::Settings>().open = false;
        assert!(!blocks_stock_input(&mut world));
    }

    #[test]
    fn native_controls_consume_their_pointer_without_hiding_stock_roster() {
        let mut world = world();
        let button = world
            .spawn((LoginAction::ConnectionSetup, Interaction::Hovered))
            .id();
        assert!(blocks_stock_input(&mut world));
        world.entity_mut(button).insert(Interaction::None);
        assert!(!blocks_stock_input(&mut world));
    }

    #[test]
    fn local_entry_never_types_into_stock_login_but_game_options_still_receive_input() {
        let mut world = world();
        let config = benilla_config::AppConfig::parse(
            "[p2p]\npsk = \"public test label only\"\n",
            std::path::Path::new("/tmp"),
        )
        .unwrap();
        world.insert_resource(crate::net::P2pSettings(config.p2p.unwrap(), config.ruleset));
        assert!(blocks_stock_input(&mut world));
        world
            .resource_mut::<crate::ui_script::login_options::LoginOptions>()
            .active = true;
        assert!(!blocks_stock_input(&mut world));
        world
            .resource_mut::<crate::ui_script::login_options::LoginOptions>()
            .active = false;
        world.insert_resource(State::new(ClientState::CharSelect));
        assert!(
            !blocks_stock_input(&mut world),
            "stock roster remains the character picker"
        );
    }
}
