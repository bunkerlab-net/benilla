//! Owner-bound local recovery on the existing profile worker.
use anyhow::{Context, Result};
use benilla_profile::ProfileStore;

/// The same durable restore as the CLI, served by the profile's existing exclusive writer.
/// Expected refusals preserve the lobby; an unexpected save failure closes it before retries.
pub(super) fn recover(
    store: &mut ProfileStore,
    request: crate::char_select::recovery_model::Request,
    changed_roster: impl FnOnce(&ProfileStore) -> Result<()>,
) -> Result<()> {
    use crate::char_select::recovery_model::Request;
    let (target, reply) = match request {
        Request::List(reply) => {
            let _ = reply.send(Ok(store.archived_characters().to_vec()));
            return Ok(());
        }
        Request::Restore { target, reply } => (target, reply),
    };
    let refusal = if target.owner_id != store.owner_id()
        || !store.archived_characters().contains(&target)
    {
        Some("This archived character is no longer available. Reopen recovery to refresh the list.")
    } else if store.characters().len() >= benilla_profile::MAX_CHARACTERS {
        Some("Your character list is full. Delete a character before restoring another.")
    } else if store
        .characters()
        .iter()
        .any(|c| c.name.eq_ignore_ascii_case(&target.name))
    {
        Some("An active character already has this name. No characters were replaced.")
    } else {
        None
    };
    if let Some(message) = refusal {
        let _ = reply.send(Err(message.into()));
        return Ok(());
    }
    if let Err(error) = store.restore_character(target.owner_id, target.id) {
        let _ = reply.send(Err("Recovery could not be saved. Reopen the local profile and inspect the character list before retrying.".into()));
        return Err(error).context("could not save character recovery; local profile closed");
    }
    changed_roster(store)?;
    let _ = reply.send(Ok(store.archived_characters().to_vec()));
    Ok(())
}
