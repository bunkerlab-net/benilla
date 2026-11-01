// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// ReputationMgr.cpp:74-84,140-150,197-229,243-263; DBCStructure.h:359-386,401-410.
use super::*;

pub const MAX_INITIAL_FACTION_ROWS: usize = 4096;

/// Complete validated Faction numeric projection, plus selected template references.
/// None is missing full-table coverage, not an empty table or an admission token.
#[derive(Clone, Debug)]
pub struct InitialFactionCapture {
    pub factions: Option<Vec<[u32; 19]>>,
    pub templates: Vec<[u32; 14]>,
    pub required_templates: Vec<u32>,
    pub required_factions: Vec<u32>,
    pub race: u8,
    pub class: u8,
}

/// Independently resolved SQL policies; DBC initialization cannot prove their absence.
#[derive(Clone, Debug)]
pub struct FactionRewardPolicy {
    pub faction: u32,
    pub reward_rates: Resolved<RewardRates>,
    pub spillover: Resolved<[Spillover; 4]>,
}

impl InitialFactionCapture {
    pub fn identity(&self) -> Result<Identity> {
        if !(1..=8).contains(&self.race) || ![1, 2, 3, 4, 5, 7, 8, 9, 11].contains(&self.class) {
            return Err(Error::Invalid("initial_faction_identity"));
        }
        Ok(Identity {
            race_mask: 1 << (self.race - 1),
            class_mask: 1 << (self.class - 1),
        })
    }

    pub fn compile(&self) -> Result<Vec<Faction>> {
        let identity = self.identity()?;
        let source = self
            .factions
            .as_ref()
            .ok_or(Error::Unsupported("initial_faction_table_missing"))?;
        if [
            source.len(),
            self.templates.len(),
            self.required_templates.len(),
            self.required_factions.len(),
        ]
        .into_iter()
        .any(|n| n > MAX_INITIAL_FACTION_ROWS)
        {
            return Err(Error::Invalid("initial_faction_row_limit"));
        }
        let mut rows = BTreeMap::new();
        let mut indices = BTreeSet::new();
        for words in source {
            let faction = Faction {
                id: words[0],
                reputation_list_id: words[1] as i32,
                race_masks: words[2..6].try_into().unwrap(),
                class_masks: words[6..10].try_into().unwrap(),
                base_values: std::array::from_fn(|i| words[10 + i] as i32),
                flags: words[14..18].try_into().unwrap(),
                team: words[18],
            };
            if faction.id == 0 || !(-1..64).contains(&faction.reputation_list_id) {
                return Err(Error::Invalid("initial_faction_index_or_id"));
            }
            // ReputationMgr.h:29-35 defines all seven source bits, including RIVAL.
            if faction.flags.iter().any(|flags| flags & !0x7f != 0) {
                return Err(Error::Unsupported("initial_faction_flag_bits"));
            }
            // Trailing zero slots are DBC padding; first-match selection still includes slot zero.
            let last = (0..4)
                .rfind(|&i| {
                    faction.race_masks[i] != 0
                        || faction.class_masks[i] != 0
                        || faction.base_values[i] != 0
                        || faction.flags[i] != 0
                })
                .unwrap_or(0);
            let mut matched = None;
            for i in 0..=last {
                if (faction.race_masks[i] == 0 || faction.race_masks[i] & identity.race_mask != 0)
                    && (faction.class_masks[i] == 0
                        || faction.class_masks[i] & identity.class_mask != 0)
                {
                    let value = (faction.base_values[i], faction.flags[i]);
                    if matched.is_some_and(|prior| prior != value) {
                        return Err(Error::Unsupported("initial_faction_conflicting_slots"));
                    }
                    matched = Some(value);
                }
            }
            if faction.reputation_list_id >= 0 && !indices.insert(faction.reputation_list_id) {
                return Err(Error::Unsupported("initial_faction_duplicate_index"));
            }
            if rows.insert(faction.id, faction).is_some() {
                return Err(Error::Invalid("initial_faction_duplicate_row"));
            }
        }
        let mut templates = BTreeMap::new();
        for words in &self.templates {
            if words[0] == 0 || templates.insert(words[0], words).is_some() {
                return Err(Error::Invalid("initial_faction_duplicate_template"));
            }
            for id in std::iter::once(words[1])
                .chain(words[6..14].iter().copied())
                .filter(|id| *id != 0)
            {
                if !rows.contains_key(&id) {
                    return Err(Error::Unresolved(id));
                }
            }
        }
        for id in &self.required_templates {
            if !templates.contains_key(id) {
                return Err(Error::Invalid("initial_faction_template_missing"));
            }
        }
        for id in &self.required_factions {
            if !rows.contains_key(id) {
                return Err(Error::Unresolved(*id));
            }
        }
        Ok(rows.into_values().collect())
    }
}

impl Catalog {
    /// Compiles DBC bases/flags into the existing catalog without inventing reward policies.
    pub fn from_initial_factions(
        capture: &InitialFactionCapture,
        policies: Vec<FactionRewardPolicy>,
        config: Config,
        audit: Audit,
        auras: Vec<GeneralReputationAura>,
    ) -> Result<Self> {
        if policies.len() > MAX_INITIAL_FACTION_ROWS {
            return Err(Error::Invalid("initial_faction_row_limit"));
        }
        let mut rewards = BTreeMap::new();
        for policy in policies {
            let id = policy.faction;
            if rewards.insert(id, policy).is_some() {
                return Err(Error::Invalid("initial_faction_duplicate_policy"));
            }
        }
        let rows = capture
            .compile()?
            .into_iter()
            .map(|faction| {
                let policy = rewards
                    .remove(&faction.id)
                    .ok_or(Error::Unresolved(faction.id))?;
                Ok(Row {
                    faction,
                    reward_rates: policy.reward_rates,
                    spillover: policy.spillover,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if !rewards.is_empty() {
            return Err(Error::Invalid("initial_faction_extra_policy"));
        }
        Self::new_with_general_reputation_auras(rows, vec![], config, audit, auras)
    }
}
