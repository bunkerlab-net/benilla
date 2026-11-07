// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded dry MMap query product. Successful routes describe this snapshot only.
use super::{liquid::DryLiquidSnapshot, Error, Hash, InventoriedProducts, ProductId};
use benilla_native_nav::{
    LoadedSourceSmooth, Mesh, Route, SourceOwnerFilter, SourceSmoothingBudget,
    SourceSmoothingError, TileInput,
};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, time::Instant};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotBinding {
    pub map: u32,
    pub content: Hash,
    pub source_epoch: Hash,
    pub inventory: Hash,
    pub policy: Hash,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RouteRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    pub from: [u32; 3],
    pub to: [u32; 3],
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RouteReply {
    pub request: RouteRequest,
    pub route: Route,
}
/// Actual source HaveTile pair, before updateFilter; never a polygon lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceTileRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    pub from: [u32; 3],
    pub to: [u32; 3],
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceTileReply {
    request: SourceTileRequest,
    result: benilla_native_nav::LoadedSourceTilePresence,
}
impl SourceTileReply {
    pub(crate) fn request(&self) -> SourceTileRequest {
        self.request
    }
    pub(crate) fn result(&self) -> &benilla_native_nav::LoadedSourceTilePresence {
        &self.result
    }
}
/// Separate source-stage query; the conservative funnel/veto API stays unchanged.
/// Endpoints are the original unsnapped world-coordinate binary32 inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceSmoothRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    pub from: [u32; 3],
    pub to: [u32; 3],
    pub filter: SourceOwnerFilter,
    pub point_limit: usize,
    pub budget: SourceSmoothingBudget,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceSmoothReply {
    pub request: SourceSmoothRequest,
    /// Raw pre-OptimizePath/pre-NormalizeZ dependency, never a movement route fact.
    pub result: LoadedSourceSmooth,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SourceSmoothError {
    Identity,
    Unavailable,
    Deadline,
    Native(SourceSmoothingError),
}
/// An opaque loaded-mesh capability. Its projection is not a restoration API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WarmCacheLineage {
    snapshot: SnapshotBinding,
    cache: benilla_native_nav::WarmCache,
}
impl WarmCacheLineage {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.snapshot
    }
    pub(crate) fn cache(&self) -> &benilla_native_nav::WarmCache {
        &self.cache
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceWarmRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    pub from: [u32; 3],
    pub to: [u32; 3],
    pub budget: benilla_native_nav::SourceWarmCacheBudget,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceWarmReply {
    request: SourceWarmRequest,
    result: benilla_native_nav::LoadedSourceWarmCache,
}
impl SourceWarmReply {
    pub(crate) fn request(&self) -> SourceWarmRequest {
        self.request
    }
    pub(crate) fn result(&self) -> &benilla_native_nav::LoadedSourceWarmCache {
        &self.result
    }
    /// Caller may retain a candidate capability, but this does not commit runtime state.
    pub(crate) fn into_candidate_lineage(self) -> WarmCacheLineage {
        WarmCacheLineage {
            snapshot: self.request.snapshot,
            cache: self.result.into_after(),
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SourceWarmError {
    Identity,
    Unavailable,
    Deadline,
    Native(benilla_native_nav::SourceWarmCacheError),
    Seed(benilla_native_nav::SourceSmoothingError),
    Boundary(benilla_native_nav::Error),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourcePointRequest {
    pub warm: SourceWarmRequest,
    pub ticket: u64,
    pub configuration: benilla_native_nav::SourcePointPathConfig,
    pub budget: benilla_native_nav::SourcePointPathBudget,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourcePointReply {
    request: SourcePointRequest,
    result: benilla_native_nav::LoadedSourceWarmPointPath,
}
impl SourcePointReply {
    pub(crate) fn request(&self) -> SourcePointRequest {
        self.request
    }
    pub(crate) fn result(&self) -> &benilla_native_nav::LoadedSourceWarmPointPath {
        &self.result
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SourcePointError {
    Identity,
    Unavailable,
    Deadline,
    Native(benilla_native_nav::SourcePointPathError),
}
/// Explicit loaded straight calculation; no Chase branch-selection claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceStraightRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    pub from: [u32; 3],
    pub to: [u32; 3],
    pub collision_width_bits: u32,
    pub configuration: benilla_native_nav::SourcePointPathConfig,
    pub budget: benilla_native_nav::SourceStraightPathBudget,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceStraightReply {
    request: SourceStraightRequest,
    result: benilla_native_nav::LoadedSourceWarmStraightPath,
}
impl SourceStraightReply {
    pub(crate) fn request(&self) -> SourceStraightRequest {
        self.request
    }
    pub(crate) fn result(&self) -> &benilla_native_nav::LoadedSourceWarmStraightPath {
        &self.result
    }
}
/// Only a fresh lookup from the straight terminal cache can mint this continuation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceStraightWarmReply {
    before: benilla_native_nav::WarmCache,
    straight: SourceStraightRequest,
    previous_path_type: u32,
    warm: SourceWarmReply,
}
impl SourceStraightWarmReply {
    pub(crate) fn before(&self) -> &benilla_native_nav::WarmCache {
        &self.before
    }
    pub(crate) fn warm(&self) -> &SourceWarmReply {
        &self.warm
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SourceStraightError {
    Identity,
    Unavailable,
    Deadline,
    Native(benilla_native_nav::SourceStraightPathError),
}
pub(crate) struct NavigationSnapshot {
    binding: SnapshotBinding,
    mesh: RefCell<Mesh>,
}
impl InventoriedProducts {
    pub(crate) fn load_navigation(&self) -> Result<NavigationSnapshot, Error> {
        // The separate WMO-liquid envelope cannot enter this conservative adapter,
        // including through a future internal caller reaching its private inventory.
        if self.vmap_liquid_profile {
            return Err(Error::Unsupported);
        }
        // Preserve this dry-only adapter's domain now that the independent terrain
        // reader can decode wet products. A dry MMap is not dry terrain evidence.
        if self
            .products
            .values()
            .filter_map(|p| p.terrain.as_ref())
            .any(|t| t.has_liquid())
        {
            return Err(Error::Unsupported);
        }
        self.load_ground_navigation(b"benilla-pinned-dry-detour-nearest-sliced-funnel-v1", None)
    }

    /// Separate opt-in path; possession of a matching actual-byte dry proof is
    /// mandatory. Never relabel these products as a conservative inventory.
    pub(super) fn load_dry_route_navigation(
        &self,
        proof: &DryLiquidSnapshot,
    ) -> Result<NavigationSnapshot, Error> {
        if !self.vmap_liquid_profile {
            return Err(Error::Unsupported);
        }
        proof.verify_inventory(self)?;
        self.load_ground_navigation(
            b"benilla-pinned-detour-complete-dry-liquid-projection-v1",
            Some(proof.binding().policy),
        )
    }

    fn load_ground_navigation(
        &self,
        policy: &[u8],
        dry_proof: Option<Hash>,
    ) -> Result<NavigationSnapshot, Error> {
        let params = self.inventoried_bytes(self.map, &ProductId::MmapParams)?;
        let mut tiles = Vec::new();
        for (id, p) in &self.products {
            if let ProductId::MmapTile(t) = id {
                tiles.try_reserve(1).map_err(|_| Error::Budget)?;
                tiles.push(TileInput {
                    grid: [t.x, t.y],
                    bytes: &p.bytes,
                });
            }
        }
        let mesh = Mesh::load(params, &tiles).map_err(|error| match error {
            benilla_native_nav::Error::Budget => Error::Budget,
            benilla_native_nav::Error::Unsupported => Error::Unsupported,
            benilla_native_nav::Error::Identity => Error::Identity,
            benilla_native_nav::Error::Framing => Error::Framing,
            _ => Error::Format,
        })?;
        let mut hash = Sha256::new();
        hash.update(policy);
        hash.update(self.revision());
        hash.update(super::SOURCE_PIN);
        hash.update(super::DETOUR_TREE);
        if let Some(proof) = dry_proof {
            hash.update(proof);
        }
        Ok(NavigationSnapshot {
            binding: SnapshotBinding {
                map: self.map,
                content: self.content,
                source_epoch: self.source_epoch,
                inventory: self.revision(),
                policy: hash.finalize().into(),
            },
            mesh: RefCell::new(mesh),
        })
    }
}
impl NavigationSnapshot {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }
    /// Snapshot equality alone cannot transfer a capability to a reloaded mesh.
    /// The straight provider checks this before binding, then privately retains
    /// both owners without a mesh/lineage replacement API. Retry caches can only
    /// come from that bound mesh's opaque straight result.
    pub(crate) fn check_warm_lineage(
        &self,
        lineage: &WarmCacheLineage,
    ) -> Result<(), benilla_native_nav::Error> {
        if lineage.binding() != self.binding {
            return Err(benilla_native_nav::Error::Identity);
        }
        let mesh = self
            .mesh
            .try_borrow()
            .map_err(|_| benilla_native_nav::Error::Unavailable)?;
        lineage.cache().check_mesh(&mesh)
    }
    pub(crate) fn agent_dimensions(&self) -> [f32; 3] {
        self.mesh.borrow().agent_dimensions()
    }
    pub(crate) fn query(
        &self,
        request: RouteRequest,
        deadline: Instant,
    ) -> Result<RouteReply, benilla_native_nav::Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(benilla_native_nav::Error::Identity);
        }
        let route = self
            .mesh
            .try_borrow_mut()
            .map_err(|_| benilla_native_nav::Error::Unavailable)?
            .route(request.from, request.to, deadline)?;
        if Instant::now() >= deadline {
            return Err(benilla_native_nav::Error::Deadline);
        }
        Ok(RouteReply { request, route })
    }

    /// PathFinder::HaveTile uses original XYZ and layer zero on this exact mesh.
    /// A missing start leaves the destination unqueried; neither negative is a route.
    pub(crate) fn source_tile_presence(
        &self,
        request: SourceTileRequest,
        deadline: Instant,
    ) -> Result<SourceTileReply, benilla_native_nav::Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(benilla_native_nav::Error::Identity);
        }
        let mesh = self
            .mesh
            .try_borrow()
            .map_err(|_| benilla_native_nav::Error::Unavailable)?;
        let result = mesh.source_tile_presence_pair(request.from, request.to, deadline)?;
        if Instant::now() >= deadline {
            return Err(benilla_native_nav::Error::Deadline);
        }
        if result.from() != request.from || result.to() != request.to {
            return Err(benilla_native_nav::Error::Identity);
        }
        result.check_mesh(&mesh)?;
        Ok(SourceTileReply { request, result })
    }

    /// Mint only from an actual loaded bounded corridor. This is not a source
    /// initial-owner cache producer or a serialized-ref rehydration path.
    pub(crate) fn seed_bounded_warm_lineage(
        &self,
        request: SourceSmoothRequest,
        deadline: Instant,
    ) -> Result<WarmCacheLineage, SourceWarmError> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(SourceWarmError::Identity);
        }
        let mut mesh = self
            .mesh
            .try_borrow_mut()
            .map_err(|_| SourceWarmError::Unavailable)?;
        let corridor = mesh
            .loaded_source_corridor(
                request.from,
                request.to,
                request.filter,
                request.point_limit,
                request.budget,
                deadline,
            )
            .map_err(SourceWarmError::Seed)?;
        let cache = corridor
            .warm_cache_seed()
            .map_err(SourceWarmError::Boundary)?;
        if Instant::now() >= deadline {
            return Err(SourceWarmError::Deadline);
        }
        Ok(WarmCacheLineage {
            snapshot: self.binding,
            cache,
        })
    }
    pub(crate) fn source_warm(
        &self,
        lineage: &WarmCacheLineage,
        request: SourceWarmRequest,
        deadline: Instant,
    ) -> Result<SourceWarmReply, SourceWarmError> {
        if request.snapshot != self.binding
            || lineage.snapshot != self.binding
            || request.ticket == 0
        {
            return Err(SourceWarmError::Identity);
        }
        let result = self
            .mesh
            .try_borrow_mut()
            .map_err(|_| SourceWarmError::Unavailable)?
            .loaded_source_warm_cache(
                &lineage.cache,
                request.from,
                request.to,
                request.budget,
                deadline,
            )
            .map_err(SourceWarmError::Native)?;
        if Instant::now() >= deadline {
            return Err(SourceWarmError::Deadline);
        }
        if result.from() != request.from || result.to() != request.to {
            return Err(SourceWarmError::Identity);
        }
        Ok(SourceWarmReply { request, result })
    }

    pub(crate) fn source_straight_path(
        &self,
        lineage: &WarmCacheLineage,
        request: SourceStraightRequest,
        deadline: Instant,
    ) -> Result<SourceStraightReply, SourceStraightError> {
        if request.snapshot != self.binding
            || lineage.snapshot != self.binding
            || request.ticket == 0
        {
            return Err(SourceStraightError::Identity);
        }
        if Instant::now() >= deadline {
            return Err(SourceStraightError::Deadline);
        }
        let result = self
            .mesh
            .try_borrow_mut()
            .map_err(|_| SourceStraightError::Unavailable)?
            .loaded_source_warm_straight_path(
                &lineage.cache,
                request.from,
                request.to,
                request.collision_width_bits,
                request.configuration,
                request.budget,
                deadline,
            )
            .map_err(SourceStraightError::Native)?;
        if Instant::now() >= deadline {
            return Err(SourceStraightError::Deadline);
        }
        if result.from() != request.from
            || result.requested_end() != request.to
            || result.collision_width_bits() != request.collision_width_bits
            || result.config() != request.configuration
        {
            return Err(SourceStraightError::Identity);
        }
        Ok(SourceStraightReply { request, result })
    }

    /// TargetedMovementGenerator.cpp:536-538 restores the original goal for the
    /// second calculate. Preserve the entire terminal backing, including tails.
    pub(crate) fn source_straight_warm(
        &self,
        straight: &SourceStraightReply,
        request: SourceWarmRequest,
        deadline: Instant,
    ) -> Result<SourceStraightWarmReply, SourceWarmError> {
        if straight.request.snapshot != self.binding
            || request.snapshot != self.binding
            || straight.request.ticket.checked_add(4) != Some(request.ticket)
            || request.from != straight.request.from
            || request.to != straight.request.to
            || straight.result.path_type() & (8 | 4) == 0
        {
            return Err(SourceWarmError::Identity);
        }
        let lineage = WarmCacheLineage {
            snapshot: self.binding,
            cache: straight.result.after().clone(),
        };
        let warm = self.source_warm(&lineage, request, deadline)?;
        Ok(SourceStraightWarmReply {
            before: lineage.cache,
            straight: straight.request,
            previous_path_type: straight.result.path_type(),
            warm,
        })
    }

    pub(crate) fn source_straight_point_path(
        &self,
        continuation: &SourceStraightWarmReply,
        request: SourcePointRequest,
        deadline: Instant,
    ) -> Result<SourcePointReply, SourcePointError> {
        let configuration = benilla_native_nav::SourcePointPathConfig {
            previous_path_type: continuation.previous_path_type,
            ..continuation.straight.configuration
        };
        if request.configuration != configuration {
            return Err(SourcePointError::Identity);
        }
        self.source_point_path(&continuation.warm, request, deadline)
    }

    pub(crate) fn source_point_path(
        &self,
        warm: &SourceWarmReply,
        request: SourcePointRequest,
        deadline: Instant,
    ) -> Result<SourcePointReply, SourcePointError> {
        if request.warm != warm.request
            || request.warm.snapshot != self.binding
            || request.ticket == 0
            || request.warm.ticket.checked_add(1) != Some(request.ticket)
        {
            return Err(SourcePointError::Identity);
        }
        if Instant::now() >= deadline {
            return Err(SourcePointError::Deadline);
        }
        let result = self
            .mesh
            .try_borrow_mut()
            .map_err(|_| SourcePointError::Unavailable)?
            .loaded_source_warm_point_path(
                &warm.result,
                request.configuration,
                request.budget,
                deadline,
            )
            .map_err(SourcePointError::Native)?;
        if Instant::now() >= deadline {
            return Err(SourcePointError::Deadline);
        }
        Ok(SourcePointReply { request, result })
    }

    pub(crate) fn source_smooth(
        &self,
        request: SourceSmoothRequest,
        deadline: Instant,
    ) -> Result<SourceSmoothReply, SourceSmoothError> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(SourceSmoothError::Identity);
        }
        let result = self
            .mesh
            .try_borrow_mut()
            .map_err(|_| SourceSmoothError::Unavailable)?
            .loaded_source_smooth(
                request.from,
                request.to,
                request.filter,
                request.point_limit,
                request.budget,
                deadline,
            )
            .map_err(SourceSmoothError::Native)?;
        if Instant::now() >= deadline {
            return Err(SourceSmoothError::Deadline);
        }
        if result.from != request.from
            || result.to != request.to
            || result.filter != request.filter
            || result.point_limit != request.point_limit
        {
            return Err(SourceSmoothError::Identity);
        }
        Ok(SourceSmoothReply { request, result })
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use super::super::{tests::load, vmap::tests::fixture as vmap_fixture, TileId};
    use super::*;
    use std::{collections::BTreeMap, time::Duration};
    pub(crate) fn products() -> InventoriedProducts {
        let (spec, files) = fixture();
        load(spec, &files).unwrap()
    }
    /// Existing authored snake mesh, with the original complete dry-product envelope.
    pub(crate) fn straight_ground_products() -> InventoriedProducts {
        let (mut spec, mut files) = fixture();
        let keep = TileId { x: 32, y: 32 };
        spec.tiles = vec![keep];
        spec.products.retain(|p| match p.id {
            ProductId::Terrain(t) | ProductId::VmapTile(t) | ProductId::MmapTile(t) => t == keep,
            _ => true,
        });
        for (id, bytes) in [
            (ProductId::MmapParams, include_bytes!(
                "../../../../../../../benilla-native-nav/tools/fixtures/smoothing/params.test-fixture"
            ).as_slice()),
            (ProductId::MmapTile(keep), include_bytes!(
                "../../../../../../../benilla-native-nav/tools/fixtures/smoothing/ground.test-fixture"
            ).as_slice()),
        ] {
            let product = spec.products.iter_mut().find(|p| p.id == id).unwrap();
            product.bytes = bytes.len() as u64;
            product.sha256 = Sha256::digest(bytes).into();
            files.insert(id.filename(spec.map).unwrap(), bytes.to_vec());
        }
        load(spec, &files).unwrap()
    }
    pub(crate) fn fixture() -> (
        super::super::InventorySpec,
        std::collections::BTreeMap<String, Vec<u8>>,
    ) {
        let (mut spec, files) = vmap_fixture();
        let old = spec.tiles[0];
        let wanted = [TileId { x: 32, y: 31 }, TileId { x: 32, y: 32 }];
        let mut newfiles = BTreeMap::new();
        let mut specs = Vec::new();
        for p in &spec.products {
            match p.id {
                ProductId::Terrain(t) | ProductId::VmapTile(t) | ProductId::MmapTile(t)
                    if t != old =>
                {
                    continue;
                }
                _ => {}
            }
            for t in wanted {
                let mut p = p.clone();
                let mut data = files[&p.id.filename(spec.map).unwrap()].clone();
                let id = match p.id {
                    ProductId::Terrain(_) => ProductId::Terrain(t),
                    ProductId::VmapTile(_) => ProductId::VmapTile(t),
                    ProductId::MmapTile(_) => ProductId::MmapTile(t),
                    _ => p.id.clone(),
                };
                if newfiles.contains_key(&id.filename(spec.map).unwrap()) {
                    continue;
                }
                if matches!(id, ProductId::MmapParams) {
                    data = include_bytes!(
                        "../../../../../../../benilla-native-nav/tools/fixtures/authored.mmap"
                    )
                    .to_vec();
                }
                if matches!(id, ProductId::MmapTile(_)) {
                    data = if t.y == 31 {
                        include_bytes!(
                            "../../../../../../../benilla-native-nav/tools/fixtures/0003231.mmtile"
                        )
                        .as_slice()
                    } else {
                        include_bytes!(
                            "../../../../../../../benilla-native-nav/tools/fixtures/0003232.mmtile"
                        )
                        .as_slice()
                    }
                    .to_vec();
                }
                p.id = id;
                p.bytes = data.len() as u64;
                p.sha256 = Sha256::digest(&data).into();
                p.dependencies = p
                    .dependencies
                    .into_iter()
                    .map(|d| match d {
                        ProductId::Terrain(_) => ProductId::Terrain(t),
                        ProductId::VmapTile(_) => ProductId::VmapTile(t),
                        d => d,
                    })
                    .collect();
                newfiles.insert(p.id.filename(spec.map).unwrap(), data);
                specs.push(p);
            }
        }
        spec.tiles = wanted.to_vec();
        spec.products = specs;
        (spec, newfiles)
    }
    #[test]
    fn inventoried_navigation_echoes_binding_and_preserves_failed_requests() {
        let p = products();
        if !benilla_native_nav::SUPPORTED_ABI {
            assert!(matches!(p.load_navigation(), Err(Error::Unsupported)));
            return;
        }
        let snapshot = p.load_navigation().unwrap();
        let request = RouteRequest {
            snapshot: snapshot.binding(),
            ticket: 1,
            from: [-400f32, -400., 0.].map(f32::to_bits),
            to: [-400f32, 400., 7.5].map(f32::to_bits),
        };
        let reply = snapshot
            .query(request, Instant::now() + Duration::from_secs(2))
            .unwrap();
        assert_eq!(reply.request, request);
        for field in 0..6 {
            let mut changed = request;
            match field {
                0 => changed.snapshot.map += 1,
                1 => changed.snapshot.content = [99; 32],
                2 => changed.snapshot.source_epoch = [99; 32],
                3 => changed.snapshot.inventory = [99; 32],
                4 => changed.snapshot.policy = [99; 32],
                _ => changed.ticket = 0,
            };
            assert_eq!(
                snapshot.query(changed, Instant::now() + Duration::from_secs(2)),
                Err(benilla_native_nav::Error::Identity)
            );
        }
        assert_eq!(
            snapshot.query(request, Instant::now()),
            Err(benilla_native_nav::Error::Deadline)
        );
    }

    #[test]
    fn source_tile_pair_keeps_original_bits_source_boundary_and_native_identity() {
        let snapshot = products().load_navigation().unwrap();
        let until = || Instant::now() + Duration::from_secs(5);
        let grid = 533.3333f32;
        let boundary = [-400., -grid, 1.].map(f32::to_bits);
        let request = SourceTileRequest {
            snapshot: snapshot.binding(),
            ticket: 9,
            from: boundary,
            to: [-400., -530., 2.].map(f32::to_bits),
        };
        let reply = snapshot.source_tile_presence(request, until()).unwrap();
        assert_eq!(reply.request(), request);
        assert_eq!(reply.result().from(), request.from);
        assert_eq!(reply.result().to(), request.to);
        assert_eq!(reply.result().start().coordinates(), [0, 0]);
        assert_eq!(reply.result().end().unwrap().coordinates(), [0, 0]);
        assert!(reply.result().both_present());
        reply.result().check_mesh(&snapshot.mesh.borrow()).unwrap();
        let reloaded = products().load_navigation().unwrap();
        assert_eq!(snapshot.binding(), reloaded.binding());
        assert_eq!(
            reply.result().check_mesh(&reloaded.mesh.borrow()),
            Err(benilla_native_nav::Error::Identity)
        );
        let mut missing = request;
        missing.from[1] = (-grid).to_bits() + 1;
        let negative = snapshot.source_tile_presence(missing, until()).unwrap();
        assert_eq!(negative.result().start().coordinates(), [-1, 0]);
        assert!(!negative.result().start().present());
        assert!(negative.result().end().is_none());
        for edit in 0..6 {
            let mut changed = request;
            match edit {
                0 => changed.snapshot.map += 1,
                1 => changed.snapshot.content[0] ^= 1,
                2 => changed.snapshot.source_epoch[0] ^= 1,
                3 => changed.snapshot.inventory[0] ^= 1,
                4 => changed.snapshot.policy[0] ^= 1,
                _ => changed.ticket = 0,
            }
            assert_eq!(
                snapshot.source_tile_presence(changed, until()),
                Err(benilla_native_nav::Error::Identity)
            );
        }
        assert_eq!(
            snapshot.source_tile_presence(request, Instant::now()),
            Err(benilla_native_nav::Error::Deadline)
        );
        let borrowing = snapshot.mesh.borrow_mut();
        assert_eq!(
            snapshot.source_tile_presence(request, until()),
            Err(benilla_native_nav::Error::Unavailable)
        );
        drop(borrowing);
        assert!(snapshot.source_tile_presence(request, until()).is_ok());
    }

    #[test]
    fn source_valid_z_outside_native_domain_is_unsupported_not_a_tile_negative() {
        let snapshot = products().load_navigation().unwrap();
        let request = SourceTileRequest {
            snapshot: snapshot.binding(),
            ticket: 9,
            from: [-400., -400., 0.].map(f32::to_bits),
            to: [-400., -390., 100_001.].map(f32::to_bits),
        };
        assert_eq!(
            benilla_native_nav::source_map_coord_pair(request.from, request.to),
            Ok(())
        );
        assert_eq!(
            snapshot.source_tile_presence(request, Instant::now() + Duration::from_secs(5)),
            Err(benilla_native_nav::Error::Unsupported)
        );
    }

    #[test]
    fn source_stage_is_separate_and_echoes_original_inputs_filter_configuration_and_ticket() {
        if !benilla_native_nav::SUPPORTED_ABI {
            return;
        }
        let snapshot = products().load_navigation().unwrap();
        let request = SourceSmoothRequest {
            snapshot: snapshot.binding(),
            ticket: 7,
            // Distinct vertical bits expose replacement by a snapped endpoint.
            from: [-400f32, -400., 1.].map(f32::to_bits),
            to: [-400f32, -390., 2.].map(f32::to_bits),
            filter: SourceOwnerFilter {
                include_flags: 1,
                exclude_flags: 0,
            },
            point_limit: 74,
            budget: SourceSmoothingBudget::default(),
        };
        let deadline = || Instant::now() + Duration::from_secs(5);
        let reply = snapshot.source_smooth(request, deadline()).unwrap();
        assert_eq!(reply.request, request);
        assert_eq!(reply.result.from, request.from);
        assert_eq!(reply.result.to, request.to);
        assert_ne!(reply.result.start.point, request.from);
        assert_ne!(reply.result.end.point, request.to);
        for field in 0..6 {
            let mut changed = request;
            match field {
                0 => changed.snapshot.map += 1,
                1 => changed.snapshot.content[0] ^= 1,
                2 => changed.snapshot.source_epoch[0] ^= 1,
                3 => changed.snapshot.inventory[0] ^= 1,
                4 => changed.snapshot.policy[0] ^= 1,
                _ => changed.ticket = 0,
            }
            assert_eq!(
                snapshot.source_smooth(changed, deadline()),
                Err(SourceSmoothError::Identity)
            );
        }
        let mut changed = request;
        changed.ticket += 1;
        let next = snapshot.source_smooth(changed, deadline()).unwrap();
        assert_ne!(next.request, reply.request);
        assert_eq!(next.result, reply.result);
        let mut exhausted = request;
        exhausted.budget.smoothing_primitives = 0;
        assert!(matches!(
            snapshot.source_smooth(exhausted, deadline()),
            Err(SourceSmoothError::Native(SourceSmoothingError {
                error: benilla_native_nav::Error::Budget,
                ..
            }))
        ));
        assert!(matches!(
            snapshot.source_smooth(request, Instant::now()),
            Err(SourceSmoothError::Native(SourceSmoothingError {
                error: benilla_native_nav::Error::Deadline,
                ..
            }))
        ));
        assert!(snapshot.source_smooth(request, deadline()).is_ok());
    }
}
