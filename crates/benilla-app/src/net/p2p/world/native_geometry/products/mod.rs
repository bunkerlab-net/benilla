// SPDX-License-Identifier: GPL-3.0-or-later
//! Immutable generated-product inventory; optional validated readers never grant authority.
//! Caller-supplied evidence is bound for consistency, not authenticated as provenance.
//! Production entry and non-render participant admission remain independent and closed.
#![expect(
    dead_code,
    reason = "native production session admission remains closed"
)]
mod envelopes;
pub(crate) mod height;
pub(crate) mod liquid;
pub(crate) mod navigation;
mod terrain;
pub(crate) mod terrain_liquid;
pub(crate) mod vmap;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::Path,
};
pub(crate) use terrain::TerrainTile;

pub(crate) type Hash = [u8; 32];
pub(crate) const SOURCE_PIN: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
pub(crate) const DETOUR_TREE: &str = "9694f7477cd03d3838322160833b2c995f6e5686";
const MAX_PRODUCTS: usize = 4096;
const MAX_TILES: usize = 64;
const MAX_EDGES: usize = 16_384;
const MAX_PRODUCT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const POLICY: &[u8] = b"benilla-native-product-inventory-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Budget,
    Identity,
    Missing,
    Unexpected,
    Duplicate,
    Dependency,
    Profile,
    Framing,
    Format,
    Unsupported,
    Hash,
    Io,
    OutsideTile,
}

/// A terrain point result, never composite height or continuous body support.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PointHeight {
    Height(f32),
    Hole,
}

/// GridMap/TerrainInfo grid coordinates, not ADT filename order or Detour tile indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TileId {
    pub x: u8,
    pub y: u8,
}
impl TileId {
    fn validate(self) -> Result<(), Error> {
        if self.x >= 64 || self.y >= 64 {
            return Err(Error::Identity);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ProductId {
    Terrain(TileId),
    VmapTree,
    VmapTile(TileId),
    VmapModel(String),
    MmapParams,
    MmapTile(TileId),
}
impl ProductId {
    pub(crate) fn filename(&self, map: u32) -> Result<String, Error> {
        if map > 999 {
            return Err(Error::Identity);
        }
        Ok(match self {
            Self::Terrain(tile) | Self::MmapTile(tile) | Self::VmapTile(tile) => {
                tile.validate()?;
                match self {
                    Self::Terrain(_) => format!("maps/{map:03}{:02}{:02}.map", tile.x, tile.y),
                    Self::MmapTile(_) => format!("mmaps/{map:03}{:02}{:02}.mmtile", tile.x, tile.y),
                    // StaticMapTree::getTileFileName reverses TerrainInfo's grid pair.
                    Self::VmapTile(_) => {
                        format!("vmaps/{map:03}_{:02}_{:02}.vmtile", tile.y, tile.x)
                    }
                    _ => unreachable!(),
                }
            }
            Self::VmapTree => format!("vmaps/{map:03}.vmtree"),
            Self::MmapParams => format!("mmaps/{map:03}.mmap"),
            Self::VmapModel(name) => {
                if name.len() > 160
                    || name.len() <= 4
                    || !name.ends_with(".vmo")
                    || name.starts_with('.')
                    || name.contains("..")
                    || !name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
                {
                    return Err(Error::Identity);
                }
                format!("vmaps/{name}")
            }
        })
    }
}

/// Serialized native ABI envelope, NOT evidence that a binary actually used these flags.
/// Only the pinned little-endian 64-bit build profile is recognized in this prerequisite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Compatibility {
    pub source_pin: String,
    pub detour_tree: String,
    pub little_endian: bool,
    pub pointer_bytes: u8,
    pub poly_ref_bytes: u8,
    pub float_bytes: u8,
    pub map_header_bytes: u16,
    pub mmap_params_bytes: u16,
    pub mmap_tile_header_bytes: u16,
    pub detour_header_bytes: u16,
    pub detour_poly_bytes: u16,
    pub detour_link_bytes: u16,
    pub playerbots: bool,
    pub float_terrain: bool,
    pub small_vmap: bool,
}
impl Compatibility {
    pub(crate) fn pinned() -> Self {
        Self {
            source_pin: SOURCE_PIN.into(),
            detour_tree: DETOUR_TREE.into(),
            little_endian: true,
            pointer_bytes: 8,
            poly_ref_bytes: 8,
            float_bytes: 4,
            map_header_bytes: 40,
            mmap_params_bytes: 28,
            mmap_tile_header_bytes: 20,
            detour_header_bytes: 100,
            detour_poly_bytes: 32,
            detour_link_bytes: 16,
            playerbots: false,
            float_terrain: true,
            small_vmap: true,
        }
    }
}

/// Untrusted recorded evidence. A future audited producer must establish patch-member
/// equality, extraction completion and a live immutable source epoch independently.
#[derive(Clone, Debug)]
pub(crate) struct SourceBinding {
    pub source_epoch: Hash,
    pub raw_source_manifest: Hash,
    pub raw_static_inventory: Hash,
    pub content: Hash,
    pub tool_binaries: [Hash; 4], // ad, vmap_extractor, vmap_assembler, MoveMapGen
    pub effective_config: Hash,
    pub offmesh_input: Hash,
    pub build_configuration: Hash, // platform, compiler, flags and effective native policy
}
impl SourceBinding {
    fn hashes(&self) -> impl Iterator<Item = &Hash> {
        [
            &self.source_epoch,
            &self.raw_source_manifest,
            &self.raw_static_inventory,
            &self.content,
        ]
        .into_iter()
        .chain(self.tool_binaries.iter())
        .chain([
            &self.effective_config,
            &self.offmesh_input,
            &self.build_configuration,
        ])
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ProductSpec {
    pub id: ProductId,
    pub bytes: u64,
    pub sha256: Hash,
    /// Declared generation edges plus exact direct VMap tile model-name matching.
    /// This does not validate BIH/native references or source extraction completeness.
    pub dependencies: Vec<ProductId>,
}

#[derive(Clone, Debug)]
pub(crate) struct InventorySpec {
    pub map: u32,
    pub compatibility: Compatibility,
    pub binding: SourceBinding,
    /// Every expected tile, including intermediate/guard tiles, from the future producer.
    /// This caller list alone cannot certify complete polygon-route coverage.
    pub tiles: Vec<TileId>,
    pub products: Vec<ProductSpec>,
}

#[derive(Debug)]
struct Product {
    bytes: Box<[u8]>,
    terrain: Option<TerrainTile>,
}

/// Successful construction says only that the exact declared file set is present,
/// hash-bound and framed, and that terrain is decoded. Native model/nav references
/// are validated/loaded only by the optional separate readers. No conversion to authority.
#[derive(Debug)]
pub(crate) struct InventoriedProducts {
    map: u32,
    revision: Hash,
    content: Hash,
    source_epoch: Hash,
    products: BTreeMap<ProductId, Product>,
}
impl InventoriedProducts {
    /// Read-only off-thread loader for a curated private staging root, outside the install.
    /// No lazy reads: later file deletion/replacement cannot mutate this owned snapshot.
    /// Filesystem checks are hygiene, not protection against a hostile local filesystem.
    pub(crate) fn load(root: &Path, spec: InventorySpec) -> Result<Self, Error> {
        let expected = validate_spec(&spec)?;
        if !root.is_absolute()
            || fs::canonicalize(root).map_err(|_| Error::Io)? != root
            || !fs::symlink_metadata(root).map_err(|_| Error::Io)?.is_dir()
        {
            return Err(Error::Identity);
        }
        check_directory(root, &expected)?;
        Self::materialize(spec, |name, bytes| {
            let path = root.join(name);
            let meta = fs::symlink_metadata(&path).map_err(|_| Error::Missing)?;
            if !meta.is_file() || meta.len() != bytes {
                return Err(Error::Framing);
            }
            let file = File::open(path).map_err(|_| Error::Io)?;
            let meta = file.metadata().map_err(|_| Error::Io)?;
            if !meta.is_file() || meta.len() != bytes {
                return Err(Error::Framing);
            }
            let mut data = Vec::new();
            data.try_reserve_exact(bytes as usize + 1)
                .map_err(|_| Error::Budget)?;
            file.take(bytes + 1)
                .read_to_end(&mut data)
                .map_err(|_| Error::Io)?;
            if data.len() as u64 != bytes {
                return Err(Error::Framing);
            }
            Ok(data)
        })
    }

    fn materialize(
        spec: InventorySpec,
        mut read: impl FnMut(&str, u64) -> Result<Vec<u8>, Error>,
    ) -> Result<Self, Error> {
        // Validate ALL identities, lengths, budgets and declared closure before any read.
        validate_spec(&spec)?;
        let mut revision = Sha256::new();
        revision.update(POLICY);
        revision.update(SOURCE_PIN);
        revision.update(DETOUR_TREE);
        revision.update(spec.map.to_le_bytes());
        for hash in spec.binding.hashes() {
            revision.update(hash);
        }
        let tiles: BTreeSet<_> = spec.tiles.into_iter().collect();
        revision.update((tiles.len() as u32).to_le_bytes());
        for tile in tiles {
            revision.update([tile.x, tile.y]);
        }
        let specs: BTreeMap<_, _> = spec
            .products
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect();
        revision.update((specs.len() as u32).to_le_bytes());
        let mut products = BTreeMap::new();
        for (id, item) in specs {
            let name = id.filename(spec.map)?;
            let data = read(&name, item.bytes)?;
            if data.len() as u64 != item.bytes {
                return Err(Error::Framing);
            }
            if Hash::from(Sha256::digest(&data)) != item.sha256 {
                return Err(Error::Hash);
            }
            let terrain = match id {
                ProductId::Terrain(tile) => Some(TerrainTile::decode(tile, &data)?),
                _ => {
                    let models = envelopes::validate(&id, &data)?;
                    let declared: BTreeSet<_> = item
                        .dependencies
                        .iter()
                        .filter(|d| matches!(d, ProductId::VmapModel(_)))
                        .cloned()
                        .collect();
                    if models != declared {
                        return Err(Error::Dependency);
                    }
                    None
                }
            };
            hash_name(&mut revision, &name);
            revision.update(item.bytes.to_le_bytes());
            revision.update(item.sha256);
            let dependencies: BTreeSet<_> = item.dependencies.into_iter().collect();
            revision.update((dependencies.len() as u32).to_le_bytes());
            for dependency in dependencies {
                hash_name(&mut revision, &dependency.filename(spec.map)?);
            }
            products.insert(
                id,
                Product {
                    bytes: data.into_boxed_slice(),
                    terrain,
                },
            );
        }
        Ok(Self {
            map: spec.map,
            content: spec.binding.content,
            source_epoch: spec.binding.source_epoch,
            revision: revision.finalize().into(),
            products,
        })
    }

    pub(crate) fn revision(&self) -> Hash {
        self.revision
    }

    pub(crate) fn terrain(&self, map: u32, tile: TileId) -> Result<&TerrainTile, Error> {
        if map != self.map {
            return Err(Error::Identity);
        }
        self.products
            .get(&ProductId::Terrain(tile))
            .and_then(|p| p.terrain.as_ref())
            .ok_or(Error::Missing)
    }

    /// Future pinned native loaders may consume these bytes. Access never grants LOS/path use.
    pub(crate) fn inventoried_bytes(&self, map: u32, id: &ProductId) -> Result<&[u8], Error> {
        if map != self.map {
            return Err(Error::Identity);
        }
        self.products
            .get(id)
            .map(|p| p.bytes.as_ref())
            .ok_or(Error::Missing)
    }
}

fn hash_name(hash: &mut Sha256, name: &str) {
    hash.update((name.len() as u32).to_le_bytes());
    hash.update(name.as_bytes());
}

fn validate_spec(spec: &InventorySpec) -> Result<BTreeSet<String>, Error> {
    if spec.products.is_empty()
        || spec.products.len() > MAX_PRODUCTS
        || spec.tiles.is_empty()
        || spec.tiles.len() > MAX_TILES
    {
        return Err(Error::Budget);
    }
    if spec.map > 999 {
        return Err(Error::Identity);
    }
    if spec.compatibility != Compatibility::pinned() || spec.binding.hashes().any(|h| *h == [0; 32])
    {
        return Err(Error::Profile);
    }
    let mut tiles = BTreeSet::new();
    for tile in &spec.tiles {
        tile.validate()?;
        if !tiles.insert(*tile) {
            return Err(Error::Duplicate);
        }
    }
    let mut products = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut aliases = BTreeSet::new();
    let mut total = 0u64;
    let mut edges = 0usize;
    for product in &spec.products {
        if product.bytes == 0 || product.bytes > MAX_PRODUCT_BYTES {
            return Err(Error::Budget);
        }
        total = total.checked_add(product.bytes).ok_or(Error::Budget)?;
        edges = edges
            .checked_add(product.dependencies.len())
            .ok_or(Error::Budget)?;
        if total > MAX_TOTAL_BYTES || edges > MAX_EDGES {
            return Err(Error::Budget);
        }
        let name = product.id.filename(spec.map)?;
        if !aliases.insert(name.to_ascii_lowercase())
            || products.insert(&product.id, product).is_some()
        {
            return Err(Error::Duplicate);
        }
        names.insert(name);
        match &product.id {
            ProductId::Terrain(t) | ProductId::VmapTile(t) | ProductId::MmapTile(t)
                if !tiles.contains(t) =>
            {
                return Err(Error::Unexpected);
            }
            _ => {}
        }
    }
    // A bounded exterior bundle always contains all three tile products, plus map roots.
    for id in [ProductId::VmapTree, ProductId::MmapParams] {
        if !products.contains_key(&id) {
            return Err(Error::Missing);
        }
    }
    for tile in tiles {
        for id in [
            ProductId::Terrain(tile),
            ProductId::VmapTile(tile),
            ProductId::MmapTile(tile),
        ] {
            if !products.contains_key(&id) {
                return Err(Error::Missing);
            }
        }
    }
    let mut referenced_models = BTreeSet::new();
    for product in &spec.products {
        let mut dependencies = BTreeSet::new();
        for dependency in &product.dependencies {
            dependency.filename(spec.map)?;
            if !products.contains_key(dependency) {
                return Err(Error::Missing);
            }
            if !dependencies.insert(dependency) {
                return Err(Error::Duplicate);
            }
            let valid = matches!(
                (&product.id, dependency),
                (
                    ProductId::VmapTile(_),
                    ProductId::VmapTree | ProductId::VmapModel(_)
                ) | (
                    ProductId::MmapTile(_),
                    ProductId::MmapParams | ProductId::Terrain(_) | ProductId::VmapTile(_)
                )
            );
            if !valid {
                return Err(Error::Dependency);
            }
            if matches!(dependency, ProductId::VmapModel(_)) {
                referenced_models.insert(dependency);
            }
        }
        let required = match product.id {
            ProductId::VmapTile(_) => vec![ProductId::VmapTree],
            ProductId::MmapTile(tile) => vec![
                ProductId::MmapParams,
                ProductId::Terrain(tile),
                ProductId::VmapTile(tile),
            ],
            _ => vec![],
        };
        if required.iter().any(|id| !dependencies.contains(id)) {
            return Err(Error::Dependency);
        }
    }
    if spec
        .products
        .iter()
        .any(|p| matches!(p.id, ProductId::VmapModel(_)) && !referenced_models.contains(&p.id))
    {
        return Err(Error::Unexpected);
    }
    Ok(names)
}

fn check_directory(root: &Path, expected: &BTreeSet<String>) -> Result<(), Error> {
    let mut found = BTreeSet::new();
    let mut directories = 0;
    for entry in fs::read_dir(root).map_err(|_| Error::Io)? {
        let entry = entry.map_err(|_| Error::Io)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::Identity)?;
        directories += 1;
        if directories > 3
            || !matches!(name.as_str(), "maps" | "vmaps" | "mmaps")
            || !entry.file_type().map_err(|_| Error::Io)?.is_dir()
        {
            return Err(Error::Unexpected);
        }
        for file in fs::read_dir(entry.path()).map_err(|_| Error::Io)? {
            let file = file.map_err(|_| Error::Io)?;
            if found.len() >= MAX_PRODUCTS {
                return Err(Error::Budget);
            }
            let basename = file
                .file_name()
                .into_string()
                .map_err(|_| Error::Identity)?;
            if basename.len() > 160 || !file.file_type().map_err(|_| Error::Io)?.is_file() {
                return Err(Error::Unexpected);
            }
            let path = format!("{name}/{basename}");
            if !expected.contains(&path) {
                return Err(Error::Unexpected);
            }
            found.insert(path);
        }
    }
    if &found != expected {
        return Err(Error::Missing);
    }
    Ok(())
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or(Error::Framing)?
            .try_into()
            .unwrap(),
    ))
}
fn u16_at(bytes: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or(Error::Framing)?
            .try_into()
            .unwrap(),
    ))
}

#[cfg(test)]
mod tests;

/// Original serialized geometry only, shared by the app and exact-code controller tests.
#[cfg(test)]
pub(crate) fn authored_vmap(map: u32) -> vmap::VmapSnapshot {
    let (mut spec, files) = vmap::tests::fixture();
    let old_map = spec.map;
    spec.map = map;
    let files = spec
        .products
        .iter()
        .map(|p| {
            (
                p.id.filename(map).unwrap(),
                files[&p.id.filename(old_map).unwrap()].clone(),
            )
        })
        .collect();
    tests::load(spec, &files)
        .unwrap()
        .load_static_vmap()
        .unwrap()
}
