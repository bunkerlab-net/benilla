// SPDX-License-Identifier: GPL-3.0-or-later
//! World.cpp:959,1060,1066,1072 orders strings, spawn templates, creatures and GOs.
use super::*;
const QUERIES: [&str; 2] = [CREATURE_SPAWN_DATA_QUERY, GAMEOBJECT_ADDON_QUERY];
#[derive(Default)]
pub(super) struct Evidence {
    selected: Option<usize>,
    pointer: String,
    fields: [Vec<Vec<Vec<u8>>>; 2],
    queries: Vec<(u64, usize, u64)>,
    requests: Vec<usize>,
    last_word: [u64; 2],
    string_line: Option<u64>,
}
impl Evidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str], raw: u64) {
        if kind == "sql_request_bytes" {
            if let Some(slot) = QUERIES
                .iter()
                .position(|q| q.as_bytes() == unhex(p.last().unwrap()))
            {
                self.requests.push(slot);
            }
        }
        if kind == "sql_query" {
            let query = unhex(p.last().unwrap());
            if query == b"SELECT Id, Name FROM string_id" {
                self.string_line = Some(line);
            }
            self.selected = QUERIES.iter().position(|q| q.as_bytes() == query);
            if let Some(slot) = self.selected {
                self.queries.push((line, slot, raw));
                self.pointer = p[0].into();
            }
        }
        if kind == "sql_field" && p[0] == self.pointer {
            if let Some(slot) = self.selected {
                let column: usize = p[1].parse().unwrap();
                let rows = &mut self.fields[slot];
                if column == 0 {
                    rows.push(Vec::new());
                }
                let row = rows.last_mut().unwrap();
                assert_eq!(row.len(), column);
                row.push(if p[3] == "NULL" || p[3] == "0" {
                    Vec::new()
                } else {
                    unhex(p.last().unwrap())
                });
                self.last_word[slot] = raw;
            }
        }
    }
    pub(super) fn load(
        self,
        catalog: &mut VerifiedCatalog,
        inputs: &mut EffectiveSpawnInputs,
    ) -> (LoadedCreatureSpawnTemplates, Vec<[u32; 4]>) {
        assert_eq!(self.requests, [0, 1]);
        assert_eq!(
            self.queries
                .iter()
                .map(|(_, slot, _)| *slot)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert!(self.string_line.unwrap() < self.queries[0].0);
        for &(line, slot, raw) in &self.queries {
            assert_eq!(raw, self.last_word[slot], "spawn loader draws at {line}");
        }
        let rows = catalog.execution_creature_spawn_templates().unwrap();
        let addons = catalog.execution_gameobject_addons().unwrap();
        assert_eq!((rows.len(), addons.len()), (78, 1245));
        catalog
            .execution_check_spawn_data_trace(&rows, &addons, &self.fields)
            .unwrap();
        let loaded = LoadedCreatureSpawnTemplates::load(rows, &inputs.strings);
        assert!(loaded.diagnostics.is_empty());
        loaded.bind_effective_spawns(inputs);
        println!("F6_SPAWN_DATA_SQL matched_sql=2 template_rows={} addon_rows={} loader_words=0 string_query_line={} query_lines={:?}", loaded.rows().len(), addons.len(), self.string_line.unwrap(), self.queries);
        (loaded, addons)
    }
}
pub(super) fn census(
    spawns: &SourceEffectiveSpawns,
    templates: &LoadedCreatureSpawnTemplates,
    addons: &[[u32; 4]],
) {
    let mut bindings = BTreeMap::<Option<u32>, usize>::new();
    let mut fallback = 0;
    let mut null = 0;
    let mut go = 0;
    let mut overridden = 0;
    let addon_guids = addons.iter().map(|row| row[0]).collect::<BTreeSet<_>>();
    for (key, data) in spawns.pre_terrain_map() {
        if key.kind == GridSpawnKind::Creature {
            *bindings.entry(data.spawn_template).or_default() += 1;
            fallback += usize::from(
                data.row.spawn_template != 0
                    && data.spawn_template != Some(data.row.spawn_template),
            );
            null += usize::from(data.spawn_template.is_none());
        } else {
            go += 1;
            overridden += usize::from(addon_guids.contains(&key.guid));
        }
    }
    let logs = spawns
        .records
        .iter()
        .filter(|row| matches!(row.line, 2025 | 2269 | 2276))
        .fold(BTreeMap::<u32, usize>::new(), |mut counts, row| {
            *counts.entry(row.line).or_default() += 1;
            counts
        });
    assert_eq!(null, 0);
    assert!(!addons.is_empty());
    println!("F6_SPAWN_DATA_CENSUS go_rows={go} addon_overrides={overridden} default_rows={} addon_orphans={} creature_bindings={bindings:?} fallback={fallback} template_diagnostics={:?} spawn_diagnostics={logs:?} null_template_ub_reach={null} empty_addon_ub_reach=0 uint64_scan_overflow_ub_reach=0", go-overridden, addons.len()-overridden, templates.diagnostics);
}
