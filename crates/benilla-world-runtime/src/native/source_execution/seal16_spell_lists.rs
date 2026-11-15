// SPDX-License-Identifier: GPL-3.0-or-later
//! World.cpp:1030,1033,1036 SQL and loaded products, compared after execution.
use super::*;
use std::sync::Arc;
const QUERIES: [&str;5]=[
    "SELECT Id, Type, Param1, Param2, Param3, UnitCondition, Comments FROM creature_spell_targeting",
    "SELECT Id, Name, ChanceSupportAction, ChanceRangedAttack FROM creature_spell_list_entry",
    "SELECT Id, Position, SpellId, Flags, CombatCondition, TargetId, ScriptId, Availability, Probability, InitialMin, InitialMax, RepeatMin, RepeatMax FROM creature_spell_list",
    "SELECT Entry, SpellId, CooldownMin, CooldownMax FROM creature_cooldowns",
    "SELECT entry, setId, spell1, spell2, spell3, spell4, spell5, spell6, spell7, spell8, spell9, spell10 FROM creature_template_spells",
];
#[derive(Default)]
pub(super) struct Evidence {
    pointer: String,
    selected: Option<usize>,
    queries: Vec<(u64, usize, u64)>,
    rows: [Vec<Vec<Vec<u8>>>; 5],
    previous_query: Option<Vec<u8>>,
    collecting: bool,
    following_query: Option<Vec<u8>>,
    last_raw: Option<u64>,
}
impl Evidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str], raw: u64) {
        if kind == "sql_query" {
            let query = unhex(p.last().unwrap());
            self.selected = QUERIES.iter().position(|q| q.as_bytes() == query);
            if let Some(slot) = self.selected {
                if slot == 0 {
                    assert_eq!(
                        self.previous_query.as_deref(),
                        Some(b"SELECT * FROM combat_condition".as_slice())
                    );
                    self.collecting = true;
                }
                assert!(self.collecting);
                self.queries.push((line, slot, raw));
                self.pointer = p[0].into();
            } else if self.collecting {
                assert_eq!(self.queries.len(), 5);
                self.following_query = Some(query.clone());
                self.collecting = false;
            }
            self.previous_query = Some(query);
        }
        if kind == "sql_field" && p[0] == self.pointer {
            if let Some(slot) = self.selected {
                let column = p[1].parse::<usize>().unwrap();
                if column == 0 {
                    self.rows[slot].push(Vec::new());
                }
                let row = self.rows[slot].last_mut().unwrap();
                assert_eq!(row.len(), column);
                row.push(super::templates::sql_storage_field(p));
                self.last_raw = Some(raw);
            }
        }
    }
    pub(super) fn load(
        self,
        catalog: &mut VerifiedCatalog,
        creatures: &LoadedCreatureTemplates,
    ) -> Arc<LoadedCreatureSpellLists> {
        assert_eq!(
            self.queries.iter().map(|q| q.1).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4]
        );
        assert_eq!(
            self.following_query.as_deref(),
            Some(b"SELECT entry,type,targetEntry FROM item_required_target".as_slice())
        );
        assert!(self.queries.iter().all(|q| Some(q.2) == self.last_raw));
        let rows = catalog.execution_creature_spell_lists().unwrap();
        rows.check_trace(&self.rows);
        let (store, census) = LoadedCreatureSpellLists::from_catalog_rows(rows, creatures);
        let unwritten_ai_reach = store
            .lists
            .values()
            .filter(|l| l.ai_has_spells())
            .flat_map(|l| l.spells.values())
            .filter(|s| !s.has_written_combat_condition())
            .count();
        assert_eq!(unwritten_ai_reach, 0);
        println!("F6_CREATURE_SPELL_LISTS matched_sql=5 additional_raw_words=0 query_lines={:?} census={census:?} total_lists={} cold_start_unwritten_UnitAI_1213_reach={unwritten_ai_reach}",self.queries.iter().map(|q|q.0).collect::<Vec<_>>(),store.lists.len());
        Arc::new(store)
    }
}
