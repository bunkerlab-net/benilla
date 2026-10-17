//! Owner-bound local character actions, independent of Bevy and transport startup.
use anyhow::{Context, Result};
use benilla_profile::{Character, ProfileStore};
use benilla_protocol::{messages::CHAR_DELETE_SUCCESS, CharAction, SessionEvent};
use sha2::{Digest, Sha256};

/// A character's game identity is scoped to its authenticated owner, not an attacker-chosen ID.
pub(super) fn character_guid(character: &Character) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"benilla-character-guid-v1\0");
    hash.update(character.owner_id);
    hash.update(character.id);
    let mut bytes = [0; 8];
    bytes[..6].copy_from_slice(&hash.finalize()[..6]);
    u64::from_le_bytes(bytes).max(1)
}

/// The existing typed-confirmation UI supplies a snapshotted GUID, never a roster index.
/// Emit a changed roster and success only after durable save. On uncertain I/O, report failure
/// and close the lobby so it must reopen the actual disk state before accepting more requests.
pub(super) fn delete(
    store: &mut ProfileStore,
    wanted: u64,
    mut emit: impl FnMut(SessionEvent) -> Result<()>,
    changed_roster: impl FnOnce(&ProfileStore) -> Result<()>,
) -> Result<()> {
    let mut matches = store
        .characters()
        .iter()
        .filter(|c| c.owner_id == store.owner_id() && character_guid(c) == wanted);
    let id = matches.next().map(|c| c.id);
    let unique = matches.next().is_none();
    let Some(id) = id.filter(|_| unique) else {
        return emit(SessionEvent::CharActionResult {
            action: CharAction::Delete,
            code: 0x3a,
        });
    };
    match store.archive_character(store.owner_id(), id) {
        Ok(()) => {
            changed_roster(store)?;
            emit(SessionEvent::CharActionResult {
                action: CharAction::Delete,
                code: CHAR_DELETE_SUCCESS,
            })
        }
        Err(error) => {
            emit(SessionEvent::CharActionResult {
                action: CharAction::Delete,
                code: 0x3a, // CHAR_DELETE_FAILED; never present uncertain writes as success.
            })?;
            // Storage security/path checks can also return Invalid. Once a live target was
            // resolved, every failed commit closes the lobby rather than allowing blind retries.
            Err(error).context("could not save character removal; close and reopen the local profile before retrying")
        }
    }
}
