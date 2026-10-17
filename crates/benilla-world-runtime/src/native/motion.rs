// SPDX-License-Identifier: GPL-3.0-or-later
//! Transactional source-motion integration. This is an explicitly bounded solo
//! component domain; no native geometry or visitor producer is supplied. The
//! explicitly selected combat host derives current semantic state internally.
use super::*;
use benilla_sim::creature_motion as motion;
use serde::{Deserialize, Serialize};

/// Shared player and world configuration. Every NPC owns an explicit entry;
/// the unselected legacy visitor lane remains singleton-only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionGenesis {
    /// Captured raw spell inputs; no database inspection is implied.
    pub spell_hits: BTreeMap<u32, SpellHitPolicy>,
    pub player_can_enter_combat: bool,
    pub player_collision_height: ai::Real,
    pub player_movement_flags: motion::GroundMovementFlags,
    pub creature_initial_aggro_delay_ms: u32,
    pub relocation_aggro_delay_ms: u32,
    /// UnitVisitObjectsInRangeNotifyEvent uses the common CONFIG_FLOAT_RATE_CREATURE_AGGRO.
    pub aggro_rate: ai::Real,
    #[serde(deserialize_with = "unique_npc_motion_owners")]
    pub npcs: BTreeMap<u64, NpcMotionGenesis>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcMotionGenesis {
    pub npc_policy: MotionPolicy,
    pub npc_can_enter_combat: bool,
    pub npc_stand_state: u8,
    pub npc_spawn_orientation: Orientation,
    pub npc_initial_active: bool,
    pub chase_walking: bool,
    pub home_walking: bool,
    pub creature_detection_range: u32,
}
fn unique_npc_motion_owners<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<u64, NpcMotionGenesis>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Owners;
    impl<'de> serde::de::Visitor<'de> for Owners {
        type Value = BTreeMap<u64, NpcMotionGenesis>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("unique stable-spawn motion owners")
        }
        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut owners = BTreeMap::new();
            while let Some((spawn, genesis)) = map.next_entry()? {
                if owners.insert(spawn, genesis).is_some() {
                    return Err(serde::de::Error::custom("duplicate NPC motion owner"));
                }
            }
            Ok(owners)
        }
    }
    deserializer.deserialize_map(Owners)
}
impl NpcMotionGenesis {
    pub(super) fn validate_for(
        &self,
        profile: &Profile,
        npc: &NpcProfile,
    ) -> Result<(), NativeError> {
        if npc.ai.creature_id <= SOLO_UNIT || self.creature_detection_range > 1000 {
            return Err(NativeError::InvalidProfile);
        }
        ai::Real::from_bits(self.npc_spawn_orientation.bits())?;
        self.npc_policy
            .input(npc.max_health, npc.max_health)
            .validate()?;
        self.npc_policy.home.validate(profile, npc)?;
        if self.npc_stand_state != 0 {
            return Err(NativeError::UnsupportedMechanic(
                "source hostile-hit stand state",
            ));
        }
        if ai::Real::new(self.creature_detection_range as f32)? != npc.ai.detection
            || self.npc_policy.run_speed != npc.ai.run_speed
            || self.npc_policy.settings.combat_reach != npc.ai.combat_reach
            || self.npc_policy.settings.bounding_radius != npc.ai.bounding_radius
        {
            return Err(NativeError::InvalidProfile);
        }
        Ok(())
    }
}
impl MotionGenesis {
    /// Stable-spawn policy selection is not a current-generation witness.
    pub fn npc(&self, spawn: u64) -> Result<&NpcMotionGenesis, NativeError> {
        self.npcs
            .get(&spawn)
            .filter(|_| spawn > SOLO_UNIT)
            .ok_or(NativeError::InvalidProfile)
    }
    pub(super) fn validate_owners(&self, profile: &Profile) -> Result<(), NativeError> {
        if !self
            .npcs
            .keys()
            .copied()
            .eq(profile.npcs.iter().map(|npc| npc.ai.creature_id))
        {
            return Err(NativeError::InvalidProfile);
        }
        if let Some(generations) = &profile.creature_generations {
            generations.validate(profile)?;
        }
        for npc in &profile.npcs {
            self.npc(npc.ai.creature_id)?.validate_for(profile, npc)?;
            if self.aggro_rate != npc.ai.aggro_rate {
                return Err(NativeError::InvalidProfile);
            }
        }
        Ok(())
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        self.validate_owners(profile)?;
        // A deliberate execution/visitor boundary, not a Classic world/game rule.
        if let Some(visitors) = &profile.source_visitors {
            visitors.validate_scene(profile)?;
        } else if profile.npcs.len() != 1 {
            return Err(NativeError::InvalidProfile);
        }
        if self.player_collision_height.as_f32() < 0.0 {
            return Err(NativeError::InvalidProfile);
        }
        ai::Real::from_bits(profile.player.orientation.bits())?;
        for ability in &profile.abilities {
            if let AbilityAdmission::Available(spec) = &ability.admission {
                self.spell_hits
                    .get(&ability.id)
                    .ok_or(NativeError::AuthorityUnavailable(
                        "raw hostile spell metadata",
                    ))?
                    .validate(spec)?;
            }
        }
        // Player::SetPosition, Unit::DealDamage and RemoveAurasOnCast can
        // remove even passive holders. No player holder-removal callback is
        // represented here, so every nonzero raw interrupt mask is unsupported.
        if profile
            .combat
            .as_ref()
            .is_some_and(|g| g.initial_auras.iter().any(|a| a.aura_interrupt_flags != 0))
        {
            return Err(NativeError::UnsupportedMechanic(
                "source motion interruptible initial player auras",
            ));
        }
        Ok(())
    }
}

/// Raw source fields required by CasterHitTargetWithSpell. The existing bounded
/// projectile recipe does not retain them; no absence is inferred from that recipe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellHitPolicy {
    pub attributes: [u32; 5],
    pub implicit_target_a: [u32; 3],
    pub implicit_target_b: [u32; 3],
    pub aura_interrupt_flags: u32,
}
impl SpellHitPolicy {
    pub(super) fn validate(&self, spec: &spell::MageProjectileSpec) -> Result<(), NativeError> {
        // Ordinary hostile unit targeting is exactly negative in SpellMgr.h;
        // passive, neutral/script targets and triggered effects are not inferred.
        if self.attributes[0] & 0x40 != 0
            || self.attributes[1] & 0x1000 != 0
            || self.attributes[3] & 1 != 0
            || self.aura_interrupt_flags & 0x0100_0003 != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "hostile spell stealth/PvP/damage aura-removal branch",
            ));
        }
        for (index, effect) in spec.effects.iter().enumerate() {
            if !matches!(effect, spell::MageEffect::Empty)
                && (self.implicit_target_a[index] != 6 || self.implicit_target_b[index] != 0)
            {
                return Err(NativeError::UnsupportedMechanic(
                    "hostile spell target positivity",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn initial_threat(&self, success: bool) -> bool {
        self.attributes[2] & 0x0040_0000 == 0
            && self.attributes[1] & 0x400 == 0
            && (!success || self.attributes[1] & 0x0020_0000 == 0)
    }
    pub(super) fn damage_threat(&self) -> bool {
        self.attributes[1] & 0x400 == 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpellRelationDirection {
    PlayerToCreature,
    CreatureToPlayer,
}
/// Exact complete-frame callback view. Visibility means IsVisibleForOrDetect
/// with creature as detector and player as detected object; it is not segment LOS.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostileSpellRequest {
    pub witness: InteractionWitness,
    pub cast_id: u64,
    pub ability: u32,
    pub success: bool,
    pub direction: SpellRelationDirection,
    pub player_pose: motion::Pose,
    pub creature_pose: motion::Pose,
    pub player_health: u32,
    pub creature_health: u32,
    pub creature_phase: ai::Phase,
    pub player_can_enter_combat: bool,
    pub creature_can_enter_combat: bool,
}

/// Captured initial policy remains effective only within the explicitly admitted
/// damage/periodic-only runtime. No speed/control/script mutation is represented.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionPolicy {
    pub walk_speed: ai::Real,
    pub run_speed: ai::Real,
    pub settings: motion::GroundSettings,
    pub audit: motion::MotionAudit,
    pub home: HomeAudit,
}
impl MotionPolicy {
    pub(super) fn input(&self, health: u32, maximum_health: u32) -> motion::UnitInputs {
        motion::UnitInputs {
            health,
            maximum_health,
            walk_speed: self.walk_speed,
            run_speed: self.run_speed,
            settings: self.settings,
            audit: self.audit,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcMotion {
    pub motion: motion::MotionState,
    pub active: bool,
    pub combat_start: Option<motion::Pose>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcMotionSnapshot {
    pub motion: motion::MotionSnapshot,
    pub active: bool,
    pub combat_start: Option<motion::Pose>,
}
impl NpcMotionSnapshot {
    pub(super) fn validate_saved_session_reopen(&self) -> Result<(), NativeError> {
        let prior_cache = self
            .motion
            .spline
            .as_ref()
            .and_then(|s| s.route_request.prior_cache.as_ref());
        let current_cache = match &self.motion.generator {
            motion::Generator::Chase(chase) => chase.pathfinder_cache.as_ref(),
            _ => None,
        };
        if prior_cache
            .into_iter()
            .chain(current_cache)
            .any(|cache| cache.source != motion::RouteSource::SyntheticFixture)
        {
            return Err(NativeError::UnsupportedMechanic(
                "saved native PathFinder cache rehydration",
            ));
        }
        Ok(())
    }
}
impl NpcMotion {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn new(profile: &Profile, npc: &NpcProfile) -> Result<Option<Self>, NativeError> {
        if profile.source_motion.is_none() {
            return Ok(None);
        }
        let g = profile.npc_motion(npc.ai.creature_id)?;
        Ok(Some(Self {
            motion: motion::MotionState::new(
                motion::Identity {
                    creature: npc.ai.creature_id,
                    map: profile.map_id,
                    content: ai::movement::Revision(profile.content_digest),
                    geometry: ai::movement::Revision(profile.geometry_revision),
                },
                motion::Pose {
                    position: npc.ai.spawn,
                    orientation: ai::Real::from_bits(g.npc_spawn_orientation.bits())?,
                },
                profile.clock_origin_ms,
                profile.clock_origin_ms,
            )?,
            active: g.npc_initial_active,
            combat_start: None,
        }))
    }
    pub(super) fn snapshot(&self) -> NpcMotionSnapshot {
        NpcMotionSnapshot {
            motion: self.motion.snapshot(),
            active: self.active,
            combat_start: self.combat_start,
        }
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore(
        s: NpcMotionSnapshot,
        profile: &Profile,
        ai: &ai::SoloCreatureAi,
        now: u64,
        npc: crate::NpcId,
    ) -> Result<Self, NativeError> {
        Self::restore_with_generation(s, profile, ai, now, npc, None)
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn restore_generation(
        s: NpcMotionSnapshot,
        profile: &Profile,
        ai: &ai::SoloCreatureAi,
        now: u64,
        npc: crate::NpcId,
        generation: &CreatureGenerationOwner,
    ) -> Result<Self, NativeError> {
        Self::restore_with_generation(s, profile, ai, now, npc, Some(generation))
    }
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    fn restore_with_generation(
        s: NpcMotionSnapshot,
        profile: &Profile,
        ai: &ai::SoloCreatureAi,
        now: u64,
        npc: crate::NpcId,
        generation: Option<&CreatureGenerationOwner>,
    ) -> Result<Self, NativeError> {
        let Some(g) = &profile.source_motion else {
            return Err(NativeError::InvalidCheckpoint);
        };
        let owner = profile
            .npc(npc.spawn)
            .ok_or(NativeError::InvalidCheckpoint)?;
        let owner_genesis = profile
            .npc_motion(npc.spawn)
            .map_err(|_| NativeError::InvalidCheckpoint)?;
        let collision_height = match (&profile.creature_generations, generation) {
            (None, None) => owner_genesis.npc_policy.settings.collision_height,
            (Some(_), Some(generation)) if generation.npc_id() == npc => {
                CreatureCollisionHeightState::new_generation(profile, npc, generation)?
                    .map(|height| ai::Real::from_bits(height.witness().height().bits()))
                    .transpose()?
                    .unwrap_or(owner_genesis.npc_policy.settings.collision_height)
            }
            _ => return Err(NativeError::InvalidCheckpoint),
        };
        let expected = Self::new(profile, owner)?
            .ok_or(NativeError::InvalidCheckpoint)?
            .motion
            .snapshot()
            .identity;
        if s.motion.identity != expected
            || s.motion.now_ms != now
            || s.motion.processed_map_clock_ms != now
            || s.motion.map_pose.position != ai.position()
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        // Source dispatch passes None only for Home; every Chase/Backpedal uses
        // Some(player). Preserve that presence and both owners' original bounds.
        if s.motion.spline.as_ref().is_some_and(|spline| {
            spline.route_request.owner_collision_height != collision_height
                || matches!(
                    spline.route_request.kind,
                    motion::RouteKind::HomeForceDestination
                ) != spline.route_request.target.is_none()
                || !matches!(
                    (spline.route_request.kind, spline.facing),
                    (
                        motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan,
                        motion::Facing::Target(SOLO_UNIT)
                    ) | (motion::RouteKind::Backpedal, motion::Facing::Tangent)
                        | (
                            motion::RouteKind::HomeForceDestination,
                            motion::Facing::Angle(_)
                        )
                )
                || spline.route_request.target.as_ref().is_some_and(|target| {
                    target.id != SOLO_UNIT
                        || target.collision_height != g.player_collision_height
                        || target.bounding_radius != profile.player.bounding_radius
                        || target.combat_reach != profile.player.combat_reach
                })
        }) {
            return Err(NativeError::InvalidCheckpoint);
        }
        // Runtime alone knows owner generation. These consistency checks do not
        // authenticate source refs or supply a production rehydration path.
        s.validate_saved_session_reopen()
            .map_err(|_| NativeError::InvalidCheckpoint)?;
        let prior_cache = s
            .motion
            .spline
            .as_ref()
            .and_then(|s| s.route_request.prior_cache.as_ref());
        let current_cache = match &s.motion.generator {
            motion::Generator::Chase(chase) => chase.pathfinder_cache.as_ref(),
            _ => None,
        };
        for cache in prior_cache.into_iter().chain(current_cache) {
            let capability = profile
                .creature_capabilities
                .as_ref()
                .ok_or(NativeError::InvalidCheckpoint)?;
            let include_flags = benilla_sim::route_filter::initial_include_flags(
                capability.source_capabilities(profile, npc)?,
            )
            .ok_or(NativeError::InvalidCheckpoint)?;
            if cache.binding.owner_generation != npc.generation
                || cache.binding.identity.creature != npc.spawn
                || cache.include_flags != include_flags
                || cache.exclude_flags != 0
            {
                return Err(NativeError::InvalidCheckpoint);
            }
        }
        if let Some(p) = s.combat_start {
            p.validate()?;
        }
        // Home SetFacing wraps TAU to zero; all other captured pose bits must match.
        let home_pose = s.combat_start.map(|mut pose| {
            if pose.orientation.bits() == 0x40c9_0fdb {
                pose.orientation = ai::Real::ZERO;
            }
            pose
        });
        let motion = motion::MotionState::restore(s.motion)?;
        match (ai.motion(), motion.generator()) {
            (Some(command), motion::Generator::Chase(chase))
                if command.purpose
                    == ai::movement::MovementPurpose::Chase {
                        target: chase.target,
                    } => {}
            (Some(command), motion::Generator::Home { destination, .. })
                if command.purpose == ai::movement::MovementPurpose::Home
                    && command.home == Some(destination.position)
                    && home_pose == Some(*destination) => {}
            (None, motion::Generator::Idle) => {}
            _ => return Err(NativeError::InvalidCheckpoint),
        }
        let result = Self {
            motion,
            active: s.active,
            combat_start: s.combat_start,
        };
        match ai.phase() {
            ai::Phase::Idle
                if result.combat_start.is_some()
                    || result.active != owner_genesis.npc_initial_active =>
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            ai::Phase::Combat | ai::Phase::ReturningHome
                if result
                    .combat_start
                    .is_none_or(|pose| pose.position != ai.combat_start_position())
                    || (ai.phase() == ai::Phase::ReturningHome && !result.active) =>
            {
                return Err(NativeError::InvalidCheckpoint);
            }
            _ => {}
        }
        Ok(result)
    }
}
/// HomeMovementGenerator.cpp:88-97 reloads the immutable selected addon. The
/// generation-owned sheath-only domain makes that reload idempotent; its raw
/// move_flags remain unused. This reference is not a separately writable addon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum HomeAddonClosure {
    Absent,
    GenerationSource { source_identity: crate::Digest },
}
/// Explicit source closure for Home callbacks and alternative destination selection.
/// Other callbacks must remain absent; current health/control is checked separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomeAudit {
    pub reset_position_override: motion::Evidence,
    pub restore_temporary_faction: motion::Evidence,
    pub addon_closure: HomeAddonClosure,
    pub scripted_home_events: motion::Evidence,
    pub temporary_summon_callback: motion::Evidence,
}
impl HomeAudit {
    pub(super) fn validate(&self, profile: &Profile, npc: &NpcProfile) -> Result<(), NativeError> {
        for proof in [
            self.reset_position_override,
            self.restore_temporary_faction,
            self.scripted_home_events,
            self.temporary_summon_callback,
        ] {
            match proof {
                motion::Evidence::Absent => {}
                motion::Evidence::Present => {
                    return Err(NativeError::UnsupportedMechanic(
                        "source Home callbacks/reset override",
                    ));
                }
                motion::Evidence::Unknown => {
                    return Err(NativeError::AuthorityUnavailable(
                        "captured source Home lifecycle evidence",
                    ));
                }
            }
        }
        let source = profile
            .creature_generations
            .as_ref()
            .and_then(|catalog| catalog.source(npc.ai.creature_id));
        match (self.addon_closure, source) {
            (HomeAddonClosure::Absent, None) => {}
            (HomeAddonClosure::Absent, Some(source)) => {
                if !matches!(&source.input.spawn_addons, benilla_sim::creature_init::Rows::Complete(rows) if rows.is_empty())
                    || !matches!(&source.input.template_addons, benilla_sim::creature_init::Rows::Complete(rows) if rows.is_empty())
                {
                    return Err(NativeError::UnsupportedMechanic(
                        "source Home addon closure",
                    ));
                }
            }
            (HomeAddonClosure::GenerationSource { .. }, None) => {
                return Err(NativeError::AuthorityUnavailable(
                    "source Home generation source",
                ));
            }
            (HomeAddonClosure::GenerationSource { source_identity }, Some(source)) => {
                if source.spawn_id != npc.ai.creature_id
                    || source.spawn_entry != npc.entry
                    || source.identity() != source_identity
                {
                    return Err(NativeError::AuthorityBinding);
                }
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum VisitorOwner {
    Player,
    Creature(crate::NpcId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisitorRequest {
    pub callback: benilla_sim::event_processor::EventCallbackSnapshot,
    pub owner: VisitorOwner,
    pub event_clock_ms: u64,
    pub now_ms: u64,
    pub radius: ai::Real,
    pub player_pose: motion::Pose,
    pub player_alive: bool,
    pub creature: crate::NpcId,
    pub creature_pose: motion::Pose,
    pub creature_phase: ai::Phase,
    pub creature_can_aggro: bool,
}
/// Complete membership includes both represented participants. A visited player
/// only requests the source MoveInLineOfSight call; it does not assert hostility,
/// LOS, accessibility or CanAggro. Unknown coverage cannot be an empty set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum VisitorFacts {
    Complete {
        loaded_membership: ai::movement::Check,
        call_move_in_line_of_sight: bool,
    },
    Missing,
    UnrepresentedParticipants,
}
/// Exact actor identity in the selected authored scene. Membership classifications
/// are spatial evidence, never an inferred distance or liveness filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum VisitorParticipant {
    Player(crate::PlayerId),
    Creature(crate::NpcId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisitorContainer {
    GridCreatures,
    WorldPlayers,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisitorDirection {
    VisitedToOwner,
    OwnerToVisited,
}
/// Full staged scene, live event callback/queue and action ordinal. A serialized
/// receipt is not a live scene view and cannot authorize a semantic decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneWitness {
    pub frame: InteractionWitness,
    pub callback: crate::Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisitorCreature {
    pub id: crate::NpcId,
    pub pose: motion::Pose,
    pub health: u32,
    pub phase: ai::Phase,
    pub can_aggro: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneVisitorRequest {
    pub witness: SceneWitness,
    pub notification_owner: VisitorParticipant,
    pub callback: benilla_sim::event_processor::EventCallbackSnapshot,
    pub event_clock_ms: u64,
    pub now_ms: u64,
    pub radius: ai::Real,
    /// An authored permutation, not a claim about Map's unordered update set.
    pub owner_order: Vec<u64>,
    pub player: crate::PlayerId,
    pub player_pose: motion::Pose,
    pub player_alive: bool,
    pub creatures: Vec<VisitorCreature>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SceneVisitorFacts {
    Complete {
        loaded_membership: ai::movement::Check,
        grid_creatures: Vec<crate::NpcId>,
        world_players: Vec<crate::PlayerId>,
        not_visited: Vec<VisitorParticipant>,
    },
    Missing,
    UnrepresentedParticipants,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisitorSightRequest {
    pub witness: SceneWitness,
    pub notification_owner: VisitorParticipant,
    pub callback: benilla_sim::event_processor::EventCallbackSnapshot,
    pub container: VisitorContainer,
    pub visit_index: u32,
    pub direction: VisitorDirection,
    pub observer: VisitorParticipant,
    pub target: VisitorParticipant,
}
/// Runtime-owned no-op/dispatch receipt. Providers answer membership and the
/// directional AI::IsVisible predicate only, never these semantic branches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SightDisposition {
    Evading,
    NotVisible,
    Peer(PeerSightDisposition),
    PlayerSightApplied,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MotionQuery {
    Combat(CombatQuery),
    HostileSpellRelation(HostileSpellRequest),
    HostileSpellVisibility(HostileSpellRequest),
    Spatial(motion::Query),
    /// Selected source host supplies a bound current capability witness.
    NearPoint(CreatureNearPointRequest),
    /// Selected source host binds PathFinder's current owner and map position.
    Route(Box<CreatureRouteRequest>),
    Visitor(VisitorRequest),
    SceneVisitor(Box<SceneVisitorRequest>),
    /// UnitAI::IsVisible: within this observer's AI visibility distance AND
    /// target.IsVisibleForOrDetect(observer, observer, true), not static LOS.
    /// CMaNGOS Classic 8ec338a1, AI/BaseAI/UnitAI.cpp:662–665.
    VisitorVisibility(Box<VisitorSightRequest>),
    /// Runtime-derived disposition; an external authority cannot choose it.
    VisitorSight(Box<VisitorSightRequest>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MotionFacts {
    Combat(CombatFacts),
    HostileSpellRelation(ai::movement::Check),
    HostileSpellVisibility(ai::movement::Check),
    Spatial(Box<motion::Reply>),
    Visitor(VisitorFacts),
    SceneVisitor(SceneVisitorFacts),
    VisitorVisibility(ai::movement::Check),
    VisitorSight(SightDisposition),
}
impl From<motion::MotionError> for NativeError {
    fn from(value: motion::MotionError) -> Self {
        Self::Motion(value)
    }
}

#[cfg(test)]
#[path = "route_cache_tests.rs"]
pub(super) mod route_cache_tests;
