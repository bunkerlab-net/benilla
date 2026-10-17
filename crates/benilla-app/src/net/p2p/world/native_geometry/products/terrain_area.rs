// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual-byte TerrainInfo area resolution and TerrainManager zone/area selection.
//! Not a wet liquid status, player environment state, source lease or admission proof.
use super::{
    liquid::world_tile,
    terrain::{AreaEntry, AreaTable},
    vmap::{
        area::{AreaResult, AreaSnapshot},
        QueryBudget,
    },
    Error, Hash, PointHeight, VmapLiquidProducts,
};
use benilla_world_runtime::native::{SourceLocationCatalog, SourceLocationSnapshotBinding};
use sha2::{Digest, Sha256};
use std::{ops::Deref, sync::Arc, time::Instant};

mod stores;
use stores::{MapTable, WmoAreaTable};

const POLICY: &[u8] = b"cmangos-8ec338a1-terrain-area-complete-dbc-v1";
const INVALID_HEIGHT_VALUE: f32 = -200_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotBinding {
    pub map: u32,
    pub content: Hash,
    pub source_epoch: Hash,
    pub inventory: Hash,
    pub policy: Hash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerrainAreaRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    /// Exact original source-world binary32 pose. No caller-supplied area predicate.
    pub position: [u32; 3],
}

/// An owned result row from this query's complete AreaTable, not an input resolver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedArea {
    pub id: u32,
    pub map_id: u32,
    pub zone: u32,
    pub explore_flag: u32,
    pub liquid_override: u32,
}
impl From<AreaEntry> for ResolvedArea {
    fn from(entry: AreaEntry) -> Self {
        Self {
            id: entry.id,
            map_id: entry.map_id,
            zone: entry.zone,
            explore_flag: entry.explore_flag,
            liquid_override: entry.liquid_override,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerrainAreaResult {
    /// Independent raw floor/ADT and root/group/flags writes, before terrain rejection.
    pub raw_area: AreaResult,
    /// TerrainInfo::GetAreaInfo; None includes the terrain-between rejection.
    pub area_info: AreaResult,
    /// TerrainInfo::GetAreaFlag narrows exploreFlag to uint16 before final resolution.
    pub area_flag: u16,
    /// GetAreaEntryByAreaFlagAndMap over the narrowed flag and bound map.
    pub area: Option<ResolvedArea>,
    /// One parent by ID when area.zone != 0, for the later liquid-override branch.
    pub parent_area: Option<ResolvedArea>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerrainAreaReply {
    pub request: TerrainAreaRequest,
    pub result: TerrainAreaResult,
}

/// GetZoneAndAreaId does not look up the parent row. Present ID zero is retained
/// separately from a complete-store miss, even when both scalar IDs are zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerrainLocationResult {
    pub area_flag: u16,
    pub area: Option<ResolvedArea>,
    pub zone_id: u32,
    pub area_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerrainLocationReply {
    pub request: TerrainAreaRequest,
    pub result: TerrainLocationResult,
}

/// Borrowed readers and retained readers share the same immutable product closure.
#[derive(Clone, Debug)]
pub(super) enum WetProducts<'a> {
    Borrowed(&'a VmapLiquidProducts),
    Owned(Arc<VmapLiquidProducts>),
}
impl Deref for WetProducts<'_> {
    type Target = VmapLiquidProducts;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(products) => products,
            Self::Owned(products) => products,
        }
    }
}

#[derive(Debug)]
pub(crate) struct TerrainAreaSnapshot<'a> {
    binding: SnapshotBinding,
    products: WetProducts<'a>,
    // AreaSnapshot never exposes its wet geometry as a dry conservative reader.
    raw: AreaSnapshot,
    areas: AreaTable,
    wmo_areas: WmoAreaTable,
    maps: MapTable,
    location_catalog: SourceLocationCatalog,
}

impl VmapLiquidProducts {
    /// Decode complete real WDBC bytes, including valid empty stores. Missing bytes
    /// are never interpreted as empty stores or caller-authored area/closure claims.
    pub(crate) fn load_terrain_area(
        &self,
        area_table_dbc: &[u8],
        wmo_area_table_dbc: &[u8],
        map_dbc: &[u8],
    ) -> Result<TerrainAreaSnapshot<'_>, Error> {
        TerrainAreaSnapshot::load(
            WetProducts::Borrowed(self),
            area_table_dbc,
            wmo_area_table_dbc,
            map_dbc,
        )
    }

    /// Retain the same actual products; dropping the caller's Arc cannot unload them.
    pub(crate) fn load_terrain_area_owned(
        self: Arc<Self>,
        area_table_dbc: &[u8],
        wmo_area_table_dbc: &[u8],
        map_dbc: &[u8],
    ) -> Result<TerrainAreaSnapshot<'static>, Error> {
        TerrainAreaSnapshot::load(
            WetProducts::Owned(self),
            area_table_dbc,
            wmo_area_table_dbc,
            map_dbc,
        )
    }
}
impl<'a> TerrainAreaSnapshot<'a> {
    pub(super) fn load(
        products: WetProducts<'a>,
        area_table_dbc: &[u8],
        wmo_area_table_dbc: &[u8],
        map_dbc: &[u8],
    ) -> Result<Self, Error> {
        let areas = AreaTable::decode(area_table_dbc)?;
        // The source's inclusive u32 loop wraps when GetNumRows() == UINT32_MAX.
        // Keep this narrower guard local: old GridMap/dry AreaTable policies are unchanged.
        if areas.entry(u32::MAX - 1).is_some() {
            return Err(Error::Unsupported);
        }
        let wmo_areas = WmoAreaTable::decode(wmo_area_table_dbc)?;
        let maps = MapTable::decode(map_dbc)?;
        let location_catalog =
            SourceLocationCatalog::decode(area_table_dbc, wmo_area_table_dbc, map_dbc)
                .map_err(|_| Error::Format)?;
        let raw = products.load_vmap_area()?;
        let source = raw.binding();
        let mut policy = Sha256::new();
        policy.update(POLICY);
        policy.update(source.policy);
        policy.update(areas.digest());
        policy.update(wmo_areas.digest());
        policy.update(maps.digest());
        Ok(TerrainAreaSnapshot {
            binding: SnapshotBinding {
                map: source.map,
                content: source.content,
                source_epoch: source.source_epoch,
                inventory: source.inventory,
                policy: policy.finalize().into(),
            },
            products,
            raw,
            areas,
            wmo_areas,
            maps,
            location_catalog,
        })
    }
}

impl TerrainAreaSnapshot<'_> {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }

    pub(crate) fn location_binding(&self) -> SourceLocationSnapshotBinding {
        SourceLocationSnapshotBinding {
            map: self.binding.map,
            content: self.binding.content,
            source_epoch: self.binding.source_epoch,
            inventory: self.binding.inventory,
            policy: self.binding.policy,
            catalog: self.location_catalog.digest(),
        }
    }

    /// Complete byte-derived catalog from the exact stores used by this reader.
    pub(crate) fn location_catalog(&self) -> &SourceLocationCatalog {
        &self.location_catalog
    }

    /// GridMap.cpp:1415–1421 follows GetAreaFlag and final indexed DBC resolution.
    /// It uses the selected row's zone directly, without checking a parent row.
    pub(crate) fn query_location(
        &self,
        request: TerrainAreaRequest,
        deadline: Instant,
    ) -> Result<TerrainLocationReply, Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(Error::Identity);
        }
        let mut budget = QueryBudget::new(deadline);
        budget.take()?;
        let resolved = self.resolve(request.position.map(f32::from_bits), false, &mut budget)?;
        let area_id = resolved.area.map_or(0, |area| area.id);
        let zone_id = resolved
            .area
            .map_or(0, |area| if area.zone != 0 { area.zone } else { area.id });
        budget.complete()?;
        Ok(TerrainLocationReply {
            request,
            result: TerrainLocationResult {
                area_flag: resolved.area_flag,
                area: resolved.area,
                zone_id,
                area_id,
            },
        })
    }

    pub(crate) fn query(
        &self,
        request: TerrainAreaRequest,
        deadline: Instant,
    ) -> Result<TerrainAreaReply, Error> {
        self.query_with_budget(request, &mut QueryBudget::new(deadline))
    }

    fn query_with_budget(
        &self,
        request: TerrainAreaRequest,
        budget: &mut QueryBudget,
    ) -> Result<TerrainAreaReply, Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(Error::Identity);
        }
        budget.take()?;
        let result = self.resolve(request.position.map(f32::from_bits), true, budget)?;
        Ok(TerrainAreaReply { request, result })
    }

    /// Wet liquid needs only the resolved area; parent lookup is conditional on
    /// override==0 and must not be moved before that source branch.
    pub(super) fn liquid_area(
        &self,
        position: [f32; 3],
        budget: &mut QueryBudget,
    ) -> Result<Option<ResolvedArea>, Error> {
        budget.take()?;
        Ok(self.resolve(position, false, budget)?.area)
    }

    pub(super) fn area_by_id(&self, id: u32) -> Option<ResolvedArea> {
        self.areas.entry(id).map(Into::into)
    }

    pub(super) fn areas(&self) -> &AreaTable {
        &self.areas
    }

    fn resolve(
        &self,
        position: [f32; 3],
        include_parent: bool,
        budget: &mut QueryBudget,
    ) -> Result<TerrainAreaResult, Error> {
        let raw_area = self.raw.area(position, budget)?;
        budget.take()?;
        // A missing inventoried tile is not source GetGrid's null-grid state. Never
        // invent GetAreaFlagByMapId fallback, even if a WMO could supply an area.
        let tile = self
            .products
            .inventory
            .terrain(self.binding.map, world_tile(position[0], position[1])?)?;
        let area_info = if let AreaResult::Area { floor, .. } = raw_area {
            budget.take()?;
            let height = match tile.point_height(position[0], position[1])? {
                PointHeight::Height(height) => height,
                PointHeight::Hole => INVALID_HEIGHT_VALUE,
            };
            // GetAreaInfo uses raw GridMap height, never GetHeightStatic's selection.
            if position[2] + 2.0 > height && height > f32::from_bits(floor) {
                AreaResult::None
            } else {
                raw_area
            }
        } else {
            AreaResult::None
        };
        let mut selected = None;
        if let AreaResult::Area {
            root_id,
            adt_id,
            group_id,
            ..
        } = area_info
        {
            // DBCStores builds triple vectors by ascending indexed WMO row ID.
            // GetAreaFlag retains the LAST map-matching area. There is no group=-1
            // fallback here (that belongs to GetAreaName), and area ID zero is real.
            for row in self.wmo_areas.ordered_entries() {
                budget.take()?;
                if row.triple == [root_id as i32, i32::from(adt_id), group_id as i32] {
                    budget.take()?;
                    if let Some(area) = self.areas.entry(row.area_id) {
                        if area.map_id == self.binding.map {
                            selected = Some(area);
                        }
                    }
                }
            }
        }
        budget.take()?;
        let area_flag = match selected {
            Some(area) => area.explore_flag as u16,
            None => tile.area_flag(position[0], position[1])?,
        };
        let area = self.resolve_flag(area_flag, budget)?;
        let parent_area = if let Some(area) = area.filter(|area| include_parent && area.zone != 0) {
            budget.take()?;
            self.areas.entry(area.zone)
        } else {
            None
        };
        budget.complete()?;
        Ok(TerrainAreaResult {
            raw_area,
            area_info,
            area_flag,
            area: area.map(Into::into),
            parent_area: parent_area.map(Into::into),
        })
    }

    fn resolve_flag(
        &self,
        flag: u16,
        budget: &mut QueryBudget,
    ) -> Result<Option<AreaEntry>, Error> {
        let mut last_wrong_map = None;
        // Source LookupEntry(i) scans indexed IDs, not physical record order. Sparse
        // absent IDs cannot match; bounded iteration charges every actual row visited.
        for area in self.areas.ordered_entries() {
            budget.take()?;
            if flag != 0 && area.explore_flag == u32::from(flag) {
                if area.map_id == self.binding.map {
                    return Ok(Some(area));
                }
                last_wrong_map = Some(area);
            }
        }
        if last_wrong_map.is_some() {
            return Ok(last_wrong_map);
        }
        budget.take()?;
        let Some(linked_zone) = self.maps.linked_zone(self.binding.map) else {
            return Ok(None);
        };
        budget.take()?;
        // linked_zone==0 can resolve an actual AreaTable ID zero; do not filter it.
        Ok(self.areas.entry(linked_zone))
    }
}

#[cfg(test)]
pub(super) mod tests;

#[cfg(test)]
mod oracle_tests;
