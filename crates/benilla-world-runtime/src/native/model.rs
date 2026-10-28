// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{Digest, NpcId, PlayerId, WorldId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Domain {
    SyntheticComponentTest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerProfile {
    pub start: ai::Point,
    pub orientation: Orientation,
    pub level: u8,
    pub max_health: u32,
    pub max_mana: u32,
    pub current_health: u32,
    pub current_mana: u32,
    pub combat_reach: ai::Real,
    pub bounding_radius: ai::Real,
    pub normal_mana_per_second: spell::SpellFloat,
    pub interrupted_mana_per_second: spell::SpellFloat,
    pub world_mana_rate: spell::SpellFloat,
    pub mana_exclusions: ManaExclusions,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManaExclusions {
    pub cast_item: bool,
    pub triggered_by_aura: bool,
    pub dont_block_mana_regen: bool,
    pub other_mana_mutation: bool,
}
impl From<ManaExclusions> for mana::ManaUseExclusions {
    fn from(x: ManaExclusions) -> Self {
        Self {
            cast_item: x.cast_item,
            triggered_by_aura: x.triggered_by_aura,
            dont_block_mana_regen: x.dont_block_mana_regen,
            other_mana_mutation: x.other_mana_mutation,
        }
    }
}
impl PlayerProfile {
    pub fn regen(&self) -> mana::ManaRegen {
        mana::ManaRegen {
            normal_per_second: self.normal_mana_per_second,
            interrupted_per_second: self.interrupted_mana_per_second,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ability {
    pub id: u32,
    pub school: u8,
    pub admission: AbilityAdmission,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbilityAdmission {
    Available(spell::MageProjectileSpec),
    /// Known learned active remains visible; its precise unsupported reason is preserved.
    Unavailable {
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcProfile {
    pub health_regeneration: benilla_sim::creature_regeneration::Policy,
    pub combat: Option<CreatureCombatProfile>,
    pub ai: ai::Profile,
    pub entry: u32,
    pub display_id: u32,
    pub faction_template: u32,
    pub name: String,
    pub max_health: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Profile {
    pub player_melee: Option<PlayerMeleePolicy>,
    pub creature_generations: Option<CreatureGenerationCatalog>,
    pub admin: Option<AdminPolicy>,
    pub player_slow: Option<PlayerSlowGenesis>,
    pub source_motion: Option<MotionGenesis>,
    pub source_visitors: Option<SourceVisitorCatalog>,
    pub creature_capabilities: Option<CreatureCapabilityCatalog>,
    pub creature_collision_widths: Option<CreatureCollisionWidthCatalog>,
    pub creature_collision_heights: Option<CreatureCollisionHeightCatalog>,
    pub player_collision_height: Option<PlayerCollisionHeightSource>,
    pub player_environment: Option<PlayerEnvironmentSource>,
    pub pathfinder_configuration: Option<PathfinderConfigurationSource>,
    pub interactions: Option<InteractionCatalog>,
    pub quest_progression: Option<QuestProgressionGenesis>,
    pub combat: Option<CombatGenesis>,
    pub content_digest: Digest,
    pub authority_policy: Digest,
    pub geometry_revision: Digest,
    pub tick_ms: u32,
    pub clock_origin_ms: u64,
    pub map_id: u32,
    pub player: PlayerProfile,
    pub abilities: Vec<Ability>,
    pub npcs: Vec<NpcProfile>,
}
impl Profile {
    pub fn digest(&self) -> Digest {
        crate::digest(
            b"benilla-native-profile-v65\0",
            &(
                NATIVE_POLICY,
                NPC_SPELL_POLICY,
                ADMIN_POLICY,
                benilla_sim::player_slow::PLAYER_SLOW_POLICY,
                RNG_POLICY,
                spell::SPELL_EXECUTION_POLICY_ID,
                ai::CREATURE_AI_POLICY_ID,
                benilla_sim::creature_motion::CREATURE_MOTION_POLICY_ID,
                benilla_sim::creature_regeneration::CREATURE_REGENERATION_POLICY_ID,
                mana::MAGE_RESOURCE_POLICY_ID,
                benilla_sim::starter_init::STARTER_INIT_POLICY_ID,
                benilla_sim::npc_melee::NPC_MELEE_POLICY_ID,
                equipment::EQUIPMENT_POLICY,
                benilla_sim::player_death::PLAYER_DEATH_POLICY,
                self,
            ),
        )
    }
    pub fn ability(&self, id: u32) -> Option<&Ability> {
        self.abilities
            .binary_search_by_key(&id, |a| a.id)
            .ok()
            .map(|i| &self.abilities[i])
    }
    pub fn npc(&self, id: u64) -> Option<&NpcProfile> {
        self.npcs
            .binary_search_by_key(&id, |n| n.ai.creature_id)
            .ok()
            .map(|i| &self.npcs[i])
    }
    /// Exact configured owner lookup; it does not authenticate a generation.
    pub fn npc_motion(&self, spawn: u64) -> Result<&NpcMotionGenesis, NativeError> {
        self.npc(spawn).ok_or(NativeError::InvalidProfile)?;
        self.source_motion
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?
            .npc(spawn)
    }
    pub fn specs(&self) -> Vec<spell::MageProjectileSpec> {
        self.abilities
            .iter()
            .filter_map(|a| match &a.admission {
                AbilityAdmission::Available(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }
    pub fn validate(&self) -> Result<(), NativeError> {
        if let Some(policy) = &self.player_melee {
            policy.validate(self)?;
        }
        if let Some(catalog) = &self.creature_generations {
            if self.player_slow.is_some() {
                return Err(NativeError::UnsupportedMechanic(
                    "generation-owned player slow slot residue history",
                ));
            }
            catalog.validate(self)?;
            creature_generation_binding::validate_other_sources(self)?;
        }
        if let Some(combat) = &self.combat {
            combat.validate_melee_genesis()?;
            // Draft preparation may lack a seal; an installed seal must remain
            // consistent when later catalogs or source snapshots are selected.
            if let Some(seal) = &combat.initial_cast_source {
                seal.validate(self)?;
            }
        }
        if let Some(policy) = &self.admin {
            policy.validate()?;
            if self.quest_progression.is_none() {
                return Err(NativeError::InvalidProfile);
            }
        }
        if let Some(g) = &self.player_slow {
            g.validate(self)?;
        }
        if let Some(g) = &self.source_motion {
            g.validate(self)?;
        }
        if let Some(catalog) = &self.source_visitors {
            catalog.validate(self)?;
        }
        if let Some(catalog) = &self.creature_capabilities {
            catalog.validate(self)?;
        }
        if let Some(catalog) = &self.creature_collision_widths {
            catalog.validate(self)?;
        }
        if let Some(catalog) = &self.creature_collision_heights {
            catalog.validate(self)?;
        }
        if let Some(source) = &self.player_collision_height {
            source.validate(self)?;
        }
        if let Some(source) = &self.player_environment {
            source.validate(self)?;
        }
        if let Some(source) = &self.pathfinder_configuration {
            source.validate(self)?;
        }
        let p = &self.player;
        if self.content_digest == [0; 32]
            || self.authority_policy == [0; 32]
            || self.geometry_revision == [0; 32]
            || self.tick_ms == 0
            || self.tick_ms > 1000
            || !self.clock_origin_ms.is_multiple_of(1000)
            || p.current_health == 0
            || p.current_health > p.max_health
            || p.current_mana > p.max_mana
            || p.level == 0
            || p.max_health == 0
            || p.max_health > 1_000_000
            || p.max_mana > 1_000_000
            || self.abilities.len() > 64
            || self.npcs.len() > 4096
            || self.abilities.windows(2).any(|w| w[0].id >= w[1].id)
            || self
                .npcs
                .windows(2)
                .any(|w| w[0].ai.creature_id >= w[1].ai.creature_id)
        {
            return Err(NativeError::InvalidProfile);
        }
        p.start.distance_squared(p.start)?;
        ai::combat_reach(p.combat_reach, p.bounding_radius)?;
        mana::MageManaState::from_state(
            spell::CasterResources::new(p.max_mana)?,
            p.max_mana,
            p.regen(),
            0,
            0,
        )?
        .after_spell_and_aura_events(
            mana::ManaFrame {
                diff_ms: 0,
                in_world: true,
                alive_after_events: true,
                world_mana_rate: p.world_mana_rate,
                exclusions: p.mana_exclusions.into(),
            },
            vec![],
        )?;
        for a in &self.abilities {
            a.validate(self.tick_ms)?;
        }
        if let Some(g) = &self.combat {
            if p.level != 1
                || p.current_health != g.initial_health
                || p.current_mana != g.initial_mana
                || self.abilities != g.admitted_abilities
                || g.skills.len() > 128
                || p.max_health != g.stats.max_health
                || p.max_mana != g.stats.max_mana
                || p.regen() != g.stats.regen()
                || self.npcs.iter().any(|n| n.combat.is_none())
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        if let Some(g) = &self.quest_progression {
            g.validate(self)?;
        }
        if let Some(interactions) = &self.interactions {
            interactions.validate(self)?;
        }
        for n in &self.npcs {
            if n.ai.creature_id <= SOLO_UNIT
                || n.entry == 0
                || n.entry > 0xff_ffff
                || n.display_id == 0
                || n.name.is_empty()
                || n.name.len() > 256
                || n.max_health == 0
                || n.max_health > 1_000_000
                || n.ai.map_id != self.map_id
                || n.ai.clock_origin_ms != self.clock_origin_ms
                || n.ai.content_revision.0 != self.content_digest
            {
                return Err(NativeError::InvalidProfile);
            }
            n.health_regeneration.validate()?;
            ai::SoloCreatureAi::new(n.ai.clone())?;
            if let (Some(g), Some(c)) = (&self.combat, &n.combat) {
                benilla_sim::npc_melee::prepare(benilla_sim::npc_melee::MeleeInput {
                    attacker: c.attacker(n.ai.level),
                    defender: benilla_sim::npc_melee::MageDefense {
                        level: p.level,
                        agility: g.stats.attributes[1] as u32,
                        defense_skill: g
                            .skills
                            .get(&combat::DEFENSE_SKILL)
                            .map(SkillState::current_value)
                            .unwrap_or(0),
                        armor: g.stats.resistances[0],
                        standing: true,
                        casting_non_melee: false,
                        has_channeled_spell: false,
                        has_auto_repeat_spell: false,
                    },
                    health: p.current_health,
                    front_facing: true,
                })?;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cast {
    /// Immutable prepare-time source witness, including the original spell ID.
    pub binding: ExecutionBinding,
    pub release_binding: Option<ExecutionBinding>,
    pub id: u64,
    pub ability: u32,
    pub target: NpcId,
    pub origin: ai::Point,
    pub execution: spell::MageProjectileCast,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Npc {
    pub(in crate::native) defense_reactive: Option<player_melee::NpcDefenseReactive>,
    pub(super) source_generation: Option<CreatureGenerationOwner>,
    pub events: NpcEvents,
    pub source_motion: Option<NpcMotion>,
    pub(super) source_capabilities: Option<CreatureCapabilityState>,
    pub(super) source_collision_width: Option<CreatureCollisionWidthState>,
    pub(super) source_collision_height: Option<CreatureCollisionHeightState>,
    pub regen_elapsed_ms: u32,
    pub original_loot_recipient: Option<PlayerId>,
    pub corpse_loot: Option<CorpseLoot>,
    pub aura_bindings: BTreeMap<u32, ExecutionBinding>,
    pub id: NpcId,
    pub health: u32,
    pub ai: ai::SoloCreatureAi,
    pub auras: BTreeMap<u32, spell::DamageTarget>,
}
impl Npc {
    /// Completed-frame NPC defense state, after this owner's Unit update.
    /// This is not a pre-update countdown or a source eligibility capability.
    pub fn defense_reactive_active(&self, now_ms: u64, tick_ms: u32) -> Result<bool, NativeError> {
        match &self.defense_reactive {
            Some(reactive) if self.health != 0 => Ok(reactive.remaining(now_ms, tick_ms)? > 0),
            Some(_) => Err(NativeError::InvalidState),
            None => Ok(false),
        }
    }
    pub(in crate::native) fn validate_defense_reactives(
        profile: &Profile,
        state: &State,
    ) -> Result<(), NativeError> {
        for npc in state.npcs.values() {
            if let Some(reactive) = &npc.defense_reactive {
                if profile.player_melee.is_none()
                    || reactive.remaining(state.now_ms, profile.tick_ms)? == 0
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                reactive.validate(profile, state, npc.id)?;
            }
        }
        Ok(())
    }
    /// A retained cast or event uses the source life that actually produced it.
    pub fn historical_profile(
        &self,
        profile: &Profile,
        id: crate::NpcId,
    ) -> Result<NpcProfile, NativeError> {
        if id.spawn != self.id.spawn || id.generation > self.id.generation {
            return Err(NativeError::AuthorityBinding);
        }
        let base = profile.npc(id.spawn).ok_or(NativeError::UnknownTarget)?;
        match (&profile.creature_generations, &self.source_generation) {
            (None, None) => Ok(base.clone()),
            (Some(_), Some(owner)) if owner.npc_id() == self.id => owner
                .historical(id)
                .ok_or(NativeError::AuthorityBinding)?
                .project_profile(base),
            _ => Err(NativeError::AuthorityBinding),
        }
    }
    pub fn historical_initialized_creature(
        &self,
        id: crate::NpcId,
    ) -> Option<&benilla_sim::creature_init::InitializedCreature> {
        if id.spawn != self.id.spawn || id.generation > self.id.generation {
            return None;
        }
        self.source_generation
            .as_ref()
            .filter(|owner| owner.npc_id() == self.id)
            .and_then(|owner| owner.historical(id))
            .map(CreatureGeneration::result)
    }
    pub fn initialized_creature(&self) -> Option<&benilla_sim::creature_init::InitializedCreature> {
        self.source_generation
            .as_ref()
            .filter(|owner| owner.npc_id() == self.id)
            .map(CreatureGenerationOwner::result)
    }
    /// Current-life source projection; the immutable profile is only the static owner.
    pub fn effective_profile(&self, profile: &Profile) -> Result<NpcProfile, NativeError> {
        let base = profile
            .npc(self.id.spawn)
            .ok_or(NativeError::UnknownTarget)?;
        match (&profile.creature_generations, &self.source_generation) {
            (None, None) => Ok(base.clone()),
            (Some(_), Some(owner)) if owner.npc_id() == self.id => owner.project_profile(base),
            _ => Err(NativeError::AuthorityBinding),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub(super) action_bar_toggles: action_bar_toggles::ActionBarTogglesState,
    pub(super) watched_faction: watched_faction::WatchedFactionState,
    pub(super) player_melee: Option<player_melee::PlayerMeleeState>,
    pub initial_cast_genesis: InitialCastGenesis,
    pub(super) source_location: Option<SourceLocationFacts>,
    pub(super) event_ai: Option<EventAiState>,
    pub(super) source_player_environment: Option<PlayerEnvironmentState>,
    pub(super) source_player_collision_height: Option<PlayerCollisionHeightState>,
    pub admin: AdminState,
    pub player_slow: Option<PlayerSlowState>,
    pub player_movement_flags: Option<benilla_sim::creature_motion::GroundMovementFlags>,
    pub player_events: PlayerEvents,
    pub quest_progression: Option<QuestProgressionState>,
    pub domain: Domain,
    pub world: WorldId,
    pub player: PlayerId,
    pub seed: u64,
    pub tick: u64,
    pub now_ms: u64,
    pub next_sequence: u64,
    pub position: ai::Point,
    pub orientation: Orientation,
    pub health: u32,
    pub resources: mana::MageManaState,
    pub npcs: BTreeMap<u64, Npc>,
    pub casts: BTreeMap<u64, Cast>,
    pub next_cast_id: u64,
    pub rng_counter: u64,
    pub combat: Option<PlayerCombatState>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Command {
    AttackStart {
        target: NpcId,
    },
    AttackStop,
    InventoryMove(InventoryMoveRequest),
    EquipmentMove(EquipmentMoveRequest),
    EquipmentAutoEquip {
        source: u8,
    },
    InventorySplit(InventorySplitRequest),
    InventoryAutoStore(InventoryAutoStoreRequest),
    InventoryDestroy(InventoryDestroyRequest),
    Admin(AdminRequest),
    MovementPose {
        destination: ai::Point,
        orientation: Orientation,
        movement_flags: benilla_sim::creature_motion::GroundMovementFlags,
    },
    ChooseQuestReward {
        quest: u32,
        giver: NpcId,
        choice: u32,
    },
    OpenLoot {
        corpse: NpcId,
    },
    LootItem {
        corpse: NpcId,
        slot: u8,
    },
    LootMoney {
        corpse: NpcId,
    },
    ReleaseLoot {
        corpse: NpcId,
    },
    AcceptQuest {
        quest: u32,
        giver: NpcId,
    },
    TurnInQuest {
        quest: u32,
        giver: NpcId,
    },
    /// Internal controller lifecycle acknowledgement; no network/UI request maps here.
    AcknowledgeInitialReputations,
    SetActionBarToggles {
        toggles: u8,
    },
    SetWatchedFaction {
        list_id: i32,
    },
    SetFactionAtWar {
        list_id: u32,
        at_war: bool,
    },
    SetFactionInactive {
        list_id: u32,
        inactive: bool,
    },
    ReleaseSpirit,
    AbandonQuest {
        slot: u8,
    },
    Cast {
        ability: u32,
        target: NpcId,
    },
    CancelCast {
        spell_id: u32,
        target: CancelCastTarget,
    },
    /// Position-only intent preserves the current orientation.
    Move {
        destination: ai::Point,
    },
    /// Atomic pose intent. An unchanged destination turns without a movement request.
    Pose {
        destination: ai::Point,
        orientation: Orientation,
    },
}
/// Source positional intent; each sequenced input resolves against its phase-owned inventory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryMoveRequest {
    pub source: u8,
    pub destination: u8,
}
/// Explicit ordinary-backpack split; amount retains the source packet uint8 domain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryAutoStoreRequest {
    pub source: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventorySplitRequest {
    pub source: u8,
    pub destination: u8,
    pub count: u8,
}
/// Source selected backpack slot; count zero requests the complete selected stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryDestroyRequest {
    pub slot: u8,
    pub count: u8,
}
/// An exact preparing instance, or the cast created by an earlier input in this frame.
/// PriorInput never falls back to whichever cast happens to be active after a rejection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CancelCastTarget {
    Instance(u64),
    PriorInput(u64),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub world: WorldId,
    pub tick: u64,
    pub player: PlayerId,
    pub sequence: u64,
    pub command: Command,
}
/// Transport ownership is never decoded from a peer's claimed envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedInput {
    pub authenticated_owner: crate::OwnerId,
    pub input: Input,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Rejection {
    AttackDenied,
    Inventory {
        reason: u8,
        source: u8,
        destination: u8,
    },
    Admin(AdminRejection),
    Loot {
        corpse: NpcId,
        reason: u8,
    },
    Quest(benilla_sim::quest_lifecycle::Gate),
    QuestInteraction,
    UnknownAbility,
    UnavailableAbility {
        ability: u32,
        reason: String,
    },
    AlreadyCasting,
    UnknownTarget,
    Dead,
    NoCast,
    CastFailed(spell::CastFailure),
    MovementBlocked,
    SpiritReleaseUnavailable {
        phase: Option<benilla_sim::player_death::DeathPhase>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Event {
    PlayerAttackStart {
        target: NpcId,
    },
    PlayerAttackStop {
        target: NpcId,
    },
    PlayerAttackCancel,
    PlayerMeleeCriticalWound {
        target: NpcId,
    },
    PlayerMelee {
        target: NpcId,
        action: u64,
        outcome: MeleeOutcome,
        effective: u32,
    },
    PlayerSwingError {
        code: u8,
    },
    EventAiEvaluation(Box<EventAiEvaluation>),
    EventAiSpeech(Box<EventAiSpeech>),
    PlayerEnvironment(PlayerEnvironmentReceipt),
    NpcSpell(NpcSpellEvent),
    AdminCommitted(AdminReceipt),
    AdminRejected {
        request: AdminRequest,
        reason: AdminRejection,
    },
    PlayerSlow(PlayerSlowEvent),
    NpcMotion {
        id: NpcId,
        event: benilla_sim::creature_motion::Event,
    },
    PlayerMovementState {
        movement_flags: benilla_sim::creature_motion::GroundMovementFlags,
    },
    Loot(LootEvent),
    InventoryStored(Box<benilla_sim::inventory::Stored>),
    InventoryMoved(Box<benilla_sim::inventory::MovementReceipt>),
    EquipmentMoved(Box<EquipmentMoveReceipt>),
    EquipmentMoveRefused {
        request: EquipmentMoveRequest,
        instance: u64,
        reason: u8,
    },
    InventorySplit(Box<benilla_sim::inventory::SplitReceipt>),
    InventoryAutoStored(Box<benilla_sim::inventory::AutoStoreReceipt>),
    ItemAcquired(ItemAcquisition),
    QuestProgression(QuestProgressionEvent),
    InputResult {
        sequence: u64,
        accepted: bool,
    },
    Death(benilla_sim::player_death::DeathAction),
    Combat(CombatEvent),
    Rejected(Rejection),
    Spell {
        cast_id: u64,
        ability: u32,
        target: NpcId,
        event: spell::SpellEvent,
    },
    Npc {
        id: NpcId,
        effect: ai::Effect,
    },
    Melee {
        id: NpcId,
        outcome: MeleeOutcome,
        effective: u32,
    },
    NpcHealthRegenerated {
        id: NpcId,
        before: u32,
        after: u32,
    },
    Mana {
        before: u32,
        after: u32,
    },
    PlayerPose {
        position: ai::Point,
        orientation: Orientation,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub world: WorldId,
    pub tick: u64,
    pub profile: Digest,
    pub authority_policy: Digest,
    pub geometry: Digest,
    pub npc: Option<NpcId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CastPhase {
    Prepare,
    Release,
    Impact,
    Aura,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Query {
    CreatureInitialization(Box<CreatureInitializationRequest>),
    SourceLocation(Box<SourceLocationQuery>),
    EventAiAudience(Box<EventAiAudienceRequest>),
    CombatAssistance(Box<CombatAssistanceRequest>),
    PlayerEnvironment(Box<PlayerEnvironmentQuery>),
    NpcSpell(NpcSpellQuery),
    SourceSpell(SourceSpellQuery),
    SourceMotion(MotionQuery),
    LootInteraction {
        witness: InteractionWitness,
        player_position: ai::Point,
        npc_position: ai::Point,
    },
    QuestInteraction {
        witness: InteractionWitness,
        quest: u32,
        reward: bool,
        player_position: ai::Point,
        npc_position: ai::Point,
        player_health: u32,
        npc_health: u32,
        npc_entry: u32,
        npc_flags: u32,
        unit_flags: u32,
    },
    QuestStatus {
        witness: InteractionWitness,
        player_position: ai::Point,
        npc_position: ai::Point,
        npc_entry: u32,
        npc_flags: u32,
        unit_flags: u32,
        player_ghost: bool,
    },
    NativeMelee {
        /// Present only when source SetInFront has run at the ordered AI phase.
        npc_orientation: Option<Orientation>,
        action: u64,
        state_binding: Digest,
        player_in_combat: bool,
        player_position: ai::Point,
        npc_position: ai::Point,
    },
    Spell {
        cast_id: u64,
        ability: u32,
        phase: CastPhase,
        /// Exact current pose witness, not an implemented facing predicate.
        player_orientation: Orientation,
        player_position: ai::Point,
        target_position: ai::Point,
    },
    Npc {
        position: ai::Point,
        player_position: ai::Point,
        motion: Option<ai::MotionCommand>,
    },
    Movement(ai::movement::MovementRequest),
    PlayerMovement {
        /// Runtime-derived current movement speed; None belongs to the legacy fixture policy.
        speed: Option<ai::Real>,
        /// One immutable origin and elapsed budget for the entire frame, never per packet.
        frame_origin: ai::Point,
        /// Previously accepted endpoints in arrival order, excluding frame_origin.
        /// Validate every segment plus destination against the single elapsed budget.
        accepted_path: Vec<ai::Point>,
        destination: ai::Point,
        elapsed_ms: u32,
    },
    /// Native NPC damage resolution is intentionally an explicit missing callback.
    /// A future native kernel must define its dice before this policy can admit it.
    Melee {
        player_health: u32,
        npc_health: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityRequest {
    pub binding: Binding,
    pub query: Query,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellFacts {
    pub other_failure: Option<u32>,
    pub allowed: bool,
    pub immune: bool,
    pub resist_chance: u16,
    pub critical_chance: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcFacts {
    pub can_attack_in_combat: bool,
    pub can_attack_on_sight: bool,
    pub visible: bool,
    pub accessible: bool,
    pub line_of_sight: bool,
    pub can_initiate_attack: bool,
    pub stationary_melee_refresh_eligible: bool,
    pub creature_moving_and_running: bool,
    pub chase_goal: Option<ai::Point>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeOutcome {
    pub damage: u32,
    pub hit_info: u32,
    pub victim_state: u32,
    pub blocked: u32,
    pub absorb: u32,
    pub resist: i32,
    pub school: u8,
    pub pushback: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Facts {
    CreatureInitialization(Box<CreatureInitializationFacts>),
    SourceLocation(SourceLocationFacts),
    EventAiAudience(EventAiAudienceFacts),
    CombatAssistance(CombatAssistanceFacts),
    PlayerEnvironment(PlayerEnvironmentFacts),
    NpcSpell(NpcSpellFacts),
    SourceSpell(SourceSpellFacts),
    SourceMotion(MotionFacts),
    LootInteraction {
        player_in_world: bool,
        player_stunned: bool,
    },
    QuestInteraction(QuestInteractionFacts),
    QuestStatus {
        player_reaction: u8,
        npc_reaction: u8,
    },
    NativeMelee(MeleeFacts),
    Spell(SpellFacts),
    Npc(NpcFacts),
    Movement(ai::movement::MovementOutcome),
    PlayerMovement {
        accepted: bool,
    },
    Melee(MeleeOutcome),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityReply {
    pub request: AuthorityRequest,
    pub facts: Facts,
}
/// Trusted local/replicated simulation adapter, never peer result-frame input. Methods must
/// be read-only proposals: the runtime commits the full frame atomically after all checks.
/// PlayerMovement must validate frame_origin -> accepted_path -> destination against one
/// elapsed_ms budget and all segment/geometry rules. Repeated requests do not renew time.
pub trait Authority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError>;
    /// Observe or verify a runtime-derived reply in original query order. This hook
    /// cannot substitute facts; replay compares the complete recorded reply here.
    fn observe_derived(&mut self, _reply: &AuthorityReply) -> Result<(), NativeError> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestInteractionFacts {
    pub player_in_world: bool,
    pub player_taxi_flying: bool,
    pub player_cannot_react_or_lost_control: bool,
    pub player_charmed: bool,
    pub npc_charmed: bool,
    pub npc_pet_in_combat: bool,
    pub npc_invisible_to_alive: bool,
    pub player_reaction: u8,
    pub npc_reaction: u8,
}
impl State {
    /// Raw source action-bar display byte, not the contents of action slots.
    pub fn action_bar_toggles(&self) -> u8 {
        self.action_bar_toggles.value()
    }
    /// Raw source signed descriptor; unknown and negative values are not normalized.
    pub fn watched_faction(&self) -> i32 {
        self.watched_faction.value()
    }
    /// Conservative liveness for the supported native player domain.
    ///
    /// A retained death lifecycle must be Alive, and health must still be positive.
    /// The health guard deliberately preserves the existing lethal callback window:
    /// HP reaches zero before the enclosing death plan installs its next phase.
    /// This is not a general replacement for upstream Unit::IsAlive/IsDead, and
    /// does not admit a positive-health ghost or relax any checkpoint invariant.
    pub fn player_alive(&self) -> bool {
        self.health != 0
            && self
                .combat
                .as_ref()
                .and_then(|combat| combat.death.as_ref())
                .is_none_or(benilla_sim::player_death::PlayerDeathState::is_alive)
    }
    pub(in crate::native) fn level(&self, profile: &Profile) -> u8 {
        self.quest_progression
            .as_ref()
            .map_or(profile.player.level, |s| s.player.view().level as u8)
    }
    pub(in crate::native) fn abilities<'a>(&'a self, profile: &'a Profile) -> &'a [Ability] {
        self.quest_progression
            .as_ref()
            .map_or(&profile.abilities, |s| &s.abilities)
    }
    pub(in crate::native) fn ability<'a>(
        &'a self,
        profile: &'a Profile,
        id: u32,
    ) -> Option<&'a Ability> {
        self.abilities(profile).iter().find(|a| a.id == id)
    }
}

impl Ability {
    pub(in crate::native) fn validate(&self, tick_ms: u32) -> Result<(), NativeError> {
        if self.id == 0 || self.id > u16::MAX.into() || self.school > 6 {
            return Err(NativeError::InvalidProfile);
        }
        match &self.admission {
            AbilityAdmission::Available(s) => {
                s.validate()?;
                if s.spell_id != self.id || s.cast_ms == 0 {
                    return Err(NativeError::InvalidProfile);
                }
                if s.effects.iter().any(|e|matches!(e,spell::MageEffect::PeriodicDamage{amplitude_ms,..} if *amplitude_ms<tick_ms)) {return Err(NativeError::InvalidProfile);}
            }
            AbilityAdmission::Unavailable { reason } if reason.is_empty() || reason.len() > 512 => {
                return Err(NativeError::InvalidProfile);
            }
            _ => {}
        }
        Ok(())
    }
}
