// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! Bounded ordinary-NPC death of a solo mage. See PLAYER-DEATH.md.
//!
//! A plan is an ordered transaction, not an HP-only mutation. The caller stages
//! every action, including the existing equipment/stat callback, before committing
//! `next`. No mana reset, passive-aura removal, or released-flight cancellation is
//! implicit. No corpse object or ghost is fabricated when release is unavailable.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PLAYER_DEATH_POLICY: &str = "cmangos-8ec338a1-solo-mage-death-v1";
pub const PLAYER_DEATH_FORMAT: u32 = 1;
pub const DEATH_TIMER_MS: u32 = 360_000;
pub const DEATH_EXPIRE_STEP_SECONDS: u64 = 300;
pub const MAX_AURA_HOLDERS: usize = 256;
pub const MAX_RELEASED_FLIGHTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DeathPhase {
    Alive,
    JustDied,
    /// Player body state CORPSE; CreateCorpse has NOT run yet.
    Corpse,
}

/// Actual captured map/config values, never inferred from map ID or a default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathConfig {
    pub map_instanceable: bool,
    pub pve_reclaim_delay_enabled: bool,
}

/// Inert checkpoint DTO. Only checked restore creates a live lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerDeathSnapshot {
    pub version: u32,
    pub player_id: u64,
    pub config: DeathConfig,
    pub phase: DeathPhase,
    pub last_tick: u64,
    pub last_now_seconds: u64,
    pub last_update_tick: Option<u64>,
    pub died_at_tick: Option<u64>,
    pub corpse_at_tick: Option<u64>,
    pub death_timer_ms: u32,
    pub death_expire_time_seconds: u64,
    /// Expiration entering this death; native first-life policy requires zero.
    pub death_expire_before_death_seconds: Option<u64>,
    /// Source timer reached BuildPlayerRepop/RepopAtGraveyard; those unported
    /// operations are pending. This is not a source ghost/death-state value.
    pub release_due: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerDeathState(PlayerDeathSnapshot);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Dependency {
    Battleground,
    Duel,
    DeathPrevention,
    KillerOwner,
    ResponsiblePlayer,
    KillProcHooks,
    KillerAiDeathHooks,
    KillerControlledUnits,
    VictimPetOrGuardians,
    VictimCharmOrTotemsOrMinipet,
    Formation,
    InstanceDeathHooks,
    SelfResurrection,
    Group,
    FlyingOrFallingMovement,
    AuraRemovalHooks,
    PlayerAiCombatHooks,
    ActiveMeleeSpell,
    SpellCancellationDependencies,
}
/// Every item is required explicitly. `None` is missing evidence, not false.
pub const REQUIRED_ABSENCES: [Dependency; 19] = [
    Dependency::Battleground,
    Dependency::Duel,
    Dependency::DeathPrevention,
    Dependency::KillerOwner,
    Dependency::ResponsiblePlayer,
    Dependency::KillProcHooks,
    Dependency::KillerAiDeathHooks,
    Dependency::KillerControlledUnits,
    Dependency::VictimPetOrGuardians,
    Dependency::VictimCharmOrTotemsOrMinipet,
    Dependency::Formation,
    Dependency::InstanceDeathHooks,
    Dependency::SelfResurrection,
    Dependency::Group,
    Dependency::FlyingOrFallingMovement,
    Dependency::AuraRemovalHooks,
    Dependency::PlayerAiCombatHooks,
    Dependency::ActiveMeleeSpell,
    Dependency::SpellCancellationDependencies,
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeathSupport(());
impl DeathSupport {
    /// The caller must obtain these facts from admitted state/source closure.
    /// Merely calling this constructor does not establish capture completeness.
    pub fn checked(evidence: &[(Dependency, Option<bool>)]) -> Result<Self, DeathError> {
        let mut found = BTreeSet::new();
        for &(dependency, present) in evidence {
            if !found.insert(dependency) {
                return Err(DeathError::DuplicateDependency(dependency));
            }
            match present {
                Some(false) => (),
                Some(true) => return Err(DeathError::Unsupported(dependency)),
                None => return Err(DeathError::MissingDependency(dependency)),
            }
        }
        for dependency in REQUIRED_ABSENCES {
            if !found.contains(&dependency) {
                return Err(DeathError::MissingDependency(dependency));
            }
        }
        Ok(Self(()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeathError {
    Invalid(&'static str),
    Unsupported(Dependency),
    MissingDependency(Dependency),
    DuplicateDependency(Dependency),
    NonLethal,
    AlreadyDead,
    ClockRegression,
    RepeatedUpdate,
    ClockOverflow,
    RestoreMismatch,
    UnsupportedAuraState(u32),
}
impl std::fmt::Display for DeathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "player death: {self:?}")
    }
}
impl std::error::Error for DeathError {}

/// Attributes and AttributesEx3 from the authoritative spell row. CMaNGOS
/// derives both passive and death-persistent holder flags from these bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathAura {
    pub holder_id: u64,
    pub spell_id: u32,
    pub attributes: u32,
    pub attributes_ex3: u32,
    /// Actual SpellEntry CasterAuraState. Nonzero conditional holders are
    /// outside this port: clearing health/defense aura state can remove them.
    pub caster_aura_state: u32,
}
impl DeathAura {
    pub fn survives_death(self) -> bool {
        self.attributes & 0x40 != 0 || self.attributes_ex3 & 0x0010_0000 != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CurrentSpellSlot {
    Generic,
    AutoRepeat,
    Channeled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrentSpellState {
    Created,
    Targeting,
    Casting,
    Delayed,
    Traveling,
    Landing,
    Channeling,
    Finished,
}
impl CurrentSpellState {
    fn can_be_interrupted(self) -> bool {
        matches!(
            self,
            Self::Created | Self::Targeting | Self::Casting | Self::Delayed | Self::Channeling
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurrentSpell {
    pub slot: CurrentSpellSlot,
    pub cast_id: u64,
    pub state: CurrentSpellState,
    /// Neither trait bypasses death interruption; they affect other cast checks.
    pub triggered: bool,
    pub allow_cast_while_casting: bool,
}

/// Called before applying the lethal damage. Source Kill is reached only when
/// health <= damage; instakill/duel/death-prevention cases are outside this API.
#[derive(Clone, Debug)]
pub struct DeathInput<'a> {
    pub tick: u64,
    pub now_seconds: u64,
    pub player_id: u64,
    pub class: u8,
    pub killer_npc_id: u64,
    pub health_before: u32,
    pub lethal_damage: u32,
    pub mana_before: u32,
    pub durability_loss: bool,
    /// None = melee/no SpellEntry. Some = actual killing SpellEntry AttributesEx3.
    pub killing_spell_attributes_ex3: Option<u32>,
    pub support: &'a DeathSupport,
    /// Source multimap order: spell ID, then stable holder insertion order.
    pub auras: &'a [DeathAura],
    /// At most one entry per slot, in Generic, AutoRepeat, Channeled order.
    pub current_spells: &'a [CurrentSpell],
    /// Already released flights, in stable ID order; never canceled by this port.
    pub released_flight_ids: &'a [u64],
}

/// Call boundaries in the pinned source's exact order. Names with no payload
/// still require their documented state mutation, not just a diagnostic event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DeathAction {
    KillProc {
        killer_npc_id: u64,
        player_id: u64,
    },
    SetHealthZero,
    KillerKilledUnit {
        killer_npc_id: u64,
        player_id: u64,
    },
    KillerPetOwnerKilledUnit,
    SetPvpDeathFalse,
    /// Invoke existing EquipmentState::apply_npc_death_loss and validated
    /// StatRecipe callback HERE, before any aura removal/death-state change.
    NpcDeathDurabilityLoss,
    SendDurabilityDamageDeath,
    ClearDrunkenness,
    ClearComboPoints,
    ClearResurrectRequest,
    RemovePet,
    RemoveGuardians,
    RemoveAuraByDeath {
        holder_id: u64,
        spell_id: u32,
    },
    BreakCharmOutgoing,
    BreakCharmIncoming,
    RemoveMiniPet,
    UnsummonTotems,
    StopMoving,
    ClearMotion,
    MoveIdle,
    StopEvade,
    ClearHealthBelow20AuraState,
    /// Zero all six reactive timers and the mage's AURA_STATE_DEFENSE flag.
    ClearAllReactives,
    ClearDiminishings,
    /// AttackStop(true,false,true), RemoveAllAttackers, DeleteThreatList
    /// (including incoming hostile references), StopCombatTimer, ClearInCombat.
    /// Does not cancel released spells or clear the player's selected target.
    CombatStop,
    ClearComboPointHolders,
    SendAutoRepeatCancel,
    InterruptCurrentSpell {
        slot: CurrentSpellSlot,
        cast_id: u64,
    },
    SetDeathState {
        phase: DeathPhase,
    },
    RootMovement,
    ClearDynamicFlags,
    SetReleaseTimerFlag {
        enabled: bool,
    },
    SetDeathTimer {
        milliseconds: u32,
    },
    SetDeathExpireTime {
        seconds: u64,
    },
    UpdateVisibility,
    /// A pending operation, not a completed ghost/teleport/resurrection.
    ReleaseUnavailable {
        automatic: bool,
    },
}

#[must_use = "stage every ordered action before committing next"]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeathPlan {
    pub next: PlayerDeathState,
    pub actions: Vec<DeathAction>,
    pub removed_aura_holder_ids: Vec<u64>,
    pub retained_aura_holder_ids: Vec<u64>,
    pub interrupted_cast_ids: Vec<u64>,
    pub retained_current_cast_ids: Vec<u64>,
    pub retained_released_flight_ids: Vec<u64>,
    /// The Kill/SetDeathState code itself does not change mana. Durability
    /// callbacks may change/clamp it, and their result takes precedence.
    pub mana_before_equipment_callback: u32,
}
/// Native first-life fixed-step derivation contract. Tick zero is genesis; each
/// later tick runs player update before NPC attacks. No offline catch-up is added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeDeathClock {
    pub clock_origin_ms: u64,
    pub tick_ms: u32,
    pub current_tick: u64,
}
impl NativeDeathClock {
    fn seconds_at(self, tick: u64) -> Result<u64, DeathError> {
        tick.checked_mul(u64::from(self.tick_ms))
            .and_then(|elapsed| self.clock_origin_ms.checked_add(elapsed))
            .map(|ms| ms / 1000)
            .ok_or(DeathError::ClockOverflow)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathUpdate {
    pub tick: u64,
    pub now_seconds: u64,
    pub diff_ms: u32,
    pub in_world: bool,
}
#[must_use = "apply ordered actions and commit next even when release is unavailable"]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeathUpdatePlan {
    pub next: PlayerDeathState,
    pub actions: Vec<DeathAction>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseEligibility {
    Alive,
    WaitingForKillPlayer,
    /// Corpse body can request release. Ghost aura, corpse creation/placement,
    /// reclaim and graveyard navigation remain unimplemented dependencies.
    EligibleButUnavailable {
        automatic_due: bool,
    },
}

impl PlayerDeathState {
    /// Stronger restore for native first-life worlds with player-update-before-
    /// NPC-melee ordering. The host must still authenticate the death tick using
    /// its event/checkpoint chain; arithmetic cannot prove historical events.
    pub fn restore_native_fixed_step(
        snapshot: PlayerDeathSnapshot,
        player_id: u64,
        config: DeathConfig,
        clock: NativeDeathClock,
    ) -> Result<Self, DeathError> {
        let state = Self::restore(snapshot, player_id, config)?;
        let snapshot = &state.0;
        if clock.tick_ms == 0 {
            return Err(DeathError::Invalid("native tick duration"));
        }
        let now = clock.seconds_at(clock.current_tick)?;
        let expected_update = (clock.current_tick > 0).then_some(clock.current_tick);
        if snapshot.last_tick != clock.current_tick
            || snapshot.last_now_seconds != now
            || snapshot.last_update_tick != expected_update
        {
            return Err(DeathError::Invalid("native snapshot frame clock"));
        }
        if snapshot.phase == DeathPhase::Alive {
            if snapshot.death_expire_time_seconds != 0 {
                return Err(DeathError::Invalid("native first-life expiration"));
            }
            return Ok(state);
        }
        if snapshot.death_expire_before_death_seconds != Some(0) {
            return Err(DeathError::Invalid("native previous death history"));
        }
        let died = snapshot
            .died_at_tick
            .ok_or(DeathError::Invalid("native death tick"))?;
        if died == 0 {
            return Err(DeathError::Invalid("native death before first update"));
        }
        if snapshot.phase == DeathPhase::JustDied {
            if died != clock.current_tick || snapshot.death_expire_time_seconds != 0 {
                return Err(DeathError::Invalid("native deferred corpse frame"));
            }
            return Ok(state);
        }
        let corpse_tick = died.checked_add(1).ok_or(DeathError::ClockOverflow)?;
        if snapshot.corpse_at_tick != Some(corpse_tick) || corpse_tick > clock.current_tick {
            return Err(DeathError::Invalid("native corpse update order"));
        }
        let expire = if config.pve_reclaim_delay_enabled {
            clock
                .seconds_at(corpse_tick)?
                .checked_add(DEATH_EXPIRE_STEP_SECONDS)
                .ok_or(DeathError::ClockOverflow)?
        } else {
            0
        };
        let frames = clock.current_tick - corpse_tick + 1;
        let elapsed = frames
            .checked_mul(u64::from(clock.tick_ms))
            .ok_or(DeathError::ClockOverflow)?;
        let timer = if config.map_instanceable {
            DEATH_TIMER_MS
        } else {
            u64::from(DEATH_TIMER_MS).saturating_sub(elapsed) as u32
        };
        if snapshot.death_expire_time_seconds != expire
            || snapshot.death_timer_ms != timer
            || snapshot.release_due != (timer == 0)
        {
            return Err(DeathError::Invalid("native derived death timers"));
        }
        Ok(state)
    }
    pub fn new(
        player_id: u64,
        config: DeathConfig,
        tick: u64,
        now_seconds: u64,
    ) -> Result<Self, DeathError> {
        Self::from_snapshot(PlayerDeathSnapshot {
            version: PLAYER_DEATH_FORMAT,
            player_id,
            config,
            phase: DeathPhase::Alive,
            last_tick: tick,
            last_now_seconds: now_seconds,
            last_update_tick: None,
            died_at_tick: None,
            corpse_at_tick: None,
            death_timer_ms: 0,
            death_expire_time_seconds: 0,
            death_expire_before_death_seconds: None,
            release_due: false,
        })
    }
    pub fn from_snapshot(snapshot: PlayerDeathSnapshot) -> Result<Self, DeathError> {
        if snapshot.version != PLAYER_DEATH_FORMAT || snapshot.player_id == 0 {
            return Err(DeathError::Invalid("snapshot version/player identity"));
        }
        if snapshot.death_timer_ms > DEATH_TIMER_MS
            || snapshot
                .last_update_tick
                .is_some_and(|v| v > snapshot.last_tick)
            || snapshot
                .died_at_tick
                .is_some_and(|v| v > snapshot.last_tick)
            || snapshot
                .corpse_at_tick
                .is_some_and(|v| v > snapshot.last_tick)
        {
            return Err(DeathError::Invalid("snapshot clock/timer"));
        }
        let valid = match snapshot.phase {
            DeathPhase::Alive => {
                snapshot.died_at_tick.is_none()
                    && snapshot.death_expire_before_death_seconds.is_none()
                    && snapshot.corpse_at_tick.is_none()
                    && snapshot.death_timer_ms == 0
                    && !snapshot.release_due
            }
            DeathPhase::JustDied => {
                snapshot.died_at_tick.is_some()
                    && snapshot.death_expire_before_death_seconds
                        == Some(snapshot.death_expire_time_seconds)
                    && snapshot.corpse_at_tick.is_none()
                    && snapshot.death_timer_ms == 0
                    && !snapshot.release_due
            }
            DeathPhase::Corpse => {
                snapshot.death_expire_before_death_seconds.is_some()
                    && snapshot
                        .died_at_tick
                        .zip(snapshot.corpse_at_tick)
                        .is_some_and(|(d, c)| d <= c)
                    && snapshot
                        .last_update_tick
                        .is_some_and(|v| Some(v) >= snapshot.corpse_at_tick)
                    && if snapshot.config.map_instanceable {
                        snapshot.death_timer_ms == DEATH_TIMER_MS && !snapshot.release_due
                    } else {
                        (snapshot.death_timer_ms == 0) == snapshot.release_due
                    }
            }
        };
        if !valid {
            return Err(DeathError::Invalid("snapshot lifecycle"));
        }
        let limit = snapshot
            .last_now_seconds
            .checked_add(3 * DEATH_EXPIRE_STEP_SECONDS)
            .ok_or(DeathError::ClockOverflow)?;
        if snapshot.death_expire_time_seconds > limit
            || snapshot
                .death_expire_before_death_seconds
                .is_some_and(|expiry| expiry > limit)
        {
            return Err(DeathError::Invalid("snapshot reclaim expiration"));
        }
        Ok(Self(snapshot))
    }
    pub fn restore(
        snapshot: PlayerDeathSnapshot,
        player_id: u64,
        config: DeathConfig,
    ) -> Result<Self, DeathError> {
        if snapshot.player_id != player_id || snapshot.config != config {
            return Err(DeathError::RestoreMismatch);
        }
        Self::from_snapshot(snapshot)
    }
    pub fn snapshot(&self) -> PlayerDeathSnapshot {
        self.0.clone()
    }
    pub fn phase(&self) -> DeathPhase {
        self.0.phase
    }
    pub fn is_alive(&self) -> bool {
        self.phase() == DeathPhase::Alive
    }
    pub fn release_eligibility(&self) -> ReleaseEligibility {
        match self.phase() {
            DeathPhase::Alive => ReleaseEligibility::Alive,
            DeathPhase::JustDied => ReleaseEligibility::WaitingForKillPlayer,
            DeathPhase::Corpse => ReleaseEligibility::EligibleButUnavailable {
                automatic_due: self.0.release_due,
            },
        }
    }
    /// Source delay applies from ghost creation, not from this body's death.
    /// Returning it does not establish that a corpse is reclaimable yet.
    pub fn corpse_reclaim_delay_seconds(&self, now_seconds: u64) -> u32 {
        let count = if self.0.config.pve_reclaim_delay_enabled {
            u64::from(self.0.death_expire_time_seconds.saturating_sub(now_seconds) as u32)
                / DEATH_EXPIRE_STEP_SECONDS
        } else {
            0
        };
        [30, 60, 120][count.min(2) as usize]
    }
    fn clock(&self, tick: u64, now_seconds: u64) -> Result<(), DeathError> {
        if tick < self.0.last_tick || now_seconds < self.0.last_now_seconds {
            Err(DeathError::ClockRegression)
        } else if now_seconds > u64::MAX - 3 * DEATH_EXPIRE_STEP_SECONDS {
            Err(DeathError::ClockOverflow)
        } else {
            Ok(())
        }
    }
    pub fn prepare_kill(&self, input: DeathInput<'_>) -> Result<DeathPlan, DeathError> {
        let _ = input.support;
        self.clock(input.tick, input.now_seconds)?;
        if !self.is_alive() {
            return Err(DeathError::AlreadyDead);
        }
        if input.player_id != self.0.player_id || input.class != 8 || input.killer_npc_id == 0 {
            return Err(DeathError::Invalid("solo mage/ordinary NPC identities"));
        }
        if input.health_before == 0 {
            return Err(DeathError::Invalid("alive health is zero"));
        }
        if input.lethal_damage < input.health_before {
            return Err(DeathError::NonLethal);
        }
        validate_members(&input)?;
        let mut next = self.clone();
        next.0.last_tick = input.tick;
        next.0.last_now_seconds = input.now_seconds;
        next.0.died_at_tick = Some(input.tick);
        next.0.death_expire_before_death_seconds = Some(self.0.death_expire_time_seconds);
        next.0.phase = DeathPhase::JustDied;
        use DeathAction::*;
        let mut actions = vec![
            KillProc {
                killer_npc_id: input.killer_npc_id,
                player_id: input.player_id,
            },
            SetHealthZero,
            KillerKilledUnit {
                killer_npc_id: input.killer_npc_id,
                player_id: input.player_id,
            },
            KillerPetOwnerKilledUnit,
            SetPvpDeathFalse,
        ];
        if input.durability_loss
            && input
                .killing_spell_attributes_ex3
                .is_none_or(|v| v & 0x20 == 0)
        {
            actions.extend([NpcDeathDurabilityLoss, SendDurabilityDamageDeath]);
        }
        actions.extend([
            ClearDrunkenness,
            ClearComboPoints,
            ClearResurrectRequest,
            RemovePet,
            RemoveGuardians,
        ]);
        let mut removed_aura_holder_ids = Vec::new();
        let mut retained_aura_holder_ids = Vec::new();
        for aura in input.auras {
            if aura.survives_death() {
                retained_aura_holder_ids.push(aura.holder_id);
            } else {
                removed_aura_holder_ids.push(aura.holder_id);
                actions.push(RemoveAuraByDeath {
                    holder_id: aura.holder_id,
                    spell_id: aura.spell_id,
                });
            }
        }
        actions.extend([
            BreakCharmOutgoing,
            BreakCharmIncoming,
            RemoveMiniPet,
            UnsummonTotems,
            StopMoving,
            ClearMotion,
            MoveIdle,
            StopEvade,
            ClearHealthBelow20AuraState,
            ClearAllReactives,
            ClearDiminishings,
            CombatStop,
            ClearComboPointHolders,
        ]);
        let mut interrupted_cast_ids = Vec::new();
        let mut retained_current_cast_ids = Vec::new();
        for cast in input.current_spells {
            let attempted = cast.slot == CurrentSpellSlot::Channeled
                || cast.state != CurrentSpellState::Traveling;
            if attempted && cast.slot == CurrentSpellSlot::AutoRepeat {
                actions.push(SendAutoRepeatCancel);
            }
            if attempted && cast.state.can_be_interrupted() {
                actions.push(InterruptCurrentSpell {
                    slot: cast.slot,
                    cast_id: cast.cast_id,
                });
                interrupted_cast_ids.push(cast.cast_id);
            } else {
                retained_current_cast_ids.push(cast.cast_id);
            }
        }
        actions.extend([
            SetDeathState {
                phase: DeathPhase::JustDied,
            },
            CombatStop,
        ]);
        Ok(DeathPlan {
            next,
            actions,
            removed_aura_holder_ids,
            retained_aura_holder_ids,
            interrupted_cast_ids,
            retained_current_cast_ids,
            retained_released_flight_ids: input.released_flight_ids.to_vec(),
            mana_before_equipment_callback: input.mana_before,
        })
    }
    /// Player::Update ordering: KillPlayer first, death-timer update later in the
    /// same frame. Surviving current-spell slots must be supplied again so the
    /// CORPSE SetDeathState has the same interruption semantics as JUST_DIED.
    pub fn prepare_update(
        &self,
        update: DeathUpdate,
        current_spells: &[CurrentSpell],
    ) -> Result<DeathUpdatePlan, DeathError> {
        if !update.in_world {
            return Ok(DeathUpdatePlan {
                next: self.clone(),
                actions: Vec::new(),
            });
        }
        self.clock(update.tick, update.now_seconds)?;
        if self
            .0
            .last_update_tick
            .is_some_and(|tick| update.tick <= tick)
        {
            return Err(DeathError::RepeatedUpdate);
        }
        validate_spells(current_spells)?;
        let mut next = self.clone();
        next.0.last_tick = update.tick;
        next.0.last_now_seconds = update.now_seconds;
        next.0.last_update_tick = Some(update.tick);
        let mut actions = Vec::new();
        use DeathAction::*;
        if self.phase() == DeathPhase::JustDied {
            actions.extend([RootMovement, CombatStop, ClearComboPointHolders]);
            for cast in current_spells {
                let attempted = cast.slot == CurrentSpellSlot::Channeled
                    || cast.state != CurrentSpellState::Traveling;
                if attempted && cast.slot == CurrentSpellSlot::AutoRepeat {
                    actions.push(SendAutoRepeatCancel);
                }
                if attempted && cast.state.can_be_interrupted() {
                    actions.push(InterruptCurrentSpell {
                        slot: cast.slot,
                        cast_id: cast.cast_id,
                    });
                }
            }
            next.0.phase = DeathPhase::Corpse;
            next.0.corpse_at_tick = Some(update.tick);
            next.0.death_timer_ms = DEATH_TIMER_MS;
            actions.extend([
                SetDeathState {
                    phase: DeathPhase::Corpse,
                },
                ClearDynamicFlags,
                SetReleaseTimerFlag {
                    enabled: !self.0.config.map_instanceable,
                },
                SetDeathTimer {
                    milliseconds: DEATH_TIMER_MS,
                },
            ]);
            if self.0.config.pve_reclaim_delay_enabled {
                let now = update.now_seconds;
                let steps = if now < next.0.death_expire_time_seconds {
                    ((next.0.death_expire_time_seconds - now) / DEATH_EXPIRE_STEP_SECONDS + 2)
                        .min(3)
                } else {
                    1
                };
                next.0.death_expire_time_seconds = now
                    .checked_add(steps * DEATH_EXPIRE_STEP_SECONDS)
                    .ok_or(DeathError::ClockOverflow)?;
                actions.push(SetDeathExpireTime {
                    seconds: next.0.death_expire_time_seconds,
                });
            }
            actions.push(UpdateVisibility);
        }
        if next.0.death_timer_ms > 0 && !next.0.config.map_instanceable {
            next.0.death_timer_ms = next.0.death_timer_ms.saturating_sub(update.diff_ms);
            actions.push(SetDeathTimer {
                milliseconds: next.0.death_timer_ms,
            });
            if next.0.death_timer_ms == 0 {
                next.0.release_due = true;
                actions.push(ReleaseUnavailable { automatic: true });
            }
        }
        Ok(DeathUpdatePlan { next, actions })
    }
}

fn validate_spells(spells: &[CurrentSpell]) -> Result<(), DeathError> {
    if spells.len() > 3
        || spells.windows(2).any(|w| w[0].slot >= w[1].slot)
        || spells.iter().any(|s| s.cast_id == 0)
        || spells
            .iter()
            .map(|s| s.cast_id)
            .collect::<BTreeSet<_>>()
            .len()
            != spells.len()
    {
        return Err(DeathError::Invalid("current spell slots/identities"));
    }
    Ok(())
}
fn validate_members(input: &DeathInput<'_>) -> Result<(), DeathError> {
    validate_spells(input.current_spells)?;
    if let Some(aura) = input.auras.iter().find(|a| a.caster_aura_state != 0) {
        return Err(DeathError::UnsupportedAuraState(aura.caster_aura_state));
    }
    if input.auras.len() > MAX_AURA_HOLDERS
        || input
            .auras
            .windows(2)
            .any(|w| w[0].spell_id > w[1].spell_id)
        || input
            .auras
            .iter()
            .any(|a| a.spell_id == 0 || a.holder_id == 0)
        || input
            .auras
            .iter()
            .map(|a| a.holder_id)
            .collect::<BTreeSet<_>>()
            .len()
            != input.auras.len()
    {
        return Err(DeathError::Invalid("aura holder metadata/order"));
    }
    if input.released_flight_ids.len() > MAX_RELEASED_FLIGHTS
        || input.released_flight_ids.contains(&0)
        || input.released_flight_ids.windows(2).any(|w| w[0] >= w[1])
    {
        return Err(DeathError::Invalid("released flight identities/order"));
    }
    Ok(())
}
