// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only immutable ownership prerequisite, not a resident Service or authority.
use super::super::{terrain::TerrainTile, vmap::tile_prepared::OwnedTileCollision, TileId};
use super::{prepare_terrain, Binding, CompositionError, PreparedSourceHeight, TerrainStorage};
use benilla_native_terrain::PreparedTerrain;
use benilla_native_vmap_source::AssembledSourceMap;
use std::collections::BTreeMap;

pub(crate) struct OwnedSourceProducts {
    terrain: PreparedTerrain,
    collision: OwnedTileCollision,
    tiles: BTreeMap<TileId, TerrainTile>,
    binding: Binding,
}
impl OwnedSourceProducts {
    /// Prepare once against these exact captures, then move the non-cloneable decoded
    /// payloads beside them. No constructor accepts detached bindings or decoded data.
    pub(crate) fn new(
        terrain: PreparedTerrain,
        collision: AssembledSourceMap<'static>,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, CompositionError> {
        let decoded = prepare_terrain(&terrain, &collision, &cancelled)?;
        let collision = OwnedTileCollision::from_source(collision, &decoded.selected, &cancelled)
            .map_err(CompositionError::Collision)?;
        let prepared =
            PreparedSourceHeight::finish(&terrain, collision.view(), decoded, &cancelled)?;
        let PreparedSourceHeight { tiles, binding, .. } = prepared;
        let TerrainStorage::Owned(tiles, _) = tiles else {
            unreachable!("from_source owns its decoded terrain")
        };
        Ok(Self {
            terrain,
            collision,
            tiles,
            binding,
        })
    }

    /// Borrow the retained decoded products without rebuilding them. Recheck exact
    /// sources before and after the reader; immutable captured bytes are not a live
    /// source lease. These verifications still perform synchronous MPQ reads.
    pub(crate) fn with_height<T>(
        &self,
        cancelled: impl Fn() -> bool,
        read: impl for<'view> FnOnce(
            &PreparedSourceHeight<'view, 'view, 'static>,
        ) -> Result<T, CompositionError>,
    ) -> Result<T, CompositionError> {
        let height = PreparedSourceHeight {
            terrain: &self.terrain,
            collision: self.collision.view(),
            tiles: TerrainStorage::Borrowed(&self.tiles),
            binding: self.binding,
        };
        height.verify_unchanged(&cancelled)?;
        let result = read(&height)?;
        height.verify_unchanged(&cancelled)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
