// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine terrain/collision captures into the existing source area owner.
use super::super::{
    source_height, vmap::tile_prepared::PreparedTileCollision, TerrainTile, TileId,
};
use super::*;
use benilla_native_terrain::location::PreparedLocation;
use benilla_native_vmap_source::AssembledSourceMap;
use std::collections::BTreeMap;

const CAPTURE_POLICY: &[u8] = b"benilla-genuine-resident-source-location-v1";
impl TerrainAreaSnapshot<'static> {
    pub(crate) fn from_source(
        capture: PreparedLocation,
        collision: &AssembledSourceMap<'_>,
        content: Hash,
    ) -> Result<Self, Error> {
        if capture.terrain().map_id() != collision.map().map_id() {
            return Err(Error::Identity);
        }
        capture.verify_unchanged().map_err(|_| Error::Io)?;
        collision
            .verify_unchanged(|| false)
            .map_err(|_| Error::Io)?;
        source_height::compatible_receipts(capture.terrain(), collision, &|| false).map_err(
            |error| match error {
                source_height::CompositionError::Validation(error) => error,
                source_height::CompositionError::Cancelled => Error::Deadline,
                _ => Error::Io,
            },
        )?;
        let mut tiles = BTreeMap::new();
        let mut inventory = Sha256::new();
        inventory.update(CAPTURE_POLICY);
        inventory.update(capture.source_epoch());
        for (tile, bytes) in capture.terrain().tiles() {
            let id = TileId {
                x: tile.x,
                y: tile.y,
            };
            tiles.insert(id, TerrainTile::decode(id, bytes)?);
            inventory.update([tile.x, tile.y]);
            inventory.update((bytes.len() as u64).to_le_bytes());
            inventory.update(bytes);
        }
        let selected: Vec<_> = tiles.keys().copied().collect();
        let raw = PreparedTileCollision::from_source(collision, &selected, || false)
            .map_err(|error| match error {
                super::super::vmap::prepared::PrepareError::Validation(error) => error,
                super::super::vmap::prepared::PrepareError::Cancelled => Error::Deadline,
                super::super::vmap::prepared::PrepareError::Source(_) => Error::Io,
            })?
            .into_area()?;
        inventory.update(raw.binding().inventory);
        let [area_bytes, wmo_bytes, map_bytes] = capture.catalog_bytes();
        let areas = AreaTable::decode(area_bytes)?;
        let wmo_areas = WmoAreaTable::decode(wmo_bytes)?;
        let maps = MapTable::decode(map_bytes)?;
        let location_catalog = SourceLocationCatalog::decode(area_bytes, wmo_bytes, map_bytes)
            .map_err(|_| Error::Format)?;
        let mut epoch = Sha256::new();
        epoch.update(CAPTURE_POLICY);
        epoch.update(capture.source_epoch());
        epoch.update(collision.map().source_binding());
        let mut policy = Sha256::new();
        policy.update(CAPTURE_POLICY);
        policy.update(raw.binding().policy);
        policy.update(areas.digest());
        policy.update(wmo_areas.digest());
        policy.update(maps.digest());
        capture.verify_unchanged().map_err(|_| Error::Io)?;
        collision
            .verify_unchanged(|| false)
            .map_err(|_| Error::Io)?;
        Ok(Self {
            binding: SnapshotBinding {
                map: capture.terrain().map_id(),
                content,
                source_epoch: epoch.finalize().into(),
                inventory: inventory.finalize().into(),
                policy: policy.finalize().into(),
            },
            products: AreaProducts::Captured {
                source: capture,
                tiles,
            },
            raw,
            areas,
            wmo_areas,
            maps,
            location_catalog,
        })
    }
}

#[cfg(test)]
mod tests;
