// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, GPL-2.0-or-later; translation GPL-3.0-or-later.
//! Source-native reputation primitives with explicit resolved inputs. See ../../REPUTATION.md.
mod numeric;
use crate::classic_spell::SpellFloat;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const UPSTREAM_COMMIT: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
pub const POLICY_ID: &str = "cmangos-8ec338a1-reputation-v1";
pub const CAP: i32 = 42_999;
pub const BOTTOM: i32 = -42_000;
pub const VISIBLE: u32 = 1;
pub const AT_WAR: u32 = 2;
pub const HIDDEN: u32 = 4;
pub const INVISIBLE_FORCED: u32 = 8;
pub const PEACE_FORCED: u32 = 16;
pub const INACTIVE: u32 = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(&'static str),
    Unsupported(&'static str),
    Unresolved(u32),
    Numeric(&'static str),
    Callback(&'static str),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Classic reputation: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

/// An explicit source lookup result. Unlisted lookups are errors, never absence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Resolved<T> {
    Absent,
    Present(T),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    Absent,
    Present,
    Unknown,
}
/// These are supplied audit assertions, not proof of any raw database's completeness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Audit {
    pub general_reputation_auras: Evidence,
    pub faction_reputation_auras: Evidence,
    pub forced_reactions: Evidence,
    pub reputation_scripts: Evidence,
}
impl Audit {
    fn validate(&self) -> Result<()> {
        if [
            self.general_reputation_auras,
            self.faction_reputation_auras,
            self.forced_reactions,
            self.reputation_scripts,
        ]
        .into_iter()
        .any(|e| e != Evidence::Absent)
        {
            return Err(Error::Unsupported("reputation_dependency_not_absent"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub race_mask: u32,
    pub class_mask: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Faction {
    pub id: u32,
    pub reputation_list_id: i32,
    pub race_masks: [u32; 4],
    pub class_masks: [u32; 4],
    pub base_values: [i32; 4],
    pub flags: [u32; 4],
    pub team: u32,
}
impl Faction {
    /// FactionEntry::GetIndexFitTo: zero masks are wildcards, including slot zero.
    pub fn matching_slot(&self, identity: Identity) -> Option<usize> {
        (0..4).find(|&i| {
            (self.race_masks[i] == 0 || self.race_masks[i] & identity.race_mask != 0)
                && (self.class_masks[i] == 0 || self.class_masks[i] & identity.class_mask != 0)
        })
    }
    pub fn base(&self, identity: Identity) -> i32 {
        self.matching_slot(identity)
            .map_or(0, |i| self.base_values[i])
    }
    pub fn default_flags(&self, identity: Identity) -> u32 {
        self.matching_slot(identity).map_or(0, |i| self.flags[i])
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Rank {
    Hated,
    Hostile,
    Unfriendly,
    Neutral,
    Friendly,
    Honored,
    Revered,
    Exalted,
}
pub fn rank(standing: i32) -> Rank {
    match standing {
        ..=-6001 => Rank::Hated,
        -6000..=-3001 => Rank::Hostile,
        -3000..=-1 => Rank::Unfriendly,
        0..=2999 => Rank::Neutral,
        3000..=8999 => Rank::Friendly,
        9000..=20999 => Rank::Honored,
        21000..=41999 => Rank::Revered,
        _ => Rank::Exalted,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardRates {
    pub quest: SpellFloat,
    pub creature: SpellFloat,
    pub spell: SpellFloat,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spillover {
    pub faction: u32,
    pub rate: SpellFloat,
    pub maximum_rank: Rank,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub faction: Faction,
    pub reward_rates: Resolved<RewardRates>,
    pub spillover: Resolved<[Spillover; 4]>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub gain_rate: SpellFloat,
    pub low_level_kill_rate: SpellFloat,
    pub low_level_quest_rate: SpellFloat,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    rows: BTreeMap<u32, Resolved<Row>>,
    config: Config,
    audit: Audit,
}
impl Catalog {
    pub fn new(
        rows: Vec<Row>,
        absent_factions: Vec<u32>,
        config: Config,
        audit: Audit,
    ) -> Result<Self> {
        let mut resolved = BTreeMap::new();
        for row in rows {
            if resolved
                .insert(row.faction.id, Resolved::Present(row))
                .is_some()
            {
                return Err(Error::Invalid("duplicate_faction"));
            }
        }
        for id in absent_factions {
            if resolved.insert(id, Resolved::Absent).is_some() {
                return Err(Error::Invalid("duplicate_faction"));
            }
        }
        let out = Self {
            rows: resolved,
            config,
            audit,
        };
        out.validate()?;
        Ok(out)
    }
    pub fn row(&self, faction: u32) -> Result<Option<&Row>> {
        match self.rows.get(&faction) {
            Some(Resolved::Present(row)) => Ok(Some(row)),
            Some(Resolved::Absent) => Ok(None),
            None => Err(Error::Unresolved(faction)),
        }
    }
    pub fn config(&self) -> Config {
        self.config
    }
    pub fn validate(&self) -> Result<()> {
        self.audit.validate()?;
        for rate in [
            self.config.gain_rate,
            self.config.low_level_kill_rate,
            self.config.low_level_quest_rate,
        ] {
            let _ = numeric::nonnegative(rate)?;
        }
        for (&id, resolved) in &self.rows {
            let Resolved::Present(row) = resolved else {
                continue;
            };
            if id != row.faction.id || id == 0 || row.faction.reputation_list_id >= 64 {
                return Err(Error::Invalid("faction_row"));
            }
            if let Resolved::Present(rates) = row.reward_rates {
                for rate in [rates.quest, rates.creature, rates.spell] {
                    let _ = numeric::nonnegative(rate)?;
                }
            }
            if let Resolved::Present(spillovers) = row.spillover {
                for spill in spillovers {
                    let _ = numeric::finite(spill.rate)?;
                    if spill.faction != 0 {
                        self.row(spill.faction)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub fn hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(POLICY_ID.as_bytes());
        h.update([0]);
        for rate in [
            self.config.gain_rate,
            self.config.low_level_kill_rate,
            self.config.low_level_quest_rate,
        ] {
            h.update(rate.bits().to_le_bytes());
        }
        for e in [
            self.audit.general_reputation_auras,
            self.audit.faction_reputation_auras,
            self.audit.forced_reactions,
            self.audit.reputation_scripts,
        ] {
            h.update([e as u8]);
        }
        h.update((self.rows.len() as u64).to_le_bytes());
        for (&id, resolved) in &self.rows {
            h.update(id.to_le_bytes());
            let Resolved::Present(row) = resolved else {
                h.update([0]);
                continue;
            };
            h.update([1]);
            let f = &row.faction;
            h.update(f.reputation_list_id.to_le_bytes());
            for n in f.race_masks.into_iter().chain(f.class_masks).chain(f.flags) {
                h.update(n.to_le_bytes());
            }
            for n in f.base_values {
                h.update(n.to_le_bytes());
            }
            h.update(f.team.to_le_bytes());
            match row.reward_rates {
                Resolved::Absent => h.update([0]),
                Resolved::Present(rates) => {
                    h.update([1]);
                    for r in [rates.quest, rates.creature, rates.spell] {
                        h.update(r.bits().to_le_bytes());
                    }
                }
            }
            match row.spillover {
                Resolved::Absent => h.update([0]),
                Resolved::Present(spills) => {
                    h.update([1]);
                    for s in spills {
                        h.update(s.faction.to_le_bytes());
                        h.update(s.rate.bits().to_le_bytes());
                        h.update([s.maximum_rank as u8]);
                    }
                }
            }
        }
        h.finalize().into()
    }
    pub fn initialize(&self, identity: Identity) -> Result<State> {
        self.validate()?;
        let mut factions = BTreeMap::new();
        // Initialize walks Faction.dbc in ascending ID order; later duplicate list IDs win.
        for row in self.rows.values() {
            if let Resolved::Present(row) = row {
                let f = &row.faction;
                if f.reputation_list_id >= 0 {
                    factions.insert(
                        f.reputation_list_id as u32,
                        FactionState {
                            id: f.id,
                            reputation_list_id: f.reputation_list_id as u32,
                            standing: 0,
                            flags: f.default_flags(identity),
                            need_send: true,
                            need_save: true,
                        },
                    );
                }
            }
        }
        Ok(State {
            catalog_hash: self.hash(),
            identity,
            factions,
        })
    }
    pub fn calculate_gain(&self, input: GainInput) -> Result<i32> {
        self.validate()?;
        let row = self.row(input.faction)?;
        numeric::gain(self.config, row.map(|r| &r.reward_rates), input)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Kill,
    Quest,
    Spell,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GainInput {
    pub source: Source,
    pub reputation: i32,
    pub faction: u32,
    pub player_level: u8,
    pub target_level: u32,
    pub no_aura_bonus: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactionState {
    pub id: u32,
    pub reputation_list_id: u32,
    pub standing: i32,
    pub flags: u32,
    pub need_send: bool,
    pub need_save: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    catalog_hash: [u8; 32],
    identity: Identity,
    factions: BTreeMap<u32, FactionState>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Event {
    Visible {
        list_id: u32,
    },
    /// Synchronous source callback observation; host must process before later effects.
    ReputationChanged {
        faction: u32,
        snapshot: State,
    },
    StateSent {
        standings: Vec<(u32, i32)>,
    },
    InitialSent {
        factions: Vec<FactionState>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    pub state: State,
    pub events: Vec<Event>,
    pub rank_increased: bool,
}
/// Host owns quest and other mutable callback state; Clone makes failures atomic.
/// Returned nested reputation events are inserted at the exact callback position.
pub trait Host: Clone {
    fn reputation_changed(
        &mut self,
        catalog: &Catalog,
        state: &mut State,
        faction: u32,
    ) -> Result<Vec<Event>>;
    fn current_level(&self) -> u8;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostedTransition<H> {
    pub transition: Transition,
    pub host: H,
}
#[derive(Clone)]
struct ObserveOnly {
    level: u8,
}
impl Host for ObserveOnly {
    fn reputation_changed(&mut self, _: &Catalog, _: &mut State, _: u32) -> Result<Vec<Event>> {
        Ok(vec![])
    }
    fn current_level(&self) -> u8 {
        self.level
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Command {
    Set {
        faction: u32,
        standing: i32,
        incremental: bool,
        no_spillover: bool,
    },
    Visible {
        faction: u32,
    },
    AtWar {
        list_id: u32,
        on: bool,
    },
    Inactive {
        list_id: u32,
        on: bool,
    },
    SendInitial,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestReward {
    pub factions: [u32; 5],
    pub values: [i32; 5],
    pub faction_flags: u32,
    pub quest_level: i32,
}
impl State {
    pub fn identity(&self) -> Identity {
        self.identity
    }
    pub fn factions(&self) -> &BTreeMap<u32, FactionState> {
        &self.factions
    }
    pub fn validate(&self, catalog: &Catalog) -> Result<()> {
        catalog.validate()?;
        let initial = catalog.initialize(self.identity)?;
        if self.catalog_hash != catalog.hash() || self.factions.len() != initial.factions.len() {
            return Err(Error::Invalid("state_catalog"));
        }
        for (&id, state) in &self.factions {
            let expected = initial
                .factions
                .get(&id)
                .ok_or(Error::Invalid("state_faction"))?;
            if state.id != expected.id || state.reputation_list_id != id {
                return Err(Error::Invalid("state_faction"));
            }
        }
        Ok(())
    }
    pub fn base(&self, catalog: &Catalog, faction: u32) -> Result<i32> {
        self.check_catalog(catalog)?;
        Ok(catalog
            .row(faction)?
            .map_or(0, |r| r.faction.base(self.identity)))
    }
    fn check_catalog(&self, catalog: &Catalog) -> Result<()> {
        if self.catalog_hash != catalog.hash() {
            return Err(Error::Invalid("state_catalog"));
        }
        Ok(())
    }
    pub fn base_rank(&self, catalog: &Catalog, faction: u32) -> Result<Rank> {
        Ok(rank(self.base(catalog, faction)?))
    }
    pub fn current(&self, catalog: &Catalog, faction: u32) -> Result<i32> {
        self.check_catalog(catalog)?;
        let Some(row) = catalog.row(faction)? else {
            return Ok(0);
        };
        if row.faction.reputation_list_id < 0 {
            return Ok(0);
        }
        self.factions
            .get(&(row.faction.reputation_list_id as u32))
            .map_or(Ok(0), |s| {
                s.standing
                    .checked_add(row.faction.base(self.identity))
                    .ok_or(Error::Numeric("current_overflow"))
            })
    }
    pub fn rank(&self, catalog: &Catalog, faction: u32) -> Result<Rank> {
        Ok(rank(self.current(catalog, faction)?))
    }
    pub fn at_war(&self, catalog: &Catalog, faction: u32) -> Result<bool> {
        self.check_catalog(catalog)?;
        let Some(row) = catalog.row(faction)? else {
            return Ok(false);
        };
        Ok(row.faction.reputation_list_id >= 0
            && self
                .factions
                .get(&(row.faction.reputation_list_id as u32))
                .is_some_and(|s| s.flags & AT_WAR != 0))
    }
    /// Pure staging: errors leave this state untouched. Replay events before committing.
    pub fn reduce(&self, catalog: &Catalog, command: Command) -> Result<Transition> {
        self.validate(catalog)?;
        let mut next = self.clone();
        let mut events = vec![];
        let rank_increased =
            next.apply(catalog, command, &mut events, &mut ObserveOnly { level: 1 })?;
        Ok(Transition {
            state: next,
            events,
            rank_increased,
        })
    }
    /// Clone-staged synchronous host callbacks, including reentrant reputation changes.
    pub fn reduce_with_host<H: Host>(
        &self,
        catalog: &Catalog,
        command: Command,
        host: &H,
    ) -> Result<HostedTransition<H>> {
        self.validate(catalog)?;
        let mut state = self.clone();
        let mut host = host.clone();
        let mut events = vec![];
        let rank_increased = state.apply(catalog, command, &mut events, &mut host)?;
        state.validate(catalog)?;
        Ok(HostedTransition {
            transition: Transition {
                state,
                events,
                rank_increased,
            },
            host,
        })
    }
    /// Uses the host's current level before each source loop iteration. The outer
    /// quest's GiveXP must happen only after this transition and host commit.
    pub fn reward_quest_with_host<H: Host>(
        &self,
        catalog: &Catalog,
        reward: QuestReward,
        host: &H,
    ) -> Result<HostedTransition<H>> {
        self.validate(catalog)?;
        let mut state = self.clone();
        let mut host = host.clone();
        let mut events = vec![];
        let mut rank_increased = false;
        for slot in 0..5 {
            if let Some(command) = reward.command(catalog, host.current_level(), slot)? {
                rank_increased |= state.apply(catalog, command, &mut events, &mut host)?;
            }
        }
        state.validate(catalog)?;
        Ok(HostedTransition {
            transition: Transition {
                state,
                events,
                rank_increased,
            },
            host,
        })
    }
    /// Observation-only callback branch. Use reward_quest_with_host whenever an
    /// active quest callback can mutate reputation or level. The supplied level
    /// is BEFORE RewardQuest clears the slot and calls GiveXP.
    pub fn reward_quest(
        &self,
        catalog: &Catalog,
        reward: QuestReward,
        pre_xp_level: u8,
    ) -> Result<Transition> {
        self.validate(catalog)?;
        if pre_xp_level == 0 {
            return Err(Error::Invalid("player_level"));
        }
        let mut next = self.clone();
        let mut events = vec![];
        let mut increased = false;
        for i in 0..5 {
            if let Some(command) = reward.command(catalog, pre_xp_level, i)? {
                increased |= next.apply(
                    catalog,
                    command,
                    &mut events,
                    &mut ObserveOnly {
                        level: pre_xp_level,
                    },
                )?;
            }
        }
        Ok(Transition {
            state: next,
            events,
            rank_increased: increased,
        })
    }
    fn visible(&mut self, id: u32, events: &mut Vec<Event>) {
        if let Some(s) = self.factions.get_mut(&id) {
            if s.flags & (HIDDEN | INVISIBLE_FORCED | VISIBLE) == 0 {
                s.flags |= VISIBLE;
                s.need_send = true;
                s.need_save = true;
                events.push(Event::Visible { list_id: id });
            }
        }
    }
    fn set_at_war(&mut self, id: u32, on: bool, wrapper: bool) {
        if let Some(s) = self.factions.get_mut(&id) {
            if wrapper && s.flags & (HIDDEN | INVISIBLE_FORCED) != 0 {
                return;
            }
            // Deliberately uses Standing, not Standing + BaseRep.
            if on && s.flags & PEACE_FORCED != 0 && rank(s.standing) > Rank::Hated {
                return;
            }
            if on && s.flags & AT_WAR != 0 {
                return;
            }
            if on {
                s.flags |= AT_WAR;
            } else {
                s.flags &= !AT_WAR;
            }
            s.need_send = true;
            s.need_save = true;
        }
    }
    fn one<H: Host>(
        &mut self,
        catalog: &Catalog,
        faction: u32,
        mut standing: i32,
        incremental: bool,
        events: &mut Vec<Event>,
        host: &mut H,
    ) -> Result<bool> {
        let Some(row) = catalog.row(faction)? else {
            return Ok(false);
        };
        let id = row.faction.reputation_list_id as u32;
        let Some(s) = self.factions.get(&id) else {
            return Ok(false);
        };
        let base = row.faction.base(self.identity);
        let current = s
            .standing
            .checked_add(base)
            .ok_or(Error::Numeric("current_overflow"))?;
        if incremental {
            standing = standing
                .checked_add(current)
                .ok_or(Error::Numeric("standing_overflow"))?;
        }
        standing = standing.clamp(BOTTOM, CAP);
        let old = rank(current);
        let new = rank(standing);
        let delta = standing
            .checked_sub(base)
            .ok_or(Error::Numeric("delta_overflow"))?;
        let s = self.factions.get_mut(&id).expect("state existed");
        s.standing = delta;
        s.need_send = true;
        s.need_save = true;
        self.visible(id, events);
        if new != old {
            if new < Rank::Unfriendly && new < old && old > Rank::Hostile {
                self.set_at_war(id, true, false);
            } else if new > Rank::Unfriendly && new > old && old < Rank::Neutral {
                self.set_at_war(id, false, true);
            }
        }
        events.push(Event::ReputationChanged {
            faction,
            snapshot: self.clone(),
        });
        events.extend(host.reputation_changed(catalog, self, faction)?);
        self.validate(catalog)?;
        Ok(new > old)
    }
    fn apply<H: Host>(
        &mut self,
        catalog: &Catalog,
        command: Command,
        events: &mut Vec<Event>,
        host: &mut H,
    ) -> Result<bool> {
        match command {
            Command::Set {
                faction,
                standing,
                incremental,
                no_spillover,
            } => {
                let Some(row) = catalog.row(faction)? else {
                    return Ok(false);
                };
                if !no_spillover {
                    if let Resolved::Present(spills) = row.spillover {
                        for s in spills {
                            if s.faction != 0 && self.rank(catalog, s.faction)? <= s.maximum_rank {
                                let amount = numeric::spillover(standing, s.rate)?;
                                self.one(catalog, s.faction, amount, incremental, events, host)?;
                            }
                        }
                    }
                }
                let id = row.faction.reputation_list_id as u32;
                if self.factions.contains_key(&id) {
                    let increased =
                        self.one(catalog, faction, standing, incremental, events, host)?;
                    let mut standings = vec![(id, self.factions[&id].standing)];
                    for (&other, s) in &mut self.factions {
                        if s.need_send {
                            s.need_send = false;
                            if other != id {
                                standings.push((other, s.standing));
                            }
                        }
                    }
                    events.push(Event::StateSent { standings });
                    return Ok(increased);
                }
            }
            Command::Visible { faction } => {
                if let Some(row) = catalog.row(faction)? {
                    if row.faction.reputation_list_id >= 0 {
                        self.visible(row.faction.reputation_list_id as u32, events);
                    }
                }
            }
            Command::AtWar { list_id, on } => self.set_at_war(list_id, on, true),
            Command::Inactive { list_id, on } => {
                if let Some(s) = self.factions.get_mut(&list_id) {
                    if on && (s.flags & (HIDDEN | INVISIBLE_FORCED) != 0 || s.flags & VISIBLE == 0)
                    {
                        return Ok(false);
                    }
                    if (s.flags & INACTIVE != 0) != on {
                        if on {
                            s.flags |= INACTIVE;
                        } else {
                            s.flags &= !INACTIVE;
                        }
                        s.need_send = true;
                        s.need_save = true;
                    }
                }
            }
            Command::SendInitial => {
                events.push(Event::InitialSent {
                    factions: self.factions.values().cloned().collect(),
                });
                for s in self.factions.values_mut() {
                    s.need_send = false;
                }
            }
        }
        Ok(false)
    }
}
impl QuestReward {
    /// One source loop iteration, for use with the synchronous host reducer.
    pub fn command(
        &self,
        catalog: &Catalog,
        current_level: u8,
        slot: usize,
    ) -> Result<Option<Command>> {
        if slot >= 5 {
            return Err(Error::Invalid("quest_reward_slot"));
        }
        let faction = self.factions[slot];
        let reputation = self.values[slot];
        if faction == 0 || reputation == 0 {
            return Ok(None);
        }
        let target_level = if self.quest_level > 0 {
            self.quest_level as u32
        } else {
            u32::from(current_level)
        };
        let standing = catalog.calculate_gain(GainInput {
            source: Source::Quest,
            reputation,
            faction,
            player_level: current_level,
            target_level,
            no_aura_bonus: false,
        })?;
        Ok(catalog.row(faction)?.map(|_| Command::Set {
            faction,
            standing,
            incremental: true,
            no_spillover: self.faction_flags & (1 << slot) != 0,
        }))
    }
}
