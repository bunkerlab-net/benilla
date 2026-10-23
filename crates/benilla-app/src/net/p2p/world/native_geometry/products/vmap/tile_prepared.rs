// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit tile residency over a validated complete source map index.
//! Unloaded geometry is unavailable when required; it never becomes a clear ray.
use super::super::{envelopes, Error, Hash, ProductId, TileId};
use super::{
    bih::Bih, prepared::PrepareError, Cursor, HeightReply, HeightRequest, RayReply, RayRequest,
    SnapshotBinding, Spawn, VmapSnapshot,
};
use benilla_native_vmap_source::AssembledSourceMap;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

const POLICY: &[u8] = b"benilla-prepared-tile-collision-historical-snapshot-v1";
const MAX_SOURCE_MODELS: usize = 32_768;
const MAX_SOURCE_TILES: usize = 4096;
const MAX_SOURCE_BYTES: usize = 512 * 1024 * 1024;
const MAX_INDEX_BYTES: usize = 64 * 1024 * 1024;
const MAX_INDEX_REFERENCES: usize = 1_048_576;
const MAX_RESIDENT_TILES: usize = 64;

/// Owned decoded resident geometry over an immutable historical source version.
/// The complete map index is checked, but unresident model geometry is not decoded.
/// No full inventory, live source lease or gameplay admission is exposed.
#[derive(Debug)]
pub(crate) struct PreparedTileCollision<'bundle, 'source> {
    source: &'bundle AssembledSourceMap<'source>,
    decoded: CollisionStorage<'bundle>,
}
/// Non-cloneable decoded payload, with no source authority of its own.
#[derive(Debug)]
struct DecodedTileCollision {
    tiles: Vec<TileId>,
    geometry: VmapSnapshot,
}
// Preserve the existing inline production allocation; only test views borrow.
#[cfg_attr(test, allow(clippy::large_enum_variant))]
#[derive(Debug)]
enum CollisionStorage<'bundle> {
    Owned(DecodedTileCollision, std::marker::PhantomData<&'bundle ()>),
    #[cfg(test)]
    Borrowed(&'bundle DecodedTileCollision),
}
impl std::ops::Deref for CollisionStorage<'_> {
    type Target = DecodedTileCollision;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(decoded, _) => decoded,
            #[cfg(test)]
            Self::Borrowed(decoded) => decoded,
        }
    }
}
/// Exact immutable capture paired inseparably with its once-decoded resident geometry.
#[cfg(test)]
pub(in super::super) struct OwnedTileCollision {
    source: AssembledSourceMap<'static>,
    decoded: DecodedTileCollision,
}
#[cfg(test)]
impl OwnedTileCollision {
    pub(in super::super) fn from_source(
        source: AssembledSourceMap<'static>,
        tiles: &[TileId],
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, PrepareError> {
        let prepared = PreparedTileCollision::from_source(&source, tiles, cancelled)?;
        let CollisionStorage::Owned(decoded, _) = prepared.decoded else {
            unreachable!("from_source owns its decoded collision")
        };
        Ok(Self { source, decoded })
    }
    pub(in super::super) fn view(&self) -> PreparedTileCollision<'_, 'static> {
        PreparedTileCollision {
            source: &self.source,
            decoded: CollisionStorage::Borrowed(&self.decoded),
        }
    }
}
impl<'bundle, 'source> PreparedTileCollision<'bundle, 'source> {
    pub(crate) fn from_source(
        source: &'bundle AssembledSourceMap<'source>,
        tiles: &[TileId],
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, PrepareError> {
        #[cfg(test)]
        super::super::source_height::preparation_counts::collision();
        check(&cancelled)?;
        source
            .verify_unchanged(&cancelled)
            .map_err(PrepareError::Source)?;
        let tiles = selection(tiles, &cancelled)?;
        let rows = source_rows(source, &cancelled)?;
        let geometry = decode_rows(
            source.map().map_id(),
            *source.map().source_binding(),
            &rows,
            &tiles,
            &cancelled,
        )?;
        source
            .verify_unchanged(&cancelled)
            .map_err(PrepareError::Source)?;
        check(&cancelled)?;
        Ok(Self {
            source,
            decoded: CollisionStorage::Owned(
                DecodedTileCollision { tiles, geometry },
                std::marker::PhantomData,
            ),
        })
    }
    pub(in super::super) fn append_navigation_mesh(
        &self,
        mesh: &mut super::super::nav_generation::terrain_mesh::Mesh,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), Error> {
        super::nav_mesh::append(&self.decoded.geometry, mesh, cancelled)
    }
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.decoded.geometry.binding()
    }
    /// Metadata from the actual borrowed capture; no raw member bytes are exposed.
    pub(in super::super) fn source_receipts(
        &self,
    ) -> impl Iterator<Item = benilla_native_vmap_source::SourceReceipt<'_>> {
        self.source.source_receipts()
    }
    pub(crate) fn source_version(&self) -> Hash {
        *self.source.map().source_binding()
    }
    pub(crate) fn resident_tiles(&self) -> &[TileId] {
        &self.decoded.tiles
    }
    pub(crate) fn verify_unchanged(
        &self,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), PrepareError> {
        check(&cancelled)?;
        self.source
            .verify_unchanged(&cancelled)
            .map_err(PrepareError::Source)?;
        check(&cancelled)
    }
    /// Scalar observation composition retains one shared query work/deadline budget.
    pub(in super::super) fn height_with_budget(
        &self,
        position: [f32; 3],
        search: f32,
        budget: &mut super::QueryBudget,
    ) -> Result<f32, Error> {
        self.decoded.geometry.height(position, search, budget)
    }
    /// Exact selected group absence only; a wet group remains unsupported here.
    /// Traversal uses the complete index and refuses any required unloaded slot.
    pub(in super::super) fn no_liquid_with_budget(
        &self,
        position: [f32; 3],
        budget: &mut super::QueryBudget,
    ) -> Result<Option<super::LocationFloor>, Error> {
        let Some(selected) = self
            .decoded
            .geometry
            .select_resident_location(position, budget)?
        else {
            return Ok(None);
        };
        budget.take()?;
        let spawn = self.decoded.geometry.spawns[selected.spawn]
            .as_ref()
            .ok_or(Error::Missing)?;
        let group = &self.decoded.geometry.models[spawn.model].groups[selected.group];
        if group.liquid.is_some() {
            return Err(Error::Unsupported);
        }
        budget.complete()?;
        Ok(Some(selected.floor))
    }
    pub(in super::super) fn query_with_budget(
        &self,
        request: RayRequest,
        budget: &mut super::QueryBudget,
    ) -> Result<RayReply, Error> {
        self.decoded.geometry.query_with_budget(request, budget)
    }
    pub(crate) fn query(&self, request: RayRequest, deadline: Instant) -> Result<RayReply, Error> {
        self.decoded.geometry.query(request, deadline)
    }
    pub(crate) fn query_height(
        &self,
        request: HeightRequest,
        deadline: Instant,
    ) -> Result<HeightReply, Error> {
        self.decoded.geometry.query_height(request, deadline)
    }
}
fn check(cancelled: &impl Fn() -> bool) -> Result<(), PrepareError> {
    if cancelled() {
        Err(PrepareError::Cancelled)
    } else {
        Ok(())
    }
}
fn selection(tiles: &[TileId], cancelled: &impl Fn() -> bool) -> Result<Vec<TileId>, PrepareError> {
    check(cancelled)?;
    if tiles.len() > MAX_RESIDENT_TILES {
        return Err(Error::Budget.into());
    }
    let mut selected = Vec::new();
    selected
        .try_reserve_exact(tiles.len())
        .map_err(|_| Error::Budget)?;
    for &tile in tiles {
        check(cancelled)?;
        tile.validate()?;
        selected.push(tile);
    }
    selected.sort_unstable();
    if selected.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(Error::Duplicate.into());
    }
    Ok(selected)
}
fn source_rows<'a>(
    source: &'a AssembledSourceMap<'_>,
    cancelled: &impl Fn() -> bool,
) -> Result<BTreeMap<ProductId, &'a [u8]>, PrepareError> {
    check(cancelled)?;
    let map = source.map();
    if map.map_id() > 999 {
        return Err(Error::Identity.into());
    }
    if map.tiles().len() > MAX_SOURCE_TILES || source.models().len() > MAX_SOURCE_MODELS {
        return Err(Error::Budget.into());
    }
    let mut rows = BTreeMap::new();
    rows.insert(ProductId::VmapTree, map.tree_bytes());
    for tile in map.tiles() {
        check(cancelled)?;
        let (x, y) = tile.coordinates();
        if x >= 64 || y >= 64 {
            return Err(Error::Unsupported.into());
        }
        if rows
            .insert(
                ProductId::VmapTile(TileId {
                    x: y as u8,
                    y: x as u8,
                }),
                tile.bytes(),
            )
            .is_some()
        {
            return Err(Error::Duplicate.into());
        }
    }
    let mut effective = BTreeSet::new();
    for model in source.models() {
        check(cancelled)?;
        let name = model.source_identity().map_err(PrepareError::Source)?;
        if !effective.insert(
            name.effective_source_basename()
                .map_err(|_| Error::Identity)?,
        ) {
            return Err(Error::Duplicate.into());
        }
        if rows
            .insert(ProductId::VmapSourceModel(name), model.bytes())
            .is_some()
        {
            return Err(Error::Duplicate.into());
        }
    }
    Ok(rows)
}
fn decode_rows(
    map: u32,
    source: Hash,
    rows: &BTreeMap<ProductId, &[u8]>,
    tiles: &[TileId],
    cancelled: &impl Fn() -> bool,
) -> Result<VmapSnapshot, PrepareError> {
    check(cancelled)?;
    if map > 999 || rows.len() > 1 + MAX_SOURCE_TILES + MAX_SOURCE_MODELS {
        return Err(Error::Budget.into());
    }
    let mut total = 0usize;
    let mut index_bytes = 0usize;
    let mut model_ids = BTreeMap::new();
    let mut referenced = BTreeSet::new();
    let mut selected_models = BTreeSet::new();
    let mut revision = Sha256::new();
    revision.update(POLICY);
    revision.update(map.to_le_bytes());
    revision.update(source);
    revision.update((tiles.len() as u32).to_le_bytes());
    for tile in tiles {
        revision.update([tile.x, tile.y]);
    }
    revision.update((rows.len() as u32).to_le_bytes());
    for (id, bytes) in rows {
        check(cancelled)?;
        total = total.checked_add(bytes.len()).ok_or(Error::Budget)?;
        if bytes.is_empty() || total > MAX_SOURCE_BYTES {
            return Err(Error::Budget.into());
        }
        match id {
            ProductId::VmapSourceModel(name) => {
                revision.update((name.as_bytes().len() as u32).to_le_bytes());
                revision.update(name.as_bytes());
                model_ids.insert(id.clone(), model_ids.len());
                if model_ids.len() > MAX_SOURCE_MODELS {
                    return Err(Error::Budget.into());
                }
            }
            ProductId::VmapTree | ProductId::VmapTile(_) => {
                index_bytes = index_bytes.checked_add(bytes.len()).ok_or(Error::Budget)?;
                if index_bytes > MAX_INDEX_BYTES {
                    return Err(Error::Budget.into());
                }
                let dependencies = envelopes::validate_source_names(id, bytes, true)?;
                if let ProductId::VmapTile(tile) = id {
                    tile.validate()?;
                    if tiles.binary_search(tile).is_ok() {
                        selected_models.extend(dependencies.iter().cloned());
                    }
                }
                referenced.extend(dependencies);
                if referenced.len() > MAX_SOURCE_MODELS || selected_models.len() > MAX_SOURCE_MODELS
                {
                    return Err(Error::Budget.into());
                }
            }
            _ => return Err(Error::Unexpected.into()),
        }
        let filename = id.filename(map)?;
        revision.update((filename.len() as u32).to_le_bytes());
        revision.update(filename.as_bytes());
        revision.update((bytes.len() as u64).to_le_bytes());
        for chunk in bytes.chunks(64 * 1024) {
            check(cancelled)?;
            revision.update(chunk);
        }
    }
    if model_ids.len() > MAX_SOURCE_MODELS || referenced != model_ids.keys().cloned().collect() {
        return Err(Error::Dependency.into());
    }
    validate_index(rows, &model_ids, cancelled)?;
    let inventory: Hash = revision.finalize().into();
    let mut policy = Sha256::new();
    policy.update(POLICY);
    policy.update(inventory);
    let binding = SnapshotBinding {
        map,
        content: inventory,
        source_epoch: source,
        inventory,
        policy: policy.finalize().into(),
    };
    let mut resident = BTreeMap::new();
    for (id, bytes) in rows {
        check(cancelled)?;
        let keep = match id {
            ProductId::VmapTree => true,
            ProductId::VmapTile(tile) => tiles.binary_search(tile).is_ok(),
            ProductId::VmapSourceModel(_) => selected_models.contains(id),
            _ => false,
        };
        if keep {
            resident.insert(id.clone(), *bytes);
        }
    }
    // Retain every existing loaded geometry/transform/enclosing-bound validator.
    // This synchronous decode keeps the existing 16 MiB resident byte envelope.
    check(cancelled)?;
    let geometry = VmapSnapshot::load_rows(binding, &resident, true)?;
    check(cancelled)?;
    Ok(geometry)
}
fn validate_index(
    rows: &BTreeMap<ProductId, &[u8]>,
    models: &BTreeMap<ProductId, usize>,
    cancelled: &impl Fn() -> bool,
) -> Result<(), PrepareError> {
    check(cancelled)?;
    let mut c = Cursor::new(rows.get(&ProductId::VmapTree).ok_or(Error::Missing)?);
    c.magic(b"VMAP_7.0\x01NODE")?;
    let tree = Bih::decode(&mut c)?;
    c.magic(b"GOBJ")?;
    c.finish()?;
    let mut spawns: Vec<Option<Spawn>> = Vec::new();
    spawns
        .try_reserve_exact(tree.primitive_count())
        .map_err(|_| Error::Budget)?;
    while spawns.len() < tree.primitive_count() {
        check(cancelled)?;
        let end = (spawns.len() + 1024).min(tree.primitive_count());
        spawns.resize_with(end, || None);
    }
    let mut ids = BTreeMap::new();
    let mut classes = BTreeMap::new();
    let mut count = 0usize;
    for (id, bytes) in rows {
        if !matches!(id, ProductId::VmapTile(_)) {
            continue;
        }
        check(cancelled)?;
        let mut c = Cursor::new(bytes);
        c.magic(b"VMAP_7.0")?;
        let n = c.count()?;
        count = count.checked_add(n).ok_or(Error::Budget)?;
        if count > MAX_INDEX_REFERENCES {
            return Err(Error::Budget.into());
        }
        for _ in 0..n {
            check(cancelled)?;
            let (slot, spawn) = Spawn::decode(&mut c, models, true)?;
            let target = spawns.get_mut(slot).ok_or(Error::Format)?;
            if let Some(previous) = target {
                if !previous.same_source(&spawn) {
                    return Err(Error::Dependency.into());
                }
            } else {
                if ids.insert(spawn.id, slot).is_some() {
                    return Err(Error::Duplicate.into());
                }
                if classes
                    .insert(spawn.model, spawn.classification())
                    .is_some_and(|old| old != spawn.classification())
                {
                    return Err(Error::Unsupported.into());
                }
                *target = Some(spawn);
            }
        }
        c.finish()?;
    }
    let mut bounds = Vec::new();
    bounds
        .try_reserve_exact(spawns.len())
        .map_err(|_| Error::Budget)?;
    for spawn in &spawns {
        check(cancelled)?;
        bounds.push(Some(spawn.as_ref().ok_or(Error::Missing)?.bounds));
    }
    // Complete index coverage is checked even for unresident models. Their geometry
    // is deliberately not claimed valid until loaded through the ordinary decoder.
    tree.validate_bounds(&bounds)?;
    check(cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_native_vmap::SourceModelName;
    fn rows() -> BTreeMap<ProductId, Vec<u8>> {
        let mut p = super::super::captured_map_tests::expected_products();
        BTreeMap::from([
            (ProductId::VmapTree, p.remove("000.vmtree").unwrap()),
            (
                ProductId::VmapTile(TileId { x: 31, y: 30 }),
                p.remove("000_30_31.vmtile").unwrap(),
            ),
            (
                ProductId::VmapSourceModel(SourceModelName::new(b"Hull.wmo").unwrap()),
                p.remove("Hull.wmo.vmo").unwrap(),
            ),
        ])
    }
    fn decode(rows: &BTreeMap<ProductId, Vec<u8>>) -> Result<VmapSnapshot, PrepareError> {
        let rows = rows
            .iter()
            .map(|(id, b)| (id.clone(), b.as_slice()))
            .collect();
        decode_rows(0, [1; 32], &rows, &[], &|| false)
    }
    #[test]
    fn resident_point_ignores_only_unvisited_unloaded_slots() {
        let geometry = decode(&rows()).unwrap();
        let point = [
            f32::from_bits(0x43e8_d540),
            f32::from_bits(0x4471_aaa0),
            20.0,
        ];
        let budget =
            || super::super::QueryBudget::new(Instant::now() + std::time::Duration::from_secs(10));
        // Original captured Hull tree spans z=1..10. Point traversal rejects
        // z=20 at the root; unlike a height ray, it does not search downwards.
        assert!(geometry
            .select_resident_location(point, &mut budget())
            .unwrap()
            .is_none());
        assert!(matches!(
            geometry.select_location(point, &mut budget()),
            Err(Error::Missing)
        ));
        assert!(matches!(
            geometry.select_resident_location([point[0], point[1], 8.0], &mut budget()),
            Err(Error::Missing)
        ));
    }
    #[test]
    fn unloaded_index_is_still_validated_completely() {
        let base = rows();
        decode(&base).unwrap();
        let tile = ProductId::VmapTile(TileId { x: 31, y: 30 });
        let mut bad = base.clone();
        let bytes = bad.get_mut(&tile).unwrap();
        let n = bytes.len();
        bytes[n - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&bad).is_err());
        let mut bad = base.clone();
        bad.get_mut(&tile).unwrap()[22..26].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(decode(&bad).is_err());
        let mut bad = base.clone();
        bad.remove(&tile);
        bad.retain(|id, _| !matches!(id, ProductId::VmapSourceModel(_)));
        assert!(matches!(
            decode(&bad),
            Err(PrepareError::Validation(Error::Missing))
        ));
        let mut bad = base.clone();
        let mut copied = bad[&tile].clone();
        copied[22..26].copy_from_slice(&123.0f32.to_le_bytes());
        bad.insert(ProductId::VmapTile(TileId { x: 32, y: 30 }), copied);
        assert!(matches!(
            decode(&bad),
            Err(PrepareError::Validation(Error::Dependency))
        ));
        let mut bad = base;
        bad.retain(|id, _| !matches!(id, ProductId::VmapSourceModel(_)));
        assert!(matches!(
            decode(&bad),
            Err(PrepareError::Validation(Error::Dependency))
        ));
    }
    #[test]
    fn distinct_slots_reject_duplicate_ids_and_mixed_model_classes() {
        let mut base = rows();
        let tree = base.get_mut(&ProductId::VmapTree).unwrap();
        assert_eq!(u32::from_le_bytes(tree[37..41].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(tree[53..57].try_into().unwrap()), 1);
        tree[45..49].copy_from_slice(&2u32.to_le_bytes());
        tree[53..57].copy_from_slice(&2u32.to_le_bytes());
        tree.splice(61..61, 1u32.to_le_bytes());
        let tile = ProductId::VmapTile(TileId { x: 31, y: 30 });
        let mut copy = base[&tile].clone();
        let n = copy.len();
        copy[n - 4..].copy_from_slice(&1u32.to_le_bytes());
        let other = ProductId::VmapTile(TileId { x: 32, y: 30 });
        base.insert(other.clone(), copy);
        assert!(matches!(
            decode(&base),
            Err(PrepareError::Validation(Error::Duplicate))
        ));
        let copy = base.get_mut(&other).unwrap();
        copy[18..22].copy_from_slice(&77u32.to_le_bytes());
        copy[12..16].copy_from_slice(&5u32.to_le_bytes());
        assert!(matches!(
            decode(&base),
            Err(PrepareError::Validation(Error::Unsupported))
        ));
    }
    #[test]
    fn complete_index_bounds_are_checked_without_residency() {
        let mut rows = rows();
        rows.get_mut(&ProductId::VmapTree).unwrap()[25..29]
            .copy_from_slice(&16603.0f32.to_le_bytes());
        assert!(matches!(
            decode(&rows),
            Err(PrepareError::Validation(Error::Format))
        ));
    }
    #[test]
    fn unloaded_model_bytes_are_bound_but_not_claimed_decoded() {
        let mut rows = rows();
        let original = decode(&rows).unwrap().binding();
        let model = rows
            .iter_mut()
            .find(|(id, _)| matches!(id, ProductId::VmapSourceModel(_)))
            .unwrap()
            .1;
        model[0] ^= 1;
        assert_ne!(original, decode(&rows).unwrap().binding());
        let borrowed = rows
            .iter()
            .map(|(id, b)| (id.clone(), b.as_slice()))
            .collect();
        assert!(decode_rows(0, [1; 32], &borrowed, &[TileId { x: 31, y: 30 }], &|| false).is_err());
    }
    #[test]
    fn unresident_bytes_do_not_consume_the_resident_decoder_budget() {
        let mut rows = rows();
        rows.iter_mut()
            .find(|(id, _)| matches!(id, ProductId::VmapSourceModel(_)))
            .unwrap()
            .1
            .resize(super::super::MAX_VMAP_BYTES + 1, 0);
        decode(&rows).unwrap();
        let borrowed = rows
            .iter()
            .map(|(id, b)| (id.clone(), b.as_slice()))
            .collect();
        assert!(matches!(
            decode_rows(0, [1; 32], &borrowed, &[TileId { x: 31, y: 30 }], &|| false),
            Err(PrepareError::Validation(Error::Budget))
        ));
    }
    #[test]
    fn residency_selection_has_explicit_limits() {
        assert!(selection(&[TileId { x: 64, y: 0 }], &|| false).is_err());
        assert!(matches!(
            selection(&[TileId { x: 0, y: 0 }; 65], &|| false),
            Err(PrepareError::Validation(Error::Budget))
        ));
        assert!(matches!(
            selection(&[], &|| true),
            Err(PrepareError::Cancelled)
        ));
    }
}
