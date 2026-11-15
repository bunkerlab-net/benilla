// SPDX-License-Identifier: GPL-3.0-or-later
//! SQLStorageImpl.h:309-329 submits MAX, COUNT, then SELECT * without drawing RNG.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use std::collections::BTreeMap;
#[path = "seal16_template_order.rs"]
mod restored;
const QUERIES: [&str; 6] = [
    "SELECT MAX(entry) FROM gameobject_template",
    "SELECT COUNT(*) FROM gameobject_template",
    "SELECT * FROM gameobject_template",
    "SELECT MAX(entry) FROM creature_template",
    "SELECT COUNT(*) FROM creature_template",
    "SELECT * FROM creature_template",
];
pub(super) fn sql_storage_field(p: &[&str]) -> Vec<u8> {
    if p[3] == "NULL" {
        assert_eq!(p.len(), 4);
        return Vec::new(); // Field.h:45-59 converts SQL NULL to empty strings or numeric zero.
    }
    let bytes = p[3].parse::<usize>().unwrap();
    assert_eq!(p.len(), if bytes == 0 { 5 } else { 6 });
    assert_eq!(p[4], "0");
    if bytes == 0 {
        return Vec::new();
    }
    assert_eq!(p[5].len(), bytes * 2);
    unhex(p[5])
}
#[derive(Default)]
pub(super) struct Evidence {
    queries: Vec<(u64, String, u64)>,
    pointer: String,
    selected: Option<usize>,
    fields: BTreeMap<usize, Vec<Vec<Vec<u8>>>>,
    last_word: BTreeMap<usize, u64>,
}
impl Evidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str], raw: u64) {
        if kind == "sql_query" {
            let query = unhex(p.last().unwrap());
            self.selected = QUERIES.iter().position(|value| value.as_bytes() == query);
            if let Some(slot) = self.selected {
                self.queries.push((line, QUERIES[slot].into(), raw));
                self.pointer = p[0].into();
                assert!(self.fields.insert(slot, Vec::new()).is_none());
            }
        }
        if kind == "sql_field" && p[0] == self.pointer {
            if let Some(slot) = self.selected {
                let field: usize = p[1].parse().unwrap();
                let rows = self.fields.get_mut(&slot).unwrap();
                if field == 0 {
                    rows.push(Vec::new());
                }
                let row = rows.last_mut().unwrap();
                assert_eq!(row.len(), field);
                row.push(sql_storage_field(p));
                self.last_word.insert(slot, raw);
            }
        }
    }
    pub(super) fn load(
        self,
        catalog: &mut VerifiedCatalog,
        recording: &Path,
        model_evidence: super::creature_models::Evidence,
        geometry: &mut SourceTerrainManager,
    ) -> (
        LoadedGameObjectTemplates,
        LoadedCreatureTemplates,
        LoadedCreatureModels,
        SourceDynamicModels,
    ) {
        assert_eq!(
            self.queries
                .iter()
                .map(|(_, q, _)| q.as_str())
                .collect::<Vec<_>>(),
            QUERIES
        );
        let check_storage = |slot: usize, rows: usize, max: u32| {
            assert_eq!(self.fields[&slot], vec![vec![max.to_string().into_bytes()]]);
            assert_eq!(
                self.fields[&(slot + 1)],
                vec![vec![rows.to_string().into_bytes()]]
            );
            assert_eq!(self.queries[slot].2, self.last_word[&(slot + 2)]);
        };
        let order = restored::snapshot_order(recording).unwrap();
        let go_rows = restored::bind_rows(
            catalog.execution_loaded_gameobject_rows().unwrap(),
            &order[0],
        )
        .unwrap();
        catalog
            .execution_check_gameobject_template_trace(&go_rows, &self.fields[&2])
            .unwrap();
        check_storage(
            0,
            go_rows.len(),
            go_rows.iter().map(|row| row.words[0]).max().unwrap_or(0),
        );
        let go =
            LoadedGameObjectTemplates::load(go_rows, &mut TemplateCatalog::new(catalog)).unwrap();
        let dynamic_models = super::gameobject_models::load(recording, catalog, geometry);
        let models = model_evidence.load(catalog, self.queries[2].0, self.queries[3].0);
        let creature_rows =
            restored::bind_rows(catalog.execution_loaded_creature_rows().unwrap(), &order[1])
                .unwrap();
        assert_eq!(
            LoadedCreatureTemplates::class_shift_ub_reach(&creature_rows),
            0
        );
        catalog
            .execution_check_creature_template_trace(&creature_rows, &self.fields[&5])
            .unwrap();
        check_storage(
            3,
            creature_rows.len(),
            creature_rows
                .iter()
                .map(|row| row.words[0])
                .max()
                .unwrap_or(0),
        );
        let creatures =
            LoadedCreatureTemplates::load(creature_rows, &mut TemplateCatalog::new(catalog))
                .unwrap();
        println!(
            "F6_TEMPLATE_TRANSPORT_DISPLAYS {:?}",
            go.transport_displays()
        );
        println!("F6_TEMPLATE_SQL matched_sql=6 gameobject_rows={} creature_rows={} loader_words=0 query_lines={:?}",go.rows().len(),creatures.rows().len(),self.queries.iter().map(|(line,_,_)|line).collect::<Vec<_>>());
        (go, creatures, models, dynamic_models)
    }
}

#[test]
fn sql_storage_fields_distinguish_explicit_null_empty_and_hex_bytes() {
    assert!(sql_storage_field(&["result", "2", "1", "NULL"]).is_empty());
    assert!(sql_storage_field(&["result", "2", "1", "0", "0"]).is_empty());
    assert_eq!(
        sql_storage_field(&["result", "2", "1", "3", "0", "4e5043"]),
        b"NPC"
    );
    assert!(std::panic::catch_unwind(|| {
        sql_storage_field(&["result", "2", "1", "1", "0", "f"])
    })
    .is_err());
}
