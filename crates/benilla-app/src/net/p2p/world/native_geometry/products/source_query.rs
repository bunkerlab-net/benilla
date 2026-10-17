// SPDX-License-Identifier: GPL-3.0-or-later
//! Distinct actual1024 map-instance product loaded independently for later
//! owner-constructor composition; this loader does not establish dispatch timing.
//! The configured source instance slot is not the private native allocation ID.
use super::{
    liquid::DryLiquidSnapshot, navigation::SnapshotBinding, vmap::VmapSnapshot, Error, Hash,
    InventoriedProducts, ProductId, VmapLiquidProducts,
};
use benilla_native_nav::{SourceMapQuery, SourceMapQuerySlot, TileInput, SOURCE_MAP_QUERY_POLICY};
use benilla_world_runtime::native::{MMapForcingStatus, Profile};
use sha2::{Digest, Sha256};

pub(crate) struct SourceMapQuerySnapshot {
    binding: SnapshotBinding,
    configuration: Hash,
    query: SourceMapQuery,
}
impl SourceMapQuerySnapshot {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }
    pub(crate) fn configuration_binding(&self) -> Hash {
        self.configuration
    }
    pub(crate) fn query(&self) -> &SourceMapQuery {
        &self.query
    }
}

/// Exact dry proof and VMap accompany the distinct query without changing the
/// legacy DryRouteProjection or granting any movement/constructor authority.
pub(crate) struct SourceQueryProjection {
    liquid: DryLiquidSnapshot,
    navigation: SourceMapQuerySnapshot,
    vmap: VmapSnapshot,
}
impl SourceQueryProjection {
    pub(crate) fn liquid(&self) -> &DryLiquidSnapshot {
        &self.liquid
    }
    pub(crate) fn navigation(&self) -> &SourceMapQuerySnapshot {
        &self.navigation
    }
    pub(crate) fn static_vmap(&self) -> &VmapSnapshot {
        &self.vmap
    }
    pub(crate) fn into_parts(self) -> (DryLiquidSnapshot, SourceMapQuerySnapshot, VmapSnapshot) {
        (self.liquid, self.navigation, self.vmap)
    }
}
impl VmapLiquidProducts {
    pub(crate) fn load_source_query_projection(
        &self,
        profile: &Profile,
    ) -> Result<SourceQueryProjection, Error> {
        let (liquid, vmap) = self.inventory.load_dry_liquid_and_vmap()?;
        let navigation = self.inventory.load_source_map_query(&liquid, profile)?;
        Ok(SourceQueryProjection {
            liquid,
            navigation,
            vmap,
        })
    }
}
impl InventoriedProducts {
    fn load_source_map_query(
        &self,
        proof: &DryLiquidSnapshot,
        profile: &Profile,
    ) -> Result<SourceMapQuerySnapshot, Error> {
        profile.validate().map_err(|_| Error::Profile)?;
        let configuration = profile.pathfinder_configuration().ok_or(Error::Missing)?;
        if !self.vmap_liquid_profile
            || profile.map_id != self.map
            || profile.content_digest != self.content
            || configuration.map_id() != self.map
            || configuration.raw_instance_type() != 0
            || configuration.source_is_dungeon()
            || configuration.selected().source_instance_id != 0
            || !configuration
                .selected()
                .pathfinding_enabled(self.map, MMapForcingStatus::Default)
                .map_err(|_| Error::Profile)?
        {
            return Err(Error::Unsupported);
        }
        proof.verify_inventory(self)?;
        let params = self.inventoried_bytes(self.map, &ProductId::MmapParams)?;
        let mut tiles = Vec::new();
        for (id, product) in &self.products {
            if let ProductId::MmapTile(tile) = id {
                tiles.try_reserve(1).map_err(|_| Error::Budget)?;
                tiles.push(TileInput {
                    grid: [tile.x, tile.y],
                    bytes: &product.bytes,
                });
            }
        }
        let slot = SourceMapQuerySlot {
            map_id: self.map,
            instance_id: configuration.selected().source_instance_id,
        };
        let query = SourceMapQuery::load(slot, params, &tiles).map_err(|error| match error {
            benilla_native_nav::Error::Budget => Error::Budget,
            benilla_native_nav::Error::Unsupported => Error::Unsupported,
            benilla_native_nav::Error::Identity => Error::Identity,
            benilla_native_nav::Error::Framing => Error::Framing,
            _ => Error::Format,
        })?;
        let configuration = configuration.binding();
        let mut policy = Sha256::new();
        policy.update(SOURCE_MAP_QUERY_POLICY);
        policy.update(super::SOURCE_PIN);
        policy.update(super::DETOUR_TREE);
        policy.update(self.revision());
        policy.update(proof.binding().policy);
        policy.update(configuration);
        policy.update(slot.map_id.to_le_bytes());
        policy.update(slot.instance_id.to_le_bytes());
        policy.update(query.query_capacity().to_le_bytes());
        Ok(SourceMapQuerySnapshot {
            binding: SnapshotBinding {
                map: self.map,
                content: self.content,
                source_epoch: self.source_epoch,
                inventory: self.revision(),
                policy: policy.finalize().into(),
            },
            configuration,
            query,
        })
    }
}
