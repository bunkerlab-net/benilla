// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! World.cpp:933 precedes spawn loading at :1066/:1072.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// SQLStorages.cpp:34-35 and Map.h:74-87, after SQLInstanceLoader conversion.
#[derive(Clone, Debug)]
pub struct InstanceTemplateRow {
    pub map: u32,
    pub parent: u32,
    pub level_min: u32,
    pub level_max: u32,
    pub max_players: u32,
    pub reset_days: u32,
    pub ghost_map: i32,
    pub ghost_xy: [f32; 2],
    pub script_id: u32,
    pub mount_allowed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InstanceTemplateDisposition {
    pub map: u32,
    pub line: u32,
    pub erased: bool,
}
/// Immutable post-LoadInstanceTemplate lookup, not the raw SQL key set.
#[derive(Debug)]
pub struct LoadedInstanceTemplates {
    rows: BTreeMap<u32, InstanceTemplateRow>,
    raw_rows: usize,
    dispositions: Vec<InstanceTemplateDisposition>,
}
impl LoadedInstanceTemplates {
    /// Executes the World.cpp:933 loader before spawn loaders at :1066/:1072.
    /// Catalog inputs remain diagnostics, not an F1 SQL occurrence binding.
    pub fn catalog_diagnostic(
        catalog: &mut super::super::production_compiler::VerifiedCatalog,
        reset: &InstanceResetConfig,
    ) -> Result<Self> {
        let maps = catalog.execution_map_entries()?.into_iter().collect();
        Self::load(catalog.execution_instance_templates()?, &maps, reset)
    }
    pub fn load(
        rows: impl IntoIterator<Item = InstanceTemplateRow>,
        maps: &BTreeMap<u32, u32>,
        reset: &InstanceResetConfig,
    ) -> Result<Self> {
        let mut loaded = BTreeMap::new();
        let mut raw_rows = 0;
        for row in rows {
            raw_rows += 1;
            require(
                loaded.insert(row.map, row).is_none(),
                "execution.instance_templates.duplicate_sql_key_order_owner",
            )?;
        }
        let mut dispositions = Vec::new();
        // ObjectMgr.cpp:5372 visits indexed keys, not SELECT * occurrence order.
        let keys: Vec<_> = loaded.keys().copied().collect();
        for map in keys {
            let erased = if let Some(kind) = maps.get(&map) {
                if !matches!(kind, 1..=3) && !matches!(map, 369 | 449 | 450) {
                    Some(5390)
                } else {
                    None
                }
            } else {
                Some(5382)
            };
            if let Some(line) = erased {
                loaded.remove(&map);
                dispositions.push(InstanceTemplateDisposition {
                    map,
                    line,
                    erased: true,
                });
                continue;
            }
            let row = loaded.get_mut(&map).unwrap();
            if row.parent > 0 {
                if !maps.contains_key(&row.parent) {
                    row.parent = 0;
                    dispositions.push(InstanceTemplateDisposition {
                        map,
                        line: 5402,
                        erased: false,
                    });
                    // This continue also skips ghost checks and reset scaling, :5403.
                    continue;
                }
                if matches!(row.parent, 0 | 1) {
                    row.parent = 0;
                    dispositions.push(InstanceTemplateDisposition {
                        map,
                        line: 5410,
                        erased: false,
                    });
                }
            }
            let row = &loaded[&map];
            if row.ghost_map >= 0 {
                let ghost = row.ghost_map as u32;
                // MapManager.h:100-102 first calls IsValidMAP, including current loaded state.
                let valid_map = maps
                    .get(&ghost)
                    .is_some_and(|kind| !matches!(kind, 1 | 2) || loaded.contains_key(&ghost));
                let limit = f64::from(533.333_3_f32 * 64.0 / 2.0) - 0.5;
                let valid_xy = row
                    .ghost_xy
                    .iter()
                    .all(|v| v.is_finite() && f64::from(v.abs()) <= limit);
                let line = if !valid_map || !valid_xy {
                    Some(5420)
                }
                // :5428 is unreachable: IsValidMAP already required the DBC entry.
                else if !matches!(ghost, 0 | 1) {
                    Some(5436)
                } else {
                    None
                };
                if let Some(line) = line {
                    loaded.remove(&map);
                    dispositions.push(InstanceTemplateDisposition {
                        map,
                        line,
                        erased: true,
                    });
                    continue;
                }
            }
            let row = loaded.get_mut(&map).unwrap();
            if row.reset_days != 0 {
                row.reset_days = reset.scale(row.reset_days)?;
            }
        }
        Ok(Self {
            rows: loaded,
            raw_rows,
            dispositions,
        })
    }
    pub fn rows(&self) -> &BTreeMap<u32, InstanceTemplateRow> {
        &self.rows
    }
    pub fn raw_rows(&self) -> usize {
        self.raw_rows
    }
    pub fn dispositions(&self) -> &[InstanceTemplateDisposition] {
        &self.dispositions
    }
    pub fn bind_effective_spawns(&self, inputs: &mut EffectiveSpawnInputs) {
        inputs.instances = Some(self.rows.keys().copied().collect::<BTreeSet<_>>());
    }
}

#[cfg(test)]
#[path = "instance_templates_tests.rs"]
mod tests;
