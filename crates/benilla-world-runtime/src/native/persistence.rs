// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "synthetic-native-tests"))]
#[path = "wander_reopen.rs"]
mod wander_reopen;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub use wander_reopen::PendingWanderReopen;
#[cfg(any(test, feature = "synthetic-native-tests"))]
#[path = "dual_owner_reopen.rs"]
mod dual_owner_reopen;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub use dual_owner_reopen::PendingWanderAndChaseReopen;

/// One exact snapshot may be structurally restored only inside an unpublished
/// pending owner. This is not a native certificate or a public restore policy.
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(super) struct PendingMotionRestore {
    npc: crate::NpcId,
    snapshot: NpcMotionSnapshot,
}
#[cfg(any(test, feature = "synthetic-native-tests"))]
impl PendingMotionRestore {
    pub(super) fn matches(&self, npc: crate::NpcId, snapshot: &NpcMotionSnapshot) -> bool {
        self.npc == npc && self.snapshot == *snapshot
    }
}
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
        crate::digest(b"benilla-native-record-v72\0", self)
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
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    defense_reactive: Option<player_melee::NpcDefenseReactive>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    source_generation: Option<creature_generation::CreatureGenerationCheckpoint>,
    events: npc_events::NpcEventsSnapshot,
    source_motion: Option<NpcMotionSnapshot>,
    source_capabilities: Option<creature_capabilities::CreatureCapabilitySnapshot>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    source_collision_width: Option<creature_collision_width::CreatureCollisionWidthSnapshot>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    source_collision_height: Option<creature_collision_height::CreatureCollisionHeightSnapshot>,
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
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    source_player_appearance: Option<PlayerAppearanceState>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    source_splines: Option<SourceSplineIds>,
    action_bar_toggles: action_bar_toggles::ActionBarTogglesState,
    player_hide_flags: player_hide_flags::PlayerHideFlags,
    watched_faction: watched_faction::WatchedFactionState,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    player_melee: Option<player_melee::PlayerMeleeState>,
    initial_cast_genesis: InitialCastGenesis,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    source_location: Option<SourceLocationFacts>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    event_ai: Option<EventAiState>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    source_player_environment: Option<player_environment::PlayerEnvironmentSnapshot>,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    source_player_collision_height: Option<player_collision_height::PlayerCollisionHeightSnapshot>,
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
/// Bound aggregate replay before any source callback runs, including callers
/// passing a typed/deserialized checkpoint directly to restore.
const MAX_HISTORY_REPLAY_CALLS: u64 = 65_536;
impl StateCheckpoint {
    fn validate_history_budget(&self) -> Result<(), NativeError> {
        self.history_replay_work().map(|_| ())
    }
    fn validate_source_history_budget(&self, profile: &Profile) -> Result<(), NativeError> {
        let mut total = self.history_replay_work()?;
        // Raw launch validation additionally reconstructs historical spell crit.
        // Without that owner the callback returns before touching its prefix.
        // Charge a conservative second complete base pass only for this domain;
        // melee bindings separately include their unconditional physical replay.
        if profile
            .interactions
            .as_ref()
            .is_some_and(InteractionCatalog::has_raw_spell_history_replay)
        {
            total = total.checked_mul(2).ok_or(NativeError::TooLarge)?;
            if total > MAX_HISTORY_REPLAY_CALLS {
                return Err(NativeError::TooLarge);
            }
        }
        for npc in self.npcs.values() {
            match (&profile.creature_generations, &npc.source_generation) {
                (Some(catalog), Some(owner)) => {
                    let extra = owner
                        .source_replay_work(catalog)?
                        .checked_sub(owner.replay_work()?)
                        .ok_or(NativeError::TooLarge)?;
                    total = total.checked_add(extra).ok_or(NativeError::TooLarge)?;
                    if total > MAX_HISTORY_REPLAY_CALLS {
                        return Err(NativeError::TooLarge);
                    }
                }
                (None, None) => {}
                _ => return Err(NativeError::AuthorityBinding),
            }
        }
        Ok(())
    }
    fn history_replay_work(&self) -> Result<u64, NativeError> {
        if let Some(q) = &self.quest_progression {
            if q.derived_calls.iter().any(|call| {
                matches!(
                    call,
                    DerivedCall::ItemMembership { .. }
                        | DerivedCall::ItemRelocation { .. }
                        | DerivedCall::ItemAutoStore { .. }
                        | DerivedCall::ItemDestruction { .. }
                )
            }) && serde_json::to_vec(&(&q.training.calls, &q.derived_calls))
                .map_err(|_| NativeError::TooLarge)?
                .len()
                > MAX_BYTES / 2
            {
                return Err(NativeError::TooLarge);
            }
        }
        // Owner loading replays training in restore_inputs, state restore,
        // derived replay, and the final synchronize consistency check.
        let mut total = self
            .quest_progression
            .as_ref()
            .map_or(Some(0), |q| {
                (q.training.calls.len() as u64)
                    .checked_mul(5)
                    .and_then(|n| n.checked_add(2 * q.derived_calls.len() as u64))
            })
            .ok_or(NativeError::TooLarge)?;
        if let Some(q) = &self.quest_progression {
            for call in &q.derived_calls {
                if matches!(
                    call,
                    DerivedCall::ItemRelocation { .. }
                        | DerivedCall::ItemAutoStore { .. }
                        | DerivedCall::ItemDestruction { .. }
                ) {
                    total = total.checked_add(2).ok_or(NativeError::TooLarge)?;
                }
                if let DerivedCall::ItemMembership { movement, .. } = call {
                    total = total
                        .checked_add(movement.replay_work())
                        .and_then(|n| n.checked_add(2))
                        .ok_or(NativeError::TooLarge)?;
                }
            }
        }
        total = total
            .checked_add(
                self.event_ai
                    .as_ref()
                    .map_or(Ok(0), EventAiState::replay_work)?,
            )
            .ok_or(NativeError::TooLarge)?;
        let current_derived_calls = self
            .quest_progression
            .as_ref()
            .map_or(0, |q| q.derived_calls.len() as u64);
        if let Some(owner) = &self.player_melee {
            // Conservative current-policy scans in the bounded four-cause proof,
            // in addition to the prefix replays charged by each source binding.
            total = total
                .checked_add(owner.replay_work())
                .and_then(|n| {
                    current_derived_calls
                        .checked_mul(64)
                        .and_then(|work| n.checked_add(work))
                })
                .ok_or(NativeError::TooLarge)?;
        }
        if let Some(witness) = self.combat.as_ref().and_then(|c| c.melee_reset.as_ref()) {
            total = total
                .checked_add(witness.replay_work())
                .ok_or(NativeError::TooLarge)?;
        }
        for npc in self.npcs.values() {
            if let Some(reactive) = &npc.defense_reactive {
                total = total
                    .checked_add(reactive.replay_work())
                    .and_then(|n| {
                        current_derived_calls
                            .checked_mul(8)
                            .and_then(|work| n.checked_add(work))
                    })
                    .ok_or(NativeError::TooLarge)?;
            }
            if let Some(assistance) = &npc.events.assistance {
                total = total
                    .checked_add(assistance.replay_work()?)
                    .ok_or(NativeError::TooLarge)?;
            }
            if let Some(owner) = &npc.source_generation {
                total = total
                    .checked_add(owner.replay_work()?)
                    .ok_or(NativeError::TooLarge)?;
                if total > MAX_HISTORY_REPLAY_CALLS {
                    return Err(NativeError::TooLarge);
                }
            }
        }
        for binding in self
            .casts
            .values()
            .flat_map(|c| std::iter::once(&c.binding).chain(c.release_binding.as_ref()))
            .chain(self.npcs.values().flat_map(|n| n.aura_bindings.values()))
        {
            total = total
                .checked_add(binding.replay_work())
                .ok_or(NativeError::TooLarge)?;
            if total > MAX_HISTORY_REPLAY_CALLS {
                return Err(NativeError::TooLarge);
            }
        }
        if total > MAX_HISTORY_REPLAY_CALLS {
            return Err(NativeError::TooLarge);
        }
        Ok(total)
    }
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

    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn validate_encoded_budget(&self) -> Result<(), NativeError> {
        struct Counter(usize);
        impl std::io::Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .filter(|n| *n <= MAX_BYTES)
                    .ok_or_else(|| std::io::Error::other("native checkpoint byte budget"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        serde_json::to_writer(&mut Counter(0), self).map_err(|_| NativeError::TooLarge)
    }
    pub fn encode(&self) -> Result<Vec<u8>, NativeError> {
        self.state.validate_history_budget()?;
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
            || s.digest != crate::digest(b"benilla-native-state-v72\0", &s.state)
            || s.checksum
                != crate::digest(
                    b"benilla-native-checkpoint-v72\0",
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
        player_melee: state.player_melee.clone(),
        source_splines: state.source_splines.clone(),
        watched_faction: state.watched_faction.clone(),
        action_bar_toggles: state.action_bar_toggles.clone(),
        player_hide_flags: state.player_hide_flags.clone(),
        initial_cast_genesis: state.initial_cast_genesis.clone(),
        source_location: state.source_location.clone(),
        event_ai: state.event_ai.clone(),
        source_player_environment: state
            .source_player_environment
            .as_ref()
            .map(PlayerEnvironmentState::snapshot),
        source_player_appearance: state.source_player_appearance.clone(),
        source_player_collision_height: state
            .source_player_collision_height
            .as_ref()
            .map(PlayerCollisionHeightState::snapshot),
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
                        defense_reactive: n.defense_reactive.clone(),
                        source_generation: n
                            .source_generation
                            .as_ref()
                            .map(CreatureGenerationOwner::checkpoint),
                        events: n.events.snapshot(),
                        source_motion: n.source_motion.as_ref().map(NpcMotion::snapshot),
                        source_capabilities: n
                            .source_capabilities
                            .as_ref()
                            .map(CreatureCapabilityState::snapshot),
                        source_collision_width: n
                            .source_collision_width
                            .as_ref()
                            .map(CreatureCollisionWidthState::snapshot),
                        source_collision_height: n
                            .source_collision_height
                            .as_ref()
                            .map(CreatureCollisionHeightState::snapshot),
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
    /// Reject retained native cache state whose loaded query cannot be rehydrated.
    /// Saved-session callers must check the final world after journal replay;
    /// exact in-memory replay alone does not establish native query ownership.
    pub fn validate_saved_session_reopen(&self) -> Result<(), NativeError> {
        for npc in self.state.npcs.values() {
            if let Some(source) = &npc.source_motion {
                source.snapshot().validate_saved_session_reopen()?;
            }
        }
        Ok(())
    }
    pub(super) fn make_checkpoint(
        &self,
        state: &State,
        history_tip: crate::Digest,
    ) -> Result<Checkpoint, NativeError> {
        let snapshot = self.snapshot_of(state);
        snapshot.validate_source_history_budget(&self.profile)?;
        state
            .player_hide_flags
            .validate(&self.profile, state.next_sequence)?;
        SourceSplineIds::validate(&self.profile, state)?;
        quest_progression::equipment_movement::validate_state(&self.profile, state)?;
        if self.profile.player_melee.is_some() {
            player_melee::validate_current(&self.profile, state)?;
        }
        Npc::validate_defense_reactives(&self.profile, state)?;
        initial_cast_genesis::validate_state(&self.profile, state)?;
        source_location::validate_current(&self.profile, state)?;
        PlayerAppearanceState::validate_optional(
            state.source_player_appearance.as_ref(),
            &self.profile,
            state.world,
            state.player,
        )?;
        PlayerCollisionHeightState::validate_optional(
            state.source_player_collision_height.as_ref(),
            &self.profile,
            state.world,
            state.player,
        )?;
        PlayerEnvironmentState::validate_optional(
            state.source_player_environment.as_ref(),
            &self.profile,
            state,
        )?;
        EventAiState::validate_optional(&self.profile, state)?;
        combat_assistance::validate_current(&self.profile, state)?;
        let state = snapshot;
        let digest = crate::digest(b"benilla-native-state-v72\0", &state);
        let s = Checkpoint {
            version: NATIVE_FORMAT_VERSION,
            state,
            digest,
            history_tip,
            checksum: [0; 32],
        };
        let mut s = s;
        s.checksum = crate::digest(
            b"benilla-native-checkpoint-v72\0",
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
        Self::restore_synthetic_checkpoint(profile, checkpoint, None)
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn restore_synthetic_checkpoint(
        profile: Profile,
        checkpoint: Checkpoint,
        pending_motion: Option<&PendingMotionRestore>,
    ) -> Result<Self, NativeError> {
        checkpoint.state.validate_source_history_budget(&profile)?;
        profile.validate()?;
        profile.require_realizable_initial_casts()?;
        // Typed callers may bypass decode. Bound work and encoded size before
        // hashing or replay, without materializing another full JSON buffer.
        checkpoint.state.validate_history_budget()?;
        checkpoint.validate_encoded_budget()?;
        let profile_digest = profile.digest();
        if checkpoint.version != NATIVE_FORMAT_VERSION
            || checkpoint.digest != crate::digest(b"benilla-native-state-v72\0", &checkpoint.state)
            || checkpoint.checksum
                != crate::digest(
                    b"benilla-native-checkpoint-v72\0",
                    &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
                )
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        let s = checkpoint.state;
        if s.version != NATIVE_FORMAT_VERSION
            || s.policy != NATIVE_POLICY
            || s.rng_policy != RNG_POLICY
            || s.profile != profile_digest
            || s.domain != Domain::SyntheticComponentTest
        {
            return Err(NativeError::Incompatible);
        }
        s.initial_cast_genesis
            .verify(&profile, s.world, s.player, s.seed)?;
        // The full ordered creature genesis prefix is checked after owners are restored.
        if s.world == [0; 16]
            || s.player.owner == [0; 16]
            || s.player.character == [0; 16]
            || s.tick > crate::MAX_TICK
            || s.now_ms
                != s.tick
                    .checked_mul(u64::from(profile.tick_ms))
                    .and_then(|t| t.checked_add(profile.clock_origin_ms))
                    .ok_or(NativeError::InvalidCheckpoint)?
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
        let (abilities, combat_genesis, replayed_recipe) = match (
            &profile.quest_progression,
            &s.quest_progression,
            &profile.combat,
        ) {
            (Some(g), Some(q), Some(c)) => {
                let (a, c, recipe) = g.restore_inputs(q, c)?;
                (a, Some(c), Some(recipe))
            }
            (None, None, c) => (profile.abilities.clone(), c.clone(), None),
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
        let restore_combat = if profile.player_melee.is_some() {
            PlayerCombatState::restore_selected_fields
        } else {
            PlayerCombatState::restore
        };
        let combat = match (&combat_genesis, s.combat) {
            (Some(g), Some(c)) => Some(match replayed_recipe {
                Some(recipe) => PlayerCombatState::restore_replayed_fields(
                    g,
                    c,
                    recipe,
                    s.health,
                    caster.mana,
                    s.now_ms,
                    s.tick,
                    profile.clock_origin_ms,
                    profile.tick_ms,
                    profile.player_melee.is_some(),
                )?,
                None => restore_combat(
                    g,
                    c,
                    s.health,
                    caster.mana,
                    s.now_ms,
                    s.tick,
                    profile.clock_origin_ms,
                    profile.tick_ms,
                )?,
            }),
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
        let offline_targets = profile
            .source_combat_authority()
            .then(|| profile.source_player_offline_targets())
            .transpose()?;
        let mut npcs = BTreeMap::new();
        for (id, n) in s.npcs {
            let base = profile.npc(id).ok_or(NativeError::InvalidCheckpoint)?;
            let source_generation = match (&profile.creature_generations, n.source_generation) {
                (None, None) => None,
                (Some(catalog), Some(snapshot)) => Some(CreatureGenerationOwner::restore(
                    catalog,
                    snapshot,
                    s.world,
                    profile_digest,
                    n.id,
                    s.seed,
                    s.tick,
                    s.rng_counter,
                    profile.tick_ms,
                    profile.clock_origin_ms,
                    base,
                    Some(&profile),
                )?),
                _ => return Err(NativeError::InvalidCheckpoint),
            };
            let projected = match &source_generation {
                Some(owner) => owner.project_profile(base)?,
                None => base.clone(),
            };
            let p = &projected;
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
            let ai = if source_generation.is_some() {
                ai::SoloCreatureAi::restore_generation_owned(
                    n.ai,
                    p.ai.clone(),
                    s.now_ms,
                    s.tick,
                    SOLO_UNIT,
                    n.id.generation,
                )?
            } else {
                ai::SoloCreatureAi::restore(n.ai, p.ai.clone(), s.now_ms, s.tick, SOLO_UNIT)?
            };
            ai.validate_fixed_step(profile.tick_ms)?;
            // The selected native source domain owns the available player and excludes
            // feign-changing hooks. Generic AI offline/unresolved links are not reachable here.
            if let Some(offline_targets) = &offline_targets {
                if let Some(reference) = ai.hostile_reference() {
                    if !reference.valid()
                        || reference.online()
                            == *offline_targets
                                .get(&n.id.spawn)
                                .ok_or(NativeError::InvalidCheckpoint)?
                    {
                        return Err(NativeError::InvalidCheckpoint);
                    }
                }
            }

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
                binding.validate(
                    &profile,
                    quest_progression.as_ref(),
                    s.now_ms,
                    (profile_digest, s.domain, s.world, s.player),
                )?;
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
                .map(|m| {
                    if let Some(pending) = pending_motion.filter(|p| p.npc == n.id) {
                        return NpcMotion::restore_pending_motion(
                            m,
                            &profile,
                            &ai,
                            s.now_ms,
                            n.id,
                            source_generation.as_ref(),
                            pending,
                        );
                    }
                    match &source_generation {
                        Some(owner) => {
                            NpcMotion::restore_generation(m, &profile, &ai, s.now_ms, n.id, owner)
                        }
                        None => NpcMotion::restore(m, &profile, &ai, s.now_ms, n.id),
                    }
                })
                .transpose()?;
            npcs.insert(
                id,
                Npc {
                    defense_reactive: n.defense_reactive,
                    source_generation: source_generation.clone(),
                    events: NpcEvents::restore(
                        n.events, s.world, n.id, s.player, &profile, &ai, s.tick,
                    )?,
                    source_motion,
                    source_capabilities: CreatureCapabilityState::restore(
                        n.source_capabilities,
                        &profile,
                        n.id,
                        source_generation.as_ref(),
                    )?,
                    source_collision_width: match &source_generation {
                        Some(owner) => CreatureCollisionWidthState::restore_generation(
                            n.source_collision_width,
                            &profile,
                            n.id,
                            owner,
                        )?,
                        None => CreatureCollisionWidthState::restore(
                            n.source_collision_width,
                            &profile,
                            n.id,
                        )?,
                    },
                    source_collision_height: match &source_generation {
                        Some(owner) => CreatureCollisionHeightState::restore_generation(
                            n.source_collision_height,
                            &profile,
                            n.id,
                            owner,
                        )?,
                        None => CreatureCollisionHeightState::restore(
                            n.source_collision_height,
                            &profile,
                            n.id,
                        )?,
                    },
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
            c.binding.validate(
                &profile,
                quest_progression.as_ref(),
                s.now_ms,
                (profile_digest, s.domain, s.world, s.player),
            )?;
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
                    released.validate(
                        &profile,
                        quest_progression.as_ref(),
                        s.now_ms,
                        (profile_digest, s.domain, s.world, s.player),
                    )?;
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
                        &profile,
                        quest_progression.as_ref(),
                        npcs.get(&c.target.spawn)
                            .ok_or(NativeError::InvalidCheckpoint)?,
                        released,
                        c.target,
                        &launch,
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
        s.watched_faction.validate(s.next_sequence)?;
        s.action_bar_toggles.validate(&profile, s.next_sequence)?;
        s.player_hide_flags.validate(&profile, s.next_sequence)?;
        let state = State {
            source_splines: s.source_splines,
            watched_faction: s.watched_faction,
            action_bar_toggles: s.action_bar_toggles,
            player_hide_flags: s.player_hide_flags,
            initial_cast_genesis: s.initial_cast_genesis,
            player_melee: s.player_melee,
            source_location: s.source_location,
            event_ai: s.event_ai,
            source_player_environment: PlayerEnvironmentState::restore(
                s.source_player_environment,
                &profile,
            )?,
            source_player_appearance: PlayerAppearanceState::restore(
                s.source_player_appearance,
                &profile,
                s.world,
                s.player,
            )?,
            source_player_collision_height: PlayerCollisionHeightState::restore(
                s.source_player_collision_height,
                &profile,
                s.world,
                s.player,
            )?,
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
        SourceSplineIds::validate(&profile, &state)?;
        quest_progression::equipment_movement::validate_state(&profile, &state)?;
        Npc::validate_defense_reactives(&profile, &state)?;
        initial_cast_genesis::validate_state(&profile, &state)?;
        PlayerEnvironmentState::validate_optional(
            state.source_player_environment.as_ref(),
            &profile,
            &state,
        )
        .map_err(|_| NativeError::InvalidCheckpoint)?;
        source_location::validate_current(&profile, &state)
            .map_err(|_| NativeError::InvalidCheckpoint)?;
        player_melee::validate_weapon_history(&profile, &state)?;
        melee_reset::validate_restored(&profile, &state)?;
        EventAiState::validate_optional(&profile, &state)?;
        combat_assistance::validate_current(&profile, &state)?;
        state.admin.validate(&profile, &state)?;
        state.player_events.validate(&state, &profile)?;
        if let Some(slow) = &state.player_slow {
            slow.validate(&state, &profile)?;
            slow.validate_npc_spell_holders(&state, &profile)?;
        }
        let result = Self {
            profile_digest,
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

#[cfg(test)]
mod history_budget_tests {
    use super::*;
    use crate::native::synthetic_quest_fixture as f;

    #[test]
    fn aggregate_prefix_work_rejects_encode_and_typed_restore_before_replay() {
        let mut world = f::world();
        let command = f::command(
            &world,
            Command::Cast {
                ability: 60001,
                target: world.state.npcs[&2].id,
            },
        );
        world
            .step(1, Some(command), &mut f::QuestAuthority::default())
            .unwrap();
        let mut cp = world.checkpoint().unwrap();
        assert_eq!(world.state.casts[&1].binding.replay_work(), 64);
        let mut cast = serde_json::to_value(&cp.state.casts[&1]).unwrap();
        for kind in ["training_prefix", "derived_prefix"] {
            cast["binding"]["progression"][kind]["count"] = serde_json::json!(4096);
        }
        let cast: CastState = serde_json::from_value(cast).unwrap();
        cp.state.casts = (1..=9).map(|id| (id, cast.clone())).collect();
        cp.digest = crate::digest(b"benilla-native-state-v72\0", &cp.state);
        cp.checksum = crate::digest(
            b"benilla-native-checkpoint-v72\0",
            &(&cp.state, cp.digest, cp.history_tip),
        );
        assert_eq!(cp.encode().unwrap_err(), NativeError::TooLarge);
        // Direct serde decoding bypasses Checkpoint::decode. Typed restore must
        // still check aggregate work before attempting any invalid prefix.
        let bytes = serde_json::to_vec(&cp).unwrap();
        let typed: Checkpoint = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            NativeWorld::restore_synthetic_component_test(world.profile.clone(), typed).err(),
            Some(NativeError::TooLarge)
        );
        assert_eq!(
            Checkpoint::decode(&bytes).err(),
            Some(NativeError::TooLarge)
        );
    }
    #[test]
    fn typed_restore_cannot_bypass_the_unchanged_eight_mib_envelope() {
        let world = f::world();
        let cp = world.checkpoint().unwrap();
        cp.validate_encoded_budget().unwrap();
        let mut oversized = cp;
        oversized.state.policy = "x".repeat(MAX_BYTES);
        assert_eq!(
            oversized.validate_encoded_budget(),
            Err(NativeError::TooLarge)
        );
        assert_eq!(oversized.encode().unwrap_err(), NativeError::TooLarge);
        assert_eq!(
            NativeWorld::restore_synthetic_component_test(world.profile.clone(), oversized).err(),
            Some(NativeError::TooLarge)
        );
    }
}
