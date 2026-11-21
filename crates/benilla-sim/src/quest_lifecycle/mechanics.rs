// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, ScriptMgr.cpp:117-159,1774-1827;
// Player.cpp:11516-11781,11974-12003; Conditions.cpp:178-180,220-226.
//! Checked completion-script and ordinary quest-gossip projections.
use super::{Error, QuestView, Status};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRow {
    pub id: u32,
    pub delay: u32,
    pub priority: u32,
    /// Distinguishes equal occurrences; never orders different script operands.
    pub position: u32,
    pub command: u32,
    pub datalong: [u32; 3],
    pub buddy_entry: u32,
    pub search_radius: u32,
    pub data_flags: u32,
    pub dataint: [i32; 4],
    pub float_bits: [u32; 6],
    pub condition: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ScriptEffect {
    Talk { broadcast: u32 },
    Emote { emote: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptStep {
    pub delay_ms: u32,
    pub effect: ScriptEffect,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GossipMenuRow {
    pub entry: u32,
    pub text: u32,
    pub script: u32,
    pub condition: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GossipOptionRow {
    pub menu: u32,
    pub id: u32,
    pub option: u32,
    pub npc_flags: u32,
    pub condition: u32,
    pub icon: u32,
    pub text: String,
    pub broadcast: u32,
    pub action_menu: i32,
    pub action_poi: u32,
    pub action_script: u32,
    pub box_coded: bool,
    pub box_money: u32,
    pub box_text: String,
    pub box_broadcast: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GossipConditionRow {
    pub id: u32,
    pub kind: i32,
    pub values: [u32; 4],
    pub flags: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptBroadcast {
    pub id: u32,
    pub chat_type: i32,
    pub language: i32,
    pub sounds: [i32; 2],
    pub emotes: [i32; 3],
    pub delays: [i32; 3],
    pub condition: i32,
    pub emotes_id: i32,
    pub flags: i32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MechanicsInput {
    pub script_keys: Vec<u32>,
    pub scripts: Vec<ScriptRow>,
    pub broadcasts: Vec<ScriptBroadcast>,
    pub emotes: Vec<(u32, u32)>,
    /// Complete effective entry/default-menu/NPC-flags selection.
    pub givers: Vec<(u32, u32, u32)>,
    pub menus: Vec<GossipMenuRow>,
    /// Includes menu zero fallback, even when it has no rows.
    pub options: Vec<GossipOptionRow>,
    pub conditions: Vec<GossipConditionRow>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mechanics {
    scripts: BTreeMap<u32, Vec<ScriptStep>>,
    broadcasts: BTreeMap<u32, ScriptBroadcast>,
    givers: BTreeMap<u32, (u32, u32)>,
    menus: Vec<GossipMenuRow>,
    options: Vec<GossipOptionRow>,
    conditions: BTreeMap<u32, GossipConditionRow>,
}
fn refusal(field: &'static str, target: i64, reason: &'static str) -> Error {
    Error::Relationship {
        quest: 0,
        field,
        target,
        reason,
    }
}
impl Mechanics {
    pub fn compile(input: MechanicsInput) -> Result<Self, Error> {
        if input.scripts.len() > 4096
            || input.menus.len() > 4096
            || input.options.len() > 4096
            || input.conditions.len() > 4096
            || input.givers.len() > 4096
            || input.script_keys.len() > 1024
            || input.broadcasts.len() > 4096
            || input.emotes.len() > 4096
        {
            return Err(refusal("quest_mechanics", 0, "row_limit"));
        }
        let mut out = Self::default();
        for row in input.broadcasts {
            if row.id == 0
                || row.chat_type != 0
                || row.language != 0
                || row.sounds != [0; 2]
                || row.emotes != [0; 3]
                || row.delays != [0; 3]
            {
                return Err(refusal(
                    "broadcast_text",
                    i64::from(row.id),
                    "unsupported_speech_effects",
                ));
            }
            let id = row.id;
            if out.broadcasts.insert(id, row).is_some() {
                return Err(refusal(
                    "broadcast_text.Id",
                    i64::from(id),
                    "conflicting_rows",
                ));
            }
        }
        let mut emotes = BTreeMap::new();
        for (id, kind) in input.emotes {
            if emotes.insert(id, kind).is_some() {
                return Err(refusal("Emotes", i64::from(id), "conflicting_rows"));
            }
        }
        for id in input.script_keys {
            if id == 0 || out.scripts.insert(id, Vec::new()).is_some() {
                return Err(refusal(
                    "quest_end_scripts.id",
                    i64::from(id),
                    "conflicting_rows",
                ));
            }
        }
        let mut scripts = input.scripts;
        scripts.sort_by(crate::canonical_order::compare_script);
        let mut positions = BTreeSet::new();
        for row in scripts {
            if !positions.insert((row.id, row.position)) {
                return Err(refusal(
                    "quest_end_scripts.position",
                    i64::from(row.id),
                    "conflicting_rows",
                ));
            }
            let steps = out.scripts.get_mut(&row.id).ok_or(refusal(
                "quest_end_scripts.id",
                i64::from(row.id),
                "unselected_key",
            ))?;
            if row.command > 1 {
                return Err(refusal(
                    "quest_end_scripts.command",
                    i64::from(row.command),
                    "unsupported_command",
                ));
            }
            if row.delay == 0 {
                return Err(refusal(
                    "quest_end_scripts.delay",
                    0,
                    "immediate_script_not_supported",
                ));
            }
            for (field, value) in [
                ("quest_end_scripts.buddy_entry", row.buddy_entry),
                ("quest_end_scripts.search_radius", row.search_radius),
                ("quest_end_scripts.data_flags", row.data_flags),
                ("quest_end_scripts.condition_id", row.condition),
            ] {
                if value != 0 {
                    return Err(refusal(field, i64::from(value), "unsupported_path"));
                }
            }
            if row.float_bits.iter().any(|&v| v != 0)
                || row.datalong[1..] != [0, 0]
                || row.dataint[1..] != [0, 0, 0]
            {
                return Err(refusal(
                    "quest_end_scripts.parameters",
                    i64::from(row.id),
                    "unsupported_path",
                ));
            }
            let effect = if row.command == 0 {
                if row.datalong[0] != 0 || row.dataint[0] <= 0 {
                    return Err(refusal(
                        "quest_end_scripts.dataint",
                        i64::from(row.dataint[0]),
                        "unsupported_talk_shape",
                    ));
                }
                let broadcast = row.dataint[0] as u32;
                if !out.broadcasts.contains_key(&broadcast) {
                    return Err(refusal(
                        "broadcast_text",
                        i64::from(broadcast),
                        "unknown_lookup",
                    ));
                }
                ScriptEffect::Talk { broadcast }
            } else {
                let emote = row.datalong[0];
                if row.dataint[0] != 0 {
                    return Err(refusal(
                        "quest_end_scripts.dataint",
                        i64::from(row.dataint[0]),
                        "unsupported_random_emote",
                    ));
                }
                let kind = emotes.get(&emote).ok_or(refusal(
                    "Emotes",
                    i64::from(emote),
                    "unknown_lookup",
                ))?;
                if emote == 0 || *kind != 0 {
                    return Err(refusal(
                        "Emotes.EmoteType",
                        i64::from(*kind),
                        "state_emote_not_supported",
                    ));
                }
                ScriptEffect::Emote { emote }
            };
            steps.push(ScriptStep {
                delay_ms: row.delay,
                effect,
            });
        }
        for (&id, steps) in &out.scripts {
            if steps.is_empty() {
                return Err(refusal(
                    "quest_end_scripts.id",
                    i64::from(id),
                    "missing_script",
                ));
            }
        }
        for row in input.conditions {
            if row.id == 0
                || !matches!(row.kind, 0 | 8 | 9 | 22)
                || row.flags & !1 != 0
                || row.values[2..] != [0, 0]
                || (row.kind != 9 && row.values[1] != 0)
                || row.values[1] > 2
                || (row.kind == 0 && row.values[0] != 0)
            {
                return Err(refusal(
                    "conditions.type",
                    i64::from(row.kind),
                    "unsupported_condition",
                ));
            }
            let id = row.id;
            if out.conditions.insert(id, row).is_some() {
                return Err(refusal(
                    "conditions.condition_entry",
                    i64::from(id),
                    "conflicting_rows",
                ));
            }
        }
        for (entry, menu, flags) in input.givers {
            if menu == 0 && flags & 1 != 0 {
                return Err(refusal(
                    "npc_gossip",
                    i64::from(entry),
                    "guid_gossip_text_not_supported",
                ));
            }
            if entry == 0
                || menu > u32::from(u16::MAX)
                || out.givers.insert(entry, (menu, flags)).is_some()
            {
                return Err(refusal(
                    "gossip_giver",
                    i64::from(entry),
                    "conflicting_rows",
                ));
            }
        }
        let keys: BTreeSet<_> = out
            .givers
            .values()
            .map(|(menu, _)| *menu)
            .chain([0])
            .collect();
        let mut menu_conditions = BTreeSet::new();
        for menu in &input.menus {
            if !menu_conditions.insert((menu.entry, menu.condition)) {
                return Err(refusal(
                    "gossip_menu.condition_id",
                    i64::from(menu.condition),
                    "ambiguous_loaded_order",
                ));
            }
            if !keys.contains(&menu.entry) || menu.script != 0 || menu.text == 0 {
                return Err(refusal(
                    "gossip_menu",
                    i64::from(menu.entry),
                    "unsupported_or_unselected_menu",
                ));
            }
            out.check_condition(menu.condition)?;
        }
        let mut options = BTreeSet::new();
        for option in &input.options {
            if !keys.contains(&option.menu) || !options.insert((option.menu, option.id)) {
                return Err(refusal(
                    "gossip_menu_option",
                    i64::from(option.id),
                    "conflicting_or_unselected_rows",
                ));
            }
            out.check_condition(option.condition)?;
            // This port exposes quest menus, not vendor/trainer/POI or bot services.
            // Nonmatching default options are inert after the native NPC-flag gate.
            let reached = out.givers.values().any(|(menu, flags)| {
                (option.menu == *menu
                    || (option.menu == 0 && !input.options.iter().any(|r| r.menu == *menu)))
                    && option.npc_flags & flags != 0
            });
            if reached && !matches!(option.option, 2 | 99) {
                return Err(refusal(
                    "gossip_menu_option.option_id",
                    i64::from(option.option),
                    "unsupported_option",
                ));
            }
            if option.condition != 0
                && option.option == 2
                && out.givers.values().any(|(menu, flags)| {
                    flags & 1 == 0
                        && (option.menu == *menu
                            || (option.menu == 0 && !input.options.iter().any(|r| r.menu == *menu)))
                })
            {
                return Err(refusal(
                    "gossip_menu_option.condition_id",
                    i64::from(option.condition),
                    "conditional_direct_quest_menu_not_supported",
                ));
            }
            if reached
                && (option.action_menu != 0
                    || option.action_poi != 0
                    || option.action_script != 0
                    || option.box_coded
                    || option.box_money != 0
                    || !option.box_text.is_empty()
                    || option.box_broadcast != 0)
            {
                return Err(refusal(
                    "gossip_menu_option.action",
                    i64::from(option.id),
                    "unsupported_path",
                ));
            }
        }
        for &(menu, _) in out.givers.values() {
            if menu != 0 && !input.menus.iter().any(|r| r.entry == menu) {
                return Err(refusal("gossip_menu", i64::from(menu), "unknown_lookup"));
            }
            if menu != 0
                && !input
                    .menus
                    .iter()
                    .any(|r| r.entry == menu && r.condition == 0)
            {
                return Err(refusal(
                    "gossip_menu",
                    i64::from(menu),
                    "unconditional_text_required",
                ));
            }
        }
        out.menus = input.menus;
        out.options = input.options;
        Ok(out)
    }
    fn check_condition(&self, id: u32) -> Result<(), Error> {
        if id != 0 && !self.conditions.contains_key(&id) {
            return Err(refusal(
                "conditions.condition_entry",
                i64::from(id),
                "unknown_lookup",
            ));
        }
        Ok(())
    }
    fn condition(&self, id: u32, view: &QuestView) -> bool {
        if id == 0 {
            return true;
        }
        let row = &self.conditions[&id];
        let progress = view.records.get(&row.values[0]);
        let current = progress.is_some_and(|p| !p.rewarded && p.status != Status::None);
        let rewarded = progress.is_some_and(|p| p.rewarded && p.status != Status::None);
        let result = match row.kind {
            0 => true,
            8 => rewarded,
            9 => {
                current
                    && match row.values[1] {
                        0 => true,
                        1 => progress.is_some_and(|p| p.status == Status::Incomplete),
                        _ => progress.is_some_and(|p| p.status == Status::Complete),
                    }
            }
            22 => !current && !rewarded,
            _ => unreachable!("checked condition"),
        };
        result ^ (row.flags & 1 != 0)
    }
    pub fn script(&self, id: u32) -> Option<&[ScriptStep]> {
        self.scripts.get(&id).map(Vec::as_slice)
    }
    pub fn giver(&self, entry: u32) -> Option<(u32, u32)> {
        self.givers.get(&entry).copied()
    }
    pub fn effective_option_ids(&self, entry: u32) -> Vec<u32> {
        let Some(&(menu, flags)) = self.givers.get(&entry) else {
            return vec![];
        };
        let selected = if self.options.iter().any(|r| r.menu == menu) {
            menu
        } else {
            0
        };
        self.options
            .iter()
            .filter(|r| r.menu == selected && r.npc_flags & flags != 0)
            .map(|r| r.id)
            .collect()
    }
    /// Native default-menu fallback and conditional questgiver-option suppression.
    /// BOT is always hidden in the closed native domain without playerbots.
    pub fn gossip(&self, entry: u32, view: &QuestView) -> Result<Option<(u32, bool)>, Error> {
        let Some(&(menu, flags)) = self.givers.get(&entry) else {
            return Ok(None);
        };
        if menu == 0 && flags & 1 == 0 {
            return Ok(None);
        }
        let mut selected = (0, 0xffffff);
        for row in self.menus.iter().filter(|r| r.entry == menu) {
            if (row.condition == 0 && selected.0 == 0)
                || (row.condition > selected.0 && self.condition(row.condition, view))
            {
                selected = (row.condition, row.text);
            }
        }
        let option_menu = if self.options.iter().any(|r| r.menu == menu) {
            menu
        } else {
            0
        };
        let mut quests = flags & 2 != 0;
        for option in self.options.iter().filter(|r| r.menu == option_menu) {
            // CMaNGOS evaluates conditions BEFORE the NPC-flag gate.
            if !self.condition(option.condition, view) && option.option == 2 {
                quests = false;
            }
        }
        Ok(Some((selected.1, quests)))
    }
    pub fn script_broadcast_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.broadcasts.keys().copied()
    }
    pub fn gossip_text_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.menus.iter().map(|row| row.text)
    }
    pub(super) fn validate_quests(&self, ids: &BTreeSet<u32>) -> Result<(), Error> {
        for row in self.conditions.values() {
            if row.kind != 0 && !ids.contains(&row.values[0]) {
                return Err(refusal(
                    "conditions.value1",
                    i64::from(row.values[0]),
                    "uncaptured_reference",
                ));
            }
        }
        Ok(())
    }
}
