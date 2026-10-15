// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded dry MMap query product. Successful routes describe this snapshot only.
use super::{Error, Hash, InventoriedProducts, ProductId};
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
        hash.update(b"benilla-pinned-dry-detour-nearest-sliced-funnel-v1");
        hash.update(self.revision());
        hash.update(super::SOURCE_PIN);
        hash.update(super::DETOUR_TREE);
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
                    continue
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
