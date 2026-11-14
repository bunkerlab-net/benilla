//! Fork-only saved-world setup. This mailbox is entirely separate from admin gameplay authority.
//! UI calls only queue bounded requests; the profile worker owns signing and all disk I/O.
use super::UiScript;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedWorld {
    pub id: String,
    pub label: String,
    pub owner_id: String,
    pub allowlist: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldSetupView {
    pub available: bool,
    pub context: String,
    pub revision: u64,
    pub owner_id: String,
    pub worlds: Vec<SavedWorld>,
    pub selected_id: Option<String>,
    pub status: String,
}
impl Default for WorldSetupView {
    fn default() -> Self {
        Self {
            available: false,
            context: String::new(),
            revision: 0,
            owner_id: String::new(),
            worlds: Vec::new(),
            selected_id: None,
            status: "Local world setup is available only with an open saved P2P character.".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldSetupOperation {
    Create {
        label: String,
        allowlist: Vec<String>,
    },
    Select {
        world_id: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldSetupRequest {
    pub context: String,
    pub sequence: u64,
    pub revision: u64,
    pub operation: WorldSetupOperation,
}
#[derive(Default)]
struct Shared {
    view: WorldSetupView,
    pending: Option<WorldSetupRequest>,
    taken: bool,
    sequence: u64,
    message: String,
}
/// Shared across VM reloads. At most one queued/in-flight request, never a gameplay backend.
#[derive(Clone, Default)]
pub struct WorldSetupMailbox(Arc<Mutex<Shared>>);
impl WorldSetupMailbox {
    /// Host-only, when an already-open profile enters exploration. Context must be fresh.
    pub fn activate(&self, view: WorldSetupView) {
        let mut shared = self.0.lock().expect("world setup mailbox");
        let unresolved = shared.pending.is_some();
        *shared = Shared {
            view,
            message: if unresolved {
                "A previous setup result was unknown. Review the saved worlds before creating another.".into()
            } else {
                String::new()
            },
            ..Shared::default()
        };
    }
    /// The worker takes each request once; an uncertain commit leaves the pending latch intact.
    pub fn take_request(&self) -> Option<WorldSetupRequest> {
        let mut shared = self.0.lock().expect("world setup mailbox");
        if shared.taken {
            return None;
        }
        let request = shared.pending.clone()?;
        shared.taken = true;
        Some(request)
    }
    pub fn finish(
        &self,
        request: &WorldSetupRequest,
        view: WorldSetupView,
        message: String,
        uncertain: bool,
    ) {
        let mut shared = self.0.lock().expect("world setup mailbox");
        if shared.pending.as_ref() != Some(request) || shared.view.context != request.context {
            return;
        }
        shared.view = view;
        shared.message = message;
        if !uncertain {
            shared.pending = None;
            shared.taken = false;
        }
    }
    pub fn close(&self, context: &str) {
        let mut shared = self.0.lock().expect("world setup mailbox");
        if shared.view.context != context {
            return;
        }
        shared.view.available = false;
        shared.view.status =
            "Saved profile session closed. Reopen your P2P character to review setup.".into();
        if shared.pending.is_some() {
            shared.message =
                "Setup result is unknown. Reopen and inspect saved worlds before retrying.".into();
        }
    }
    fn submit(
        &self,
        context: &str,
        revision: &str,
        operation: Result<WorldSetupOperation, String>,
    ) -> bool {
        let mut shared = self.0.lock().expect("world setup mailbox");
        let result = (|| {
            if !shared.view.available {
                return Err(shared.view.status.clone());
            }
            if shared.pending.is_some() {
                return Err("A setup save is pending. Wait for its result.".into());
            }
            if context != shared.view.context || revision != shared.view.revision.to_string() {
                return Err("Profile or setup changed. Refresh and review before saving.".into());
            }
            let operation = operation?;
            match &operation {
                WorldSetupOperation::Create { allowlist, .. } => {
                    if shared.view.worlds.len() >= 16 {
                        return Err("At most 16 local world setups may be saved.".into());
                    }
                    if allowlist.contains(&shared.view.owner_id) {
                        return Err("The owner is already an admin; omit their owner ID.".into());
                    }
                }
                WorldSetupOperation::Select { world_id } => {
                    if !shared.view.worlds.iter().any(|w| &w.id == world_id) {
                        return Err("Choose a world saved by this profile.".into());
                    }
                }
            }
            shared.sequence = shared
                .sequence
                .checked_add(1)
                .ok_or("Setup request sequence exhausted.")?;
            shared.pending = Some(WorldSetupRequest {
                context: context.into(),
                revision: shared.view.revision,
                sequence: shared.sequence,
                operation,
            });
            shared.taken = false;
            Ok(())
        })();
        shared.message = match &result {
            Ok(()) => "Saving setup. No success confirmed yet.".into(),
            Err(reason) => reason.clone(),
        };
        result.is_ok()
    }
}
fn id(text: &str) -> Result<String, String> {
    if text.len() != 32
        || !text.bytes().all(|b| b.is_ascii_hexdigit())
        || text.bytes().all(|b| b == b'0')
    {
        return Err("Owner and world IDs must be 32 nonzero hexadecimal characters.".into());
    }
    Ok(text.to_ascii_lowercase())
}
fn create(label: &str, text: &str) -> Result<WorldSetupOperation, String> {
    let label = label.trim();
    if label.is_empty()
        || label.len() > 40
        || !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'-' | b'_'))
    {
        return Err("World name needs 1-40 letters, numbers, spaces, - or _.".into());
    }
    if text.len() > 64 * 34 {
        return Err("Use at most 64 distinct other admin owner IDs.".into());
    }
    let mut allowlist = std::collections::BTreeSet::new();
    for value in text
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|v| !v.is_empty())
    {
        if !allowlist.insert(id(value)?) {
            return Err("An admin owner ID is listed more than once.".into());
        }
        if allowlist.len() > 64 {
            return Err("Use at most 64 distinct other admin owner IDs.".into());
        }
    }
    Ok(WorldSetupOperation::Create {
        label: label.into(),
        allowlist: allowlist.into_iter().collect(),
    })
}
impl UiScript {
    pub fn world_setup_interface_installed(&self) -> bool {
        self.lua.app_data_ref::<WorldSetupMailbox>().is_some()
    }
    pub fn install_world_setup_interface(&self) -> mlua::Result<()> {
        if self.lua.app_data_ref::<WorldSetupMailbox>().is_some() {
            return Ok(());
        }
        self.lua.set_app_data(WorldSetupMailbox::default());
        self.lua.globals().set(
            "BenillaWorldSetupGetView",
            self.lua.create_function(|lua, ()| {
                let mailbox = lua
                    .app_data_ref::<WorldSetupMailbox>()
                    .expect("world setup interface");
                let shared = mailbox.0.lock().expect("world setup mailbox");
                let view = lua.create_table()?;
                view.set("available", shared.view.available)?;
                view.set("pending", shared.pending.is_some())?;
                view.set("context", shared.view.context.as_str())?;
                view.set("revision", shared.view.revision.to_string())?;
                view.set("owner_id", shared.view.owner_id.as_str())?;
                view.set("status", shared.view.status.as_str())?;
                view.set("message", shared.message.as_str())?;
                view.set("selected_id", shared.view.selected_id.as_deref())?;
                let worlds = lua.create_table()?;
                for (i, world) in shared.view.worlds.iter().enumerate() {
                    let row = lua.create_table()?;
                    row.set("id", world.id.as_str())?;
                    row.set("label", world.label.as_str())?;
                    row.set("owner_id", world.owner_id.as_str())?;
                    row.set(
                        "selected",
                        shared.view.selected_id.as_ref() == Some(&world.id),
                    )?;
                    row.set(
                        "allowlist",
                        lua.create_sequence_from(world.allowlist.iter().cloned())?,
                    )?;
                    worlds.set(i + 1, row)?;
                }
                view.set("worlds", worlds)?;
                Ok(view)
            })?,
        )?;
        self.lua.globals().set(
            "BenillaWorldSetupCreate",
            self.lua.create_function(
                |lua, (label, admins, context, revision): (String, String, String, String)| {
                    let mailbox = lua
                        .app_data_ref::<WorldSetupMailbox>()
                        .expect("world setup interface");
                    Ok(mailbox.submit(&context, &revision, create(&label, &admins)))
                },
            )?,
        )?;
        self.lua.globals().set(
            "BenillaWorldSetupSelect",
            self.lua.create_function(
                |lua, (world, context, revision): (String, String, String)| {
                    let mailbox = lua
                        .app_data_ref::<WorldSetupMailbox>()
                        .expect("world setup interface");
                    Ok(mailbox.submit(
                        &context,
                        &revision,
                        id(&world).map(|world_id| WorldSetupOperation::Select { world_id }),
                    ))
                },
            )?,
        )?;
        Ok(())
    }
    /// The host installs the existing mailbox; rebinding never grants any admin capability.
    pub fn set_world_setup_mailbox(&self, mailbox: WorldSetupMailbox) -> mlua::Result<()> {
        self.install_world_setup_interface()?;
        self.lua.set_app_data(mailbox);
        Ok(())
    }
}
#[cfg(test)]
mod tests;
