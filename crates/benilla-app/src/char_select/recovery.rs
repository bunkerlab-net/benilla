//! Fork-only recovery with Vanilla dialog art and explicit, snapshotted confirmation.
use super::recovery_model::{Confirmation, Reply, Request};
use crate::glue::{
    art::{GlueArt, GOLD},
    widgets::{glue_button, outlined_text_centered, GlueBtnKind, GlueText},
};
use crate::net::{CharPick, CharRequest};
use bevy::prelude::*;
use std::sync::{mpsc, Mutex};

const PAGE: usize = 5;
#[derive(Resource, Default)]
pub(crate) struct RecoveryPanel {
    pub(crate) open: bool,
    rows: Vec<benilla_profile::Character>,
    selected: Option<benilla_profile::Character>,
    confirmation: Confirmation,
    pending: Option<Mutex<mpsc::Receiver<Reply>>>,
    status: String,
    restoring: bool,
    page: usize,
    root: Option<Entity>,
    scale: f32,
    dirty: bool,
    with_art: bool,
}
impl RecoveryPanel {
    pub(crate) fn open(&mut self, pick: &CharPick) {
        self.open = true;
        self.restoring = false;
        self.rows.clear();
        self.selected = None;
        self.confirmation.0 = None;
        self.page = 0;
        self.status = "Loading deleted characters...".into();
        let (tx, rx) = mpsc::channel();
        self.pending = Some(Mutex::new(rx));
        if pick
            .0
            .send(CharRequest::Recover(Request::List(tx)))
            .is_err()
        {
            self.pending = None;
            self.status = "Local profile is unavailable. Return to login and try again.".into();
        }
        self.dirty = true;
    }
    fn needs_rebuild(&self, scale: f32, with_art: bool) -> bool {
        self.root.is_none() || self.dirty || self.scale != scale || self.with_art != with_art
    }
    fn receive(&mut self, reply: Reply) {
        let restored = self.restoring;
        self.restoring = false;
        self.pending = None;
        self.selected = None;
        self.confirmation.0 = None;
        match reply {
            Ok(rows) => {
                self.status = if restored {
                    "Character restored. It is now available on character select."
                } else if rows.is_empty() {
                    "No deleted characters to recover."
                } else {
                    "Select a deleted character to recover."
                }
                .into();
                self.rows = rows;
                self.page = self.page.min(self.rows.len().saturating_sub(1) / PAGE);
            }
            Err(message) => self.status = message,
        }
        self.dirty = true;
    }
    pub(crate) fn dismiss(&mut self, commands: &mut Commands) {
        if let Some(root) = self.root.take() {
            commands.entity(root).despawn();
        }
        *self = Self::default();
    }
}
#[derive(Component, Clone, Copy)]
pub(crate) enum Action {
    Row(usize),
    Restore,
    Confirm,
    Cancel,
    Close,
    Previous,
    Next,
}

pub(crate) fn drive(
    mut commands: Commands,
    mut panel: ResMut<RecoveryPanel>,
    pick: Res<CharPick>,
    glue: crate::ui_script::glue::StockGlueProbe,
    art: Res<GlueArt>,
    assets: Res<AssetServer>,
    keys: Res<ButtonInput<KeyCode>>,
    clicks: Res<crate::glue::GlueClicks>,
    buttons: Query<(Entity, &Action)>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    if !panel.open {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if panel.confirmation.0.take().is_some() {
            panel.dirty = true;
        } else {
            panel.dismiss(&mut commands);
            return;
        }
    }
    let received = panel
        .pending
        .as_ref()
        .map(|rx| rx.lock().expect("recovery reply lock").try_recv());
    if let Some(result) = received {
        match result {
            Ok(reply) => {
                let refresh = panel.restoring && reply.is_ok();
                panel.receive(reply);
                if refresh {
                    glue.vm().press("GetCharacterListUpdate()");
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                panel.pending = None;
                panel.status = "Local profile closed. Reopen it and inspect the character list before retrying.".into();
                panel.rows.clear();
                panel.selected = None;
                panel.dirty = true;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
    for (entity, action) in &buttons {
        if !clicks.hit(entity) {
            continue;
        }
        match action {
            Action::Close => {
                panel.dismiss(&mut commands);
                return;
            }
            Action::Cancel => {
                panel.confirmation.0 = None;
            }
            _ if panel.pending.is_some() => continue,
            Action::Row(i) if panel.confirmation.0.is_none() => {
                panel.selected = panel.rows.get(*i).cloned();
            }
            Action::Restore => {
                panel.confirmation.0 = panel.selected.clone();
            }
            Action::Confirm => {
                let rows = panel.rows.clone();
                if let Some(target) = panel.confirmation.confirm(&rows) {
                    let (reply, rx) = mpsc::channel();
                    panel.pending = Some(Mutex::new(rx));
                    panel.restoring = true;
                    panel.status = format!("Restoring {}...", target.name);
                    if pick
                        .0
                        .send(CharRequest::Recover(Request::Restore { target, reply }))
                        .is_err()
                    {
                        panel.pending = None;
                        panel.status =
                            "Local profile is unavailable. Reopen it before retrying.".into();
                    }
                }
            }
            Action::Previous => {
                panel.page = panel.page.saturating_sub(1);
            }
            Action::Next if (panel.page + 1) * PAGE < panel.rows.len() => {
                panel.page += 1;
            }
            _ => {}
        }
        panel.dirty = true;
        // One action per frame prevents a stale confirm from being dispatched twice.
        break;
    }
    let s = crate::glue::screen_scale(window.single().ok());
    if !panel.needs_rebuild(s, art.button_up.is_some()) {
        return;
    }
    if let Some(root) = panel.root.take() {
        commands.entity(root).despawn();
    }
    panel.root = Some(spawn(&mut commands, &art, &assets, &panel, s));
    panel.scale = s;
    panel.with_art = art.button_up.is_some();
    panel.dirty = false;
}

fn spawn(
    commands: &mut Commands,
    art: &GlueArt,
    assets: &AssetServer,
    panel: &RecoveryPanel,
    s: f32,
) -> Entity {
    let px = |v: f32| Val::Px(v * s);
    let font = super::wow_font(assets);
    commands.spawn((crate::glue::NativeGlue, GlobalZIndex(1200), Node {
        width: Val::Percent(100.0), height: Val::Percent(100.0),
        align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default()
    })).with_children(|overlay| {
        overlay.spawn(Node { width: px(512.0), height: px(400.0), ..default() }).with_children(|box_ui| {
            if let (Some(bg), Some(border)) = (&art.dialog_bg, &art.dialog_border) {
                box_ui.spawn((crate::glue::backdrop::tiled_bg_node(bg.clone(), 32.0, s, Color::WHITE), Node {
                    position_type: PositionType::Absolute, left: px(11.0), right: px(12.0), top: px(12.0), bottom: px(11.0), ..default()
                }));
                crate::glue::backdrop::backdrop_border(box_ui, border, 32.0, Color::WHITE);
            } else {
                box_ui.spawn((BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.98)), crate::glue::widgets::overlay()));
            }
            let label = |ui: &mut ChildSpawnerCommands, text: &str, top: f32, size: f32| {
                outlined_text_centered(ui, Node { position_type: PositionType::Absolute,
                    top: px(top), left: px(30.0), right: px(30.0), justify_content: JustifyContent::Center, ..default()
                }, (), (), GlueText { text, size, color: GOLD, wrap: true }, &font, s);
            };
            label(box_ui, "Recover Characters", 22.0, 18.0);
            label(box_ui, &panel.status, 55.0, 12.0);
            if let Some(target) = &panel.confirmation.0 {
                label(box_ui, &format!("Restore {} (Level {} {})?", target.name, target.level, super::class_name(target.class)), 145.0, 18.0);
                label(box_ui, "The original character and saved progress will be kept. No active character will be replaced.", 205.0, 12.0);
            } else if panel.pending.is_none() {
                box_ui.spawn(Node { position_type: PositionType::Absolute, top: px(110.0), left: px(40.0), flex_direction: FlexDirection::Column, ..default() }).with_children(|list| {
                    for (i, character) in panel.rows.iter().enumerate().skip(panel.page * PAGE).take(PAGE) {
                        let selected = panel.selected.as_ref() == Some(character);
                        glue_button(list, art, &font, Action::Row(i),
                            &format!("{}{} - Level {} {}", if selected { "> " } else { "" }, character.name, character.level, super::class_name(character.class)),
                            432.0, 32.0, GlueBtnKind::Small, s);
                    }
                });
            }
            box_ui.spawn(Node { position_type: PositionType::Absolute, bottom: px(65.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() }).with_children(|row| {
                if panel.confirmation.0.is_none() && panel.pending.is_none() && panel.rows.len() > PAGE {
                    if panel.page > 0 { glue_button(row, art, &font, Action::Previous, "Previous", 140.0, 32.0, GlueBtnKind::Small, s); }
                    if (panel.page + 1) * PAGE < panel.rows.len() { glue_button(row, art, &font, Action::Next, "Next", 140.0, 32.0, GlueBtnKind::Small, s); }
                }
            });
            box_ui.spawn(Node { position_type: PositionType::Absolute, bottom: px(16.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, column_gap: px(13.0), ..default() }).with_children(|row| {
                if panel.confirmation.0.is_some() {
                    glue_button(row, art, &font, Action::Confirm, "Restore", 200.0, 40.0, GlueBtnKind::Dialog, s);
                    glue_button(row, art, &font, Action::Cancel, "Cancel", 200.0, 40.0, GlueBtnKind::Dialog, s);
                } else {
                    if panel.selected.is_some() && panel.pending.is_none() { glue_button(row, art, &font, Action::Restore, "Recover", 200.0, 40.0, GlueBtnKind::Dialog, s); }
                    glue_button(row, art, &font, Action::Close, "Close", 200.0, 40.0, GlueBtnKind::Dialog, s);
                }
            });
        });
    }).id()
}

/// No native recovery panel survives entering the world.
pub(crate) fn close(mut commands: Commands, mut panel: ResMut<RecoveryPanel>) {
    panel.dismiss(&mut commands);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn redraw_tracks_resize_and_art_arrival() {
        let mut panel = RecoveryPanel {
            root: Some(Entity::PLACEHOLDER),
            scale: 1.0,
            ..default()
        };
        assert!(!panel.needs_rebuild(1.0, false));
        assert!(panel.needs_rebuild(2.0, false));
        assert!(panel.needs_rebuild(1.0, true));
        panel.dirty = true;
        assert!(panel.needs_rebuild(1.0, false));
    }

    #[test]
    fn close_and_reopen_discard_pending_replies_and_old_status() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let pick = CharPick(tx.into());
        let mut app = App::new();
        let mut panel = RecoveryPanel::default();
        panel.open(&pick);
        let CharRequest::Recover(Request::List(first)) = rx.recv().unwrap() else {
            panic!("list request")
        };
        panel.receive(Err("Synthetic name conflict".into()));
        assert_eq!(panel.status, "Synthetic name conflict");
        assert!(panel.pending.is_none());
        assert!(panel.selected.is_none());
        app.insert_resource(panel);
        app.world_mut()
            .run_system_once(|mut commands: Commands, mut panel: ResMut<RecoveryPanel>| {
                panel.dismiss(&mut commands)
            })
            .unwrap();
        let mut panel = app.world_mut().resource_mut::<RecoveryPanel>();
        assert!(!panel.open);
        assert!(first.send(Ok(Vec::new())).is_err());
        panel.open(&pick);
        assert!(panel.open && panel.pending.is_some());
        assert_eq!(panel.status, "Loading deleted characters...");
        assert!(panel.confirmation.0.is_none());
        let CharRequest::Recover(Request::List(second)) = rx.recv().unwrap() else {
            panic!("fresh list request")
        };
        second.send(Ok(Vec::new())).unwrap();
    }

    #[test]
    fn success_and_error_resolve_pending_confirmation() {
        let mut panel = RecoveryPanel {
            restoring: true,
            ..default()
        };
        panel.receive(Ok(Vec::new()));
        assert!(panel.status.starts_with("Character restored"));
        assert!(!panel.restoring);
        assert!(panel.confirmation.0.is_none());
        panel.receive(Err("The roster is full".into()));
        assert_eq!(panel.status, "The roster is full");
        assert!(panel.dirty);
    }
}
