// SPDX-License-Identifier: GPL-3.0-or-later
//! Map.cpp:712,2668-2672,2767-2779 and DynamicTree.cpp:60-100.
use super::*;
use benilla_native_vmap::live::DynamicMapTree;
use std::time::{Duration, Instant};
pub struct SourceDynamicMap {
    map: MapIdentity,
    pub tree: DynamicMapTree,
}
impl SourceDynamicMap {
    pub fn new(map: MapIdentity) -> Self {
        Self {
            map,
            tree: DynamicMapTree::new(),
        }
    }
    pub fn map(&self) -> MapIdentity {
        self.map
    }
    pub fn update(&mut self, models: &SourceDynamicModels, diff: u32) -> Result<()> {
        self.tree
            .update(&models.models, diff)
            .map_err(dynamic_error)
    }
    pub fn line_of_sight(
        &mut self,
        models: &SourceDynamicModels,
        vmap: &SourceVMapManager,
        from: [f32; 3],
        to: [f32; 3],
        ignore_m2: bool,
    ) -> Result<bool> {
        self.tree
            .line_of_sight(
                &models.models,
                &vmap.collision,
                from,
                to,
                ignore_m2,
                Instant::now() + Duration::from_secs(30),
            )
            .map_err(dynamic_error)
    }
    pub fn balance(&mut self, models: &SourceDynamicModels) -> Result<()> {
        self.tree.balance(&models.models).map_err(dynamic_error)
    }
    pub fn height(
        &mut self,
        models: &SourceDynamicModels,
        vmap: &SourceVMapManager,
        position: [f32; 3],
        search: f32,
    ) -> Result<f32> {
        self.tree
            .height(
                &models.models,
                &vmap.collision,
                position,
                search,
                Instant::now() + Duration::from_secs(30),
            )
            .map_err(dynamic_error)
    }
    /// Map.cpp:2758-2765 combines static terrain and the live dynamic tree.
    pub(super) fn map_height(
        &mut self,
        models: &SourceDynamicModels,
        terrain: &mut SourceTerrainManager,
        [x, y, z]: [f32; 3],
        swim: bool,
    ) -> Result<f32> {
        use benilla_native_terrain::{
            grid_height::DEFAULT_HEIGHT_SEARCH, grid_liquid::DEFAULT_WATER_SEARCH,
        };
        let distance = if swim {
            DEFAULT_WATER_SEARCH
        } else {
            DEFAULT_HEIGHT_SEARCH
        };
        let ground = terrain.height_static(self.map.map, [x, y, z], true, distance)?;
        let search_z = 2.0 + if z < ground { ground } else { z };
        let dynamic = self.height(models, &terrain.vmap, [x, y, search_z], search_z - ground)?;
        Ok(if ground < dynamic { dynamic } else { ground })
    }
    pub fn hit_position(
        &mut self,
        models: &SourceDynamicModels,
        vmap: &SourceVMapManager,
        from: [f32; 3],
        to: [f32; 3],
        modify: f32,
    ) -> Result<(bool, [f32; 3])> {
        self.tree
            .hit_position(
                &models.models,
                &vmap.collision,
                from,
                to,
                modify,
                Instant::now() + Duration::from_secs(30),
            )
            .map_err(dynamic_error)
    }
}
