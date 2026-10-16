// SPDX-License-Identifier: GPL-3.0-or-later
//! Versioned integration of native-mechanic components. Production admission is closed.
//! OriginalFixture `World` v2 remains distinct. This module is not a full Classic server.
mod admin;
pub use admin::*;
mod combat;
mod creature_capabilities;
mod player_collision_height;
mod player_environment;
pub use player_environment::*;
#[cfg(test)]
mod player_environment_tests;
pub use player_collision_height::*;
mod creature_collision_height;
#[cfg(test)]
mod player_collision_height_tests;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub mod synthetic_player_model_fixture;
pub use creature_collision_height::*;
mod creature_collision_width;
pub use creature_collision_width::*;
#[cfg(test)]
mod creature_capability_tests;
#[cfg(test)]
mod creature_collision_height_tests;
#[cfg(test)]
mod creature_collision_width_tests;
#[cfg(test)]
mod route_owner_tests;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub mod synthetic_creature_model_fixture;
pub use creature_capabilities::*;
mod loot;
pub use loot::*;
mod engine;
pub mod environment;
mod npc_events;
mod npc_spell;
pub use npc_events::{NpcEventHandle, NpcEvents, NPC_EVENT_POLICY};
pub use npc_spell::*;
mod player_slow;
pub use player_slow::{
    PlayerSlowCasterSource, PlayerSlowCatalog, PlayerSlowEvent, PlayerSlowGenesis,
    PlayerSlowHolder, PlayerSlowHostSource, PlayerSlowRemoval, PlayerSlowSource, PlayerSlowState,
};
mod player_events;
pub use player_events::{
    FinishedSpellReason, PlayerEvents, SpellEventLifetime, PLAYER_EVENT_POLICY,
};
pub mod equipment;
pub use combat::*;
mod interactions;
mod model;
mod motion;
pub use interactions::{
    CombatAuthorityCatalog, CombatFacts, CombatPhase, CombatQuery, CombatRequest, CombatWitness,
    FacingNumericPolicy, InteractionAudit, InteractionCatalog, InteractionControlSeed,
    InteractionEvidence, InteractionFactionSource, InteractionInput, InteractionMoment,
    InteractionWitness, MeleeCombatFacts, MeleeEntryFacts, NpcCombatAuthoritySource,
    NpcInteractionSource, NpcSemanticFacts, NpcSpellSource, PlayerInteractionSource,
    SourceSpellFacts, SourceSpellPhase, SourceSpellQuery, SourceSpellRequest,
    SpellAuthorityCatalog, SpellAuthorityInput, SpellAuthoritySource, SpellCastTimeSource,
    SpellDurationSource, SpellGroupSource, SpellHookSources, SpellRangeResult, SpellRangeSource,
    SpellRankSource, SpellSessionSource, SpellSourceRows, INTERACTION_POLICY,
    SPELL_AUTHORITY_POLICY,
};
pub use motion::*;
#[cfg(test)]
mod interaction_tests;
mod orientation;
pub use orientation::Orientation;
mod persistence;
mod quest_progression;
pub use quest_progression::*;
mod rng;
use benilla_sim::{classic_spell as spell, creature_ai as ai, mage_resource as mana};
pub use model::*;
pub use persistence::{decode_records, encode_records, Checkpoint, CheckpointIdentity, TickRecord};
pub use rng::{Draw, DrawPurpose, RNG_POLICY};
use std::collections::BTreeMap;

pub const NATIVE_FORMAT_VERSION: u32 = 27;
pub const NATIVE_POLICY: &str = "benilla-native-owned-player-environment-v27";
// Keeps direct/periodic combat receipts under the AI component's 64-event bound.
pub const MAX_CASTS: usize = 16;
/// Flat, source-ordered local inputs admitted before one simulation update.
pub const MAX_INPUTS_PER_TICK: usize = 64;
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_AURAS: usize = 16;
pub const MAX_RECORDS: usize = 2048;
pub const MAX_RNG_WORDS_PER_TICK: u64 = 1_000_000;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const SOLO_UNIT: u64 = 1;
/// Deliberately uninhabited until a Rust raw-input compiler can prove every critical dependency.
/// No JSON preparation file, hash, test constructor or network packet grants this capability.
/// ```compile_fail
/// use benilla_world_runtime::native::NativeCapability;
/// let capability: NativeCapability = serde_json::from_str("{}").unwrap();
/// ```
pub enum NativeCapability {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeError {
    PlayerEnvironment(environment::EnvironmentError),
    PlayerSlow(benilla_sim::player_slow::SlowError),
    Inventory(benilla_sim::inventory::Error),
    Loot(benilla_sim::loot::Error),
    Quest(benilla_sim::quest_lifecycle::Error),
    Reputation(benilla_sim::reputation::Error),
    Progression(benilla_sim::player_progression::Error),
    CreatureRegeneration(benilla_sim::creature_regeneration::Error),
    InteractionRelation(benilla_sim::classic_targets::TargetUnsupported),
    InteractionEvidence {
        dependency: &'static str,
        observed: InteractionEvidence,
    },
    Facing(benilla_sim::facing::Error),
    SpellRange(benilla_sim::spell_range::Error),
    InvalidProfile,
    InvalidInput,
    InvalidState,
    InvalidCheckpoint,
    Incompatible,
    TooLarge,
    /// The current tick was not committed; checkpoint existing records before retrying it.
    JournalFull,
    Limit,
    AuthorityBinding,
    AuthorityUnavailable(&'static str),
    UnsupportedAbility(u32),
    UnknownTarget,
    Navigation(ai::AiError),
    Motion(benilla_sim::creature_motion::MotionError),
    Spell(spell::SpellError),
    EventProcessor(benilla_sim::event_processor::EventError),
    SpellScheduler(benilla_sim::spell_event::SpellEventError),
    ReplayMismatch,
    UnsupportedMechanic(&'static str),
    Equipment(equipment::EquipmentError),
    Initialization(benilla_sim::starter_init::InitError),
    Melee(benilla_sim::npc_melee::MeleeError),
    Death(benilla_sim::player_death::DeathError),
}
impl std::fmt::Display for NativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native world: {self:?}")
    }
}
impl std::error::Error for NativeError {}
impl From<benilla_sim::spell_range::Error> for NativeError {
    fn from(value: benilla_sim::spell_range::Error) -> Self {
        Self::SpellRange(value)
    }
}
impl From<spell::SpellError> for NativeError {
    fn from(e: spell::SpellError) -> Self {
        Self::Spell(e)
    }
}
impl From<ai::AiError> for NativeError {
    fn from(e: ai::AiError) -> Self {
        Self::Navigation(e)
    }
}

#[derive(Clone)]
pub struct NativeWorld {
    profile: Profile,
    profile_digest: crate::Digest,
    state: State,
    journal: Vec<TickRecord>,
    journal_bytes: usize,
    journal_tip: crate::Digest,
}
impl NativeWorld {
    pub fn from_verified_native(_profile: Profile, capability: NativeCapability) -> Self {
        match capability {}
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub fn synthetic_component_test(
        profile: Profile,
        world: crate::WorldId,
        player: crate::PlayerId,
        seed: u64,
    ) -> Result<Self, NativeError> {
        profile.validate()?;
        if profile.admin.as_ref().is_some_and(|p| p.world != world) {
            return Err(NativeError::InvalidProfile);
        }
        if world == [0; 16] || player.owner == [0; 16] || player.character == [0; 16] {
            return Err(NativeError::InvalidProfile);
        }
        let mut npcs = BTreeMap::new();
        for p in &profile.npcs {
            npcs.insert(
                p.ai.creature_id,
                Npc {
                    events: NpcEvents::new(
                        world,
                        crate::NpcId {
                            spawn: p.ai.creature_id,
                            generation: 0,
                        },
                        &profile,
                    )?,
                    source_motion: NpcMotion::new(&profile, p)?,
                    source_capabilities: CreatureCapabilityState::new(
                        &profile,
                        crate::NpcId {
                            spawn: p.ai.creature_id,
                            generation: 0,
                        },
                    )?,
                    source_collision_width: CreatureCollisionWidthState::new(
                        &profile,
                        crate::NpcId {
                            spawn: p.ai.creature_id,
                            generation: 0,
                        },
                    )?,
                    source_collision_height: CreatureCollisionHeightState::new(
                        &profile,
                        crate::NpcId {
                            spawn: p.ai.creature_id,
                            generation: 0,
                        },
                    )?,
                    regen_elapsed_ms: 0,
                    id: crate::NpcId {
                        spawn: p.ai.creature_id,
                        generation: 0,
                    },
                    original_loot_recipient: None,
                    corpse_loot: None,
                    health: p.max_health,
                    ai: ai::SoloCreatureAi::new(p.ai.clone())?,
                    auras: BTreeMap::new(),
                    aura_bindings: BTreeMap::new(),
                },
            );
        }
        let resources = mana::MageManaState::from_state(
            spell::CasterResources::new(profile.player.current_mana)?,
            profile.player.max_mana,
            profile.player.regen(),
            0,
            0,
        )?;
        let mut player_events = PlayerEvents::new(world, player);
        if profile.source_motion.is_some() {
            player_events.schedule_notify(0, false)?;
        }
        let state = State {
            source_player_environment: PlayerEnvironmentState::new(&profile, world, player)?,
            source_player_collision_height: PlayerCollisionHeightState::new(
                &profile, world, player,
            )?,
            admin: AdminState::default(),
            player_slow: profile.player_slow.as_ref().map(|_| PlayerSlowState::new()),
            player_movement_flags: profile
                .source_motion
                .as_ref()
                .map(|g| g.player_movement_flags),
            player_events,
            domain: Domain::SyntheticComponentTest,
            world,
            player,
            seed,
            tick: 0,
            next_sequence: 0,
            now_ms: profile.clock_origin_ms,
            position: profile.player.start,
            orientation: profile.player.orientation,
            health: profile.player.current_health,
            resources,
            npcs,
            casts: BTreeMap::new(),
            next_cast_id: 1,
            rng_counter: 0,
            quest_progression: profile
                .quest_progression
                .as_ref()
                .map(|g| QuestProgressionState::new(g, profile.clock_origin_ms))
                .transpose()?,
            combat: profile
                .combat
                .as_ref()
                .map(|g| PlayerCombatState::new(g, profile.clock_origin_ms))
                .transpose()?,
        };
        Ok(Self {
            profile_digest: profile.digest(),
            profile,
            state,
            journal: Vec::new(),
            journal_bytes: 2,
            journal_tip: [0; 32],
        })
    }
    pub fn profile(&self) -> &Profile {
        &self.profile
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn state_digest(&self) -> crate::Digest {
        crate::digest(b"benilla-native-state-v27\0", &self.checkpoint_state())
    }
    pub fn profile_digest(&self) -> crate::Digest {
        self.profile_digest
    }
    pub fn last_tick(&self) -> Option<&TickRecord> {
        self.journal.last()
    }
    /// Warn before either hard journal bound. A single large next frame may still require
    /// an explicit JournalFull checkpoint/retry; no simulation error is reclassified.
    pub fn checkpoint_due(&self) -> bool {
        self.journal.len() >= MAX_RECORDS / 2 || self.journal_bytes >= MAX_BYTES / 2
    }
    pub fn records(&self) -> &[TickRecord] {
        &self.journal
    }
}

#[cfg(any(test, feature = "synthetic-native-tests"))]
pub mod synthetic_fixture;
#[cfg(test)]
mod tests;

impl From<equipment::EquipmentError> for NativeError {
    fn from(e: equipment::EquipmentError) -> Self {
        Self::Equipment(e)
    }
}
impl From<benilla_sim::starter_init::InitError> for NativeError {
    fn from(e: benilla_sim::starter_init::InitError) -> Self {
        Self::Initialization(e)
    }
}
impl From<benilla_sim::npc_melee::MeleeError> for NativeError {
    fn from(e: benilla_sim::npc_melee::MeleeError) -> Self {
        Self::Melee(e)
    }
}

#[cfg(test)]
mod melee_tests;

impl From<benilla_sim::player_death::DeathError> for NativeError {
    fn from(e: benilla_sim::player_death::DeathError) -> Self {
        Self::Death(e)
    }
}

#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod death_tests;

impl From<benilla_sim::quest_lifecycle::Error> for NativeError {
    fn from(e: benilla_sim::quest_lifecycle::Error) -> Self {
        Self::Quest(e)
    }
}
impl From<benilla_sim::player_progression::Error> for NativeError {
    fn from(e: benilla_sim::player_progression::Error) -> Self {
        Self::Progression(e)
    }
}
impl NativeWorld {
    pub fn player_level(&self) -> u8 {
        self.state.level(&self.profile)
    }
    pub fn abilities(&self) -> &[Ability] {
        self.state.abilities(&self.profile)
    }
    pub fn ability(&self, id: u32) -> Option<&Ability> {
        self.state.ability(&self.profile, id)
    }
    pub fn ability_school(&self, id: u32) -> Option<u8> {
        self.profile
            .quest_progression
            .as_ref()
            .and_then(|g| g.school(id))
            .or_else(|| self.profile.ability(id).map(|a| a.school))
    }
}
#[cfg(test)]
mod quest_tests;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub mod synthetic_quest_fixture;

#[cfg(test)]
mod loot_tests;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub mod synthetic_loot_fixture;

#[cfg(test)]
mod quest_reward_tests;

#[cfg(test)]
mod orientation_tests;
impl From<benilla_sim::creature_regeneration::Error> for NativeError {
    fn from(e: benilla_sim::creature_regeneration::Error) -> Self {
        Self::CreatureRegeneration(e)
    }
}
impl From<benilla_sim::reputation::Error> for NativeError {
    fn from(e: benilla_sim::reputation::Error) -> Self {
        Self::Reputation(e)
    }
}
#[cfg(test)]
mod regeneration_tests;
#[cfg(test)]
mod reputation_tests;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub mod synthetic_reputation_fixture;

#[cfg(test)]
mod motion_tests;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub mod synthetic_motion_fixture;

impl From<benilla_sim::event_processor::EventError> for NativeError {
    fn from(error: benilla_sim::event_processor::EventError) -> Self {
        Self::EventProcessor(error)
    }
}
impl From<benilla_sim::spell_event::SpellEventError> for NativeError {
    fn from(error: benilla_sim::spell_event::SpellEventError) -> Self {
        Self::SpellScheduler(error)
    }
}
#[cfg(test)]
mod event_scheduler_tests;

#[cfg(test)]
mod combat_authority_tests;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub mod synthetic_combat_fixture;

#[cfg(test)]
mod spell_authority_tests;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub mod synthetic_spell_fixture;

impl From<benilla_sim::facing::Error> for NativeError {
    fn from(error: benilla_sim::facing::Error) -> Self {
        Self::Facing(error)
    }
}
#[cfg(test)]
mod facing_tests;

#[cfg(test)]
mod npc_event_tests;

#[cfg(test)]
mod player_slow_tests;
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub mod synthetic_slow_fixture;
impl From<benilla_sim::player_slow::SlowError> for NativeError {
    fn from(error: benilla_sim::player_slow::SlowError) -> Self {
        Self::PlayerSlow(error)
    }
}

#[cfg(test)]
mod npc_spell_tests;

#[cfg(test)]
mod admin_tests;

#[cfg(test)]
mod admin_item_tests;

#[cfg(test)]
mod inventory_move_tests;

#[cfg(test)]
mod spell_range_tests;
