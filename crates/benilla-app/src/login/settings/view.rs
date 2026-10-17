use super::*;
use crate::glue::{
    art::{tc_rect, GlueArt, GOLD, REALM_TAB_TC},
    backdrop::{backdrop_border, tiled_bg_node},
    widgets::{abs, glue_button, glue_edit_box, overlay, GlueBtnKind, Hilight, LockHighlight},
};
use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    ui::RelativeCursorPosition,
};

#[derive(Component)]
pub(in crate::login) struct Root;
#[derive(Component, Clone)]
pub(in crate::login) struct Edit;
#[derive(Component, Default)]
pub(in crate::login) struct ScrollArea(f32);
#[derive(Component)]
struct Footer;

// Match the stock glue edit-box tint; the tooltip texture is a neutral tint mask.
const BOX_BORDER: Color = Color::srgb(0.8, 0.8, 0.8);
const BOX_FILL: Color = Color::srgb(0.09, 0.09, 0.09);

#[derive(PartialEq)]
pub(in crate::login) struct StyleKey {
    scale: f32,
    art: [bool; 10],
    font: Handle<Font>,
    edit_font: Handle<Font>,
}

struct Skin<'a> {
    art: &'a GlueArt,
    font: Handle<Font>,
    edit_font: Handle<Font>,
    s: f32,
    border_scale: f32,
}
impl Skin<'_> {
    fn text(
        &self,
        ui: &mut ChildSpawnerCommands,
        text: impl Into<String>,
        size: f32,
        color: Color,
        node: Node,
    ) {
        ui.spawn((
            Text::new(text),
            TextFont {
                font: self.font.clone(),
                font_size: size * self.s,
                ..default()
            },
            TextColor(color),
            TextShadow {
                offset: Vec2::splat(self.s),
                color: Color::BLACK,
            },
            bevy::text::LineHeight::RelativeToFont(1.15),
            node,
        ));
    }
    fn button(&self, ui: &mut ChildSpawnerCommands, action: Action, text: &str, w: f32) {
        glue_button(
            ui,
            self.art,
            &self.font,
            action,
            text,
            w,
            34.0,
            GlueBtnKind::Small,
            self.s,
        );
    }
    fn frame(&self, ui: &mut ChildSpawnerCommands, dialog: bool) {
        let (bg, border, edge, fill, border_color, insets) = if dialog {
            // GlueDialog's stock background insets and 32-pixel edge.
            (
                &self.art.dialog_bg,
                &self.art.dialog_border,
                32.0,
                Color::WHITE,
                Color::WHITE,
                [11.0, 12.0, 12.0, 11.0],
            )
        } else {
            (
                &self.art.tooltip_bg,
                &self.art.name_border,
                16.0,
                BOX_FILL,
                BOX_BORDER,
                [6.0; 4],
            )
        };
        if let (Some(bg), Some(border)) = (bg, border) {
            let [left, right, top, bottom] = insets.map(|v| Val::Px(v * self.s));
            ui.spawn((
                tiled_bg_node(bg.clone(), edge, self.s, fill),
                Node {
                    position_type: PositionType::Absolute,
                    left,
                    right,
                    top,
                    bottom,
                    ..default()
                },
            ));
            // The shared fitter multiplies by the global glue scale; this dialog also fits width.
            backdrop_border(ui, border, edge * self.border_scale, border_color);
        } else {
            ui.spawn((
                BackgroundColor(Color::srgba(0.04, 0.04, 0.055, 0.97)),
                overlay(),
            ));
        }
    }
    fn scroll_text(&self, ui: &mut ChildSpawnerCommands, text: &str, rect: [f32; 4]) {
        let [x, y, w, h] = rect;
        ui.spawn((
            ScrollArea::default(),
            RelativeCursorPosition::default(),
            ScrollPosition::default(),
            Node {
                overflow: Overflow::scroll_y(),
                flex_direction: FlexDirection::Column,
                ..abs(self.s, x, y, w, h)
            },
        ))
        .with_children(|inner| {
            self.text(
                inner,
                text,
                13.0,
                Color::WHITE,
                Node {
                    width: Val::Percent(100.0),
                    flex_shrink: 0.0,
                    ..default()
                },
            );
        });
    }
}

// Do not assign an unresolved MPQ font: recovery must remain legible with no/broken Data.
fn loaded_font(
    assets: Option<&AssetServer>,
    fonts: Option<&Assets<Font>>,
    path: &'static str,
) -> Handle<Font> {
    assets
        .and_then(|a| a.get_handle::<Font>(path))
        .filter(|h| fonts.is_some_and(|f| f.contains(h.id())))
        .unwrap_or_default()
}

#[allow(clippy::type_complexity)]
pub(in crate::login) fn render(
    mut commands: Commands,
    mut settings: ResMut<Settings>,
    roots: Query<Entity, With<Root>>,
    art: Res<GlueArt>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    assets: Option<Res<AssetServer>>,
    fonts: Option<Res<Assets<Font>>>,
    mut last_style: Local<Option<StyleKey>>,
    mut wheel: MessageReader<MouseWheel>,
    // Own only this dialog's offset; applying it through Commands avoids adding a shared
    // ScrollPosition writer that conflicts with character creation's independent scrollers.
    mut scrolls: Query<(
        Entity,
        &RelativeCursorPosition,
        &ComputedNode,
        &mut ScrollArea,
    )>,
) {
    for ev in wheel.read() {
        if !settings.open {
            continue;
        }
        let delta = if ev.unit == MouseScrollUnit::Line {
            ev.y * 24.0
        } else {
            ev.y
        };
        for (entity, cursor, node, mut pos) in &mut scrolls {
            if cursor.cursor_over() {
                pos.0 = layout::scroll(
                    pos.0,
                    delta,
                    node.content_size.y * node.inverse_scale_factor,
                    node.size.y * node.inverse_scale_factor,
                );
                commands
                    .entity(entity)
                    .insert(ScrollPosition(Vec2::new(0.0, pos.0)));
            }
        }
    }
    if !settings.open {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }
    let window = windows.single().ok();
    let s = window
        .map(|w| layout::scale(w.width(), w.height()))
        .unwrap_or(1.0);
    let font = loaded_font(
        assets.as_deref(),
        fonts.as_deref(),
        "mpq://Fonts/FRIZQT__.ttf",
    );
    let edit_font = loaded_font(
        assets.as_deref(),
        fonts.as_deref(),
        "mpq://Fonts/ARIALN.ttf",
    );
    let style = StyleKey {
        scale: s,
        art: [
            art.dialog_bg.is_some(),
            art.dialog_border.is_some(),
            art.name_border.is_some(),
            art.tooltip_bg.is_some(),
            art.button_up.is_some(),
            art.button_hi.is_some(),
            art.select_highlight.is_some(),
            art.tab_active.is_some(),
            art.dialog_header.is_some(),
            art.title_highlight.is_some(),
        ],
        font: font.clone(),
        edit_font: edit_font.clone(),
    };
    if !settings.refresh && last_style.as_ref() == Some(&style) && !roots.is_empty() {
        return;
    }
    settings.refresh = false;
    *last_style = Some(style);
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    let skin = Skin {
        art: &art,
        font,
        edit_font,
        s,
        border_scale: s / crate::glue::screen_scale(window).max(f32::EPSILON),
    };
    commands
        .spawn((
            Root,
            GlobalZIndex(3000),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        ))
        .with_children(|root| {
            root.spawn(Node {
                width: Val::Px(layout::WIDTH * s),
                height: Val::Px(layout::HEIGHT * s),
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|ui| {
                // Keep the underlying login text out of the translucent stock backdrop.
                ui.spawn((
                    BackgroundColor(Color::BLACK),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(12.0 * s),
                        right: Val::Px(12.0 * s),
                        top: Val::Px(12.0 * s),
                        bottom: Val::Px(12.0 * s),
                        ..default()
                    },
                ));
                skin.frame(ui, true);
                content(ui, &settings, &skin);
            });
        });
}

fn content(ui: &mut ChildSpawnerCommands, settings: &Settings, skin: &Skin) {
    let s = skin.s;
    // Realm selection uses this 256×64 stock plaque with a 12-point caption 14 below its top.
    if let Some((header, _)) = &skin.art.dialog_header {
        ui.spawn((
            ImageNode::new(header.clone()),
            abs(s, (layout::WIDTH - 256.0) / 2.0, -12.0, 256.0, 64.0),
        ));
    }
    ui.spawn(Node {
        justify_content: JustifyContent::Center,
        ..abs(s, (layout::WIDTH - 256.0) / 2.0, 2.0, 256.0, 20.0)
    })
    .with_children(|title| skin.text(title, "Connection setup", 12.0, GOLD, Node::default()));
    skin.text(
        ui,
        "Executable config.toml • Save requires restart • Passphrase is stored locally",
        13.0,
        Color::WHITE,
        abs(s, 28.0, 57.0, 864.0, 20.0),
    );
    skin.text(ui, "Short or empty passphrases are allowed. Empty gives no privacy; short phrases are easy to guess.",
        12.0, GOLD, abs(s, 28.0, 82.0, 864.0, 26.0));
    let [x, y, w, h] = layout::LIST;
    ui.spawn(abs(s, x, y, w, h)).with_children(|list| {
        skin.frame(list, false);
        field_list(list, settings, skin);
    });
    let [x, y, w, h] = layout::EDITOR;
    ui.spawn(abs(s, x, y, w, h)).with_children(|editor| {
        skin.frame(editor, false);
        if let Some(i) = settings.selected { edit_field(editor, settings, i, skin); }
        else {
            skin.text(editor, "Select a startup setting", 16.0, GOLD, abs(s, 16.0, 18.0, 400.0, 26.0));
            skin.text(editor, "Select a row to edit its value. Changes remain in this draft until Save; closing discards them.",
                14.0, Color::WHITE, abs(s, 16.0, 58.0, 400.0, 70.0));
        }
        if settings.config.is_none() {
            editor.spawn(abs(s, 16.0, 198.0, 400.0, 34.0)).with_children(|r| {
                skin.button(r, Action::Repair, "Start blank repair (backup on Save)", 400.0);
            });
        }
        editor.spawn(abs(s, 12.0, 240.0, 408.0, 34.0)).with_children(|r| {
            skin.button(r, Action::RevealTicket, if settings.ticket_preview.is_some() { "Hide ticket preview" } else { "Reveal ticket preview" }, 204.0);
            skin.button(r, Action::CopyTicket, "Copy current ticket", 204.0);
        });
        if let Some(ticket) = &settings.ticket_preview {
            let preview: String = ticket.chars().take(160).collect();
            skin.scroll_text(editor, &format!("Endpoint ticket preview: {preview}…\nCopy for the full current ticket. Scroll for more."), [16.0, 284.0, 400.0, 50.0]);
        } else {
            skin.text(editor, "Share endpoint tickets privately. They contain network addresses and change on reconnect.",
                12.0, Color::WHITE, abs(s, 16.0, 284.0, 400.0, 50.0));
        }
    });
    let [x, y, w, h] = layout::TESTS;
    ui.spawn(abs(s, x, y, w, h)).with_children(|r| {
        skin.button(r, Action::TestLocal, "Test local TCP", 160.0);
        skin.button(r, Action::TestPeers, "Test peer TCP", 160.0);
        skin.button(r, Action::TestPsk, "Test local PSK", 160.0);
        skin.text(
            r,
            "Tests run only when clicked",
            12.0,
            GOLD,
            Node {
                margin: UiRect::left(Val::Px(24.0 * s)),
                align_self: AlignSelf::Center,
                ..default()
            },
        );
    });
    skin.text(ui, "TCP success does not prove PSK, relay or Internet reachability. Status below: scroll for more.",
        12.0, GOLD, abs(s, 28.0, 510.0, 864.0, 22.0));
    let [x, y, w, h] = layout::STATUS;
    ui.spawn(abs(s, x, y, w, h)).with_children(|status| {
        skin.frame(status, false);
        skin.scroll_text(status, &settings.status, [12.0, 9.0, w - 24.0, h - 18.0]);
    });
    let [x, y, w, h] = layout::FOOTER;
    ui.spawn((
        Footer,
        Node {
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(16.0 * s),
            ..abs(s, x, y, w, h)
        },
    ))
    .with_children(|r| {
        skin.button(r, Action::Save, "Save startup settings", 240.0);
        skin.button(r, Action::Cancel, "Close / discard unsaved", 260.0);
    });
}

fn field_list(ui: &mut ChildSpawnerCommands, settings: &Settings, skin: &Skin) {
    let s = skin.s;
    for (slot, (i, field)) in settings
        .fields
        .iter()
        .enumerate()
        .skip(settings.offset)
        .take(PAGE_SIZE)
        .enumerate()
    {
        let selected = settings.selected == Some(i);
        ui.spawn((
            Button,
            Action::Select(i),
            LockHighlight(selected),
            abs(
                s,
                12.0,
                layout::FIELD_TOP + slot as f32 * layout::FIELD_HEIGHT,
                406.0,
                layout::FIELD_HEIGHT,
            ),
        ))
        .with_children(|r| {
            if selected {
                if let Some(image) = &skin.art.title_highlight {
                    r.spawn((
                        ImageNode {
                            image: image.clone(),
                            color: Color::srgb(0.9, 0.65, 0.1),
                            ..default()
                        },
                        overlay(),
                    ));
                } else {
                    r.spawn((BackgroundColor(Color::srgb(0.22, 0.16, 0.06)), overlay()));
                }
            }
            if let Some(hi) = &skin.art.select_highlight {
                r.spawn((
                    Hilight,
                    Visibility::Hidden,
                    bevy::ui_render::ui_material::MaterialNode(hi.clone()),
                    overlay(),
                ));
            }
            let lock = if field.locked { " [read-only]" } else { "" };
            skin.text(
                r,
                format!("{}{lock}", field.name),
                13.0,
                GOLD,
                abs(
                    s,
                    layout::FIELD_TEXT_LEFT,
                    layout::FIELD_TEXT_TOP,
                    layout::FIELD_TEXT_WIDTH,
                    layout::FIELD_LABEL_HEIGHT,
                ),
            );
            let value = if field.secret {
                if field.value.is_empty() {
                    "(empty; no privacy)".to_owned()
                } else {
                    "********".to_owned()
                }
            } else {
                let mut value: String = field.value.chars().take(46).collect();
                if field.value.chars().count() > 46 {
                    value.push('…');
                }
                value
            };
            skin.text(
                r,
                value,
                12.0,
                Color::WHITE,
                Node {
                    overflow: Overflow::clip(),
                    ..abs(
                        s,
                        layout::FIELD_TEXT_LEFT,
                        layout::FIELD_TEXT_TOP + layout::FIELD_LABEL_HEIGHT,
                        layout::FIELD_TEXT_WIDTH,
                        layout::FIELD_VALUE_HEIGHT,
                    )
                },
            );
        });
    }
    ui.spawn(abs(s, 12.0, layout::PAGE_TOP, 406.0, 34.0))
        .with_children(|r| {
            skin.button(r, Action::Previous, "Previous", 128.0);
            r.spawn(Node {
                width: Val::Px(150.0 * s),
                height: Val::Px(34.0 * s),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|page| {
                // The active realm-tab's three pieces keep the stock end caps intact.
                if let Some((sheet, size)) = &skin.art.tab_active {
                    for (part, tc) in REALM_TAB_TC.iter().enumerate() {
                        let (left, width) = [(0.0, 20.0), (20.0, 110.0), (130.0, 20.0)][part];
                        page.spawn((
                            ImageNode {
                                image: sheet.clone(),
                                rect: Some(tc_rect(*size, *tc)),
                                ..default()
                            },
                            abs(s, left, 0.0, width, 32.0),
                        ));
                    }
                }
                skin.text(
                    page,
                    format!(
                        "Page {} / {}",
                        settings.offset / PAGE_SIZE + 1,
                        settings.fields.len().div_ceil(PAGE_SIZE).max(1)
                    ),
                    12.0,
                    GOLD,
                    Node::default(),
                );
            });
            skin.button(r, Action::Next, "Next", 128.0);
        });
}

fn edit_field(ui: &mut ChildSpawnerCommands, settings: &Settings, i: usize, skin: &Skin) {
    let s = skin.s;
    skin.text(
        ui,
        format!(
            "Edit {}{}",
            settings.fields[i].name,
            if settings.fields[i].locked {
                " (read-only)"
            } else {
                ""
            }
        ),
        15.0,
        GOLD,
        abs(s, 16.0, 14.0, 400.0, 36.0),
    );
    let help = field_help(&settings.fields[i].name).expect("startup field has help");
    skin.text(
        ui,
        help,
        13.0,
        Color::WHITE,
        abs(s, 16.0, 54.0, 400.0, 64.0),
    );
    ui.spawn(Node {
        overflow: Overflow::clip(),
        ..abs(s, 12.0, 122.0, 408.0, 36.0)
    })
    .with_children(|r| {
        glue_edit_box(
            r,
            skin.art,
            &skin.edit_font,
            (),
            Edit,
            (408.0, 36.0),
            (BOX_BORDER, BOX_FILL),
            (12.0, 12.0, 0.0, 5.0),
            s,
        );
    });
    if settings.fields[i].secret {
        ui.spawn(abs(s, 12.0, 160.0, 120.0, 34.0))
            .with_children(|r| {
                skin.button(
                    r,
                    Action::Reveal,
                    if settings.edit.password {
                        "Reveal"
                    } else {
                        "Hide"
                    },
                    120.0,
                );
            });
    }
}

// Keep startup explanations here so every listed field has a specific, testable description.
fn field_help(name: &str) -> Option<&'static str> {
    Some(match name {
        "p2p.enabled" => {
            "true enables P2P exploration after restart; false removes the P2P config and uses realm login. Peers need matching network, passphrase and ruleset."
        }
        "p2p.psk" => {
            "Shared passphrase, up to 1024 UTF-8 bytes. Exact spaces/case matter. Empty gives no privacy; use random words. This authenticates peers, but does not find them."
        }
        "p2p.network" => {
            "Shared group name; all peers must match. Use 1-64 ASCII letters, digits, hyphens, underscores or dots. This is not an address or a peer-discovery service."
        }
        "p2p.listen" => {
            "Local bind IP:port, e.g. 0.0.0.0:7474 (all IPv4 interfaces). Uses TCP normally or QUIC/UDP in iroh mode. Choose a nonzero port; this is not a remote peer address."
        }
        "p2p.peers" => {
            "Direct TCP peers as a TOML list, e.g. [\"192.168.1.20:7474\"]. Empty listens without dialing peers. Leave [] for iroh mode or a fixed encounter roster."
        }
        "p2p.max_peers" => {
            "Maximum peer connections, 1-64 (default 16). Must cover the configured peers or tickets; for encounters, allow at least roster size minus one."
        }
        "p2p.timeout_ms" => {
            "Peer handshake timeout in milliseconds, 100-60000 (default 5000 = 5 seconds). Increase for slow connections; it does not change the game tick rate."
        }
        "wow.data" => {
            "The Data folder in your own 1.12.1 install. Empty uses normal discovery; relative paths start beside the executable."
        }
        "p2p.classic_start_captures" => {
            "Optional private source-v2 start-capture directory. Empty uses built-in racial starts. Bad or missing configured captures block new creation only; saved positions stay unchanged."
        }
        "ruleset.xp_rate" => {
            "XP multiplier, 0.001-1000 with up to 3 decimal places (1.0 = normal). Peers must match. Ordinary P2P exploration does not implement XP progression."
        }
        "ruleset.drop_rate" => {
            "Drop-chance multiplier, 0.001-1000, up to 3 decimals (1.0 = normal). Chance caps at 100%; item quantity is unchanged. Peers must match. P2P exploration has no loot progression."
        }
        "p2p.iroh.enabled" => {
            "true opts into iroh QUIC; false uses TCP. Clear p2p.peers and exchange endpoint tickets manually. A shared passphrase alone does not discover friends. Cannot combine with encounters."
        }
        "p2p.iroh.relay_urls" => {
            "TOML list of allowed HTTPS relay URLs. [] means direct QUIC only, with no relay fallback or hole-punch assistance. No public relay is added automatically."
        }
        "p2p.iroh.peer_tickets" => {
            "TOML list of quoted endpoint tickets shared privately. They contain addresses, not the passphrase. Refresh after a peer restarts; ticket relays must be in relay_urls."
        }
        "p2p.iroh.port_mapping" => {
            "false disables router mapping (default). true permits UPnP/PCP/NAT-PMP mapping for iroh mode after restart. Router support and permission are still required."
        }
        "p2p.encounter.enabled" => {
            "true enables synthetic chat combat tests with a fixed 2-16-player TCP roster and shared epoch. iroh mode is incompatible. Leave false for ordinary exploration."
        }
        "p2p.encounter.local_id" => {
            "Your public ID in the fixed encounter roster: exactly 32 hex characters, not all zero. Must match one roster entry; every player needs a unique ID."
        }
        "p2p.encounter.epoch" => {
            "Shared encounter session ID: exactly 64 hex characters, not all zero. All roster members must use the same fresh value for each encounter restart."
        }
        "p2p.encounter.tick_limit" => {
            "Synthetic encounter simulation tick limit, 1-100000 (default 1200). All roster members must agree. This bounds the test; it is not a network timeout."
        }
        "p2p.encounter.roster" => {
            "TOML list: [{ id = \"32 hex chars\", address = \"IP:port\" }, ...]. Include all 2-16 players with unique IDs/addresses. Your entry must match local_id and listen port."
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
