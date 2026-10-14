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
            b"benilla-native-profile-v8\0",
            &(
                NATIVE_POLICY,
                RNG_POLICY,
                spell::SPELL_EXECUTION_POLICY_ID,
                ai::CREATURE_AI_POLICY_ID,
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
    pub original_loot_recipient: Option<PlayerId>,
    pub corpse_loot: Option<CorpseLoot>,
    pub aura_bindings: BTreeMap<u32, ExecutionBinding>,
    pub id: NpcId,
    pub health: u32,
    pub ai: ai::SoloCreatureAi,
    pub auras: BTreeMap<u32, spell::DamageTarget>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub quest_progression: Option<QuestProgressionState>,
    pub domain: Domain,
    pub world: WorldId,
    pub player: PlayerId,
    pub seed: u64,
    pub tick: u64,
    pub now_ms: u64,
    pub next_sequence: u64,
    pub position: ai::Point,
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
    ReleaseSpirit,
    Cast {
        ability: u32,
        target: NpcId,
    },
    CancelCast {
        spell_id: u32,
        target: CancelCastTarget,
    },
    Move {
        destination: ai::Point,
    },
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
    Loot(LootEvent),
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
    Mana {
        before: u32,
        after: u32,
    },
    PlayerMoved(ai::Point),
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
    LootInteraction {
        player_position: ai::Point,
        npc_position: ai::Point,
    },
    QuestInteraction {
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
        player_position: ai::Point,
        npc_position: ai::Point,
        npc_entry: u32,
        npc_flags: u32,
        unit_flags: u32,
        player_ghost: bool,
    },
    NativeMelee {
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
    pub line_of_sight: bool,
}
impl State {
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
