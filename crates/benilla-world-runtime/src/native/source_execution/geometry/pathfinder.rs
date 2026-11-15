// SPDX-License-Identifier: GPL-3.0-or-later
//! PathFinder.cpp:35-54,1030-1120. Query ownership and liquid filtering.
use super::*;
use benilla_native_nav::{
    live::{PathCache, PathDistance, QueryHandle},
    SourceOwnerFilter, SourceWarmLookup,
};
use benilla_native_terrain::grid_liquid::{LiquidData, Status};

// MoveMapSharedDefines.h:45-60, bits are area differences, not liquid type bits.
const NAV_GROUND: u16 = 1;
const NAV_WATER: u16 = 4;
const NAV_MAGMA_SLIME: u16 = 8;

#[derive(Debug)]
pub enum SourceDefaultQuery {
    /// Source :50 never wrote this pointer. Refuse at its first read, not construction.
    Unwritten,
    Lookup(Option<QueryHandle>),
}
/// Explicit attachment ownership, never inferred from a world pose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourcePathTransport {
    Unbound,
    None,
    Attached,
}
#[derive(Debug)]
pub struct SourcePathFinder {
    pub unit: ObjectHandle,
    pub default_map: MapIdentity,
    pub ignore_normalization: bool,
    pub default_query: SourceDefaultQuery,
    pub current_query: Option<QueryHandle>,
    pub current_mesh_map: Option<u32>,
    pub filter: SourceOwnerFilter,
    pub cache: PathCache,
    pub cached_points: Vec<f32>,
    pub smooth_refs: Vec<u64>,
    pub points: Vec<[f32; 3]>,
    pub path_type: u32,
    pub use_straight_path: bool,
    pub force_destination: bool,
    pub straight_line: bool,
    pub transport: SourcePathTransport,
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub actual_end: [f32; 3],
    /// Unit model-derived width, bound only by its concrete shape owner.
    pub collision_width: Option<f32>,
    attachment: Option<Result<Option<ObjectHandle>>>,
}
impl SourcePathFinder {
    pub const DISTANCE_INTENT: (&'static str, u32, &'static str) = (
        benilla_native_nav::SOURCE_PATH_DISTANCE_INTENT_ID,
        benilla_native_nav::SOURCE_PATH_DISTANCE_INTENT_VERSION,
        benilla_native_nav::SOURCE_PATH_DISTANCE_INTENT_REVISION,
    );
    pub fn construct(
        geometry: &mut SourceTerrainManager,
        unit: ObjectHandle,
        context: &MotionContext<'_>,
        ignore_normalization: bool,
    ) -> Result<Self> {
        let object = context.objects.get(unit)?;
        let map = object
            .map
            .ok_or_else(|| refuse("execution.pathfinder.map_owner"))?;
        let fields = context.units.get(context.objects, unit)?;
        // Creature.cpp:3065-3073, Unit.cpp:12363-12369. Disable precedes force.
        let forcing = if fields.mmap_extra_flags & 0x4000 != 0 {
            MMapForcing::Ignored
        } else if fields.mmap_extra_flags & 0x2000 != 0 || fields.flags & 8 != 0 {
            MMapForcing::Forced
        } else {
            MMapForcing::Default
        };
        let default_query = if geometry.mmap.pathfinding_enabled(map.map, forcing)? {
            SourceDefaultQuery::Lookup(geometry.mmap.query(map.map, map.instance))
        } else {
            SourceDefaultQuery::Unwritten
        };
        let include_flags = match object.kind {
            SourceObjectKind::Creature => {
                (if fields.can_walk { NAV_GROUND } else { 0 })
                    | if fields.can_swim {
                        NAV_WATER | NAV_MAGMA_SLIME
                    } else {
                        0
                    }
            }
            SourceObjectKind::Pet => return Err(refuse("execution.pathfinder.pet_can_swim_owner")),
            SourceObjectKind::Player => {
                return Err(refuse("execution.pathfinder.player_water_owner"))
            }
            _ => return Err(refuse("execution.pathfinder.not_unit")),
        };
        let mut result = Self {
            unit,
            default_map: map,
            ignore_normalization,
            default_query,
            current_query: None,
            current_mesh_map: None,
            filter: SourceOwnerFilter {
                include_flags,
                exclude_flags: 0,
            },
            cache: PathCache::new(),
            cached_points: vec![0.; 74 * 3],
            smooth_refs: vec![0; 74],
            points: Vec::new(),
            path_type: 0,
            use_straight_path: false,
            force_destination: false,
            straight_line: false,
            transport: SourcePathTransport::Unbound,
            start: [0.; 3],
            end: [0.; 3],
            actual_end: [0.; 3],
            collision_width: None,
            attachment: None,
        };
        result.update_filter(geometry, context.objects)?;
        geometry.pathfinder_constructor_completions += 1;
        Ok(result)
    }
    /// PathFinder.cpp:1092 uses short-circuit IsInWater || IsUnderwater, then queries again.
    pub fn update_filter(
        &mut self,
        geometry: &mut SourceTerrainManager,
        objects: &SourceObjects,
    ) -> Result<()> {
        let object = objects.get(self.unit)?;
        let map = object
            .map
            .ok_or_else(|| refuse("execution.pathfinder.map_owner"))?;
        let p = objects.pose(self.unit)?.0.map(f32::from_bits);
        let p = [p[0], p[1], p[2]];
        if geometry.is_in_water(map.map, p, None)? || geometry.is_under_water(map.map, p, None)? {
            let mut data = LiquidData::default();
            let status = geometry.liquid_status(
                map.map,
                p,
                15,
                benilla_native_terrain::grid_liquid::DEFAULT_COLLISION_HEIGHT,
                Some(&mut data),
            )?;
            let flag = if status == Status::NoWater {
                NAV_GROUND
            } else {
                match data.type_flags {
                    1 | 2 => NAV_WATER,
                    4 | 8 => NAV_MAGMA_SLIME,
                    _ => NAV_GROUND,
                }
            };
            self.filter.include_flags |= flag;
        }
        Ok(())
    }
    pub fn read_default_query(&self) -> Result<Option<&QueryHandle>> {
        match &self.default_query {
            SourceDefaultQuery::Unwritten => {
                Err(refuse("execution.pathfinder.undefined_default_query_read"))
            }
            SourceDefaultQuery::Lookup(query) => Ok(query.as_ref()),
        }
    }
    /// Current selection is explicit. Never substitute the default or a new query.
    pub fn nearest_poly(
        &mut self,
        geometry: &SourceTerrainManager,
        point: [f32; 3],
        distance: &mut PathDistance,
    ) -> Result<Option<SourceWarmLookup>> {
        let map = self
            .current_mesh_map
            .ok_or_else(|| refuse("execution.pathfinder.current_mesh_owner"))?;
        let query = self
            .current_query
            .as_ref()
            .ok_or_else(|| refuse("execution.pathfinder.current_query_owner"))?;
        geometry
            .mmap
            .mesh(map)
            .ok_or_else(|| refuse("execution.mmap.stale_query"))?
            .pathfinder_lookup(query, &mut self.cache, self.filter, point, distance)
            .map_err(mmap_manager::mmap_error)
    }
    /// PathFinder.h:146-150 does not erase the inactive refs or their appended tail.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.points.clear();
    }
}

impl MotionExecutionOwner for SourceTerrainManager {
    fn pathfinder_geometry(&mut self) -> Result<&mut SourceTerrainManager> {
        Ok(self)
    }
}

#[cfg(test)]
#[path = "pathfinder_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "pathfinder_census.rs"]
pub(super) mod census;

#[path = "pathfinder_path.rs"]
mod path;
