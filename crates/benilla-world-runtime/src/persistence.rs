// SPDX-License-Identifier: GPL-3.0-or-later
use crate::rules::strictly_sorted;
use crate::*;
use serde::{Deserialize, Serialize};
use std::io::Read;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub format_version: u32,
    pub content_digest: Digest,
    pub rules_digest: Digest,
    pub state_digest: Digest,
    pub journal_tip: Digest,
    pub state: State,
    pub checksum: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickRecord {
    pub tick: u64,
    pub before_digest: Digest,
    pub after_digest: Digest,
    pub previous_digest: Digest,
    pub inputs: Vec<Input>,
    pub result: TickResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub format_version: u32,
    pub base: JoinBoundary,
    pub base_journal_tip: Digest,
    pub final_boundary: JoinBoundary,
    pub final_journal_tip: Digest,
    pub records: Vec<TickRecord>,
}
impl World {
    pub(crate) fn from_state(
        data: WorldData,
        rules: Rules,
        state: State,
        journal_tip: Digest,
    ) -> Self {
        let base = JoinBoundary {
            world: state.world,
            revision: state.revision,
            tick: state.tick,
            state_digest: digest(b"benilla-world-state-v2\0", &state),
            content_digest: data.content_digest(),
            rules_digest: rules.digest(),
        };
        Self {
            data,
            rules,
            state,
            journal_tip,
            journal_bytes: 0,
            journal: Journal {
                format_version: FORMAT_VERSION,
                final_boundary: base.clone(),
                base,
                base_journal_tip: journal_tip,
                final_journal_tip: journal_tip,
                records: Vec::new(),
            },
        }
    }
    /// Canonical, bounded serialization only. The storage layer must commit these bytes
    /// atomically and keep a previous-good checkpoint; hashes are not signatures or anti-cheat.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut snapshot = Snapshot {
            format_version: FORMAT_VERSION,
            content_digest: self.data.content_digest(),
            rules_digest: self.rules.digest(),
            state_digest: self.state_digest(),
            journal_tip: self.journal_tip,
            state: self.state.clone(),
            checksum: [0; 32],
        };
        snapshot.checksum = snapshot_checksum(&snapshot);
        encode(&snapshot, MAX_SNAPSHOT_BYTES)
    }
    /// Call only after persisting snapshot_bytes successfully. This starts a new journal at
    /// the current boundary; it does not acknowledge a filesystem commit on the caller's behalf.
    pub fn checkpoint_committed(&mut self, persisted_boundary: &JoinBoundary) -> Result<(), Error> {
        if *persisted_boundary != self.join_boundary() {
            return Err(Error::StaleCheckpoint);
        }
        self.journal = Journal {
            format_version: FORMAT_VERSION,
            base: self.join_boundary(),
            base_journal_tip: self.journal_tip,
            final_boundary: self.join_boundary(),
            final_journal_tip: self.journal_tip,
            records: Vec::new(),
        };
        self.journal_bytes = 0;
        Ok(())
    }
    /// Backpressure warning before either hard journal bound; storage should also checkpoint
    /// on its regular durable-save cadence. Never wait for JournalFull in a normal session.
    pub fn checkpoint_due(&self) -> bool {
        self.journal.records.len() >= MAX_JOURNAL_TICKS / 2
            || self.journal_bytes >= MAX_JOURNAL_BYTES / 2
    }
    pub fn journal_bytes(&self) -> Result<Vec<u8>, Error> {
        encode(&self.journal, MAX_JOURNAL_BYTES)
    }
    pub fn restore(data: WorldData, rules: Rules, bytes: &[u8]) -> Result<Self, Error> {
        rules.validate(&data)?;
        let snapshot: Snapshot = decode(bytes, MAX_SNAPSHOT_BYTES)?;
        if snapshot.format_version != FORMAT_VERSION
            || snapshot.content_digest != data.content_digest()
            || snapshot.rules_digest != rules.digest()
        {
            return Err(Error::Incompatible);
        }
        if snapshot.checksum != snapshot_checksum(&snapshot)
            || snapshot.state_digest != digest(b"benilla-world-state-v2\0", &snapshot.state)
        {
            return Err(Error::InvalidSnapshot);
        }
        validate_state(&snapshot.state, &data, &rules)?;
        Ok(Self::from_state(
            data,
            rules,
            snapshot.state,
            snapshot.journal_tip,
        ))
    }
    /// Re-executes every journal command and compares outcomes, events, state hashes and the
    /// hash chain. Untrusted journals still require an authenticated trusted base outside this
    /// crate; an editor who rewrites the entire save can forge a different valid history.
    pub fn replay(
        data: WorldData,
        rules: Rules,
        base_snapshot: &[u8],
        bytes: &[u8],
    ) -> Result<Self, Error> {
        let journal: Journal = decode(bytes, MAX_JOURNAL_BYTES)?;
        let mut world = Self::restore(data, rules, base_snapshot)?;
        if journal.format_version != FORMAT_VERSION
            || journal.base != world.join_boundary()
            || journal.base_journal_tip != world.journal_tip
            || journal.records.len() > MAX_JOURNAL_TICKS
        {
            return Err(Error::ReplayMismatch);
        }
        for expected in journal.records {
            if expected.inputs.len() > MAX_PLAYERS {
                return Err(Error::ReplayMismatch);
            }
            let inputs = expected
                .inputs
                .iter()
                .cloned()
                .map(|input| OwnedInput {
                    authenticated_owner: input.player.owner,
                    input,
                })
                .collect();
            world
                .step(expected.tick, inputs)
                .map_err(|_| Error::ReplayMismatch)?;
            if world.journal.records.last() != Some(&expected) {
                return Err(Error::ReplayMismatch);
            }
        }
        if world.join_boundary() != journal.final_boundary
            || world.journal_tip != journal.final_journal_tip
        {
            return Err(Error::ReplayMismatch);
        }
        Ok(world)
    }
    /// Bounded read, at most the snapshot size limit plus one byte. No filesystem paths,
    /// keys, or host clocks are part of the runtime. Atomic durable writes belong to storage.
    pub fn restore_reader(
        data: WorldData,
        rules: Rules,
        reader: impl std::io::Read,
    ) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        reader
            .take((MAX_SNAPSHOT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Io)?;
        Self::restore(data, rules, &bytes)
    }
}
fn snapshot_checksum(s: &Snapshot) -> Digest {
    digest(
        b"benilla-world-snapshot-v2\0",
        &(
            s.format_version,
            s.content_digest,
            s.rules_digest,
            s.state_digest,
            s.journal_tip,
            &s.state,
        ),
    )
}
fn encode(value: &impl Serialize, max: usize) -> Result<Vec<u8>, Error> {
    let bytes = serde_json::to_vec(value).map_err(|_| Error::Json)?;
    if bytes.len() > max {
        return Err(Error::TooLarge);
    }
    Ok(bytes)
}
fn decode<T: serde::de::DeserializeOwned + Serialize>(
    bytes: &[u8],
    max: usize,
) -> Result<T, Error> {
    if bytes.len() > max {
        return Err(Error::TooLarge);
    }
    let value = serde_json::from_slice(bytes).map_err(|_| Error::Json)?;
    // Reject duplicate map keys, alternative ordering and coercive/noncanonical encodings.
    // This is our versioned integer-only JSON encoding, not a generic JSON standard.
    if serde_json::to_vec(&value).map_err(|_| Error::Json)? != bytes {
        return Err(Error::NonCanonical);
    }
    Ok(value)
}
fn validate_state(s: &State, data: &WorldData, rules: &Rules) -> Result<(), Error> {
    let invalid = || Error::InvalidSnapshot;
    if s.world == [0; 16]
        || s.tick > MAX_TICK
        || s.revision != s.tick
        || s.players.is_empty()
        || s.players.len() > MAX_PLAYERS
        || s.npcs.len() != data.export().spawns.len()
        || s.npcs.len() > MAX_NPCS
        || !strictly_sorted(&s.players.iter().map(|p| p.id).collect::<Vec<_>>())
        || !strictly_sorted(&s.npcs.iter().map(|n| n.id.spawn).collect::<Vec<_>>())
    {
        return Err(invalid());
    }
    let mut owners = BTreeSet::new();
    let ids: BTreeSet<_> = s.players.iter().map(|p| p.id).collect();
    let timer_limit = s.tick + rules.ticks(86_400_000);
    let known: BTreeSet<_> = rules.starting_abilities.iter().copied().collect();
    for p in &s.players {
        if p.id.owner == [0; 16]
            || p.id.character == [0; 16]
            || !owners.insert(p.id.owner)
            || p.position.map != rules.start.map
            || p.position
                .xyz
                .iter()
                .any(|x| x.unsigned_abs() > MAX_COORD as u32)
            || p.health > rules.player_health
            || p.mana > rules.player_mana
            || p.xp > MAX_PROGRESSION
            || p.money > MAX_PROGRESSION
            || p.level != rules.level(p.xp)
            || p.next_sequence > s.tick
            || p.melee_ready_at > timer_limit
            || p.global_ready_at > timer_limit
            || p.abilities != known
            || p.cooldowns.len() > MAX_ABILITIES
            || p.cooldowns
                .iter()
                .any(|(id, tick)| !known.contains(id) || *tick > timer_limit)
            || p.quests.len() > MAX_QUESTS
            || p.inventory.len() > rules.inventory_slots
            || p.next_item_instance == 0
            || !strictly_sorted(&p.inventory.iter().map(|x| x.slot).collect::<Vec<_>>())
            || (p.health == 0) != p.revive_at.is_some()
            || p.revive_at.is_some_and(|tick| tick > timer_limit)
            || (p.health == 0 && p.casting.is_some())
        {
            return Err(invalid());
        }
        if let Some(c) = &p.casting {
            let a = data.ability(c.ability).ok_or_else(invalid)?;
            if !known.contains(&c.ability)
                || c.completes_at <= s.tick
                || c.completes_at > timer_limit
            {
                return Err(invalid());
            }
            match (a.effect, c.target) {
                (benilla_world_data::DirectEffect::Heal, Target::SelfPlayer) => {}
                (benilla_world_data::DirectEffect::Damage, Target::Npc { id }) => {
                    let n = s
                        .npcs
                        .iter()
                        .find(|n| n.id.spawn == id.spawn)
                        .ok_or_else(invalid)?;
                    if id.generation > n.id.generation {
                        return Err(invalid());
                    }
                }
                _ => return Err(invalid()),
            }
        }
        for (id, q) in &p.quests {
            let def = data.quest(*id).ok_or_else(invalid)?;
            if q.counts.len() != def.objectives.len()
                || def
                    .objectives
                    .iter()
                    .zip(&q.counts)
                    .any(|(obj, count)| *count > obj.count || (q.turned_in && *count != obj.count))
            {
                return Err(invalid());
            }
        }
        let mut instances = BTreeSet::new();
        for item in &p.inventory {
            let def = data.item(item.item_entry).ok_or_else(invalid)?;
            if item.count == 0
                || item.count > def.max_stack
                || usize::from(item.slot) >= rules.inventory_slots
                || item.instance == 0
                || item.instance >= p.next_item_instance
                || !instances.insert(item.instance)
            {
                return Err(invalid());
            }
        }
        if let Some(target) = p.active_loot {
            let n = s.npcs.iter().find(|n| n.id == target).ok_or_else(invalid)?;
            let spawn = data.spawn(target.spawn).ok_or_else(invalid)?;
            let position = Position {
                map: spawn.map_id,
                xyz: spawn.position_milliunits,
            };
            if p.health == 0
                || n.health != 0
                || n.tagged_by != Some(p.id)
                || !p.position.in_range(position, 0, rules.interact_range)
            {
                return Err(invalid());
            }
        }
    }
    for (n, spawn) in s.npcs.iter().zip(&data.export().spawns) {
        let c = data.creature(spawn.creature_entry).unwrap();
        if n.id.spawn != spawn.spawn_id
            || n.id.generation > s.tick
            || n.health > c.max_health
            || n.melee_ready_at > timer_limit
            || (n.health == 0) != n.respawn_at.is_some()
            || n.respawn_at
                .is_some_and(|tick| tick <= s.tick || tick > timer_limit)
            || (n.health > 0 && (n.looted || n.loot_money != 0 || !n.loot_items.is_empty()))
            || n.tagged_by.is_some_and(|id| !ids.contains(&id))
            || (n.tagged_by.is_none() && (n.health != c.max_health || n.melee_ready_at != 0))
            || (c.behavior == benilla_world_data::CreatureBehavior::Passive
                && n.tagged_by.is_some())
        {
            return Err(invalid());
        }
        if n.health == 0 {
            let drop = rules.loot.iter().find(|drop| drop.creature == c.entry);
            let fixed_money = drop.map_or(0, |x| x.money);
            let fixed_items = drop.map_or(&[][..], |x| x.items.as_slice());
            if (n.loot_money != 0 && n.loot_money != fixed_money)
                || n.loot_items.len() != fixed_items.len()
                || n.loot_items
                    .iter()
                    .zip(fixed_items)
                    .any(|(left, fixed)| left.as_ref().is_some_and(|left| left != fixed))
                || n.looted != (n.loot_money == 0 && n.loot_items.iter().all(Option::is_none))
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
