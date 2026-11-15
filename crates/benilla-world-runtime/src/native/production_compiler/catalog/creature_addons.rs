// SPDX-License-Identifier: GPL-3.0-or-later
//! SQLStorages.cpp:23,27,46,48 fixes both seven-column iibbiis layouts.
use super::*;
use crate::native::source_execution::{CreatureAddonRow, CreatureAddonTable, SqlTemplateRow};
const TEMPLATE_COLUMNS: [&str; 7] = [
    "entry",
    "mount",
    "stand_state",
    "sheath_state",
    "emote",
    "moveflags",
    "auras",
];
const GUID_COLUMNS: [&str; 7] = [
    "guid",
    "mount",
    "stand_state",
    "sheath_state",
    "emote",
    "moveflags",
    "auras",
];
fn projection(table: &str, columns: &[String]) -> Result<()> {
    let expected = match table {
        "creature_template_addon" => TEMPLATE_COLUMNS,
        "creature_addon" => GUID_COLUMNS,
        _ => return Err(fail("creature_addons.fixed_projection")),
    };
    check(
        columns.iter().map(String::as_str).eq(expected),
        "creature_addons.fixed_projection",
    )
}
fn convert(ordinal: u64, cells: &[Option<&[u8]>; 256]) -> Result<CreatureAddonRow> {
    let mut row = SqlTemplateRow {
        ordinal,
        words: [0; 7],
        strings: [Vec::new()],
    };
    for (field, kind) in b"iibbiis".iter().enumerate() {
        row.words[field] = match kind {
            b'i' => spawns::uint(cells[field])?,
            b'b' => u32::from(spawns::uint(cells[field])? as u8),
            b's' => {
                row.strings[0] = cells[field]
                    .unwrap_or_default()
                    .split(|b| *b == 0)
                    .next()
                    .unwrap_or_default()
                    .to_vec();
                0
            }
            _ => unreachable!(),
        };
    }
    Ok(row)
}
impl VerifiedCatalog {
    pub(crate) fn execution_creature_addon_rows(
        &mut self,
        table: CreatureAddonTable,
    ) -> Result<Vec<CreatureAddonRow>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        projection(table.name(), &spawns::columns(&schema, table.name())?)?;
        let mut rows = Vec::new();
        self.scan(table.name(), 0, 256, |raw| {
            let mut cells = [None; 256];
            spawns::fill(&raw, &mut cells)?;
            rows.push(convert(raw.ordinal, &cells)?);
            check(
                rows.len() <= DECODED / std::mem::size_of::<CreatureAddonRow>(),
                "creature_addons.view_limit",
            )
        })?;
        // Reference environment: restored MyISAM primary-key order, mangos.sql:758,1352.
        rows.sort_unstable_by_key(|row| row.words[0]);
        check(
            rows.windows(2)
                .all(|pair| pair[0].words[0] < pair[1].words[0]),
            "creature_addons.primary_key",
        )?;
        Ok(rows)
    }
    #[cfg(test)]
    pub(crate) fn execution_check_creature_addon_trace(
        &mut self,
        table: CreatureAddonTable,
        trace: &[Vec<Vec<u8>>],
    ) -> Result<()> {
        let rows = self.execution_creature_addon_rows(table)?;
        check(rows.len() == trace.len(), "creature_addons.trace_row_count")?;
        for (expected, fields) in rows.iter().zip(trace) {
            check(fields.len() == 7, "creature_addons.trace_columns")?;
            let mut cells = [None; 256];
            for (cell, bytes) in cells.iter_mut().zip(fields) {
                *cell = Some(bytes.as_slice());
            }
            check(
                convert(expected.ordinal, &cells)? == *expected,
                "creature_addons.trace_order_or_fields",
            )?;
        }
        Ok(())
    }
}
#[test]
fn fixed_addon_reader_refuses_every_other_relation_and_projection() {
    for (table, columns) in [
        ("creature_template_addon", TEMPLATE_COLUMNS),
        ("creature_addon", GUID_COLUMNS),
    ] {
        let columns = columns.map(str::to_owned);
        projection(table, &columns).unwrap();
        for bad in ["creature", "creature_template", "gameobject_addon", ""] {
            assert_eq!(
                projection(bad, &columns).unwrap_err().id,
                "creature_addons.fixed_projection"
            );
        }
        let mut bad = columns.to_vec();
        bad.swap(2, 3);
        assert_eq!(
            projection(table, &bad).unwrap_err().id,
            "creature_addons.fixed_projection"
        );
        assert!(projection(table, &columns[..6]).is_err());
        let mut bad = columns.to_vec();
        bad.push("extra".into());
        assert!(projection(table, &bad).is_err());
    }
}
#[test]
fn addon_sql_storage_preserves_null_empty_c_strings_and_byte_wraps() {
    let mut cells: [Option<&[u8]>; 256] = [None; 256];
    cells[0] = Some(b"0x12");
    cells[2] = Some(b"257");
    cells[3] = Some(b"-1");
    cells[4] = Some(b"");
    cells[5] = Some(b"4294967295");
    cells[6] = Some(b"123\0ignored");
    let row = convert(1, &cells).unwrap();
    assert_eq!(row.words, [0, 0, 1, 255, 0, u32::MAX, 0]);
    assert_eq!(row.strings, [b"123".to_vec()]);
    cells[6] = None;
    let null = convert(1, &cells).unwrap();
    cells[6] = Some(b"");
    assert_eq!(null, convert(1, &cells).unwrap());
}
