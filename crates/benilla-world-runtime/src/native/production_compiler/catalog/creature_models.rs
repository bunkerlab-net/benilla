// SPDX-License-Identifier: GPL-3.0-or-later
//! SQLStorages.cpp:26,47 fixes the eight-column creature_model_info layout.
use super::*;
use crate::native::source_execution::{LoadedCreatureModelInfo, SqlTemplateRow};
const COLUMNS: [&str; 8] = [
    "modelid",
    "bounding_radius",
    "combat_reach",
    "SpeedWalk",
    "SpeedRun",
    "gender",
    "modelid_other_gender",
    "modelid_alternative",
];
fn projection(table: &str, columns: &[String]) -> Result<()> {
    check(
        table == "creature_model_info" && columns.iter().map(String::as_str).eq(COLUMNS),
        "creature_models.fixed_projection",
    )
}
fn convert(ordinal: u64, cells: &[Option<&[u8]>; 256]) -> Result<LoadedCreatureModelInfo> {
    let mut row = SqlTemplateRow {
        ordinal,
        words: [0; 8],
        strings: [],
    };
    for (index, word) in row.words.iter_mut().enumerate() {
        *word = match index {
            1..=4 => spawns::floating(cells[index])?.to_bits(),
            5 => u32::from(spawns::uint(cells[index])? as u8),
            _ => spawns::uint(cells[index])?,
        };
    }
    Ok(row)
}
impl VerifiedCatalog {
    pub(crate) fn execution_creature_model_rows(&mut self) -> Result<Vec<LoadedCreatureModelInfo>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        projection(
            "creature_model_info",
            &spawns::columns(&schema, "creature_model_info")?,
        )?;
        let mut rows = Vec::new();
        self.scan("creature_model_info", 0, 256, |raw| {
            let mut cells = [None; 256];
            spawns::fill(&raw, &mut cells)?;
            rows.push(convert(raw.ordinal, &cells)?);
            check(
                rows.len() <= DECODED / std::mem::size_of::<LoadedCreatureModelInfo>(),
                "creature_models.view_limit",
            )
        })?;
        // Restored MyISAM primary-key scan, seal16_template_order.rs:86-118.
        rows.sort_unstable_by_key(|row| row.words[0]);
        check(
            rows.windows(2)
                .all(|pair| pair[0].words[0] < pair[1].words[0]),
            "creature_models.primary_key",
        )?;
        Ok(rows)
    }
    #[cfg(test)]
    pub(crate) fn execution_check_creature_model_trace(
        &self,
        rows: &[LoadedCreatureModelInfo],
        trace: &[Vec<Vec<u8>>],
    ) -> Result<()> {
        check(rows.len() == trace.len(), "creature_models.trace_row_count")?;
        for (expected, fields) in rows.iter().zip(trace) {
            check(fields.len() == 8, "creature_models.trace_columns")?;
            let mut cells = [None; 256];
            for (cell, bytes) in cells.iter_mut().zip(fields) {
                *cell = Some(bytes.as_slice());
            }
            check(
                convert(expected.ordinal, &cells)? == *expected,
                "creature_models.trace_order_or_fields",
            )?;
        }
        Ok(())
    }
}
#[test]
fn fixed_model_reader_refuses_other_relations_and_columns() {
    let columns = COLUMNS.map(str::to_owned);
    projection("creature_model_info", &columns).unwrap();
    assert_eq!(
        projection("creature_template", &columns).unwrap_err().id,
        "creature_models.fixed_projection"
    );
    let mut bad = columns.clone();
    bad.swap(1, 2);
    assert_eq!(
        projection("creature_model_info", &bad).unwrap_err().id,
        "creature_models.fixed_projection"
    );
    assert_eq!(
        projection("creature_model_info", &columns[..7])
            .unwrap_err()
            .id,
        "creature_models.fixed_projection"
    );
}
#[test]
fn model_storage_byte_gender_wraps_before_validation_and_float_bits_survive() {
    let mut cells = [None; 256];
    let values: [&[u8]; 8] = [b"1", b"-0", b"1.5", b"2", b"3", b"257", b"0", b"0"];
    for (cell, value) in cells.iter_mut().zip(values) {
        *cell = Some(value);
    }
    assert_eq!(
        convert(3, &cells).unwrap().words,
        [
            1,
            0x80000000,
            1.5f32.to_bits(),
            2f32.to_bits(),
            3f32.to_bits(),
            1,
            0,
            0
        ]
    );
}
