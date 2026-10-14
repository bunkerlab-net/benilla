// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickRecord {
    pub version: u32,
    pub profile: crate::Digest,
    pub tick: u64,
    pub inputs: Vec<Input>,
    pub before: crate::Digest,
    pub after: crate::Digest,
    pub previous_record: crate::Digest,
    pub replies: Vec<AuthorityReply>,
    pub events: Vec<Event>,
    pub draws: Vec<Draw>,
}
impl TickRecord {
    pub fn digest(&self) -> crate::Digest {
        crate::digest(b"benilla-native-record-v16\0", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CastState {
    binding: ExecutionBinding,
    release_binding: Option<ExecutionBinding>,
    id: u64,
    ability: u32,
    target: crate::NpcId,
    origin: ai::Point,
    execution: spell::CastCheckpoint,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NpcState {
    events: npc_events::NpcEventsSnapshot,
    source_motion: Option<NpcMotionSnapshot>,
    regen_elapsed_ms: u32,
    original_loot_recipient: Option<crate::PlayerId>,
    corpse_loot: Option<CorpseLoot>,
    aura_bindings: BTreeMap<u32, ExecutionBinding>,
    id: crate::NpcId,
    health: u32,
    ai: ai::AiCheckpoint,
    auras: BTreeMap<u32, spell::DamageTargetCheckpoint>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StateCheckpoint {
    admin: AdminState,
    player_slow: Option<player_slow::PlayerSlowSnapshot>,
    player_movement_flags: Option<benilla_sim::creature_motion::GroundMovementFlags>,
    player_events: player_events::PlayerEventsSnapshot,
    quest_progression: Option<quest_progression::QuestProgressionSnapshot>,
    version: u32,
    policy: String,
    rng_policy: String,
    profile: crate::Digest,
    domain: Domain,
    world: crate::WorldId,
    player: crate::PlayerId,
    seed: u64,
    tick: u64,
    now_ms: u64,
    next_sequence: u64,
    position: ai::Point,
    orientation: Orientation,
    health: u32,
    resources: spell::ResourceCheckpoint,
    regen_elapsed_ms: u32,
    last_mana_use_timer_ms: u32,
    npcs: BTreeMap<u64, NpcState>,
    casts: BTreeMap<u64, CastState>,
    next_cast_id: u64,
    rng_counter: u64,
    combat: Option<combat::CombatSnapshot>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    version: u32,
    pub(super) state: StateCheckpoint,
    pub(super) digest: crate::Digest,
    pub(super) history_tip: crate::Digest,
    pub(super) checksum: crate::Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointIdentity {
    pub world: crate::WorldId,
    pub player: crate::PlayerId,
    pub tick: u64,
    pub profile: crate::Digest,
    pub domain: Domain,
    pub state_digest: crate::Digest,
    pub history_tip: crate::Digest,
}
/// Canonical bounded journal wire format, including array delimiters and separators.
pub fn encode_records(records: &[TickRecord]) -> Result<Vec<u8>, NativeError> {
    if records.len() > MAX_RECORDS {
        return Err(NativeError::TooLarge);
    }
    let bytes = serde_json::to_vec(records).map_err(|_| NativeError::InvalidCheckpoint)?;
    if bytes.len() > MAX_BYTES {
        return Err(NativeError::TooLarge);
    }
    Ok(bytes)
}
pub fn decode_records(bytes: &[u8]) -> Result<Vec<TickRecord>, NativeError> {
    if bytes.len() > MAX_BYTES {
        return Err(NativeError::TooLarge);
    }
    let records: Vec<TickRecord> =
        serde_json::from_slice(bytes).map_err(|_| NativeError::InvalidCheckpoint)?;
    if records.iter().any(|r| r.version != NATIVE_FORMAT_VERSION)
        || encode_records(&records)? != bytes
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(records)
}
impl Checkpoint {
    pub fn identity(&self) -> CheckpointIdentity {
        CheckpointIdentity {
            world: self.state.world,
            player: self.state.player,
            tick: self.state.tick,
            profile: self.state.profile,
            domain: self.state.domain,
            state_digest: self.digest,
            history_tip: self.history_tip,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, NativeError> {
        let bytes = serde_json::to_vec(self).map_err(|_| NativeError::InvalidCheckpoint)?;
        if bytes.len() > MAX_BYTES {
            return Err(NativeError::TooLarge);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, NativeError> {
        if bytes.len() > MAX_BYTES {
            return Err(NativeError::TooLarge);
        }
        let s: Self = serde_json::from_slice(bytes).map_err(|_| NativeError::InvalidCheckpoint)?;
        if s.version != NATIVE_FORMAT_VERSION
            || s.state.version != NATIVE_FORMAT_VERSION
            || s.state.policy != NATIVE_POLICY
            || s.state.rng_policy != RNG_POLICY
            || s.digest != crate::digest(b"benilla-native-state-v16\0", &s.state)
            || s.checksum
                != crate::digest(
                    b"benilla-native-checkpoint-v16\0",
                    &(&s.state, s.digest, s.history_tip),
                )
            || s.encode()? != bytes
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(s)
    }
}
pub(super) fn snapshot_state(profile_digest: crate::Digest, state: &State) -> StateCheckpoint {
    StateCheckpoint {
        admin: state.admin.clone(),
        player_slow: state.player_slow.as_ref().map(PlayerSlowState::snapshot),
        player_movement_flags: state.player_movement_flags,
        player_events: state.player_events.snapshot(),
        quest_progression: state
            .quest_progression
            .as_ref()
            .map(QuestProgressionState::snapshot),
        version: NATIVE_FORMAT_VERSION,
        policy: NATIVE_POLICY.into(),
        rng_policy: RNG_POLICY.into(),
        profile: profile_digest,
        domain: state.domain,
        world: state.world,
        player: state.player,
        seed: state.seed,
        tick: state.tick,
        now_ms: state.now_ms,
        next_sequence: state.next_sequence,
        position: state.position,
        orientation: state.orientation,
        health: state.health,
        resources: state.resources.caster.checkpoint(),
        regen_elapsed_ms: state.resources.regen_elapsed_ms(),
        last_mana_use_timer_ms: state.resources.last_mana_use_timer_ms(),
        npcs: state
            .npcs
            .iter()
            .map(|(&id, n)| {
                (
                    id,
                    NpcState {
                        events: n.events.snapshot(),
                        source_motion: n.source_motion.as_ref().map(NpcMotion::snapshot),
                        regen_elapsed_ms: n.regen_elapsed_ms,
                        original_loot_recipient: n.original_loot_recipient,
                        corpse_loot: n.corpse_loot.clone(),
                        aura_bindings: n.aura_bindings.clone(),
                        id: n.id,
                        health: n.health,
                        ai: n.ai.checkpoint(),
                        auras: n
                            .auras
                            .iter()
                            .map(|(&spell, a)| {
                                let mut a = a.clone();
                                a.health = n.health;
                                (spell, a.checkpoint())
                            })
                            .collect(),
                    },
                )
            })
            .collect(),
        casts: state
            .casts
            .iter()
            .map(|(&id, c)| {
                (
                    id,
                    CastState {
                        binding: c.binding.clone(),
                        release_binding: c.release_binding.clone(),
                        id: c.id,
                        ability: c.ability,
                        target: c.target,
                        origin: c.origin,
                        execution: c.execution.checkpoint(),
                    },
                )
            })
            .collect(),
        next_cast_id: state.next_cast_id,
        rng_counter: state.rng_counter,
        combat: state.combat.as_ref().map(PlayerCombatState::snapshot),
    }
}
impl NativeWorld {
    pub(super) fn snapshot_of(&self, state: &State) -> StateCheckpoint {
        snapshot_state(self.profile_digest, state)
    }
    pub(super) fn checkpoint_state(&self) -> StateCheckpoint {
        self.snapshot_of(&self.state)
    }
    pub fn checkpoint(&self) -> Result<Checkpoint, NativeError> {
        self.make_checkpoint(&self.state, self.journal_tip)
    }
    pub(super) fn make_checkpoint(
        &self,
        state: &State,
        history_tip: crate::Digest,
    ) -> Result<Checkpoint, NativeError> {
        let state = self.snapshot_of(state);
        let digest = crate::digest(b"benilla-native-state-v16\0", &state);
        let s = Checkpoint {
            version: NATIVE_FORMAT_VERSION,
            state,
            digest,
            history_tip,
            checksum: [0; 32],
        };
        let mut s = s;
        s.checksum = crate::digest(
            b"benilla-native-checkpoint-v16\0",
            &(&s.state, s.digest, s.history_tip),
        );
        s.encode()?;
        Ok(s)
    }
    /// Clear bounded journal only after the caller durably commits this exact checkpoint.
    /// Filesystem locking/atomic replacement belongs to the pending WorldStore adapter.
    pub fn checkpoint_committed(&mut self, checkpoint: &Checkpoint) -> Result<(), NativeError> {
        if checkpoint != &self.checkpoint()? {
            return Err(NativeError::InvalidCheckpoint);
        }
        self.journal.clear();
        self.journal_bytes = 2;
        Ok(())
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub fn restore_synthetic_component_test(
        profile: Profile,
        checkpoint: Checkpoint,
    ) -> Result<Self, NativeError> {
        profile.validate()?;
        if checkpoint.version != NATIVE_FORMAT_VERSION
            || checkpoint.digest != crate::digest(b"benilla-native-state-v16\0", &checkpoint.state)
            || checkpoint.checksum
                != crate::digest(
                    b"benilla-native-checkpoint-v16\0",
                    &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
                )
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let s = checkpoint.state;
        if s.version != NATIVE_FORMAT_VERSION
            || s.policy != NATIVE_POLICY
            || s.rng_policy != RNG_POLICY
            || s.profile != profile.digest()
            || s.domain != Domain::SyntheticComponentTest
        {
            return Err(NativeError::Incompatible);
        }
        if s.world == [0; 16]
            || s.player.owner == [0; 16]
            || s.player.character == [0; 16]
            || s.tick > crate::MAX_TICK
            || s.now_ms
                != s.tick
                    .checked_mul(u64::from(profile.tick_ms))
                    .and_then(|t| t.checked_add(profile.clock_origin_ms))
                    .ok_or(NativeError::InvalidCheckpoint)?
            || s.rng_counter > s.tick.saturating_mul(MAX_RNG_WORDS_PER_TICK)
            || s.next_sequence > s.tick.saturating_mul(MAX_INPUTS_PER_TICK as u64)
            || (s.next_sequence == 0 && s.orientation != profile.player.orientation)
            || s.health
                > s.combat
                    .as_ref()
                    .map(|c| c.stats.max_health)
                    .unwrap_or(profile.player.max_health)
            || s.casts.len() > MAX_CASTS
            || s.npcs.len() != profile.npcs.len()
            || s.next_cast_id == 0
            || s.next_cast_id > s.next_sequence.saturating_add(1)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        s.position.distance_squared(s.position)?;
        let (abilities, combat_genesis) = match (
            &profile.quest_progression,
            &s.quest_progression,
            &profile.combat,
        ) {
            (Some(g), Some(q), Some(c)) => {
                let (a, c) = g.restore_inputs(q, c)?;
                (a, Some(c))
            }
            (None, None, c) => (profile.abilities.clone(), c.clone()),
            _ => return Err(NativeError::InvalidCheckpoint),
        };
        let specs = abilities
            .iter()
            .filter_map(|a| match &a.admission {
                AbilityAdmission::Available(s) => Some(s.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let caster = spell::CasterResources::restore(s.resources, s.now_ms, &specs)?;
        let combat = match (&combat_genesis, s.combat) {
            (Some(g), Some(c)) => Some(PlayerCombatState::restore(
                g,
                c,
                s.health,
                caster.mana,
                s.now_ms,
                s.tick,
                profile.clock_origin_ms,
                profile.tick_ms,
            )?),
            (None, None) => None,
            _ => return Err(NativeError::InvalidCheckpoint),
        };
        let quest_progression = match (&profile.quest_progression, s.quest_progression, &combat) {
            (Some(g), Some(q), Some(c)) => Some(QuestProgressionState::restore(
                g,
                q,
                c,
                s.health,
                caster.mana,
                s.now_ms,
            )?),
            (None, None, _) => None,
            _ => return Err(NativeError::InvalidCheckpoint),
        };
        let resources = mana::MageManaState::from_state(
            caster,
            combat
                .as_ref()
                .map(|c| c.stats.max_mana)
                .unwrap_or(profile.player.max_mana),
            combat
                .as_ref()
                .map(|c| c.stats.regen())
                .unwrap_or(profile.player.regen()),
            s.regen_elapsed_ms,
            s.last_mana_use_timer_ms,
        )?;
        if resources.caster.last_mana_use_ms.is_some_and(|t| {
            t < profile.clock_origin_ms
                || !(t - profile.clock_origin_ms).is_multiple_of(u64::from(profile.tick_ms))
        }) {
            return Err(NativeError::InvalidCheckpoint);
        }
        let expected_mana_timer = match resources.caster.last_mana_use_ms {
            None => 0,
            Some(last) => 5000u64.saturating_sub(
                s.now_ms
                    .checked_sub(last)
                    .ok_or(NativeError::InvalidCheckpoint)?
                    .saturating_add(u64::from(profile.tick_ms)),
            ) as u32,
        };
        if resources.last_mana_use_timer_ms() != expected_mana_timer {
            return Err(NativeError::InvalidCheckpoint);
        }
        let mut npcs = BTreeMap::new();
        for (id, n) in s.npcs {
            let p = profile.npc(id).ok_or(NativeError::InvalidCheckpoint)?;
            if n.id.spawn != id
                || n.id.generation > s.tick
                || n.regen_elapsed_ms >= benilla_sim::creature_regeneration::REGEN_INTERVAL_MS
                || u64::from(n.regen_elapsed_ms) > s.tick.saturating_mul(u64::from(profile.tick_ms))
                || !n.regen_elapsed_ms.is_multiple_of(gcd(
                    profile.tick_ms,
                    benilla_sim::creature_regeneration::REGEN_INTERVAL_MS,
                ))
                || n.health > p.max_health
                || n.auras.len() > MAX_AURAS
                || n.auras.keys().ne(n.aura_bindings.keys())
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            let ai = ai::SoloCreatureAi::restore(n.ai, p.ai.clone(), s.now_ms, s.tick, SOLO_UNIT)?;
            ai.validate_fixed_step(profile.tick_ms)?;
            if profile.source_motion.is_none() && ai.has_pre_unit_death() {
                return Err(NativeError::InvalidCheckpoint);
            }
            if (n.health == 0) != matches!(ai.phase(), ai::Phase::Corpse | ai::Phase::Despawned) {
                return Err(NativeError::InvalidCheckpoint);
            }
            let mut auras = BTreeMap::new();
            for (ability, a) in n.auras {
                let binding = n
                    .aura_bindings
                    .get(&ability)
                    .ok_or(NativeError::InvalidCheckpoint)?;
                binding.validate(&profile, quest_progression.as_ref(), s.now_ms)?;
                if binding.ability.id != ability {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let spec = binding.spec()?;
                let target = spell::DamageTarget::restore(a, spec, SOLO_UNIT, s.now_ms)?;
                if target.id != id
                    || target.health != n.health
                    || target.aura().is_none()
                    || matches!(
                        ai.phase(),
                        ai::Phase::ReturningHome | ai::Phase::Despawned | ai::Phase::Corpse
                    )
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                let aura = target.aura().unwrap();
                let elapsed = s
                    .now_ms
                    .checked_sub(aura.applied_ms())
                    .ok_or(NativeError::InvalidCheckpoint)?;
                if binding.captured_ms != aura.applied_ms()
                    || aura.applied_ms() < profile.clock_origin_ms
                    || !(aura.applied_ms() - profile.clock_origin_ms)
                        .is_multiple_of(u64::from(profile.tick_ms))
                    || aura.timer_ms <= 0
                    || aura.ticks as u64 != elapsed / u64::from(aura.amplitude_ms)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                auras.insert(ability, target);
            }
            if n.source_motion.is_some() != profile.source_motion.is_some() {
                return Err(NativeError::InvalidCheckpoint);
            }
            let source_motion = n
                .source_motion
                .map(|m| NpcMotion::restore(m, &profile, &ai, s.now_ms))
                .transpose()?;
            npcs.insert(
                id,
                Npc {
                    events: NpcEvents::restore(
                        n.events, s.world, n.id, s.player, &profile, &ai, s.tick,
                    )?,
                    source_motion,
                    regen_elapsed_ms: n.regen_elapsed_ms,
                    original_loot_recipient: n.original_loot_recipient,
                    corpse_loot: n.corpse_loot,
                    id: n.id,
                    health: n.health,
                    ai,
                    auras,
                    aura_bindings: n.aura_bindings,
                },
            );
        }
        if s.health > 0
            && combat.as_ref().is_some_and(|c| !c.in_combat)
            && npcs.values().any(|n| n.ai.phase() == ai::Phase::Combat)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        if let Some(c) = &combat {
            if let Some(killer) = c.death_killer {
                if !npcs
                    .get(&killer.spawn)
                    .is_some_and(|n| killer.generation <= n.id.generation)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
            for n in npcs.values() {
                if n.ai.victim_detached_at_ms().is_some() {
                    if s.health != 0
                        || c.death.as_ref().and_then(|d| d.snapshot().died_at_tick) != Some(s.tick)
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                } else if s.health == 0 && n.ai.phase() == ai::Phase::Combat {
                    return Err(NativeError::InvalidCheckpoint);
                }
            }
        }
        let player_events = PlayerEvents::restore(s.player_events)?;
        let mut casts = BTreeMap::new();
        for (id, c) in s.casts {
            c.binding
                .validate(&profile, quest_progression.as_ref(), s.now_ms)?;
            if c.binding.ability.id != c.ability {
                return Err(NativeError::InvalidCheckpoint);
            }
            let spec = c.binding.spec()?;
            if id == 0
                || id != c.id
                || id >= s.next_cast_id
                || !npcs
                    .get(&c.target.spawn)
                    .is_some_and(|n| c.target.generation <= n.id.generation)
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            c.origin.distance_squared(c.origin)?;
            let release_context = c
                .release_binding
                .as_ref()
                .map(|released| {
                    released.validate(&profile, quest_progression.as_ref(), s.now_ms)?;
                    released.release_context(&c.binding)
                })
                .transpose()?;
            let callback_ms = player_events
                .spell_lifetime(id)
                .and_then(|l| l.last_callback_ms)
                .ok_or(NativeError::InvalidCheckpoint)?;
            let execution = spell::MageProjectileCast::restore_scheduled_with_release(
                c.execution,
                spec.clone(),
                release_context,
                s.now_ms,
                callback_ms,
            )
            .map_err(|_| NativeError::InvalidCheckpoint)?;
            match (execution.state(), &c.release_binding) {
                (spell::CastState::Casting { .. }, None) => {}
                (spell::CastState::Traveling { impact_at_ms }, Some(released)) => {
                    let launch = execution
                        .launch_context()
                        .ok_or(NativeError::InvalidCheckpoint)?;
                    interactions::spell_authority::validate_historical_launch(
                        &profile, released, c.target, &launch,
                    )?;
                    let delay =
                        spell::projectile_delay(launch.distance_squared, spec.projectile_speed)?;
                    if impact_at_ms.checked_sub(u64::from(delay)) != Some(released.captured_ms) {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                }
                _ => return Err(NativeError::InvalidCheckpoint),
            }
            if c.binding.captured_ms != execution.created_ms() {
                return Err(NativeError::InvalidCheckpoint);
            }
            execution.validate_fixed_step(profile.clock_origin_ms, profile.tick_ms)?;
            execution.validate_resources(&resources.caster)?;
            if execution.created_ms() < profile.clock_origin_ms
                || !(execution.created_ms() - profile.clock_origin_ms)
                    .is_multiple_of(u64::from(profile.tick_ms))
                || execution.caster_id() != SOLO_UNIT
                || execution.target_id() != c.target.spawn
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            casts.insert(
                id,
                Cast {
                    binding: c.binding,
                    release_binding: c.release_binding,
                    id,
                    ability: c.ability,
                    target: c.target,
                    origin: c.origin,
                    execution,
                },
            );
        }
        if casts
            .values()
            .filter(|c| matches!(c.execution.state(), spell::CastState::Casting { .. }))
            .count()
            > 1
            || (s.health == 0
                && casts
                    .values()
                    .any(|c| !matches!(c.execution.state(), spell::CastState::Traveling { .. })))
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        if let Some(q) = &quest_progression {
            let mut deaths = 0u64;
            for (&spawn, &generation) in &q.credited_generations {
                let npc = npcs.get(&spawn).ok_or(NativeError::InvalidCheckpoint)?;
                if generation > npc.id.generation
                    || (npc.health > 0 && generation == npc.id.generation)
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                deaths = deaths
                    .checked_add(generation.checked_add(1).ok_or(NativeError::Limit)?)
                    .ok_or(NativeError::Limit)?;
            }
            if deaths.checked_add(1) != Some(q.next_death_event)
                || npcs.values().any(|n| {
                    n.health == 0
                        && q.credited_generations.get(&n.id.spawn) != Some(&n.id.generation)
                })
                || q.played_time_total != ((s.now_ms - profile.clock_origin_ms) / 1000) as u32
                || q.quests.next_sequence()
                    > s.next_sequence.saturating_add(deaths.saturating_mul(3))
            {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        if s.player_movement_flags.is_some() != profile.source_motion.is_some() {
            return Err(NativeError::InvalidCheckpoint);
        }
        let player_slow = match (&profile.player_slow, s.player_slow) {
            (Some(_), Some(snapshot)) => Some(PlayerSlowState::restore(snapshot)?),
            (None, None) => None,
            _ => return Err(NativeError::InvalidCheckpoint),
        };
        let state = State {
            admin: s.admin,
            player_slow,
            player_movement_flags: s.player_movement_flags,
            player_events,
            quest_progression,
            domain: s.domain,
            world: s.world,
            player: s.player,
            seed: s.seed,
            tick: s.tick,
            now_ms: s.now_ms,
            next_sequence: s.next_sequence,
            position: s.position,
            orientation: s.orientation,
            health: s.health,
            resources,
            npcs,
            casts,
            next_cast_id: s.next_cast_id,
            rng_counter: s.rng_counter,
            combat,
        };
        state.admin.validate(&profile, &state)?;
        state.player_events.validate(&state, &profile)?;
        if let Some(slow) = &state.player_slow {
            slow.validate(&state, &profile)?;
            slow.validate_npc_spell_holders(&state, &profile)?;
        }
        let result = Self {
            profile_digest: profile.digest(),
            profile,
            state,
            journal: vec![],
            journal_bytes: 2,
            journal_tip: checkpoint.history_tip,
        };
        result.validate_loot_state()?;
        if result.state_digest() != checkpoint.digest {
            return Err(NativeError::InvalidCheckpoint);
        }
        Ok(result)
    }
    /// Replay verifies exact requests, conditional RNG draws, events and state digests.
    /// Records are replay material, not authenticated permission to mutate someone else's world.
    pub fn replay(&mut self, records: &[TickRecord]) -> Result<(), NativeError> {
        if records.len() > MAX_RECORDS
            || serde_json::to_vec(records)
                .map_err(|_| NativeError::InvalidCheckpoint)?
                .len()
                > MAX_BYTES
        {
            return Err(NativeError::TooLarge);
        }
        let mut next = self.clone();
        for expected in records {
            if expected.version != NATIVE_FORMAT_VERSION
                || expected.profile != next.profile_digest
                || expected.before != next.state_digest()
                || expected.previous_record != next.journal_tip
            {
                return Err(NativeError::ReplayMismatch);
            }
            let mut authority = ReplayAuthority {
                replies: &expected.replies,
                index: 0,
            };
            let inputs = expected
                .inputs
                .iter()
                .cloned()
                .map(|i| OwnedInput {
                    authenticated_owner: i.player.owner,
                    input: i,
                })
                .collect();
            let actual = next.step_batch(expected.tick, inputs, &mut authority)?;
            if actual != expected || authority.index != expected.replies.len() {
                return Err(NativeError::ReplayMismatch);
            }
        }
        *self = next;
        Ok(())
    }
}
struct ReplayAuthority<'a> {
    replies: &'a [AuthorityReply],
    index: usize,
}
impl Authority for ReplayAuthority<'_> {
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        if self.replies.get(self.index) != Some(reply) {
            return Err(NativeError::ReplayMismatch);
        }
        self.index += 1;
        Ok(())
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let r = self
            .replies
            .get(self.index)
            .filter(|r| r.request == *request)
            .ok_or(NativeError::ReplayMismatch)?
            .clone();
        self.index += 1;
        Ok(r)
    }
}

#[cfg(any(test, feature = "synthetic-native-tests"))]
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
