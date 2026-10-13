use super::*;
use crate::glue::widgets::{glue_edit_box, paint_glue_field, GlueFieldPart};
#[derive(Component)]
pub(in crate::login) struct Root;
#[derive(Component, Clone)]
pub(in crate::login) struct Edit;
fn label(ui: &mut ChildSpawnerCommands, text: &str, s: f32) {
    ui.spawn((
        Text::new(text),
        TextFont {
            font_size: 15.0 * s,
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.90, 0.75)),
    ));
}
fn button(ui: &mut ChildSpawnerCommands, action: Action, text: &str, s: f32) {
    ui.spawn((
        Button,
        action,
        Node {
            padding: UiRect::axes(Val::Px(10.0 * s), Val::Px(5.0 * s)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.20, 0.14, 0.10)),
    ))
    .with_children(|b| label(b, text, s));
}
fn row(s: f32) -> Node {
    Node {
        column_gap: Val::Px(8.0 * s),
        align_items: AlignItems::Center,
        ..default()
    }
}
#[allow(clippy::type_complexity)]
pub(in crate::login) fn render(
    mut commands: Commands,
    mut settings: ResMut<Settings>,
    roots: Query<Entity, With<Root>>,
    art: Res<crate::glue::art::GlueArt>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut boxes: Query<(&GlueFieldPart, Option<&mut Text>, &mut Visibility), With<Edit>>,
    mut last_scale: Local<f32>,
) {
    let s = crate::glue::screen_scale(windows.single().ok()).min(1.3);
    if !settings.open {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }
    paint_glue_field(&settings.edit, true, boxes.iter_mut());
    if !settings.refresh && *last_scale == s && !roots.is_empty() {
        return;
    }
    settings.refresh = false;
    *last_scale = s;
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    let overlay = Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };
    let panel = Node {
        width: Val::Px(760.0 * s),
        max_width: Val::Percent(98.0),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(7.0 * s),
        padding: UiRect::all(Val::Px(16.0 * s)),
        ..default()
    };
    // The embedded default font keeps recovery usable with a broken/missing game install.
    commands
        .spawn((
            Root,
            GlobalZIndex(3000),
            overlay,
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
        ))
        .with_children(|root| {
            root.spawn((panel, BackgroundColor(Color::srgb(0.065, 0.065, 0.075))))
                .with_children(|ui| content(ui, &settings, &art, s));
        });
}
fn content(
    ui: &mut ChildSpawnerCommands,
    settings: &Settings,
    art: &crate::glue::art::GlueArt,
    s: f32,
) {
    label(ui, "Settings", s * 1.4);
    ui.spawn(row(s)).with_children(|r| {
        for (page, title) in [
            (Page::Graphics, "Graphics"),
            (Page::Audio, "Sound"),
            (Page::All, "Advanced CVars"),
            (Page::Startup, "Startup / P2P"),
        ] {
            button(r, Action::Page(page), title, s);
        }
    });
    let help = if settings.page == Some(Page::Startup) {
        "Startup: executable config.toml. Save requires restart. Passphrase is stored locally."
    } else {
        "Game settings: engine CVars. Click a row to edit. Session-controlled rows are read-only."
    };
    label(ui, help, s * 0.82);
    for (i, field) in settings
        .fields
        .iter()
        .enumerate()
        .skip(settings.offset)
        .take(PAGE_SIZE)
    {
        let value = if field.secret {
            if field.value.is_empty() {
                "(not set)".into()
            } else {
                "********".into()
            }
        } else {
            field.value.chars().take(65).collect::<String>()
        };
        let mark = if settings.selected == Some(i) {
            "> "
        } else {
            ""
        };
        let lock = if field.locked { " [read-only]" } else { "" };
        button(
            ui,
            Action::Select(i),
            &format!("{mark}{}{lock}: {value}", field.name),
            s,
        );
    }
    ui.spawn(row(s)).with_children(|r| {
        button(r, Action::Previous, "Previous", s);
        label(
            r,
            &format!(
                "Page {} / {}",
                settings.offset / PAGE_SIZE + 1,
                settings.fields.len().div_ceil(PAGE_SIZE).max(1)
            ),
            s,
        );
        button(r, Action::Next, "Next", s);
    });
    if let Some(i) = settings.selected {
        edit_field(ui, settings, i, art, s);
    }
    if settings.page == Some(Page::Startup) {
        startup_controls(ui, settings, s);
    }
    label(ui, &settings.status, s * 0.88);
    ui.spawn(row(s)).with_children(|r| {
        let title = if settings.page == Some(Page::Startup) {
            "Save startup settings"
        } else {
            "Save game settings"
        };
        button(r, Action::Save, title, s);
        button(r, Action::Cancel, "Close / discard unsaved", s);
    });
}
fn edit_field(
    ui: &mut ChildSpawnerCommands,
    settings: &Settings,
    i: usize,
    art: &crate::glue::art::GlueArt,
    s: f32,
) {
    label(
        ui,
        &format!(
            "Edit {}{}",
            settings.fields[i].name,
            if settings.fields[i].locked {
                " (read-only)"
            } else {
                ""
            }
        ),
        s,
    );
    let help=match settings.fields[i].name.as_str() {
        "p2p.psk"=>"Use a private shared passphrase, at least 16 characters. Exact spaces/case matter.",
        "p2p.internet.enabled"=>"true selects QUIC + explicit relays; false selects direct TCP. Clear p2p.peers for Internet mode.",
        "p2p.internet.port_mapping"=>"false is the safe default. true explicitly permits router port mapping after restart.",
        "p2p.encounter.roster"=>"TOML list: [{ id = \"32 hex chars\", address = \"IP:port\" }, ...]. Fixed roster must include every peer.",
        "p2p.encounter.epoch"=>"64 nonzero hex characters, shared freshly for each encounter restart. Internet + encounter is unsupported.",
        "p2p.internet.peer_tickets"=>"TOML list of quoted endpoint tickets, shared privately. Tickets expire when the remote restarts.",
        "p2p.peers" | "p2p.internet.relay_urls"=>"TOML list of quoted addresses, for example [\"address\", \"address\"].",
        "wow.data"=>"The Data folder in your own 1.12.1 install. Empty uses normal discovery; relative paths start beside the executable.",
        _=>"Changes stay in this draft until Save. Boolean startup fields use true or false.",
    };
    label(ui, help, s * 0.78);
    ui.spawn(row(s)).with_children(|r| {
        glue_edit_box(
            r,
            art,
            &Handle::default(),
            (),
            Edit,
            (590.0, 32.0),
            (Color::srgb(0.7, 0.6, 0.3), Color::srgb(0.04, 0.04, 0.04)),
            (8.0, 8.0, 0.0, 0.0),
            s,
        );
        if settings.fields[i].secret {
            button(
                r,
                Action::Reveal,
                if settings.edit.password {
                    "Reveal"
                } else {
                    "Hide"
                },
                s,
            );
        }
    });
}
fn startup_controls(ui: &mut ChildSpawnerCommands, settings: &Settings, s: f32) {
    if settings.config.is_none() {
        button(
            ui,
            Action::Repair,
            "Start blank repair (backs up old config on Save)",
            s,
        );
    }
    ui.spawn(row(s)).with_children(|r| {
        button(r, Action::TestLocal, "Test local TCP", s);
        button(r, Action::TestPeers, "Test peer TCP", s);
        button(r, Action::TestPsk, "Test local PSK", s);
    });
    ui.spawn(row(s)).with_children(|r| {
        button(
            r,
            Action::RevealTicket,
            if settings.ticket_preview.is_some() {
                "Hide ticket preview"
            } else {
                "Reveal ticket preview"
            },
            s,
        );
        button(r, Action::CopyTicket, "Copy current ticket", s);
    });
    if let Some(ticket) = &settings.ticket_preview {
        let preview: String = ticket.chars().take(160).collect();
        label(
            ui,
            &format!("Endpoint ticket preview: {preview}… (Copy for the full current ticket)"),
            s * 0.78,
        );
    }
    label(ui,"Tests run only when clicked. TCP success does not prove PSK, relay or Internet reachability.",s*0.78);
}
