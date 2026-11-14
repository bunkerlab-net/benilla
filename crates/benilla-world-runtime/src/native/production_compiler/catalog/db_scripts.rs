// SPDX-License-Identifier: GPL-3.0-or-later
//! Dedicated DB-script query/operand boundary. No existing catalog gate is widened.
use super::*;

fn relation(table: &str) -> Result<()> {
    check(
        matches!(
            table,
            "dbscripts_on_quest_end"
                | "dbscripts_on_quest_start"
                | "dbscripts_on_spell"
                | "dbscripts_on_go_use"
                | "dbscripts_on_go_template_use"
                | "dbscripts_on_event"
                | "dbscripts_on_gossip"
                | "dbscripts_on_creature_death"
                | "dbscripts_on_creature_movement"
                | "dbscripts_on_relay"
                | "dbscript_random_templates"
        ),
        "scripts.db_script_relation",
    )
}
fn operand_relation(table: &str) -> Result<()> {
    check(
        matches!(
            table,
            "creature_template"
                | "gameobject_template"
                | "creature"
                | "gameobject"
                | "conditions"
                | "item_template"
                | "string_id"
                | "worldstate_name"
                | "quest_template"
        ),
        "scripts.db_script_operand_relation",
    )
}
impl VerifiedCatalog {
    /// Bound F2.7 SQL cursor, including its ORDER BY priority ties, not catalog order.
    pub(crate) fn execution_db_script_rows<const N: usize>(
        &mut self,
        occurrences: &Path,
        table: &str,
    ) -> Result<Vec<(u64, [u32; N])>> {
        relation(table)?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let metadata = reader::Held::open(&occurrences.join("occurrences.json"), CHUNK as u64)?;
        let meta = bounded_json(&metadata.read(
            0,
            metadata.len() as usize,
            &mut Metrics::default(),
            CACHE,
        )?)?;
        catalog_schema::fields(
            &meta,
            &["format", "streams", "complete", "source_snapshot"],
            "scripts.occurrences",
        )?;
        check(
            meta["format"] == "benilla-script-occurrences-v1"
                && meta["complete"] == false
                && meta["source_snapshot"] == "phase0-sequential-diagnostic-not-F1",
            "scripts.occurrences",
        )?;
        let tables = db_tables();
        let ids = loaders(&tables);
        let loader = if table == "dbscript_random_templates" {
            "DBScripts/ScriptMgr:1230".into()
        } else {
            format!("dbscript:{table}")
        };
        let slot = ids
            .iter()
            .position(|s| s == &loader)
            .ok_or_else(|| fail("scripts.db_script_relation"))?;
        let spec = &catalog_schema::POLICY["loaders"][&loader];
        let d = &meta["streams"][slot.to_string()];
        catalog_schema::fields(d, &["query", "rows", "bytes", "sha256"], "scripts.query")?;
        check(d["query"] == spec["query"], "scripts.query")?;
        let (count, expected) = expected(self, &schema, spec)?;
        check(count == N, "scripts.db_script_layout")?;
        check(expected.len() <= DECODED / 768, "scripts.view_limit")?;
        let mut keys = BTreeMap::<[u8; 32], VecDeque<Vec<u32>>>::new();
        for (order, hash) in expected {
            keys.entry(hash).or_default().push_back(order);
        }
        for orders in keys.values_mut() {
            orders.make_contiguous().sort_unstable();
        }
        let held = reader::Held::open(&occurrences.join(format!("{slot}.rows")), MAX_RAW)?;
        let mut buffer = vec![0; FRAME];
        let mut at = 0;
        let mut hash = Sha256::new();
        let mut previous = Vec::new();
        let rows = number(&d["rows"])?;
        check(rows <= (DECODED / 768) as u64, "scripts.view_limit")?;
        let mut output = Vec::with_capacity(rows as usize);
        for ordinal in 0..rows {
            let row = groups::read_stream_row(&held, &mut buffer, &mut at, ordinal, count)?;
            let values = cells(&row)?;
            let h = hash_cells(values[..count].iter().copied());
            let order = keys
                .get_mut(&h)
                .and_then(VecDeque::pop_front)
                .ok_or_else(|| fail("scripts.view_mismatch"))?;
            check(previous <= order, "scripts.order")?;
            previous = order;
            let mut words = [0; N];
            for (i, (word, cell)) in words.iter_mut().zip(&values[..count]).enumerate() {
                *word = if count == 20 && (13..=18).contains(&i) {
                    spawns::floating(*cell)?.to_bits()
                } else {
                    spawns::uint(*cell)?
                };
            }
            output.push((ordinal, words));
            hash.update(row.frame);
        }
        check(
            at == held.len() && keys.values().all(VecDeque::is_empty),
            "scripts.view_mismatch",
        )?;
        check(
            d["bytes"] == held.len() && d["sha256"] == hex(&hash.finalize()),
            "scripts.hash",
        )?;
        metadata.check()?;
        held.check()?;
        Ok(output)
    }
    /// SpecialFlags dependency slice of F2.9 and LoadQuestAreaTriggers' query.
    pub(crate) fn execution_quest_flag_operands<const N: usize>(
        &mut self,
        table: &str,
        names: [&str; N],
    ) -> Result<Vec<[u32; N]>> {
        check(
            matches!(table, "quest_template" | "areatrigger_involvedrelation"),
            "execution.quest_flags.relation_owner",
        )?;
        check(
            self.relations.contains_key(table),
            if table == "quest_template" {
                "execution.quest_flags.quest_template_owner"
            } else {
                "execution.quest_flags.area_trigger_relations_owner"
            },
        )?;
        self.db_script_operand_words(table, names)
    }
    /// Narrow existence/metadata views. Loaded pools remain external.
    pub(crate) fn execution_db_script_operands<const N: usize>(
        &mut self,
        table: &str,
        names: [&str; N],
    ) -> Result<Vec<[u32; N]>> {
        operand_relation(table)?;
        self.db_script_operand_words(table, names)
    }
    fn db_script_operand_words<const N: usize>(
        &mut self,
        table: &str,
        names: [&str; N],
    ) -> Result<Vec<[u32; N]>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, table)?;
        let indices = names
            .map(|name| position(cols, name))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut output = Vec::new();
        self.scan(table, 0, 256, |row| {
            let values = cells(&row)?;
            let mut words = [0; N];
            for (word, &i) in words.iter_mut().zip(&indices) {
                *word = spawns::uint(values[i])?;
            }
            output.push(words);
            check(
                output.len() <= DECODED / std::mem::size_of::<[u32; N]>().max(1),
                "scripts.view_limit",
            )
        })?;
        Ok(output)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dbscript_reader_refuses_every_unowned_relation() {
        for table in [
            "internal_use",
            "creature_ai_scripts",
            "quest_template",
            "spell_template",
            "dbscripts_on_fake",
        ] {
            assert_eq!(
                relation(table).unwrap_err().id,
                "scripts.db_script_relation"
            );
        }
        for table in [
            "spell_template",
            "creature_ai_scripts",
            "pool_template",
            "spawn_group",
            "internal_use",
        ] {
            assert_eq!(
                operand_relation(table).unwrap_err().id,
                "scripts.db_script_operand_relation"
            );
        }
        for table in db_tables() {
            relation(&table).unwrap();
        }
        relation("dbscript_random_templates").unwrap();
    }
}
