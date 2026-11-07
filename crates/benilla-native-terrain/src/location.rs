// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine terrain area inputs. Grid observations do not bypass WMO area precedence.
use crate::{
    source::CapturedSource,
    writer::{self, word, Dbc},
    PreparedTerrain, Tile,
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const CATALOGS: [&str; 3] = [
    "DBFilesClient\\AreaTable.dbc",
    "DBFilesClient\\WMOAreaTable.dbc",
    "DBFilesClient\\Map.dbc",
];
pub const POLICY: &str = "cmangos-8ec338a1-captured-grid-area-inputs-v1";

#[derive(Debug, Serialize)]
pub struct AreaGrid {
    pub tile: Tile,
    /// Original MCNK IDs in GridMap x-major cell order.
    pub area_ids: Vec<u32>,
    /// Extractor uint16 explore flags, not area IDs.
    pub area_flags: Vec<u16>,
    pub header_flags: u16,
    pub grid_area: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AreaRow {
    pub id: u32,
    pub map: u32,
    pub parent: u32,
    pub explore_flag: u32,
    pub name: String,
}
/// Only prepare can construct this retained immutable source product.
pub struct PreparedLocation {
    terrain: PreparedTerrain,
    catalogs: CapturedSource,
    grids: Vec<AreaGrid>,
    areas: BTreeMap<u32, AreaRow>,
    epoch: String,
}
impl std::fmt::Debug for PreparedLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedLocation")
            .field("source_epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

pub fn prepare(data: &Path, map: u32, tiles: &[Tile]) -> Result<PreparedLocation> {
    let terrain = crate::prepare(data, map, tiles)?;
    // Map and AreaTable reuse the terrain capture's immutable buffers.
    let catalogs = CapturedSource::capture(data, &[CATALOGS[1]])?;
    let mut grids = Vec::new();
    for (tile, bytes) in terrain.tiles() {
        let offset = word(bytes, 8)? as usize;
        ensure!(
            bytes.get(offset..offset + 4) == Some(b"AREA"),
            "location.area_header"
        );
        let header_flags = u16::from_le_bytes(bytes[offset + 4..offset + 6].try_into()?);
        let grid_area = u16::from_le_bytes(bytes[offset + 6..offset + 8].try_into()?);
        let flags = if header_flags & 1 != 0 {
            vec![grid_area; 256]
        } else {
            bytes
                .get(offset + 8..offset + 520)
                .context("location.area_grid")?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes(*b))
                .collect()
        };
        let adt = crate::adt_name(terrain.map_directory(), tile);
        grids.push(AreaGrid {
            tile,
            area_ids: writer::cell_area_ids(terrain.source.bytes(&adt)?)?,
            area_flags: flags,
            header_flags,
            grid_area,
        });
    }
    let areas = read_areas(terrain.source.bytes(CATALOGS[0])?)?;
    // Complete WMO store is retained for the existing source-location owner.
    let wmos = Dbc::read(catalogs.bytes(CATALOGS[1])?, 20)?;
    for row in wmos.rows() {
        for field in 11..=18 {
            wmos.string(word(row, field * 4)?)?;
        }
    }
    let mut hash = Sha256::new();
    hash.update(POLICY);
    hash.update(terrain.source_epoch());
    hash.update(catalogs.binding_digest());
    let result = PreparedLocation {
        terrain,
        catalogs,
        grids,
        areas,
        epoch: format!("{:x}", hash.finalize()),
    };
    result.verify_unchanged()?;
    Ok(result)
}
fn read_areas(bytes: &[u8]) -> Result<BTreeMap<u32, AreaRow>> {
    let dbc = Dbc::read(bytes, 25)?;
    let mut areas = BTreeMap::new();
    for row in dbc.rows() {
        let area = AreaRow {
            id: word(row, 0)?,
            map: word(row, 4)?,
            parent: word(row, 8)?,
            explore_flag: word(row, 12)?,
            name: dbc.string(word(row, 44)?)?.to_owned(),
        };
        ensure!(
            areas.insert(area.id, area).is_none(),
            "location.duplicate_area"
        );
    }
    Ok(areas)
}
fn parent_chain(areas: &BTreeMap<u32, AreaRow>, id: u32) -> Result<Vec<&AreaRow>> {
    let mut chain = Vec::new();
    let mut seen = BTreeSet::new();
    let mut next = id;
    loop {
        ensure!(seen.insert(next), "location.parent_cycle");
        let row = areas.get(&next).context("location.parent_missing")?;
        chain.push(row);
        if row.parent == 0 {
            return Ok(chain);
        }
        next = row.parent;
    }
}
impl PreparedLocation {
    pub fn terrain(&self) -> &PreparedTerrain {
        &self.terrain
    }
    pub fn catalog_bytes(&self) -> [&[u8]; 3] {
        [
            self.terrain
                .source
                .bytes(CATALOGS[0])
                .expect("captured AreaTable"),
            self.catalogs
                .bytes(CATALOGS[1])
                .expect("captured WMOAreaTable"),
            self.terrain
                .source
                .bytes(CATALOGS[2])
                .expect("captured Map"),
        ]
    }
    pub fn source_epoch(&self) -> &str {
        &self.epoch
    }
    pub fn grids(&self) -> &[AreaGrid] {
        &self.grids
    }
    pub fn parent_chain(&self, id: u32) -> Result<Vec<&AreaRow>> {
        parent_chain(&self.areas, id)
    }
    /// GridMap::getArea only. TerrainInfo's independent WMO override remains required.
    pub fn grid_area_flag(&self, x: f32, y: f32) -> Result<u16> {
        grid_area_flag(&self.grids, x, y)
    }
    pub fn verify_unchanged(&self) -> Result<()> {
        self.terrain.verify_unchanged()?;
        self.catalogs.verify_unchanged()
    }
    /// Fresh private export for independent readers, never a trusted snapshot loader.
    pub fn publish(&self, output: &Path) -> Result<()> {
        let parent = output
            .parent()
            .context("output.parent_must_exist")?
            .canonicalize()?;
        self.catalogs.check_output_parent(&parent)?;
        let output = parent.join(output.file_name().context("output.directory_name")?);
        ensure!(!output.try_exists()?, "output.already_exists");
        self.verify_unchanged()?;
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&output)?;
        for (name, bytes) in CATALOGS.into_iter().zip(self.catalog_bytes()) {
            crate::write_new(&output.join(name.rsplit('\\').next().unwrap()), bytes)?;
        }
        for (tile, bytes) in self.terrain.tiles() {
            crate::write_new(
                &output.join(format!(
                    "{:03}{:02}{:02}.map",
                    self.terrain.map_id(),
                    tile.x,
                    tile.y
                )),
                bytes,
            )?;
        }
        for (index, member) in self.terrain.source.members().iter().enumerate() {
            crate::write_new(
                &output.join(format!("terrain-source-{index:04}.bin")),
                self.terrain.source.bytes(&member.name)?,
            )?;
        }
        self.verify_unchanged()?;
        let manifest = serde_json::json!({"format":"benilla-source-location-inputs-v1", "source_pin":crate::SOURCE_PIN, "source_policy":self.terrain.source_policy(), "policy":POLICY, "map":self.terrain.map_id(), "source_epoch":self.epoch, "terrain_epoch":self.terrain.source_epoch(), "catalog_epoch":self.catalogs.binding_digest(), "terrain_members":self.terrain.source.members(), "terrain_absent_members":self.terrain.source.absent_members(), "catalog_members":self.catalogs.members(), "grids":self.grids, "areas":self.areas.values().collect::<Vec<_>>(), "wmo_precedence_required":true, "live_install_lease":false, "gameplay_admitted":false});
        crate::write_new(
            &output.join("location.json"),
            &serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(())
    }
}
fn grid_area_flag(grids: &[AreaGrid], x: f32, y: f32) -> Result<u16> {
    ensure!(
        x.is_finite() && y.is_finite(),
        "location.nonfinite_position"
    );
    let gx = 32.0 - x / (1600.0_f32 / 3.0);
    let gy = 32.0 - y / (1600.0_f32 / 3.0);
    ensure!(
        (0.0..64.0).contains(&gx) && (0.0..64.0).contains(&gy),
        "location.outside_capture"
    );
    let tile = Tile {
        x: gx.floor() as u8,
        y: gy.floor() as u8,
    };
    let grid = grids
        .iter()
        .find(|grid| grid.tile == tile)
        .context("location.outside_capture")?;
    let lx = ((16.0 * gx) as usize) & 15;
    let ly = ((16.0 * gy) as usize) & 15;
    Ok(grid.area_flags[lx * 16 + ly])
}

#[cfg(test)]
mod tests;
