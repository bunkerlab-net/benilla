// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_world_runtime::{MAX_JOURNAL_BYTES, MAX_SNAPSHOT_BYTES};
const MAGIC: &[u8; 8] = b"BNWORLD\0";
const HEADER: usize = 8 + 4 + 8 + 32 + 4 + 4;
pub(super) const OVERHEAD: usize = HEADER + 32;

pub(super) struct Envelope {
    pub generation: u64,
    pub parent_digest: Digest,
    pub snapshot: Vec<u8>,
    pub journal: Vec<u8>,
}
impl Envelope {
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        if self.generation == 0
            || self.snapshot.len() > MAX_SNAPSHOT_BYTES
            || self.journal.len() > MAX_JOURNAL_BYTES
        {
            return Err(Error::Corrupt);
        }
        let mut bytes = Vec::with_capacity(OVERHEAD + self.snapshot.len() + self.journal.len());
        bytes.extend(MAGIC);
        bytes.extend(FORMAT_VERSION.to_le_bytes());
        bytes.extend(self.generation.to_le_bytes());
        bytes.extend(self.parent_digest);
        bytes.extend((self.snapshot.len() as u32).to_le_bytes());
        bytes.extend((self.journal.len() as u32).to_le_bytes());
        bytes.extend(&self.snapshot);
        bytes.extend(&self.journal);
        bytes.extend(checksum(&bytes));
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < OVERHEAD || bytes.len() > MAX_CHECKPOINT_BYTES || &bytes[..8] != MAGIC {
            return Err(Error::Corrupt);
        }
        if u32::from_le_bytes(bytes[8..12].try_into().unwrap()) != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion);
        }
        let generation = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
        let parent_digest = bytes[20..52].try_into().unwrap();
        let sn = u32::from_le_bytes(bytes[52..56].try_into().unwrap()) as usize;
        let jn = u32::from_le_bytes(bytes[56..60].try_into().unwrap()) as usize;
        if generation == 0
            || sn > MAX_SNAPSHOT_BYTES
            || jn > MAX_JOURNAL_BYTES
            || bytes.len() != OVERHEAD + sn + jn
            || checksum(&bytes[..bytes.len() - 32]) != bytes[bytes.len() - 32..]
        {
            return Err(Error::Corrupt);
        }
        if (generation == 1) != (parent_digest == [0; 32]) {
            return Err(Error::Corrupt);
        }
        Ok(Self {
            generation,
            parent_digest,
            snapshot: bytes[HEADER..HEADER + sn].to_vec(),
            journal: bytes[HEADER + sn..HEADER + sn + jn].to_vec(),
        })
    }
    pub fn restore(
        &self,
        identity: &WorldIdentity,
        data: WorldData,
        rules: Rules,
    ) -> Result<World, Error> {
        let world = World::replay(data, rules, &self.snapshot, &self.journal)?;
        if !identity.matches(&world) {
            return Err(Error::Incompatible);
        }
        Ok(world)
    }
}
fn checksum(bytes: &[u8]) -> Digest {
    let mut hash = Sha256::new();
    hash.update(b"benilla-world-store-envelope-v1\0");
    hash.update(bytes);
    hash.finalize().into()
}
