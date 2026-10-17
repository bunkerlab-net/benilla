//! Full-map raw placement/dependency inventory. A resident guard alone cannot rule out
//! unloaded exterior hulls. Read-only, bounded and constructed off-thread before admission.
use super::{adt_to_tile_mesh, native_static, Doodad, WmoInstance};
use crate::{
    native_m2_collision, Chain, NativeM2Collision, NativeWmoCollision, NativeWmoGroup,
    NativeWmoRoot, WdtReader, WowVersion,
};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Cursor, sync::Arc};

/// Immutable dependency closure of a WMO root, all groups and all MODD M2 sources (including
/// inactive sets). A missing dependency never produces an empty successful entry.
pub struct NativeWmoMapModel {
    source: [u8; 32],
    collision: NativeWmoCollision,
    doodads: Vec<Arc<NativeM2Collision>>,
}
impl NativeWmoMapModel {
    pub fn source_digest(&self) -> [u8; 32] {
        self.source
    }
    pub fn collision(&self) -> &NativeWmoCollision {
        &self.collision
    }
    pub fn doodad(&self, index: usize) -> Option<&NativeM2Collision> {
        self.doodads.get(index).map(AsRef::as_ref)
    }
}
/// Historical name retained for callers; this now certifies ADT M2 and dry static WMO inputs.
/// Global-WMO maps and liquid-bearing WMO groups remain outside the supported subset.
pub struct NativeM2MapInventory {
    directory: String,
    wdt: [u8; 32],
    digest: [u8; 32],
    tiles: BTreeMap<(i32, i32), [u8; 32]>,
    placements: BTreeMap<u32, (Doodad, Arc<NativeM2Collision>)>,
    wmos: BTreeMap<u32, (WmoInstance, Arc<NativeWmoMapModel>)>,
}
impl NativeM2MapInventory {
    pub fn load(chain: &mut Chain, directory: &str) -> Result<Self> {
        Self::read(directory, |path| {
            let archive = chain.archive_for(path)?;
            let limit = if path.ends_with(".wdt") {
                1024 * 1024
            } else {
                64 * 1024 * 1024
            };
            if archive
                .file_size(path)
                .context("missing inventory member size")?
                > limit
            {
                bail!("native inventory member size limit: {path}");
            }
            archive
                .read_file(path)
                .with_context(|| format!("reading native inventory member {path}"))
        })
    }
    pub fn directory(&self) -> &str {
        &self.directory
    }
    pub fn wdt_digest(&self) -> [u8; 32] {
        self.wdt
    }
    pub fn source_digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn tile_digest(&self, tile: (i32, i32)) -> Option<[u8; 32]> {
        self.tiles.get(&tile).copied()
    }
    pub fn placements(&self) -> impl Iterator<Item = (&Doodad, &NativeM2Collision)> {
        self.placements.values().map(|(d, c)| (d, c.as_ref()))
    }
    pub fn wmo_placements(&self) -> impl Iterator<Item = (&WmoInstance, &NativeWmoMapModel)> {
        self.wmos.values().map(|(d, c)| (d, c.as_ref()))
    }
    pub fn wmo(&self, id: u32) -> Option<(&WmoInstance, &NativeWmoMapModel)> {
        self.wmos.get(&id).map(|(d, c)| (d, c.as_ref()))
    }
    fn read(directory: &str, mut read: impl FnMut(&str) -> Result<Vec<u8>>) -> Result<Self> {
        if directory.is_empty()
            || directory.len() > 128
            || !directory
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            bail!("invalid map directory");
        }
        // Bound total I/O as well as retained collision. This is a conservative local subset,
        // not permission to load an arbitrarily large map/dependency graph in one job.
        let mut bytes_read = 0u64;
        let mut read = |path: &str| -> Result<Vec<u8>> {
            let b = read(path)?;
            let max = if path.ends_with(".wdt") {
                1024 * 1024
            } else {
                64 * 1024 * 1024
            };
            if b.len() > max {
                bail!("inventory member size limit: {path}");
            }
            bytes_read = bytes_read
                .checked_add(b.len() as u64)
                .context("inventory byte overflow")?;
            if bytes_read > 16u64 * 1024 * 1024 * 1024 {
                bail!("inventory total byte limit");
            }
            Ok(b)
        };
        let wdt_bytes = read(&format!("World\\Maps\\{directory}\\{directory}.wdt"))?;
        let wdt = WdtReader::new(Cursor::new(&wdt_bytes), WowVersion::Classic).read()?;
        if !wdt.native_terrain_coverage_safe() {
            bail!("unsupported raw map index");
        }
        let wdt_digest = Sha256::digest(&wdt_bytes).into();
        let mut hash = Sha256::new();
        hash.update(b"benilla-native-complete-static-map-v2\0");
        hash.update((directory.len() as u32).to_le_bytes());
        hash.update(directory.as_bytes());
        hash.update(wdt_digest);
        let mut result = Self {
            directory: directory.into(),
            wdt: wdt_digest,
            digest: [0; 32],
            tiles: BTreeMap::new(),
            placements: BTreeMap::new(),
            wmos: BTreeMap::new(),
        };
        let mut models = BTreeMap::new();
        let mut wmos: BTreeMap<String, Arc<NativeWmoMapModel>> = BTreeMap::new();
        let mut budget = Budget::default();
        for y in 0..64 {
            for x in 0..64 {
                if !wdt.get_tile(x, y).is_some_and(|t| t.has_adt) {
                    continue;
                }
                let path = format!("World\\Maps\\{directory}\\{directory}_{x}_{y}.adt");
                let bytes =
                    read(&path).with_context(|| format!("incomplete map inventory: {path}"))?;
                if !native_static::complete_with_liquids(&bytes, true) {
                    bail!("unproven map placement inventory: {path}");
                }
                let tile = adt_to_tile_mesh(&bytes)?;
                result
                    .tiles
                    .insert((x as i32, y as i32), tile.native_source_digest);
                hash.update(b"tile\0");
                hash.update((x as u32).to_le_bytes());
                hash.update((y as u32).to_le_bytes());
                hash.update(tile.native_source_digest);
                for d in tile.doodads {
                    let model = crate::models::model_path(&d.model).replace('/', "\\");
                    if result.wmos.contains_key(&d.unique_id) {
                        bail!("M2/WMO placement ID collision");
                    }
                    if let Some((prior, _)) = result.placements.get(&d.unique_id) {
                        if crate::models::model_path(&prior.model).replace('/', "\\") != model
                            || prior.position != d.position
                            || prior.rotation != d.rotation
                            || prior.scale != d.scale
                        {
                            bail!("conflicting placement ID {}", d.unique_id);
                        }
                        continue;
                    }
                    if result.placements.len() + result.wmos.len() >= 262_144 {
                        bail!("map placement inventory limit");
                    }
                    let collision = load_m2(&model, &mut models, &mut read, &mut budget)?;
                    budget.placed(collision.hull().positions.len())?;
                    hash.update(b"m2\0");
                    hash.update(d.unique_id.to_le_bytes());
                    hash.update(collision.source_digest());
                    result.placements.insert(d.unique_id, (d, collision));
                }
                for w in tile.wmos {
                    let model = w.model.to_ascii_lowercase().replace('/', "\\");
                    if result.placements.contains_key(&w.unique_id) {
                        bail!("M2/WMO placement ID collision");
                    }
                    if let Some((prior, _)) = result.wmos.get(&w.unique_id) {
                        if prior.model.to_ascii_lowercase().replace('/', "\\") != model
                            || prior.position != w.position
                            || prior.rotation != w.rotation
                            || prior.doodad_set != w.doodad_set
                            || prior.name_set != w.name_set
                        {
                            bail!("conflicting WMO placement ID {}", w.unique_id);
                        }
                        continue;
                    }
                    if result.placements.len() + result.wmos.len() >= 262_144 {
                        bail!("map placement inventory limit");
                    }
                    if !wmos.contains_key(&model) {
                        if wmos.len() >= 4096 {
                            bail!("map WMO root limit");
                        }
                        let root = NativeWmoRoot::read(&read(&model)?)
                            .context("invalid WMO root inventory")?;
                        budget.records +=
                            root.group_count() + root.doodads().len() + root.doodad_sets().len();
                        if budget.records > 262_144 {
                            bail!("map WMO source record limit");
                        }
                        let mut groups = Vec::new();
                        let stem = model.strip_suffix(".wmo").context("invalid WMO path")?;
                        for index in 0..root.group_count() {
                            let path = format!("{stem}_{index:03}.wmo");
                            let group = NativeWmoGroup::read(&read(&path)?, &root, index)
                                .with_context(|| format!("invalid WMO group inventory: {path}"))?;
                            budget.references += group.doodad_refs().len();
                            if budget.references > 1_048_576 {
                                bail!("map WMO reference limit");
                            }
                            budget.mesh(group.walking())?;
                            budget.mesh(group.camera())?;
                            groups.push(group);
                        }
                        let collision = NativeWmoCollision::from_parts(root, groups)
                            .context("incomplete WMO source closure")?;
                        let mut h = Sha256::new();
                        h.update(b"benilla-native-wmo-dependencies-v1\0");
                        h.update(collision.source_digest());
                        let mut doodads = Vec::new();
                        for d in collision.root().doodads() {
                            let path = crate::models::model_path(&d.model).replace('/', "\\");
                            let source = load_m2(&path, &mut models, &mut read, &mut budget)?;
                            h.update((path.len() as u32).to_le_bytes());
                            h.update(path.as_bytes());
                            h.update(source.source_digest());
                            doodads.push(source);
                        }
                        wmos.insert(
                            model.clone(),
                            Arc::new(NativeWmoMapModel {
                                source: h.finalize().into(),
                                collision,
                                doodads,
                            }),
                        );
                    }
                    let source = &wmos[&model];
                    let selected = source
                        .collision
                        .root()
                        .selected_doodads(w.doodad_set)
                        .context("invalid WMO doodad set")?;
                    budget.parts += selected.len() + 2;
                    if budget.parts > 1_048_576 {
                        bail!("map WMO placement part limit");
                    }
                    budget.placed(
                        source.collision.walking().positions.len()
                            + source.collision.camera().positions.len(),
                    )?;
                    for di in selected {
                        budget.placed(source.doodads[di].hull().positions.len())?;
                    }
                    hash.update(b"wmo\0");
                    hash.update(w.unique_id.to_le_bytes());
                    hash.update(source.source_digest());
                    result.wmos.insert(w.unique_id, (w, source.clone()));
                }
            }
        }
        if result.tiles.is_empty() {
            bail!("empty map inventory");
        }
        result.digest = hash.finalize().into();
        Ok(result)
    }
}
#[derive(Default)]
struct Budget {
    records: usize,
    references: usize,
    parts: usize,
    vertices: usize,
    triangles: usize,
    placement_vertices: usize,
}
impl Budget {
    fn mesh(&mut self, m: &crate::CollisionMesh) -> Result<()> {
        self.vertices += m.positions.len();
        self.triangles += m.indices.len() / 3;
        if self.vertices > 2_097_152 || self.triangles > 2_097_152 {
            bail!("map collision inventory limit");
        }
        Ok(())
    }
    fn placed(&mut self, n: usize) -> Result<()> {
        self.placement_vertices += n;
        if self.placement_vertices > 16_777_216 {
            bail!("map placement transform limit");
        }
        Ok(())
    }
}
fn load_m2(
    path: &str,
    models: &mut BTreeMap<String, Arc<NativeM2Collision>>,
    read: &mut impl FnMut(&str) -> Result<Vec<u8>>,
    budget: &mut Budget,
) -> Result<Arc<NativeM2Collision>> {
    if !models.contains_key(path) {
        if models.len() >= 16_384 {
            bail!("map M2 source limit");
        }
        let c = native_m2_collision(&read(path)?)
            .with_context(|| format!("invalid M2 inventory collision: {path}"))?;
        budget.mesh(c.hull())?;
        models.insert(path.into(), Arc::new(c));
    }
    Ok(models[path].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn chunk(magic: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = magic.to_vec();
        v.extend((data.len() as u32).to_le_bytes());
        v.extend(data);
        v
    }
    pub(super) fn wdt() -> Vec<u8> {
        let mut b = chunk(b"REVM", &18u32.to_le_bytes());
        b.extend(chunk(b"DHPM", &[0; 32]));
        let mut grid = vec![0; 64 * 64 * 8];
        grid[(32 * 64 + 32) * 8] = 1;
        b.extend(chunk(b"NIAM", &grid));
        b
    }
    #[test]
    fn inventory_requires_every_wdt_tile_and_binds_its_bytes() {
        let terrain = super::super::native_coverage_tests::source();
        let make = |terrain: Vec<u8>| {
            NativeM2MapInventory::read("Test", |path| {
                if path.ends_with(".wdt") {
                    Ok(wdt())
                } else {
                    Ok(terrain.clone())
                }
            })
        };
        let first = make(terrain.clone()).unwrap();
        assert_eq!(first.placements().count(), 0);
        assert!(first.tile_digest((32, 32)).is_some());
        let mut changed = terrain;
        changed.extend(chunk(b"XETM", b"texture\0"));
        assert_ne!(
            first.source_digest(),
            make(changed).unwrap().source_digest()
        );
        assert!(NativeM2MapInventory::read("Test", |path| {
            if path.ends_with(".wdt") {
                Ok(wdt())
            } else {
                bail!("missing tile")
            }
        })
        .is_err());
    }
    #[test]
    fn inventory_does_not_silently_drop_unsupported_exterior_wmo() {
        let mut terrain = super::super::native_coverage_tests::source();
        terrain.extend(chunk(b"FDOM", &[0; 64]));
        assert!(NativeM2MapInventory::read("Test", |path| {
            if path.ends_with(".wdt") {
                Ok(wdt())
            } else {
                Ok(terrain.clone())
            }
        })
        .is_err());
    }
}

#[cfg(test)]
mod model_tests {
    use super::tests::{chunk, wdt};
    use super::*;
    fn placed() -> Vec<u8> {
        let mut b = super::super::native_coverage_tests::source();
        b.extend(chunk(b"XDMM", b"World\\Tree.m2\0"));
        b.extend(chunk(b"DIMM", &0u32.to_le_bytes()));
        let mut p = [0; 36];
        p[4..8].copy_from_slice(&42u32.to_le_bytes());
        p[32..34].copy_from_slice(&1024u16.to_le_bytes());
        b.extend(chunk(b"FDDM", &p));
        let offset = super::super::native_coverage_tests::append_first_chunk(
            &mut b,
            &chunk(b"FRCM", &0u32.to_le_bytes()),
        );
        b[8 + 0x10..8 + 0x14].copy_from_slice(&1u32.to_le_bytes());
        b[8 + 0x20..8 + 0x24].copy_from_slice(&offset.to_le_bytes());
        b
    }
    fn empty_model() -> Vec<u8> {
        let mut b = vec![0; 0x144];
        b[..4].copy_from_slice(b"MD20");
        b[4..8].copy_from_slice(&256u32.to_le_bytes());
        b
    }
    #[test]
    fn missing_or_malformed_m2_is_not_a_complete_map_inventory() {
        for malformed in [false, true] {
            let result = NativeM2MapInventory::read("Test", |path| {
                if path.ends_with(".wdt") {
                    Ok(wdt())
                } else if path.ends_with(".adt") {
                    Ok(placed())
                } else if malformed {
                    Ok(vec![0; 16])
                } else {
                    bail!("missing model")
                }
            });
            assert!(result.is_err());
        }
    }
    #[test]
    fn exact_model_source_replacement_changes_full_map_revision() {
        let make = |model: Vec<u8>| {
            NativeM2MapInventory::read("Test", |path| {
                if path.ends_with(".wdt") {
                    Ok(wdt())
                } else if path.ends_with(".adt") {
                    Ok(placed())
                } else {
                    Ok(model.clone())
                }
            })
        };
        let a = make(empty_model()).unwrap();
        assert_eq!(a.placements().count(), 1);
        let mut model = empty_model();
        model[0x140] = 1;
        assert_ne!(a.source_digest(), make(model).unwrap().source_digest());
    }
}

#[cfg(test)]
mod wmo_tests {
    use super::tests::{chunk, wdt};
    use super::*;
    use crate::models::native_wmo::tests::{group, root};
    fn tile() -> Vec<u8> {
        let mut b = super::super::native_coverage_tests::source();
        b.extend(chunk(b"OMWM", b"World\\House.wmo\0"));
        b.extend(chunk(b"DIWM", &0u32.to_le_bytes()));
        let mut p = vec![0; 64];
        p[4..8].copy_from_slice(&84u32.to_le_bytes());
        b.extend(chunk(b"FDOM", &p));
        let offset = super::super::native_coverage_tests::append_first_chunk(
            &mut b,
            &chunk(b"FRCM", &0u32.to_le_bytes()),
        );
        b[8 + 0x38..8 + 0x3c].copy_from_slice(&1u32.to_le_bytes());
        b[8 + 0x20..8 + 0x24].copy_from_slice(&offset.to_le_bytes());
        b
    }
    fn m2() -> Vec<u8> {
        let mut b = vec![0; 0x144];
        b[..4].copy_from_slice(b"MD20");
        b[4..8].copy_from_slice(&256u32.to_le_bytes());
        b
    }
    fn load(
        mut member: impl FnMut(&str, Vec<u8>) -> Result<Vec<u8>>,
    ) -> Result<NativeM2MapInventory> {
        NativeM2MapInventory::read("Test", |path| {
            let b = if path.ends_with(".wdt") {
                wdt()
            } else if path.ends_with(".adt") {
                tile()
            } else if path.ends_with("_000.wmo") {
                group(true)
            } else if path.ends_with(".wmo") {
                root(true)
            } else if path == "world\\chair.m2" {
                m2()
            } else {
                bail!("unexpected source {path}");
            };
            member(path, b)
        })
    }
    #[test]
    fn full_map_inventory_accepts_exact_wmo_root_group_and_doodad_closure() {
        let source = load(|_, b| Ok(b)).unwrap();
        assert_eq!(source.placements().count(), 0);
        assert_eq!(source.wmo_placements().count(), 1);
        let (w, m) = source.wmo(84).unwrap();
        assert_eq!(w.doodad_set, 0);
        assert!(m.doodad(0).unwrap().matches(None));
        let t = adt_to_tile_mesh(&tile()).unwrap();
        assert!(t.native_static_coverage_safe);
        assert!(!t.native_coverage_safe);
        assert_eq!(t.wmos.len(), 1);
        for suffix in ["house.wmo", "_000.wmo", "chair.m2"] {
            let changed = load(|path, mut b| {
                if path.ends_with(suffix) {
                    if suffix.ends_with("m2") {
                        b[0x140] = 1;
                    } else {
                        b.extend(chunk(b"TEST", &[1]));
                    }
                }
                Ok(b)
            })
            .unwrap();
            assert_ne!(source.source_digest(), changed.source_digest());
        }
    }
    #[test]
    fn missing_failed_or_malformed_wmo_dependency_never_means_empty() {
        for suffix in ["house.wmo", "_000.wmo", "chair.m2"] {
            assert!(load(|path, b| if path.ends_with(suffix) {
                bail!("missing dependency")
            } else {
                Ok(b)
            })
            .is_err());
            assert!(load(|path, b| if path.ends_with(suffix) {
                Ok(vec![0; 16])
            } else {
                Ok(b)
            })
            .is_err());
        }
        assert!(load(|path, mut b| {
            if path.ends_with(".adt") {
                let at = b.windows(4).rposition(|v| v == b"FDOM").unwrap() + 8;
                b[at + 58..at + 60].copy_from_slice(&1u16.to_le_bytes());
            }
            Ok(b)
        })
        .is_err());
    }
    #[test]
    fn wmo_raw_adt_rejects_dangling_partial_flags_and_duplicate_ids() {
        let b = tile();
        for magic in [b"OMWM", b"DIWM", b"FDOM"] {
            let mut bad = b.clone();
            bad.extend(chunk(magic, &[]));
            assert!(!native_static::complete(&bad));
        }
        for (magic, offset, n, value) in [
            (b"FDOM", 0, 4, 1u32),
            (b"FDOM", 56, 2, 1),
            (b"FDOM", 62, 2, 1024),
            (b"FRCM", 0, 4, 1),
        ] {
            let mut bad = b.clone();
            let pos = bad.windows(4).rposition(|v| v == magic).unwrap() + 8 + offset;
            bad[pos..pos + n].copy_from_slice(&value.to_le_bytes()[..n]);
            assert!(!native_static::complete(&bad));
        }
        let mut bad = b.clone();
        bad[8 + 0x38..8 + 0x3c].fill(0);
        assert!(!native_static::complete(&bad));
        let mut bad = b;
        let pos = bad.windows(4).rposition(|v| v == b"DIWM").unwrap() + 8;
        bad[pos..pos + 4].copy_from_slice(&1u32.to_le_bytes());
        assert!(!native_static::complete(&bad));
    }
    #[test]
    fn conflicting_cross_tile_wmo_doodad_set_is_rejected() {
        assert!(load(|path, mut b| {
            if path.ends_with(".wdt") {
                let at = b.windows(4).position(|v| v == b"NIAM").unwrap() + 8;
                b[at + (32 * 64 + 33) * 8] = 1;
            }
            if path.ends_with("_33_32.adt") {
                let at = b.windows(4).rposition(|v| v == b"FDOM").unwrap() + 8;
                b[at + 58..at + 60].copy_from_slice(&1u16.to_le_bytes());
            }
            Ok(b)
        })
        .is_err());
    }
}
