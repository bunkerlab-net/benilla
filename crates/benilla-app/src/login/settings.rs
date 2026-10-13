//! Fork-only login settings: actual engine CVars plus executable-adjacent startup config.
//! Draft edits do nothing until Save; startup changes require restarting the process.
mod diagnostics;
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
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Graphics,
    Audio,
    All,
    Startup,
}
#[derive(Clone)]
struct Field {
    name: String,
    value: String,
    secret: bool,
    locked: bool,
}
#[derive(Resource, Default)]
pub(super) struct Settings {
    pub(super) open: bool,
    fields: Vec<Field>,
    page: Option<Page>,
    offset: usize,
    selected: Option<usize>,
    edit: EditBoxState,
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
    Page(Page),
    Select(usize),
    Previous,
    Next,
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
const PAGE_SIZE: usize = 8;
const STARTUP: &[(&str, &str, bool)] = &[
    ("p2p.enabled", "false", false),
    ("p2p.psk", "", true),
    ("p2p.network", "benilla", false),
    ("p2p.listen", "0.0.0.0:7474", false),
    ("p2p.peers", "[]", false),
    ("p2p.max_peers", "16", false),
    ("p2p.timeout_ms", "5000", false),
    ("wow.data", "", false),
    ("ruleset.xp_rate", "1.0", false),
    ("ruleset.drop_rate", "1.0", false),
    ("p2p.internet.enabled", "false", false),
    ("p2p.internet.relay_urls", "[]", false),
    ("p2p.internet.peer_tickets", "[]", false),
    ("p2p.internet.port_mapping", "false", false),
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
        self.switch(
            if recovery.is_some() {
                Page::Startup
            } else {
                Page::Graphics
            },
            cvars,
        );
    }
    fn keep_edit(&mut self) {
        if let Some(i) = self.selected {
            if !self.fields[i].locked && self.fields[i].value != self.edit.text {
                self.fields[i].value.clone_from(&self.edit.text);
                self.changes
                    .insert(self.fields[i].name.clone(), self.edit.text.clone());
            }
        }
    }
    fn switch(&mut self, page: Page, cvars: &Cvars) {
        self.keep_edit();
        self.selected = None;
        self.edit = EditBoxState::default();
        self.offset = 0;
        self.page = Some(page);
        self.fields = if page == Page::Startup {
            STARTUP
                .iter()
                .map(|(name, default, secret)| {
                    let (section, key) = name.rsplit_once('.').unwrap();
                    let value = self
                        .changes
                        .get(*name)
                        .cloned()
                        .or_else(|| {
                            self.config.as_ref().and_then(|c| {
                                if *name == "p2p.encounter.enabled" {
                                    Some(c.has_section("p2p.encounter").to_string())
                                } else if *name == "p2p.internet.enabled" {
                                    Some(c.has_section("p2p.internet").to_string())
                                } else if *name == "p2p.enabled" {
                                    Some(c.has_section("p2p").to_string())
                                } else {
                                    c.get(section, key)
                                }
                            })
                        })
                        .unwrap_or_else(|| (*default).into());
                    Field {
                        name: (*name).into(),
                        value,
                        secret: *secret,
                        locked: self.config.is_none(),
                    }
                })
                .collect()
        } else {
            cvars
                .rows()
                .filter(|r| matches_page(page, &r.name))
                .map(|r| Field {
                    name: r.name.clone(),
                    value: self
                        .changes
                        .get(&r.name)
                        .cloned()
                        .unwrap_or_else(|| r.pending.as_ref().unwrap_or(&r.value).clone()),
                    secret: false,
                    locked: cvars.is_session_owned(&r.name),
                })
                .collect()
        };
        self.refresh = true;
    }
    fn select(&mut self, index: usize) {
        if index >= self.fields.len() {
            return;
        }
        self.keep_edit();
        self.selected = Some(index);
        self.edit = EditBoxState::default();
        self.edit.max_bytes = Some(4096);
        self.edit.password = self.fields[index].secret;
        self.edit.set_text(&self.fields[index].value);
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
            .get("p2p.internet.enabled")
            .map(String::as_str)
            .map(|v| match v {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err("p2p.internet.enabled must be true or false"),
            })
            .transpose()?
            .unwrap_or_else(|| editor.has_section("p2p.internet"));
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
                || name == "p2p.internet.enabled"
                || name == "p2p.encounter.enabled"
                || (section.starts_with("p2p") && !enabled)
                || (section == "p2p.internet" && !internet)
                || (section == "p2p.encounter" && !encounter)
            {
                continue;
            }
            if section == "wow" && value.is_empty() {
                editor.remove(section, key);
            } else if matches!(
                name.as_str(),
                "p2p.psk"
                    | "p2p.network"
                    | "p2p.listen"
                    | "wow.data"
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
        if internet && enabled && !editor.has_section("p2p.internet") {
            editor
                .set_literal("p2p.internet", "port_mapping", "false")
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
            editor.remove_section("p2p.internet");
        }
        if !enabled {
            editor.remove_section("p2p");
        }
        if enabled && editor.get("p2p", "psk").is_none() {
            return Err("Enter a P2P passphrase before enabling P2P".into());
        }
        Ok(editor)
    }
    fn save(&mut self, cvars: &mut Cvars) -> Result<(), String> {
        self.keep_edit();
        if self.page == Some(Page::Startup) {
            let mut editor = self.prepare_config()?;
            editor.save().map_err(|e| e.to_string())?;
            self.config = Some(editor);
            self.changes.retain(|key, _| !key.contains('.'));
            self.status = "Startup settings saved. Restart Benilla to use them; the current connection is unchanged.".into();
        } else {
            let game = self
                .changes
                .iter()
                .filter(|(key, _)| !key.contains('.'))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            cvars.apply_settings(&game)?;
            self.changes.retain(|key, _| key.contains('.'));
            self.status =
                "Game settings saved and applied through the engine settings registry.".into();
        }
        self.selected = None;
        self.edit = EditBoxState::default();
        self.switch(self.page.unwrap_or(Page::Graphics), cvars);
        Ok(())
    }
}
fn matches_page(page: Page, name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    match page {
        Page::Audio => {
            lower.contains("sound") || lower.contains("volume") || lower.contains("music")
        }
        Page::Graphics => {
            lower.starts_with("gx")
                || [
                    "farclip",
                    "nearclip",
                    "worlddetail",
                    "frilldensity",
                    "renderscale",
                    "uiscale",
                    "useuiscale",
                    "gamma",
                    "trilinear",
                    "anisotropic",
                    "basemip",
                    "terrainmip",
                    "shadowlevel",
                    "ffxglow",
                ]
                .contains(&lower.as_str())
        }
        Page::All => true,
        Page::Startup => false,
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
            Action::Page(page) => settings.switch(page, &cvars),
            Action::Select(i) => settings.select(i),
            Action::Previous | Action::Next => {
                settings.keep_edit();
                settings.selected = None;
                settings.edit = EditBoxState::default();
                settings.offset = if matches!(action, Action::Next) {
                    (settings.offset + PAGE_SIZE)
                        .min(settings.fields.len().saturating_sub(1) / PAGE_SIZE * PAGE_SIZE)
                } else {
                    settings.offset.saturating_sub(PAGE_SIZE)
                };
                settings.refresh = true;
            }
            Action::Reveal => {
                settings.edit.password = !settings.edit.password;
                settings.refresh = true;
            }
            Action::Save => {
                if let Err(error) = settings.save(&mut cvars) {
                    settings.status = error;
                    settings.refresh = true;
                }
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
                    settings.status="No active Internet endpoint ticket. Start an Internet connection first; tickets change on reconnect.".into();
                }
                settings.refresh = true;
            }
            Action::CopyTicket => {
                if let Some(value) = ticket.as_ref().and_then(|t| t.get()) {
                    clipboard.write(wl, &value);
                    settings.status=if clipboard.read(wl).as_deref()==Some(value.as_str()) { "Current endpoint ticket copied. Share privately; it includes network addresses, but never the passphrase." } else { "Could not verify clipboard copy. Check clipboard access and retry." }.into();
                } else {
                    settings.status="No active Internet endpoint ticket to copy. Start an Internet connection first.".into();
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
                        settings.switch(Page::Startup, &cvars);
                        settings.status="Blank repair discards previous settings from the new draft. Save backs up the old file as config.toml.invalid-*.bak. P2P is OFF until enabled; Close discards this draft.".into();
                    }
                    Err(error) => settings.status = error,
                }
                settings.refresh = true;
            }
        }
    }
    let mods = textinput::mods_now(&keys);
    let editable = settings
        .selected
        .is_some_and(|i| !settings.fields[i].locked);
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
pub(super) use view::render;

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
