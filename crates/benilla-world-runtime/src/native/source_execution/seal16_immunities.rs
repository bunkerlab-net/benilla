// SPDX-License-Identifier: GPL-3.0-or-later
//! World.cpp:1021-1027 startup position and ObjectMgr.cpp:948 SQL comparison.
use super::*;
const QUERY: &[u8] = b"SELECT Entry, SetId, Type, Value FROM creature_immunities";
#[derive(Default)]
pub(super) struct Evidence {
    previous: Option<Vec<u8>>,
    pointer: Option<String>,
    query: Option<(u64, u64)>,
    following: Option<(Vec<u8>, u64)>,
    rows: Vec<Vec<Vec<u8>>>,
    last_raw: Option<u64>,
}
impl Evidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str], raw: u64) {
        if kind == "sql_query" {
            let query = unhex(p.last().unwrap());
            if query == QUERY {
                assert_eq!(
                    self.previous.as_deref(),
                    Some(b"SELECT * FROM creature_template".as_slice())
                );
                assert!(self.query.replace((line, raw)).is_none());
                self.pointer = Some(p[0].into());
            } else if self.query.is_some() && self.following.is_none() {
                self.following = Some((query.clone(), raw));
                self.pointer = None;
            }
            self.previous = Some(query);
        }
        if kind == "sql_field" && self.pointer.as_deref() == Some(p[0]) {
            let column = p[1].parse::<usize>().unwrap();
            if column == 0 {
                self.rows.push(Vec::new());
            }
            let row = self.rows.last_mut().unwrap();
            assert_eq!(row.len(), column);
            row.push(super::templates::sql_storage_field(p));
            self.last_raw = Some(raw);
        }
    }
    pub(super) fn load(
        self,
        catalog: &mut VerifiedCatalog,
        creatures: &LoadedCreatureTemplates,
    ) -> LoadedCreatureImmunities {
        let (line, raw) = self.query.unwrap();
        assert_eq!(
            self.following,
            Some((b"SELECT * FROM unit_condition".to_vec(), raw))
        );
        assert!(self.last_raw.is_none_or(|last| last == raw));
        let rows = catalog.execution_creature_immunities().unwrap();
        let expected: Vec<[u32; 4]> = self
            .rows
            .iter()
            .map(|row| {
                assert_eq!(row.len(), 4);
                std::array::from_fn(|i| {
                    std::str::from_utf8(&row[i])
                        .unwrap()
                        .parse::<i64>()
                        .unwrap() as u32
                })
            })
            .collect();
        assert_eq!(rows, expected);
        let (store, census) =
            LoadedCreatureImmunities::load(&rows, |entry| creatures.get(entry).is_some());
        assert_eq!(census.visited, rows.len());
        println!("F6_CREATURE_IMMUNITIES matched_sql=1 query_line={line} additional_raw_words=0 census={census:?} validated_kind_array_index_UB_reach=0");
        store
    }
}
