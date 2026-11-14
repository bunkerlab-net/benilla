//! Explicit fork entry controls, without replacing stock GlueXML's session or roster.
use super::*;
#[derive(Component)]
pub(super) struct Root;
#[derive(Component)]
pub(super) struct ListenAddressReadout;

pub(super) fn prepare_art(
    mut art: ResMut<crate::glue::art::GlueArt>,
    world_assets: Option<ResMut<benilla_assets::WorldAssets>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<crate::glue::add_material::AddUiMaterial>>,
) {
    if let Some(mut world_assets) = world_assets {
        art.ensure_loaded(&mut world_assets, &mut images, &mut materials);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn materialize_screen(
    mut commands: Commands,
    state: Res<State<ClientState>>,
    p2p: Option<Res<crate::net::P2pSettings>>,
    settings: Res<settings::Settings>,
    options: Res<crate::ui_script::login_options::LoginOptions>,
    mut roots: Query<(Entity, &mut Visibility), With<Root>>,
    mut previous: Local<Option<(ClientState, bool)>>,
    assets: Res<AssetServer>,
) {
    let key = (*state.get(), p2p.is_some());
    let hidden = settings.open || options.active || options.requested;
    if *previous == Some(key) && !roots.is_empty() {
        for (_, mut visibility) in &mut roots {
            *visibility = if hidden {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        return;
    }
    for (entity, _) in &roots {
        commands.entity(entity).despawn();
    }
    *previous = Some(key);
    let font = crate::char_select::wow_font(&assets);
    commands
        .spawn((
            Root,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(24.0),
                bottom: Val::Px(24.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.95)),
            GlobalZIndex(100),
            if hidden {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            },
        ))
        .with_children(|ui| {
            let mut actions = vec![("Connection setup", LoginAction::ConnectionSetup)];
            if *state.get() == ClientState::Login {
                actions.push(("Game Options", LoginAction::Settings));
                if p2p.is_some() {
                    actions.insert(
                        0,
                        ("Solo / P2P: create or select character", LoginAction::Start),
                    );
                }
            } else if *state.get() == ClientState::CharSelect && p2p.is_some() {
                actions.push(("Recover deleted character", LoginAction::Recover));
            }
            for (label, action) in actions {
                ui.spawn((
                    Button,
                    action,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.22, 0.10, 0.04)),
                ))
                .with_child((
                    Text::new(label),
                    TextFont {
                        font: font.clone(),
                        font_size: 17.0,
                        ..default()
                    },
                    TextColor(crate::glue::art::GOLD),
                ));
            }
            if p2p.is_some() {
                ui.spawn((
                    ListenAddressReadout,
                    Text::new(""),
                    TextFont {
                        font,
                        font_size: 14.0,
                        ..default()
                    },
                ));
            }
        });
}

/// Read-only discovery, explicitly distinguishing configuration from an actual bound endpoint.
pub(super) fn refresh_listen_addresses(
    p2p: Option<Res<crate::net::P2pSettings>>,
    ticket: Option<Res<crate::net::P2pEndpointTicket>>,
    mut cached: Local<Option<(std::net::SocketAddr, String)>>,
    mut texts: Query<&mut Text, With<ListenAddressReadout>>,
) {
    let Some(settings) = p2p else {
        return;
    };
    let listening = ticket
        .as_deref()
        .and_then(crate::net::P2pEndpointTicket::local_addr);
    let listen = listening.unwrap_or(settings.0.listen);
    if cached.as_ref().is_none_or(|(old, _)| *old != listen) {
        let addresses = crate::net::listen_addresses::discover(listen);
        let value = if addresses.is_empty() {
            format!("{listen} (no active matching interface)")
        } else {
            addresses
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" · ")
        };
        *cached = Some((listen, value));
    }
    let label = format!(
        "{}\n{}",
        if listening.is_some() {
            "Listening endpoints"
        } else {
            "Configured endpoints (not listening)"
        },
        cached.as_ref().map_or("", |(_, value)| value)
    );
    for mut text in &mut texts {
        if text.0 != label {
            text.0.clone_from(&label);
        }
    }
}

/// Close the pre-world overlay at world entry; its Update driver is intentionally gated out there.
pub(super) fn close(mut commands: Commands, roots: Query<Entity, With<Root>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn refresh_boxes(
    mut settings: ResMut<super::settings::Settings>,
    time: Res<Time>,
    mut fields: Query<
        (
            &crate::glue::widgets::GlueFieldPart,
            Option<&mut Text>,
            &mut Visibility,
        ),
        With<super::settings::Edit>,
    >,
) {
    if settings.open {
        crate::textinput::tick_caret(&mut settings.edit, true, time.delta_secs());
        crate::glue::widgets::paint_glue_field(&settings.edit, true, fields.iter_mut());
    }
}
