// SPDX-License-Identifier: GPL-3.0-or-later
//! Historical consumed-source compatibility with one genuine raw inventory owner.
//! This is additional metadata evidence, never a corridor, Route or live-source lease.
use super::*;
use benilla_formats::{NativeInventorySource, NativeM2MapInventory};

const POLICY: &[u8] = b"benilla-resident-raw-inventory-consumed-winners-v1";
const CLIENT_POLICY: &str = "benilla-client-chain-native-inventory-v1";
const MAX_NAME_BYTES: usize = 16 * 1024 * 1024;
const MAX_MEMBER_NAME: usize = 1024;

/// The source view is borrowed and the exact loader-minted owner is retained.
/// There is no constructor from digests or serialized metadata.
pub(crate) struct InventoryCompatibility<'view, 'terrain, 'bundle, 'source> {
    view: &'view PreparedSourceHeight<'terrain, 'bundle, 'source>,
    owner: NativeInventorySource,
    revision: Hash,
}
impl InventoryCompatibility<'_, '_, '_, '_> {
    pub(crate) fn owner(&self) -> &NativeInventorySource {
        &self.owner
    }
    pub(crate) fn source_binding(&self) -> (Binding, Hash) {
        (
            self.view.binding(),
            self.view.generated_query_source_epoch(),
        )
    }
    pub(crate) fn revision(&self) -> Hash {
        self.revision
    }
    /// The caller must obtain `installed` from the actual current static index.
    /// This API does not prove that a supplied index is installed or that physics
    /// is current. Concrete corridor checks remain independently mandatory.
    pub(crate) fn verify_current_owner(
        &self,
        installed: &NativeInventorySource,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), CompositionError> {
        poll(deadline, &cancelled)?;
        if !self.owner.same_owner(installed) {
            return Err(Error::Identity.into());
        }
        fresh(self.view, deadline, &cancelled)?;
        poll(deadline, &cancelled)
    }
}
impl<'terrain, 'bundle, 'source> PreparedSourceHeight<'terrain, 'bundle, 'source> {
    pub(crate) fn match_raw_inventory<'view>(
        &'view self,
        inventory: &NativeM2MapInventory,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<InventoryCompatibility<'view, 'terrain, 'bundle, 'source>, CompositionError> {
        poll(deadline, &cancelled)?;
        let owner = inventory.source_provenance().ok_or(Error::Identity)?;
        if owner.policy() != CLIENT_POLICY
            || owner.directory() != self.terrain.map_directory()
            || inventory.directory() != owner.directory()
            || inventory.source_digest() != owner.inventory_digest()
        {
            return Err(Error::Identity.into());
        }
        fresh(self, deadline, &cancelled)?;
        let mut source = BTreeMap::new();
        let mut total_names = 0usize;
        let mut count = 0usize;
        for receipt in self.collision.source_receipts() {
            poll(deadline, &cancelled)?;
            count = count.checked_add(1).ok_or(Error::Budget)?;
            if count > MAX_RECEIPTS {
                return Err(Error::Budget.into());
            }
            let key = key(receipt.name())?;
            total_names = total_names.checked_add(key.len()).ok_or(Error::Budget)?;
            if total_names > MAX_NAME_BYTES {
                return Err(Error::Budget.into());
            }
            let outcome = match receipt.outcome() {
                benilla_native_vmap_source::SourceOutcome::Present {
                    winning_archive,
                    sha256,
                    bytes,
                } => RawOutcome::Present(winning_archive, decode_hash(sha256)?, bytes as u64),
                benilla_native_vmap_source::SourceOutcome::Absent => RawOutcome::Absent,
                benilla_native_vmap_source::SourceOutcome::Dummy {
                    winning_archive,
                    bytes,
                } => RawOutcome::Dummy(winning_archive, bytes as u64),
            };
            if let Some((name, prior)) = source.insert(key, (receipt.name(), outcome)) {
                if name != receipt.name() || prior != outcome {
                    return Err(Error::Dependency.into());
                }
            }
        }
        count = 0;
        for receipt in owner.receipts() {
            poll(deadline, &cancelled)?;
            count += 1;
            if count > MAX_RECEIPTS {
                return Err(Error::Budget.into());
            }
            let expected =
                RawOutcome::Present(receipt.winning_archive(), receipt.sha256(), receipt.bytes());
            if source.get(&key(receipt.member())?).map(|(_, value)| *value) != Some(expected) {
                return Err(Error::Dependency.into());
            }
        }
        if count == 0 {
            return Err(Error::Missing.into());
        }
        // Client inventory visits only MAIN-present raw (x,y) ADTs. The source
        // visits every coordinate. Exact absence elsewhere closes that mismatch;
        // an ignored/dummy/extra source ADT is not silently equivalent.
        for x in 0..64 {
            for y in 0..64 {
                poll(deadline, &cancelled)?;
                let name = format!("World\\Maps\\{0}\\{0}_{x}_{y}.adt", inventory.directory());
                let outcome = source.get(&key(&name)?).ok_or(Error::Missing)?.1;
                match (inventory.tile_digest((x, y)), outcome) {
                    (Some(raw), RawOutcome::Present(_, captured, _)) if raw == captured => {}
                    (None, RawOutcome::Absent) => {}
                    _ => return Err(Error::Dependency.into()),
                }
            }
        }
        // Generated terrain uses (raw y,raw x), not the renderer's raw naming.
        for tile in self.tiles.keys() {
            poll(deadline, &cancelled)?;
            if inventory
                .tile_digest((i32::from(tile.y), i32::from(tile.x)))
                .is_none()
            {
                return Err(Error::Missing.into());
            }
        }
        let mut digest = Sha256::new();
        digest.update(POLICY);
        digest.update(self.binding.map.to_le_bytes());
        digest.update(self.binding.revision);
        digest.update(self.generated_query_source_epoch());
        digest.update(owner.receipt_digest());
        digest.update(owner.inventory_digest());
        digest.update((owner.directory().len() as u64).to_le_bytes());
        digest.update(owner.directory().as_bytes());
        fresh(self, deadline, &cancelled)?;
        poll(deadline, &cancelled)?;
        Ok(InventoryCompatibility {
            view: self,
            owner: owner.clone(),
            revision: digest.finalize().into(),
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum RawOutcome<'a> {
    Present(&'a str, Hash, u64),
    Absent,
    Dummy(&'a str, u64),
}
fn key(name: &str) -> Result<String, Error> {
    if name.is_empty()
        || name.len() > MAX_MEMBER_NAME
        || !name.is_ascii()
        || name.bytes().any(|b| b == 0 || b.is_ascii_control())
    {
        return Err(Error::Identity);
    }
    Ok(name.replace('/', "\\").to_ascii_lowercase())
}
fn decode_hash(text: &str) -> Result<Hash, Error> {
    if text.len() != 64 {
        return Err(Error::Identity);
    }
    let mut result = [0u8; 32];
    for (out, pair) in result.iter_mut().zip(text.as_bytes().as_chunks::<2>().0) {
        let nibble = |b| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            _ => None,
        };
        *out = nibble(pair[0]).ok_or(Error::Identity)? * 16
            + nibble(pair[1]).ok_or(Error::Identity)?;
    }
    Ok(result)
}
fn poll(deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), CompositionError> {
    check(cancelled)?;
    if Instant::now() >= deadline {
        return Err(Error::Deadline.into());
    }
    Ok(())
}
fn fresh(
    view: &PreparedSourceHeight<'_, '_, '_>,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<(), CompositionError> {
    poll(deadline, cancelled)?;
    let result = view.verify_unchanged(|| cancelled() || Instant::now() >= deadline);
    poll(deadline, cancelled)?;
    result
}
