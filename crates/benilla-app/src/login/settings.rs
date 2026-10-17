//! Connection setup: executable-adjacent startup config and explicit diagnostics.
//! Draft edits do nothing until Save; validated connection changes have a separate apply lifecycle.
mod diagnostics;
mod layout;
#[cfg(test)]
mod tests;
mod view;
use crate::{
    cvars::Cvars,
    textinput::{self, HostClipboard},
};
use benilla_config::editor::ConfigEditor;
use benilla_ui::widget::EditBoxState;
use bevy::{input::keyboard::KeyboardInput, prelude::*};
use std::collections::BTreeMap;

#[derive(Resource)]
pub(crate) struct ConfigRecovery(pub String);
#[derive(Clone)]
struct Field {
    name: String,
    value: String,
    saved_value: String,
    secret: bool,
    locked: bool,
}
#[derive(Resource, Default)]
pub(crate) struct Settings {
    pub(super) open: bool,
    fields: Vec<Field>,
    group: FieldGroup,
    group_selection: [Option<usize>; 4],
    apply_status: Option<crate::net::reconfigure::ApplyStatus>,
    data_restart_required: bool,
    selected: Option<usize>,
    pub(super) edit: EditBoxState,
    changes: BTreeMap<String, String>,
    config: Option<ConfigEditor>,
    status: String,
    refresh: bool,
    recovery: bool,
    ticket_preview: Option<String>,
    diagnostic: Option<std::sync::Mutex<std::sync::mpsc::Receiver<String>>>,
}
#[derive(Component, Clone, Copy)]
pub(super) enum Action {
    Select(usize),
    Group(FieldGroup),
    ToggleBoolean(usize),
    ConfirmApply,
    KeepConnection,
    Save,
    Cancel,
    Reveal,
    TestLocal,
    TestPeers,
    TestPsk,
    Repair,
    RevealTicket,
    CopyTicket,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(usize)]
pub(super) enum FieldGroup {
    #[default]
    Connection,
    Iroh,
    World,
    Encounter,
}
impl FieldGroup {
    const ALL: [Self; 4] = [Self::Connection, Self::Iroh, Self::World, Self::Encounter];
    fn label(self) -> &'static str {
        match self {
            Self::Connection => "Connection",
            Self::Iroh => "Iroh",
            Self::World => "World",
            Self::Encounter => "Encounter",
        }
    }
    fn for_field(name: &str) -> Self {
        if name.starts_with("p2p.iroh.") {
            Self::Iroh
        } else if name.starts_with("p2p.encounter.") {
            Self::Encounter
        } else if name.starts_with("ruleset.")
            || name == "wow.data"
            || name == "p2p.classic_start_captures"
        {
            Self::World
        } else {
            Self::Connection
        }
    }
}
fn is_boolean(name: &str) -> bool {
    matches!(
        name,
        "p2p.enabled" | "p2p.iroh.enabled" | "p2p.iroh.port_mapping" | "p2p.encounter.enabled"
    )
}
const STARTUP: &[(&str, &str, bool)] = &[
    ("p2p.enabled", "false", false),
    ("p2p.psk", "", true),
    ("p2p.network", "benilla", false),
    ("p2p.listen", "0.0.0.0:7474", false),
    ("p2p.peers", "[]", false),
    ("p2p.max_peers", "16", false),
    ("p2p.timeout_ms", "5000", false),
    ("wow.data", "", false),
    ("p2p.classic_start_captures", "", false),
    ("ruleset.xp_rate", "1.0", false),
    ("ruleset.drop_rate", "1.0", false),
    ("p2p.iroh.enabled", "false", false),
    ("p2p.iroh.relay_urls", "[]", false),
    ("p2p.iroh.peer_tickets", "[]", false),
    ("p2p.iroh.port_mapping", "false", false),
    ("p2p.encounter.enabled", "false", false),
    ("p2p.encounter.local_id", "", false),
    ("p2p.encounter.epoch", "", false),
    ("p2p.encounter.tick_limit", "1200", false),
    ("p2p.encounter.roster", "[]", false),
];
impl Settings {
    pub(super) fn open(&mut self, cvars: &Cvars, recovery: Option<&ConfigRecovery>) {
        *self = Self {
            open: true,
            ..default()
        };
        let result = std::env::current_exe()
            .map_err(|_| "Cannot locate executable".to_owned())
            .and_then(|exe| {
                ConfigEditor::open(&exe.with_file_name("config.toml")).map_err(|e| e.to_string())
            });
        match result {
            Ok(editor) => self.config = Some(editor),
            Err(error) => self.status = error,
        }
        self.recovery = recovery.is_some();
        if let Some(error) = recovery {
            self.status = format!(
                "Networking disabled: {}. Save startup settings, then restart.",
                error.0
            );
        }
        self.refresh_fields(cvars);
    }
    fn has_unsaved_changes(&self) -> bool {
        self.fields.iter().enumerate().any(|(index, field)| {
            let value = if self.selected == Some(index) {
                &self.edit.text
            } else {
                &field.value
            };
            !field.locked && value != &field.saved_value
        })
    }
    fn sync_ticket_preview(&mut self, current: Option<&str>) {
        if self.ticket_preview.is_some() && self.ticket_preview.as_deref() != current {
            self.ticket_preview = None;
            self.refresh = true;
        }
    }
    fn keep_edit(&mut self) {
        if let Some(i) = self.selected {
            if !self.fields[i].locked {
                self.fields[i].value.clone_from(&self.edit.text);
                if self.edit.text == self.fields[i].saved_value {
                    self.changes.remove(&self.fields[i].name);
                } else {
                    self.changes
                        .insert(self.fields[i].name.clone(), self.edit.text.clone());
                }
            }
        }
    }
    fn refresh_fields(&mut self, _cvars: &Cvars) {
        self.keep_edit();
        self.selected = None;
        self.edit = EditBoxState::default();

        self.fields = STARTUP
            .iter()
            .map(|(name, default, secret)| {
                let (section, key) = name.rsplit_once('.').unwrap();
                let saved_value = self
                    .config
                    .as_ref()
                    .and_then(|c| {
                        if *name == "p2p.encounter.enabled" {
                            Some(c.has_section("p2p.encounter").to_string())
                        } else if *name == "p2p.iroh.enabled" {
                            Some(c.has_section("p2p.iroh").to_string())
                        } else if *name == "p2p.enabled" {
                            Some(c.has_section("p2p").to_string())
                        } else {
                            c.get(section, key)
                        }
                    })
                    .unwrap_or_else(|| (*default).into());
                let value = self
                    .changes
                    .get(*name)
                    .cloned()
                    .unwrap_or_else(|| saved_value.clone());
                Field {
                    name: (*name).into(),
                    value,
                    saved_value,
                    secret: *secret,
                    locked: self.config.is_none(),
                }
            })
            .collect();
        self.refresh = true;
    }
    fn select(&mut self, index: usize) {
        if index >= self.fields.len() {
            return;
        }
        if self.selected == Some(index) {
            return;
        }
        self.keep_edit();
        self.group = FieldGroup::for_field(&self.fields[index].name);
        self.group_selection[self.group as usize] = Some(index);
        self.selected = Some(index);
        self.edit = EditBoxState::default();
        self.edit.max_bytes = Some(4096);
        self.edit.password = self.fields[index].secret;
        self.edit.set_text(&self.fields[index].value);
        self.refresh = true;
    }
    fn select_group(&mut self, group: FieldGroup) {
        if self.group == group {
            return;
        }
        self.keep_edit();
        self.group = group;
        self.selected = None;
        self.edit = EditBoxState::default();
        let selected = self.group_selection[group as usize].or_else(|| {
            self.fields
                .iter()
                .position(|field| FieldGroup::for_field(&field.name) == group)
        });
        if let Some(index) = selected {
            self.select(index);
        }
        self.refresh = true;
    }
    fn toggle_boolean(&mut self, index: usize) {
        if index >= self.fields.len()
            || self.fields[index].locked
            || !is_boolean(&self.fields[index].name)
        {
            return;
        }
        self.select(index);
        let value = if self.edit.text == "true" {
            "false"
        } else {
            "true"
        };
        self.edit.set_text(value);
        self.keep_edit();
        self.refresh = true;
    }
    fn prepare_config(&mut self) -> Result<ConfigEditor, String> {
        self.keep_edit();
        let mut editor = self.config.as_ref().ok_or(
            "Startup config could not be opened; fix its syntax/permissions and reopen Settings",
        )?.clone();
        let enabled = self
            .changes
            .get("p2p.enabled")
            .map(String::as_str)
            .map(|v| match v {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err("p2p.enabled must be true or false"),
            })
            .transpose()?
            .unwrap_or_else(|| editor.has_section("p2p"));
        let internet = self
            .changes
            .get("p2p.iroh.enabled")
            .map(String::as_str)
            .map(|v| match v {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err("p2p.iroh.enabled must be true or false"),
            })
            .transpose()?
            .unwrap_or_else(|| editor.has_section("p2p.iroh"));
        let encounter = self
            .changes
            .get("p2p.encounter.enabled")
            .map(String::as_str)
            .map(|v| match v {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err("p2p.encounter.enabled must be true or false"),
            })
            .transpose()?
            .unwrap_or_else(|| editor.has_section("p2p.encounter"));
        for (name, value) in &self.changes {
            let Some((section, key)) = name.rsplit_once('.') else {
                continue;
            };
            if name == "p2p.enabled"
                || name == "p2p.iroh.enabled"
                || name == "p2p.encounter.enabled"
                || (section.starts_with("p2p") && !enabled)
                || (section == "p2p.iroh" && !internet)
                || (section == "p2p.encounter" && !encounter)
            {
                continue;
            }
            if (section == "wow" || name == "p2p.classic_start_captures") && value.is_empty() {
                editor.remove(section, key);
            } else if matches!(
                name.as_str(),
                "p2p.psk"
                    | "p2p.network"
                    | "p2p.listen"
                    | "wow.data"
                    | "p2p.classic_start_captures"
                    | "p2p.encounter.local_id"
                    | "p2p.encounter.epoch"
            ) {
                editor.set_string(section, key, value);
            } else {
                editor
                    .set_literal(section, key, value)
                    .map_err(|e| e.to_string())?;
            }
        }
        if internet && enabled && !editor.has_section("p2p.iroh") {
            editor
                .set_literal("p2p.iroh", "port_mapping", "false")
                .map_err(|e| e.to_string())?;
        }
        if encounter && enabled && !editor.has_section("p2p.encounter") {
            editor
                .set_literal("p2p.encounter", "tick_limit", "1200")
                .map_err(|e| e.to_string())?;
        }
        if !encounter {
            editor.remove_section("p2p.encounter");
        }
        if !internet {
            editor.remove_section("p2p.iroh");
        }
        if !enabled {
            editor.remove_section("p2p");
        }
        if enabled && editor.get("p2p", "psk").is_none() {
            editor.set_string("p2p", "psk", "");
        }
        Ok(editor)
    }
    fn save(&mut self, cvars: &mut Cvars) -> Result<benilla_config::AppConfig, String> {
        self.keep_edit();
        let mut editor = self.prepare_config()?;
        let saved = editor.save().map_err(|e| e.to_string())?;
        self.data_restart_required |= self.changes.contains_key("wow.data");
        let selected = self.selected;
        let edit = self.edit.clone();
        self.config = Some(editor);
        self.changes.clear();
        self.status = "Settings saved.".into();
        self.selected = None;
        self.edit = EditBoxState::default();
        self.refresh_fields(cvars);
        if let Some(index) = selected {
            self.select(index);
            if self.edit.text == edit.text {
                self.edit = edit;
            }
        }
        Ok(saved)
    }
}
pub(super) fn input(
    mut settings: ResMut<Settings>,
    mut cvars: ResMut<Cvars>,
    actions: Query<(Entity, &Action)>,
    clicks: Res<crate::glue::GlueClicks>,
    mut keyboard: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    clipboard: (NonSendMut<HostClipboard>, Res<crate::bindings::LayoutNames>),
    raw: Query<&bevy::window::RawHandleWrapper, With<bevy::window::PrimaryWindow>>,
    ticket: Option<Res<crate::net::P2pEndpointTicket>>,
    mut reconfigure: Option<ResMut<crate::net::reconfigure::NetworkReconfigure>>,
) {
    if !settings.open {
        keyboard.clear();
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        *settings = Settings::default();
        keyboard.clear();
        return;
    }
    let (mut clipboard, layout) = clipboard;
    let wl = textinput::wayland_display(raw.iter().next());
    for (entity, action) in &actions {
        if !clicks.hit(entity) {
            continue;
        }
        match *action {
            Action::Cancel => {
                *settings = Settings::default();
                keyboard.clear();
                return;
            }
            Action::Select(i) => settings.select(i),
            Action::Group(group) => settings.select_group(group),
            Action::ToggleBoolean(index) => settings.toggle_boolean(index),
            Action::ConfirmApply => {
                if let Some(runtime) = reconfigure.as_mut() {
                    runtime.confirm_disconnect();
                }
            }
            Action::KeepConnection => {
                if let Some(runtime) = reconfigure.as_mut() {
                    runtime.cancel_pending();
                }
                settings.status =
                    "Saved to disk; current connection unchanged. Save again at login to apply, or restart.".into();
                settings.refresh = true;
            }
            Action::Reveal => {
                settings.edit.password = !settings.edit.password;
                settings.refresh = true;
            }
            Action::Save => {
                if reconfigure.as_ref().is_some_and(|runtime| {
                    runtime.status == crate::net::reconfigure::ApplyStatus::Pending
                }) {
                    settings.status = "A connection change is underway. Your draft is retained; wait before saving again.".into();
                    settings.refresh = true;
                    continue;
                }
                match settings.save(&mut cvars) {
                    Ok(saved) => {
                        if let Some(runtime) = reconfigure.as_mut() {
                            runtime.request(saved, false);
                        } else {
                            settings.status = "Settings saved. Restart required: live connection setup is unavailable.".into();
                        }
                    }
                    Err(error) => settings.status = error,
                }
                settings.refresh = true;
            }
            Action::TestLocal | Action::TestPeers => {
                diagnostics::start(&mut settings, matches!(action, Action::TestPeers))
            }
            Action::TestPsk => diagnostics::start_psk(&mut settings),
            Action::RevealTicket => {
                if settings.ticket_preview.is_some() {
                    settings.ticket_preview = None;
                } else if let Some(value) = ticket.as_ref().and_then(|t| t.get()) {
                    settings.ticket_preview = Some(value);
                } else {
                    settings.status="No active iroh endpoint ticket. Start an iroh connection first; tickets change on reconnect.".into();
                }
                settings.refresh = true;
            }
            Action::CopyTicket => {
                if let Some(value) = ticket.as_ref().and_then(|t| t.get()) {
                    clipboard.write(wl, &value);
                    settings.status=if clipboard.read(wl).as_deref()==Some(value.as_str()) { "Current endpoint ticket copied. Share privately; it includes network addresses, but never the passphrase." } else { "Could not verify clipboard copy. Check clipboard access and retry." }.into();
                } else {
                    settings.status =
                        "No active iroh endpoint ticket to copy. Start an iroh connection first."
                            .into();
                }
                settings.refresh = true;
            }
            Action::Repair => {
                let result = std::env::current_exe()
                    .map_err(|_| "Cannot locate executable".to_owned())
                    .and_then(|exe| {
                        ConfigEditor::recover(&exe.with_file_name("config.toml"))
                            .map_err(|e| e.to_string())
                    });
                match result {
                    Ok(editor) => {
                        settings.config = Some(editor);
                        settings.changes.clear();
                        settings.selected = None;
                        settings.refresh_fields(&cvars);
                        settings.status="Blank repair discards previous settings from the new draft. Save backs up the old file as config.toml.invalid-*.bak. P2P is OFF until enabled; Close discards this draft.".into();
                    }
                    Err(error) => settings.status = error,
                }
                settings.refresh = true;
            }
        }
    }
    if let Some(runtime) = reconfigure.as_ref() {
        if settings.apply_status.as_ref() != Some(&runtime.status) {
            use crate::net::reconfigure::ApplyStatus;
            if matches!(runtime.status, ApplyStatus::Pending | ApplyStatus::Applied) {
                settings.sync_ticket_preview(None);
            }
            settings.status = match &runtime.status {
                ApplyStatus::Idle => settings.status.clone(),
                ApplyStatus::Pending => "Settings saved. Applying connection settings…".into(),
                ApplyStatus::NeedsDisconnect => "Settings saved. Applying will disconnect the current session. Choose Disconnect and apply, or Keep current connection.".into(),
                ApplyStatus::Applied => "Saved connection settings applied.".into(),
                ApplyStatus::RestartRequired(reason) => format!("Settings saved. Restart required: {reason}"),
                ApplyStatus::Failed(reason) => format!("Settings saved, but applying failed: {reason}"),
            };
            if settings.has_unsaved_changes() {
                settings
                    .status
                    .push_str(" Newer draft edits are still unsaved.");
            }
            if settings.data_restart_required {
                settings
                    .status
                    .push_str(" Data folder changes require restarting Benilla.");
            }
            settings.apply_status = Some(runtime.status.clone());
            settings.refresh = true;
        }
    }
    if settings.ticket_preview.is_some() {
        let current = ticket.as_ref().and_then(|endpoint| endpoint.get());
        settings.sync_ticket_preview(current.as_deref());
    }
    if keys.just_pressed(KeyCode::Space) {
        if let Some(index) = settings.selected {
            if is_boolean(&settings.fields[index].name) {
                settings.toggle_boolean(index);
            }
        }
    }
    let mods = textinput::mods_now(&keys);
    let editable = settings
        .selected
        .is_some_and(|i| !settings.fields[i].locked && !is_boolean(&settings.fields[i].name));
    for event in keyboard.read() {
        if editable {
            textinput::feed_key(
                &mut settings.edit,
                event,
                mods,
                &layout,
                &mut clipboard,
                wl,
                textinput::CharFilter::Any,
            );
        }
    }
    if let Some(result) = settings
        .diagnostic
        .as_ref()
        .and_then(|rx| rx.lock().ok()?.try_recv().ok())
    {
        settings.status = result;
        settings.diagnostic = None;
        settings.refresh = true;
    }
}
pub(super) fn close(
    mut commands: Commands,
    roots: Query<Entity, With<view::Root>>,
    mut settings: ResMut<Settings>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    *settings = Settings::default();
}
pub(super) fn closed(settings: Res<Settings>) -> bool {
    !settings.open
}
pub(super) use view::{render, Edit};

pub(super) fn recover(
    mut settings: ResMut<Settings>,
    cvars: Res<Cvars>,
    recovery: Option<Res<ConfigRecovery>>,
    mut shown: Local<bool>,
) {
    if !*shown {
        if let Some(error) = recovery {
            settings.open(&cvars, Some(&error));
            *shown = true;
        }
    }
}
