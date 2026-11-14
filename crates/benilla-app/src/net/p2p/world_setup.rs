//! Saved-world setup on the existing profile worker. No native capability or gameplay backend.
use benilla_profile::{world_setup as saved, Error, ProfileStore};
use benilla_ui::script::world_setup as ui;

pub(super) struct Service {
    mailbox: ui::WorldSetupMailbox,
    context: String,
    session: Option<[u8; 16]>,
}
fn hex(id: [u8; 16]) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}
fn parse_id(text: &str) -> Result<[u8; 16], Error> {
    if text.len() != 32 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Invalid("invalid owner or world ID"));
    }
    let mut id = [0; 16];
    for (i, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)
            .map_err(|_| Error::Invalid("invalid owner or world ID"))?;
    }
    if id == [0; 16] {
        return Err(Error::Invalid("owner or world ID must be nonzero"));
    }
    Ok(id)
}
impl Service {
    pub(super) fn open(profile: &mut ProfileStore, mailbox: ui::WorldSetupMailbox) -> Self {
        let session = saved::new_request_session();
        let mut service = Self {
            mailbox,
            context: String::new(),
            session: None,
        };
        let view = match session {
            Ok(session) => {
                service.context = hex(session);
                service.session = Some(session);
                service
                    .view(profile)
                    .unwrap_or_else(|e| service.unavailable(profile, &e))
            }
            Err(error) => service.unavailable(profile, &error),
        };
        service.mailbox.activate(view);
        service
    }
    fn unavailable(&self, profile: &ProfileStore, error: &Error) -> ui::WorldSetupView {
        ui::WorldSetupView {
            context: self.context.clone(),
            owner_id: hex(profile.owner_id()),
            status: format!(
                "Local world setup unavailable: {error}. Existing files are preserved; no grants are enabled."
            ),
            ..ui::WorldSetupView::default()
        }
    }
    fn view(&self, profile: &mut ProfileStore) -> Result<ui::WorldSetupView, Error> {
        let snapshot = profile.world_setups()?;
        Ok(ui::WorldSetupView {
            available: self.session.is_some()
                && snapshot.revision < saved::MAX_SETUP_RECEIPTS as u64,
            context: self.context.clone(),
            revision: snapshot.revision,
            owner_id: hex(profile.owner_id()),
            worlds: snapshot
                .worlds
                .into_iter()
                .map(|world| ui::SavedWorld {
                    id: hex(world.policy.world),
                    label: world.label,
                    owner_id: hex(world.policy.world_owner),
                    allowlist: world.policy.allowlist.into_iter().map(hex).collect(),
                })
                .collect(),
            selected_id: snapshot.selected.map(hex),
            status: if snapshot.revision >= saved::MAX_SETUP_RECEIPTS as u64 {
                "Saved setup history is full. Existing declarations are preserved; no further setup changes can be saved.".into()
            } else {
                "Saved setup only. Gameplay and admin grants are unavailable until this world can start.".into()
            },
        })
    }
    pub(super) fn poll(&mut self, profile: &mut ProfileStore) {
        let Some(request) = self.mailbox.take_request() else {
            return;
        };
        let result = (|| {
            if request.context != self.context {
                return Err(Error::ConcurrentModification);
            }
            let session = self
                .session
                .ok_or(Error::Invalid("no open profile setup session"))?;
            let operation = match &request.operation {
                ui::WorldSetupOperation::Create { label, allowlist } => {
                    if allowlist.len() > saved::MAX_ADMINS {
                        return Err(Error::Invalid("too many admin owner IDs"));
                    }
                    let parsed = allowlist
                        .iter()
                        .map(|id| parse_id(id))
                        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
                    if parsed.len() != allowlist.len() {
                        return Err(Error::Invalid("duplicate admin owner IDs"));
                    }
                    saved::SetupOperation::Create {
                        label: label.clone(),
                        allowlist: parsed,
                    }
                }
                ui::WorldSetupOperation::Select { world_id } => saved::SetupOperation::Select {
                    world: parse_id(world_id)?,
                },
            };
            profile.apply_world_setup(saved::SetupRequest {
                id: saved::SetupRequestId {
                    session,
                    sequence: request.sequence,
                },
                expected_revision: request.revision,
                operation,
            })
        })();
        match (result, self.view(profile)) {
            (Ok(_), Ok(view)) => self.mailbox.finish(
                &request,
                view,
                "Saved and selected. This setup does not start a world or enable gameplay grants."
                    .into(),
                false,
            ),
            (Err(error), Ok(view)) => self.mailbox.finish(
                &request,
                view,
                format!("Setup was not saved: {error}"),
                false,
            ),
            (result, Err(error)) => {
                let message = match result {
                    Ok(_) => "Setup commit completed, but the saved view could not be reloaded. Reopen and inspect before saving again.".into(),
                    Err(Error::CommitUncertain) => "Setup durability is unknown. Reopen and inspect saved worlds before retrying.".into(),
                    Err(e) => format!("Setup is unavailable: {e}. Reopen and inspect before saving again."),
                };
                self.mailbox
                    .finish(&request, self.unavailable(profile, &error), message, true);
            }
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.mailbox.close(&self.context);
    }
}

#[cfg(test)]
#[path = "world_setup/tests.rs"]
mod tests;
