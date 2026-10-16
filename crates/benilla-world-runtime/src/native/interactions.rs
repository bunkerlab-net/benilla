// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Relations.cpp, Player.cpp, QuestHandler.cpp and
// LootHandler.cpp; upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
//! Private exact-draft semantic host. Source rows are inputs, never an admission token.
use super::*;
use crate::NpcId;
mod combat_authority;
mod facing;
mod peer_sight;
pub use facing::FacingNumericPolicy;
pub(in crate::native) use peer_sight::source_peer_sight;
pub use peer_sight::PeerSightDisposition;
pub(in crate::native) mod spell_authority;
use benilla_formats::FactionTemplate;
use benilla_sim::classic_targets as relations;
pub(in crate::native) use combat_authority::WorkingNpc;
pub use combat_authority::*;
use relations::unit_flags;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
pub use spell_authority::*;

pub const INTERACTION_POLICY: &str = "cmangos-8ec338a1-native-interaction-host-v1";
const STUNNED: u32 = 0x0004_0000;
const PET_IN_COMBAT: u32 = 0x0000_0800;
const GHOST: u32 = 0x0000_0010;
const TAXI_STATE: u32 = 0x0000_0080;
const CANNOT_REACT_OR_LOST_CONTROL: u32 = 0x0020_244e;
const SPIRIT_SERVICE: u32 = 0x60;
const STATIC_FLAGS: u32 = unit_flags::SPAWNING
    | unit_flags::NOT_ATTACKABLE_1
    | unit_flags::UNTARGETABLE
    | unit_flags::UNINTERACTIBLE
    | unit_flags::SHEATHE;

/// Missing evidence is distinct from an observed absence. No default is provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionEvidence {
    Absent,
    Present,
    Unknown,
}
/// Complete source/session exclusions required by this bounded runtime. These
/// supplied assertions are not proof that any raw database was inspected.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InteractionAudit {
    pub external_state_mutations: InteractionEvidence,
    pub controlled_or_summoned_units: InteractionEvidence,
    pub pvp_duel_or_sanctuary: InteractionEvidence,
    pub forced_reactions: InteractionEvidence,
    pub temporary_factions: InteractionEvidence,
    pub immunity_overrides: InteractionEvidence,
    pub npc_aura_or_script_hooks: InteractionEvidence,
}
impl InteractionAudit {
    fn validate(&self) -> Result<(), NativeError> {
        for (evidence, reason) in [
            (
                self.external_state_mutations,
                "external interaction state mutations",
            ),
            (
                self.controlled_or_summoned_units,
                "controlled or summoned interaction units",
            ),
            (
                self.pvp_duel_or_sanctuary,
                "PvP, duel or sanctuary interaction context",
            ),
            (self.forced_reactions, "forced interaction reactions"),
            (self.temporary_factions, "temporary interaction factions"),
            (self.immunity_overrides, "interaction immunity overrides"),
            (
                self.npc_aura_or_script_hooks,
                "NPC interaction aura or script hooks",
            ),
        ] {
            if evidence != InteractionEvidence::Absent {
                return Err(NativeError::InteractionEvidence {
                    dependency: reason,
                    observed: evidence,
                });
            }
        }
        Ok(())
    }
}
/// Actual initial source control fields. Only the ordinary independent state is
/// currently executable; nonzero values are Unsupported, not gameplay denials.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct InteractionControlSeed {
    pub unit_state: u32,
    pub owner_guid: u64,
    pub charmer_guid: u64,
    pub mount_display_id: u32,
    pub shapeshift_form: u8,
    pub aura_flags: u32,
}
impl InteractionControlSeed {
    fn validate(self) -> Result<(), NativeError> {
        if self.unit_state != 0
            || self.owner_guid != 0
            || self.charmer_guid != 0
            || self.mount_display_id != 0
            || self.shapeshift_form != 0
            || self.aura_flags != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "initial interaction control state",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlayerInteractionSource {
    pub faction_template: u32,
    pub unit_flags: u32,
    pub player_flags: u32,
    pub control: InteractionControlSeed,
}
/// Template fields, with spawn overrides explicitly excluded. Revival restores
/// template NPC flags in Creature::SetDeathState(JUST_ALIVED).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NpcInteractionSource {
    pub entry: u32,
    pub faction_template: u32,
    pub npc_flags: u32,
    pub unit_flags: u32,
    pub creature_type_flags: u32,
    pub control: InteractionControlSeed,
    pub spawn_flag_or_faction_overrides: InteractionEvidence,
}
/// Owned source FactionTemplate.dbc and Faction.dbc metadata. No client reaction
/// colour, neutral fallback, or guessed reputation classification is accepted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InteractionFactionSource {
    pub template_id: u32,
    pub faction: u32,
    pub template_flags: u32,
    pub group_mask: u32,
    pub friend_group_mask: u32,
    pub enemy_group_mask: u32,
    pub enemies: [u32; 4],
    pub friends: [u32; 4],
    /// None means no Faction.dbc identity and is valid only for faction zero.
    pub reputation_list_id: Option<i32>,
}
impl InteractionFactionSource {
    fn template(&self) -> FactionTemplate {
        FactionTemplate {
            faction: self.faction,
            group_mask: self.group_mask,
            friend_group_mask: self.friend_group_mask,
            enemy_group_mask: self.enemy_group_mask,
            enemies: self.enemies,
            friends: self.friends,
        }
    }
    fn reference<'a>(&self, template: &'a FactionTemplate) -> relations::FactionRef<'a> {
        relations::FactionRef {
            template: Some(template),
            template_flags: Some(self.template_flags),
            reputation: match self.reputation_list_id {
                None => relations::FactionReputation::NoFaction,
                Some(index) if index < 0 => relations::FactionReputation::Untracked,
                Some(index) => relations::FactionReputation::Tracked { index },
            },
        }
    }
}
#[derive(Clone, Debug)]
pub struct InteractionInput {
    pub player: PlayerInteractionSource,
    pub npcs: Vec<NpcInteractionSource>,
    pub factions: Vec<InteractionFactionSource>,
    pub audit: InteractionAudit,
}
/// Immutable, profile-bound source semantics; deliberately no deserializer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InteractionCatalog {
    policy: &'static str,
    facing_authority: Option<FacingNumericPolicy>,
    combat_authority: Option<CombatAuthorityCatalog>,
    spell_authority: Option<SpellAuthorityCatalog>,
    player: PlayerInteractionSource,
    npcs: BTreeMap<u32, NpcInteractionSource>,
    factions: BTreeMap<u32, InteractionFactionSource>,
    audit: InteractionAudit,
}
impl Profile {
    pub fn initialize_interactions(&mut self, input: InteractionInput) -> Result<(), NativeError> {
        let mut next = self.clone();
        let mut npcs = BTreeMap::new();
        let mut factions = BTreeMap::new();
        if input.npcs.len() > 4096 || input.factions.len() > 4097 {
            return Err(NativeError::TooLarge);
        }
        for row in input.npcs {
            if npcs.insert(row.entry, row).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        for row in input.factions {
            if factions.insert(row.template_id, row).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        next.interactions = Some(InteractionCatalog {
            policy: INTERACTION_POLICY,
            facing_authority: next
                .interactions
                .as_ref()
                .and_then(|c| c.facing_authority.clone()),
            spell_authority: next
                .interactions
                .as_ref()
                .and_then(|c| c.spell_authority.clone()),
            combat_authority: next
                .interactions
                .as_ref()
                .and_then(|c| c.combat_authority.clone()),
            player: input.player,
            npcs,
            factions,
            audit: input.audit,
        });
        next.validate()?;
        *self = next;
        Ok(())
    }
}
impl InteractionCatalog {
    pub(super) fn owner_ignore_pathfinding_source(
        &self,
        profile: &Profile,
        npc: NpcId,
    ) -> Result<(crate::Digest, u32), NativeError> {
        self.validate(profile)?;
        let entry = profile
            .npc(npc.spawn)
            .ok_or(NativeError::AuthorityBinding)?
            .entry;
        let row = self.npcs.get(&entry).ok_or(NativeError::AuthorityBinding)?;
        // The explicit source seed and all mutation exclusions were validated above.
        // This is not a claim that the executing Unit's complete state stays zero.
        Ok((
            crate::digest(
                b"benilla-owner-unit-state-source-v1\0",
                &(self.policy, row, &self.audit),
            ),
            row.control.unit_state & UNIT_STAT_IGNORE_PATHFINDING,
        ))
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        self.audit.validate()?;
        if let Some(facing) = &self.facing_authority {
            facing.validate(profile)?;
        }
        if let Some(spells) = &self.spell_authority {
            spells.validate(profile)?;
        }
        if let Some(combat) = &self.combat_authority {
            combat.validate(profile)?;
        }
        self.player.control.validate()?;
        // Player::Create sets PLAYER_CONTROLLED. Logout, taxis, forms, mounts,
        // charm and ghost release have no admitted native transition here.
        if self.player.unit_flags != unit_flags::PLAYER_CONTROLLED || self.player.player_flags != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "initial player interaction flags",
            ));
        }
        let combat = profile
            .combat
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic(
                "interaction host requires source player initialization",
            ))?;
        if combat
            .initial_auras
            .iter()
            .any(|a| !matches!(a.aura_type, 22 | 29 | 30 | 80 | 83 | 98 | 101 | 137 | 142))
        {
            return Err(NativeError::UnsupportedMechanic(
                "interaction-changing player aura",
            ));
        }
        // Player::DoInteraction / DoLoot remove even passive numeric auras.
        // Their stat/resource removal callbacks are a separate unimplemented path.
        if combat
            .initial_auras
            .iter()
            .any(|a| a.aura_interrupt_flags & 0x0c00 != 0)
        {
            return Err(NativeError::UnsupportedMechanic(
                "interaction or loot interrupted player aura removal",
            ));
        }
        validate_abilities(&profile.abilities)?;
        if !self.factions.contains_key(&self.player.faction_template) {
            return Err(NativeError::UnsupportedMechanic(
                "missing player interaction faction template",
            ));
        }
        for row in self.factions.values() {
            if row.template_id == 0 || (row.faction == 0) != row.reputation_list_id.is_none() {
                return Err(NativeError::InvalidProfile);
            }
            if row.template_flags != 0 {
                return Err(NativeError::UnsupportedMechanic(
                    "interaction faction template flags",
                ));
            }
            if row.faction != 0 {
                if let Some(catalog) = profile
                    .quest_progression
                    .as_ref()
                    .and_then(|g| g.reputation_catalog())
                {
                    let actual = catalog
                        .row(row.faction)
                        .map_err(|_| {
                            NativeError::UnsupportedMechanic(
                                "unresolved interaction Faction.dbc row",
                            )
                        })?
                        .ok_or(NativeError::UnsupportedMechanic(
                            "missing interaction Faction.dbc row",
                        ))?;
                    if Some(actual.faction.reputation_list_id) != row.reputation_list_id {
                        return Err(NativeError::InvalidProfile);
                    }
                } else if row.reputation_list_id.is_some_and(|id| id >= 0) {
                    return Err(NativeError::UnsupportedMechanic(
                        "tracked interaction faction requires reputation state",
                    ));
                }
            }
        }
        for row in self.npcs.values() {
            row.control.validate()?;
            if row.spawn_flag_or_faction_overrides != InteractionEvidence::Absent {
                return Err(NativeError::InteractionEvidence {
                    dependency: "NPC interaction spawn overrides",
                    observed: row.spawn_flag_or_faction_overrides,
                });
            }
            if row.unit_flags & !STATIC_FLAGS != 0 {
                return Err(NativeError::UnsupportedMechanic(
                    "unmodeled NPC interaction unit flags",
                ));
            }
            if !profile.npcs.iter().any(|n| n.entry == row.entry) {
                return Err(NativeError::InvalidProfile);
            }
            if let Some(quest) = profile
                .quest_progression
                .as_ref()
                .and_then(|g| g.npc_rows.get(&row.entry))
            {
                if quest.npc_flags != row.npc_flags || quest.unit_flags != row.unit_flags {
                    return Err(NativeError::InvalidProfile);
                }
            }
        }
        for npc in &profile.npcs {
            if !self.npcs.contains_key(&npc.entry)
                || !self.factions.contains_key(&npc.faction_template)
            {
                return Err(NativeError::UnsupportedMechanic(
                    "missing NPC interaction source rows",
                ));
            }
            let source = &self.npcs[&npc.entry];
            if source.faction_template != npc.faction_template
                || source.unit_flags != npc.health_regeneration.unit_flags
                || source.creature_type_flags != npc.health_regeneration.creature_type_flags
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(())
    }
}

pub(super) fn validate_abilities(abilities: &[Ability]) -> Result<(), NativeError> {
    for ability in abilities {
        if let AbilityAdmission::Available(spec) = &ability.admission {
            for effect in spec.effects {
                match effect {
                    spell::MageEffect::Empty
                    | spell::MageEffect::SchoolDamage(_)
                    | spell::MageEffect::PeriodicDamage { .. } => {}
                    spell::MageEffect::Unsupported { .. } => {
                        return Err(NativeError::UnsupportedMechanic(
                            "interaction-changing active effect",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionMoment {
    ReadOnly,
    Frame { action_ordinal: u32 },
}
/// Receipt identity only. Possessing or deserializing this does not construct a
/// private current-state view or supply authority facts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionWitness {
    pub moment: InteractionMoment,
    pub draft: crate::Digest,
}
pub(super) fn witness(
    profile_digest: crate::Digest,
    state: &State,
    moment: InteractionMoment,
) -> Result<InteractionWitness, NativeError> {
    // Same full canonical JSON and domain as crate::digest. This rejects an
    // oversized draft before a second witness or authority call, but the first
    // checkpoint clone/serialization still allocates before this size check.
    let bytes = serde_json::to_vec(&persistence::snapshot_state(profile_digest, state))
        .map_err(|_| NativeError::InvalidState)?;
    if bytes.len() > MAX_BYTES {
        return Err(NativeError::TooLarge);
    }
    let mut hash = Sha256::new();
    hash.update(b"benilla-native-interaction-draft-v1\0");
    hash.update(bytes);
    Ok(InteractionWitness {
        moment,
        draft: hash.finalize().into(),
    })
}

/// Borrowed only from the executing frame or a read-only NativeWorld preview.
/// Interaction callers use the complete scene. The separate combat host takes
/// its current NPC fields from a private borrow of the removed staged owner,
/// including the live AI callback and its post-movement source pose.
struct CurrentView<'a> {
    profile: &'a Profile,
    state: &'a State,
    catalog: &'a InteractionCatalog,
    npc: CurrentNpc,
    source: &'a NpcInteractionSource,
}
#[derive(Clone, Copy)]
struct CurrentNpc {
    id: NpcId,
    health: u32,
    phase: ai::Phase,
}
impl From<&Npc> for CurrentNpc {
    fn from(npc: &Npc) -> Self {
        Self {
            id: npc.id,
            health: npc.health,
            phase: npc.ai.phase(),
        }
    }
}
impl CurrentView<'_> {
    fn npc_flags(&self) -> u32 {
        if self.npc.health == 0 {
            0
        } else {
            self.source.npc_flags
        }
    }
    fn npc_unit_flags(&self) -> u32 {
        self.source.unit_flags
            | if self.npc.phase == ai::Phase::Combat {
                unit_flags::IN_COMBAT
            } else {
                0
            }
    }
    fn player_unit_flags(&self) -> u32 {
        self.catalog.player.unit_flags
            | if self.state.combat.as_ref().is_some_and(|c| c.in_combat) {
                unit_flags::IN_COMBAT
            } else {
                0
            }
            | if self
                .state
                .quest_progression
                .as_ref()
                .is_some_and(|q| q.loot_target.is_some())
            {
                unit_flags::LOOTING
            } else {
                0
            }
    }
    fn with_relation_input<T>(
        &self,
        f: impl FnOnce(&relations::SourcePlayerNpcInput<'_>) -> Result<T, NativeError>,
    ) -> Result<T, NativeError> {
        let player = &self.catalog.factions[&self.catalog.player.faction_template];
        let npc_profile = self
            .profile
            .npc(self.npc.id.spawn)
            .ok_or(NativeError::InvalidState)?;
        let npc = &self.catalog.factions[&npc_profile.faction_template];
        let player_template = player.template();
        let npc_template = npc.template();
        let reputation = match (
            self.profile
                .quest_progression
                .as_ref()
                .and_then(|g| g.reputation_catalog()),
            self.state
                .quest_progression
                .as_ref()
                .and_then(|q| q.reputation.as_ref()),
        ) {
            (Some(catalog), Some(state)) => Some(relations::ReputationRef {
                catalog,
                state: state.view(),
            }),
            (None, None) => None,
            _ => return Err(NativeError::InvalidState),
        };
        let absent = relations::ContextState::Absent;
        // These exclusions were checked from explicit source inputs. All live
        // effects are direct/periodic damage or numeric passives; current NPC AI
        // has no control, faction, aura/script or PvP mutation operation.
        f(&relations::SourcePlayerNpcInput {
            units: relations::PlayerNpcInput {
                player_faction: player.reference(&player_template),
                npc_faction: npc.reference(&npc_template),
                player: relations::UnitTargetState {
                    unit_flags: self.player_unit_flags(),
                    alive: self.state.health != 0,
                    mounted: self.catalog.player.control.mount_display_id != 0,
                },
                npc: relations::UnitTargetState {
                    unit_flags: self.npc_unit_flags(),
                    alive: self.npc.health != 0,
                    mounted: self.source.control.mount_display_id != 0,
                },
                context: relations::RelationContext {
                    control: absent,
                    pvp: absent,
                    reputation_overrides: absent,
                    immunity: absent,
                    special_state: absent,
                },
            },
            reputation,
        })
    }
    fn reactions(&self) -> Result<relations::SourcePlayerNpcReactions, NativeError> {
        self.with_relation_input(|input| {
            relations::source_player_npc_reactions(input).map_err(NativeError::InteractionRelation)
        })
    }
    fn can_attack(&self, side: relations::Side) -> Result<ai::movement::Check, NativeError> {
        self.with_relation_input(|input| match relations::check_can_attack(input, side) {
            relations::TargetDecision::Allowed => Ok(ai::movement::Check::Passed),
            relations::TargetDecision::Denied(_) => Ok(ai::movement::Check::Failed),
            relations::TargetDecision::Unsupported(reason) => {
                Err(NativeError::InteractionRelation(reason))
            }
        })
    }
}

pub(super) fn current_npc_flags(
    profile: &Profile,
    state: &State,
    npc: &Npc,
) -> Result<(u32, u32), NativeError> {
    let p = profile.npc(npc.id.spawn).ok_or(NativeError::InvalidState)?;
    let (flags, unit_flags) = if let Some(catalog) = &profile.interactions {
        let row = catalog
            .npcs
            .get(&p.entry)
            .ok_or(NativeError::InvalidState)?;
        (
            row.npc_flags,
            row.unit_flags
                | if npc.ai.phase() == ai::Phase::Combat {
                    unit_flags::IN_COMBAT
                } else {
                    0
                },
        )
    } else {
        let row = profile
            .quest_progression
            .as_ref()
            .and_then(|g| g.npc_rows.get(&p.entry))
            .ok_or(NativeError::UnsupportedMechanic(
                "unresolved NPC dialog flags",
            ))?;
        (row.npc_flags, row.unit_flags)
    };
    if state
        .npcs
        .get(&npc.id.spawn)
        .is_none_or(|current| current != npc)
    {
        return Err(NativeError::AuthorityBinding);
    }
    Ok((if npc.health == 0 { 0 } else { flags }, unit_flags))
}

pub(super) fn resolve(
    profile: &Profile,
    profile_digest: crate::Digest,
    state: &State,
    request: &AuthorityRequest,
    moment: InteractionMoment,
) -> Result<Option<Facts>, NativeError> {
    let request_witness = match &request.query {
        Query::QuestStatus { witness, .. }
        | Query::QuestInteraction { witness, .. }
        | Query::LootInteraction { witness, .. } => witness,
        Query::SourceMotion(MotionQuery::HostileSpellRelation(q)) => &q.witness,
        _ => return Ok(None),
    };
    if *request_witness != witness(profile_digest, state, moment)?
        || request.binding.world != state.world
        || request.binding.tick != state.tick
        || request.binding.profile != profile_digest
        || request.binding.authority_policy != profile.authority_policy
        || request.binding.geometry != profile.geometry_revision
    {
        return Err(NativeError::AuthorityBinding);
    }
    let Some(catalog) = &profile.interactions else {
        return match state.domain {
            Domain::SyntheticComponentTest => Ok(None),
        };
    };
    if state.npcs.len() != profile.npcs.len() {
        return Err(NativeError::UnsupportedMechanic(
            "interaction view requires complete current NPC scene",
        ));
    }
    validate_abilities(state.abilities(profile))?;
    let id = request.binding.npc.ok_or(NativeError::AuthorityBinding)?;
    let npc = state
        .npcs
        .get(&id.spawn)
        .filter(|n| n.id == id && n.ai.phase() != ai::Phase::Despawned)
        .ok_or(NativeError::AuthorityBinding)?;
    let npc_profile = profile.npc(id.spawn).ok_or(NativeError::InvalidState)?;
    let source = catalog
        .npcs
        .get(&npc_profile.entry)
        .ok_or(NativeError::InvalidState)?;
    let view = CurrentView {
        profile,
        state,
        catalog,
        npc: npc.into(),
        source,
    };
    let facts = match &request.query {
        Query::SourceMotion(MotionQuery::HostileSpellRelation(q)) => {
            let g = profile
                .source_motion
                .as_ref()
                .ok_or(NativeError::InvalidState)?;
            let motion = npc
                .source_motion
                .as_ref()
                .ok_or(NativeError::InvalidState)?;
            if q.player_pose.position != state.position
                || q.player_pose.orientation.bits() != state.orientation.bits()
                || q.creature_pose != motion.motion.pose()
                || q.player_health != state.health
                || q.creature_health != npc.health
                || q.creature_phase != npc.ai.phase()
                || q.player_can_enter_combat != g.player_can_enter_combat
                || q.creature_can_enter_combat
                    != (profile.npc_motion(npc.id.spawn)?.npc_can_enter_combat
                        && npc.ai.phase() != ai::Phase::ReturningHome)
            {
                return Err(NativeError::AuthorityBinding);
            }
            Facts::SourceMotion(MotionFacts::HostileSpellRelation(view.can_attack(
                match q.direction {
                    SpellRelationDirection::PlayerToCreature => relations::Side::Player,
                    SpellRelationDirection::CreatureToPlayer => relations::Side::Npc,
                },
            )?))
        }
        Query::LootInteraction {
            player_position,
            npc_position,
            ..
        } => {
            if *player_position != state.position || *npc_position != npc.ai.position() {
                return Err(NativeError::AuthorityBinding);
            }
            Facts::LootInteraction {
                // Active NativeWorld owns the session; no logout/transfer action is admitted.
                player_in_world: true,
                player_stunned: view.player_unit_flags() & STUNNED != 0,
            }
        }
        Query::QuestStatus {
            player_position,
            npc_position,
            npc_entry,
            npc_flags,
            unit_flags,
            player_ghost,
            ..
        } => {
            if *player_position != state.position
                || *npc_position != npc.ai.position()
                || *npc_entry != npc_profile.entry
                || *npc_flags != view.npc_flags()
                || *unit_flags != view.npc_unit_flags()
                || *player_ghost != (catalog.player.player_flags & GHOST != 0)
            {
                return Err(NativeError::AuthorityBinding);
            }
            let reactions = view.reactions()?;
            Facts::QuestStatus {
                player_reaction: reactions.player_to_npc as u8,
                npc_reaction: reactions.npc_to_player as u8,
            }
        }
        Query::QuestInteraction {
            player_position,
            npc_position,
            player_health,
            npc_health,
            npc_entry,
            npc_flags,
            unit_flags,
            ..
        } => {
            if *player_position != state.position
                || *npc_position != npc.ai.position()
                || *player_health != state.health
                || *npc_health != npc.health
                || *npc_entry != npc_profile.entry
                || *npc_flags != view.npc_flags()
                || *unit_flags != view.npc_unit_flags()
            {
                return Err(NativeError::AuthorityBinding);
            }
            let reactions = view.reactions()?;
            Facts::QuestInteraction(QuestInteractionFacts {
                player_in_world: true,
                player_taxi_flying: catalog.player.control.unit_state & TAXI_STATE != 0
                    || view.player_unit_flags() & unit_flags::TAXI_FLIGHT != 0,
                player_cannot_react_or_lost_control: catalog.player.control.unit_state
                    & CANNOT_REACT_OR_LOST_CONTROL
                    != 0,
                player_charmed: catalog.player.control.charmer_guid != 0,
                npc_charmed: source.control.charmer_guid != 0,
                npc_pet_in_combat: view.npc_unit_flags() & PET_IN_COMBAT != 0,
                npc_invisible_to_alive: view.npc_flags() & SPIRIT_SERVICE != 0,
                player_reaction: reactions.player_to_npc as u8,
                npc_reaction: reactions.npc_to_player as u8,
            })
        }
        _ => unreachable!("interaction query selected above"),
    };
    Ok(Some(facts))
}
