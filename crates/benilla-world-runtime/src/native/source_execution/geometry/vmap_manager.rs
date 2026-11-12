// SPDX-License-Identifier: GPL-3.0-or-later
//! VMapManager2 lifetime authority, CMaNGOS 8ec338a1 VMapManager2.cpp:36-346.
use super::*;
use benilla_native_vmap::live::{self, AreaInfo, LoadResult, Manager};
use std::time::{Duration, Instant};
pub struct SourceVMapManager {
    pub(super) created: bool,
    thread_safe: bool,
    settings: [bool; 2],
    pub collision: Manager,
    pub configuration: Option<(Sha256Id, [usize; 2])>,
}
impl Default for SourceVMapManager {
    fn default() -> Self {
        Self {
            created: false,
            thread_safe: true,
            settings: [true, true],
            collision: Manager::new(),
            configuration: None,
        }
    }
}
pub(super) fn vmap_error(error: live::Error) -> Refusal {
    refuse(match error {
        live::Error::Format | live::Error::Framing => "execution.vmap.product_format",
        live::Error::SourceReadFailure => "execution.vmap.source_model_read_error",
        live::Error::Budget => "execution.vmap.resource_limit",
        live::Error::Deadline => "execution.vmap.query_deadline",
        live::Error::Missing => "execution.vmap.model_dependency",
        live::Error::Unsupported => "execution.vmap.unsupported_product",
        live::Error::ProductUnavailable => "execution.vmap.product_unavailable",
        live::Error::ImmutableSource => "execution.vmap.source_mutation",
        live::Error::UndefinedSlot => "execution.vmap.slot_write_undefined",
        live::Error::UndefinedRead => "execution.vmap.storage_read_undefined",
        live::Error::NumericBinding => "execution.vmap.glibc236_fma_rotation_binding",
        live::Error::RotationDomain => "execution.vmap.rotation_outside_bound_domain",
        live::Error::ArithmeticEnvironment => "execution.vmap.arithmetic_environment",
        live::Error::StaleGeneration => "execution.vmap.stale_generation",
        live::Error::InvalidMap => "execution.vmap.thread_unsafe_map_assertion",
    })
}
impl SourceVMapManager {
    pub fn created(&self) -> bool {
        self.created
    }
    pub fn loaded_trees(&self) -> usize {
        self.collision.loaded_trees()
    }
    pub fn thread_safe(&self) -> bool {
        self.thread_safe
    }
    pub fn map_slots(&self) -> usize {
        self.collision.map_slots()
    }
    pub fn settings(&self) -> [bool; 2] {
        self.settings
    }
    pub fn has_tree(&self, map: u32) -> bool {
        self.collision.has_tree(map)
    }
    /// World.cpp:817-824,912-919; seal16-full-01/mangosd.conf:265-266.
    pub fn initialize_recorded_world(
        &mut self,
        maps: impl IntoIterator<Item = u32>,
        bytes: &[u8],
        approved: &Sha256Id,
    ) -> Result<()> {
        require(
            Sha256Id::of(bytes) == *approved,
            "execution.terrain.vmap_config_identity",
        )?;
        let (los_line, los) = super::spawn_loader_config::assignment(bytes, "vmap.enableLOS")?;
        let (height_line, height) =
            super::spawn_loader_config::assignment(bytes, "vmap.enableHeight")?;
        let enabled =
            |v: &str| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes");
        self.created = true;
        self.settings = [enabled(&los), enabled(&height)];
        self.collision.configure(self.settings[0], self.settings[1]);
        self.configuration = Some((approved.clone(), [los_line, height_line]));
        self.collision.initialize_thread_unsafe(maps);
        self.thread_safe = false;
        Ok(())
    }
    pub fn load_map(&mut self, map: u32, x: u16, y: u16) -> Result<LoadResult> {
        self.created = true;
        self.collision
            .load_map(map, u32::from(x), u32::from(y))
            .map_err(vmap_error)
    }
    pub fn unload_tile(&mut self, map: u32, x: u16, y: u16) -> Result<()> {
        self.collision
            .unload_tile(map, u32::from(x), u32::from(y))
            .map_err(vmap_error)
    }
    pub fn unload_map(&mut self, map: u32) {
        self.collision.unload_map(map);
    }
    pub fn is_tile_loaded(&self, map: u32, x: u16, y: u16) -> bool {
        self.collision
            .is_tile_loaded(map, u32::from(x), u32::from(y))
    }
    pub fn area_info(&self, map: u32, pose: [f32; 3]) -> Result<Option<AreaInfo>> {
        self.collision
            .area_info(map, pose, Instant::now() + Duration::from_secs(30))
            .map_err(vmap_error)
    }
}
