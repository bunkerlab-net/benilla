// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Player.cpp:12712-12713;
// Map.cpp:2182-2276; ScriptMgr.cpp:1774-1827.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestScriptRules {
    pub say_range: ai::Real,
    pub visibility_distance: ai::Real,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::native) struct CompletionScript {
    pub quest: u32,
    pub giver: crate::NpcId,
    pub started_ms: u64,
    pub completed: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestScriptEvent {
    pub quest: u32,
    pub giver: crate::NpcId,
    pub step: usize,
    pub effect: q::ScriptEffect,
    pub delivered: bool,
}
impl QuestProgressionGenesis {
    pub(in crate::native) fn validate_scripts(
        &self,
        state: &QuestProgressionState,
        now_ms: u64,
    ) -> Result<(), NativeError> {
        if state.completion_scripts.len() > 1024 {
            return Err(NativeError::TooLarge);
        }
        let mut seen = BTreeSet::new();
        for script in &state.completion_scripts {
            let row = self.quest(script.quest)?;
            let steps = self
                .quests
                .mechanics()
                .script(row.excluded.complete_script)
                .ok_or(NativeError::InvalidCheckpoint)?;
            if !seen.insert(script.quest)
                || script.started_ms > now_ms
                || script.completed > steps.len()
                || !state
                    .quests
                    .progress(script.quest)
                    .is_some_and(|p| p.rewarded)
                || script.giver.spawn == 0
                || !self.presentation_relations.finishers.iter().any(|r| {
                    r.quest == script.quest
                        && self.script_givers.get(&script.giver.spawn) == Some(&r.creature)
                })
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            for step in &steps[..script.completed] {
                if script
                    .started_ms
                    .checked_add(u64::from(step.delay_ms))
                    .is_none_or(|due| due > now_ms)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
            if steps[script.completed..].iter().any(|step| {
                script
                    .started_ms
                    .checked_add(u64::from(step.delay_ms))
                    .is_some_and(|due| due <= now_ms)
            }) {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        for id in self.quests.ids() {
            if self.quest(id)?.excluded.complete_script != 0
                && state.quests.progress(id).is_some_and(|p| p.rewarded)
                && !seen.contains(&id)
            {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        Ok(())
    }
}
