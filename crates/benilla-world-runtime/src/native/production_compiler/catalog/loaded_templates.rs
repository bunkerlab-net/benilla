// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed SELECT * layouts from SQLStorages.cpp:21-30, never a general catalog reader.
use super::*;
use crate::native::source_execution::{LoadedCreatureInfo, LoadedGameObjectInfo, SqlTemplateRow};
pub(crate) const CREATURE_COLUMNS: &str = "Entry Name SubName MinLevel MaxLevel DisplayId1 DisplayId2 DisplayId3 DisplayId4 DisplayIdProbability1 DisplayIdProbability2 DisplayIdProbability3 DisplayIdProbability4 Faction Scale Family CreatureType InhabitType RegenerateStats RacialLeader NpcFlags UnitFlags DynamicFlags ExtraFlags CreatureTypeFlags StaticFlags1 StaticFlags2 StaticFlags3 StaticFlags4 SpeedWalk SpeedRun Detection CallForHelp Pursuit Leash Timeout UnitClass Rank HealthMultiplier PowerMultiplier DamageMultiplier DamageVariance ArmorMultiplier ExperienceMultiplier StrengthMultiplier AgilityMultiplier StaminaMultiplier IntellectMultiplier SpiritMultiplier MinLevelHealth MaxLevelHealth MinLevelMana MaxLevelMana MinMeleeDmg MaxMeleeDmg MinRangedDmg MaxRangedDmg Armor MeleeAttackPower RangedAttackPower MeleeBaseAttackTime RangedBaseAttackTime DamageSchool MinLootGold MaxLootGold LootId PickpocketLootId SkinningLootId KillCredit1 KillCredit2 MechanicImmuneMask SchoolImmuneMask ResistanceHoly ResistanceFire ResistanceNature ResistanceFrost ResistanceShadow ResistanceArcane PetSpellDataId MovementType TrainerType TrainerSpell TrainerClass TrainerRace TrainerTemplateId VendorTemplateId GossipMenuId InteractionPauseTimer CorpseDecay SpellList CharmedSpellList StringId1 StringId2 EquipmentTemplateId Civilian AIName ScriptName DamageMultiplierOLD DamageVarianceOLD";
pub(crate) const GO_COLUMNS: &str = "entry type displayId name IconName faction flags ExtraFlags size data0 data1 data2 data3 data4 data5 data6 data7 data8 data9 data10 data11 data12 data13 data14 data15 data16 data17 data18 data19 data20 data21 data22 data23 CustomData1 mingold maxgold StringId ScriptName";
const CREATURE_FORMAT: &str = "issiiiiiiiiiiifiiiiliiiiiiiiiffiiiiiiifffffffffffiiiiffffiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiissff";
const GO_FORMAT: &str = "iiissiiifiiiiiiiiiiiiiiiiiiiiiiiiiiiis";
fn layout(table: &str, columns: &[String]) -> Result<&'static str> {
    let (names, format) = match table {
        "creature_template" => (CREATURE_COLUMNS, CREATURE_FORMAT),
        "gameobject_template" => (GO_COLUMNS, GO_FORMAT),
        _ => return Err(fail("templates.relation")),
    };
    check(
        columns
            .iter()
            .map(String::as_str)
            .eq(names.split_whitespace()),
        "templates.sql_layout",
    )?;
    Ok(format)
}
fn reference_relation(table: &str, column: &str) -> Result<()> {
    check(
        matches!(
            (table, column),
            ("creature_model_info", "modelid") | ("page_text", "entry")
        ),
        "templates.reference_relation",
    )
}
fn convert<const N: usize, const S: usize>(
    ordinal: u64,
    cells: &[Option<&[u8]>; 256],
    format: &str,
    script: usize,
    ids: &BTreeMap<Vec<u8>, u32>,
) -> Result<SqlTemplateRow<N, S>> {
    check(format.len() == N, "templates.sql_layout")?;
    let mut row = SqlTemplateRow {
        ordinal,
        words: [0; N],
        strings: std::array::from_fn(|_| Vec::new()),
    };
    let mut text = 0;
    for (field, kind) in format.bytes().enumerate() {
        row.words[field] = match kind {
            b'i' => spawns::uint(cells[field])?,
            b'l' => u32::from(spawns::uint(cells[field])? > 0),
            b'f' => spawns::floating(cells[field])?.to_bits(),
            b's' => {
                let value = cells[field]
                    .unwrap_or_default()
                    .split(|b| *b == 0)
                    .next()
                    .unwrap_or_default();
                if field == script {
                    // GetScriptId returns zero for an unknown name, ScriptDevAIMgr.cpp:532-544.
                    ids.get(value).copied().unwrap_or(0)
                } else {
                    check(text < S, "templates.sql_layout")?;
                    row.strings[text] = value.to_vec();
                    text += 1;
                    0
                }
            }
            _ => return Err(fail("templates.sql_format")),
        };
    }
    check(text == S, "templates.sql_layout")?;
    Ok(row)
}
impl VerifiedCatalog {
    fn template_rows<const N: usize, const S: usize>(
        &mut self,
        table: &str,
        script: usize,
    ) -> Result<Vec<SqlTemplateRow<N, S>>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let format = layout(table, &spawns::columns(&schema, table)?)?;
        let ids = self.execution_script_name_ids(&schema)?;
        let mut rows = Vec::new();
        self.scan(table, 0, 256, |raw| {
            let mut cells = [None; 256];
            spawns::fill(&raw, &mut cells)?;
            rows.push(convert(raw.ordinal, &cells, format, script, &ids)?);
            check(
                rows.len() <= DECODED / std::mem::size_of::<SqlTemplateRow<N, S>>(),
                "templates.view_limit",
            )
        })?;
        Ok(rows)
    }
    pub(crate) fn execution_loaded_creature_rows(&mut self) -> Result<Vec<LoadedCreatureInfo>> {
        self.template_rows("creature_template", 96)
    }
    pub(crate) fn execution_loaded_gameobject_rows(&mut self) -> Result<Vec<LoadedGameObjectInfo>> {
        self.template_rows("gameobject_template", 37)
    }
    #[cfg(test)]
    fn check_template_trace<const N: usize, const S: usize>(
        &mut self,
        table: &str,
        script: usize,
        rows: &[SqlTemplateRow<N, S>],
        trace: &[Vec<Vec<u8>>],
    ) -> Result<()> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let format = layout(table, &spawns::columns(&schema, table)?)?;
        let ids = self.execution_script_name_ids(&schema)?;
        check(rows.len() == trace.len(), "templates.trace_row_count")?;
        for (expected, fields) in rows.iter().zip(trace) {
            check(fields.len() == N, "templates.trace_columns")?;
            let mut cells = [None; 256];
            for (cell, bytes) in cells.iter_mut().zip(fields) {
                *cell = Some(bytes.as_slice());
            }
            let actual = convert::<N, S>(expected.ordinal, &cells, format, script, &ids)?;
            check(
                actual == *expected,
                "templates.trace_sql_row_order_or_fields",
            )?;
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn execution_check_gameobject_template_trace(
        &mut self,
        rows: &[LoadedGameObjectInfo],
        trace: &[Vec<Vec<u8>>],
    ) -> Result<()> {
        self.check_template_trace("gameobject_template", 37, rows, trace)
    }
    #[cfg(test)]
    pub(crate) fn execution_check_creature_template_trace(
        &mut self,
        rows: &[LoadedCreatureInfo],
        trace: &[Vec<Vec<u8>>],
    ) -> Result<()> {
        self.check_template_trace("creature_template", 96, rows, trace)
    }
    /// These loaders never erase keys, ObjectMgr.cpp:1563-1600,1637-1682,5193-5217.
    pub(crate) fn execution_template_reference_ids(
        &mut self,
        table: &str,
        column: &str,
    ) -> Result<std::collections::BTreeSet<u32>> {
        reference_relation(table, column)?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let field = spawns::position(&spawns::columns(&schema, table)?, column)?;
        let mut ids = std::collections::BTreeSet::new();
        self.scan(table, 0, 256, |row| {
            let mut cells = [None; 256];
            spawns::fill(&row, &mut cells)?;
            ids.insert(spawns::uint(cells[field])?);
            Ok(())
        })?;
        Ok(ids)
    }
}
#[test]
fn fixed_template_reader_refuses_other_relations_and_columns() {
    for (table, columns) in [
        ("creature_template", CREATURE_COLUMNS),
        ("gameobject_template", GO_COLUMNS),
    ] {
        let mut names: Vec<_> = columns.split_whitespace().map(str::to_owned).collect();
        layout(table, &names).unwrap();
        names.swap(0, 1);
        assert_eq!(
            layout(table, &names).unwrap_err().id,
            "templates.sql_layout"
        );
    }
    assert_eq!(
        layout("creature", &[]).unwrap_err().id,
        "templates.relation"
    );
    for (table, column) in [
        ("creature", "guid"),
        ("creature_model_info", "gender"),
        ("page_text", "text"),
    ] {
        assert_eq!(
            reference_relation(table, column).unwrap_err().id,
            "templates.reference_relation"
        );
    }
}
#[test]
fn storage_formats_convert_words_float_bool_and_script_names() {
    let mut cells: [Option<&[u8]>; 256] = [None; 256];
    cells[0] = Some(b"-1");
    cells[1] = Some(b"name\0tail");
    cells[14] = Some(b"-0.0");
    cells[19] = Some(b"-1");
    cells[96] = Some(b"script\0tail");
    let ids = BTreeMap::from([(b"script".to_vec(), 9)]);
    let row: LoadedCreatureInfo = convert(7, &cells, CREATURE_FORMAT, 96, &ids).unwrap();
    assert_eq!(row.words[0], u32::MAX);
    assert_eq!(row.words[14], 0x80000000);
    assert_eq!(row.words[19], 1);
    assert_eq!(row.words[96], 9);
    assert_eq!(row.strings[0], b"name");
    cells[96] = Some(b"unknown");
    assert_eq!(
        convert::<99, 3>(0, &cells, CREATURE_FORMAT, 96, &ids)
            .unwrap()
            .words[96],
        0
    );
    cells[96] = None;
    assert_eq!(
        convert::<99, 3>(0, &cells, CREATURE_FORMAT, 96, &ids)
            .unwrap()
            .words[96],
        0
    );
    let mut cells: [Option<&[u8]>; 256] = [None; 256];
    cells[3] = Some(b"gameobject");
    cells[4] = Some(b"icon");
    cells[8] = Some(b"0.1");
    cells[37] = Some(b"script\0tail");
    let row: LoadedGameObjectInfo = convert(0, &cells, GO_FORMAT, 37, &ids).unwrap();
    assert_eq!(row.words[8], 0x3dcc_cccd);
    assert_eq!(row.words[37], 9);
    assert_eq!(row.strings, [b"gameobject".to_vec(), b"icon".to_vec()]);
}
