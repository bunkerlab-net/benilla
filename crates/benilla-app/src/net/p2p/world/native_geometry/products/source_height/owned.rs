// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only immutable ownership prerequisite, not a resident Service or authority.
use super::{CompositionError, PreparedSourceHeight};
use benilla_native_terrain::PreparedTerrain;
use benilla_native_vmap_source::AssembledSourceMap;

pub(crate) struct OwnedSourceProducts {
    terrain: PreparedTerrain,
    collision: AssembledSourceMap<'static>,
}
impl OwnedSourceProducts {
    pub(crate) fn new(terrain: PreparedTerrain, collision: AssembledSourceMap<'static>) -> Self {
        Self { terrain, collision }
    }

    /// The reader cannot escape its scope. Recheck the exact sources before returning
    /// any successful result; immutable captured bytes are not a live source lease.
    pub(crate) fn with_height<T>(
        &self,
        cancelled: impl Fn() -> bool,
        read: impl for<'view> FnOnce(
            &PreparedSourceHeight<'view, 'view, 'static>,
        ) -> Result<T, CompositionError>,
    ) -> Result<T, CompositionError> {
        let height = PreparedSourceHeight::from_source(&self.terrain, &self.collision, &cancelled)?;
        let result = read(&height)?;
        height.verify_unchanged(&cancelled)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
