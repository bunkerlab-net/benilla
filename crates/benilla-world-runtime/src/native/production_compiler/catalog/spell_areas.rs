// SPDX-License-Identifier: GPL-3.0-or-later
//! SpellMgr.cpp:2085-2116. Dedicated spell_area query, no wider reader policy.
use super::*;

const RELATIONS: &[&str] = &["spell_area"];
const FIELDS: [&str; 10] = [
    "spell",
    "area",
    "quest_start",
    "quest_start_active",
    "quest_end",
    "condition_id",
    "aura_spell",
    "racemask",
    "gender",
    "autocast",
];
fn relation(table: &str) -> Result<()> {
    check(RELATIONS.contains(&table), "spells.area_relation")
}
fn field(index: usize, raw: Option<&[u8]>) -> Result<u32> {
    let value = spawns::uint(raw)?;
    Ok(match index {
        3 | 9 => u32::from((value as i32) > 0),
        5 => value & 65535,
        8 => value & 255,
        _ => value,
    })
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct SpellAreaCatalogRow {
    pub ordinal: u64,
    pub words: [u32; 10],
}
impl VerifiedCatalog {
    /// Catalog cursor order is retained, not claimed as a recorded SQL cursor.
    pub(crate) fn execution_spell_area_rows(
        &mut self,
        table: &str,
    ) -> Result<Vec<SpellAreaCatalogRow>> {
        relation(table)?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = spawns::columns(&schema, table)?;
        check(cols.len() == FIELDS.len(), "spells.area_schema")?;
        let mut indices = [0; 10];
        for (index, name) in indices.iter_mut().zip(FIELDS) {
            *index = spawns::position(&cols, name)?;
        }
        let mut output = Vec::new();
        self.scan(table, 0, 256, |row| {
            let mut values = [None; 10];
            for (index, cell) in row.cells().enumerate() {
                *values
                    .get_mut(index)
                    .ok_or_else(|| fail("spells.area_schema"))? = cell?;
            }
            let mut words = [0; 10];
            for (index, (word, &i)) in words.iter_mut().zip(&indices).enumerate() {
                *word = field(index, values[i])?;
            }
            output.push(SpellAreaCatalogRow {
                ordinal: row.ordinal,
                words,
            });
            check(
                output.len() <= DECODED / std::mem::size_of::<SpellAreaCatalogRow>(),
                "spells.area_view_limit",
            )
        })?;
        Ok(output)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spell_area_reader_refuses_other_relations() {
        relation("spell_area").unwrap();
        for table in [
            "spell_template",
            "conditions",
            "quest_template",
            "creature_ai_scripts",
            "internal_use",
            "Spell_Area",
            "",
        ] {
            assert_eq!(relation(table).unwrap_err().id, "spells.area_relation");
        }
    }
    #[test]
    fn spell_area_field_widths_and_bool_sign_follow_field_getters() {
        assert_eq!(field(3, Some(b"-1")).unwrap(), 0);
        assert_eq!(field(9, Some(b"2")).unwrap(), 1);
        assert_eq!(field(5, Some(b"65537")).unwrap(), 1);
        assert_eq!(field(8, Some(b"258")).unwrap(), 2);
        assert_eq!(field(6, Some(b"-2147483648")).unwrap(), 2147483648);
        assert_eq!(field(0, None).unwrap(), 0);
    }
}
