// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Unit.cpp / SpellAuras.cpp. See PLAYER-SLOW.md.
use super::*;
use benilla_sim::player_slow::{MoveType, SpeedRates};
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "synthetic-native-tests"))]
use std::collections::BTreeSet;
pub(super) mod host;
pub(super) mod source;
#[cfg(test)]
mod water_tests;
pub use host::{PlayerSlowCasterSource, PlayerSlowHostSource};
pub use source::{PlayerSlowCatalog, PlayerSlowSource};
#[cfg(any(test, feature = "synthetic-native-tests"))]
const FIRST_HOLDER_ID: u64 = 1 << 32;

/// Validated source data, not permission to cast. Production has no application entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerSlowGenesis {
    pub(super) catalog: PlayerSlowCatalog,
    host: PlayerSlowHostSource,
    // Construction and execution of this schedule exist only in the explicit fixture feature.
    pub(super) fixture_effects: Vec<FixtureEffect>,
    pub(super) fixture_triggers: Vec<npc_spell::FixtureTrigger>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct FixtureEffect {
    pub(super) tick: u64,
    pub(super) spell_id: u32,
    pub(super) caster: crate::NpcId,
    pub(super) after_player_update: bool,
}
impl PlayerSlowGenesis {
    pub fn from_sources(
        sources: Vec<PlayerSlowSource>,
        host: PlayerSlowHostSource,
    ) -> Result<Self, NativeError> {
        Ok(Self {
            catalog: PlayerSlowCatalog::validate_sources(sources)?,
            host,
            fixture_effects: vec![],
            fixture_triggers: vec![],
        })
    }
    pub fn spell_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.catalog.spell_ids()
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        self.catalog.validate()?;
        self.validate_triggers(profile)?;
        self.host.validate(profile)?;
        if profile.source_motion.is_none() {
            return Err(NativeError::UnsupportedMechanic(
                "slow aura movement-mode authority",
            ));
        }
        // Existing initialized passives must independently prove that no speed/cost/script
        // modifier is hidden behind the active-holder catalog's closed source domain.
        if profile.combat.as_ref().is_some_and(|g| {
            g.initial_auras
                .iter()
                .any(|a| matches!(a.aura_type, 31 | 32 | 33 | 58 | 129 | 130 | 171 | 172 | 191))
        }) {
            return Err(NativeError::UnsupportedMechanic(
                "player passive movement modifiers",
            ));
        }
        if self.fixture_effects.len() > MAX_RECORDS
            || self.fixture_effects.windows(2).any(|w| {
                (w[0].tick, w[0].after_player_update) > (w[1].tick, w[1].after_player_update)
            })
        {
            return Err(NativeError::InvalidProfile);
        }
        let mut per_tick = BTreeMap::<u64, usize>::new();
        for effect in &self.fixture_effects {
            let count = per_tick.entry(effect.tick).or_default();
            *count += 1;
            if *count > MAX_INPUTS_PER_TICK {
                return Err(NativeError::TooLarge);
            }
            let npc = profile
                .npc(effect.caster.spawn)
                .ok_or(NativeError::InvalidProfile)?;
            if effect.tick == 0 || effect.tick > crate::MAX_TICK {
                return Err(NativeError::InvalidProfile);
            }
            self.catalog.recipe(effect.spell_id, npc.ai.level)?;
        }
        Ok(())
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn with_fixture_effects(
        mut self,
        effects: Vec<synthetic_slow_fixture::SyntheticSlowEffect>,
    ) -> Self {
        self.fixture_effects = effects
            .into_iter()
            .map(|effect| FixtureEffect {
                tick: effect.tick,
                spell_id: effect.spell_id,
                caster: effect.caster,
                after_player_update: effect.phase
                    == synthetic_slow_fixture::SyntheticSlowPhase::AfterPlayerUpdate,
            })
            .collect();
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerSlowHolder {
    pub id: u64,
    pub spell_id: u32,
    pub caster: crate::NpcId,
    pub caster_level: u8,
    pub effect_slot: u8,
    pub amount: i32,
    pub remaining_ms: u32,
    pub initial_duration_ms: u32,
    pub max_duration_ms: u32,
    pub slot: u8,
    pub applied_at_ms: u64,
    pub skip_update: bool,
    pub created_after_player_update: bool,
    pub source_binding: crate::Digest,
    pub attributes: u32,
    pub attributes_ex3: u32,
    pub caster_aura_state: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerSlowRemoval {
    /// Source AURA_REMOVE_BY_DEFAULT (including water and spell-ID callbacks).
    Default,
    Stack,
    Expire,
    Death,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PlayerSlowEvent {
    Applied {
        holder: PlayerSlowHolder,
    },
    Removed {
        holder: PlayerSlowHolder,
        reason: PlayerSlowRemoval,
        slot_level: u8,
    },
    SpeedChanged {
        kind: MoveType,
        rate: ai::Real,
        speed: ai::Real,
    },
}
// Events are inert replay material, not live components or a casting command.
impl<'de> Deserialize<'de> for PlayerSlowHolder {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        HolderSnapshot::deserialize(deserializer).map(Into::into)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSlowState {
    holders: BTreeMap<(u32, u64), PlayerSlowHolder>,
    next_holder_id: u64,
    speeds: SpeedRates,
    slot_levels: [u8; 16],
}
impl PlayerSlowState {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn new() -> Self {
        Self {
            holders: BTreeMap::new(),
            next_holder_id: FIRST_HOLDER_ID,
            speeds: SpeedRates::normal(),
            slot_levels: [0; 16],
        }
    }
    pub fn holders(&self) -> impl Iterator<Item = &PlayerSlowHolder> {
        self.holders.values()
    }
    pub fn speeds(&self) -> SpeedRates {
        self.speeds
    }
    pub fn slot_levels(&self) -> [u8; 16] {
        self.slot_levels
    }
    pub fn movement_speed(
        &self,
        flags: benilla_sim::creature_motion::GroundMovementFlags,
    ) -> Result<ai::Real, NativeError> {
        let mode = if flags.is_walking() {
            MoveType::Walk
        } else if flags.bits() & 2 != 0 {
            MoveType::RunBack
        } else {
            MoveType::Run
        };
        Ok(self.speeds.speed(mode)?)
    }
    fn changes(&mut self, events: &mut Vec<Event>) -> Result<(), NativeError> {
        let next = SpeedRates::from_amounts(self.holders.values().map(|h| h.amount))?;
        for (kind, before, rate) in [
            (MoveType::Run, self.speeds.run, next.run),
            (MoveType::RunBack, self.speeds.run_back, next.run_back),
            (MoveType::Swim, self.speeds.swim, next.swim),
        ] {
            if before != rate {
                events.push(Event::PlayerSlow(PlayerSlowEvent::SpeedChanged {
                    kind,
                    rate,
                    speed: next.speed(kind)?,
                }));
            }
        }
        self.speeds = next;
        Ok(())
    }
    pub(super) fn remove(
        &mut self,
        id: u64,
        reason: PlayerSlowRemoval,
        slot_level: u8,
        events: &mut Vec<Event>,
    ) -> Result<(), NativeError> {
        let key = self
            .holders
            .keys()
            .find(|(_, holder)| *holder == id)
            .copied()
            .ok_or(NativeError::InvalidState)?;
        let holder = self.holders.remove(&key).unwrap();
        // RemoveAura erases the modifier before _RemoveSpellAuraHolder clears its slot.
        self.changes(events)?;
        self.slot_levels[usize::from(holder.slot - 32)] = slot_level;
        events.push(Event::PlayerSlow(PlayerSlowEvent::Removed {
            holder,
            reason,
            slot_level,
        }));
        Ok(())
    }
    /// Unit::RemoveAurasWithInterruptFlags restarts at begin after each removal.
    /// The immutable catalog is the SpellProto source, never snapshot-supplied flags.
    pub(super) fn remove_interrupt_auras(
        &mut self,
        mask: u32,
        profile: &Profile,
        npcs: &BTreeMap<u64, Npc>,
        events: &mut Vec<Event>,
    ) -> Result<(), NativeError> {
        if mask & !source::WATER_INTERRUPT_FLAGS != 0 {
            return Err(NativeError::UnsupportedMechanic(
                "slow non-water aura interrupt callback",
            ));
        }
        // These recipes have no mutable holder or stat/resource unapplication.
        if profile
            .combat
            .as_ref()
            .is_some_and(|g| g.initial_auras.iter().any(|a| a.aura_interrupt_flags != 0))
        {
            return Err(NativeError::UnsupportedMechanic(
                "source motion interruptible initial player auras",
            ));
        }
        let catalog = &profile
            .player_slow
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .catalog;
        loop {
            let mut selected = None;
            for holder in self.holders.values() {
                if catalog.interrupt_flags(holder.spell_id)? & mask != 0 {
                    selected = Some(holder.id);
                    break;
                }
            }
            let Some(id) = selected else { return Ok(()) };
            let level = self.removal_level(id, profile, npcs)?;
            self.remove(id, PlayerSlowRemoval::Default, level, events)?;
        }
    }
    /// Unit::RemoveAurasDueToSpell refreshes the spell bounds after each removal.
    pub(super) fn remove_spell_auras(
        &mut self,
        spell_id: u32,
        profile: &Profile,
        npcs: &BTreeMap<u64, Npc>,
        events: &mut Vec<Event>,
    ) -> Result<(), NativeError> {
        if profile
            .combat
            .as_ref()
            .is_some_and(|g| g.initial_auras.iter().any(|a| a.spell_id == spell_id))
        {
            return Err(NativeError::UnsupportedMechanic(
                "initial player passive aura removal",
            ));
        }
        while let Some(id) = self
            .holders
            .range((spell_id, 0)..=(spell_id, u64::MAX))
            .next()
            .map(|(_, holder)| holder.id)
        {
            let level = self.removal_level(id, profile, npcs)?;
            self.remove(id, PlayerSlowRemoval::Default, level, events)?;
        }
        Ok(())
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn apply(
        &mut self,
        profile: &Profile,
        world: crate::WorldId,
        target: crate::PlayerId,
        caster: crate::NpcId,
        caster_level: u8,
        now_ms: u64,
        after_player_update: bool,
        spell_id: u32,
        events: &mut Vec<Event>,
    ) -> Result<(), NativeError> {
        let recipe = profile
            .player_slow
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .catalog
            .recipe(spell_id, caster_level)?;
        if self
            .holders
            .values()
            .any(|h| h.spell_id == spell_id && h.caster.spawn != caster.spawn)
        {
            return Err(NativeError::UnsupportedMechanic(
                "same spell from different slow-aura casters",
            ));
        }
        if let Some(id) = self
            .holders
            .values()
            .find(|h| h.spell_id == spell_id && h.caster.spawn == caster.spawn)
            .map(|h| h.id)
        {
            self.remove(id, PlayerSlowRemoval::Stack, caster_level, events)?;
        }
        if self.holders.len() >= MAX_AURAS {
            return Err(NativeError::Limit);
        }
        let slot = (32..48)
            .find(|slot| self.holders.values().all(|h| h.slot != *slot))
            .ok_or(NativeError::Limit)?;
        let id = self.next_holder_id;
        self.next_holder_id = id.checked_add(1).ok_or(NativeError::Limit)?;
        let holder = PlayerSlowHolder {
            id,
            spell_id,
            caster,
            caster_level,
            effect_slot: recipe.effect_slot,
            amount: recipe.amount,
            remaining_ms: recipe.duration_ms,
            initial_duration_ms: recipe.duration_ms,
            max_duration_ms: recipe.max_duration_ms,
            slot,
            applied_at_ms: now_ms,
            skip_update: true,
            created_after_player_update: after_player_update,
            source_binding: source_binding(
                profile,
                world,
                target,
                caster,
                caster_level,
                recipe.binding,
            ),
            attributes: recipe.attributes,
            attributes_ex3: recipe.attributes_ex3,
            caster_aura_state: recipe.caster_aura_state,
        };
        self.slot_levels[usize::from(slot - 32)] = caster_level;
        self.holders.insert((spell_id, id), holder.clone());
        // _AddSpellAuraHolder publishes slot/duration before HandleAuraModDecreaseSpeed.
        events.push(Event::PlayerSlow(PlayerSlowEvent::Applied { holder }));
        self.changes(events)
    }
    pub(super) fn update(
        &mut self,
        diff_ms: u32,
        now_ms: u64,
        profile: &Profile,
        npcs: &BTreeMap<u64, Npc>,
        events: &mut Vec<Event>,
    ) -> Result<(), NativeError> {
        for holder in self.holders.values_mut() {
            if holder.skip_update {
                holder.skip_update = false;
                if holder.applied_at_ms == now_ms {
                    continue;
                }
            }
            holder.remaining_ms = holder.remaining_ms.saturating_sub(diff_ms);
        }
        // Unit::_UpdateSpells first updates every holder, then restarts removal at begin.
        let expired = self
            .holders
            .values()
            .filter(|h| h.remaining_ms == 0)
            .map(|h| h.id)
            .collect::<Vec<_>>();
        for id in expired {
            let level = self.removal_level(id, profile, npcs)?;
            self.remove(id, PlayerSlowRemoval::Expire, level, events)?;
        }
        Ok(())
    }
    pub(super) fn removal_level(
        &self,
        id: u64,
        profile: &Profile,
        npcs: &BTreeMap<u64, Npc>,
    ) -> Result<u8, NativeError> {
        let holder = self
            .holders
            .values()
            .find(|h| h.id == id)
            .ok_or(NativeError::InvalidState)?;
        Ok(if let Some(npc) = npcs.get(&holder.caster.spawn) {
            npc.effective_profile(profile)?.ai.level
        } else {
            profile
                .player_slow
                .as_ref()
                .ok_or(NativeError::InvalidState)?
                .host
                .max_player_level
        })
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn validate(&self, state: &State, profile: &Profile) -> Result<(), NativeError> {
        let genesis = profile
            .player_slow
            .as_ref()
            .ok_or(NativeError::InvalidCheckpoint)?;
        if self.holders.len() > MAX_AURAS
            || self.next_holder_id < FIRST_HOLDER_ID
            || self.next_holder_id
                > FIRST_HOLDER_ID
                    .saturating_add(state.tick.saturating_mul(MAX_INPUTS_PER_TICK as u64))
            || (state.health == 0 && !self.holders.is_empty())
            || self.speeds != SpeedRates::from_amounts(self.holders.values().map(|h| h.amount))?
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let reachable_slots = (self.next_holder_id - FIRST_HOLDER_ID)
            .min(genesis.catalog.spell_ids().count() as u64) as usize;
        if self.slot_levels.iter().enumerate().any(|(slot, level)| {
            // All admitted NPC Units persist; permanent removal/fallback is not an active
            // world lifecycle. Untouched or unreachable slots cannot contain residue.
            (*level != 0 && !profile.npcs.iter().any(|npc| npc.ai.level == *level))
                || (slot >= reachable_slots && *level != 0)
        }) {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut slots = BTreeSet::new();
        let mut spells = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for (key, h) in &self.holders {
            let npc = state
                .npcs
                .get(&h.caster.spawn)
                .ok_or(NativeError::InvalidCheckpoint)?;
            let npc_profile = profile
                .npc(h.caster.spawn)
                .ok_or(NativeError::InvalidCheckpoint)?;
            let r = genesis.catalog.recipe(h.spell_id, h.caster_level)?;
            let age = state
                .now_ms
                .checked_sub(h.applied_at_ms)
                .ok_or(NativeError::InvalidCheckpoint)?;
            if *key != (h.spell_id, h.id)
                || h.id < FIRST_HOLDER_ID
                || h.id >= self.next_holder_id
                || !ids.insert(h.id)
                || !spells.insert(h.spell_id)
                || !slots.insert(h.slot)
                || !(32..48).contains(&h.slot)
                || usize::from(h.slot - 32) >= reachable_slots
                || u64::from(h.slot - 32) > h.id - FIRST_HOLDER_ID
                || self.slot_levels[usize::from(h.slot - 32)] != h.caster_level
                || h.caster.generation > npc.id.generation
                || h.caster_level != npc_profile.ai.level
                || h.effect_slot != r.effect_slot
                || h.amount != r.amount
                || h.initial_duration_ms != r.duration_ms
                || h.max_duration_ms != r.max_duration_ms
                || h.attributes != r.attributes
                || h.attributes_ex3 != r.attributes_ex3
                || h.caster_aura_state != r.caster_aura_state
                || h.source_binding
                    != source_binding(
                        profile,
                        state.world,
                        state.player,
                        h.caster,
                        h.caster_level,
                        r.binding,
                    )
                || h.applied_at_ms <= profile.clock_origin_ms
                || !(h.applied_at_ms - profile.clock_origin_ms)
                    .is_multiple_of(u64::from(profile.tick_ms))
                || h.remaining_ms == 0
                || u64::from(h.remaining_ms) != u64::from(h.initial_duration_ms).saturating_sub(age)
                || h.skip_update != (h.created_after_player_update && age == 0)
            {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        Ok(())
    }
}
#[cfg(any(test, feature = "synthetic-native-tests"))]
fn source_binding(
    profile: &Profile,
    world: crate::WorldId,
    target: crate::PlayerId,
    caster: crate::NpcId,
    level: u8,
    recipe: crate::Digest,
) -> crate::Digest {
    crate::digest(
        b"benilla-player-slow-holder-source-v1\0",
        &(profile.digest(), world, target, caster, level, recipe),
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HolderSnapshot {
    id: u64,
    spell_id: u32,
    caster: crate::NpcId,
    caster_level: u8,
    effect_slot: u8,
    amount: i32,
    remaining_ms: u32,
    initial_duration_ms: u32,
    max_duration_ms: u32,
    slot: u8,
    applied_at_ms: u64,
    skip_update: bool,
    created_after_player_update: bool,
    source_binding: crate::Digest,
    attributes: u32,
    attributes_ex3: u32,
    caster_aura_state: u32,
}
impl From<HolderSnapshot> for PlayerSlowHolder {
    fn from(h: HolderSnapshot) -> Self {
        Self {
            id: h.id,
            spell_id: h.spell_id,
            caster: h.caster,
            caster_level: h.caster_level,
            effect_slot: h.effect_slot,
            amount: h.amount,
            remaining_ms: h.remaining_ms,
            initial_duration_ms: h.initial_duration_ms,
            max_duration_ms: h.max_duration_ms,
            slot: h.slot,
            applied_at_ms: h.applied_at_ms,
            skip_update: h.skip_update,
            created_after_player_update: h.created_after_player_update,
            source_binding: h.source_binding,
            attributes: h.attributes,
            attributes_ex3: h.attributes_ex3,
            caster_aura_state: h.caster_aura_state,
        }
    }
}
impl From<&PlayerSlowHolder> for HolderSnapshot {
    fn from(h: &PlayerSlowHolder) -> Self {
        Self {
            id: h.id,
            spell_id: h.spell_id,
            caster: h.caster,
            caster_level: h.caster_level,
            effect_slot: h.effect_slot,
            amount: h.amount,
            remaining_ms: h.remaining_ms,
            initial_duration_ms: h.initial_duration_ms,
            max_duration_ms: h.max_duration_ms,
            slot: h.slot,
            applied_at_ms: h.applied_at_ms,
            skip_update: h.skip_update,
            created_after_player_update: h.created_after_player_update,
            source_binding: h.source_binding,
            attributes: h.attributes,
            attributes_ex3: h.attributes_ex3,
            caster_aura_state: h.caster_aura_state,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayerSlowSnapshot {
    holders: Vec<HolderSnapshot>,
    next_holder_id: u64,
    speeds: SpeedRates,
    slot_levels: [u8; 16],
}
impl PlayerSlowState {
    pub(super) fn snapshot(&self) -> PlayerSlowSnapshot {
        PlayerSlowSnapshot {
            holders: self.holders.values().map(Into::into).collect(),
            next_holder_id: self.next_holder_id,
            speeds: self.speeds,
            slot_levels: self.slot_levels,
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(snapshot: PlayerSlowSnapshot) -> Result<Self, NativeError> {
        if snapshot.holders.len() > MAX_AURAS
            || snapshot
                .holders
                .windows(2)
                .any(|w| (w[0].spell_id, w[0].id) >= (w[1].spell_id, w[1].id))
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(Self {
            holders: snapshot
                .holders
                .into_iter()
                .map(|h| ((h.spell_id, h.id), h.into()))
                .collect(),
            next_holder_id: snapshot.next_holder_id,
            speeds: snapshot.speeds,
            slot_levels: snapshot.slot_levels,
        })
    }
}

#[cfg(any(test, feature = "synthetic-native-tests"))]
impl PlayerSlowState {
    pub(super) fn validate_npc_spell_holders(
        &self,
        state: &State,
        profile: &Profile,
    ) -> Result<(), NativeError> {
        let invalid = || NativeError::InvalidCheckpoint;
        let genesis = profile.player_slow.as_ref().ok_or_else(invalid)?;
        let mut applied: Vec<_> = state
            .npcs
            .values()
            .flat_map(|n| n.events.spells())
            .filter(|l| l.applied_holder.is_some())
            .collect();
        // Source-motion admission currently represents one NPC. The fixture's
        // retained after-Unit starts are the final holder-producing phase.
        applied.sort_by_key(|l| (l.caster.spawn, l.cast_id));
        let first = self
            .next_holder_id
            .checked_sub(applied.len() as u64)
            .ok_or_else(invalid)?;
        if first < FIRST_HOLDER_ID {
            return Err(invalid());
        }
        let mut last = BTreeMap::new();
        for (ordinal, lifetime) in applied.into_iter().enumerate() {
            if state.health == 0
                || lifetime.applied_holder != Some(first + ordinal as u64)
                || lifetime.started_at_ms != state.now_ms
            {
                return Err(invalid());
            }
            last.insert(lifetime.spell_id, lifetime);
        }
        for (&spell_id, lifetime) in &last {
            let holder = self
                .holders()
                .find(|h| h.spell_id == spell_id)
                .ok_or_else(invalid)?;
            if Some(holder.id) != lifetime.applied_holder
                || holder.caster != lifetime.caster
                || holder.applied_at_ms != lifetime.started_at_ms
                || !holder.created_after_player_update
            {
                return Err(invalid());
            }
        }
        for holder in self.holders().filter(|h| h.applied_at_ms == state.now_ms) {
            if last.contains_key(&holder.spell_id) {
                continue;
            }
            let earlier_effect = genesis.fixture_effects.iter().any(|e| {
                e.tick == state.tick && e.spell_id == holder.spell_id && e.caster == holder.caster
            });
            let earlier_trigger = genesis.fixture_triggers.iter().any(|t| {
                t.tick == state.tick
                    && !t.after_npc_update
                    && t.spell_id == holder.spell_id
                    && t.caster == holder.caster
            });
            if !earlier_effect && !earlier_trigger {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
