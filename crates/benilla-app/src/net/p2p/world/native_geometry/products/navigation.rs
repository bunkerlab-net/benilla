// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded dry MMap query product. Successful routes describe this snapshot only.
use super::{Error, Hash, InventoriedProducts, ProductId};
use benilla_native_nav::{Mesh, Route, TileInput};
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
pub(crate) struct NavigationSnapshot {
    binding: SnapshotBinding,
    mesh: RefCell<Mesh>,
}
impl InventoriedProducts {
    pub(crate) fn load_navigation(&self) -> Result<NavigationSnapshot, Error> {
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
}
