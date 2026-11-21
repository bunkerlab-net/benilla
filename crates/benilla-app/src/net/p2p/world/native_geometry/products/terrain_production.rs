// SPDX-License-Identifier: GPL-3.0-or-later
//! Terrain-only consumer of captured source bytes and the exact existing GridMap reader.
//! This cannot construct an inventory, static-world closure, or gameplay authority.

use super::{Error, TerrainTile, TileId};
use benilla_native_terrain::PreparedTerrain;
use std::{collections::BTreeMap, path::Path};

/// Retains the immutable source capture alongside eagerly decoded generated tiles.
/// A successful source recheck is an observation, never a live installation lease.
pub(crate) struct SourceTerrainProducts {
    prepared: PreparedTerrain,
    tiles: BTreeMap<TileId, TerrainTile>,
}

impl SourceTerrainProducts {
    pub(crate) fn from_prepared(prepared: PreparedTerrain) -> Result<Self, Error> {
        let mut tiles = BTreeMap::new();
        for (tile, bytes) in prepared.tiles() {
            let id = TileId {
                x: tile.x,
                y: tile.y,
            };
            let decoded = TerrainTile::decode(id, bytes)?;
            if tiles.insert(id, decoded).is_some() {
                return Err(Error::Duplicate);
            }
        }
        if tiles.is_empty() {
            return Err(Error::Missing);
        }
        // Unavailable, changed or unresolvable source inputs all reject consumption.
        prepared.verify_unchanged().map_err(|_| Error::Io)?;
        Ok(Self { prepared, tiles })
    }

    pub(crate) fn source_epoch(&self) -> &str {
        self.prepared.source_epoch()
    }

    pub(crate) fn terrain(&self, map: u32, tile: TileId) -> Result<&TerrainTile, Error> {
        if map != self.prepared.map_id() {
            return Err(Error::Identity);
        }
        tile.validate()?;
        self.tiles.get(&tile).ok_or(Error::Missing)
    }

    /// Export only the bytes already decoded above; publication rechecks the source
    /// again and never grants VMap, MMap, corridor or session admission.
    pub(crate) fn publish(self, output: &Path) -> Result<(), Error> {
        self.prepared.publish(output).map_err(|_| Error::Io)
    }
}

#[cfg(test)]
#[path = "../../../../../../../benilla-native-geometry-tests/src/terrain_production_tests.rs"]
mod tests;
