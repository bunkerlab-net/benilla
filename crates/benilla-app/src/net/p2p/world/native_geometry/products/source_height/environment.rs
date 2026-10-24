// SPDX-License-Identifier: GPL-3.0-or-later
//! Metadata-only compatibility for an already-owned complete catalog. No acquisition.
use super::*;
use benilla_world_runtime::native::environment::{EnvironmentSnapshotBinding, LiquidCatalog};

const POLICY: &[u8] = b"benilla-resident-consumed-source-player-environment-v1";
impl PreparedSourceHeight<'_, '_, '_> {
    /// Bind supplied catalog content to the actual selected complete WDBC receipt.
    /// Content is explicit world configuration, not server database authenticity.
    /// The result identifies a historical consumed-source snapshot, not a live lease.
    pub(crate) fn environment_binding(
        &self,
        content: Hash,
        catalog: &LiquidCatalog,
        cancelled: impl Fn() -> bool,
    ) -> Result<EnvironmentSnapshotBinding, CompositionError> {
        check(&cancelled)?;
        if content == [0; 32] || !catalog.is_complete() {
            return Err(Error::Identity.into());
        }
        self.verify_unchanged(&cancelled)?;
        let digest = catalog.digest();
        let mut found = false;
        for (index, receipt) in self.terrain.source_receipts().enumerate() {
            if index % 256 == 0 {
                check(&cancelled)?;
            }
            if receipt.name() != "DBFilesClient\\LiquidType.dbc" {
                continue;
            }
            let benilla_native_terrain::SourceOutcome::Present { sha256, .. } = receipt.outcome()
            else {
                return Err(Error::Identity.into());
            };
            let hex = b"0123456789abcdef";
            if found
                || sha256.len() != 64
                || digest.iter().enumerate().any(|(i, byte)| {
                    sha256.as_bytes()[i * 2] != hex[usize::from(byte >> 4)]
                        || sha256.as_bytes()[i * 2 + 1] != hex[usize::from(byte & 15)]
                })
            {
                return Err(Error::Identity.into());
            }
            found = true;
        }
        if !found {
            return Err(Error::Missing.into());
        }
        let mut epoch = Sha256::new();
        epoch.update(POLICY);
        epoch.update(b"\0source\0");
        epoch.update(self.generated_query_source_epoch());
        let mut inventory = Sha256::new();
        inventory.update(POLICY);
        inventory.update(b"\0inventory\0");
        inventory.update(self.binding.map.to_le_bytes());
        inventory.update(self.binding.revision);
        inventory.update(digest);
        let result = EnvironmentSnapshotBinding {
            map: self.binding.map,
            content,
            source_epoch: epoch.finalize().into(),
            inventory: inventory.finalize().into(),
            policy: Sha256::digest(POLICY).into(),
            liquid_catalog: digest,
        };
        self.verify_unchanged(&cancelled)?;
        check(&cancelled)?;
        Ok(result)
    }
}
