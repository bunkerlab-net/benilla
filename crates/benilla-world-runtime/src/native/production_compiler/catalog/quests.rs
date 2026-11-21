// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::source_execution::{
    QuestRelationKind, SourceQuestTemplate, QUEST_TEMPLATE_QUERY,
};
use spawns::{columns, fill, floating, position, uint};

impl SourceQuestTemplate {
    /// QuestDef.cpp:23-140. Cells follow QUEST_TEMPLATE_QUERY, including SQL NULL.
    pub fn from_sql_fields(fields: [Option<&[u8]>; 131]) -> Result<Self> {
        let mut words = [0; 131];
        let mut text: [Box<[u8]>; 10] = std::array::from_fn(|_| Box::default());
        for (i, raw) in fields.into_iter().enumerate() {
            match i {
                28..=37 => {
                    let bytes = raw.unwrap_or_default();
                    let end = bytes
                        .iter()
                        .position(|&byte| byte == 0)
                        .unwrap_or(bytes.len());
                    text[i - 28] = bytes[..end].into();
                }
                103 | 104 => words[i] = floating(raw)?.to_bits(),
                _ => words[i] = uint(raw)?,
            }
        }
        Ok(Self::from_decoded_fields(words, text))
    }
}

impl VerifiedCatalog {
    pub(crate) fn scan_execution_quest_templates(
        &mut self,
        mut visit: impl FnMut(SourceQuestTemplate) -> crate::native::source_execution::Result<()>,
    ) -> Result<()> {
        const TABLE: &str = "quest_template";
        check(
            self.relations.contains_key(TABLE),
            "execution.quests.ObjectMgr_4239.quest_template_owner",
        )?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, TABLE)?;
        let projection = QUEST_TEMPLATE_QUERY
            .strip_prefix("SELECT ")
            .and_then(|s| s.strip_suffix(" FROM quest_template"))
            .ok_or_else(|| fail("execution.quests.quest_projection"))?;
        let indices: [usize; 131] = projection
            .split(',')
            .map(|name| position(&cols, name.trim()))
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .map_err(|_| fail("execution.quests.quest_projection"))?;
        self.scan(TABLE, 0, 256, |row| {
            let mut fields = [None; 256];
            fill(&row, &mut fields)?;
            visit(SourceQuestTemplate::from_sql_fields(
                indices.map(|i| fields[i]),
            )?)
            .map_err(|error| fail(&error.id))
        })
    }

    pub(crate) fn execution_quest_relations(
        &mut self,
        kind: QuestRelationKind,
    ) -> Result<Vec<[u32; 2]>> {
        let table = kind.table();
        check(
            self.relations.contains_key(table),
            &format!("execution.quests.ObjectMgr_7946.{table}_owner"),
        )?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = columns(&schema, table)?;
        let giver = position(&cols, "id")?;
        let quest = position(&cols, "quest")?;
        let mut result = Vec::new();
        self.scan(table, 0, 256, |row| {
            let mut fields = [None; 256];
            fill(&row, &mut fields)?;
            result.push([uint(fields[giver])?, uint(fields[quest])?]);
            check(
                result.len() <= DECODED / 8,
                "execution.quests.relation_limit",
            )
        })?;
        Ok(result)
    }
}
