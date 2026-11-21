// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[derive(Default)]
pub(super) struct Evidence {
    request: bool,
    result: Option<String>,
    keys: BTreeMap<u32, usize>,
    ended: bool,
}

impl Evidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, fields: &[&str]) {
        match line {
            51647696 => {
                assert_eq!(kind, "sql_request_bytes");
                assert_eq!(
                    unhex(fields.last().unwrap()),
                    REFERENCE_LOOT_QUERY.as_bytes()
                );
                self.request = true;
            }
            51648074 => {
                assert_eq!(kind, "sql_query_result");
                assert_eq!(&fields[3..], ["32972", "7", "0"]);
                self.result = Some(fields[2].to_owned());
            }
            51648115..=52609073 if kind == "sql_field" && fields[1] == "0" => {
                assert_eq!(Some(fields[0]), self.result.as_deref());
                let bytes = unhex(fields.last().unwrap());
                let id = std::str::from_utf8(&bytes).unwrap().parse().unwrap();
                *self.keys.entry(id).or_default() += 1;
            }
            52609100 => {
                assert_eq!(kind, "sql_result_end");
                assert_eq!(fields, ["4294967603"]);
                self.ended = true;
            }
            _ => {}
        }
    }

    // World.cpp:1200, after the item and condition loaders. Bind their recorded key views.
    pub(super) fn load(
        &self,
        catalog: &mut VerifiedCatalog,
        recording: &Path,
    ) -> SourceReferenceLootStore {
        assert!(self.request && self.result.is_some() && self.ended);
        let rows = catalog.execution_reference_loot_rows().unwrap();
        let mut keys = BTreeMap::new();
        for row in &rows {
            *keys.entry(row.entry).or_insert(0usize) += 1;
        }
        assert_eq!(keys, self.keys);
        assert_eq!(rows.len(), 32972);
        assert_eq!(keys.len(), 551);
        assert_eq!(keys[&1], 17);
        let conditions = catalog.execution_reference_loot_condition_ids().unwrap();
        // ObjectMgr.cpp:5526-5557 erases invalid keys. None were erased on this run.
        let log = std::fs::read_to_string(recording.join("run/console.log")).unwrap();
        assert!(log.contains(">> Loaded 1260 Condition definitions"));
        assert!(!log.contains("ObjectMgr::LoadConditions: invalid condition_entry"));
        assert_eq!(conditions.len(), 1260);
        // The lookup key widens the u16 loot operand. The condition store keeps u32 IDs.
        assert!(conditions.contains(&700001) && conditions.contains(&5310010));
        let items = catalog.execution_equipment_items().unwrap();
        let mut store = SourceReferenceLootStore::default();
        let admitted = store
            .load(&mut Inputs {
                rows,
                items,
                conditions,
            })
            .unwrap();
        assert_eq!(admitted, 32972);
        assert!(store
            .template_ids()
            .iter()
            .copied()
            .eq(keys.keys().copied()));
        assert!(store.have_loot_for(1));
        store
    }
}

struct Inputs {
    rows: Vec<ReferenceLootRow>,
    items: BTreeMap<u32, EquipmentItem>,
    conditions: BTreeSet<u32>,
}

impl ReferenceLootInputs for Inputs {
    fn rows(&mut self, query: &str) -> Result<Vec<ReferenceLootRow>> {
        assert_eq!(query, REFERENCE_LOOT_QUERY);
        Ok(std::mem::take(&mut self.rows))
    }

    fn condition_exists(&mut self, id: u16) -> Result<bool> {
        Ok(self.conditions.contains(&u32::from(id)))
    }

    fn item_exists(&mut self, id: u32) -> Result<bool> {
        Ok(self.items.contains_key(&id))
    }
}
