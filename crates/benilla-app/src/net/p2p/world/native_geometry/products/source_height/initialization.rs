// SPDX-License-Identifier: GPL-3.0-or-later
//! Metadata for a selected resident IsSwimmable configuration, never dry proof.
use super::*;
use benilla_world_runtime::native::CreatureInitializationSnapshot;

const POLICY: &[u8] = b"cmangos-8ec338a1-resident-point-creature-initialization-v1";
const CONFIGURATION: &[u8] = b"vmap-height-enabled;mask=15;inner-height=0x4002007e;no-water-only";

/// Explicit local source-policy selection, not evidence of a server setting.
/// The source can fall back to GetGrid with VMap disabled; this adapter's
/// deliberately narrower supported domain refuses disabled/unknown settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResidentInitializationConfiguration {
    VmapHeightEnabled,
    VmapHeightDisabled,
}
impl PreparedSourceHeight<'_, '_, '_> {
    /// Choose before building a Profile or issuing requests. The serialized DTO
    /// identifies compatible consumed bytes and policy; only a fresh point query
    /// by the borrowed source owner can establish an initialization answer.
    pub(crate) fn initialization_binding(
        &self,
        content: Hash,
        configuration: Option<ResidentInitializationConfiguration>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<CreatureInitializationSnapshot, CompositionError> {
        let poll = || -> Result<(), CompositionError> {
            check(&cancelled)?;
            if Instant::now() >= deadline {
                return Err(Error::Deadline.into());
            }
            Ok(())
        };
        poll()?;
        if content == [0; 32] {
            return Err(Error::Identity.into());
        }
        if configuration != Some(ResidentInitializationConfiguration::VmapHeightEnabled) {
            return Err(Error::Unsupported.into());
        }
        let fresh = self.verify_unchanged(|| cancelled() || Instant::now() >= deadline);
        poll()?;
        fresh?;
        let mut epoch = Sha256::new();
        epoch.update(POLICY);
        epoch.update(b"\0source\0");
        epoch.update(self.generated_query_source_epoch());
        let mut inventory = Sha256::new();
        inventory.update(POLICY);
        inventory.update(b"\0consumed-source\0");
        inventory.update(self.binding.map.to_le_bytes());
        inventory.update(self.binding.revision);
        inventory.update(self.generated_query_source_epoch());
        let mut config = Sha256::new();
        config.update(POLICY);
        config.update(b"\0configuration\0");
        config.update(CONFIGURATION);
        let result = CreatureInitializationSnapshot {
            map: self.binding.map,
            content,
            source_epoch: epoch.finalize().into(),
            inventory: inventory.finalize().into(),
            policy: Sha256::digest(POLICY).into(),
            effective_config: config.finalize().into(),
            vmap_height_enabled: true,
        };
        poll()?;
        Ok(result)
    }
}
