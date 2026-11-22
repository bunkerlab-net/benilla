// SPDX-License-Identifier: GPL-3.0-or-later
//! Primary-key insertion order in the restored reference environment, not a SQL ordering guarantee.
use super::*;
use std::collections::VecDeque;

// db-snapshot.py:9; seal16-event-order-analysis.json matches all 15,636 rows, field for field, in source query order.
// [INFERENCE] Restored MyISAM scan order. EXPLAIN blocked by tool policy; this is not an engine ordering guarantee.

pub(super) struct StartupEventRows {
    rows: EventCatalogRows,
    definitions: Option<VecDeque<GameEventDefinitionRow>>,
    dates: Option<VecDeque<GameEventDateRow>>,
    members: [Option<VecDeque<GameEventMemberRow>>; 2],
    substitutions: Option<VecDeque<GameEventSubstitutionRow>>,
    quests: Option<VecDeque<(u64, u32, u16)>>,
    mails: Option<VecDeque<GameEventMailRow>>,
}
fn ordered<T, K: Ord>(
    mut next: impl FnMut() -> Result<Option<T>>,
    key: impl FnMut(&T) -> K,
) -> Result<VecDeque<T>> {
    let mut rows = Vec::new();
    while let Some(row) = next()? {
        rows.push(row);
    }
    rows.sort_unstable_by_key(key);
    Ok(rows.into())
}
impl StartupEventRows {
    pub(super) fn new(rows: EventCatalogRows) -> Self {
        Self {
            rows,
            definitions: None,
            dates: None,
            members: [None, None],
            substitutions: None,
            quests: None,
            mails: None,
        }
    }
}
impl GameEventRows for StartupEventRows {
    fn maximum(&mut self) -> Result<Option<u16>> {
        self.rows.maximum()
    }
    fn definition(&mut self) -> Result<Option<GameEventDefinitionRow>> {
        if self.definitions.is_none() {
            self.definitions = Some(ordered(|| self.rows.definition(), |r| r.id)?);
        }
        Ok(self.definitions.as_mut().unwrap().pop_front())
    }
    fn date(&mut self) -> Result<Option<GameEventDateRow>> {
        if self.dates.is_none() {
            self.dates = Some(ordered(|| self.rows.date(), |r| r.id)?);
        }
        Ok(self.dates.as_mut().unwrap().pop_front())
    }
    fn member(&mut self, kind: EventSpawnKind) -> Result<Option<GameEventMemberRow>> {
        let slot = usize::from(kind == EventSpawnKind::GameObject);
        if self.members[slot].is_none() {
            self.members[slot] = Some(ordered(|| self.rows.member(kind), |r| (r.guid, r.event))?);
        }
        Ok(self.members[slot].as_mut().unwrap().pop_front())
    }
    fn substitution(&mut self) -> Result<Option<GameEventSubstitutionRow>> {
        if self.substitutions.is_none() {
            self.substitutions = Some(ordered(
                || self.rows.substitution(),
                |r| (r.data.guid, r.event),
            )?);
        }
        Ok(self.substitutions.as_mut().unwrap().pop_front())
    }
    fn quest(&mut self) -> Result<Option<(u64, u32, u16)>> {
        if self.quests.is_none() {
            self.quests = Some(ordered(|| self.rows.quest(), |r| (r.1, r.2))?);
        }
        Ok(self.quests.as_mut().unwrap().pop_front())
    }
    fn mail(&mut self) -> Result<Option<GameEventMailRow>> {
        if self.mails.is_none() {
            self.mails = Some(ordered(
                || self.rows.mail(),
                |r| (r.event as i16, r.mail.race_mask, r.mail.quest),
            )?);
        }
        Ok(self.mails.as_mut().unwrap().pop_front())
    }
}
