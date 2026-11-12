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
impl VerifiedCatalog {
    /// Last physical duplicate wins, as DBCFileLoader::AutoProduceData does.
    pub(crate) fn execution_terrain_dbc(&mut self, name: &str) -> Result<Vec<Vec<u32>>> {
        let fields = fields(name)?;
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
}
