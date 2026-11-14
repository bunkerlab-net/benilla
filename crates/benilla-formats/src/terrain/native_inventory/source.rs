//! Metadata captured at the existing client-chain read boundary, never by a later lookup.
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

const POLICY: &str = "benilla-client-chain-native-inventory-v1";

/// Immutable identity of one successful inventory load and its consumed-member receipts.
/// Cloning retains the same owner; separately loading equal bytes creates a different owner.
/// This is historical input provenance, not a current-install or live-collider guarantee.
#[derive(Clone, Debug)]
pub struct NativeInventorySource(Arc<Source>);

#[derive(Debug)]
struct Source {
    directory: String,
    inventory_digest: [u8; 32],
    receipt_digest: [u8; 32],
    receipts: Box<[NativeInventoryReceipt]>,
}

/// One exact requested member consumed from the recorded winning archive handle.
/// No raw asset bytes or independently supplied receipt constructors are exposed.
#[derive(Debug, PartialEq, Eq)]
pub struct NativeInventoryReceipt {
    member: String,
    winning_archive: String,
    bytes: u64,
    sha256: [u8; 32],
}

impl NativeInventorySource {
    /// Owner identity is stronger than equality of either digest.
    pub fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// Binds the policy, directory, inventory digest and complete framed receipt metadata.
    pub fn receipt_digest(&self) -> [u8; 32] {
        self.0.receipt_digest
    }
    /// The inventory's existing raw-content digest, without changing its winner-free meaning.
    pub fn inventory_digest(&self) -> [u8; 32] {
        self.0.inventory_digest
    }
    pub fn directory(&self) -> &str {
        &self.0.directory
    }
    pub fn policy(&self) -> &'static str {
        POLICY
    }
    /// Distinct reads, ordered by the unambiguous ASCII-case/separator comparison key.
    pub fn receipts(&self) -> impl ExactSizeIterator<Item = &NativeInventoryReceipt> {
        self.0.receipts.iter()
    }
}

impl NativeInventoryReceipt {
    pub fn member(&self) -> &str {
        &self.member
    }
    pub fn winning_archive(&self) -> &str {
        &self.winning_archive
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
}

#[derive(Clone, Copy)]
pub(super) struct SourceLimits {
    pub receipts: usize,
    pub name_bytes: usize,
    pub wdt_bytes: usize,
    pub member_bytes: usize,
}

impl Default for SourceLimits {
    fn default() -> Self {
        Self {
            receipts: 32_768,
            // Counts exact member names, temporary canonical keys, and winner basenames.
            name_bytes: 16 * 1024 * 1024,
            wdt_bytes: 1024 * 1024,
            member_bytes: 64 * 1024 * 1024,
        }
    }
}

impl SourceLimits {
    pub(super) fn member_limit(self, path: &str) -> usize {
        if path.ends_with(".wdt") {
            self.wdt_bytes
        } else {
            self.member_bytes
        }
    }
}

pub(super) struct SourceReceipts {
    limits: SourceLimits,
    name_bytes: usize,
    receipts: BTreeMap<String, NativeInventoryReceipt>,
}

impl SourceReceipts {
    pub(super) fn new(limits: SourceLimits) -> Self {
        Self {
            limits,
            name_bytes: 0,
            receipts: BTreeMap::new(),
        }
    }

    pub(super) fn record(&mut self, member: &str, winner: &str, bytes: &[u8]) -> Result<()> {
        if bytes.len() > self.limits.member_limit(member) {
            bail!("native inventory consumed member size limit: {member}");
        }
        let names = member
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(winner.len()))
            .context("native inventory receipt name overflow")?;
        // Bound the temporary comparison key before allocating it, even for repeated reads.
        if names > self.limits.name_bytes {
            bail!("native inventory receipt name-byte limit");
        }
        let mut key = member.replace('/', "\\");
        key.make_ascii_lowercase();
        let sha256 = Sha256::digest(bytes).into();
        if let Some(prior) = self.receipts.get(&key) {
            if prior.member != member {
                bail!("ambiguous native inventory member spelling: {member}");
            }
            if prior.winning_archive != winner
                || prior.bytes != bytes.len() as u64
                || prior.sha256 != sha256
            {
                bail!("contradictory native inventory repeated read: {member}");
            }
            return Ok(());
        }
        if self.receipts.len() >= self.limits.receipts {
            bail!("native inventory receipt count limit");
        }
        let total_names = self
            .name_bytes
            .checked_add(names)
            .context("native inventory receipt name overflow")?;
        if total_names > self.limits.name_bytes {
            bail!("native inventory receipt name-byte limit");
        }
        self.receipts.insert(
            key,
            NativeInventoryReceipt {
                member: member.into(),
                winning_archive: winner.into(),
                bytes: bytes.len() as u64,
                sha256,
            },
        );
        self.name_bytes = total_names;
        Ok(())
    }

    /// Called only after the actual archive-backed inventory has fully parsed successfully.
    pub(super) fn finish(
        self,
        directory: &str,
        inventory_digest: [u8; 32],
    ) -> NativeInventorySource {
        let receipts: Box<[_]> = self.receipts.into_values().collect();
        let mut hash = Sha256::new();
        hash.update(b"benilla-native-inventory-receipts-v1\0");
        for value in [POLICY, directory] {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
        hash.update(inventory_digest);
        hash.update((receipts.len() as u64).to_le_bytes());
        for receipt in &receipts {
            for value in [&receipt.member, &receipt.winning_archive] {
                hash.update((value.len() as u64).to_le_bytes());
                hash.update(value.as_bytes());
            }
            hash.update(receipt.bytes.to_le_bytes());
            hash.update(receipt.sha256);
        }
        NativeInventorySource(Arc::new(Source {
            directory: directory.into(),
            inventory_digest,
            receipt_digest: hash.finalize().into(),
            receipts,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_reads_are_exact_and_aliases_cannot_hide_conflicts() {
        let mut receipts = SourceReceipts::new(SourceLimits::default());
        receipts
            .record("World\\A.m2", "Model.MPQ", &[1, 2])
            .unwrap();
        let names = receipts.name_bytes;
        receipts
            .record("World\\A.m2", "Model.MPQ", &[1, 2])
            .unwrap();
        assert_eq!(receipts.receipts.len(), 1);
        assert_eq!(receipts.name_bytes, names);
        for (member, winner, bytes) in [
            ("world\\a.m2", "Model.MPQ", &[1, 2][..]),
            ("World/A.m2", "Model.MPQ", &[1, 2][..]),
            ("World\\A.m2", "model.MPQ", &[1, 2][..]),
            ("World\\A.m2", "Model.MPQ", &[1, 3][..]),
            ("World\\A.m2", "Model.MPQ", &[1, 2, 3][..]),
        ] {
            assert!(receipts.record(member, winner, bytes).is_err());
        }
        assert_eq!(receipts.receipts.len(), 1);
        assert_eq!(receipts.name_bytes, names);
    }

    #[test]
    fn reduced_bounds_check_actual_buffers_and_allow_only_exact_repeats_at_capacity() {
        let limits = SourceLimits {
            receipts: 1,
            name_bytes: 4,
            wdt_bytes: 1,
            member_bytes: 2,
        };
        let mut receipts = SourceReceipts::new(limits);
        receipts.record("a", "bc", &[0; 2]).unwrap();
        receipts.record("a", "bc", &[0; 2]).unwrap();
        assert!(receipts.record("a", "bc", &[0; 3]).is_err());
        assert!(receipts.record("b", "bc", &[0; 2]).is_err());
        let mut receipts = SourceReceipts::new(SourceLimits {
            receipts: 2,
            ..limits
        });
        receipts.record("a", "bc", &[]).unwrap();
        assert!(receipts.record("b", "bc", &[]).is_err());
        assert!(receipts.record("abc", "", &[]).is_err());
        let mut receipts = SourceReceipts::new(SourceLimits {
            name_bytes: 32,
            ..limits
        });
        receipts.record("a.wdt", "b", &[0]).unwrap();
        assert!(receipts.record("a.wdt", "b", &[0; 2]).is_err());
    }
}
