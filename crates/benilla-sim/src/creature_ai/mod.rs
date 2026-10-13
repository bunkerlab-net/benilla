// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic at 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS Project contributors; see ../../licenses/AUTHORS.md.
// Source grant GPL-2.0-or-later; 2026 Rust translation GPL-3.0-or-later.
// WITHOUT ANY WARRANTY. See ../../PROVENANCE.md and ../../CREATURE-AI.md.
//! Outdoor solo ordinary-creature control, not a native world enablement gate.
//! Only one authenticated player threat reference is supported. Damage, movement
//! generation, scripts, roaming, health/regen and native row resolution are external.
//! No wire decoding, wallclock, assets, fixed damage, teleport or stationary chase.

pub mod movement;
mod numeric;
use movement::{BlockReason, MovementPurpose, MovementResult, Revision, ValidatedMovement};
pub use numeric::{
    attack_distance, combat_reach, distance_2d, in_melee_range, pulls_aggro, Point, Real,
};

pub const CREATURE_AI_POLICY_ID: &str = "cmangos-8ec338a1-outdoor-solo-ai-binary32-rne-v1";
const COMBAT_TICK_MS: u32 = 1200;
const UNREACHABLE_MS: u32 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Unsupported {
    ScriptedAi,
    SpellList,
    AssistanceOrLinking,
    ControlledOrSummoned,
    Dungeon,
    FlightSwimmingOrTransport,
    CrowdControlOrSpecialFlags,
    OffhandOrExtraAttacks,
    RandomOrWaypointIdle,
    MultipleThreatTargets,
    LootAdjustedCorpseTimer,
    LongDistanceHomeDespawn,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiError {
    Invalid(&'static str),
    Unsupported(Unsupported),
    Movement(BlockReason),
}
impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bounded creature AI: {self:?}")
    }
}
impl std::error::Error for AiError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Reaction {
    Passive,
    Defensive,
    Aggressive,
}
/// Resolved native configuration, not a DB row and not evidence of complete metadata.
/// No Default: native values must be supplied and unsupported features audited.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub creature_id: u64,
    pub map_id: u32,
    pub content_revision: Revision,
    pub spawn: Point,
    /// Shared authoritative map clock; zero is a whole-second boundary.
    pub clock_origin_ms: u64,
    pub level: u8,
    pub reaction: Reaction,
    pub neutral_to_all: bool,
    pub detection: Real,
    pub aggro_rate: Real,
    pub combat_reach: Real,
    pub bounding_radius: Real,
    pub melee_leeway: Real,
    pub run_speed: Real,
    pub home_speed: Real,
    pub mainhand_interval_ms: u32,
    pub pursuit_ms: u32,
    pub leash_radius: Real,
    pub hard_leash_radius: Option<Real>,
    pub corpse_decay_ms: u32,
    pub respawn_delay_seconds: u32,
    pub unsupported: Vec<Unsupported>,
}
impl Profile {
    fn validate(&self) -> Result<(), AiError> {
        if let Some(feature) = self.unsupported.first() {
            return Err(AiError::Unsupported(*feature));
        }
        if self.creature_id == 0
            || self.level == 0
            || self.mainhand_interval_ms == 0
            || self.mainhand_interval_ms > 60_000
        {
            return Err(AiError::Invalid("creature identity/level/attack interval"));
        }
        self.spawn.validate()?;
        for v in [
            self.detection,
            self.aggro_rate,
            self.combat_reach,
            self.bounding_radius,
            self.melee_leeway,
            self.run_speed,
            self.home_speed,
            self.leash_radius,
        ] {
            v.nonnegative("profile scalar")?;
        }
        if self.run_speed == Real::ZERO || self.home_speed == Real::ZERO {
            return Err(AiError::Invalid("native speed"));
        }
        if let Some(leash) = self.hard_leash_radius {
            leash.nonnegative("hard leash")?;
        }
        Ok(())
    }
}

/// An ordinary player sampled by the authority, including resolved native relationship
/// and visibility predicates. This must not be filled from a peer's claimed state.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: u64,
    pub position: Point,
    pub level: u8,
    pub combat_reach: Real,
    pub bounding_radius: Real,
    pub moving_and_running: bool,
    pub alive: bool,
    pub online: bool,
    pub can_attack_in_combat: bool,
    pub can_attack_on_sight: bool,
    pub visible: bool,
    pub accessible: bool,
    pub line_of_sight: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ThreatKind {
    DirectDamage,
    PeriodicDamage,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CombatEvent {
    /// UnitAI::AttackedBy; authenticated successful attack initiation, not a peer command.
    AttackedBy {
        target: u64,
    },
    /// Already-resolved post-modifier threat. DOT does not refresh the pursuit timer.
    Threat {
        target: u64,
        amount: Real,
        kind: ThreatKind,
    },
    Died,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub id: u64,
    pub event: CombatEvent,
}
/// A simulation-step snapshot from an authenticated/validated authority.
/// There is deliberately no deserializer for this or the validated movement token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub sequence: u64,
    pub elapsed_ms: u32,
    pub geometry_revision: Revision,
    pub target: Option<Target>,
    pub can_initiate_attack: bool,
    pub casting_non_melee: bool,
    /// Post-movement authority predicate: lastMoveTime + 3s < now and victim !IsMoving.
    /// Applied only if this update actually issues a mainhand swing.
    pub stationary_melee_refresh_eligible: bool,
    pub creature_moving_and_running: bool,
    pub movement: Option<ValidatedMovement>,
    pub receipts: Vec<Receipt>,
}
/// Only the simulation authority implements this adapter. Receiving an authenticated
/// PSK packet is insufficient: validate ownership, movement and combat effects first.
pub trait CombatAuthority {
    fn observe(&mut self, creature_id: u64) -> Result<Observation, AiError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Phase {
    Idle,
    Combat,
    ReturningHome,
    Corpse,
    Despawned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EvadeReason {
    NoTarget,
    PursuitLeash,
    HardLeash,
    Unreachable,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionCommand {
    pub command_id: u64,
    pub purpose: MovementPurpose,
    pub home: Option<Point>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Effect {
    CombatStarted {
        target: u64,
    },
    FaceTarget {
        target: u64,
    },
    /// Invoke native hit/damage/spell resolution. Never a fixed damage amount.
    MainhandSwing {
        target: u64,
    },
    SwingDelayed {
        error: SwingError,
        retry_ms: u32,
    },
    Motion(MotionCommand),
    PositionCommitted(Point),
    /// Clear combat/threat/loot ownership and removable evade auras; do NOT instantly heal.
    EvadeStarted(EvadeReason),
    HomeReached,
    Died,
    CorpseRemoved,
    /// Reset native entry/health/addon/loot state and emit the normal spawn projection.
    Respawned {
        position: Point,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SwingError {
    OutOfRange,
    BadFacing,
    DeadTarget,
    CannotAttack,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwingConditions {
    pub has_victim: bool,
    pub casting_non_melee: bool,
    pub can_initiate: bool,
    pub in_range: bool,
    pub in_arc: bool,
    pub target_alive: bool,
    pub can_attack: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwingDecision {
    Wait,
    Swing,
    Retry(SwingError),
}
/// Unit::UpdateMeleeAttackingState's single-mainhand branches. The source's boolean
/// return means an error (and OutOfRange == 0), so expose actual decisions instead.
pub fn melee_decision(timer_ms: u32, c: SwingConditions) -> SwingDecision {
    if !c.has_victim || c.casting_non_melee || !c.can_initiate || timer_ms != 0 {
        return SwingDecision::Wait;
    }
    if !c.in_range {
        SwingDecision::Retry(SwingError::OutOfRange)
    } else if !c.in_arc {
        SwingDecision::Retry(SwingError::BadFacing)
    } else if !c.target_alive {
        SwingDecision::Retry(SwingError::DeadTarget)
    } else if !c.can_attack {
        SwingDecision::Retry(SwingError::CannotAttack)
    } else {
        SwingDecision::Swing
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoloCreatureAi {
    profile: Profile,
    phase: Phase,
    position: Point,
    combat_start: Point,
    last_refresh: Point,
    target: Option<u64>,
    threat: Real,
    now_ms: u64,
    sequence: u64,
    receipt_id: u64,
    movement_request_id: u64,
    command_id: u64,
    motion: Option<MotionCommand>,
    mainhand_ms: u32,
    pursuit_ms: u32,
    combat_tick_ms: u32,
    unreachable_ms: u32,
    corpse_at_ms: u64,
    death_at_ms: Option<u64>,
    despawned_at_ms: Option<u64>,
    respawn_at_ms: u64,
}
impl SoloCreatureAi {
    pub fn new(profile: Profile) -> Result<Self, AiError> {
        profile.validate()?;
        Ok(Self {
            position: profile.spawn,
            combat_start: profile.spawn,
            last_refresh: profile.spawn,
            now_ms: profile.clock_origin_ms,
            profile,
            phase: Phase::Idle,
            target: None,
            threat: Real::ZERO,
            sequence: 0,
            receipt_id: 0,
            movement_request_id: 0,
            command_id: 0,
            motion: None,
            mainhand_ms: 0,
            pursuit_ms: 0,
            combat_tick_ms: COMBAT_TICK_MS,
            unreachable_ms: 0,
            corpse_at_ms: 0,
            death_at_ms: None,
            despawned_at_ms: None,
            respawn_at_ms: 0,
        })
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn position(&self) -> Point {
        self.position
    }
    pub fn target(&self) -> Option<u64> {
        self.target
    }
    pub fn threat(&self) -> Real {
        self.threat
    }
    pub fn mainhand_timer_ms(&self) -> u32 {
        self.mainhand_ms
    }
    pub fn pursuit_timer_ms(&self) -> u32 {
        self.pursuit_ms
    }
    pub fn unreachable_timer_ms(&self) -> u32 {
        self.unreachable_ms
    }
    pub fn is_evading(&self) -> bool {
        self.phase == Phase::ReturningHome || self.unreachable_ms != 0
    }
    pub fn is_evade_regeneration(&self) -> bool {
        self.phase == Phase::ReturningHome
            || (self.unreachable_ms != 0 && self.unreachable_ms <= 5000)
    }
    pub fn motion(&self) -> Option<&MotionCommand> {
        self.motion.as_ref()
    }
    pub fn next_sequence(&self) -> Option<u64> {
        self.sequence.checked_add(1)
    }
    /// Input rejection is transactional. The adapter's observations and movement proposals
    /// must be non-mutating; commit returned effects together with this state.
    pub fn step(&mut self, authority: &mut impl CombatAuthority) -> Result<Vec<Effect>, AiError> {
        self.step_with_mainhand(authority, &mut |_, _, _| Ok(()))
    }
    /// Source AI-to-DealMeleeDamage callback before CombatManager::Update. The callback
    /// receives target ID, post-movement position, and whether CombatStarted happened
    /// earlier in this step. Rejection preserves AI; the caller must discard its staged
    /// callback state too. Ordinary NPC attacks do not refresh NPC pursuit: the source
    /// TriggerCombatTimer target is the player, who cannot hold a threat list.
    pub fn step_with_mainhand(
        &mut self,
        authority: &mut impl CombatAuthority,
        mainhand: &mut impl FnMut(u64, Point, bool) -> Result<(), AiError>,
    ) -> Result<Vec<Effect>, AiError> {
        let observation = authority.observe(self.profile.creature_id)?;
        let mut next = self.clone();
        let effects = next.apply(observation, mainhand)?;
        *self = next;
        Ok(effects)
    }
    fn apply(
        &mut self,
        o: Observation,
        mainhand: &mut impl FnMut(u64, Point, bool) -> Result<(), AiError>,
    ) -> Result<Vec<Effect>, AiError> {
        if Some(o.sequence) != self.next_sequence()
            || o.elapsed_ms == 0
            || o.elapsed_ms > 60_000
            || o.receipts.len() > 64
        {
            return Err(AiError::Invalid("step sequence/time/receipt count"));
        }
        if let Some(t) = &o.target {
            if t.id == 0 || t.level == 0 {
                return Err(AiError::Invalid("target identity/level"));
            }
            t.position.validate()?;
            t.combat_reach.nonnegative("target reach")?;
            t.bounding_radius.nonnegative("target bounding radius")?;
            if self.target.is_some_and(|id| id != t.id) {
                return Err(AiError::Unsupported(Unsupported::MultipleThreatTargets));
            }
        }
        self.now_ms = self
            .now_ms
            .checked_add(u64::from(o.elapsed_ms))
            .ok_or(AiError::Invalid("time overflow"))?;
        self.sequence = o.sequence;
        let mut effects = Vec::new();
        let prior_command = self.command_id;
        // Event processing precedes timer/movement/AI in Unit::Update.
        for receipt in &o.receipts {
            if receipt.id <= self.receipt_id {
                return Err(AiError::Invalid("replayed/reordered combat receipt"));
            }
            self.receipt_id = receipt.id;
            self.event(&receipt.event, o.target.as_ref(), &mut effects)?;
        }
        // Unit::Update still decrements attack timers on the death tick and while a corpse.
        if self.phase != Phase::Despawned {
            self.mainhand_ms = self.mainhand_ms.saturating_sub(o.elapsed_ms);
        }
        // Death during Unit::Update waits until the next Creature::Update for corpse timers.
        if effects.contains(&Effect::Died) {
            return Ok(effects);
        }
        match self.phase {
            Phase::Corpse => {
                if o.movement.is_some() {
                    return Err(AiError::Invalid("corpse movement"));
                }
                if self.now_ms >= self.corpse_at_ms {
                    self.phase = Phase::Despawned;
                    self.despawned_at_ms = Some(self.now_ms);
                    self.position = self.profile.spawn; // native invisible corpse removal relocation
                    effects.push(Effect::CorpseRemoved);
                }
                return Ok(effects);
            }
            Phase::Despawned => {
                if o.movement.is_some() {
                    return Err(AiError::Invalid("despawned movement"));
                }
                if self.now_ms >= self.respawn_at_ms {
                    self.phase = Phase::Idle;
                    self.death_at_ms = None;
                    self.despawned_at_ms = None;
                    effects.push(Effect::Respawned {
                        position: self.position,
                    });
                }
                return Ok(effects);
            }
            _ => {}
        }
        let reachability = self.apply_movement(&o, &mut effects)?;
        if self.phase == Phase::ReturningHome {
            if o.movement.is_none() {
                return Err(AiError::Movement(BlockReason::MissingGeometry));
            }
            return Ok(effects);
        }
        if self.phase == Phase::Idle {
            if let Some(t) = &o.target {
                if self.profile.reaction == Reaction::Aggressive
                    && !self.profile.neutral_to_all
                    && o.can_initiate_attack
                    && t.alive
                    && t.online
                    && t.visible
                    && t.can_attack_on_sight
                    && t.accessible
                    && t.line_of_sight
                {
                    let radius = attack_distance(
                        self.profile.level,
                        t.level,
                        self.profile.detection,
                        self.profile.aggro_rate,
                    )?;
                    if !self
                        .position
                        .square_distance(t.position, true)?
                        .greater(radius.mul(radius)?)
                    {
                        self.attack_start(t, &mut effects)?;
                    }
                }
            }
        }
        if self.phase != Phase::Combat {
            return Ok(effects);
        }
        let Some(t) = o
            .target
            .as_ref()
            .filter(|t| t.alive && t.online && Some(t.id) == self.target)
        else {
            self.evade(EvadeReason::NoTarget, &mut effects)?;
            return Ok(effects);
        };
        // SelectHostileTarget faces the current victim before UpdateMeleeAttackingState.
        effects.push(Effect::FaceTarget { target: t.id });
        match reachability {
            Some(false) if self.unreachable_ms == 0 => self.unreachable_ms = UNREACHABLE_MS,
            Some(true) => self.unreachable_ms = 0,
            _ => {}
        }
        let reach = combat_reach(self.profile.combat_reach, t.combat_reach)?;
        let leeway = if o.creature_moving_and_running && t.moving_and_running {
            self.profile.melee_leeway
        } else {
            Real::ZERO
        };
        let in_range = in_melee_range(self.position, t.position, reach, leeway)?;
        if !in_range && o.movement.is_none() && self.command_id == prior_command {
            return Err(AiError::Movement(BlockReason::MissingGeometry));
        }
        let decision = melee_decision(
            self.mainhand_ms,
            SwingConditions {
                has_victim: true,
                casting_non_melee: o.casting_non_melee,
                can_initiate: o.can_initiate_attack,
                in_range,
                in_arc: true,
                target_alive: t.alive,
                can_attack: t.can_attack_in_combat,
            },
        );
        match decision {
            SwingDecision::Wait => {}
            SwingDecision::Swing => {
                if o.stationary_melee_refresh_eligible {
                    self.refresh_pursuit();
                }
                self.mainhand_ms = self.profile.mainhand_interval_ms;
                effects.push(Effect::MainhandSwing { target: t.id });
                mainhand(
                    t.id,
                    self.position,
                    effects
                        .iter()
                        .any(|e| matches!(e, Effect::CombatStarted { .. })),
                )?;
            }
            SwingDecision::Retry(error) => {
                // Upstream SWING_ERROR_NOT_IN_RANGE is enum value zero, so this particular
                // error leaves both timers ready. Preserve that pinned-source quirk.
                if error != SwingError::OutOfRange {
                    self.mainhand_ms = 100;
                }
                effects.push(Effect::SwingDelayed {
                    error,
                    retry_ms: self.mainhand_ms,
                });
            }
        }
        // CombatManager::Update runs after AI; it subtracts at most one 1200ms interval.
        if self.unreachable_ms != 0 {
            if self.unreachable_ms <= o.elapsed_ms {
                self.unreachable_ms = 0;
                self.evade(EvadeReason::Unreachable, &mut effects)?;
                return Ok(effects);
            }
            self.unreachable_ms -= o.elapsed_ms;
        }
        self.pursuit_ms = self.pursuit_ms.saturating_sub(o.elapsed_ms);
        self.combat_tick_ms = self
            .combat_tick_ms
            .checked_add(o.elapsed_ms)
            .ok_or(AiError::Invalid("combat tick overflow"))?;
        if self.combat_tick_ms >= COMBAT_TICK_MS {
            self.combat_tick_ms -= COMBAT_TICK_MS;
            let distance = distance_2d(t.position, self.last_refresh, t.bounding_radius)?;
            if self.pursuit_ms == 0 && distance.greater(self.profile.leash_radius) {
                self.evade(EvadeReason::PursuitLeash, &mut effects)?;
            } else if let Some(limit) = self.profile.hard_leash_radius {
                if limit != Real::ZERO
                    && distance_2d(
                        self.position,
                        self.combat_start,
                        self.profile.bounding_radius,
                    )?
                    .greater(limit)
                {
                    self.evade(EvadeReason::HardLeash, &mut effects)?;
                }
            }
        }
        Ok(effects)
    }
    fn event(
        &mut self,
        e: &CombatEvent,
        target: Option<&Target>,
        effects: &mut Vec<Effect>,
    ) -> Result<(), AiError> {
        if matches!(e, CombatEvent::Died) {
            if matches!(self.phase, Phase::Corpse | Phase::Despawned) {
                return Err(AiError::Invalid("duplicate death"));
            }
            self.death_at_ms = Some(self.now_ms);
            self.phase = Phase::Corpse;
            self.target = None;
            self.threat = Real::ZERO;
            self.motion = None;
            self.unreachable_ms = 0;
            self.corpse_at_ms = self
                .now_ms
                .checked_add(u64::from(self.profile.corpse_decay_ms))
                .ok_or(AiError::Invalid("corpse time overflow"))?;
            self.respawn_at_ms = (self.now_ms / 1000)
                .checked_add(u64::from(self.profile.respawn_delay_seconds))
                .and_then(|seconds| seconds.checked_mul(1000))
                .ok_or(AiError::Invalid("respawn time overflow"))?;
            effects.push(Effect::Died);
            return Ok(());
        }
        if matches!(
            self.phase,
            Phase::Corpse | Phase::Despawned | Phase::ReturningHome
        ) || self.unreachable_ms != 0
        {
            return Err(AiError::Invalid("combat receipt during death/home evade"));
        }
        let id = match e {
            CombatEvent::AttackedBy { target } | CombatEvent::Threat { target, .. } => *target,
            _ => unreachable!(),
        };
        if self.target.is_some_and(|existing| existing != id) {
            return Err(AiError::Unsupported(Unsupported::MultipleThreatTargets));
        }
        let t = target
            .filter(|t| t.id == id && t.alive && t.online)
            .ok_or(AiError::Invalid("combat target not validated"))?;
        match e {
            CombatEvent::AttackedBy { .. } => {
                if self.phase == Phase::Idle {
                    self.attack_start(t, effects)?;
                }
            }
            CombatEvent::Threat { amount, kind, .. } => {
                amount.nonnegative("resolved threat")?;
                if self.profile.reaction == Reaction::Passive {
                    return Err(AiError::Unsupported(
                        Unsupported::CrowdControlOrSpecialFlags,
                    ));
                }
                // A periodic tick creates threat, but cannot initiate combat on its own in this
                // bounded controller. An authenticated AttackedBy/direct hit must precede it.
                if *kind == ThreatKind::PeriodicDamage && self.phase != Phase::Combat {
                    return Err(AiError::Invalid("periodic threat without combat"));
                }
                self.attack_start(t, effects)?;
                self.threat = self.threat.add(*amount)?;
                if *kind == ThreatKind::DirectDamage {
                    self.refresh_pursuit();
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn attack_start(&mut self, t: &Target, effects: &mut Vec<Effect>) -> Result<(), AiError> {
        if self.profile.reaction == Reaction::Passive
            || !t.can_attack_in_combat
            || self.phase == Phase::Combat
        {
            return Ok(());
        }
        self.phase = Phase::Combat;
        self.target = Some(t.id);
        self.combat_start = self.position;
        self.threat = Real::ZERO;
        self.refresh_pursuit();
        self.combat_tick_ms = COMBAT_TICK_MS;
        effects.push(Effect::CombatStarted { target: t.id });
        self.command(MovementPurpose::Chase { target: t.id }, None, effects)
    }
    fn refresh_pursuit(&mut self) {
        self.pursuit_ms = self.profile.pursuit_ms;
        self.last_refresh = self.position;
    }
    fn command(
        &mut self,
        purpose: MovementPurpose,
        home: Option<Point>,
        effects: &mut Vec<Effect>,
    ) -> Result<(), AiError> {
        self.command_id = self
            .command_id
            .checked_add(1)
            .ok_or(AiError::Invalid("command overflow"))?;
        let command = MotionCommand {
            command_id: self.command_id,
            purpose,
            home,
        };
        self.motion = Some(command.clone());
        effects.push(Effect::Motion(command));
        Ok(())
    }
    fn evade(&mut self, reason: EvadeReason, effects: &mut Vec<Effect>) -> Result<(), AiError> {
        // HomeMovementGenerator switches to delayed forced despawn beyond 150 yards.
        // That separate lifecycle is outside this bounded controller.
        let home_limit = Real::integer(150);
        if self
            .position
            .square_distance(self.combat_start, true)?
            .greater(home_limit.mul(home_limit)?)
        {
            return Err(AiError::Unsupported(Unsupported::LongDistanceHomeDespawn));
        }
        self.target = None;
        self.threat = Real::ZERO;
        self.unreachable_ms = 0;
        self.pursuit_ms = 0;
        self.phase = Phase::ReturningHome;
        effects.push(Effect::EvadeStarted(reason));
        self.command(MovementPurpose::Home, Some(self.combat_start), effects)
    }
    fn apply_movement(
        &mut self,
        o: &Observation,
        effects: &mut Vec<Effect>,
    ) -> Result<Option<bool>, AiError> {
        let Some(m) = &o.movement else {
            // Issuing a chase command is not evidence of reachability or movement.
            return Ok(None);
        };
        let command = self
            .motion
            .as_ref()
            .ok_or(AiError::Invalid("unsolicited movement"))?;
        let r = &m.request;
        if r.command_id != command.command_id
            || r.purpose != command.purpose
            || r.creature_id != self.profile.creature_id
            || r.map_id != self.profile.map_id
            || r.origin != self.position
            || r.request_id <= self.movement_request_id
            || r.elapsed_ms != o.elapsed_ms
        {
            return Err(AiError::Invalid("movement binding/time/origin"));
        }
        if r.content_revision != self.profile.content_revision {
            return Err(AiError::Movement(BlockReason::StaleContent));
        }
        if r.geometry_revision != o.geometry_revision {
            return Err(AiError::Movement(BlockReason::StaleGeometry));
        }
        let speed = if r.purpose == MovementPurpose::Home {
            self.profile.home_speed
        } else {
            self.profile.run_speed
        };
        if r.speed != speed {
            return Err(AiError::Invalid(
                "movement speed is not resolved native speed",
            ));
        }
        if r.purpose == MovementPurpose::Home && command.home != Some(r.goal) {
            return Err(AiError::Invalid("wrong home destination"));
        }
        self.movement_request_id = r.request_id;
        match &m.result {
            MovementResult::Blocked(
                BlockReason::MissingGeometry
                | BlockReason::UnsupportedNavigation
                | BlockReason::StaleGeometry
                | BlockReason::StaleContent
                | BlockReason::WrongMap
                | BlockReason::StaleRequest,
            ) => {
                if let MovementResult::Blocked(reason) = m.result {
                    Err(AiError::Movement(reason))
                } else {
                    unreachable!()
                }
            }
            MovementResult::Blocked(_) => Ok(Some(false)),
            MovementResult::Progress {
                position, arrived, ..
            } => {
                self.position = *position;
                effects.push(Effect::PositionCommitted(*position));
                if *arrived && r.purpose == MovementPurpose::Home {
                    self.phase = Phase::Idle;
                    self.motion = None;
                    effects.push(Effect::HomeReached);
                }
                Ok(Some(true))
            }
        }
    }
}

mod checkpoint;
pub use checkpoint::*;
