// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed ObjectMgr.cpp:984,1010,1032,9828,9757 catalog projections.
use super::*;
use crate::native::source_execution::{
    CreatureSpellEntryRow, CreatureSpellRow, CreatureSpellTargetRow, SqlTemplateRow,
};
const TARGET: [&str; 7] = [
    "Id",
    "Type",
    "Param1",
    "Param2",
    "Param3",
    "UnitCondition",
    "Comments",
];
const ENTRY: [&str; 4] = ["Id", "Name", "ChanceSupportAction", "ChanceRangedAttack"];
const SPELL: [&str; 14] = [
    "Id",
    "Position",
    "SpellId",
    "Flags",
    "CombatCondition",
    "TargetId",
    "ScriptId",
    "Availability",
    "Probability",
    "InitialMin",
    "InitialMax",
    "RepeatMin",
    "RepeatMax",
    "Comments",
];
const COOLDOWN: [&str; 4] = ["Entry", "SpellId", "CooldownMin", "CooldownMax"];
const LEGACY: [&str; 12] = [
    "entry", "setId", "spell1", "spell2", "spell3", "spell4", "spell5", "spell6", "spell7",
    "spell8", "spell9", "spell10",
];
fn projection(table: &str, columns: &[String]) -> Result<()> {
    let expected: &[&str] = match table {
        "creature_spell_targeting" => &TARGET,
        "creature_spell_list_entry" => &ENTRY,
        "creature_spell_list" => &SPELL,
        "creature_cooldowns" => &COOLDOWN,
        "creature_template_spells" => &LEGACY,
        _ => return Err(fail("creature_spell_lists.fixed_relation")),
    };
    check(
        columns
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied()),
        "creature_spell_lists.fixed_columns",
    )
}
fn convert<const N: usize, const S: usize>(
    ordinal: u64,
    cells: &[Option<&[u8]>; 256],
    words: [usize; N],
    strings: [usize; S],
) -> Result<SqlTemplateRow<N, S>> {
    let mut row = SqlTemplateRow {
        ordinal,
        words: [0; N],
        strings: std::array::from_fn(|_| Vec::new()),
    };
    for (dest, column) in row.words.iter_mut().zip(words) {
        *dest = spawns::uint(cells[column])?;
    }
    for (dest, column) in row.strings.iter_mut().zip(strings) {
        let bytes = cells[column].unwrap_or_default();
        // Field::GetCppString uses the C string, Field.h:49-52.
        *dest = bytes.split(|&b| b == 0).next().unwrap_or_default().to_vec();
    }
    Ok(row)
}
fn read_rows<const N: usize, const S: usize>(
    catalog: &mut VerifiedCatalog,
    schema: &Value,
    table: &str,
    words: [usize; N],
    strings: [usize; S],
) -> Result<Vec<SqlTemplateRow<N, S>>> {
    projection(table, &spawns::columns(schema, table)?)?;
    let mut rows = Vec::new();
    catalog.scan(table, 0, 256, |raw| {
        let mut cells = [None; 256];
        spawns::fill(&raw, &mut cells)?;
        rows.push(convert(raw.ordinal, &cells, words, strings)?);
        check(
            rows.len() <= DECODED / std::mem::size_of::<SqlTemplateRow<N, S>>(),
            "creature_spell_lists.view_limit",
        )
    })?;
    Ok(rows)
}
pub(crate) struct CreatureSpellCatalogRows {
    pub targets: Vec<CreatureSpellTargetRow>,
    pub entries: Vec<CreatureSpellEntryRow>,
    pub spells: Vec<CreatureSpellRow>,
    pub cooldowns: Vec<[u32; 4]>,
    pub legacy: Vec<[u32; 12]>,
    pub attributes_ex: BTreeMap<u32, u32>,
}
impl VerifiedCatalog {
    pub(crate) fn execution_creature_spell_lists(&mut self) -> Result<CreatureSpellCatalogRows> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let mut targets = read_rows(
            self,
            &schema,
            "creature_spell_targeting",
            [0, 1, 2, 3, 4, 5],
            [6],
        )?;
        let mut entries = read_rows(self, &schema, "creature_spell_list_entry", [0, 2, 3], [1])?;
        let mut spells = read_rows(
            self,
            &schema,
            "creature_spell_list",
            std::array::from_fn(|i| i),
            [],
        )?;
        let mut cooldowns: Vec<SqlTemplateRow<4, 0>> =
            read_rows(self, &schema, "creature_cooldowns", [0, 1, 2, 3], [])?;
        let mut legacy: Vec<SqlTemplateRow<12, 0>> = read_rows(
            self,
            &schema,
            "creature_template_spells",
            std::array::from_fn(|i| i),
            [],
        )?;
        // PRIMARY KEY(Id), signed int; --order-by-primary snapshot restore rule.
        targets.sort_unstable_by_key(|r| r.words[0] as i32);
        // PRIMARY KEY(Id), signed int; --order-by-primary snapshot restore rule.
        entries.sort_unstable_by_key(|r| r.words[0] as i32);
        // PRIMARY KEY(Id,Position), signed ints; --order-by-primary snapshot restore rule.
        spells.sort_unstable_by_key(|r| (r.words[0] as i32, r.words[1] as i32));
        // PRIMARY KEY(Entry,SpellId), unsigned; --order-by-primary snapshot restore rule.
        cooldowns.sort_unstable_by_key(|r| (r.words[0], r.words[1]));
        // PRIMARY KEY(entry,setId), unsigned; --order-by-primary snapshot restore rule.
        legacy.sort_unstable_by_key(|r| (r.words[0], r.words[1]));
        let mut attributes_ex = BTreeMap::new();
        self.execution_spell_scan("spell_template", |_, values| {
            let id = number(&values[0])? as u32;
            let attributes = number(&values[7])? as u32;
            attributes_ex.insert(id, attributes);
            Ok(())
        })?;
        Ok(CreatureSpellCatalogRows {
            targets,
            entries,
            spells,
            cooldowns: cooldowns.into_iter().map(|r| r.words).collect(),
            legacy: legacy.into_iter().map(|r| r.words).collect(),
            attributes_ex,
        })
    }
}
#[cfg(test)]
impl CreatureSpellCatalogRows {
    pub(crate) fn check_trace(&self, trace: &[Vec<Vec<Vec<u8>>>; 5]) {
        fn compare<const N: usize, const S: usize>(
            rows: &[SqlTemplateRow<N, S>],
            trace: &[Vec<Vec<u8>>],
            words: [usize; N],
            strings: [usize; S],
        ) {
            assert_eq!(rows.len(), trace.len());
            for (row, fields) in rows.iter().zip(trace) {
                assert_eq!(fields.len(), N + S);
                let mut cells = [None; 256];
                for (cell, bytes) in cells.iter_mut().zip(fields) {
                    *cell = Some(bytes.as_slice());
                }
                assert_eq!(*row, convert(row.ordinal, &cells, words, strings).unwrap());
            }
        }
        compare(&self.targets, &trace[0], [0, 1, 2, 3, 4, 5], [6]);
        compare(&self.entries, &trace[1], [0, 2, 3], [1]);
        compare(&self.spells, &trace[2], std::array::from_fn(|i| i), []);
        for (rows, fields, width) in [
            (
                &self
                    .cooldowns
                    .iter()
                    .map(|r| r.as_slice())
                    .collect::<Vec<_>>(),
                &trace[3],
                4,
            ),
            (
                &self.legacy.iter().map(|r| r.as_slice()).collect::<Vec<_>>(),
                &trace[4],
                12,
            ),
        ] {
            assert_eq!(rows.len(), fields.len());
            for (row, fields) in rows.iter().zip(fields) {
                assert_eq!(fields.len(), width);
                assert!(row
                    .iter()
                    .copied()
                    .eq(fields.iter().map(|v| spawns::uint(Some(v)).unwrap())));
            }
        }
    }
}
#[test]
fn spell_list_reader_has_its_own_fixed_tables_and_columns() {
    for (table, columns) in [
        ("creature_spell_targeting", TARGET.as_slice()),
        ("creature_spell_list_entry", ENTRY.as_slice()),
        ("creature_spell_list", SPELL.as_slice()),
        ("creature_cooldowns", COOLDOWN.as_slice()),
        ("creature_template_spells", LEGACY.as_slice()),
    ] {
        let mut columns = columns.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        projection(table, &columns).unwrap();
        columns.swap(0, 1);
        assert_eq!(
            projection(table, &columns).unwrap_err().id,
            "creature_spell_lists.fixed_columns"
        );
        columns.pop();
        assert_eq!(
            projection(table, &columns).unwrap_err().id,
            "creature_spell_lists.fixed_columns"
        );
    }
    assert_eq!(
        projection("creature_template", &[]).unwrap_err().id,
        "creature_spell_lists.fixed_relation"
    );
}
#[test]
fn spell_list_fields_preserve_signed_bits_and_cpp_string_truncation() {
    let mut cells: [Option<&[u8]>; 256] = [None; 256];
    cells[0] = Some(b"-1");
    cells[1] = Some(b"text\0ignored");
    let row = convert(1, &cells, [0], [1]).unwrap();
    assert_eq!(row.words, [u32::MAX]);
    assert_eq!(row.strings, [b"text".to_vec()]);
}
