// SPDX-License-Identifier: GPL-3.0-or-later
//! Live TerrainInfo water calls, preserving the full GetGrid/MMap refusal.
use super::*;
use benilla_native_terrain::grid_liquid::{
    self as liquid, Area, GridOwner, LiquidData, Lookup, Status, TerrainOwner,
};

#[derive(Default)]
pub struct SourceLiquidDbc {
    types: Option<BTreeMap<u32, u32>>,
    areas: Option<BTreeMap<u32, Area>>,
}
impl SourceLiquidDbc {
    /// Complete catalog stores, never an authored subset inferred from reached IDs.
    pub fn catalog(
        catalog: &mut super::super::super::production_compiler::VerifiedCatalog,
    ) -> Result<Self> {
        let types = catalog
            .execution_liquid_dbc("LiquidType.dbc")?
            .into_iter()
            .map(|r| (r[0], r[1]))
            .collect();
        let areas = catalog
            .execution_liquid_dbc("AreaTable.dbc")?
            .into_iter()
            .map(|r| {
                (
                    r[0],
                    Area {
                        zone: r[1],
                        liquid_override: r[2],
                    },
                )
            })
            .collect();
        Ok(Self {
            types: Some(types),
            areas: Some(areas),
        })
    }
}
impl From<liquid::Error> for Refusal {
    fn from(error: liquid::Error) -> Self {
        refuse(match error {
            liquid::Error::HeaderReadUndefined => "execution.terrain.liquid_header_read_undefined",
            liquid::Error::Magic => "execution.terrain.liquid_magic",
            liquid::Error::InvalidHeaderFlags(_) => "execution.terrain.liquid_invalid_header_flags",
            liquid::Error::CoordinateConversionUndefined => {
                "execution.terrain.liquid_coordinate_conversion_undefined"
            }
            liquid::Error::EntryReadUndefined => "execution.terrain.liquid_entry_read_undefined",
            liquid::Error::FlagsReadUndefined => "execution.terrain.liquid_flags_read_undefined",
            liquid::Error::HeightReadUndefined => "execution.terrain.liquid_height_read_undefined",
            liquid::Error::TypeShiftUndefined => "execution.terrain.liquid_type_shift_undefined",
            liquid::Error::LiquidTypeStoreUnavailable => "execution.terrain.LiquidType_dbc_owner",
            liquid::Error::AreaStoreUnavailable => "execution.terrain.AreaTable_dbc_owner",
        })
    }
}
impl Lookup for SourceLiquidDbc {
    type Error = Refusal;
    fn liquid_type(&self, id: u32) -> Result<Option<u32>> {
        Ok(self
            .types
            .as_ref()
            .ok_or(liquid::Error::LiquidTypeStoreUnavailable)?
            .get(&id)
            .copied())
    }
    fn area(&self, id: u32) -> Result<Option<Area>> {
        Ok(self
            .areas
            .as_ref()
            .ok_or(liquid::Error::AreaStoreUnavailable)?
            .get(&id)
            .copied())
    }
}
struct GridQuery<'a> {
    grid: &'a SourceGridMap,
    stores: &'a SourceLiquidDbc,
}
impl Lookup for GridQuery<'_> {
    type Error = Refusal;
    fn liquid_type(&self, id: u32) -> Result<Option<u32>> {
        self.stores.liquid_type(id)
    }
    fn area(&self, id: u32) -> Result<Option<Area>> {
        self.stores.area(id)
    }
}
impl GridOwner for GridQuery<'_> {
    fn area_key(&self, x: f32, y: f32) -> Result<u16> {
        self.grid.area(x, y)
    }
    fn height(&self, x: f32, y: f32) -> Result<f32> {
        self.grid.height(x, y)
    }
}
impl SourceGridMap {
    pub fn liquid_status(
        &self,
        pose: [f32; 3],
        mask: u8,
        collision: f32,
        data: Option<&mut LiquidData>,
        stores: &SourceLiquidDbc,
    ) -> Result<Status> {
        self.liquid.status(
            pose,
            mask,
            collision,
            data,
            &GridQuery { grid: self, stores },
        )
    }
}
struct Environment<'a> {
    manager: &'a mut SourceTerrainManager,
    map: u32,
}
impl Lookup for Environment<'_> {
    type Error = Refusal;
    fn liquid_type(&self, id: u32) -> Result<Option<u32>> {
        self.manager.liquid_dbc.liquid_type(id)
    }
    fn area(&self, id: u32) -> Result<Option<Area>> {
        self.manager.liquid_dbc.area(id)
    }
}
impl TerrainOwner for Environment<'_> {
    fn can_check(&mut self, p: [f32; 3]) -> Result<bool> {
        // GridMap.cpp:797-802. Disabled height reaches full GetGrid here.
        if !self.manager.vmap.settings()[1] {
            self.manager.query_grid(self.map, p, false)?;
        }
        Ok(true)
    }
    fn height_static(&mut self, p: [f32; 3]) -> Result<f32> {
        self.manager
            .height_static(self.map, p, true, liquid::DEFAULT_WATER_SEARCH)
    }
    fn vmap_liquid(&mut self, p: [f32; 3], mask: u8, out: &mut LiquidData) -> Result<bool> {
        let mut raw = benilla_native_vmap::live::LiquidOutput {
            level: out.level,
            floor: out.depth_level,
            kind: out.entry,
        };
        let result = self
            .manager
            .vmap
            .collision
            .liquid_level(
                self.map,
                p,
                mask,
                &mut raw,
                std::time::Instant::now() + std::time::Duration::from_secs(30),
            )
            .map_err(vmap_manager::vmap_error);
        out.level = raw.level;
        out.depth_level = raw.floor;
        out.entry = raw.kind;
        result
    }
    fn liquid_area(&mut self, p: [f32; 3]) -> Result<Option<Area>> {
        // GridMap.cpp:1076 resolves GetAreaFlag through the actual live WMO path.
        let flag = self.manager.area_flag(self.map, p)?;
        self.manager
            .liquid_dbc
            .areas
            .as_ref()
            .ok_or(liquid::Error::AreaStoreUnavailable)?;
        self.manager
            .dbc
            .entry(self.map, flag)
            .map(|a| self.area(a.id))
            .transpose()
            .map(Option::flatten)
    }
    fn grid_liquid(
        &mut self,
        p: [f32; 3],
        mask: u8,
        collision: f32,
        out: &mut LiquidData,
    ) -> Result<Status> {
        let tile = self.manager.query_grid(self.map, p, false)?;
        self.manager
            .terrain(self.map)
            .unwrap()
            .grid(tile.x, tile.y)
            .unwrap()
            .liquid_status(p, mask, collision, Some(out), &self.manager.liquid_dbc)
    }
}
impl SourceTerrainManager {
    pub fn liquid_status(
        &mut self,
        map: u32,
        pose: [f32; 3],
        mask: u8,
        collision: f32,
        data: Option<&mut LiquidData>,
    ) -> Result<Status> {
        liquid::terrain_status(
            &mut Environment { manager: self, map },
            pose,
            mask,
            collision,
            data,
        )
    }
    pub fn is_in_water(
        &mut self,
        map: u32,
        pose: [f32; 3],
        data: Option<&mut LiquidData>,
    ) -> Result<bool> {
        liquid::is_in_water(&mut Environment { manager: self, map }, pose, data)
    }
    pub fn is_swimmable(
        &mut self,
        map: u32,
        pose: [f32; 3],
        radius: f32,
        data: Option<&mut LiquidData>,
    ) -> Result<bool> {
        liquid::is_swimmable(&mut Environment { manager: self, map }, pose, radius, data)
    }
    pub fn is_under_water(
        &mut self,
        map: u32,
        pose: [f32; 3],
        water_z: Option<&mut f32>,
    ) -> Result<bool> {
        liquid::is_under_water(&mut Environment { manager: self, map }, pose, water_z)
    }
    pub fn water_or_ground_level(
        &mut self,
        map: u32,
        pose: [f32; 3],
        ground: f32,
        swim: bool,
        minimum: f32,
    ) -> Result<f32> {
        liquid::water_or_ground(
            &mut Environment { manager: self, map },
            pose,
            ground,
            swim,
            minimum,
        )
    }
    pub fn water_level(
        &mut self,
        map: u32,
        pose: [f32; 3],
        ground: Option<&mut f32>,
    ) -> Result<f32> {
        liquid::water_level(&mut Environment { manager: self, map }, pose, ground)
    }
}

#[cfg(test)]
#[path = "environment_tests.rs"]
mod tests;
