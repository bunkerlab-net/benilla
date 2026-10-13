// SPDX-License-Identifier: GPL-3.0-or-later
//! Original, bounded solo-world gameplay. CMaNGOS parity is not claimed.
//! There is no networking, host clock, floating-point authority, credential storage,
//! arbitrary reward command, script execution, or divergent-world merge.
#![forbid(unsafe_code)]
mod engine;
mod model;
mod persistence;
mod rules;
use benilla_world_data::WorldData;
pub use model::*;
pub use persistence::{Journal, Snapshot, TickRecord};
pub use rules::{FixedLoot, Rules};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

pub const FORMAT_VERSION: u32 = 2;
pub const MAX_PLAYERS: usize = 16;
pub const MAX_NPCS: usize = 4096;
pub const MAX_ABILITIES: usize = 64;
pub const MAX_QUESTS: usize = 64;
pub const MAX_INVENTORY_SLOTS: usize = 64;
pub const MAX_INPUT_BYTES: usize = 2048;
pub const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_JOURNAL_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_JOURNAL_TICKS: usize = 10_000;
pub const MAX_PROGRESSION: u64 = 1_000_000_000_000;
pub const MAX_TICK: u64 = u64::MAX - 1_000_000_000;
pub const MAX_COORD: i32 = 100_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidRules,
    UnsupportedContent,
    InvalidWorld,
    InvalidPlayers,
    TooManyNpcs,
    TooLarge,
    Json,
    Io,
    NonCanonical,
    Incompatible,
    InvalidSnapshot,
    WrongWorld,
    WrongTick,
    UnknownPlayer,
    OwnerMismatch,
    InvalidSequence,
    DuplicateInput,
    TooManyInputs,
    JournalFull,
    StaleCheckpoint,
    ReplayMismatch,
    LimitReached,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "world runtime: {self:?}")
    }
}
impl std::error::Error for Error {}

pub(crate) fn digest(domain: &[u8], value: &impl Serialize) -> Digest {
    let mut hash = Sha256::new();
    hash.update(domain);
    // All internal types are integer-only structs/ordered maps and serialize infallibly.
    hash.update(serde_json::to_vec(value).expect("runtime model serialization"));
    hash.finalize().into()
}
impl Input {
    /// Syntax boundary only; World::step also checks transport ownership and state.
    pub fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(Error::TooLarge);
        }
        serde_json::from_slice(bytes).map_err(|_| Error::Json)
    }
}

pub struct World {
    data: WorldData,
    rules: Rules,
    state: State,
    journal: Journal,
    journal_bytes: usize,
    journal_tip: Digest,
}
impl World {
    /// One player starts immediately. Additional genesis players are useful for deterministic
    /// tests; admitting remote peers during play requires a separate membership protocol.
    pub fn new(
        data: WorldData,
        rules: Rules,
        world: WorldId,
        seed: u64,
        mut players: Vec<PlayerId>,
    ) -> Result<Self, Error> {
        rules.validate(&data)?;
        if world == [0; 16] {
            return Err(Error::InvalidWorld);
        }
        if players.is_empty() || players.len() > MAX_PLAYERS {
            return Err(Error::InvalidPlayers);
        }
        players.sort();
        let mut owners = BTreeSet::new();
        if players
            .iter()
            .any(|id| id.owner == [0; 16] || id.character == [0; 16] || !owners.insert(id.owner))
        {
            return Err(Error::InvalidPlayers);
        }
        if data.export().spawns.len() > MAX_NPCS {
            return Err(Error::TooManyNpcs);
        }
        let state = State {
            world,
            seed,
            revision: 0,
            tick: 0,
            players: players
                .into_iter()
                .map(|id| Player {
                    id,
                    position: rules.start,
                    health: rules.player_health,
                    mana: rules.player_mana,
                    xp: 0,
                    level: 1,
                    money: 0,
                    next_sequence: 0,
                    melee_ready_at: 0,
                    global_ready_at: 0,
                    abilities: rules.starting_abilities.iter().copied().collect(),
                    cooldowns: BTreeMap::new(),
                    casting: None,
                    quests: BTreeMap::new(),
                    inventory: Vec::new(),
                    next_item_instance: 1,
                    active_loot: None,
                    revive_at: None,
                })
                .collect(),
            npcs: data
                .export()
                .spawns
                .iter()
                .map(|spawn| Npc {
                    id: NpcId {
                        spawn: spawn.spawn_id,
                        generation: 0,
                    },
                    health: data.creature(spawn.creature_entry).unwrap().max_health,
                    tagged_by: None,
                    melee_ready_at: 0,
                    respawn_at: None,
                    looted: false,
                    loot_money: 0,
                    loot_items: Vec::new(),
                })
                .collect(),
        };
        Ok(Self::from_state(data, rules, state, [0; 32]))
    }
    /// Exact committed inputs/results for the latest tick; cleared only at a committed checkpoint.
    pub fn last_tick(&self) -> Option<&TickRecord> {
        self.journal.records.last()
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn data(&self) -> &WorldData {
        &self.data
    }
    pub fn rules(&self) -> &Rules {
        &self.rules
    }
    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.state
            .players
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .map(|i| &self.state.players[i])
    }
    pub fn npc(&self, spawn: u64) -> Option<&Npc> {
        self.state
            .npcs
            .binary_search_by_key(&spawn, |n| n.id.spawn)
            .ok()
            .map(|i| &self.state.npcs[i])
    }
    pub fn state_digest(&self) -> Digest {
        digest(b"benilla-world-state-v2\0", &self.state)
    }
    pub fn join_boundary(&self) -> JoinBoundary {
        JoinBoundary {
            world: self.state.world,
            revision: self.state.revision,
            tick: self.state.tick,
            state_digest: self.state_digest(),
            content_digest: self.data.content_digest(),
            rules_digest: self.rules.digest(),
        }
    }
    /// Commit exactly one fixed tick from a complete batch. Arrival order is ignored.
    /// At most one command per active owner per tick bounds movement and work. Empty ticks
    /// advance casts/respawns/combat in solo mode, with no quorum or peer configuration.
    /// Envelope failures reject the entire tick without changes; gameplay rejections consume
    /// the authenticated sequence and are recorded as deterministic outcomes.
    pub fn step(&mut self, tick: u64, mut inputs: Vec<OwnedInput>) -> Result<TickResult, Error> {
        if self.state.tick >= MAX_TICK || self.state.revision >= MAX_TICK {
            return Err(Error::LimitReached);
        }
        if tick != self.state.tick + 1 {
            return Err(Error::WrongTick);
        }
        if inputs.len() > MAX_PLAYERS {
            return Err(Error::TooManyInputs);
        }
        if self.journal.records.len() >= MAX_JOURNAL_TICKS {
            return Err(Error::JournalFull);
        }
        inputs.sort_by_key(|x| {
            (
                x.input.tick,
                x.input.player.owner,
                x.input.sequence,
                x.input.player.character,
            )
        });
        let mut seen = BTreeSet::new();
        for owned in &inputs {
            let input = &owned.input;
            if input.world != self.state.world {
                return Err(Error::WrongWorld);
            }
            if input.tick != tick {
                return Err(Error::WrongTick);
            }
            if input.player.owner != owned.authenticated_owner {
                return Err(Error::OwnerMismatch);
            }
            let player = self.player(input.player).ok_or(Error::UnknownPlayer)?;
            if !seen.insert(input.player.owner) {
                return Err(Error::DuplicateInput);
            }
            if input.sequence != player.next_sequence || input.sequence == u64::MAX {
                return Err(Error::InvalidSequence);
            }
        }
        let commands: Vec<Input> = inputs.into_iter().map(|x| x.input).collect();
        let before_digest = self.state_digest();
        let mut candidate = self.state.clone();
        let result = engine::advance(&mut candidate, &self.data, &self.rules, tick, &commands)?;
        let after_digest = digest(b"benilla-world-state-v2\0", &candidate);
        let record = TickRecord {
            tick,
            before_digest,
            after_digest,
            previous_digest: self.journal_tip,
            inputs: commands,
            result: result.clone(),
        };
        let size = serde_json::to_vec(&record).map_err(|_| Error::Json)?.len() + 1;
        if self.journal_bytes + size > MAX_JOURNAL_BYTES - 4096 {
            return Err(Error::JournalFull);
        }
        self.journal_tip = digest(b"benilla-world-record-v2\0", &record);
        self.journal.records.push(record);
        self.journal_bytes += size;
        self.state = candidate;
        self.journal.final_boundary = self.join_boundary();
        self.journal.final_journal_tip = self.journal_tip;
        Ok(result)
    }
}
