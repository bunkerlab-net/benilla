// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed terrain DBC operands. This reader never accepts SQL table names.
use super::*;

fn fields(name: &str) -> Result<&'static [u32]> {
    match name {
        "AreaTable.dbc" => Ok(&[0, 1, 2, 3]),
        "Map.dbc" => Ok(&[0, 19]),
        _ => Err(fail("terrain.dbc_not_allowed")),
    }
}
fn wmo_fields(name: &str) -> Result<&'static [u32]> {
    match name {
        "WMOAreaTable.dbc" => Ok(&[0, 1, 2, 3, 9, 10]),
        _ => Err(fail("terrain.wmo_dbc_not_allowed")),
    }
}
impl VerifiedCatalog {
    /// Last physical duplicate wins, as DBCFileLoader::AutoProduceData does.
    pub(crate) fn execution_terrain_dbc(&mut self, name: &str) -> Result<Vec<Vec<u32>>> {
        let fields = fields(name)?;
        self.execution_geometry_dbc_fields(name, fields)
    }
    /// F2.6 WMOAreaTable fields, DBCStructure.h:1026-1038.
    pub(crate) fn execution_wmo_area_dbc(&mut self, name: &str) -> Result<Vec<Vec<u32>>> {
        let fields = wmo_fields(name)?;
        self.execution_geometry_dbc_fields(name, fields)
    }
    /// Complete liquid and area override operands, DBCStructure.h:40-57,546-552.
    pub(crate) fn execution_liquid_dbc(&mut self, name: &str) -> Result<Vec<Vec<u32>>> {
        let fields: &[u32] = match name {
            "LiquidType.dbc" => &[0, 2],
            "AreaTable.dbc" => &[0, 2, 24],
            _ => return Err(fail("terrain.liquid_dbc_not_allowed")),
        };
        self.execution_geometry_dbc_fields(name, fields)
    }
    fn execution_geometry_dbc_fields(
        &mut self,
        name: &str,
        fields: &[u32],
    ) -> Result<Vec<Vec<u32>>> {
        let mut view = self.dbc_view(name)?;
        let mut entries = BTreeMap::new();
        for ordinal in 0..view.record_count() {
            let record = view.record(ordinal)?;
            let mut row = Vec::with_capacity(fields.len());
            for col in fields {
                let value = record["values"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|v| v[0].as_u64() == Some(u64::from(*col)))
                    .ok_or_else(|| fail("terrain.dbc_field"))?;
                row.push(u32::try_from(number(&value[2])?).map_err(|_| fail("terrain.dbc_field"))?);
            }
            entries.insert(row[0], row);
        }
        Ok(entries.into_values().collect())
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn terrain_reader_refuses_every_unlisted_table() {
        assert!(super::fields("AreaTable.dbc").is_ok());
        assert!(super::fields("Map.dbc").is_ok());
        assert_eq!(
            super::fields("Spell.dbc").unwrap_err().id,
            "terrain.dbc_not_allowed"
        );
        assert_eq!(
            super::fields("creature").unwrap_err().id,
            "terrain.dbc_not_allowed"
        );
    }
    #[test]
    fn wmo_reader_refuses_other_tables_without_expanding_terrain_reader() {
        assert_eq!(
            super::wmo_fields("WMOAreaTable.dbc").unwrap(),
            [0, 1, 2, 3, 9, 10]
        );
        for name in ["AreaTable.dbc", "Map.dbc", "Spell.dbc", "creature"] {
            assert_eq!(
                super::wmo_fields(name).unwrap_err().id,
                "terrain.wmo_dbc_not_allowed"
            );
        }
        assert_eq!(
            super::fields("WMOAreaTable.dbc").unwrap_err().id,
            "terrain.dbc_not_allowed"
        );
    }
}
