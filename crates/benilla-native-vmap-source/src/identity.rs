// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only producer dependency receipts. A digest is diagnostic, not admission.
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

/// Roles stay distinct even when the captured bytes happen to be identical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DependencyRole {
    Archive,
    Listfile,
    Member,
    Tool,
    Library,
    NumericEnvironment,
    InstallBefore,
    InstallAfter,
    Config,
    Offmesh,
    GeneratedIds,
    BihOrder,
    ModelOrder,
    TransportModel,
    Product,
    InitialOutput,
    Arguments,
    ProducerOccurrences,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyState {
    Uncaptured,
    Absent,
    Present { bytes: u64, sha256: [u8; 32] },
}
#[derive(Debug)]
pub enum IdentityError {
    Io(std::io::ErrorKind),
    Changed,
    Missing(DependencyRole),
}
impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "producer identity: {self:?}")
    }
}
impl std::error::Error for IdentityError {}

struct Dependency {
    role: DependencyRole,
    // Exact raw occurrence label, including any terminal NUL, never a path key.
    name: Vec<u8>,
    path: std::path::PathBuf,
    state: DependencyState,
}
/// Records actual observed files/absences in consumer order. No deserializer or
/// supplied-digest constructor exists. These receipts cannot repair old lineage.
#[derive(Default)]
pub struct ProducerIdentity {
    dependencies: Vec<Dependency>,
}
impl ProducerIdentity {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn capture(
        &mut self,
        role: DependencyRole,
        raw_name: &[u8],
        path: &Path,
    ) -> Result<DependencyState, IdentityError> {
        let state = observe(path)?;
        self.dependencies.push(Dependency {
            role,
            name: raw_name.to_vec(),
            path: path.to_owned(),
            state,
        });
        Ok(state)
    }
    pub fn occurrence(&self, ordinal: usize) -> DependencyState {
        self.dependencies
            .get(ordinal)
            .map_or(DependencyState::Uncaptured, |d| d.state)
    }
    /// Require at least one positive receipt for each named role. This does not
    /// prove complete member closure or that a receipt observed an old run.
    pub fn require_present(&self, roles: &[DependencyRole]) -> Result<(), IdentityError> {
        for &role in roles {
            if !self
                .dependencies
                .iter()
                .any(|d| d.role == role && matches!(d.state, DependencyState::Present { .. }))
            {
                return Err(IdentityError::Missing(role));
            }
        }
        Ok(())
    }
    pub fn verify_unchanged(&self) -> Result<(), IdentityError> {
        for dependency in &self.dependencies {
            if observe(&dependency.path)? != dependency.state {
                return Err(IdentityError::Changed);
            }
        }
        Ok(())
    }
    pub fn digest(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        field(&mut hash, b"cmangos-8ec338a1-producer-dependencies-v1");
        for dependency in &self.dependencies {
            hash.update([dependency.role as u8]);
            field(&mut hash, &dependency.name);
            match dependency.state {
                DependencyState::Present { bytes, sha256 } => {
                    field(&mut hash, b"present");
                    hash.update(bytes.to_le_bytes());
                    hash.update(sha256);
                }
                DependencyState::Absent => field(&mut hash, b"observed-absent"),
                DependencyState::Uncaptured => unreachable!(),
            }
        }
        hash.finalize().into()
    }
}
fn observe(path: &Path) -> Result<DependencyState, IdentityError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DependencyState::Absent)
        }
        Err(error) => return Err(IdentityError::Io(error.kind())),
    };
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|e| IdentityError::Io(e.kind()))?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        hash.update(&buffer[..count]);
    }
    Ok(DependencyState::Present {
        bytes,
        sha256: hash.finalize().into(),
    })
}
pub(crate) fn field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

impl crate::PreparedDiscovery {
    /// Includes source occurrences, raw model order/bytes and the selected
    /// generated-ID record stream. No output is rewritten to fit a hash.
    pub fn producer_binding(
        &self,
        identity: &ProducerIdentity,
        profile: crate::DoodadProjectionProfile,
        cancelled: impl Fn() -> bool,
    ) -> Result<[u8; 32], crate::PlacementRecordError> {
        let records = self.placement_records(profile, cancelled)?;
        let mut hash = Sha256::new();
        field(&mut hash, self.summary().writer_policy.as_bytes());
        field(&mut hash, self.summary().listfile_policy.as_bytes());
        field(&mut hash, self.summary().source_epoch.as_bytes());
        field(&mut hash, &identity.digest());
        field(&mut hash, records.bytes());
        for product in self.products() {
            field(&mut hash, product.source_member().as_bytes());
            field(&mut hash, product.model_name().as_bytes());
            match product.raw_vmap() {
                Some(bytes) => {
                    field(&mut hash, b"file");
                    field(&mut hash, bytes);
                }
                None => field(&mut hash, b"zero-hull-no-file"),
            }
        }
        Ok(hash.finalize().into())
    }
}
