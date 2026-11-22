// SPDX-License-Identifier: GPL-3.0-or-later
//! World.cpp:1012 precedes creature templates at :1021; SQLStorageImpl.h:309-329.
use super::*;
const QUERIES: [&str; 3] = [
    "SELECT MAX(modelid) FROM creature_model_info",
    "SELECT COUNT(*) FROM creature_model_info",
    "SELECT * FROM creature_model_info",
];
#[derive(Default)]
pub(super) struct Evidence {
    pointer: String,
    selected: Option<usize>,
    queries: Vec<(u64, String, u64)>,
    fields: BTreeMap<usize, Vec<Vec<Vec<u8>>>>,
    last_word: u64,
}
impl Evidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str], raw: u64) {
        if kind == "sql_query" {
            let query = unhex(p.last().unwrap());
            self.selected = QUERIES.iter().position(|q| q.as_bytes() == query);
            if let Some(slot) = self.selected {
                self.queries.push((line, QUERIES[slot].into(), raw));
                self.pointer = p[0].into();
                assert!(self.fields.insert(slot, Vec::new()).is_none());
            }
        }
        if kind == "sql_field" && p[0] == self.pointer {
            if let Some(slot) = self.selected {
                let column = p[1].parse::<usize>().unwrap();
                let rows = self.fields.get_mut(&slot).unwrap();
                if column == 0 {
                    rows.push(Vec::new());
                }
                let row = rows.last_mut().unwrap();
                assert_eq!(row.len(), column);
                row.push(super::templates::sql_storage_field(p));
                self.last_word = raw;
            }
        }
    }
    pub(super) fn load(
        self,
        catalog: &mut VerifiedCatalog,
        before: u64,
        after: u64,
    ) -> LoadedCreatureModels {
        assert_eq!(
            self.queries
                .iter()
                .map(|(_, q, _)| q.as_str())
                .collect::<Vec<_>>(),
            QUERIES
        );
        assert!(before < self.queries[0].0 && self.queries[2].0 < after);
        let rows = catalog.execution_creature_model_rows().unwrap();
        catalog
            .execution_check_creature_model_trace(&rows, &self.fields[&2])
            .unwrap();
        assert_eq!(
            self.fields[&0],
            vec![vec![rows
                .iter()
                .map(|r| r.words[0])
                .max()
                .unwrap_or(0)
                .to_string()
                .into_bytes()]]
        );
        assert_eq!(
            self.fields[&1],
            vec![vec![rows.len().to_string().into_bytes()]]
        );
        assert!(self.queries.iter().all(|q| q.2 == self.last_word));
        let (displays, races) = LoadedCreatureModels::catalog_dbc(catalog).unwrap();
        let max_ub = rows.iter().filter(|r| r.words[0] == u32::MAX).count();
        let shift_ub = races.keys().filter(|&&race| race > 32).count();
        assert_eq!((max_ub, shift_ub), (0, 0));
        let models = LoadedCreatureModels::load(rows, &displays, &races).unwrap();
        let mut notices = BTreeMap::<u32, usize>::new();
        for &(_, line) in &models.notices {
            *notices.entry(line).or_default() += 1;
        }
        println!("F6_CREATURE_MODEL_SQL matched_sql=3 model_rows={} loader_words=0 query_lines={:?} notices={notices:?} max_index_ub={max_ub} race_shift_ub={shift_ub}", models.len(), self.queries.iter().map(|q| q.0).collect::<Vec<_>>());
        models
    }
}

pub(super) fn faction_flags(catalog: &mut VerifiedCatalog) -> BTreeMap<u32, u32> {
    let mut output = BTreeMap::new();
    let mut view = catalog.dbc_view("FactionTemplate.dbc").unwrap();
    for ordinal in 0..view.record_count() {
        let row = view.record(ordinal).unwrap();
        let values = row["values"].as_array().unwrap();
        let word = |column| {
            values.iter().find(|value| value[0] == column).unwrap()[2]
                .as_u64()
                .unwrap() as u32
        };
        output.insert(word(0), word(2));
    }
    output
}

pub(super) fn entry_census(spawns: &SourceEffectiveSpawns, templates: &LoadedCreatureTemplates) {
    let display_ub = templates
        .rows()
        .iter()
        .filter(|row| {
            let total = (0..4)
                .filter(|&i| row.words[5 + i] != 0)
                .fold(0u32, |total, i| total.wrapping_add(row.words[9 + i]));
            total == 0x80000000
        })
        .count();
    let school_ub = templates
        .rows()
        .iter()
        .filter(|row| row.words[62] > 7)
        .count();
    let movement_ub = spawns
        .pre_terrain_map()
        .iter()
        .filter(|(key, data)| key.kind == GridSpawnKind::Creature && data.row.movement > 31)
        .count();
    assert_eq!((display_ub, school_ub, movement_ub), (0, 0, 0));
    println!("F6_OBJECT_CREATE_UB template_rows={} spawn_rows={} display_signed_overflow={display_ub} school_enum_undefined={school_ub} movement_enum_undefined={movement_ub}", templates.rows().len(), spawns.pre_terrain_map().len());
}
