// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::source_rows::{QuestRow, RowDelete, RowInsert, RowOperation, RowUpdate};
use super::*;

// QuestDef.h:358-373. A default entry is NEW even if its status is NONE.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum QuestSaveState {
    Unchanged,
    #[allow(dead_code, reason = "Player.cpp:13340 quest mutation is not yet bound")]
    Changed,
    #[default]
    New,
}

#[derive(Clone, Copy, Default)]
pub(super) struct QuestStatusData {
    pub(super) status: u8,
    pub(super) rewarded: bool,
    pub(super) explored: bool,
    pub(super) timer_ms: u32,
    pub(super) state: QuestSaveState,
    pub(super) mob: [u32; 4],
    pub(super) item: [u32; 4],
}

impl SourcePlayer {
    // Player.cpp:16105-16165 iterates the quest status map in quest ID order.
    pub(super) fn save_quest_status(
        &self,
        templates: &SourceQuestMap,
        calendar: Calendar,
        guid: u32,
        observe: &mut impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        for (&quest, status) in &self.quest_status {
            use QuestSaveState::{Changed, New, Unchanged};
            let (missing_template, timer_overflow) = match status.state {
                Unchanged => continue,
                New => (
                    "execution.player_save.Player_16120.quest_template_null_undefined",
                    "execution.player_save.Player_16131.quest_timer_add_undefined",
                ),
                Changed => (
                    "execution.player_save.Player_16142.quest_template_null_undefined",
                    "execution.player_save.Player_16151.quest_timer_add_undefined",
                ),
            };
            let template = templates
                .template(quest)
                .ok_or_else(|| refuse(missing_template))?;
            // QuestDef.h:248. Skipped entries stay dirty.
            if template.method == 0 && !status.rewarded {
                continue;
            }
            let timer = calendar
                .world_seconds()?
                .checked_add(i64::from(status.timer_ms / 1000))
                .ok_or_else(|| refuse(timer_overflow))?;
            let row = QuestRow {
                guid,
                quest,
                status: status.status,
                rewarded: status.rewarded,
                explored: status.explored,
                timer_seconds: timer as u64,
                mob: status.mob,
                item: status.item,
            };
            let op = if status.state == New {
                RowOperation::Insert(RowInsert::Quest(row))
            } else {
                RowOperation::Update(RowUpdate::Quest(row))
            };
            observe(SourcePlayerSaveObservation::Row(op))?;
        }
        Ok(())
    }

    // Player.cpp:16164. Skipped autocomplete entries stay dirty.
    pub(super) fn acknowledge_quest_status(&mut self, written: Vec<u32>) {
        for quest in written {
            if let Some(status) = self.quest_status.get_mut(&quest) {
                status.state = QuestSaveState::Unchanged;
            }
        }
    }

    // Player.cpp:16168-16185 saves the whole set only if dirty and nonempty.
    pub(super) fn save_weekly_quest_status(
        &self,
        guid: u32,
        observe: &mut impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        if !self.weekly_quests_changed || self.weekly_quests.is_empty() {
            return Ok(());
        }
        observe(SourcePlayerSaveObservation::Row(RowOperation::Delete(
            RowDelete::WeeklyQuestsByGuid(guid),
        )))?;
        for &quest in &self.weekly_quests {
            observe(SourcePlayerSaveObservation::Row(RowOperation::Insert(
                RowInsert::WeeklyQuest { guid, quest },
            )))?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "player_save_progress_tests.rs"]
mod tests;
