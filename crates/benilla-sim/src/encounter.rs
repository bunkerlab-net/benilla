// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic fixed-damage encounter harness, NOT CMaNGOS combat parity.
//! A frozen trusted roster supplies one complete input frame per logical tick.
//! No movement, range, pathfinding, randomness, network timeout, or wall clock.

use crate::xp::{base_gain, Level};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub type PeerId = u64;
pub type EntityId = u64;
pub type StateHash = [u8; 32];
pub const MAX_ROSTER: usize = 64;
pub const MAX_COMMANDS_PER_FRAME: usize = 16;
pub const SIMULATION_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    pub tick_ms: u32,
    /// 1000 is the unmodified XP rate. Values 1..=1_000_000 are supported.
    pub xp_milli: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSpec {
    pub peer: PeerId,
    pub level: Level,
    pub max_health: u32,
    pub damage: u32,
    pub swing_ticks: u32,
    pub respawn_ticks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LootSpec {
    pub item: u32,
    pub quantity: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureSpec {
    pub entity: EntityId,
    pub level: Level,
    pub max_health: u32,
    pub damage: u32,
    pub swing_ticks: u32,
    pub respawn_ticks: u32,
    pub loot: Option<LootSpec>,
}

/// Explicit caller-supplied content, never implicit game database rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Content {
    pub creatures: Vec<CreatureSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Attack(EntityId),
    StopAttack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputFrame {
    pub tick: u64,
    pub peer: PeerId,
    /// Ordered by the sender. No network arrival order enters simulation order.
    pub commands: Vec<Command>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerState {
    pub spec: PlayerSpec,
    pub health: u32,
    pub xp: u64,
    pub inventory: BTreeMap<u32, u64>,
    pub target: Option<EntityId>,
    pub next_swing: u64,
    pub respawn_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureState {
    pub spec: CreatureSpec,
    pub health: u32,
    /// Incremented on respawn. (entity, generation) identifies one kill/reward.
    pub generation: u64,
    pub target: Option<PeerId>,
    pub next_swing: u64,
    pub respawn_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    CreatureDamaged {
        entity: EntityId,
        by: PeerId,
        damage: u32,
        health: u32,
    },
    CreatureDied {
        entity: EntityId,
        generation: u64,
        killer: PeerId,
    },
    Reward {
        entity: EntityId,
        generation: u64,
        peer: PeerId,
        xp: u32,
        loot: Option<LootSpec>,
    },
    PlayerDamaged {
        peer: PeerId,
        by: EntityId,
        damage: u32,
        health: u32,
    },
    PlayerDied {
        peer: PeerId,
    },
    CreatureRespawned {
        entity: EntityId,
        generation: u64,
    },
    PlayerRespawned {
        peer: PeerId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedTick {
    /// Tick just executed; the resulting snapshot is at tick + 1.
    pub tick: u64,
    pub inputs: Vec<InputFrame>,
    pub events: Vec<Event>,
    pub state_hash: StateHash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    Waiting { missing: Vec<PeerId> },
    Advanced(CommittedTick),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Submission {
    Accepted,
    Duplicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidRules,
    InvalidRoster,
    InvalidContent,
    UnknownPeer,
    UnknownTarget,
    WrongTick { expected: u64, received: u64 },
    TooManyCommands,
    ConflictingInput,
    PendingInputs,
    Overflow,
    ReplayMismatch { tick: u64 },
    IncompleteReplay { tick: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Core {
    rules: Rules,
    tick: u64,
    players: BTreeMap<PeerId, PlayerState>,
    creatures: BTreeMap<EntityId, CreatureState>,
}

/// In-memory committed snapshot. No persistence/deserialization format is promised.
/// Private fields ensure callers cannot create invalid states through this API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot(Core);

impl Snapshot {
    pub fn tick(&self) -> u64 {
        self.0.tick
    }
    pub fn state_hash(&self) -> StateHash {
        hash_core(&self.0)
    }
    /// Versioned canonical encoding for diagnostic comparison, not a save-file API.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        encode_core(&self.0)
    }
}

#[derive(Clone, Debug)]
pub struct World {
    core: Core,
    pending: BTreeMap<PeerId, InputFrame>,
}

impl World {
    pub fn new(rules: Rules, content: Content, roster: Vec<PlayerSpec>) -> Result<Self, Error> {
        if rules.tick_ms == 0 || !(1..=1_000_000).contains(&rules.xp_milli) {
            return Err(Error::InvalidRules);
        }
        if roster.is_empty() || roster.len() > MAX_ROSTER {
            return Err(Error::InvalidRoster);
        }
        let mut players = BTreeMap::new();
        for spec in roster {
            if spec.max_health == 0
                || spec.damage == 0
                || spec.swing_ticks == 0
                || spec.respawn_ticks == 0
            {
                return Err(Error::InvalidRoster);
            }
            let peer = spec.peer;
            let state = PlayerState {
                health: spec.max_health,
                spec,
                xp: 0,
                inventory: BTreeMap::new(),
                target: None,
                next_swing: 0,
                respawn_at: None,
            };
            if players.insert(peer, state).is_some() {
                return Err(Error::InvalidRoster);
            }
        }
        let mut creatures = BTreeMap::new();
        for spec in content.creatures {
            if spec.max_health == 0
                || spec.damage == 0
                || spec.swing_ticks == 0
                || spec.respawn_ticks == 0
                || spec
                    .loot
                    .as_ref()
                    .is_some_and(|loot| loot.item == 0 || loot.quantity == 0)
            {
                return Err(Error::InvalidContent);
            }
            let entity = spec.entity;
            let state = CreatureState {
                health: spec.max_health,
                spec,
                generation: 0,
                target: None,
                next_swing: 0,
                respawn_at: None,
            };
            if creatures.insert(entity, state).is_some() {
                return Err(Error::InvalidContent);
            }
        }
        Ok(Self {
            core: Core {
                rules,
                tick: 0,
                players,
                creatures,
            },
            pending: BTreeMap::new(),
        })
    }

    pub fn tick(&self) -> u64 {
        self.core.tick
    }
    pub fn players(&self) -> &BTreeMap<PeerId, PlayerState> {
        &self.core.players
    }
    pub fn creatures(&self) -> &BTreeMap<EntityId, CreatureState> {
        &self.core.creatures
    }
    /// Hash excludes incomplete network arrivals and covers all committed rules/content/state.
    pub fn state_hash(&self) -> StateHash {
        hash_core(&self.core)
    }

    pub fn submit(&mut self, input: InputFrame) -> Result<Submission, Error> {
        if input.tick != self.core.tick {
            return Err(Error::WrongTick {
                expected: self.core.tick,
                received: input.tick,
            });
        }
        if !self.core.players.contains_key(&input.peer) {
            return Err(Error::UnknownPeer);
        }
        if input.commands.len() > MAX_COMMANDS_PER_FRAME {
            return Err(Error::TooManyCommands);
        }
        for command in &input.commands {
            if let Command::Attack(entity) = command {
                if !self.core.creatures.contains_key(entity) {
                    return Err(Error::UnknownTarget);
                }
            }
        }
        if let Some(existing) = self.pending.get(&input.peer) {
            return if existing == &input {
                Ok(Submission::Duplicate)
            } else {
                Err(Error::ConflictingInput)
            };
        }
        self.pending.insert(input.peer, input);
        Ok(Submission::Accepted)
    }

    pub fn step(&mut self) -> Result<StepOutcome, Error> {
        let missing: Vec<_> = self
            .core
            .players
            .keys()
            .filter(|peer| !self.pending.contains_key(peer))
            .copied()
            .collect();
        if !missing.is_empty() {
            return Ok(StepOutcome::Waiting { missing });
        }
        // Transactional staging: an arithmetic failure cannot partially award XP or loot.
        let mut next = self.core.clone();
        let inputs: Vec<_> = self.pending.values().cloned().collect();
        let events = advance(&mut next, &inputs)?;
        let committed = CommittedTick {
            tick: self.core.tick,
            inputs,
            events,
            state_hash: hash_core(&next),
        };
        self.core = next;
        self.pending.clear();
        Ok(StepOutcome::Advanced(committed))
    }

    /// Only snapshot at a committed boundary; never silently discard queued input.
    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        if !self.pending.is_empty() {
            return Err(Error::PendingInputs);
        }
        Ok(Snapshot(self.core.clone()))
    }
    pub fn from_snapshot(snapshot: Snapshot) -> Self {
        Self {
            core: snapshot.0,
            pending: BTreeMap::new(),
        }
    }

    /// Re-execute committed frames and verify both state digests and ordered events.
    pub fn replay(snapshot: Snapshot, log: &[CommittedTick]) -> Result<Self, Error> {
        let mut world = Self::from_snapshot(snapshot);
        for expected in log {
            if world.tick() != expected.tick {
                return Err(Error::ReplayMismatch { tick: world.tick() });
            }
            for input in &expected.inputs {
                world.submit(input.clone())?;
            }
            match world.step()? {
                StepOutcome::Advanced(actual) if actual == *expected => (),
                StepOutcome::Advanced(_) => {
                    return Err(Error::ReplayMismatch {
                        tick: expected.tick,
                    })
                }
                StepOutcome::Waiting { .. } => {
                    return Err(Error::IncompleteReplay {
                        tick: expected.tick,
                    })
                }
            }
        }
        Ok(world)
    }
}

fn add_ticks(tick: u64, duration: u32) -> Result<u64, Error> {
    tick.checked_add(duration as u64).ok_or(Error::Overflow)
}

fn scaled_xp(unit: Level, mob: Level, rate: u32) -> Result<u32, Error> {
    let ratio = base_gain(unit, mob);
    let numerator = ratio.numerator() as u64 * rate as u64;
    let denominator = ratio.denominator() as u64 * 1000;
    let quotient = numerator / denominator;
    let twice_remainder = numerator % denominator * 2;
    let rounded = quotient
        + u64::from(
            twice_remainder > denominator || (twice_remainder == denominator && quotient % 2 == 1),
        );
    u32::try_from(rounded).map_err(|_| Error::Overflow)
}

fn advance(core: &mut Core, inputs: &[InputFrame]) -> Result<Vec<Event>, Error> {
    let tick = core.tick;
    let mut events = Vec::new();
    // Phase order is part of the rules: respawns, input, player swings, creature swings.
    for (&peer, player) in &mut core.players {
        if player.respawn_at.is_some_and(|at| tick >= at) {
            player.health = player.spec.max_health;
            player.respawn_at = None;
            player.target = None;
            player.next_swing = tick;
            events.push(Event::PlayerRespawned { peer });
        }
    }
    for (&entity, creature) in &mut core.creatures {
        if creature.respawn_at.is_some_and(|at| tick >= at) {
            creature.health = creature.spec.max_health;
            creature.respawn_at = None;
            creature.target = None;
            creature.next_swing = tick;
            creature.generation = creature.generation.checked_add(1).ok_or(Error::Overflow)?;
            events.push(Event::CreatureRespawned {
                entity,
                generation: creature.generation,
            });
        }
    }
    for input in inputs {
        let player = core
            .players
            .get_mut(&input.peer)
            .ok_or(Error::UnknownPeer)?;
        if player.health == 0 {
            continue;
        }
        for command in &input.commands {
            match command {
                Command::Attack(entity) => {
                    player.target = if core.creatures[entity].health > 0 {
                        Some(*entity)
                    } else {
                        None
                    };
                }
                Command::StopAttack => player.target = None,
            }
        }
    }
    for (&peer, player) in &mut core.players {
        let Some(entity) = player.target else {
            continue;
        };
        if player.health == 0 || tick < player.next_swing {
            continue;
        }
        let creature = core
            .creatures
            .get_mut(&entity)
            .ok_or(Error::UnknownTarget)?;
        if creature.health == 0 {
            player.target = None;
            continue;
        }
        let damage = player.spec.damage.min(creature.health);
        creature.health -= damage;
        player.next_swing = add_ticks(tick, player.spec.swing_ticks)?;
        if creature.target.is_none() {
            creature.target = Some(peer);
        }
        events.push(Event::CreatureDamaged {
            entity,
            by: peer,
            damage,
            health: creature.health,
        });
        if creature.health == 0 {
            creature.respawn_at = Some(add_ticks(tick, creature.spec.respawn_ticks)?);
            creature.target = None;
            player.target = None;
            let xp = scaled_xp(player.spec.level, creature.spec.level, core.rules.xp_milli)?;
            player.xp = player.xp.checked_add(xp as u64).ok_or(Error::Overflow)?;
            if let Some(loot) = &creature.spec.loot {
                let count = player.inventory.entry(loot.item).or_default();
                *count = count
                    .checked_add(loot.quantity as u64)
                    .ok_or(Error::Overflow)?;
            }
            events.push(Event::CreatureDied {
                entity,
                generation: creature.generation,
                killer: peer,
            });
            events.push(Event::Reward {
                entity,
                generation: creature.generation,
                peer,
                xp,
                loot: creature.spec.loot.clone(),
            });
        }
    }
    // A dead spawn ends every participant's attack; respawn needs a fresh command.
    for player in core.players.values_mut() {
        if player
            .target
            .is_some_and(|entity| core.creatures[&entity].health == 0)
        {
            player.target = None;
        }
    }
    for (&entity, creature) in &mut core.creatures {
        if creature.health == 0 {
            continue;
        }
        if creature
            .target
            .is_some_and(|peer| core.players[&peer].health == 0)
        {
            creature.target = core
                .players
                .iter()
                .find(|(_, player)| player.health > 0 && player.target == Some(entity))
                .map(|(&peer, _)| peer);
        }
        let Some(peer) = creature.target else {
            continue;
        };
        if tick < creature.next_swing {
            continue;
        }
        let player = core.players.get_mut(&peer).ok_or(Error::UnknownPeer)?;
        let damage = creature.spec.damage.min(player.health);
        player.health -= damage;
        creature.next_swing = add_ticks(tick, creature.spec.swing_ticks)?;
        events.push(Event::PlayerDamaged {
            peer,
            by: entity,
            damage,
            health: player.health,
        });
        if player.health == 0 {
            player.target = None;
            player.respawn_at = Some(add_ticks(tick, player.spec.respawn_ticks)?);
            events.push(Event::PlayerDied { peer });
        }
    }
    core.tick = tick.checked_add(1).ok_or(Error::Overflow)?;
    Ok(events)
}

fn hash_core(core: &Core) -> StateHash {
    Sha256::digest(encode_core(core)).into()
}

struct Encoder(Vec<u8>);
impl Encoder {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn u32(&mut self, value: u32) {
        self.0.extend(value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.0.extend(value.to_le_bytes());
    }
    fn option(&mut self, value: Option<u64>) {
        self.u8(u8::from(value.is_some()));
        if let Some(value) = value {
            self.u64(value);
        }
    }
}
fn encode_core(core: &Core) -> Vec<u8> {
    let mut out = Encoder(b"BENILLA-SYNTHETIC-ENCOUNTER\0".to_vec());
    out.u32(SIMULATION_VERSION);
    out.u32(core.rules.tick_ms);
    out.u32(core.rules.xp_milli);
    out.u64(core.tick);
    out.u64(core.players.len() as u64);
    for (&peer, player) in &core.players {
        out.u64(peer);
        out.u8(player.spec.level.get());
        out.u32(player.spec.max_health);
        out.u32(player.spec.damage);
        out.u32(player.spec.swing_ticks);
        out.u32(player.spec.respawn_ticks);
        out.u32(player.health);
        out.u64(player.xp);
        out.option(player.target);
        out.u64(player.next_swing);
        out.option(player.respawn_at);
        out.u64(player.inventory.len() as u64);
        for (&item, &quantity) in &player.inventory {
            out.u32(item);
            out.u64(quantity);
        }
    }
    out.u64(core.creatures.len() as u64);
    for (&entity, creature) in &core.creatures {
        out.u64(entity);
        out.u8(creature.spec.level.get());
        out.u32(creature.spec.max_health);
        out.u32(creature.spec.damage);
        out.u32(creature.spec.swing_ticks);
        out.u32(creature.spec.respawn_ticks);
        out.u8(u8::from(creature.spec.loot.is_some()));
        if let Some(loot) = &creature.spec.loot {
            out.u32(loot.item);
            out.u32(loot.quantity);
        }
        out.u32(creature.health);
        out.u64(creature.generation);
        out.option(creature.target);
        out.u64(creature.next_swing);
        out.option(creature.respawn_at);
    }
    out.0
}

#[cfg(test)]
mod transactional_tests {
    use super::*;
    #[test]
    fn failed_step_is_atomic_even_after_a_lethal_hit() {
        let peer = PlayerSpec {
            peer: 1,
            level: Level::new(10).unwrap(),
            max_health: 1,
            damage: 1,
            swing_ticks: 1,
            respawn_ticks: 1,
        };
        let creature = CreatureSpec {
            entity: 1,
            level: Level::new(10).unwrap(),
            max_health: 1,
            damage: 1,
            swing_ticks: 1,
            respawn_ticks: 1,
            loot: None,
        };
        let mut world = World::new(
            Rules {
                tick_ms: 50,
                xp_milli: 1000,
            },
            Content {
                creatures: vec![creature],
            },
            vec![peer],
        )
        .unwrap();
        world.core.players.get_mut(&1).unwrap().xp = u64::MAX;
        let before = world.core.clone();
        world
            .submit(InputFrame {
                tick: 0,
                peer: 1,
                commands: vec![Command::Attack(1)],
            })
            .unwrap();
        assert_eq!(world.step(), Err(Error::Overflow));
        assert_eq!(world.core, before);
        assert_eq!(world.pending.len(), 1);
        assert_eq!(world.step(), Err(Error::Overflow));
        assert_eq!(world.core, before);
    }
}
