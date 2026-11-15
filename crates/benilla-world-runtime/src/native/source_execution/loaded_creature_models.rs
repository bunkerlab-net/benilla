// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ObjectMgr.cpp:1615-1731, loaded creature model storage and random gender.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use std::collections::{BTreeMap, BTreeSet};
pub type LoadedCreatureModelInfo = SqlTemplateRow<8, 0>;
type ModelDbcInputs = (BTreeSet<u32>, BTreeMap<u32, [u32; 2]>);
#[derive(Debug)]
pub struct LoadedCreatureModels {
    rows: BTreeMap<u32, LoadedCreatureModelInfo>,
    pub notices: Vec<(u32, u32)>,
}
impl LoadedCreatureModels {
    pub fn load(
        rows: Vec<LoadedCreatureModelInfo>,
        displays: &BTreeSet<u32>,
        races: &BTreeMap<u32, [u32; 2]>,
    ) -> Result<Self> {
        require(
            !rows.iter().any(|r| r.words[0] == u32::MAX),
            "execution.creature_models.SQLStorageImpl_317.max_entry_index_undefined",
        )?;
        let mut this = Self {
            rows: rows.into_iter().map(|r| (r.words[0], r)).collect(),
            notices: Vec::new(),
        };
        for (&id, row) in this.rows.range_mut(1..) {
            let w = &mut row.words;
            if !displays.contains(&id) {
                this.notices.push((id, 1647));
            }
            if w[5] >= 3 {
                w[5] = 0;
                this.notices.push((id, 1652));
            }
            for (column, self_line, absent_line) in [(6, 1660, 1665), (7, 1674, 1679)] {
                if w[column] != 0 {
                    if w[column] == id {
                        w[column] = 0;
                        this.notices.push((id, self_line));
                    } else if !displays.contains(&w[column]) {
                        w[column] = 0;
                        this.notices.push((id, absent_line));
                    }
                }
            }
        }
        for (&race, &[male, female]) in races.range(1..) {
            require(
                race <= 32,
                "execution.creature_models.ObjectMgr_1691.race_shift_undefined",
            )?;
            if (1u32 << (race - 1)) & 0xff == 0 {
                continue;
            }
            for (id, gender, other, lines) in [
                (female, 1, male, [1697, 1700, 1705, 1709]),
                (male, 0, female, [1714, 1717, 1722, 1726]),
            ] {
                let Some(row) = this.rows.get_mut(&id) else {
                    this.notices.push((id, lines[3]));
                    continue;
                };
                if row.words[5] != gender {
                    this.notices.push((id, lines[0]));
                }
                if row.words[6] != other {
                    this.notices.push((id, lines[1]));
                }
                if f32::from_bits(row.words[1]) <= 0.0 {
                    row.words[1] = 0.389f32.to_bits();
                    this.notices.push((id, lines[2]));
                }
            }
        }
        Ok(this)
    }
    pub fn get(&self, id: u32) -> Option<&LoadedCreatureModelInfo> {
        self.rows.get(&id)
    }
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    pub fn random_gender(
        &self,
        display: u32,
        random: &mut dyn CreatureRandom,
    ) -> Result<Option<&LoadedCreatureModelInfo>> {
        let Some(model) = self.get(display) else {
            return Ok(None);
        };
        if model.words[6] != 0 && random.creature_urand(0, 1)? == 0 {
            return Ok(Some(self.get(model.words[6]).unwrap_or(model)));
        }
        Ok(Some(model))
    }
    pub fn catalog_diagnostic(catalog: &mut VerifiedCatalog) -> Result<Self> {
        let rows = catalog.execution_creature_model_rows()?;
        let (displays, races) = Self::catalog_dbc(catalog)?;
        Self::load(rows, &displays, &races)
    }
    pub(crate) fn catalog_dbc(catalog: &mut VerifiedCatalog) -> Result<ModelDbcInputs> {
        fn word(row: &serde_json::Value, column: u32) -> Result<u32> {
            row["values"]
                .as_array()
                .and_then(|v| v.iter().find(|v| v[0] == column))
                .and_then(|v| v[2].as_u64())
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| refuse("execution.creature_models.dbc_layout"))
        }
        let mut displays = BTreeSet::new();
        let mut view = catalog.dbc_view("CreatureDisplayInfo.dbc")?;
        for ordinal in 0..view.record_count() {
            displays.insert(word(&view.record(ordinal)?, 0)?);
        }
        drop(view);
        let mut races = BTreeMap::new();
        let mut view = catalog.dbc_view("ChrRaces.dbc")?;
        for ordinal in 0..view.record_count() {
            let row = view.record(ordinal)?;
            races.insert(word(&row, 0)?, [word(&row, 4)?, word(&row, 5)?]);
        }
        Ok((displays, races))
    }
}
#[cfg(test)]
#[path = "loaded_creature_models_tests.rs"]
mod tests;
