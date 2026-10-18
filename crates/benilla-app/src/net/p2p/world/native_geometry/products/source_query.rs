// SPDX-License-Identifier: GPL-3.0-or-later
//! Distinct actual1024 map-instance product loaded independently for later
//! owner-constructor composition; this loader does not establish dispatch timing.
//! The configured source instance slot is not the private native allocation ID.
use super::{
    liquid::DryLiquidSnapshot, navigation::SnapshotBinding, vmap::VmapSnapshot, Error, Hash,
    InventoriedProducts, ProductId, VmapLiquidProducts,
};
use benilla_native_nav::{
    QueryProfile, ReconstructedSourceChaseFixedCache, ReconstructedSourceWanderAndChase,
    ReconstructedSourceWanderTerminal, SourceChaseFixedCacheChallenge, SourceChaseFixedCacheError,
    SourceChaseFixedCacheRecipe, SourceChaseFixedCacheStage, SourceMapQuery, SourceMapQuerySlot,
    SourceWanderAndChaseReconstructionError, SourceWanderReconstructionChallenge,
    SourceWanderReconstructionError, SourceWanderReconstructionRecipe,
    SourceWanderReconstructionStage, TileInput, SOURCE_MAP_QUERY_POLICY,
};
use benilla_world_runtime::native::{MMapForcingStatus, Profile};
use sha2::{Digest, Sha256};
use std::time::Instant;

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

/// Inventory-owned immutable geometry available before a native query exists.
/// The navigation binding describes the selected loading policy, not loaded authority.
pub(crate) struct SourceQueryGeometry {
    liquid: DryLiquidSnapshot,
    vmap: VmapSnapshot,
    navigation: SnapshotBinding,
    configuration: Hash,
    slot: SourceMapQuerySlot,
}
impl SourceQueryGeometry {
    pub(crate) fn navigation_binding(&self) -> SnapshotBinding {
        self.navigation
    }
    pub(crate) fn configuration_binding(&self) -> Hash {
        self.configuration
    }
    pub(crate) fn liquid(&self) -> &DryLiquidSnapshot {
        &self.liquid
    }
    pub(crate) fn static_vmap(&self) -> &VmapSnapshot {
        &self.vmap
    }
    fn check_query(&self, query: &SourceMapQuery) -> Result<(), Error> {
        if query.slot() != self.slot
            || query.query_profile() != QueryProfile::SourceMapInstance1024
            || query.query_capacity() != QueryProfile::SourceMapInstance1024.node_capacity()
        {
            return Err(Error::Identity);
        }
        Ok(())
    }
}

/// Exact inventoried bytes and immutable products; no replacement-query setter.
pub(crate) struct StagedSourceQueryProjection<'inventory> {
    inventory: &'inventory InventoriedProducts,
    params: &'inventory [u8],
    tiles: Vec<TileInput<'inventory>>,
    geometry: SourceQueryGeometry,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SourceQueryReconstructionError<E> {
    Product(Error),
    Native(SourceWanderAndChaseReconstructionError),
    Acceptance(E),
}
impl StagedSourceQueryProjection<'_> {
    pub(crate) fn geometry(&self) -> &SourceQueryGeometry {
        &self.geometry
    }
    pub(crate) fn navigation_binding(&self) -> SnapshotBinding {
        self.geometry.navigation_binding()
    }
    pub(crate) fn configuration_binding(&self) -> Hash {
        self.geometry.configuration_binding()
    }
    pub(crate) fn liquid(&self) -> &DryLiquidSnapshot {
        self.geometry.liquid()
    }
    pub(crate) fn static_vmap(&self) -> &VmapSnapshot {
        self.geometry.static_vmap()
    }
    /// A filter may own an independent proof over the same decoded VMap/inventory.
    /// This neither reloads VMap nor allocates a navigation query.
    pub(crate) fn filter_liquid(&self) -> Result<DryLiquidSnapshot, Error> {
        self.inventory.dry_liquid_for_vmap(&self.geometry.vmap)
    }
    fn load(self) -> Result<SourceQueryProjection, Error> {
        let query = SourceMapQuery::load(self.geometry.slot, self.params, &self.tiles)
            .map_err(native_product_error)?;
        self.geometry.check_query(&query)?;
        Ok(SourceQueryProjection {
            liquid: self.geometry.liquid,
            navigation: SourceMapQuerySnapshot {
                binding: self.geometry.navigation,
                configuration: self.geometry.configuration,
                query,
            },
            vmap: self.geometry.vmap,
        })
    }

    /// The factory owns the single fresh allocation. Acceptance keeps its opaque
    /// dual proof intact; only this boundary can wrap the returned actual query.
    pub(crate) fn reconstruct_wander_and_chase_observed<T, E>(
        self,
        wander_recipe: &SourceWanderReconstructionRecipe,
        wander_challenge: &SourceWanderReconstructionChallenge,
        chase_recipe: &SourceChaseFixedCacheRecipe,
        chase_challenge: &SourceChaseFixedCacheChallenge,
        deadline: Instant,
        mut wander_observer: impl FnMut(
            &SourceQueryGeometry,
            SourceWanderReconstructionStage<'_>,
        ) -> Result<(), SourceWanderReconstructionError>,
        mut chase_observer: impl FnMut(
            &SourceQueryGeometry,
            SourceChaseFixedCacheStage<'_>,
        ) -> Result<(), SourceChaseFixedCacheError>,
        accept_pair: impl FnOnce(
            &SourceQueryGeometry,
            ReconstructedSourceWanderAndChase,
        ) -> Result<(T, ReconstructedSourceWanderAndChase), E>,
    ) -> Result<
        (
            T,
            SourceQueryProjection,
            ReconstructedSourceWanderTerminal,
            ReconstructedSourceChaseFixedCache,
        ),
        SourceQueryReconstructionError<E>,
    > {
        let pair = SourceMapQuery::reconstruct_wander_and_chase_observed(
            self.geometry.slot,
            self.params,
            &self.tiles,
            wander_recipe,
            wander_challenge,
            chase_recipe,
            chase_challenge,
            deadline,
            |stage| wander_observer(&self.geometry, stage),
            |stage| chase_observer(&self.geometry, stage),
        )
        .map_err(SourceQueryReconstructionError::Native)?;
        self.geometry
            .check_query(pair.query())
            .map_err(SourceQueryReconstructionError::Product)?;
        let original_allocation = pair.query().retain_default();
        let (accepted, pair) = accept_pair(&self.geometry, pair)
            .map_err(SourceQueryReconstructionError::Acceptance)?;
        original_allocation
            .check_query(pair.query())
            .and_then(|()| pair.check_query(pair.query()))
            .map_err(|error| {
                SourceQueryReconstructionError::Product(native_product_error(error))
            })?;
        if !pair.matches_challenges(wander_challenge, chase_challenge) {
            return Err(SourceQueryReconstructionError::Product(Error::Identity));
        }
        if Instant::now() >= deadline {
            return Err(SourceQueryReconstructionError::Product(Error::Deadline));
        }
        let (query, wander, chase) = pair.into_parts();
        let projection = SourceQueryProjection {
            liquid: self.geometry.liquid,
            navigation: SourceMapQuerySnapshot {
                binding: self.geometry.navigation,
                configuration: self.geometry.configuration,
                query,
            },
            vmap: self.geometry.vmap,
        };
        Ok((accepted, projection, wander, chase))
    }
}

impl VmapLiquidProducts {
    pub(crate) fn load_source_query_projection(
        &self,
        profile: &Profile,
    ) -> Result<SourceQueryProjection, Error> {
        self.stage_source_query_projection(profile)?.load()
    }
    pub(crate) fn stage_source_query_projection(
        &self,
        profile: &Profile,
    ) -> Result<StagedSourceQueryProjection<'_>, Error> {
        let (liquid, vmap) = self.inventory.load_dry_liquid_and_vmap()?;
        self.inventory.stage_source_map_query(liquid, vmap, profile)
    }
}
impl InventoriedProducts {
    fn stage_source_map_query(
        &self,
        proof: DryLiquidSnapshot,
        vmap: VmapSnapshot,
        profile: &Profile,
    ) -> Result<StagedSourceQueryProjection<'_>, Error> {
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
        let static_binding = vmap.binding();
        if static_binding.map != self.map
            || static_binding.content != self.content
            || static_binding.inventory != self.revision()
            || static_binding.source_epoch != self.source_epoch
        {
            return Err(Error::Identity);
        }
        vmap.require_complete_dry_map()?;
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
        policy.update(
            QueryProfile::SourceMapInstance1024
                .node_capacity()
                .to_le_bytes(),
        );
        Ok(StagedSourceQueryProjection {
            inventory: self,
            params,
            tiles,
            geometry: SourceQueryGeometry {
                liquid: proof,
                vmap,
                navigation: SnapshotBinding {
                    map: self.map,
                    content: self.content,
                    source_epoch: self.source_epoch,
                    inventory: self.revision(),
                    policy: policy.finalize().into(),
                },
                configuration,
                slot,
            },
        })
    }
}

fn native_product_error(error: benilla_native_nav::Error) -> Error {
    match error {
        benilla_native_nav::Error::Budget => Error::Budget,
        benilla_native_nav::Error::Unsupported => Error::Unsupported,
        benilla_native_nav::Error::Identity => Error::Identity,
        benilla_native_nav::Error::Framing => Error::Framing,
        _ => Error::Format,
    }
}

#[cfg(test)]
#[path = "source_query/tests.rs"]
mod tests;
