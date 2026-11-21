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
