// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-version-bound historical collision queries, without terrain/nav evidence.
//! Queries describe owned decoded bytes, not the current install. Any future live-use
//! adapter must explicitly recheck freshness; this is not a source lease or admission.
use super::super::{envelopes, Error, Hash, ProductId, TileId};
use super::{HeightReply, HeightRequest, RayReply, RayRequest, SnapshotBinding, VmapSnapshot};
use benilla_native_vmap_source::{AssembledSourceMap, MapBundleError};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

const POLICY: &[u8] = b"benilla-prepared-collision-historical-snapshot-v1";
const MAX_PRODUCTS: usize = 4096;

#[derive(Debug)]
pub(crate) enum PrepareError {
    Cancelled,
    Source(MapBundleError),
    Validation(Error),
}
impl From<Error> for PrepareError {
    fn from(error: Error) -> Self {
        Self::Validation(error)
    }
}

/// Immutable decoded snapshot tied to a genuine captured source bundle. No raw
/// geometry or full-inventory conversion is exposed. The source borrow permits
/// explicit freshness checks but does not guarantee that disk contents stay fixed.
#[derive(Debug)]
pub(crate) struct PreparedCollision<'bundle, 'source> {
    source: &'bundle AssembledSourceMap<'source>,
    geometry: VmapSnapshot,
}
impl<'bundle, 'source> PreparedCollision<'bundle, 'source> {
    pub(crate) fn from_source(
        source: &'bundle AssembledSourceMap<'source>,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, PrepareError> {
        check(&cancelled)?;
        source
            .verify_unchanged(&cancelled)
            .map_err(PrepareError::Source)?;
        let map = source.map();
        if map.map_id() > 999 || map.tiles().is_empty() {
            return Err(Error::Identity.into());
        }
        let count = 1usize
            .checked_add(map.tiles().len())
            .and_then(|n| n.checked_add(source.models().len()))
            .ok_or(Error::Budget)?;
        if count > MAX_PRODUCTS {
            return Err(Error::Budget.into());
        }
        let mut rows = BTreeMap::new();
        rows.insert(ProductId::VmapTree, map.tree_bytes());
        for tile in map.tiles() {
            check(&cancelled)?;
            let (x, y) = tile.coordinates();
            if x >= 64 || y >= 64 {
                return Err(Error::Unsupported.into());
            }
            let id = ProductId::VmapTile(TileId {
                x: y as u8,
                y: x as u8,
            });
            if rows.insert(id, tile.bytes()).is_some() {
                return Err(Error::Duplicate.into());
            }
        }
        let mut effective = BTreeSet::new();
        for model in source.models() {
            check(&cancelled)?;
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
        let geometry = decode_rows(map.map_id(), *map.source_binding(), &rows, &cancelled)?;
        source
            .verify_unchanged(&cancelled)
            .map_err(PrepareError::Source)?;
        check(&cancelled)?;
        Ok(Self { source, geometry })
    }
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.geometry.binding()
    }
    /// Captured preprocessing identity, not evidence about the current install.
    pub(crate) fn source_version(&self) -> Hash {
        *self.source.map().source_binding()
    }
    /// Explicit recheck for a new live-use operation. Not a lock: later changes
    /// remain possible. Historical queries deliberately do not call this implicitly.
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
    pub(crate) fn query(&self, request: RayRequest, deadline: Instant) -> Result<RayReply, Error> {
        self.geometry.query(request, deadline)
    }
    pub(crate) fn query_height(
        &self,
        request: HeightRequest,
        deadline: Instant,
    ) -> Result<HeightReply, Error> {
        self.geometry.query_height(request, deadline)
    }
}
fn check(cancelled: &impl Fn() -> bool) -> Result<(), PrepareError> {
    if cancelled() {
        Err(PrepareError::Cancelled)
    } else {
        Ok(())
    }
}
fn decode_rows(
    map: u32,
    source: Hash,
    rows: &BTreeMap<ProductId, &[u8]>,
    cancelled: &impl Fn() -> bool,
) -> Result<VmapSnapshot, PrepareError> {
    let binding = validate_rows(map, source, rows, cancelled)?;
    check(cancelled)?;
    // Reuse every existing model, BIH, transform and enclosing-bound check.
    // The bounded complete decode is synchronous (at most 16 MiB serialized);
    // cancellation is polled immediately before and after it.
    let geometry = VmapSnapshot::load_rows(binding, rows, true)?;
    if geometry.spawns.iter().any(Option::is_none) {
        return Err(Error::Missing.into());
    }
    check(cancelled)?;
    Ok(geometry)
}
fn validate_rows(
    map: u32,
    source: Hash,
    rows: &BTreeMap<ProductId, &[u8]>,
    cancelled: &impl Fn() -> bool,
) -> Result<SnapshotBinding, PrepareError> {
    let mut total = 0usize;
    let mut referenced = BTreeSet::new();
    let mut models = BTreeSet::new();
    let mut revision = Sha256::new();
    revision.update(POLICY);
    revision.update(map.to_le_bytes());
    revision.update(source);
    revision.update((rows.len() as u32).to_le_bytes());
    for (id, bytes) in rows {
        check(cancelled)?;
        total = total.checked_add(bytes.len()).ok_or(Error::Budget)?;
        if bytes.is_empty() || total > super::MAX_VMAP_BYTES {
            return Err(Error::Budget.into());
        }
        if let ProductId::VmapSourceModel(name) = id {
            models.insert(id.clone());
            revision.update((name.as_bytes().len() as u32).to_le_bytes());
            revision.update(name.as_bytes());
        } else if !matches!(id, ProductId::VmapTree | ProductId::VmapTile(_)) {
            return Err(Error::Unexpected.into());
        }
        let filename = id.filename(map)?;
        revision.update((filename.len() as u32).to_le_bytes());
        revision.update(filename.as_bytes());
        revision.update((bytes.len() as u64).to_le_bytes());
        for chunk in bytes.chunks(64 * 1024) {
            check(cancelled)?;
            revision.update(chunk);
        }
        referenced.extend(envelopes::validate_source_names(id, bytes, true)?);
    }
    if referenced != models {
        return Err(Error::Dependency.into());
    }
    let inventory: Hash = revision.finalize().into();
    let mut policy = Sha256::new();
    policy.update(POLICY);
    policy.update(inventory);
    Ok(SnapshotBinding {
        map,
        source_epoch: source,
        content: inventory,
        inventory,
        policy: policy.finalize().into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_native_vmap::SourceModelName;
    fn rows() -> BTreeMap<ProductId, Vec<u8>> {
        let mut expected = super::super::captured_map_tests::expected_products();
        BTreeMap::from([
            (ProductId::VmapTree, expected.remove("000.vmtree").unwrap()),
            (
                ProductId::VmapTile(TileId { x: 31, y: 30 }),
                expected.remove("000_30_31.vmtile").unwrap(),
            ),
            (
                ProductId::VmapSourceModel(SourceModelName::new(b"Hull.wmo").unwrap()),
                expected.remove("Hull.wmo.vmo").unwrap(),
            ),
        ])
    }
    fn borrowed(rows: &BTreeMap<ProductId, Vec<u8>>) -> BTreeMap<ProductId, &[u8]> {
        rows.iter()
            .map(|(id, bytes)| (id.clone(), bytes.as_slice()))
            .collect()
    }
    #[test]
    fn exact_source_version_and_rows_bind_the_collision_snapshot() {
        let rows = rows();
        let rows = borrowed(&rows);
        let a = decode_rows(0, [1; 32], &rows, &|| false).unwrap();
        let b = decode_rows(0, [2; 32], &rows, &|| false).unwrap();
        assert_ne!(a.binding(), b.binding());
        assert_eq!(a.binding().source_epoch, [1; 32]);
        assert_eq!(
            a.binding(),
            decode_rows(0, [1; 32], &rows, &|| false).unwrap().binding()
        );
    }
    #[test]
    fn incomplete_and_unexpected_rows_cannot_become_a_collision_snapshot() {
        let base = rows();
        let model = ProductId::VmapSourceModel(SourceModelName::new(b"Hull.wmo").unwrap());
        let mut missing = base.clone();
        missing.remove(&model);
        assert!(matches!(
            decode_rows(0, [1; 32], &borrowed(&missing), &|| false),
            Err(PrepareError::Validation(Error::Dependency))
        ));
        let mut extra = base.clone();
        extra.insert(ProductId::MmapParams, vec![0; 28]);
        assert!(matches!(
            decode_rows(0, [1; 32], &borrowed(&extra), &|| false),
            Err(PrepareError::Validation(Error::Unexpected))
        ));
        let mut no_spawn = base;
        no_spawn.remove(&model);
        let mut empty_tile = b"VMAP_7.0".to_vec();
        empty_tile.extend(0u32.to_le_bytes());
        no_spawn.insert(ProductId::VmapTile(TileId { x: 31, y: 30 }), empty_tile);
        assert!(matches!(
            decode_rows(0, [1; 32], &borrowed(&no_spawn), &|| false),
            Err(PrepareError::Validation(Error::Missing))
        ));
    }
    #[test]
    fn malformed_and_oversized_rows_fail_before_use() {
        let mut rows = rows();
        rows.insert(
            ProductId::VmapTree,
            vec![0; super::super::MAX_VMAP_BYTES + 1],
        );
        assert!(matches!(
            decode_rows(0, [1; 32], &borrowed(&rows), &|| false),
            Err(PrepareError::Validation(Error::Budget))
        ));
        rows.insert(ProductId::VmapTree, b"invalid".to_vec());
        assert!(matches!(
            decode_rows(0, [1; 32], &borrowed(&rows), &|| false),
            Err(PrepareError::Validation(Error::Format | Error::Framing))
        ));
    }
}
