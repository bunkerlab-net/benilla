// SPDX-License-Identifier: GPL-3.0-or-later
//! Versioned integration of native-mechanic components. Production admission is closed.
//! OriginalFixture `World` v2 remains distinct. This module is not a full Classic server.
mod combat;
mod engine;
pub mod equipment;
pub use combat::*;
mod model;
mod persistence;
mod quest_progression;
pub use quest_progression::*;
mod rng;
use benilla_sim::{classic_spell as spell, creature_ai as ai, mage_resource as mana};
pub use model::*;
pub use persistence::{decode_records, encode_records, Checkpoint, CheckpointIdentity, TickRecord};
pub use rng::{Draw, DrawPurpose, RNG_POLICY};
use std::collections::BTreeMap;

pub const NATIVE_FORMAT_VERSION: u32 = 5;
pub const NATIVE_POLICY: &str = "benilla-native-quest-progression-reconstruction-v5";
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
    Quest(benilla_sim::quest_lifecycle::Error),
    Progression(benilla_sim::player_progression::Error),
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
    Spell(spell::SpellError),
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
        if world == [0; 16] || player.owner == [0; 16] || player.character == [0; 16] {
            return Err(NativeError::InvalidProfile);
        }
        let mut npcs = BTreeMap::new();
        for p in &profile.npcs {
            npcs.insert(
                p.ai.creature_id,
                Npc {
                    id: crate::NpcId {
                        spawn: p.ai.creature_id,
                        generation: 0,
                    },
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
        let state = State {
            domain: Domain::SyntheticComponentTest,
            world,
            player,
            seed,
            tick: 0,
            next_sequence: 0,
            now_ms: profile.clock_origin_ms,
            position: profile.player.start,
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
        crate::digest(b"benilla-native-state-v5\0", &self.checkpoint_state())
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
